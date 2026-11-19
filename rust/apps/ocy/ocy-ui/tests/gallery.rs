//! Independent pixel checks on the headless gallery renders (no golden images).
mod support;

use std::collections::HashSet;
use support::{Rgba, Shot};

const LIGHT_CLAY: [u8; 3] = [0xB0, 0x4E, 0x2C];
const DARK_CLAY: [u8; 3] = [0xE0, 0x8A, 0x64];
const LIGHT_ACCENT: [u8; 3] = [0x2F, 0x7D, 0x62];
const DARK_ACCENT: [u8; 3] = [0x5F, 0xBF, 0x97];
const DARK_SURFACE: [u8; 3] = [0x21, 0x24, 0x26];
/// Card margin in physical pixels (16 logical px at 2x).
const MARGIN: u32 = 32;

fn near(p: [u8; 4], c: [u8; 3], tol: i32) -> bool {
    p[3] == 255 && (0..3).all(|i| (p[i] as i32 - c[i] as i32).abs() <= tol)
}

fn count_near(img: &Rgba, c: [u8; 3], tol: i32) -> usize {
    img.pixels.iter().filter(|p| near(**p, c, tol)).count()
}

fn check_not_blank(img: &Rgba) -> Result<(), String> {
    let opaque = img.pixels.iter().filter(|p| p[3] > 0).count();
    if opaque * 10 <= img.pixels.len() {
        return Err(format!(
            "only {opaque} of {} pixels are opaque",
            img.pixels.len()
        ));
    }
    let colours: HashSet<[u8; 4]> = img.pixels.iter().copied().collect();
    if colours.len() <= 3 {
        return Err(format!("only {} distinct colours", colours.len()));
    }
    Ok(())
}

/// The outermost 2 px band is fully transparent, the rest of the 16 px margin holds at most the faint
/// drop shadow (alpha < 64, so the card is not clipped into it), and the card body is opaque.
fn check_card_margin(img: &Rgba) -> Result<(), String> {
    let band = 2;
    for y in 0..img.height {
        for x in 0..img.width {
            let p = img.at(x, y);
            let outer = x < band || y < band || x >= img.width - band || y >= img.height - band;
            let margin =
                x < MARGIN || y < MARGIN || x >= img.width - MARGIN || y >= img.height - MARGIN;
            if outer && p[3] != 0 {
                return Err(format!("edge pixel ({x},{y}) is not transparent: {p:?}"));
            }
            // Leave 2 px for the anti-aliased card edge.
            let deep = x < MARGIN - 2
                || y < MARGIN - 2
                || x >= img.width - MARGIN + 2
                || y >= img.height - MARGIN + 2;
            if margin && deep && p[3] >= 64 {
                return Err(format!("margin pixel ({x},{y}) is too opaque: {p:?}"));
            }
        }
    }
    let probes = [
        (MARGIN + 12, img.height / 2),
        (img.width - MARGIN - 12, img.height / 2),
        (img.width / 2, MARGIN + 3),
        (img.width / 2, img.height - MARGIN - 3),
    ];
    for (x, y) in probes {
        if img.at(x, y)[3] != 255 {
            return Err(format!(
                "card body pixel ({x},{y}) is not opaque: {:?}",
                img.at(x, y)
            ));
        }
    }
    Ok(())
}

fn find<'a>(shots: &'a [Shot], theme: &str, name: &str) -> &'a Rgba {
    match shots.iter().find(|s| s.theme == theme && s.name == name) {
        Some(shot) => &shot.image,
        None => panic!("missing shot {theme}/{name}"),
    }
}

fn blank(w: u32, h: u32, px: [u8; 4]) -> Rgba {
    Rgba {
        width: w,
        height: h,
        pixels: vec![px; (w * h) as usize],
    }
}

#[test]
fn checks_reject_bad_images() {
    assert!(check_not_blank(&blank(64, 64, [0, 0, 0, 0])).is_err());
    assert!(check_not_blank(&blank(64, 64, [255, 255, 255, 255])).is_err());
    // A card filling the whole image has no transparent margin.
    assert!(check_card_margin(&blank(200, 120, [255, 255, 255, 255])).is_err());
    // A fully transparent image has no card body.
    assert!(check_card_margin(&blank(200, 120, [0, 0, 0, 0])).is_err());
    assert_eq!(
        count_near(&blank(4, 4, [0xB0, 0x4E, 0x2C, 255]), LIGHT_CLAY, 6),
        16
    );
}

#[test]
fn gallery_renders_pass_pixel_checks() {
    let shots = match support::render_all() {
        Ok(shots) => shots,
        Err(e) => panic!("render failed: {e}"),
    };
    assert_eq!(shots.len(), 12);

    // PNG round-trip, then every image is non-blank.
    let dir = std::env::temp_dir().join(format!("ocy-ui-test-{}", std::process::id()));
    for shot in &shots {
        let path = dir.join(shot.theme).join(format!("{}.png", shot.name));
        support::write_png(&path, &shot.image).unwrap_or_else(|e| panic!("{e}"));
        let back = support::read_png(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            back.pixels, shot.image.pixels,
            "{}/{} png round-trip",
            shot.theme, shot.name
        );
        check_not_blank(&shot.image)
            .unwrap_or_else(|e| panic!("{}/{}: {e}", shot.theme, shot.name));
    }
    let _ = std::fs::remove_dir_all(&dir);

    for theme in ["light", "dark"] {
        for name in [
            "short-break",
            "short-break-palming",
            "warning",
            "warning-strict",
        ] {
            check_card_margin(find(&shots, theme, name))
                .unwrap_or_else(|e| panic!("{theme}/{name}: {e}"));
        }
    }

    // Warning: opaque button pixels in the lower third of the card (accent fill).
    for (theme, accent) in [("light", LIGHT_ACCENT), ("dark", DARK_ACCENT)] {
        let img = find(&shots, theme, "warning");
        let top = img.height * 2 / 3 - MARGIN / 2;
        let lower: usize = (top..img.height)
            .flat_map(|y| (0..img.width).map(move |x| (x, y)))
            .filter(|&(x, y)| near(img.at(x, y), accent, 4))
            .count();
        assert!(
            lower > 500,
            "{theme} warning lower third has {lower} accent button pixels"
        );
    }

    // Light and dark differ; dark card interior matches Surface #212426.
    for name in [
        "short-break",
        "short-break-palming",
        "warning",
        "warning-strict",
        "long-break",
        "settings",
    ] {
        let (l, d) = (find(&shots, "light", name), find(&shots, "dark", name));
        assert_ne!(l.pixels, d.pixels, "{name}: light and dark are identical");
    }
    for name in ["short-break", "warning"] {
        let img = find(&shots, "dark", name);
        // Just inside the card's left edge, vertically centred: card padding, no content.
        let p = img.at(MARGIN + 8, img.height / 2);
        assert!(
            near(p, DARK_SURFACE, 2),
            "dark {name} interior is {p:?}, want #212426"
        );
    }
    let lb = find(&shots, "dark", "long-break");
    let p = lb.at(lb.width / 2, lb.height / 2 - 140);
    assert!(
        near(p, DARK_SURFACE, 2) || p[3] == 255,
        "dark long-break centre {p:?}"
    );

    // Short break: the close x is drawn in the card's top-right corner (non-surface pixels there),
    // and every card has its 1 px Border token edge (darker than the surface, opaque) in light.
    for (theme, surface) in [("light", [0xFF, 0xFF, 0xFF]), ("dark", DARK_SURFACE)] {
        for name in ["short-break", "short-break-palming"] {
            let img = find(&shots, theme, name);
            // 28x28 logical button inside the 14x12 px card padding, at 2x.
            let (x0, x1) = (img.width - MARGIN - 28 - 56, img.width - MARGIN - 28);
            let (y0, y1) = (MARGIN + 24, MARGIN + 24 + 56);
            let ink = (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| !near(img.at(x, y), surface, 12))
                .count();
            assert!(
                ink > 20,
                "{theme} {name}: close x missing ({ink} ink pixels)"
            );
        }
    }
    for name in ["short-break", "warning"] {
        let img = find(&shots, "light", name);
        let edge = img.at(img.width / 2, MARGIN);
        assert!(
            edge[3] == 255 && edge[0] < 235,
            "light {name} card border missing: {edge:?}"
        );
    }

    // Clay is in the warning and not in the short break.
    for (theme, clay) in [("light", LIGHT_CLAY), ("dark", DARK_CLAY)] {
        let warn = count_near(find(&shots, theme, "warning"), clay, 6);
        assert!(warn > 200, "{theme} warning has only {warn} clay pixels");
        let strict = count_near(find(&shots, theme, "warning-strict"), clay, 6);
        assert!(
            strict > warn,
            "{theme} strict warning should add the clay strict line"
        );
        for name in ["short-break", "short-break-palming"] {
            let n = count_near(find(&shots, theme, name), clay, 6);
            assert!(n < 5, "{theme} {name} has {n} clay pixels");
        }
    }

    // Settings: Accent present (toggles), clay absent.
    for (theme, accent, clay) in [
        ("light", LIGHT_ACCENT, LIGHT_CLAY),
        ("dark", DARK_ACCENT, DARK_CLAY),
    ] {
        let img = find(&shots, theme, "settings");
        let a = count_near(img, accent, 3);
        assert!(a > 500, "{theme} settings has only {a} accent pixels");
        let c = count_near(img, clay, 6);
        assert!(c < 5, "{theme} settings has {c} clay pixels");
    }
}
