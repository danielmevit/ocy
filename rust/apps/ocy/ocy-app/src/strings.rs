//! Embedded string tables, BCP-47 `"auto"` resolution, CLDR cardinal plural rules,
//! and `{name}` placeholder formatting (port + i18n extension of `Localization/Strings.cs`).

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use jiff::SignedDuration;

include!(concat!(env!("OUT_DIR"), "/embedded_strings.rs"));

const PLURAL_SUFFIXES: &[&str] = &[".zero", ".one", ".two", ".few", ".many", ".other"];

/// CLDR cardinal plural category for integer quantities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    /// Key suffix without leading dot (`"zero"`, `"one"`, `"two"`, `"few"`, `"many"`, `"other"`).
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        }
    }
}

/// Returns all embedded `(bcp47_tag, raw_json)` string tables compiled into `ocy-app`.
#[must_use]
pub fn embedded_language_tables() -> &'static [(&'static str, &'static str)] {
    EMBEDDED_STRINGS
}

/// Returns the sorted list of available embedded BCP-47 language tags (e.g. `["de", "en", "mk"]`).
#[must_use]
pub fn available_languages() -> Vec<&'static str> {
    EMBEDDED_STRINGS.iter().map(|(tag, _)| *tag).collect()
}

/// Returns the endonym (a language's own name for itself, e.g. `"Español"` for `es`,
/// `"Srpski (latinica)"` for `sr-Latn`) used by the Settings language picker (S2).
/// Matching is case-insensitive and accepts `_` separators; unknown tags return the
/// trimmed tag itself.
#[must_use]
pub fn language_display_name(tag: &str) -> &str {
    let normalized = tag.trim().replace('_', "-").to_ascii_lowercase();
    match normalized.as_str() {
        "en" => "English",
        "de" => "Deutsch",
        "mk" => "Македонски",
        "ja" => "日本語",
        "es" => "Español",
        "fr" => "Français",
        "it" => "Italiano",
        "pt-br" => "Português (Brasil)",
        "pt-pt" => "Português (Portugal)",
        "nl" => "Nederlands",
        "pl" => "Polski",
        "cs" => "Čeština",
        "sk" => "Slovenčina",
        "sl" => "Slovenščina",
        "hr" => "Hrvatski",
        "bs" => "Bosanski",
        "sr-latn" => "Srpski (latinica)",
        "hu" => "Magyar",
        "ro" => "Română",
        "sv" => "Svenska",
        "da" => "Dansk",
        "nb" => "Norsk bokmål",
        "fi" => "Suomi",
        "et" => "Eesti",
        "lv" => "Latviešu",
        "lt" => "Lietuvių",
        "sq" => "Shqip",
        "tr" => "Türkçe",
        "sr" => "Српски",
        "bg" => "Български",
        "ru" => "Русский",
        "uk" => "Українська",
        "el" => "Ελληνικά",
        "zh-hans" => "简体中文",
        "zh-hant" => "繁體中文",
        "ko" => "한국어",
        "hi" => "हिन्दी",
        "th" => "ไทย",
        "vi" => "Tiếng Việt",
        "id" => "Bahasa Indonesia",
        _ => tag.trim(),
    }
}

/// Strips a trailing CLDR plural suffix (`.zero`, `.one`, `.two`, `.few`, `.many`, `.other`),
/// returning `(base_key, category)` if present.
#[must_use]
pub fn split_plural_suffix(key: &str) -> Option<(&str, PluralCategory)> {
    let cats = [
        (".zero", PluralCategory::Zero),
        (".one", PluralCategory::One),
        (".two", PluralCategory::Two),
        (".few", PluralCategory::Few),
        (".many", PluralCategory::Many),
        (".other", PluralCategory::Other),
    ];
    for (suffix, cat) in cats {
        if let Some(base) = key.strip_suffix(suffix) {
            return Some((base, cat));
        }
    }
    None
}

/// Returns whether `key` is a metadata entry ignored by the runtime (e.g. `"_meta.status"`).
#[must_use]
pub fn is_metadata_key(key: &str) -> bool {
    key.starts_with("_meta.")
}

/// Computes the CLDR cardinal plural category for integer `n` in `language`.
///
/// Supported families (source: the CLDR cardinal plural rules chart,
/// `common/supplemental/plurals.xml` in unicode-org/cldr):
/// - English / German-like (`en`, `de`, `nl`, `sv`, `da`, `no`, `nb`, `nn`, `fi`, `et`, `el`, `bg`,
///   `hu`, `tr`, `sq`)
/// - Spanish / Italian (`es`, `it`): `one` + `many` (exact millions) + `other`
/// - French / Brazilian Portuguese (`fr`, `pt`, `pt-BR`): `one` (0 and 1) + `many` + `other`
/// - European Portuguese (`pt-PT`): `one` (1 only) + `many` + `other`
/// - Armenian / Hindi / Bengali / Persian (`hy`, `hi`, `bn`, `fa`): 0 and 1 -> `one`, else `other`
/// - Macedonian (`mk`, per CLDR: `n % 10 == 1 && n % 100 != 11` -> `one`, else `other`)
/// - Romanian (`ro`): `one` / `few` / `other`
/// - Lithuanian (`lt`): `one` / `few` / `many` / `other`
/// - Latvian (`lv`): `zero` / `one` / `other`
/// - East-Slavic (`ru`, `uk`, `be`)
/// - Serbo-Croatian (`sr`, `sr-Latn`, `hr`, `bs`, `sh`)
/// - Polish (`pl`)
/// - Czech / Slovak (`cs`, `sk`)
/// - Slovenian (`sl`)
/// - Arabic (`ar`, 6 forms)
/// - Japanese / Chinese / Korean (`ja`, `zh`, `ko`, `other`-only)
/// - Any language without an explicit rule falls back to [`PluralCategory::Other`].
#[must_use]
pub fn plural_category(language: &str, n: u64) -> PluralCategory {
    let normalized = language.trim().replace('_', "-").to_ascii_lowercase();
    // CLDR 42+ `many` for es/it/fr/pt/pt-PT covers exact multiples of 1,000,000
    // (plus non-integer exponents, which this integer API never sees).
    let exact_millions = n >= 1_000_000 && n.is_multiple_of(1_000_000);

    if normalized == "pt-pt" {
        if n == 1 {
            return PluralCategory::One;
        }
        if exact_millions {
            return PluralCategory::Many;
        }
        return PluralCategory::Other;
    }

    let primary = normalized.split('-').next().unwrap_or("");
    match primary {
        // English / German-like (1 -> one, else other)
        "en" | "de" | "nl" | "sv" | "da" | "no" | "nb" | "nn" | "fi" | "et" | "el" | "bg"
        | "hu" | "tr" | "sq" => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Spanish / Italian (1 -> one, exact millions -> many, else other)
        "es" | "it" => {
            if n == 1 {
                PluralCategory::One
            } else if exact_millions {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // French / Brazilian Portuguese (0 and 1 -> one, exact millions -> many, else other)
        "fr" | "pt" => {
            if n <= 1 {
                PluralCategory::One
            } else if exact_millions {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Armenian / Hindi / Bengali / Persian (0 and 1 -> one, else other)
        "hy" | "hi" | "bn" | "fa" => {
            if n <= 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Romanian (1 -> one; exact hundreds not thousands -> few; else other)
        "ro" => {
            if n == 1 {
                PluralCategory::One
            } else if n != 0 && n.is_multiple_of(100) && !n.is_multiple_of(1000) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Lithuanian (one / few / many / other)
        "lt" => {
            let mod10 = n % 10;
            let mod100 = n % 100;
            let teens = (11..=19).contains(&mod100);
            if mod10 == 1 && !teens {
                PluralCategory::One
            } else if (2..=9).contains(&mod10) && !teens {
                PluralCategory::Few
            } else if mod10 == 0 || teens {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Latvian (zero / one / other)
        "lv" => {
            let mod10 = n % 10;
            let mod100 = n % 100;
            if mod10 == 0 || (11..=19).contains(&mod100) {
                PluralCategory::Zero
            } else if mod10 == 1 && mod100 != 11 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Macedonian (CLDR: i % 10 = 1 and i % 100 != 11 -> one, else other)
        "mk" => {
            if n % 10 == 1 && n % 100 != 11 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // East-Slavic (one / few / many / other)
        "ru" | "uk" | "be" => {
            let mod10 = n % 10;
            let mod100 = n % 100;
            if mod10 == 1 && mod100 != 11 {
                PluralCategory::One
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                PluralCategory::Few
            } else if mod10 == 0 || (5..=9).contains(&mod10) || (11..=14).contains(&mod100) {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Serbo-Croatian (one / few / other)
        "sr" | "hr" | "bs" | "sh" => {
            let mod10 = n % 10;
            let mod100 = n % 100;
            if mod10 == 1 && mod100 != 11 {
                PluralCategory::One
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Polish (one / few / many / other)
        "pl" => {
            let mod10 = n % 10;
            let mod100 = n % 100;
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                PluralCategory::Few
            } else if mod10 <= 1 || (5..=9).contains(&mod10) || (12..=14).contains(&mod100) {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Czech / Slovak (one / few / other for integers)
        "cs" | "sk" => {
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&n) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Slovenian (one / two / few / other)
        "sl" => match n % 100 {
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            3 | 4 => PluralCategory::Few,
            _ => PluralCategory::Other,
        },
        // Arabic (zero / one / two / few / many / other)
        "ar" => {
            if n == 0 {
                PluralCategory::Zero
            } else if n == 1 {
                PluralCategory::One
            } else if n == 2 {
                PluralCategory::Two
            } else {
                match n % 100 {
                    3..=10 => PluralCategory::Few,
                    11..=99 => PluralCategory::Many,
                    _ => PluralCategory::Other,
                }
            }
        }
        // Japanese / Chinese / Korean / Thai / Vietnamese / Indonesian (other only) and any
        // unmapped language
        "ja" | "zh" | "ko" | "th" | "vi" | "id" => PluralCategory::Other,
        _ => PluralCategory::Other,
    }
}

/// Queries the current OS UI language as a BCP-47 tag (Windows `GetUserDefaultUILanguage` ->
/// `LCIDToLocaleName`; `None` on non-Windows).
#[must_use]
pub fn os_ui_language() -> Option<String> {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{
            GetUserDefaultUILanguage, LCIDToLocaleName, LOCALE_ALLOW_NEUTRAL_NAMES,
        };

        // LOCALE_NAME_MAX_LENGTH is 85 UTF-16 code units including the terminating NUL.
        let mut buf = [0u16; 85];
        // SAFETY: `GetUserDefaultUILanguage` has no preconditions, and `LCIDToLocaleName` writes
        // at most `buf.len()` UTF-16 code units into `buf` when `Some(&mut buf)` is passed.
        let len = unsafe {
            let lang_id = GetUserDefaultUILanguage();
            LCIDToLocaleName(
                u32::from(lang_id),
                Some(&mut buf),
                LOCALE_ALLOW_NEUTRAL_NAMES,
            )
        };
        if len > 1 {
            let count = (len as usize).saturating_sub(1).min(buf.len());
            let decoded = String::from_utf16_lossy(&buf[..count]);
            let trimmed = decoded.trim_end_matches('\0').trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Returns the Windows 10/11 built-in UI font family covering all glyphs for `language`,
/// defaulting to `"Segoe UI"` for Latin, Greek, Cyrillic, and unknown/fallback tags.
#[must_use]
pub fn font_family_for(language: &str) -> &'static str {
    let mut parts = language.trim().split(['-', '_']).filter(|s| !s.is_empty());
    let Some(primary) = parts.next() else {
        return "Segoe UI";
    };
    if primary.eq_ignore_ascii_case("ja") {
        "Yu Gothic UI"
    } else if primary.eq_ignore_ascii_case("zh") {
        let mut has_hant = false;
        let mut has_hans = false;
        let mut has_trad_region = false;
        for sub in parts {
            if sub.eq_ignore_ascii_case("hant") {
                has_hant = true;
            } else if sub.eq_ignore_ascii_case("hans") {
                has_hans = true;
            } else if sub.eq_ignore_ascii_case("tw")
                || sub.eq_ignore_ascii_case("hk")
                || sub.eq_ignore_ascii_case("mo")
            {
                has_trad_region = true;
            }
        }
        if has_hant || (!has_hans && has_trad_region) {
            "Microsoft JhengHei UI"
        } else {
            "Microsoft YaHei UI"
        }
    } else if primary.eq_ignore_ascii_case("ko") {
        "Malgun Gothic"
    } else if primary.eq_ignore_ascii_case("hi")
        || primary.eq_ignore_ascii_case("mr")
        || primary.eq_ignore_ascii_case("ne")
    {
        "Nirmala UI"
    } else if primary.eq_ignore_ascii_case("th") {
        "Leelawadee UI"
    } else {
        "Segoe UI"
    }
}

/// Returns `true` if `language` uses an RTL script not yet supported (`ar`, `he`, `fa`).
#[must_use]
pub fn is_rtl_language(language: &str) -> bool {
    let primary = language
        .trim()
        .split(['-', '_'])
        .find(|s| !s.is_empty())
        .unwrap_or("");
    primary.eq_ignore_ascii_case("ar")
        || primary.eq_ignore_ascii_case("he")
        || primary.eq_ignore_ascii_case("fa")
}

/// Returns the requested RTL tag when `(setting, os_ui_language)` asks for an unsupported
/// RTL language (`ar`, `he`, `fa`) and therefore resolves to English.
#[must_use]
pub fn rtl_fallback_request<'a>(
    setting: &'a str,
    os_ui_language: Option<&'a str>,
) -> Option<&'a str> {
    let trimmed = setting.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        os_ui_language
            .map(str::trim)
            .filter(|os| is_rtl_language(os))
    } else if is_rtl_language(trimmed) {
        Some(trimmed)
    } else {
        None
    }
}

/// Parses the hidden `--demo-language <tag>` / `--demo-language=<tag>` CLI flag from `args`
/// (excluding `argv[0]`). Returns `None` when absent or empty.
#[must_use]
pub fn parse_demo_language<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut lang = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if arg == "--demo-break"
            || arg == "--demo-in"
            || arg == "--soak"
            || arg == "--instance-scope"
        {
            let _ = iter.next();
        } else if arg == "--demo-language" {
            if let Some(next) = iter.next() {
                let trimmed = next.as_ref().trim();
                if !trimmed.is_empty() && !trimmed.starts_with("--") {
                    lang = Some(trimmed.to_owned());
                }
            }
        } else if let Some(rest) = arg.strip_prefix("--demo-language=") {
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                lang = Some(trimmed.to_owned());
            }
        }
    }
    lang
}

/// Iterates over every non-control, non-placeholder character in a localized template string.
/// Placeholders of the form `{name}` are skipped; control characters are exempt; punctuation such
/// as `·` and `—` is kept.
pub fn for_each_checkable_char(value: &str, mut visit: impl FnMut(char)) {
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        for ch in rest[..open].chars() {
            if !ch.is_control() {
                visit(ch);
            }
        }
        let after_open = &rest[open + 1..];
        if let Some(close) = after_open.find('}') {
            let inner = &after_open[..close];
            if !inner.is_empty() && !inner.contains('{') {
                rest = &after_open[close + 1..];
                continue;
            }
        }
        visit('{');
        rest = after_open;
    }
    for ch in rest.chars() {
        if !ch.is_control() {
            visit(ch);
        }
    }
}

/// Missing glyph report produced by [`check_table_coverage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingGlyph {
    pub language: String,
    pub key: String,
    pub ch: char,
    pub font: String,
}

impl fmt::Display for MissingGlyph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "language={} key={} char={:?} (U+{:04X}) font={}",
            self.language, self.key, self.ch, self.ch as u32, self.font
        )
    }
}

/// Checks every non-metadata entry in `entries` against `has_char`, returning any missing glyphs.
pub fn check_table_coverage<'a>(
    language: &str,
    font: &str,
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    mut has_char: impl FnMut(char) -> bool,
) -> Vec<MissingGlyph> {
    let mut missing = Vec::new();
    for (key, value) in entries {
        if is_metadata_key(key) {
            continue;
        }
        for_each_checkable_char(value, |ch| {
            if !has_char(ch) {
                missing.push(MissingGlyph {
                    language: language.to_owned(),
                    key: key.to_owned(),
                    ch,
                    font: font.to_owned(),
                });
            }
        });
    }
    missing
}

fn match_available_tag<'a>(tag: &str, available: &[&'a str]) -> Option<&'a str> {
    let normalized = tag.trim().replace('_', "-");
    if normalized.is_empty() {
        return None;
    }

    if let Some(&exact) = available
        .iter()
        .find(|&&cand| cand.eq_ignore_ascii_case(&normalized))
    {
        return Some(exact);
    }

    let primary = normalized.split('-').next().unwrap_or("");
    if !primary.is_empty()
        && let Some(&by_lang) = available
            .iter()
            .find(|&&cand| cand.eq_ignore_ascii_case(primary))
    {
        return Some(by_lang);
    }

    None
}

/// Resolves a `settings.language` value (`"auto"`, `"en"`, `"de"`, `"pt-BR"`, ...) against
/// `available` language tags using `os_ui_language` when `setting` is `"auto"` (or blank).
/// Matches exact tag first (case-insensitive), then primary language subtag, else `"en"`.
/// Unsupported RTL languages (`ar`, `he`, `fa`) resolve to `"en"`.
#[must_use]
pub fn resolve_language_tag<'a>(
    setting: &str,
    os_ui_language: Option<&str>,
    available: &[&'a str],
) -> &'a str {
    let trimmed = setting.trim();
    let fallback = available
        .iter()
        .copied()
        .find(|&tag| tag.eq_ignore_ascii_case("en"))
        .unwrap_or("en");

    if rtl_fallback_request(setting, os_ui_language).is_some() {
        return fallback;
    }

    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        os_ui_language
            .and_then(|os| match_available_tag(os, available))
            .unwrap_or(fallback)
    } else {
        match_available_tag(trimmed, available).unwrap_or(fallback)
    }
}

fn parse_table(raw_json: &str) -> Result<HashMap<String, String>, serde_json::Error> {
    let mut map: HashMap<String, String> = serde_json::from_str(raw_json)?;
    map.retain(|k, _| !is_metadata_key(k));
    Ok(map)
}

fn embedded_raw_for(tag: &str) -> Option<&'static str> {
    EMBEDDED_STRINGS
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(tag))
        .map(|(_, json)| *json)
}

/// Localized string lookup, CLDR plural selection, and placeholder formatting backed by
/// embedded `content/strings/*.json` tables with English fallback.
#[derive(Debug, Clone)]
pub struct Strings {
    language: String,
    rtl_fallback_from: Option<String>,
    strings: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

impl Default for Strings {
    fn default() -> Self {
        Self::from_embedded().unwrap_or_else(|_| Self {
            language: "en".to_owned(),
            rtl_fallback_from: None,
            strings: HashMap::new(),
            fallback: HashMap::new(),
        })
    }
}

impl Strings {
    /// Parses the embedded `content/strings/en.json` resource.
    pub fn from_embedded() -> Result<Self, serde_json::Error> {
        let raw = embedded_raw_for("en").unwrap_or("{}");
        let strings = parse_table(raw)?;
        Ok(Self {
            language: "en".to_owned(),
            rtl_fallback_from: None,
            strings,
            fallback: HashMap::new(),
        })
    }

    /// Loads the embedded string table for `setting` (`"auto"` resolves via [`os_ui_language`]),
    /// always keeping English as the fallback for any missing key or missing file.
    #[must_use]
    pub fn from_language(setting: &str) -> Self {
        let os_lang = if setting.trim().is_empty() || setting.trim().eq_ignore_ascii_case("auto") {
            os_ui_language()
        } else {
            None
        };
        Self::from_language_with_os(setting, os_lang.as_deref())
    }

    /// Deterministic version of [`Self::from_language`] with an explicit `os_ui_language`
    /// (for testing `"auto"` resolution without touching the OS).
    #[must_use]
    pub fn from_language_with_os(setting: &str, os_ui_language: Option<&str>) -> Self {
        let rtl_fallback_from = rtl_fallback_request(setting, os_ui_language).map(str::to_owned);
        if let Some(ref rtl) = rtl_fallback_from {
            eprintln!("Ocy i18n: RTL language {rtl:?} is not supported yet; falling back to 'en'");
        }

        let available = available_languages();
        let resolved = resolve_language_tag(setting, os_ui_language, &available);
        let en_map = embedded_raw_for("en")
            .and_then(|raw| parse_table(raw).ok())
            .unwrap_or_default();

        if resolved.eq_ignore_ascii_case("en") {
            return Self {
                language: "en".to_owned(),
                rtl_fallback_from,
                strings: en_map,
                fallback: HashMap::new(),
            };
        }

        let active_map = embedded_raw_for(resolved)
            .and_then(|raw| parse_table(raw).ok())
            .unwrap_or_default();

        Self {
            language: resolved.to_owned(),
            rtl_fallback_from,
            strings: active_map,
            fallback: en_map,
        }
    }

    /// Constructs a `Strings` instance from an explicit key-value map (English, no fallback).
    #[must_use]
    pub fn from_map(strings: HashMap<String, String>) -> Self {
        Self {
            language: "en".to_owned(),
            rtl_fallback_from: None,
            strings,
            fallback: HashMap::new(),
        }
    }

    /// Constructs a `Strings` instance for `language` with an explicit active map and English
    /// `fallback` map (primarily for unit tests).
    #[must_use]
    pub fn with_fallback(
        language: impl Into<String>,
        strings: HashMap<String, String>,
        fallback: HashMap<String, String>,
    ) -> Self {
        Self {
            language: language.into(),
            rtl_fallback_from: None,
            strings,
            fallback,
        }
    }

    /// Resolved BCP-47 language tag active in this `Strings` instance (e.g. `"en"`, `"de"`, `"mk"`).
    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Returns the requested RTL language tag if this instance fell back to English because RTL
    /// scripts (`ar`, `he`, `fa`) are not supported yet.
    #[must_use]
    pub fn rtl_fallback_from(&self) -> Option<&str> {
        self.rtl_fallback_from.as_deref()
    }

    /// Number of entries in the active string table (excluding `_meta.*` metadata keys).
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.strings.len()
    }

    /// Looks up `key` in the active language, falling back to English, then returning `key` itself.
    #[must_use]
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(val) = self.strings.get(key) {
            return val.as_str();
        }
        let other_key = format!("{key}.other");
        if let Some(val) = self.strings.get(&other_key) {
            return val.as_str();
        }
        if let Some(val) = self.fallback.get(key) {
            return val.as_str();
        }
        if let Some(val) = self.fallback.get(&other_key) {
            return val.as_str();
        }
        key
    }

    /// Picks the CLDR cardinal plural form for `(self.language(), n)` under `key`
    /// (`<key>.<cat>` -> `<key>.other` -> `<key>` in active language, then English fallback,
    /// then `key` itself).
    #[must_use]
    pub fn plural<'a>(&'a self, key: &'a str, n: u64) -> &'a str {
        let cat = plural_category(&self.language, n);
        if let Some(val) = lookup_plural_in_map(&self.strings, key, cat) {
            return val;
        }
        let en_cat = plural_category("en", n);
        if let Some(val) = lookup_plural_in_map(&self.fallback, key, en_cat) {
            return val;
        }
        key
    }

    /// Replaces `{name}` placeholders in `template`.
    #[must_use]
    pub fn format_template(template: &str, args: &[(&str, &dyn fmt::Display)]) -> String {
        if args.is_empty() {
            return template.to_owned();
        }

        let mut result = template.to_owned();
        for (name, value) in args {
            let placeholder = format!("{{{name}}}");
            let replacement = value.to_string();
            result = result.replace(&placeholder, &replacement);
        }
        result
    }

    /// Replaces `{name}` placeholders in the template looked up by `key`.
    #[must_use]
    pub fn format(&self, key: &str, args: &[(&str, &dyn fmt::Display)]) -> String {
        Self::format_template(self.get(key), args)
    }

    /// Looks up the pluralized template for `(key, n)` and replaces `{name}` placeholders.
    #[must_use]
    pub fn format_plural(&self, key: &str, n: u64, args: &[(&str, &dyn fmt::Display)]) -> String {
        Self::format_template(self.plural(key, n), args)
    }

    /// Formats a break countdown as `m:ss`.
    #[must_use]
    pub fn format_countdown(duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{minutes}:{seconds:02}")
    }

    /// Formats a break countdown using the localized `"time.countdown"` pattern.
    #[must_use]
    pub fn format_countdown_localized(&self, duration: Duration) -> String {
        let total_seconds = duration.as_secs();
        let minutes = total_seconds / 60;
        let seconds = format!("{:02}", total_seconds % 60);
        self.format_plural(
            "time.countdown",
            total_seconds,
            &[("minutes", &minutes), ("seconds", &seconds)],
        )
    }

    /// Formats `duration` using the localized time patterns and pluralized units.
    #[must_use]
    pub fn format_time(&self, duration: Duration) -> String {
        let signed = SignedDuration::try_from(duration).unwrap_or(SignedDuration::MAX);
        self.format_time_signed(signed)
    }

    /// Formats a signed `duration` using the localized time patterns and pluralized units.
    #[must_use]
    pub fn format_time_signed(&self, duration: SignedDuration) -> String {
        if duration < SignedDuration::from_mins(1) {
            let seconds = u64::try_from(duration.as_secs().max(0)).unwrap_or(0);
            let seconds_padded = format!("{seconds:02}");
            return self.format_plural(
                "time.format.seconds",
                seconds,
                &[("seconds", &seconds_padded)],
            );
        }

        let total_seconds = u64::try_from(duration.as_secs().max(0)).unwrap_or(0);
        if duration >= SignedDuration::from_hours(1) {
            let hours = total_seconds / 3_600;
            let minutes = (total_seconds % 3_600) / 60;
            let hours_unit = self.plural("time.hours", hours);
            if minutes == 0 {
                // Exact hours read naturally: "1 h", "2 h" — no "00 min" filler.
                return self.format_plural(
                    "time.format.hours",
                    hours,
                    &[("hours", &hours), ("hoursUnit", &hours_unit)],
                );
            }
            let minutes_unit = self.plural("time.minutes", minutes);
            let minutes_padded = format!("{minutes:02}");
            return self.format_plural(
                "time.format.hoursMinutes",
                hours,
                &[
                    ("hours", &hours),
                    ("hoursUnit", &hours_unit),
                    ("minutes", &minutes_padded),
                    ("minutesUnit", &minutes_unit),
                ],
            );
        }

        let total_minutes = total_seconds / 60;
        let unit = self.plural("time.minutes", total_minutes);
        self.format_plural(
            "time.format.minutes",
            total_minutes,
            &[("minutes", &total_minutes), ("unit", &unit)],
        )
    }

    /// Formats `duration` with default English `"h"` and `"min"` units.
    #[must_use]
    pub fn time_text(duration: Duration) -> String {
        let signed = SignedDuration::try_from(duration).unwrap_or(SignedDuration::MAX);
        Self::time_text_signed(signed)
    }

    /// Formats a signed `duration` with default English `"h"` and `"min"` units.
    #[must_use]
    pub fn time_text_signed(duration: SignedDuration) -> String {
        Self::format_time_with_units(duration, "h", "min")
    }

    /// Formats `duration` with explicit `hours_unit` and `minutes_unit` labels.
    #[must_use]
    pub fn format_time_with_units(
        duration: SignedDuration,
        hours_unit: &str,
        minutes_unit: &str,
    ) -> String {
        if duration < SignedDuration::from_mins(1) {
            let seconds = duration.as_secs().max(0);
            return format!("0:{seconds:02}");
        }

        if duration >= SignedDuration::from_hours(1) {
            let total_seconds = duration.as_secs();
            let hours = total_seconds / 3_600;
            let minutes = (total_seconds % 3_600) / 60;
            return format!("{hours} {hours_unit} {minutes:02} {minutes_unit}");
        }

        let total_minutes = duration.as_secs() / 60;
        format!("{total_minutes} {minutes_unit}")
    }
}

fn lookup_plural_in_map<'a>(
    map: &'a HashMap<String, String>,
    key: &str,
    cat: PluralCategory,
) -> Option<&'a str> {
    let cat_key = format!("{key}.{}", cat.suffix());
    if let Some(val) = map.get(&cat_key) {
        return Some(val.as_str());
    }
    if cat != PluralCategory::Other {
        let other_key = format!("{key}.other");
        if let Some(val) = map.get(&other_key) {
            return Some(val.as_str());
        }
    }
    if let Some(val) = map.get(key) {
        return Some(val.as_str());
    }
    None
}

/// Returns whether `key` is a known plural-suffix variant of any key in `en_keys`.
#[must_use]
pub fn is_valid_translation_key(key: &str, en_keys: &HashMap<String, String>) -> bool {
    if en_keys.contains_key(key) {
        return true;
    }
    for suffix in PLURAL_SUFFIXES {
        if let Some(base) = key.strip_suffix(suffix)
            && (en_keys.contains_key(base)
                || en_keys.contains_key(&format!("{base}.other"))
                || en_keys.contains_key(&format!("{base}.one")))
        {
            return true;
        }
    }
    false
}
