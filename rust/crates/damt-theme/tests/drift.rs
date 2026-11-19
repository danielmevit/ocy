//! Drift guard for the migration: every `*Color` (and the literal state brushes) in the C# app's
//! `App.axaml` theme dictionaries must equal Ocy's `tokens.toml`. Avalonia writes `#AARRGGBB`;
//! the tokens use `#RRGGBBAA`, so the values are converted before comparing.

use std::collections::BTreeMap;

use damt_theme::{ColorPair, Rgba, Tokens};

const TOKENS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/ocy/theme/tokens.toml"
);
const APP_AXAML: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../src/Ocy.Desktop/App.axaml"
);

/// C# resource key → tokens.toml colour name (kebab-case).
fn token_for_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "BgColor" => "bg",
        "SurfaceColor" => "surface",
        "TextColor" => "text",
        "TextMutedColor" => "text-muted",
        "AccentColor" => "accent",
        "OnAccentColor" => "on-accent",
        "ErrorColor" => "error",
        "AttentionColor" => "attention",
        "BorderColor" => "border",
        "OverlayColor" => "overlay",
        "DividerColor" => "divider",
        "SystemAccentColor" => "system-accent",
        "SystemAccentColorLight1" => "system-accent-light-1",
        "SystemAccentColorLight2" => "system-accent-light-2",
        "SystemAccentColorLight3" => "system-accent-light-3",
        "SystemAccentColorDark1" => "system-accent-dark-1",
        "SystemAccentColorDark2" => "system-accent-dark-2",
        "SystemAccentColorDark3" => "system-accent-dark-3",
        _ => return None,
    })
}

/// C# brush key → tokens.toml colour name for the brushes with a literal `Color` value.
fn token_for_brush(key: &str) -> Option<&'static str> {
    Some(match key {
        "ProgressBarTrackBrush" => "progress-track",
        "ButtonOutlineHoverBrush" => "button-outline-hover",
        "ButtonOutlinePressedBrush" => "button-outline-pressed",
        _ => return None,
    })
}

/// The XAML `x:Key` attribute (the `x` prefix is the 2006 XAML namespace).
fn x_key<'a>(node: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    node.attribute(("http://schemas.microsoft.com/winfx/2006/xaml", "Key"))
}

/// The `x:Key="Light"` / `x:Key="Dark"` resource dictionaries inside ThemeDictionaries.
fn theme_dictionaries<'a>(
    doc: &'a roxmltree::Document<'a>,
) -> Vec<(&'a str, roxmltree::Node<'a, 'a>)> {
    doc.descendants()
        .filter(|n| n.has_tag_name("ResourceDictionary.ThemeDictionaries"))
        .flat_map(|theme| {
            theme
                .children()
                .filter(|c| c.has_tag_name("ResourceDictionary"))
        })
        .filter_map(|dict| {
            let key = x_key(dict)?;
            Some((key, dict))
        })
        .collect()
}

fn tokens() -> Tokens {
    let source = std::fs::read_to_string(TOKENS).unwrap();
    Tokens::parse(&source).unwrap()
}

fn pair_for(tokens: &Tokens, theme: &str, name: &str) -> ColorPair {
    let pair = tokens
        .color(name)
        .unwrap_or_else(|| panic!("tokens.toml has no colour `{name}`"));
    match theme {
        "Light" => ColorPair {
            light: pair.light,
            dark: pair.light,
        },
        "Dark" => ColorPair {
            light: pair.dark,
            dark: pair.dark,
        },
        _ => panic!("unknown theme `{theme}`"),
    }
}

#[test]
fn every_theme_dictionary_color_matches_tokens_toml() {
    let tokens = tokens();
    let axaml = std::fs::read_to_string(APP_AXAML).unwrap();
    let doc = roxmltree::Document::parse(&axaml).unwrap();

    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (theme, dict) in theme_dictionaries(&doc) {
        for node in dict.children().filter(|c| c.has_tag_name("Color")) {
            let key = x_key(node).unwrap_or_else(|| panic!("Color without x:Key in {theme}"));
            let value = node.text().unwrap_or_default().trim();
            let argb = Rgba::from_argb_hex(value)
                .unwrap_or_else(|e| panic!("bad colour for {key} in {theme}: {e}"));
            let name = token_for_key(key)
                .unwrap_or_else(|| panic!("unmapped `*Color` key `{key}` in App.axaml; add it to the drift test and tokens.toml"));
            let expected = match theme {
                "Light" => tokens.color(name).unwrap().light,
                "Dark" => tokens.color(name).unwrap().dark,
                _ => panic!("unknown theme `{theme}`"),
            };
            assert_eq!(
                argb, expected,
                "{key} ({theme}) in App.axaml does not match tokens.toml `{name}`"
            );
            seen.insert(format!("{key}/{theme}"), name.to_string());
        }
    }
    // Completeness: every mapped colour must exist in both dictionaries.
    assert_eq!(
        seen.len(),
        2 * 18,
        "expected 18 colours x 2 themes in App.axaml"
    );
}

#[test]
fn literal_state_brushes_match_derived_tokens() {
    let tokens = tokens();
    let axaml = std::fs::read_to_string(APP_AXAML).unwrap();
    let doc = roxmltree::Document::parse(&axaml).unwrap();

    for (theme, dict) in theme_dictionaries(&doc) {
        for node in dict
            .children()
            .filter(|c| c.has_tag_name("SolidColorBrush"))
        {
            let Some(key) = x_key(node) else { continue };
            let Some(name) = token_for_brush(key) else {
                continue;
            };
            let value = node.attribute("Color").unwrap_or_default();
            let argb = Rgba::from_argb_hex(value)
                .unwrap_or_else(|e| panic!("bad colour for brush {key} in {theme}: {e}"));
            let expected = pair_for(&tokens, theme, name);
            assert_eq!(
                argb, expected.light,
                "{key} ({theme}) in App.axaml does not match tokens.toml `{name}`"
            );
        }
    }
}

#[test]
fn frame_brush_opacities_match_the_frame_tokens() {
    // AttentionFrameBrush and EdgeFrameBrush keep the full colour and set Opacity="0.55";
    // tokens.toml bakes the same 55 % into attention-frame / edge-frame (alpha 0x8C = 140).
    let tokens = tokens();
    let axaml = std::fs::read_to_string(APP_AXAML).unwrap();
    let doc = roxmltree::Document::parse(&axaml).unwrap();

    for (brush, token, base) in [
        ("AttentionFrameBrush", "attention-frame", "attention"),
        ("EdgeFrameBrush", "edge-frame", "accent"),
    ] {
        for (theme, dict) in theme_dictionaries(&doc) {
            let node = dict
                .children()
                .find(|c| c.has_tag_name("SolidColorBrush") && x_key(*c) == Some(brush))
                .unwrap_or_else(|| panic!("{brush} missing in {theme}"));
            let opacity: f64 = node.attribute("Opacity").unwrap().parse().unwrap();
            assert!(
                (opacity - 0.55).abs() < 1e-9,
                "{brush} ({theme}) opacity is {opacity}, expected 0.55"
            );
            let frame = pair_for(&tokens, theme, token).light;
            let base = pair_for(&tokens, theme, base).light;
            // 0.55 * 255 = 140.25 -> 140 (0x8C): the alpha tokens.toml bakes into the frame tokens.
            assert_eq!(frame, Rgba { a: 140, ..base });
        }
    }
}
