//! AN2 review renders: Settings V2 with the usage stats consent UI (ask card, after "No thanks",
//! after "Share stats") in en, de and ja, light and dark, to `<out_dir>/<theme>/<name>.png` at 2x,
//! plus the German ask and a long-label stress test at the 480 px minimum width.
//!
//! `cargo run -p ocy-ui --example consent_screens -- <out_dir>`
#![deny(clippy::unwrap_used, clippy::expect_used)]

#[path = "../tests/support/mod.rs"]
mod support;

use support::{ConsentState, consent_shot, init_platform, write_png};

fn main() -> std::process::ExitCode {
    let Some(out_dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: consent_screens <out_dir>");
        return std::process::ExitCode::from(2);
    };
    let out_dir = std::path::PathBuf::from(out_dir);
    init_platform();
    let mut shots = Vec::new();
    for (theme, dark) in [("light", false), ("dark", true)] {
        for lang in ["en", "de", "ja"] {
            for (name, state) in [
                ("ask", ConsentState::Ask),
                ("declined", ConsentState::Declined),
                ("granted", ConsentState::Granted),
            ] {
                shots.push((theme, format!("{lang}-{name}"), lang, dark, state, 560.0));
            }
        }
        shots.push((
            theme,
            "de-ask-min-width".to_owned(),
            "de",
            dark,
            ConsentState::Ask,
            480.0,
        ));
        shots.push((
            theme,
            "long-labels-min-width".to_owned(),
            "en",
            dark,
            ConsentState::AskLongLabels,
            480.0,
        ));
    }
    for (theme, name, lang, dark, state, width) in shots {
        let path = out_dir.join(theme).join(format!("{name}.png"));
        match consent_shot(lang, dark, state, width).and_then(|img| write_png(&path, &img)) {
            Ok(()) => println!("{}", path.display()),
            Err(error) => {
                eprintln!("consent_screens failed: {error}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    std::process::ExitCode::SUCCESS
}
