//! Headless rendering of every Ocy surface with Slint's software renderer on a minimal custom platform.
//! Shared by `examples/gallery.rs` and the gallery tests.
#![allow(dead_code)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ocy_ui::{LongBreakCard, SettingsWindow, ShortBreakCard, Theme, WarningCard};
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

fn settings(dark: bool) -> Result<Rgba, String> {
    let ui = SettingsWindow::new().map_err(|e| e.to_string())?;
    let window = take_window()?;
    ui.global::<Theme>().set_dark(dark);
    ui.set_window_title("Ocy — Settings".into());
    ui.set_general_title("General".into());
    ui.set_short_breaks_title("Short breaks".into());
    ui.set_long_breaks_title("Long breaks".into());
    ui.set_behaviour_title("Behaviour".into());
    ui.set_about_title("About".into());
    ui.set_start_with_system_label("Start with system".into());
    ui.set_start_with_system_help("Open Ocy automatically when you sign in.".into());
    ui.set_short_enabled_label("Short breaks".into());
    ui.set_short_interval_label("Short break interval".into());
    ui.set_short_interval_help("Choose how often a short break is due.".into());
    ui.set_short_duration_label("Short break length".into());
    ui.set_short_duration_help("Choose how long each short break lasts.".into());
    ui.set_short_fullscreen_label("Full-screen short breaks".into());
    ui.set_short_fullscreen_help(
        "Show short breaks over a dimmed screen instead of a small corner window.".into(),
    );
    ui.set_try_short_label("Try a short break".into());
    ui.set_long_enabled_label("Long breaks".into());
    ui.set_long_interval_label("Long break interval".into());
    ui.set_long_interval_help("Choose how often a long break is due.".into());
    ui.set_long_duration_label("Long break length".into());
    ui.set_long_duration_help("Choose how long each long break lasts.".into());
    ui.set_long_warning_label("Warning time".into());
    ui.set_long_warning_help(
        "Choose how long before a long break to show its warning; set to zero for no warning."
            .into(),
    );
    ui.set_long_postpone_label("Postpone by".into());
    ui.set_long_postpone_help("Choose how long a postponement lasts.".into());
    ui.set_long_max_postpones_label("Maximum postpones".into());
    ui.set_long_max_postpones_help("Set how many times you can postpone a long break.".into());
    ui.set_try_long_label("Try a long break".into());
    ui.set_strict_label("Strict mode".into());
    ui.set_strict_help(
        "Keep long breaks on schedule; skipping and postponing them is unavailable.".into(),
    );
    ui.set_inactivity_label("Track inactivity".into());
    ui.set_inactivity_help("Pause break timing while you are away from your computer.".into());
    ui.set_away_label("Away after".into());
    ui.set_away_help("Count you as away after this much idle time.".into());
    ui.set_pause_fullscreen_label("Pause in fullscreen".into());
    ui.set_pause_fullscreen_help(
        "Defer breaks while a fullscreen app or presentation is active.".into(),
    );
    ui.set_window_nearby_label("A window is nearby".into());
    ui.set_window_nearby_help(
        "Turn this on if you can look through a window from your workspace.".into(),
    );
    ui.set_unit_minutes("min".into());
    ui.set_unit_seconds("s".into());
    ui.set_about_name_version("Ocy 0.2.0".into());
    ui.set_about_tagline("A quieter rhythm for time at your screen.".into());
    Ok(render(&window, 560.0, || 640.0))
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
        shots.push(Shot {
            theme,
            name: "settings",
            image: settings(dark)?,
        });
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
