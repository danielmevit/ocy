//! Headless gallery: renders every Ocy surface in light and dark at 2x to `<out_dir>/<theme>/<name>.png`.
//!
//! `cargo run -p ocy-ui --example gallery -- <out_dir>`
#![deny(clippy::unwrap_used, clippy::expect_used)]

#[path = "../tests/support/mod.rs"]
mod support;

fn main() -> std::process::ExitCode {
    let Some(out_dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: gallery <out_dir>");
        return std::process::ExitCode::from(2);
    };
    match support::write_all(std::path::Path::new(&out_dir)) {
        Ok(paths) => {
            for path in paths {
                println!("{}", path.display());
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("gallery failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
