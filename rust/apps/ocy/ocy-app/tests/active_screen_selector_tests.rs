use damt_platform::ScreenRect;
use ocy_app::layout::{ActiveScreenSelector, ScreenInfo};

fn real_layout() -> [ScreenInfo; 3] {
    [
        ScreenInfo::from_xywh(0, 0, 1920, 1080, 1.0, true),
        ScreenInfo::from_xywh(-3840, 0, 1920, 1200, 1.0, false),
        ScreenInfo::from_xywh(-1920, 0, 1920, 1200, 1.0, false),
    ]
}

#[test]
fn select_when_active_is_null_returns_primary_screen() {
    let layout = real_layout();
    let index = ActiveScreenSelector::select(&layout, None);
    assert_eq!(index, 0);
}

#[test]
fn select_exact_match_secondary_screen_1() {
    let layout = real_layout();
    let active = ScreenRect {
        x: -3840,
        y: 0,
        width: 1920,
        height: 1200,
    };
    assert_eq!(ActiveScreenSelector::select(&layout, Some(active)), 1);
}

#[test]
fn select_exact_match_secondary_screen_2() {
    let layout = real_layout();
    let active = ScreenRect {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1200,
    };
    assert_eq!(ActiveScreenSelector::select(&layout, Some(active)), 2);
}

#[test]
fn select_exact_match_primary_screen() {
    let layout = real_layout();
    let active = ScreenRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    assert_eq!(ActiveScreenSelector::select(&layout, Some(active)), 0);
}

#[test]
fn select_center_match_window_on_secondary_screen_1() {
    let layout = real_layout();
    let active_window = ScreenRect {
        x: -3000,
        y: 100,
        width: 800,
        height: 600,
    };
    assert_eq!(
        ActiveScreenSelector::select(&layout, Some(active_window)),
        1
    );
}

#[test]
fn select_center_match_window_on_secondary_screen_2() {
    let layout = real_layout();
    let active_window = ScreenRect {
        x: -1000,
        y: 100,
        width: 800,
        height: 600,
    };
    assert_eq!(
        ActiveScreenSelector::select(&layout, Some(active_window)),
        2
    );
}

#[test]
fn select_center_match_window_on_primary_screen() {
    let layout = real_layout();
    let active_window = ScreenRect {
        x: 100,
        y: 100,
        width: 800,
        height: 600,
    };
    assert_eq!(
        ActiveScreenSelector::select(&layout, Some(active_window)),
        0
    );
}

#[test]
fn select_when_center_is_off_screen_falls_back_to_primary_screen() {
    let layout = real_layout();
    let offscreen = ScreenRect {
        x: 9999,
        y: 9999,
        width: 500,
        height: 500,
    };
    assert_eq!(ActiveScreenSelector::select(&layout, Some(offscreen)), 0);
}

#[test]
fn select_when_no_primary_marked_falls_back_to_first_screen() {
    let screens = [
        ScreenInfo::from_xywh(1920, 0, 1920, 1080, 1.0, false),
        ScreenInfo::from_xywh(3840, 0, 1920, 1080, 1.0, false),
    ];
    assert_eq!(ActiveScreenSelector::select(&screens, None), 0);
}

#[test]
fn select_empty_screens_returns_zero() {
    assert_eq!(ActiveScreenSelector::select(&[], None), 0);
}
