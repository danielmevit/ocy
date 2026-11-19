use std::time::Duration;

use damt_platform::{FocusState, PlatformServices, Unsupported};

#[test]
fn unsupported_platform_services_reports_unsupported_and_safe_defaults() {
    let services: &dyn PlatformServices = &Unsupported;

    assert!(!services.idle().is_supported());
    assert_eq!(services.idle().idle_time(), Duration::ZERO);

    assert!(!services.focus().is_supported());
    assert_eq!(services.focus().state(), FocusState::Unknown);

    assert!(!services.autostart().is_supported());
    assert!(!services.autostart().is_enabled());
    assert!(services.autostart().set_enabled(true).is_ok());

    assert!(!services.active_screen().is_supported());
    assert_eq!(services.active_screen().active_screen_bounds(), None);

    assert!(!services.motion().is_supported());
    assert!(!services.motion().prefers_reduced_motion());

    assert!(!services.taskbar_theme().is_supported());
    assert_eq!(services.taskbar_theme().is_light_taskbar(), None);
}
