//! README / website app screenshots for a release: renders the real Ocy surfaces (`ui/ocy.slint`,
//! interpreted, with the shipped generated theme) with real values in English, light and dark, to
//! `<out_dir>/{light,dark}/{settings,short-break,short-roll,long-break,warning}.png` at 2x, plus the exercise
//! strip frames `<out_dir>/exercises/{light,dark}/NNN.png`. `site/scripts/readme-visuals.py` fits
//! them into the committed frames (`assets/readme/*`, `site/public/assets/screens/*`).
//!
//! `cargo run -p ocy-ui --example readme_screens -- <out_dir>`
//!
//! `cargo run -p ocy-ui --example readme_screens -- <out_dir> games` renders the gaming-mode review
//! set instead: `<out_dir>/{light,dark}/{settings-games-empty,settings-games-list,gentle-short,
//! gentle-long}.png`.
#![deny(clippy::unwrap_used, clippy::expect_used)]

#[path = "../tests/support/mod.rs"]
mod support;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::PhysicalSize;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint_interpreter::{ComponentDefinition, ComponentInstance, Value};
use support::{Rgba, SCALE, Table, write_png};

/// The six exercises in the README strip, in order.
const STRIP: &[&str] = &[
    "blink",
    "look-left-right",
    "close-tight",
    "figure-eight",
    "palming",
    "stretch-arms",
];
/// Strip animation: frames at this tick, for this many ticks.
const TICK_MS: u32 = 60;
const TICKS: u32 = 126;
const ART: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/svg"
);
const MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/manifest.json"
);

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

fn main() -> std::process::ExitCode {
    let Some(out_dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: readme_screens <out_dir>");
        return std::process::ExitCode::from(2);
    };
    let games = std::env::args().nth(2).is_some_and(|mode| mode == "games");
    let result = if games {
        run_games(Path::new(&out_dir))
    } else {
        run(Path::new(&out_dir))
    };
    match result {
        Ok(paths) => {
            for path in paths {
                println!("{}", path.display());
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("readme_screens failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn compile() -> Result<slint_interpreter::CompilationResult, String> {
    // Err means a platform is already set, which is fine.
    let _ = slint::platform::set_platform(Box::new(HeadlessPlatform));
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut paths = HashMap::new();
    paths.insert(damt_ui::LIBRARY_NAME.to_owned(), damt_ui::ui_dir());
    paths.insert(
        damt_ui::THEME_LIBRARY_NAME.to_owned(),
        manifest.join("../theme/generated"),
    );
    let mut compiler = slint_interpreter::Compiler::default();
    compiler.set_library_paths(paths);
    compiler.set_style("fluent".into());
    let result = spin_on::spin_on(compiler.build_from_path(manifest.join("ui/ocy.slint")));
    let mut errors = String::new();
    for d in result.diagnostics() {
        if d.level() == slint_interpreter::DiagnosticLevel::Error {
            errors.push_str(&format!("{d}\n"));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(result)
}

fn run_games(out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let result = compile()?;
    let get = |name: &str| {
        result
            .component(name)
            .ok_or_else(|| format!("component {name} missing"))
    };
    let mut written = Vec::new();
    for (theme, dark) in [("light", false), ("dark", true)] {
        let shots = [
            (
                "settings-games-empty",
                settings(&get("SettingsWindowV2")?, dark, Some(&[]))?,
            ),
            (
                "settings-games-list",
                settings(
                    &get("SettingsWindowV2")?,
                    dark,
                    Some(&["game.exe", "another-game.exe"]),
                )?,
            ),
            (
                "gentle-short",
                gentle_card(&get("ShortBreakCard")?, dark, false)?,
            ),
            (
                "gentle-long",
                gentle_card(&get("ShortBreakCard")?, dark, true)?,
            ),
        ];
        for (screen, image) in shots {
            let path = out_dir.join(theme).join(format!("{screen}.png"));
            write_png(&path, &image)?;
            written.push(path);
        }
    }
    Ok(written)
}

fn run(out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let result = compile()?;
    let mut written = Vec::new();
    let get = |name: &str| {
        result
            .component(name)
            .ok_or_else(|| format!("component {name} missing"))
    };
    for (theme, dark) in [("light", false), ("dark", true)] {
        let shots = [
            ("settings", settings(&get("SettingsWindowV2")?, dark, None)?),
            ("short-break", short_break(&get("ShortBreakCard")?, dark)?),
            (
                "short-roll",
                short_exercise(&get("ShortBreakCard")?, dark, "roll")?,
            ),
            ("long-break", long_break(&get("LongBreakCard")?, dark)?),
            ("warning", warning(&get("WarningCard")?, dark)?),
        ];
        for (screen, image) in shots {
            let path = out_dir.join(theme).join(format!("{screen}.png"));
            write_png(&path, &image)?;
            written.push(path);
        }
        written.extend(strip_frames(&out_dir.join("exercises").join(theme), theme)?);
    }
    Ok(written)
}

/// One PNG per tick: each exercise's current frame (per the manifest's sequence and step) at
/// 240 px, side by side in 300 px cells; `readme-visuals.py` draws the panels and encodes the webp.
fn strip_frames(dir: &Path, theme: &str) -> Result<Vec<PathBuf>, String> {
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(MANIFEST).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut cache: HashMap<String, Rgba> = HashMap::new();
    let mut out = Vec::new();
    for tick in 0..TICKS {
        let cell = 300u32;
        let width = cell * STRIP.len() as u32;
        let mut pixels = vec![[0u8; 4]; (width * cell) as usize];
        for (i, ex) in STRIP.iter().enumerate() {
            let e = &manifest["exercises"][*ex];
            let step = e["stepMs"].as_u64().unwrap_or(300) as u32;
            let seq = e["sequence"].as_array().ok_or("bad manifest")?;
            let frame = seq[((tick * TICK_MS / step) as usize) % seq.len()]
                .as_u64()
                .unwrap_or(1);
            let file = format!("{ex}-{theme}-f{frame}.svg");
            if !cache.contains_key(&file) {
                cache.insert(file.clone(), raster_px(&file, 240)?);
            }
            let img = &cache[&file];
            let (x0, y0) = (
                i as u32 * cell + (cell - img.width) / 2,
                (cell - img.height) / 2,
            );
            for y in 0..img.height {
                for x in 0..img.width {
                    pixels[((y0 + y) * width + x0 + x) as usize] =
                        img.pixels[(y * img.width + x) as usize];
                }
            }
        }
        let path = dir.join(format!("{tick:03}.png"));
        write_png(
            &path,
            &Rgba {
                width,
                height: cell,
                pixels,
            },
        )?;
        out.push(path);
    }
    Ok(out)
}

fn take_window() -> Result<Rc<MinimalSoftwareWindow>, String> {
    LAST_WINDOW
        .with(|slot| slot.borrow_mut().take())
        .ok_or_else(|| "no window adapter was created".to_owned())
}

/// Same render loop as the gallery (`tests/support`), for an interpreted instance.
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
        height: size.1,
        pixels,
    }
}

struct Ui(ComponentInstance);

impl Ui {
    fn new(def: &ComponentDefinition, dark: bool) -> Result<Self, String> {
        let ui = Ui(def.create().map_err(|e| e.to_string())?);
        ui.0.set_global_property("Theme", "dark", Value::Bool(dark))
            .map_err(|e| e.to_string())?;
        Ok(ui)
    }
    fn set(&self, name: &str, value: impl Into<Value>) -> Result<(), String> {
        self.0
            .set_property(name, value.into())
            .map_err(|e| format!("{name}: {e}"))
    }
    fn text(&self, name: &str, value: &str) -> Result<(), String> {
        self.set(name, slint::SharedString::from(value))
    }
    fn num(&self, name: &str, value: f64) -> Result<(), String> {
        self.set(name, Value::Number(value))
    }
    fn height(&self) -> f32 {
        match self.0.get_property("content-height") {
            Ok(Value::Number(h)) => h as f32,
            _ => 0.0,
        }
    }
}

/// An art frame rasterised at `size` physical px, straight alpha.
fn raster_px(file: &str, size: u32) -> Result<Rgba, String> {
    let svg = std::fs::read(Path::new(ART).join(file)).map_err(|e| format!("{file}: {e}"))?;
    let r = damt_art::Raster::render(&svg, size).map_err(|e| e.to_string())?;
    let pixels = r
        .pixels
        .chunks_exact(4)
        .map(|p| {
            let a = p[3];
            if a == 0 {
                [0, 0, 0, 0]
            } else {
                let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                [un(p[0]), un(p[1]), un(p[2]), a]
            }
        })
        .collect();
    Ok(Rgba {
        width: r.width,
        height: r.height,
        pixels,
    })
}

fn art(file: &str, logical: f32) -> Result<slint::Image, String> {
    let svg = std::fs::read(Path::new(ART).join(file)).map_err(|e| format!("{file}: {e}"))?;
    let size = (logical * SCALE).round() as u32;
    let raster = damt_art::Raster::render(&svg, size).map_err(|e| e.to_string())?;
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &raster.pixels,
        raster.width,
        raster.height,
    );
    Ok(slint::Image::from_rgba8_premultiplied(buffer))
}

fn theme(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

fn short_break(def: &ComponentDefinition, dark: bool) -> Result<Rgba, String> {
    let ui = Ui::new(def, dark)?;
    let window = take_window()?;
    ui.set(
        "art",
        art(&format!("look-left-right-{}-f2.svg", theme(dark)), 56.0)?,
    )?;
    ui.text("heading", "Look left and right")?;
    ui.text(
        "body",
        "Keep your head still and look left, then right. Repeat at an easy pace.",
    )?;
    ui.text("close-label", "Close")?;
    ui.num("progress", 0.8)?;
    Ok(render(&window, 392.0, || ui.height()))
}

/// A plain short card for one exercise, with its shipped English title, body and first frame.
fn short_exercise(def: &ComponentDefinition, dark: bool, id: &str) -> Result<Rgba, String> {
    let t = Table::load("en")?;
    let ui = Ui::new(def, dark)?;
    let window = take_window()?;
    ui.set("art", art(&format!("{id}-{}-f1.svg", theme(dark)), 56.0)?)?;
    ui.text("heading", &t.get(&format!("exercise.{id}.title")))?;
    ui.text("body", &t.get(&format!("exercise.{id}.body")))?;
    ui.text("close-label", &t.get("shortbreak.skip"))?;
    ui.num("progress", 0.6)?;
    Ok(render(&window, 392.0, || ui.height()))
}

/// A gentle game card: the short card with "Not during this game", plus "Take the long break" on
/// the gentle long card.
fn gentle_card(def: &ComponentDefinition, dark: bool, long: bool) -> Result<Rgba, String> {
    let t = Table::load("en")?;
    let ui = Ui::new(def, dark)?;
    let window = take_window()?;
    if long {
        ui.set(
            "art",
            art(&format!("stand-walk-{}-f1.svg", theme(dark)), 56.0)?,
        )?;
        ui.text("heading", &t.get("exercise.stand-walk.title"))?;
        ui.text("body", &t.get("exercise.stand-walk.body"))?;
    } else {
        ui.set("art", art(&format!("blink-{}-f1.svg", theme(dark)), 56.0)?)?;
        ui.text("heading", &t.get("exercise.blink.title"))?;
        ui.text("body", &t.get("exercise.blink.body"))?;
    }
    ui.text("close-label", &t.get("shortbreak.skip"))?;
    ui.set("show-hold-game", true)?;
    ui.text("hold-game-label", &t.get("games.holdThisGame"))?;
    ui.set("show-take-long", long)?;
    ui.text("take-long-label", &t.get("games.takeLongBreak"))?;
    ui.num("progress", 0.7)?;
    Ok(render(&window, 392.0, || ui.height()))
}

fn warning(def: &ComponentDefinition, dark: bool) -> Result<Rgba, String> {
    let ui = Ui::new(def, dark)?;
    let window = take_window()?;
    ui.text("heading", "Long break in 0:42")?;
    ui.text("start-now-label", "Start now")?;
    ui.text("postpone-label", "Postpone 3 min")?;
    ui.text("skip-label", "Skip")?;
    ui.text("strict-label", "Strict mode is on")?;
    ui.set("strict", false)?;
    Ok(render(&window, 392.0, || ui.height()))
}

fn long_break(def: &ComponentDefinition, dark: bool) -> Result<Rgba, String> {
    let ui = Ui::new(def, dark)?;
    let window = take_window()?;
    let t = theme(dark);
    ui.text("countdown-label", "Back to your screen in")?;
    ui.text("countdown", "4:12")?;
    ui.text("heading", "Time to step away")?;
    ui.text(
        "body",
        "Step away from your screen and take a few unhurried minutes.",
    )?;
    // The manifest's frozen frames (window-far 1, palming 4), as the gallery shows them.
    ui.set(
        "suggestion1-art",
        art(&format!("window-far-{t}-f1.svg"), 40.0)?,
    )?;
    ui.text("suggestion1-title", "Look into the distance")?;
    ui.text(
        "suggestion1-body",
        "Look through a window at a distant point, and let your gaze rest there.",
    )?;
    ui.set(
        "suggestion2-art",
        art(&format!("palming-{t}-f4.svg"), 40.0)?,
    )?;
    ui.text("suggestion2-title", "Rest your eyes in your palms")?;
    ui.text(
        "suggestion2-body",
        "Warm your hands by rubbing them together, then rest your cupped palms over closed eyes.",
    )?;
    ui.num("progress", 0.16)?;
    ui.text("end-label", "End break")?;
    Ok(render(&window, 640.0, || 520.0))
}

/// Settings V2 as it ships, both sections open (the gallery's `settings-v2-both-open`).
const LABELS: &[(&str, &str)] = &[
    ("today-title", "settings.v2.today.title"),
    ("today-taken-label", "settings.v2.today.taken"),
    ("today-skipped-label", "settings.v2.today.skipped"),
    ("timeline-title", "settings.v2.timeline.title"),
    ("timeline-short-label", "settings.v2.timeline.shortBreaks"),
    ("timeline-long-label", "settings.v2.timeline.longBreaks"),
    ("now-label", "settings.v2.timeline.now"),
    ("preset-gentle-label", "settings.v2.preset.gentle"),
    ("preset-balanced-label", "settings.v2.preset.balanced"),
    ("preset-focused-label", "settings.v2.preset.focused"),
    ("preset-custom-label", "settings.v2.preset.custom"),
    ("customize-title", "settings.v2.section.customize"),
    ("general-title", "settings.section.general"),
    ("tab-breaks-label", "settings.v2.tab.breaks"),
    ("tab-strictness-label", "settings.v2.tab.strictness"),
    ("tab-quiet-label", "settings.v2.tab.quietTimes"),
    ("start-with-system-label", "settings.startWithSystem.label"),
    ("start-with-system-help", "settings.startWithSystem.help"),
    ("short-enabled-label", "settings.shortBreaks.enabled.label"),
    (
        "short-interval-label",
        "settings.shortBreaks.intervalMinutes.label",
    ),
    (
        "short-duration-label",
        "settings.shortBreaks.durationSeconds.label",
    ),
    (
        "short-fullscreen-label",
        "settings.shortBreaks.fullscreen.label",
    ),
    (
        "short-fullscreen-help",
        "settings.shortBreaks.fullscreen.help",
    ),
    ("try-short-label", "settings.tryShortBreak"),
    ("long-enabled-label", "settings.longBreaks.enabled.label"),
    (
        "long-interval-label",
        "settings.longBreaks.intervalMinutes.label",
    ),
    (
        "long-duration-label",
        "settings.longBreaks.durationMinutes.label",
    ),
    (
        "long-warning-label",
        "settings.longBreaks.warningSeconds.label",
    ),
    (
        "long-postpone-label",
        "settings.longBreaks.postponeMinutes.label",
    ),
    (
        "long-max-postpones-label",
        "settings.longBreaks.maxPostpones.label",
    ),
    ("try-long-label", "settings.tryLongBreak"),
    ("water-nudges-label", "settings.waterNudges.label"),
    ("water-nudges-help", "settings.waterNudges.help"),
    ("strict-label", "settings.strictMode.label"),
    ("strict-help", "settings.strictMode.help"),
    ("inactivity-label", "settings.inactivityTracking.label"),
    ("inactivity-help", "settings.inactivityTracking.help"),
    ("away-label", "settings.awayThresholdSeconds.label"),
    (
        "pause-fullscreen-label",
        "settings.pauseDuringFullscreen.label",
    ),
    (
        "pause-fullscreen-help",
        "settings.pauseDuringFullscreen.help",
    ),
    ("window-nearby-label", "settings.hasWindowNearby.label"),
    ("window-nearby-help", "settings.hasWindowNearby.help"),
    ("language-label", "settings.language.label"),
    ("sounds-label", "settings.sounds.label"),
    ("sounds-help", "settings.sounds.help"),
    ("notifications-label", "settings.statusNotifications.label"),
    ("notifications-help", "settings.statusNotifications.help"),
    ("about-tagline", "app.tagline"),
];

/// Formats an integer setting for display.
type Formatter = fn(&Table, i64) -> String;

fn settings(def: &ComponentDefinition, dark: bool, games: Option<&[&str]>) -> Result<Rgba, String> {
    let t = Rc::new(Table::load("en")?);
    let ui = Ui::new(def, dark)?;
    let window = take_window()?;
    for (prop, key) in LABELS {
        ui.text(prop, &t.get(key))?;
    }
    ui.text(
        "window-title",
        &t.fmt(
            "settings.windowTitle",
            &[("app", "Ocy".into()), ("name", "Ocy".into())],
        ),
    )?;
    ui.set(
        "mascot",
        art(&format!("blink-{}-f1.svg", theme(dark)), 48.0)?,
    )?;
    ui.text(
        "next-break-text",
        &t.fmt(
            "settings.v2.header.nextShort",
            &[(
                "time",
                t.fmt("settings.v2.value.minutes", &[("minutes", "7".into())]),
            )],
        ),
    )?;
    ui.num("today-taken", 9.0)?;
    ui.num("today-skipped", 1.0)?;
    let mark = |kind: &str, at: f64| {
        let mut s = slint_interpreter::Struct::default();
        s.set_field(
            "kind".into(),
            Value::EnumerationValue("TimelineMarkKind".into(), kind.into()),
        );
        s.set_field("position".into(), Value::Number(at / 120.0));
        Value::Struct(s)
    };
    let mut marks: Vec<Value> = [7.0, 32.0, 82.0, 107.0]
        .into_iter()
        .map(|at| mark("short-break", at))
        .collect();
    marks.extend([57.0, 117.0].into_iter().map(|at| mark("long-break", at)));
    ui.set(
        "timeline",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(marks))),
    )?;
    let labels: Vec<Value> = ["15:00", "15:30", "16:00", "16:30"]
        .into_iter()
        .map(|l| Value::String(l.into()))
        .collect();
    ui.set(
        "timeline-labels",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(labels))),
    )?;
    let summary = |sd: i32, si: i32, ld: i32, li: i32| {
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
    };
    // The real built-in presets (ocy-app src/presets.rs, 0.2.1).
    ui.text("preset-gentle-summary", &summary(15, 40, 5, 90))?;
    ui.text("preset-balanced-summary", &summary(15, 25, 5, 60))?;
    ui.text("preset-focused-summary", &summary(20, 50, 10, 90))?;
    ui.text(
        "about-name-version",
        &t.fmt(
            "about.version",
            &[("name", "Ocy".into()), ("version", "0.2.1".into())],
        ),
    )?;
    let languages: Vec<Value> = [
        t.get("settings.v2.language.automatic"),
        "English".into(),
        "Deutsch".into(),
        "日本語".into(),
    ]
    .into_iter()
    .map(|l| Value::String(l.into()))
    .collect();
    ui.set(
        "language-model",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(languages))),
    )?;
    ui.num("language-index", 0.0)?;
    ui.set("start-with-system", true)?;
    ui.num("preset", 1.0)?;
    ui.num("short-interval", 25.0)?;
    ui.num("short-duration", 15.0)?;
    ui.num("long-interval", 60.0)?;
    ui.num("long-duration", 5.0)?;
    let callbacks: [(&str, Formatter); 5] = [
        ("format-every-minutes", |t, v| {
            t.fmt(
                "settings.v2.value.everyMinutes",
                &[("minutes", v.to_string())],
            )
        }),
        ("format-minutes", |t, v| {
            t.fmt("settings.v2.value.minutes", &[("minutes", v.to_string())])
        }),
        ("format-seconds", |t, v| {
            t.fmt("settings.v2.value.seconds", &[("seconds", v.to_string())])
        }),
        ("format-times", |t, v| {
            let form = if v == 1 { "one" } else { "other" };
            t.fmt(
                &format!("settings.v2.value.times.{form}"),
                &[("count", v.to_string())],
            )
        }),
        ("format-duration", |t, v| {
            if v % 60 == 0 {
                t.fmt(
                    "settings.v2.value.minutes",
                    &[("minutes", (v / 60).to_string())],
                )
            } else {
                t.fmt("settings.v2.value.seconds", &[("seconds", v.to_string())])
            }
        }),
    ];
    for (name, f) in callbacks {
        let tt = Rc::clone(&t);
        ui.0.set_callback(name, move |args| {
            let v = match args.first() {
                Some(Value::Number(n)) => *n as i64,
                _ => 0,
            };
            Value::String(f(&tt, v).into())
        })
        .map_err(|e| format!("{name}: {e}"))?;
    }
    ui.set("customize-open", true)?;
    // General collapsed so Settings fits the committed 560 x 640 frame.
    ui.set("general-open", false)?;
    let Some(games) = games else {
        ui.num("customize-tab", 0.0)?;
        return Ok(render(&window, 560.0, || 640.0));
    };
    // Gaming review: the Quiet times tab with the Games group; the window grows downwards.
    ui.num("customize-tab", 2.0)?;
    ui.text("games-title", &t.get("settings.section.games"))?;
    let modes: Vec<Value> = [t.get("settings.games.gentle"), t.get("settings.games.hold")]
        .into_iter()
        .map(|l| Value::String(l.into()))
        .collect();
    ui.set(
        "games-mode-model",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(modes))),
    )?;
    ui.num("games-mode-index", 0.0)?;
    ui.text("games-gentle-help", &t.get("settings.games.gentleHelp"))?;
    ui.text("games-hold-help", &t.get("settings.games.holdHelp"))?;
    ui.text("games-silent-title", &t.get("settings.games.silent"))?;
    ui.text("games-silent-empty", &t.get("settings.games.silentEmpty"))?;
    let rows: Vec<Value> = games
        .iter()
        .map(|name| {
            let mut row = slint_interpreter::Struct::default();
            row.set_field("name".into(), Value::String((*name).into()));
            row.set_field(
                "remove-label".into(),
                Value::String(
                    t.fmt("settings.games.remove", &[("name", (*name).into())])
                        .into(),
                ),
            );
            Value::Struct(row)
        })
        .collect();
    ui.set(
        "games-silent",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(rows))),
    )?;
    Ok(render(&window, 560.0, || ui.height().max(640.0)))
}
