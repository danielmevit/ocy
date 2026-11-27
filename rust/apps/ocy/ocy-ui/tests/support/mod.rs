//! Headless rendering of every Ocy surface with Slint's software renderer on a minimal custom platform.
//! Shared by `examples/gallery.rs` and the gallery tests.
#![allow(dead_code)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ocy_ui::{LongBreakCard, ShortBreakCard, Theme, WarningCard};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, PhysicalSize};

/// Render scale (the C# reference screenshots are 2x).
pub const SCALE: f32 = 2.0;

thread_local! {
    static LAST_WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct HeadlessPlatform;

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        LAST_WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
        Ok(window)
    }
}

/// Installs the headless platform (once per process; later calls are no-ops).
pub fn init_platform() {
    // Err means a platform is already set, which is what we want.
    let _ = slint::platform::set_platform(Box::new(HeadlessPlatform));
}

fn take_window() -> Result<Rc<MinimalSoftwareWindow>, String> {
    LAST_WINDOW
        .with(|slot| slot.borrow_mut().take())
        .ok_or_else(|| "no window adapter was created".to_owned())
}

/// A straight-alpha RGBA8 image.
#[derive(Debug, Clone)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Rgba {
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y * self.width + x) as usize]
    }
}

/// Renders `window` at `logical_w` x `height()`. `height` is re-read after a first paint, because
/// conditional elements (`if`) are instantiated lazily and change a card's preferred height.
fn render(window: &MinimalSoftwareWindow, logical_w: f32, height: impl Fn() -> f32) -> Rgba {
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: SCALE,
    });
    let width = (logical_w * SCALE).ceil() as u32;
    let mut size = (0, 0);
    let mut buffer = Vec::new();
    for _ in 0..3 {
        let h = (height() * SCALE).ceil() as u32;
        if (width, h) != size {
            size = (width, h);
            window.set_size(PhysicalSize::new(width, h));
            buffer = vec![PremultipliedRgbaColor::default(); (width * h) as usize];
        }
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            buffer.fill(PremultipliedRgbaColor::default());
            renderer.render(&mut buffer, width as usize);
        });
    }
    let height = size.1;
    let pixels = buffer
        .iter()
        .map(|p| {
            if p.alpha == 0 {
                [0, 0, 0, 0]
            } else {
                let un =
                    |c: u8| ((c as u32 * 255 + p.alpha as u32 / 2) / p.alpha as u32).min(255) as u8;
                [un(p.red), un(p.green), un(p.blue), p.alpha]
            }
        })
        .collect();
    Rgba {
        width,
        height,
        pixels,
    }
}

/// Writes `image` as a PNG.
pub fn write_png(path: &Path, image: &Rgba) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let flat: Vec<u8> = image.pixels.iter().flatten().copied().collect();
    writer.write_image_data(&flat).map_err(|e| e.to_string())
}

/// Reads a PNG written by [`write_png`].
pub fn read_png(path: &Path) -> Result<Rgba, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("unexpected png format".to_owned());
    }
    let pixels = buf[..info.buffer_size()]
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    Ok(Rgba {
        width: info.width,
        height: info.height,
        pixels,
    })
}

// The exercise art the gallery shows, embedded so the gallery runs from any directory.
const LOOK_LEFT_RIGHT_LIGHT_F2: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/svg/look-left-right-light-f2.svg"
));
const LOOK_LEFT_RIGHT_DARK_F2: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/svg/look-left-right-dark-f2.svg"
));
const PALMING_LIGHT_F1: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/svg/palming-light-f1.svg"
));
const PALMING_DARK_F1: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/svg/palming-dark-f1.svg"
));
const MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/manifest.json"
));
/// Every frame of the long-break suggestion exercises; the manifest picks which one is shown.
const SUGGESTION_FRAMES: &[(&str, &[u8])] = &[
    (
        "palming-light-f1.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-light-f1.svg"
        )),
    ),
    (
        "palming-light-f2.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-light-f2.svg"
        )),
    ),
    (
        "palming-light-f3.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-light-f3.svg"
        )),
    ),
    (
        "palming-light-f4.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-light-f4.svg"
        )),
    ),
    (
        "palming-dark-f1.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-dark-f1.svg"
        )),
    ),
    (
        "palming-dark-f2.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-dark-f2.svg"
        )),
    ),
    (
        "palming-dark-f3.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-dark-f3.svg"
        )),
    ),
    (
        "palming-dark-f4.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/palming-dark-f4.svg"
        )),
    ),
    (
        "window-far-light-f1.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/window-far-light-f1.svg"
        )),
    ),
    (
        "window-far-light-f2.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/window-far-light-f2.svg"
        )),
    ),
    (
        "window-far-light-f3.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/window-far-light-f3.svg"
        )),
    ),
    (
        "window-far-dark-f1.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/window-far-dark-f1.svg"
        )),
    ),
    (
        "window-far-dark-f2.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/window-far-dark-f2.svg"
        )),
    ),
    (
        "window-far-dark-f3.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../assets/illustrations/ocy-v1f/svg/window-far-dark-f3.svg"
        )),
    ),
];

/// The exercise's manifest `frozen` frame (the still the host shows in suggestions), rasterised.
fn frozen_art(id: &str, dark: bool, logical: f32) -> Result<slint::Image, String> {
    use damt_art::FrameSource;
    let manifest = damt_art::Manifest::parse(MANIFEST).map_err(|e| e.to_string())?;
    let frozen = manifest
        .get(id)
        .ok_or_else(|| format!("{id} not in manifest"))?
        .frozen;
    let source = damt_art::EmbeddedSource::new(SUGGESTION_FRAMES);
    let theme = if dark {
        damt_art::Theme::Dark
    } else {
        damt_art::Theme::Light
    };
    let svg = source
        .svg(id, theme, frozen)
        .ok_or_else(|| format!("{id} frame {frozen} not embedded"))?;
    art(&svg, logical)
}

fn art(svg: &[u8], logical: f32) -> Result<slint::Image, String> {
    let size = (logical * SCALE).round() as u32;
    let raster = damt_art::Raster::render(svg, size).map_err(|e| e.to_string())?;
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &raster.pixels,
        raster.width,
        raster.height,
    );
    Ok(slint::Image::from_rgba8_premultiplied(buffer))
}

/// One rendered gallery entry.
#[derive(Debug)]
pub struct Shot {
    pub theme: &'static str,
    pub name: &'static str,
    pub image: Rgba,
}

fn short_break(dark: bool, palming: bool) -> Result<Rgba, String> {
    let ui = ShortBreakCard::new().map_err(|e| e.to_string())?;
    let window = take_window()?;
    ui.global::<Theme>().set_dark(dark);
    let (svg, title, body) = match (palming, dark) {
        (false, false) => (
            LOOK_LEFT_RIGHT_LIGHT_F2,
            "Look left and right",
            "Keep your head still and look left, then right. Repeat at an easy pace.",
        ),
        (false, true) => (
            LOOK_LEFT_RIGHT_DARK_F2,
            "Look left and right",
            "Keep your head still and look left, then right. Repeat at an easy pace.",
        ),
        (true, false) => (
            PALMING_LIGHT_F1,
            "Rest your eyes in your palms",
            "Warm your hands by rubbing them together, then rest your cupped palms over closed eyes.",
        ),
        (true, true) => (
            PALMING_DARK_F1,
            "Rest your eyes in your palms",
            "Warm your hands by rubbing them together, then rest your cupped palms over closed eyes.",
        ),
    };
    ui.set_art(art(svg, 56.0)?);
    ui.set_heading(title.into());
    ui.set_body(body.into());
    ui.set_close_label("Close".into());
    ui.set_progress(0.8);
    Ok(render(&window, 392.0, || ui.get_content_height()))
}

fn warning(dark: bool, strict: bool) -> Result<Rgba, String> {
    let ui = WarningCard::new().map_err(|e| e.to_string())?;
    let window = take_window()?;
    ui.global::<Theme>().set_dark(dark);
    ui.set_heading("Long break in 0:42".into());
    ui.set_start_now_label("Start now".into());
    ui.set_postpone_label("Postpone 3 min".into());
    ui.set_skip_label("Skip".into());
    ui.set_strict_label("Strict mode is on".into());
    ui.set_strict(strict);
    Ok(render(&window, 392.0, || ui.get_content_height()))
}

fn long_break(dark: bool) -> Result<Rgba, String> {
    let ui = LongBreakCard::new().map_err(|e| e.to_string())?;
    let window = take_window()?;
    ui.global::<Theme>().set_dark(dark);
    ui.set_countdown_label("Back to your screen in".into());
    ui.set_countdown("4:12".into());
    ui.set_heading("Time to step away".into());
    ui.set_body("Step away from your screen and take a few unhurried minutes.".into());
    ui.set_suggestion1_art(frozen_art("window-far", dark, 40.0)?);
    ui.set_suggestion1_title("Look into the distance".into());
    ui.set_suggestion1_body(
        "Look through a window at a distant point, and let your gaze rest there.".into(),
    );
    ui.set_suggestion2_art(frozen_art("palming", dark, 40.0)?);
    ui.set_suggestion2_title("Rest your eyes in your palms".into());
    ui.set_suggestion2_body(
        "Warm your hands by rubbing them together, then rest your cupped palms over closed eyes."
            .into(),
    );
    ui.set_progress(0.16);
    ui.set_end_label("End break".into());
    Ok(render(&window, 640.0, || 520.0))
}

/// Renders every gallery state in light and dark.
pub fn render_all() -> Result<Vec<Shot>, String> {
    init_platform();
    let mut shots = Vec::new();
    for (theme, dark) in [("light", false), ("dark", true)] {
        shots.push(Shot {
            theme,
            name: "short-break",
            image: short_break(dark, false)?,
        });
        shots.push(Shot {
            theme,
            name: "short-break-palming",
            image: short_break(dark, true)?,
        });
        shots.push(Shot {
            theme,
            name: "warning",
            image: warning(dark, false)?,
        });
        shots.push(Shot {
            theme,
            name: "warning-strict",
            image: warning(dark, true)?,
        });
        shots.push(Shot {
            theme,
            name: "long-break",
            image: long_break(dark)?,
        });
        for &state in V2_STATES {
            shots.push(Shot {
                theme,
                name: state.name,
                image: settings_v2_shot(state, dark)?,
            });
        }
        for &state in MODE_STATES {
            shots.push(Shot {
                theme,
                name: state.name,
                image: mode_shot(state, dark)?,
            });
        }
    }
    Ok(shots)
}

/// Renders every state and writes `<out_dir>/<theme>/<name>.png`; returns the written paths.
pub fn write_all(out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for shot in render_all()? {
        let path = out_dir.join(shot.theme).join(format!("{}.png", shot.name));
        write_png(&path, &shot.image)?;
        paths.push(path);
    }
    Ok(paths)
}

// ---- Settings V2 ----

const EN_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../content/strings/en.json"
));
const DE_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../content/strings/de.json"
));
const JA_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../content/strings/ja.json"
));
const BLINK_LIGHT_F1: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/svg/blink-light-f1.svg"
));
const BLINK_DARK_F1: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/svg/blink-dark-f1.svg"
));

macro_rules! mode_svg {
    ($name:expr) => {
        (
            $name,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../assets/illustrations/modes/svg/",
                $name
            )) as &[u8],
        )
    };
}
macro_rules! mode_svgs {
    ($($mode:literal),*) => {
        &[$(
            mode_svg!(concat!("scene-", $mode, "-light-f1.svg")),
            mode_svg!(concat!("scene-", $mode, "-light-f2.svg")),
            mode_svg!(concat!("scene-", $mode, "-light-f3.svg")),
            mode_svg!(concat!("scene-", $mode, "-light-f4.svg")),
            mode_svg!(concat!("scene-", $mode, "-dark-f1.svg")),
            mode_svg!(concat!("scene-", $mode, "-dark-f2.svg")),
            mode_svg!(concat!("scene-", $mode, "-dark-f3.svg")),
            mode_svg!(concat!("scene-", $mode, "-dark-f4.svg")),
            mode_svg!(concat!("glyph-", $mode, "-light-f1.svg")),
            mode_svg!(concat!("glyph-", $mode, "-dark-f1.svg")),
        )*]
    };
}
const MODE_SVGS: &[(&str, &[u8])] = mode_svgs!("gentle", "balanced", "focused", "custom");
const MODES: [&str; 4] = ["gentle", "balanced", "focused", "custom"];

fn mode_svg(name: &str) -> Result<&'static [u8], String> {
    MODE_SVGS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, b)| *b)
        .ok_or_else(|| format!("{name} not embedded"))
}

fn art_width(svg: &[u8], logical_w: f32) -> Result<slint::Image, String> {
    let w = (logical_w * SCALE).round() as u32;
    let raster = damt_art::Raster::render_width(svg, w).map_err(|e| e.to_string())?;
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &raster.pixels,
        raster.width,
        raster.height,
    );
    Ok(slint::Image::from_rgba8_premultiplied(buffer))
}

/// Sets the placeholder mode art (16 scene frames, 4 glyphs) and the mode descriptions, as the host does.
fn set_mode_art(ui: &ocy_ui::SettingsWindowV2, t: &Table, dark: bool) -> Result<(), String> {
    let theme = if dark { "dark" } else { "light" };
    let mut scenes = Vec::new();
    let mut glyphs = Vec::new();
    for mode in MODES {
        for f in 1..=4 {
            scenes.push(art_width(
                mode_svg(&format!("scene-{mode}-{theme}-f{f}.svg"))?,
                160.0,
            )?);
        }
        glyphs.push(art(
            mode_svg(&format!("glyph-{mode}-{theme}-f1.svg"))?,
            20.0,
        )?);
    }
    ui.set_mode_scenes(slint::ModelRc::new(slint::VecModel::from(scenes)));
    ui.set_mode_glyphs(slint::ModelRc::new(slint::VecModel::from(glyphs)));
    let d = |m: &str| -> slint::SharedString {
        t.get(&format!("settings.v2.mode.{m}.description")).into()
    };
    ui.set_mode_gentle_description(d("gentle"));
    ui.set_mode_balanced_description(d("balanced"));
    ui.set_mode_focused_description(d("focused"));
    ui.set_mode_custom_description(d("custom"));
    Ok(())
}

/// One V11 gallery state: a mode (preset 0..3), an idle frame, and an optional Store licence state.
#[derive(Debug, Clone, Copy)]
pub struct ModeState {
    pub name: &'static str,
    pub preset: i32,
    pub frame: i32,
    /// `None` = GitHub build (no Store elements); else the `license-state` tag.
    pub license: Option<&'static str>,
    pub days_left: i32,
}

pub const MODE_STATES: &[ModeState] = &[
    ModeState {
        name: "v11-mode-gentle",
        preset: 0,
        frame: 1,
        license: None,
        days_left: -1,
    },
    ModeState {
        name: "v11-mode-balanced",
        preset: 1,
        frame: 1,
        license: None,
        days_left: -1,
    },
    ModeState {
        name: "v11-mode-focused",
        preset: 2,
        frame: 1,
        license: None,
        days_left: -1,
    },
    ModeState {
        name: "v11-mode-yours",
        preset: 3,
        frame: 1,
        license: None,
        days_left: -1,
    },
    ModeState {
        name: "v11-mode-balanced-f3",
        preset: 1,
        frame: 3,
        license: None,
        days_left: -1,
    },
    ModeState {
        name: "v11-store-trial",
        preset: 1,
        frame: 1,
        license: Some("trial"),
        days_left: 12,
    },
    ModeState {
        name: "v11-store-trial-ended",
        preset: 1,
        frame: 1,
        license: Some("trial-expired"),
        days_left: 0,
    },
    ModeState {
        name: "v11-store-full",
        preset: 1,
        frame: 1,
        license: Some("full"),
        days_left: -1,
    },
];

fn mode_shot(state: ModeState, dark: bool) -> Result<Rgba, String> {
    let ui = settings_v2("en", dark)?;
    let t = Table::load("en")?;
    let window = take_window()?;
    ui.set_preset(state.preset);
    ui.set_mode_frame(state.frame);
    if let Some(license) = state.license {
        ui.set_store_build(true);
        ui.set_license_state(license.into());
        ui.set_trial_days_left(state.days_left);
        ui.set_trial_days_left_text(
            t.fmt(
                if state.days_left == 1 {
                    "store.trialDaysLeft.one"
                } else {
                    "store.trialDaysLeft.other"
                },
                &[("days", state.days_left.to_string())],
            )
            .into(),
        );
        ui.set_trial_ended_title(t.get("store.trialEnded.title").into());
        ui.set_trial_ended_body(t.get("store.trialEnded.body").into());
        ui.set_buy_label(t.get("store.buy").into());
        ui.set_made_with_slint_label(t.get("store.madeWithSlint").into());
    }
    Ok(render(&window, 560.0, || {
        640.0_f32.max(ui.get_content_height().ceil())
    }))
}

/// Parses a flat `{"key": "string", ...}` JSON object (the string tables have no nesting).
pub fn parse_flat_json(src: &str) -> Result<Vec<(String, String)>, String> {
    fn string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, String> {
        let mut out = String::new();
        loop {
            match chars.next().ok_or("unterminated string")? {
                '"' => return Ok(out),
                '\\' => match chars.next().ok_or("bad escape")? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'u' => {
                        let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                        let code = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
                        out.push(char::from_u32(code).ok_or("bad \\u escape")?);
                    }
                    c => out.push(c),
                },
                c => out.push(c),
            }
        }
    }
    let mut chars = src.chars().peekable();
    let mut out = Vec::new();
    let mut key: Option<String> = None;
    while let Some(c) = chars.next() {
        if c == '"' {
            let s = string(&mut chars)?;
            match key.take() {
                None => key = Some(s),
                Some(k) => out.push((k, s)),
            }
        }
    }
    Ok(out)
}

/// A string table with English fallback.
pub struct Table(std::collections::HashMap<String, String>);

impl Table {
    pub fn load(lang: &str) -> Result<Self, String> {
        let mut map = std::collections::HashMap::new();
        let mut layers = vec![EN_JSON];
        match lang {
            "de" => layers.push(DE_JSON),
            "ja" => layers.push(JA_JSON),
            _ => {}
        }
        for layer in layers {
            map.extend(parse_flat_json(layer)?);
        }
        Ok(Self(map))
    }
    pub fn get(&self, key: &str) -> String {
        self.0.get(key).cloned().unwrap_or_else(|| key.to_owned())
    }
    pub fn fmt(&self, key: &str, args: &[(&str, String)]) -> String {
        let mut s = self.get(key);
        for (name, value) in args {
            s = s.replace(&format!("{{{name}}}"), value);
        }
        s
    }
    fn plural(&self, key: &str, count: i32) -> String {
        let form = if count == 1 { "one" } else { "other" };
        let k = format!("{key}.{form}");
        let k = if self.0.contains_key(&k) {
            k
        } else {
            format!("{key}.other")
        };
        self.fmt(&k, &[("count", count.to_string())])
    }
}

/// Builds a fully labelled `SettingsWindowV2` in `lang` ("en", "de", "ja") with sample data.
pub fn settings_v2(lang: &str, dark: bool) -> Result<ocy_ui::SettingsWindowV2, String> {
    use ocy_ui::{TimelineMark, TimelineMarkKind};
    let t = Rc::new(Table::load(lang)?);
    let ui = ocy_ui::SettingsWindowV2::new().map_err(|e| e.to_string())?;
    ui.global::<Theme>().set_dark(dark);
    if lang == "ja" {
        ui.set_font_family("Yu Gothic UI".into());
    }
    let s = |k: &str| -> slint::SharedString { t.get(k).into() };
    ui.set_window_title(
        t.fmt(
            "settings.windowTitle",
            &[("app", "Ocy".into()), ("name", "Ocy".into())],
        )
        .into(),
    );
    ui.set_mascot(art(
        if dark { BLINK_DARK_F1 } else { BLINK_LIGHT_F1 },
        48.0,
    )?);
    set_mode_art(&ui, &t, dark)?;
    ui.set_next_break_text(
        t.fmt(
            "settings.v2.header.nextShort",
            &[(
                "time",
                t.fmt("settings.v2.value.minutes", &[("minutes", "7".into())]),
            )],
        )
        .into(),
    );
    ui.set_today_title(s("settings.v2.today.title"));
    ui.set_today_taken_label(s("settings.v2.today.taken"));
    ui.set_today_skipped_label(s("settings.v2.today.skipped"));
    ui.set_today_taken(9);
    ui.set_today_skipped(1);
    ui.set_timeline_title(s("settings.v2.timeline.title"));
    ui.set_timeline_short_label(s("settings.v2.timeline.shortBreaks"));
    ui.set_timeline_long_label(s("settings.v2.timeline.longBreaks"));
    ui.set_now_label(s("settings.v2.timeline.now"));
    // Balanced: short every 25 min (next in 7), long every 60 min (next in 25), over 120 min.
    let mut marks = Vec::new();
    // A long break replaces the short one that falls on it.
    for at in [7, 32, 57, 82, 107] {
        marks.push(TimelineMark {
            kind: TimelineMarkKind::ShortBreak,
            position: at as f32 / 120.0,
        });
    }
    for at in [25, 85] {
        marks.push(TimelineMark {
            kind: TimelineMarkKind::LongBreak,
            position: at as f32 / 120.0,
        });
    }
    ui.set_timeline(slint::ModelRc::new(slint::VecModel::from(marks)));
    let labels: Vec<slint::SharedString> = ["15:00", "15:30", "16:00", "16:30"]
        .iter()
        .map(|&l| l.into())
        .collect();
    ui.set_timeline_labels(slint::ModelRc::new(slint::VecModel::from(labels)));
    ui.set_preset_gentle_label(s("settings.v2.preset.gentle"));
    ui.set_preset_balanced_label(s("settings.v2.preset.balanced"));
    ui.set_preset_focused_label(s("settings.v2.preset.focused"));
    ui.set_preset_custom_label(s("settings.v2.preset.custom"));
    let summary = |sd: i32, si: i32, ld: i32, li: i32| -> slint::SharedString {
        format!(
            "{}\n{}",
            t.fmt(
                "settings.v2.preset.shortSummary",
                &[("seconds", sd.to_string()), ("minutes", si.to_string())]
            ),
            t.fmt(
                "settings.v2.preset.longSummary",
                &[("duration", ld.to_string()), ("minutes", li.to_string())]
            ),
        )
        .into()
    };
    ui.set_preset_gentle_summary(summary(15, 40, 5, 90));
    ui.set_preset_balanced_summary(summary(15, 25, 5, 60));
    ui.set_preset_focused_summary(summary(20, 50, 10, 90));
    ui.set_customize_title(s("settings.v2.section.customize"));
    ui.set_general_title(s("settings.section.general"));
    ui.set_tab_breaks_label(s("settings.v2.tab.breaks"));
    ui.set_tab_strictness_label(s("settings.v2.tab.strictness"));
    ui.set_tab_quiet_label(s("settings.v2.tab.quietTimes"));
    ui.set_start_with_system_label(s("settings.startWithSystem.label"));
    ui.set_start_with_system_help(s("settings.startWithSystem.help"));
    ui.set_short_enabled_label(s("settings.shortBreaks.enabled.label"));
    ui.set_short_interval_label(s("settings.shortBreaks.intervalMinutes.label"));
    ui.set_short_duration_label(s("settings.shortBreaks.durationSeconds.label"));
    ui.set_short_fullscreen_label(s("settings.shortBreaks.fullscreen.label"));
    ui.set_short_fullscreen_help(s("settings.shortBreaks.fullscreen.help"));
    ui.set_try_short_label(s("settings.tryShortBreak"));
    ui.set_long_enabled_label(s("settings.longBreaks.enabled.label"));
    ui.set_long_interval_label(s("settings.longBreaks.intervalMinutes.label"));
    ui.set_long_duration_label(s("settings.longBreaks.durationMinutes.label"));
    ui.set_long_warning_label(s("settings.longBreaks.warningSeconds.label"));
    ui.set_long_postpone_label(s("settings.longBreaks.postponeMinutes.label"));
    ui.set_long_max_postpones_label(s("settings.longBreaks.maxPostpones.label"));
    ui.set_try_long_label(s("settings.tryLongBreak"));
    ui.set_water_nudges_label(s("settings.waterNudges.label"));
    ui.set_water_nudges_help(s("settings.waterNudges.help"));
    ui.set_usage_stats_label(s("settings.usageStats.label"));
    ui.set_usage_stats_help(s("settings.usageStats.help"));
    ui.set_usage_stats_link(s("settings.usageStats.link"));
    ui.set_usage_stats_ask_title(s("settings.usageStats.ask.title"));
    ui.set_usage_stats_ask_body(s("settings.usageStats.ask.body"));
    ui.set_usage_stats_share_label(s("settings.usageStats.ask.share"));
    ui.set_usage_stats_decline_label(s("settings.usageStats.ask.decline"));
    ui.set_strict_label(s("settings.strictMode.label"));
    ui.set_strict_help(s("settings.strictMode.help"));
    ui.set_inactivity_label(s("settings.inactivityTracking.label"));
    ui.set_inactivity_help(s("settings.inactivityTracking.help"));
    ui.set_away_label(s("settings.awayThresholdSeconds.label"));
    ui.set_pause_fullscreen_label(s("settings.pauseDuringFullscreen.label"));
    ui.set_pause_fullscreen_help(s("settings.pauseDuringFullscreen.help"));
    ui.set_games_title(s("settings.section.games"));
    ui.set_games_mode_model(slint::ModelRc::new(slint::VecModel::from(vec![
        s("settings.games.gentle"),
        s("settings.games.hold"),
    ])));
    ui.set_games_gentle_help(s("settings.games.gentleHelp"));
    ui.set_games_hold_help(s("settings.games.holdHelp"));
    ui.set_games_silent_title(s("settings.games.silent"));
    ui.set_games_silent_empty(s("settings.games.silentEmpty"));
    ui.set_window_nearby_label(s("settings.hasWindowNearby.label"));
    ui.set_window_nearby_help(s("settings.hasWindowNearby.help"));
    ui.set_language_label(s("settings.language.label"));
    ui.set_sounds_label(s("settings.sounds.label"));
    ui.set_sounds_help(s("settings.sounds.help"));
    ui.set_notifications_label(s("settings.statusNotifications.label"));
    ui.set_notifications_help(s("settings.statusNotifications.help"));
    ui.set_about_name_version(
        t.fmt(
            "about.version",
            &[("name", "Ocy".into()), ("version", "0.2.1".into())],
        )
        .into(),
    );
    ui.set_about_tagline(s("app.tagline"));
    let languages: Vec<slint::SharedString> = [
        t.get("settings.v2.language.automatic"),
        "English".into(),
        "Deutsch".into(),
        "日本語".into(),
        "Македонски".into(),
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    ui.set_language_model(slint::ModelRc::new(slint::VecModel::from(languages)));
    ui.set_language_index(0);
    ui.set_start_with_system(true);
    ui.set_preset(1);
    ui.set_short_interval(25);
    ui.set_short_duration(15);
    ui.set_long_interval(60);
    ui.set_long_duration(5);

    let tt = Rc::clone(&t);
    ui.on_format_every_minutes(move |v| {
        tt.fmt(
            "settings.v2.value.everyMinutes",
            &[("minutes", v.to_string())],
        )
        .into()
    });
    let tt = Rc::clone(&t);
    ui.on_format_minutes(move |v| {
        tt.fmt("settings.v2.value.minutes", &[("minutes", v.to_string())])
            .into()
    });
    let tt = Rc::clone(&t);
    ui.on_format_seconds(move |v| {
        tt.fmt("settings.v2.value.seconds", &[("seconds", v.to_string())])
            .into()
    });
    let tt = Rc::clone(&t);
    ui.on_format_times(move |v| tt.plural("settings.v2.value.times", v).into());
    let tt = Rc::clone(&t);
    ui.on_format_duration(move |v| {
        if v % 60 == 0 {
            tt.fmt(
                "settings.v2.value.minutes",
                &[("minutes", (v / 60).to_string())],
            )
            .into()
        } else {
            tt.fmt("settings.v2.value.seconds", &[("seconds", v.to_string())])
                .into()
        }
    });
    Ok(ui)
}

/// One V2 gallery state.
#[derive(Debug, Clone, Copy)]
pub struct V2State {
    pub name: &'static str,
    pub lang: &'static str,
    pub customize_tab: Option<i32>,
    /// General stays open alongside Customize (both sections open).
    pub general_open: bool,
    pub width: f32,
    pub height: f32,
}

pub const V2_STATES: &[V2State] = &[
    V2State {
        name: "settings-v2",
        lang: "en",
        customize_tab: None,
        general_open: true,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-breaks",
        lang: "en",
        customize_tab: Some(0),
        general_open: false,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-strictness",
        lang: "en",
        customize_tab: Some(1),
        general_open: false,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-quiet",
        lang: "en",
        customize_tab: Some(2),
        general_open: false,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-de",
        lang: "de",
        customize_tab: None,
        general_open: true,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-de-breaks",
        lang: "de",
        customize_tab: Some(0),
        general_open: false,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-ja",
        lang: "ja",
        customize_tab: None,
        general_open: true,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-ja-breaks",
        lang: "ja",
        customize_tab: Some(0),
        general_open: false,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-compact",
        lang: "en",
        customize_tab: None,
        general_open: true,
        width: 480.0,
        height: 400.0,
    },
    V2State {
        name: "settings-v2-compact-breaks",
        lang: "en",
        customize_tab: Some(0),
        general_open: false,
        width: 480.0,
        height: 400.0,
    },
    V2State {
        name: "settings-v2-both-open",
        lang: "en",
        customize_tab: Some(0),
        general_open: true,
        width: 560.0,
        height: 640.0,
    },
    V2State {
        name: "settings-v2-de-both-open",
        lang: "de",
        customize_tab: Some(0),
        general_open: true,
        width: 560.0,
        height: 640.0,
    },
];

fn settings_v2_shot(state: V2State, dark: bool) -> Result<Rgba, String> {
    let ui = settings_v2(state.lang, dark)?;
    let window = take_window()?;
    if let Some(tab) = state.customize_tab {
        ui.set_customize_open(true);
        ui.set_customize_tab(tab);
    }
    ui.set_general_open(state.general_open);
    // At the default size the host grows the window to the content (see `settings_window_height`).
    let grow = state.height >= 640.0;
    Ok(render(&window, state.width, || {
        if grow {
            state.height.max(ui.get_content_height().ceil())
        } else {
            state.height
        }
    }))
}

// ---- Usage stats consent (AN2 review set) ----

/// Consent state for a review render: the ask card, after "No thanks", after "Share stats".
#[derive(Debug, Clone, Copy)]
pub enum ConsentState {
    Ask,
    /// The ask with two long (English) button labels: a layout stress test for long languages.
    AskLongLabels,
    Declined,
    Granted,
}

/// Settings V2 with the usage stats UI enabled (as in a build with a stats key), General open.
/// The buttons are clicked through the real Slint handlers so the after-states come from the UI.
pub fn consent_shot(
    lang: &str,
    dark: bool,
    state: ConsentState,
    width: f32,
) -> Result<Rgba, String> {
    let ui = settings_v2(lang, dark)?;
    let window = take_window()?;
    ui.set_show_usage_stats(true);
    ui.set_usage_stats_on(false);
    ui.set_usage_stats_ask(true);
    ui.set_general_open(true);
    match state {
        ConsentState::Ask => {}
        ConsentState::AskLongLabels => {
            ui.set_usage_stats_share_label("Share my anonymous usage statistics".into());
            ui.set_usage_stats_decline_label("No thank you, do not share".into());
        }
        ConsentState::Declined => ui.invoke_choose_usage_stats(false),
        ConsentState::Granted => ui.invoke_choose_usage_stats(true),
    }
    Ok(render(&window, width, || {
        640f32.max(ui.get_content_height().ceil())
    }))
}
