//! CR3 review renders: the crash-report card (one report, several reports ticked, the preview, a
//! long-label stress case) and the Settings crash-reports row, in en, de and ja, light and dark,
//! to `<out_dir>/<theme>/<name>.png` at 2x.
//!
//! `cargo run -p ocy-ui --example crash_screens -- <out_dir>`
#![deny(clippy::unwrap_used, clippy::expect_used)]

#[path = "../tests/support/mod.rs"]
mod support;

use support::{
    CrashCardState, crash_card_shot, crash_settings_shot, crash_settings_shot_with, init_platform,
    write_png,
};

/// One render job: theme folder, file name, and the render.
type Job = (
    &'static str,
    String,
    Box<dyn Fn() -> Result<support::Rgba, String>>,
);

fn main() -> std::process::ExitCode {
    let Some(out_dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: crash_screens <out_dir>");
        return std::process::ExitCode::from(2);
    };
    let out_dir = std::path::PathBuf::from(out_dir);
    init_platform();
    let mut jobs: Vec<Job> = Vec::new();
    for (theme, dark) in [("light", false), ("dark", true)] {
        for lang in ["en", "de", "ja"] {
            for (name, state) in [
                ("card", CrashCardState::One),
                ("card-several-ticked", CrashCardState::SeveralTicked),
                ("preview", CrashCardState::Preview),
            ] {
                jobs.push((
                    theme,
                    format!("{lang}-{name}"),
                    Box::new(move || crash_card_shot(lang, dark, state)),
                ));
            }
            jobs.push((
                theme,
                format!("{lang}-settings-row"),
                Box::new(move || crash_settings_shot(lang, dark, 560.0)),
            ));
        }
        jobs.push((
            theme,
            "long-labels-card".to_owned(),
            Box::new(move || crash_card_shot("en", dark, CrashCardState::LongLabels)),
        ));
        jobs.push((
            theme,
            "long-labels-preview".to_owned(),
            Box::new(move || crash_card_shot("en", dark, CrashCardState::PreviewLongLabels)),
        ));
        jobs.push((
            theme,
            "long-labels-settings-row-min-width".to_owned(),
            Box::new(move || crash_settings_shot_with("en", dark, 480.0, true)),
        ));
        jobs.push((
            theme,
            "de-settings-row-min-width".to_owned(),
            Box::new(move || crash_settings_shot("de", dark, 480.0)),
        ));
    }
    for (theme, name, job) in jobs {
        let path = out_dir.join(theme).join(format!("{name}.png"));
        match job().and_then(|img| write_png(&path, &img)) {
            Ok(()) => println!("{}", path.display()),
            Err(error) => {
                eprintln!("crash_screens failed: {error}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    std::process::ExitCode::SUCCESS
}
