use damt_overlay::{MonitorId, MonitorInfo};
use damt_platform::ScreenRect;
use ocy_app::layout::{DipSize, OverlayLayout, PixelPoint, ScreenInfo, select_active_screen};

#[test]
fn compute_single_screen_returns_exact_dip_size_and_position() {
    let screens = [ScreenInfo::new(
        ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        1.0,
        true,
        None,
    )];

    let layouts = OverlayLayout::compute(&screens);

    assert_eq!(layouts.len(), 1);
    assert_eq!(layouts[0].position, PixelPoint::new(0, 0));
    assert_eq!(layouts[0].dip_size, DipSize::new(1920.0, 1080.0));
    assert!(layouts[0].is_primary);
}

#[test]
fn compute_mixed_dpi_computes_correct_dip_sizes() {
    let screens = [
        ScreenInfo::new(
            ScreenRect {
                x: 0,
                y: 0,
                width: 3840,
                height: 2160,
            },
            1.5,
            true,
            None,
        ),
        ScreenInfo::new(
            ScreenRect {
                x: 3840,
                y: 0,
                width: 1920,
                height: 1080,
            },
            1.0,
            false,
            None,
        ),
    ];

    let layouts = OverlayLayout::compute(&screens);

    assert_eq!(layouts.len(), 2);

    assert_eq!(layouts[0].position, PixelPoint::new(0, 0));
    assert_eq!(layouts[0].dip_size, DipSize::new(2560.0, 1440.0));
    assert!(layouts[0].is_primary);

    assert_eq!(layouts[1].position, PixelPoint::new(3840, 0));
    assert_eq!(layouts[1].dip_size, DipSize::new(1920.0, 1080.0));
    assert!(!layouts[1].is_primary);
}

#[test]
fn compute_negative_coordinates_preserves_negative_position() {
    let screens = [
        ScreenInfo::new(
            ScreenRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            1.0,
            false,
            None,
        ),
        ScreenInfo::new(
            ScreenRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            1.0,
            true,
            None,
        ),
    ];

    let layouts = OverlayLayout::compute(&screens);

    assert_eq!(layouts.len(), 2);
    assert_eq!(layouts[0].position, PixelPoint::new(-1920, 0));
    assert_eq!(layouts[0].dip_size, DipSize::new(1920.0, 1080.0));
    assert!(!layouts[0].is_primary);

    assert_eq!(layouts[1].position, PixelPoint::new(0, 0));
    assert_eq!(layouts[1].dip_size, DipSize::new(1920.0, 1080.0));
    assert!(layouts[1].is_primary);
}

#[test]
fn compute_corner_position_standard_dpi_calculates_bottom_right_with_margin() {
    let screen = ScreenInfo::new(
        ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        1.0,
        true,
        Some(ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        }),
    );

    let pos = OverlayLayout::compute_corner_position(&screen, DipSize::new(360.0, 140.0), 16.0);
    assert_eq!(pos, PixelPoint::new(1544, 884));
}

#[test]
fn compute_corner_position_high_dpi_and_offset_calculates_physical_pixels_accurately() {
    let screen = ScreenInfo::new(
        ScreenRect {
            x: 1920,
            y: 0,
            width: 3840,
            height: 2160,
        },
        1.5,
        true,
        Some(ScreenRect {
            x: 1920,
            y: 0,
            width: 3840,
            height: 2100,
        }),
    );

    let pos = OverlayLayout::compute_corner_position(&screen, DipSize::new(360.0, 140.0), 16.0);
    assert_eq!(pos, PixelPoint::new(5196, 1866));
}

#[test]
fn compute_dip_size_standard_dpi_returns_exact_size() {
    assert_eq!(
        OverlayLayout::compute_dip_size(1920, 1080, 1.0),
        DipSize::new(1920.0, 1080.0)
    );
}

#[test]
fn compute_dip_size_high_dpi_1_point_5_computes_correct_dip() {
    assert_eq!(
        OverlayLayout::compute_dip_size(3840, 2160, 1.5),
        DipSize::new(2560.0, 1440.0)
    );
}

#[test]
fn frame_geometry_scaling_1_scaling_1_point_5_negative_x_computes_accurate_bounds() {
    let screens = [
        ScreenInfo::from_xywh(-3840, 0, 1920, 1200, 1.0, false),
        ScreenInfo::from_xywh(-1920, 0, 1920, 1200, 1.5, false),
        ScreenInfo::from_xywh(0, 0, 1920, 1080, 1.0, true),
    ];

    let layouts = OverlayLayout::compute(&screens);
    assert_eq!(layouts.len(), 3);

    assert_eq!(layouts[0].position, PixelPoint::new(-3840, 0));
    assert_eq!(layouts[0].dip_size, DipSize::new(1920.0, 1200.0));
    assert!(!layouts[0].is_primary);

    assert_eq!(layouts[1].position, PixelPoint::new(-1920, 0));
    assert_eq!(layouts[1].dip_size, DipSize::new(1280.0, 800.0));
    assert!(!layouts[1].is_primary);

    assert_eq!(layouts[2].position, PixelPoint::new(0, 0));
    assert_eq!(layouts[2].dip_size, DipSize::new(1920.0, 1080.0));
    assert!(layouts[2].is_primary);
}

#[test]
fn compute_centered_work_area_position_handles_dpi_100_125_150_and_negative_origins() {
    let settings_dip = DipSize::new(560.0, 640.0);

    // 100% DPI (96), primary monitor with 40 px bottom taskbar.
    let screen_100 = ScreenInfo::new(
        ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        1.0,
        true,
        Some(ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        }),
    );
    assert_eq!(
        OverlayLayout::compute_centered_work_area_position(&screen_100, settings_dip),
        PixelPoint::new(680, 200)
    );

    // 125% DPI (120) on negative-origin monitor with 60 px left taskbar and 48 px top bar.
    // 560 * 1.25 = 700, 640 * 1.25 = 800.
    let screen_125 = ScreenInfo::new(
        ScreenRect {
            x: -2560,
            y: -120,
            width: 2560,
            height: 1440,
        },
        1.25,
        false,
        Some(ScreenRect {
            x: -2500,
            y: -72,
            width: 2500,
            height: 1392,
        }),
    );
    assert_eq!(
        OverlayLayout::compute_centered_work_area_position(&screen_125, settings_dip),
        PixelPoint::new(-2500 + (2500 - 700) / 2, -72 + (1392 - 800) / 2)
    );

    // 150% DPI (144) on 4K secondary monitor at +1920 with 60 px bottom taskbar.
    // 560 * 1.5 = 840, 640 * 1.5 = 960.
    let screen_150 = ScreenInfo::new(
        ScreenRect {
            x: 1920,
            y: 0,
            width: 3840,
            height: 2160,
        },
        1.5,
        false,
        Some(ScreenRect {
            x: 1920,
            y: 0,
            width: 3840,
            height: 2100,
        }),
    );
    assert_eq!(
        OverlayLayout::compute_centered_work_area_position(&screen_150, settings_dip),
        PixelPoint::new(1920 + (3840 - 840) / 2, (2100 - 960) / 2)
    );
}

#[test]
fn compute_centered_work_area_selects_active_monitor_and_clamps_small_work_area() {
    let monitors = [
        MonitorInfo {
            id: MonitorId::new(1),
            rect: ScreenRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            work_area: ScreenRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
            dpi: 96,
            primary: true,
        },
        MonitorInfo {
            id: MonitorId::new(2),
            rect: ScreenRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1200,
            },
            work_area: ScreenRect {
                x: -1920,
                y: 48,
                width: 1920,
                height: 1152,
            },
            dpi: 120,
            primary: false,
        },
    ];

    let active_window_on_left = ScreenRect {
        x: -1500,
        y: 200,
        width: 900,
        height: 700,
    };
    let selected = select_active_screen(&monitors, Some(active_window_on_left)).unwrap();
    let pos =
        OverlayLayout::compute_centered_work_area_position(&selected, DipSize::new(560.0, 640.0));
    assert_eq!(
        pos,
        PixelPoint::new(-1920 + (1920 - 700) / 2, 48 + (1152 - 800) / 2)
    );

    // When outer window size exceeds work area height (e.g. tiny display), clamp at work area origin.
    let small_work = ScreenRect {
        x: -800,
        y: 30,
        width: 800,
        height: 600,
    };
    assert_eq!(
        OverlayLayout::compute_centered_rect_position(small_work, 840, 960),
        PixelPoint::new(-800, 30)
    );
}
