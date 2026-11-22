//! Pure-logic tests for `damt-platform-mac` (no Apple framework required; these run on
//! Linux and in the macOS CI job).

use std::time::Duration;

use damt_platform::{FocusState, ScreenRect, SessionEvent};
use damt_platform_mac::pure::{
    AppKitRect, BREAK_CARD_WINDOW_LEVEL, DisplayGeometry, IdleMath, K_CG_ANY_INPUT_EVENT_TYPE,
    MacOsVersion, MacTrayAction, MacTrayIconState, MacTrayMenuItem, MacTrayMenuSlot,
    MacTrayPresentation, MacTrayRenderedEntry, MacWindowRole, NS_FLOATING_WINDOW_LEVEL,
    NS_NORMAL_WINDOW_LEVEL, NS_SCREEN_SAVER_WINDOW_LEVEL, SCREEN_IS_LOCKED_NOTIFICATION,
    SCREEN_IS_UNLOCKED_NOTIFICATION, SmAppServiceState, WORKSPACE_DID_WAKE_NOTIFICATION,
    WORKSPACE_WILL_SLEEP_NOTIFICATION, action_for_tag, appkit_rect_to_cg_physical,
    decode_png_to_rgba, frontmost_observation, is_sm_app_service_supported,
    map_distributed_notification, map_sm_app_service_status, map_workspace_notification,
    parse_apple_interface_style, parse_reduce_motion, plan_tray_menu_entries,
    rgba_to_template_mask, scale_to_dpi, select_screen_containing_point, tag_for_action,
    template_tray_icon_png, window_level_for_role,
};

// --- Idle conversion (CGEventSourceSecondsSinceLastEventType) ---

#[test]
fn idle_constants_match_apple_documented_values() {
    // kCGAnyInputEventType is `(CGEventType)~0`; the objc2 bindings skip it.
    assert_eq!(K_CG_ANY_INPUT_EVENT_TYPE, u32::MAX);
}

#[test]
fn idle_seconds_convert_to_duration() {
    assert_eq!(IdleMath::from_seconds(0.0), Duration::ZERO);
    assert_eq!(IdleMath::from_seconds(1.5), Duration::from_millis(1500));
    assert_eq!(IdleMath::from_seconds(90.0), Duration::from_secs(90));
}

#[test]
fn idle_seconds_negative_or_nan_become_zero() {
    // -1.0 is documented as "no event of the given type has been received".
    assert_eq!(IdleMath::from_seconds(-1.0), Duration::ZERO);
    assert_eq!(IdleMath::from_seconds(f64::NAN), Duration::ZERO);
    assert_eq!(IdleMath::from_seconds(f64::NEG_INFINITY), Duration::ZERO);
}

#[test]
fn idle_seconds_saturate_instead_of_panicking() {
    assert_eq!(IdleMath::from_seconds(f64::INFINITY), Duration::MAX);
    assert_eq!(IdleMath::from_seconds(1.0e30), Duration::MAX);
}

// --- Theme parsing (AppleInterfaceStyle) ---

#[test]
fn interface_style_absent_key_means_light() {
    assert_eq!(parse_apple_interface_style(None), Some(true));
}

#[test]
fn interface_style_dark_maps_to_dark() {
    assert_eq!(parse_apple_interface_style(Some("Dark")), Some(false));
    assert_eq!(parse_apple_interface_style(Some("  Dark  ")), Some(false));
}

#[test]
fn interface_style_light_maps_to_light() {
    assert_eq!(parse_apple_interface_style(Some("Light")), Some(true));
}

#[test]
fn interface_style_unknown_value_is_unknown_not_guessed() {
    assert_eq!(parse_apple_interface_style(Some("")), None);
    assert_eq!(parse_apple_interface_style(Some("HighContrast")), None);
}

// --- Reduce motion parsing ---

#[test]
fn reduce_motion_only_true_reduces_motion() {
    assert!(parse_reduce_motion(Some(true)));
    assert!(!parse_reduce_motion(Some(false)));
    assert!(!parse_reduce_motion(None));
}

// --- Session notification mapping ---

#[test]
fn distributed_notifications_map_to_lock_and_unlock() {
    assert_eq!(
        map_distributed_notification(SCREEN_IS_LOCKED_NOTIFICATION),
        Some(SessionEvent::Locked)
    );
    assert_eq!(
        map_distributed_notification(SCREEN_IS_UNLOCKED_NOTIFICATION),
        Some(SessionEvent::Unlocked)
    );
}

#[test]
fn unrelated_notifications_do_not_map() {
    assert_eq!(
        map_distributed_notification("com.apple.screenSaverStarted"),
        None
    );
    assert_eq!(map_distributed_notification(""), None);
}

#[test]
fn workspace_notifications_map_to_sleep_and_wake() {
    assert_eq!(
        map_workspace_notification(WORKSPACE_WILL_SLEEP_NOTIFICATION),
        Some(SessionEvent::Suspending)
    );
    assert_eq!(
        map_workspace_notification(WORKSPACE_DID_WAKE_NOTIFICATION),
        Some(SessionEvent::Resumed)
    );
    assert_eq!(
        map_workspace_notification("NSWorkspaceDidWakeNotificationX"),
        None
    );
}

// --- Display scale maths ---

#[test]
fn scale_factor_is_one_on_non_retina() {
    let geometry = DisplayGeometry {
        origin_x: 0.0,
        origin_y: 0.0,
        width_points: 1920.0,
        height_points: 1080.0,
        pixel_width: 1920,
        pixel_height: 1080,
    };
    assert_eq!(geometry.scale_factor(), Some(1.0));
    assert_eq!(
        geometry.physical_rect(),
        Some(ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080
        })
    );
}

#[test]
fn scale_factor_is_two_on_retina() {
    let geometry = DisplayGeometry {
        origin_x: 0.0,
        origin_y: 0.0,
        width_points: 1512.0,
        height_points: 982.0,
        pixel_width: 3024,
        pixel_height: 1964,
    };
    assert_eq!(geometry.scale_factor(), Some(2.0));
    assert_eq!(
        geometry.physical_rect(),
        Some(ScreenRect {
            x: 0,
            y: 0,
            width: 3024,
            height: 1964
        })
    );
}

#[test]
fn scaled_display_keeps_negative_origin_and_fractional_scale() {
    // A 2560x1440 monitor scaled to "looks like 1920x1080" (scale 1.333...),
    // placed left of the main display (negative x origin).
    let geometry = DisplayGeometry {
        origin_x: -1920.0,
        origin_y: 0.0,
        width_points: 1920.0,
        height_points: 1080.0,
        pixel_width: 2560,
        pixel_height: 1440,
    };
    assert!((geometry.scale_factor().unwrap_or(0.0) - (2560.0 / 1920.0)).abs() < 1.0e-9);
    assert_eq!(
        geometry.physical_rect(),
        Some(ScreenRect {
            x: -2560,
            y: 0,
            width: 2560,
            height: 1440
        })
    );
}

#[test]
fn rounding_matches_the_nearest_pixel() {
    // Width: 1439.5 points at exactly 2x (2879 px). Height: 900.25 points on a
    // 1801 px panel (scale 2.000555...), so the -0.25 point origin lands at
    // -0.5001 px, which rounds to -1.
    let geometry = DisplayGeometry {
        origin_x: 0.5,
        origin_y: -0.25,
        width_points: 1439.5,
        height_points: 900.25,
        pixel_width: 2879,
        pixel_height: 1801,
    };
    assert_eq!(
        geometry.physical_rect(),
        Some(ScreenRect {
            x: 1,
            y: -1,
            width: 2879,
            height: 1801
        })
    );
}

#[test]
fn degenerate_geometry_is_rejected() {
    let zero_points = DisplayGeometry {
        origin_x: 0.0,
        origin_y: 0.0,
        width_points: 0.0,
        height_points: 0.0,
        pixel_width: 1920,
        pixel_height: 1080,
    };
    assert_eq!(zero_points.scale_factor(), None);
    assert_eq!(zero_points.physical_rect(), None);

    let zero_pixels = DisplayGeometry {
        origin_x: 0.0,
        origin_y: 0.0,
        width_points: 1920.0,
        height_points: 1080.0,
        pixel_width: 0,
        pixel_height: 0,
    };
    assert_eq!(zero_pixels.scale_factor(), None);
    assert_eq!(zero_pixels.physical_rect(), None);

    let nan_origin = DisplayGeometry {
        origin_x: f64::NAN,
        origin_y: 0.0,
        width_points: 1920.0,
        height_points: 1080.0,
        pixel_width: 1920,
        pixel_height: 1080,
    };
    assert_eq!(nan_origin.physical_rect(), None);
}

// --- Frontmost-app observation mapping ---

#[test]
fn frontmost_observation_prefers_bundle_id() {
    let observation = frontmost_observation(Some("com.apple.Safari"), Some("Safari"), Some(42));
    assert_eq!(observation.exe_name.as_deref(), Some("com.apple.Safari"));
    assert_eq!(observation.exe_path, None);
    assert_eq!(observation.process_id, Some(42));
    assert_eq!(observation.notification_state, None);
}

#[test]
fn frontmost_observation_falls_back_to_app_name() {
    let observation = frontmost_observation(None, Some("Safari"), Some(7));
    assert_eq!(observation.exe_name.as_deref(), Some("Safari"));
    assert_eq!(observation.process_id, Some(7));
}

#[test]
fn frontmost_observation_trims_and_skips_empty_names() {
    let observation = frontmost_observation(Some("   "), Some("  Safari  "), Some(1));
    assert_eq!(observation.exe_name.as_deref(), Some("Safari"));

    let empty = frontmost_observation(Some(""), Some(""), None);
    assert_eq!(empty.exe_name, None);
    assert_eq!(empty.process_id, None);
}

#[test]
fn frontmost_observation_rejects_negative_pids() {
    let observation = frontmost_observation(Some("com.apple.Safari"), None, Some(-1));
    assert_eq!(observation.process_id, None);
    assert!(observation.exe_name.is_some());
}

// --- Contract sanity (keeps the crate honest against damt-platform) ---

#[test]
fn focus_state_unknown_is_the_default() {
    // M1 reports full-screen state as Unknown on macOS; make sure the contract agrees.
    assert_eq!(FocusState::default(), FocusState::Unknown);
}

// --- M2: AppKit -> CG placement maths ---

#[test]
fn appkit_rect_flips_y_and_scales_to_physical_pixels() {
    // 1512x982 Retina (2x) main screen; visibleFrame starts above 70pt Dock and stops
    // below 38pt menu bar: (0, 70, 1512, 874).
    let visible = AppKitRect {
        x: 0.0,
        y: 70.0,
        width: 1512.0,
        height: 874.0,
    };
    assert_eq!(
        appkit_rect_to_cg_physical(visible, 982.0, 2.0),
        Some(ScreenRect {
            x: 0,
            y: 76,
            width: 3024,
            height: 1748,
        })
    );
}

#[test]
fn appkit_rect_handles_secondary_display_above_or_left_of_main() {
    // Secondary 2560x1440 (1x) display placed to the left (-2560) and aligned at bottom (-360 in AppKit).
    // Main screen height = 1080. Top in CG = 1080 - (-360 + 1440) = 0.
    let secondary = AppKitRect {
        x: -2560.0,
        y: -360.0,
        width: 2560.0,
        height: 1440.0,
    };
    assert_eq!(
        appkit_rect_to_cg_physical(secondary, 1080.0, 1.0),
        Some(ScreenRect {
            x: -2560,
            y: 0,
            width: 2560,
            height: 1440,
        })
    );
}

#[test]
fn appkit_rect_rejects_degenerate_inputs() {
    let invalid = AppKitRect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 100.0,
    };
    assert_eq!(appkit_rect_to_cg_physical(invalid, 1000.0, 2.0), None);
    let valid = AppKitRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    assert_eq!(appkit_rect_to_cg_physical(valid, 0.0, 2.0), None);
    assert_eq!(appkit_rect_to_cg_physical(valid, 1000.0, 0.0), None);
}

#[test]
fn scale_to_dpi_maps_standard_scales_and_guards_invalid() {
    assert_eq!(scale_to_dpi(1.0), 96);
    assert_eq!(scale_to_dpi(2.0), 192);
    assert_eq!(scale_to_dpi(1.5), 144);
    assert_eq!(scale_to_dpi(0.0), 96);
    assert_eq!(scale_to_dpi(f64::NAN), 96);
}

#[test]
fn select_screen_containing_point_finds_matching_monitor() {
    let frames = [
        AppKitRect {
            x: 0.0,
            y: 0.0,
            width: 1512.0,
            height: 982.0,
        },
        AppKitRect {
            x: 1512.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        },
    ];
    assert_eq!(
        select_screen_containing_point(&frames, 100.0, 200.0),
        Some(0)
    );
    assert_eq!(
        select_screen_containing_point(&frames, 2000.0, 1200.0),
        Some(1)
    );
    assert_eq!(select_screen_containing_point(&frames, -10.0, 0.0), None);
    assert_eq!(select_screen_containing_point(&frames, f64::NAN, 0.0), None);
}

// --- M2: SMAppService version & status mapping ---

#[test]
fn sm_app_service_requires_macos_13_and_runtime_class() {
    let macos_12 = MacOsVersion {
        major: 12,
        minor: 7,
        patch: 6,
    };
    let macos_13 = MacOsVersion {
        major: 13,
        minor: 0,
        patch: 0,
    };
    let macos_15 = MacOsVersion {
        major: 15,
        minor: 2,
        patch: 0,
    };
    assert!(!is_sm_app_service_supported(macos_12, true));
    assert!(!is_sm_app_service_supported(macos_12, false));
    assert!(!is_sm_app_service_supported(macos_13, false));
    assert!(is_sm_app_service_supported(macos_13, true));
    assert!(is_sm_app_service_supported(macos_15, true));
}

#[test]
fn sm_app_service_status_maps_all_documented_codes() {
    assert_eq!(
        map_sm_app_service_status(0),
        SmAppServiceState::NotRegistered
    );
    assert!(!map_sm_app_service_status(0).is_enabled());

    assert_eq!(map_sm_app_service_status(1), SmAppServiceState::Enabled);
    assert!(map_sm_app_service_status(1).is_enabled());

    assert_eq!(
        map_sm_app_service_status(2),
        SmAppServiceState::RequiresApproval
    );
    assert!(!map_sm_app_service_status(2).is_enabled());

    assert_eq!(map_sm_app_service_status(3), SmAppServiceState::NotFound);
    assert!(!map_sm_app_service_status(3).is_enabled());

    assert_eq!(
        map_sm_app_service_status(99),
        SmAppServiceState::Unknown(99)
    );
    assert!(!map_sm_app_service_status(99).is_enabled());
}

// --- M2: Template tray icon derivation ---

#[test]
fn rgba_to_template_mask_zeros_rgb_and_preserves_alpha() {
    let input = [19, 87, 62, 200, 140, 54, 24, 0, 255, 255, 255, 255];
    let masked = rgba_to_template_mask(&input);
    assert_eq!(masked, vec![0, 0, 0, 200, 0, 0, 0, 0, 0, 0, 0, 255]);
}

#[test]
fn template_tray_icons_decode_as_32x32_black_alpha_pngs_for_every_state() {
    for state in [
        MacTrayIconState::Active,
        MacTrayIconState::Paused,
        MacTrayIconState::OnBreak,
        MacTrayIconState::Attention,
    ] {
        let png_bytes = template_tray_icon_png(state).expect("valid template png");
        let (w, h, rgba) = decode_png_to_rgba(&png_bytes).expect("round-trip decode");
        assert_eq!((w, h), (32, 32));
        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert!(rgba.chunks_exact(4).any(|px| px[3] > 0));
        assert!(
            rgba.chunks_exact(4)
                .all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0)
        );
    }
}

// --- M2: Tray menu mapping & action tags ---

#[test]
fn tray_menu_entries_preserve_order_separators_and_hold_visibility() {
    let item = |text: &str, visible: bool, enabled: bool, checked: bool| MacTrayMenuItem {
        text: text.to_owned(),
        is_visible: visible,
        is_enabled: enabled,
        is_checked: checked,
    };
    let presentation = MacTrayPresentation {
        tooltip: "Ocy".to_owned(),
        pause_status: item("Paused", true, true, false),
        take_short_break_now: item("Short", true, true, false),
        take_long_break_now: item("Long", true, false, false),
        hold_until_game_closes: item("Hold", true, false, false),
        pause_1_hour: item("1h", true, true, true),
        pause_3_hours: item("3h", true, true, false),
        pause_until_resumed: item("Until", true, true, false),
        resume: item("Resume", true, true, false),
        settings: item("Settings", true, true, false),
        quit: item("Quit", true, true, false),
    };

    let entries = plan_tray_menu_entries(&presentation);
    assert_eq!(entries.len(), 12);
    assert!(matches!(entries[4], MacTrayRenderedEntry::Separator));
    assert!(matches!(entries[9], MacTrayRenderedEntry::Separator));

    // PauseStatus stays non-clickable and disabled.
    match &entries[0] {
        MacTrayRenderedEntry::Item { slot, action, item } => {
            assert_eq!(*slot, MacTrayMenuSlot::PauseStatus);
            assert_eq!(*action, None);
            assert!(!item.is_enabled);
        }
        MacTrayRenderedEntry::Separator => panic!("expected PauseStatus"),
    }

    // HoldUntilGameCloses hides when not enabled (matching SlintTraySink).
    match &entries[3] {
        MacTrayRenderedEntry::Item { slot, action, item } => {
            assert_eq!(*slot, MacTrayMenuSlot::HoldUntilGameCloses);
            assert_eq!(*action, Some(MacTrayAction::HoldUntilGameCloses));
            assert!(!item.is_visible);
        }
        MacTrayRenderedEntry::Separator => panic!("expected HoldUntilGameCloses"),
    }

    // Check state preserved.
    match &entries[5] {
        MacTrayRenderedEntry::Item { slot, item, .. } => {
            assert_eq!(*slot, MacTrayMenuSlot::Pause1Hour);
            assert!(item.is_checked);
        }
        MacTrayRenderedEntry::Separator => panic!("expected Pause1Hour"),
    }
}

#[test]
fn tray_action_tags_round_trip_without_collisions() {
    let all = [
        MacTrayAction::TakeShortBreakNow,
        MacTrayAction::TakeLongBreakNow,
        MacTrayAction::HoldUntilGameCloses,
        MacTrayAction::Pause1Hour,
        MacTrayAction::Pause3Hours,
        MacTrayAction::PauseUntilResumed,
        MacTrayAction::Resume,
        MacTrayAction::Settings,
        MacTrayAction::Quit,
    ];
    for action in all {
        assert_eq!(action_for_tag(tag_for_action(action)), Some(action));
    }
    assert_eq!(action_for_tag(0), None);
    assert_eq!(action_for_tag(99), None);
}

// --- M2b: Window level mapping & z-order invariants ---

#[test]
fn window_levels_match_appkit_constants_and_keep_cards_above_dims() {
    assert_eq!(NS_NORMAL_WINDOW_LEVEL, 0);
    assert_eq!(NS_FLOATING_WINDOW_LEVEL, 3);
    assert_eq!(NS_SCREEN_SAVER_WINDOW_LEVEL, 1000);
    assert_eq!(BREAK_CARD_WINDOW_LEVEL, 1001);

    assert_eq!(
        window_level_for_role(MacWindowRole::Normal),
        NS_NORMAL_WINDOW_LEVEL
    );
    assert_eq!(
        window_level_for_role(MacWindowRole::EdgeFrame),
        NS_FLOATING_WINDOW_LEVEL
    );
    assert_eq!(
        window_level_for_role(MacWindowRole::Dim),
        NS_SCREEN_SAVER_WINDOW_LEVEL
    );
    assert_eq!(
        window_level_for_role(MacWindowRole::BreakCard),
        BREAK_CARD_WINDOW_LEVEL
    );
}

#[test]
fn window_level_ordering_keeps_card_above_dim_above_normal_and_edge_frames_below_card() {
    let normal = window_level_for_role(MacWindowRole::Normal);
    let edge_frame = window_level_for_role(MacWindowRole::EdgeFrame);
    let dim = window_level_for_role(MacWindowRole::Dim);
    let card = window_level_for_role(MacWindowRole::BreakCard);

    assert!(card > dim);
    assert!(dim > normal);
    assert!(edge_frame < card);
    assert!(edge_frame > normal);
    assert!(edge_frame < dim);
}
