//! Pure-logic tests for `damt-platform-mac` (no Apple framework required; these run on
//! Linux and in the macOS CI job).

use std::time::Duration;

use damt_platform::{FocusState, ScreenRect, SessionEvent};
use damt_platform_mac::pure::{
    DisplayGeometry, IdleMath, K_CG_ANY_INPUT_EVENT_TYPE, SCREEN_IS_LOCKED_NOTIFICATION,
    SCREEN_IS_UNLOCKED_NOTIFICATION, WORKSPACE_DID_WAKE_NOTIFICATION,
    WORKSPACE_WILL_SLEEP_NOTIFICATION, frontmost_observation, map_distributed_notification,
    map_workspace_notification, parse_apple_interface_style, parse_reduce_motion,
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
