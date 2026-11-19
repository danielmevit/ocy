//! Cross-platform API checks; the Windows behaviour is smoke-tested by the lead with `overlay_demo`.

#![cfg(not(windows))]

use damt_overlay::{Dims, EdgeFrames, Rgba, enumerate_monitors};

#[test]
fn non_windows_creates_no_windows() {
    assert!(enumerate_monitors().is_empty());

    let frames = EdgeFrames::show(&[], Rgba::rgb(0x2F, 0x7D, 0x62), 3);
    assert!(frames.is_empty());
    assert!(frames.window_info().is_empty());

    let dims = Dims::show(&[], Rgba::new(0, 0, 0, 199));
    assert!(dims.is_empty());
    assert!(dims.window_info().is_empty());
}

#[test]
fn color_round_trips_through_the_public_api() {
    assert_eq!(
        Rgba::from_hex_rgb("#2F7D62", 140),
        Ok(Rgba::new(0x2F, 0x7D, 0x62, 140))
    );
}
