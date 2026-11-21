//! Cross-platform unit tests for the pure helper ports (see `PARITY.md`).

use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use damt_platform::{FocusState, ScreenRect, SessionEvent};
use damt_platform_win::{
    AutostartCommand, ExStyle, FocusStateMapper, IdleMath, MotionMath, RectMath, RegistryValue,
    SessionEventMapper, TaskbarThemeMath, ToastXml, aumid_registry_subkey, notify_icon_path,
};

#[test]
fn to_screen_rect_converts_ltrb() {
    assert_eq!(
        RectMath::to_screen_rect(100, 200, 2020, 1280),
        ScreenRect {
            x: 100,
            y: 200,
            width: 1920,
            height: 1080
        }
    );
}

#[test]
fn to_screen_rect_negative_coordinates_work() {
    assert_eq!(
        RectMath::to_screen_rect(-3840, 0, -1920, 1200),
        ScreenRect {
            x: -3840,
            y: 0,
            width: 1920,
            height: 1200
        }
    );
}

#[test]
fn to_screen_rect_all_negative_works() {
    assert_eq!(
        RectMath::to_screen_rect(-3840, -1200, -1920, 0),
        ScreenRect {
            x: -3840,
            y: -1200,
            width: 1920,
            height: 1200
        }
    );
}

#[test]
fn to_screen_rect_zero_sized_rect_works() {
    assert_eq!(
        RectMath::to_screen_rect(5, 5, 5, 5),
        ScreenRect {
            x: 5,
            y: 5,
            width: 0,
            height: 0
        }
    );
}

#[test]
fn click_through_keeps_existing_bits() {
    const FLAGS: i32 = 0x20 | 0x80 | 0x0008_0000 | 0x0800_0000;
    for current in [0, 0x10, 0x100, i32::MIN] {
        assert_eq!(ExStyle::click_through(current), current | FLAGS);
    }
}

#[test]
fn click_through_all_four_flags_set() {
    let result = ExStyle::click_through(0);
    assert_eq!(result, 0x20 | 0x80 | 0x0008_0000 | 0x0800_0000);
    assert_eq!(result, 0x0808_00A0);
}

#[test]
fn click_through_idempotent() {
    let once = ExStyle::click_through(0x4);
    let twice = ExStyle::click_through(once);
    assert_eq!(once, twice);
}

#[test]
fn reduced_call_failed_returns_false() {
    for value in [0, 1, 7] {
        assert!(!MotionMath::reduced(false, value));
    }
}

#[test]
fn reduced_value_zero_returns_true() {
    assert!(MotionMath::reduced(true, 0));
}

#[test]
fn reduced_value_non_zero_returns_false() {
    for value in [1, 2, 42] {
        assert!(!MotionMath::reduced(true, value));
    }
}

#[test]
fn from_registry_value_one_returns_true() {
    assert_eq!(
        TaskbarThemeMath::from_registry_value(Some(RegistryValue::Dword(1))),
        Some(true)
    );
}

#[test]
fn from_registry_value_zero_returns_false() {
    assert_eq!(
        TaskbarThemeMath::from_registry_value(Some(RegistryValue::Dword(0))),
        Some(false)
    );
}

#[test]
fn from_registry_value_null_returns_null() {
    assert_eq!(TaskbarThemeMath::from_registry_value(None), None);
}

#[test]
fn from_registry_value_other_int_returns_null() {
    for value in [2, u32::MAX, i32::MAX as u32] {
        assert_eq!(
            TaskbarThemeMath::from_registry_value(Some(RegistryValue::Dword(value))),
            None
        );
    }
}

#[test]
fn from_registry_value_other_type_returns_null() {
    assert_eq!(
        TaskbarThemeMath::from_registry_value(Some(RegistryValue::Other)),
        None
    );
}

#[test]
fn compute_now_after_last_input_returns_difference() {
    assert_eq!(IdleMath::compute(12_345, 0), Duration::from_millis(12_345));
}

#[test]
fn compute_same_ticks_returns_zero() {
    assert_eq!(IdleMath::compute(100_000, 100_000), Duration::ZERO);
}

#[test]
fn compute_wraparound_is_safe() {
    let now = 5;
    let last_input = u32::MAX - 100;
    assert_eq!(
        IdleMath::compute(now, last_input),
        Duration::from_millis(106)
    );
}

#[test]
fn compute_max_span_wraparound_is_safe() {
    let last_input = 1;
    assert_eq!(
        IdleMath::compute(0, last_input),
        Duration::from_millis(u64::from(u32::MAX))
    );
}

#[test]
fn map_successful_hresult_maps_each_state() {
    let cases = [
        (1, FocusState::NotPresent),
        (2, FocusState::FullscreenApp),
        (3, FocusState::FullscreenApp),
        (4, FocusState::Presentation),
        (5, FocusState::Normal),
        (6, FocusState::QuietTime),
        (7, FocusState::FullscreenApp),
    ];

    for (quns, expected) in cases {
        assert_eq!(FocusStateMapper::map(0, quns), expected);
    }
}

#[test]
fn map_failed_hresult_returns_unknown() {
    for hresult in [-2_147_467_259, -1] {
        assert_eq!(FocusStateMapper::map(hresult, 5), FocusState::Unknown);
    }
}

#[test]
fn map_unknown_quns_returns_unknown() {
    for quns in [0, 8, 99] {
        assert_eq!(FocusStateMapper::map(0, quns), FocusState::Unknown);
    }
}

#[test]
fn build_quotes_path() {
    assert_eq!(
        AutostartCommand::build("C:\\Program Files\\Ocy\\Ocy.exe").as_deref(),
        Ok("\"C:\\Program Files\\Ocy\\Ocy.exe\" --autostart")
    );
}

#[test]
fn build_simple_path() {
    assert_eq!(
        AutostartCommand::build("C:\\Ocy\\Ocy.exe").as_deref(),
        Ok("\"C:\\Ocy\\Ocy.exe\" --autostart")
    );
}

#[test]
fn build_empty_path_errors() {
    for path in ["", "   "] {
        assert!(AutostartCommand::build(path).is_err());
    }
}

#[test]
fn build_path_with_quote_errors() {
    assert!(AutostartCommand::build("C:\\O\"cy\\Ocy.exe").is_err());
}

#[test]
fn from_session_switch_lock_returns_locked() {
    assert_eq!(
        SessionEventMapper::from_session_switch(7),
        Some(SessionEvent::Locked)
    );
}

#[test]
fn from_session_switch_unlock_returns_unlocked() {
    assert_eq!(
        SessionEventMapper::from_session_switch(8),
        Some(SessionEvent::Unlocked)
    );
}

#[test]
fn from_session_switch_logoff_returns_logging_off() {
    assert_eq!(
        SessionEventMapper::from_session_switch(6),
        Some(SessionEvent::LoggingOff)
    );
}

#[test]
fn from_session_switch_unmapped_reason_returns_none() {
    for reason in [0, 1, 5, 9, 11] {
        assert_eq!(SessionEventMapper::from_session_switch(reason), None);
    }
}

#[test]
fn from_power_mode_resume_returns_resumed() {
    assert_eq!(
        SessionEventMapper::from_power_mode(1),
        Some(SessionEvent::Resumed)
    );
}

#[test]
fn from_power_mode_suspend_returns_suspending() {
    assert_eq!(
        SessionEventMapper::from_power_mode(3),
        Some(SessionEvent::Suspending)
    );
}

#[test]
fn from_power_mode_unmapped_mode_returns_none() {
    for mode in [0, 2, 4] {
        assert_eq!(SessionEventMapper::from_power_mode(mode), None);
    }
}

#[test]
fn toast_xml_escapes_all_five_special_characters() {
    let raw = r#"<tag attr="val" single='s'> & more"#;
    let escaped = ToastXml::escape(raw);
    assert_eq!(
        escaped,
        "&lt;tag attr=&quot;val&quot; single=&apos;s&apos;&gt; &amp; more"
    );
}

#[test]
fn toast_xml_strips_disallowed_control_chars_preserving_whitespace() {
    let raw = "line1\n\tline2\r\x00\x07\x1Fend";
    assert_eq!(ToastXml::escape(raw), "line1\n\tline2\rend");
}

#[test]
fn toast_xml_build_generic_escapes_title_and_body_without_raw_concat() {
    let title = r#"Ocy <1 & "2">'s"#;
    let body = r#"Break <soon> & "rest" 'now'"#;
    let xml = ToastXml::build_generic(title, body);
    assert_eq!(
        xml,
        "<toast><visual><binding template=\"ToastGeneric\">\
         <text>Ocy &lt;1 &amp; &quot;2&quot;&gt;&apos;s</text>\
         <text>Break &lt;soon&gt; &amp; &quot;rest&quot; &apos;now&apos;</text>\
         </binding></visual></toast>"
    );
}

#[test]
fn aumid_registry_subkey_formats_expected_hkcu_path() {
    assert_eq!(
        aumid_registry_subkey("xyz.damt.ocy"),
        r"Software\Classes\AppUserModelId\xyz.damt.ocy"
    );
}

#[test]
fn notify_icon_path_uses_localappdata_or_userprofile_fallback() {
    assert_eq!(
        notify_icon_path(
            "Ocy",
            Some(OsStr::new("/localappdata")),
            Some(OsStr::new("/userprofile"))
        ),
        Some(
            PathBuf::from("/localappdata")
                .join("Ocy")
                .join("notify-icon.png")
        )
    );
    assert_eq!(
        notify_icon_path(
            "Ocy",
            Some(OsStr::new("")),
            Some(OsStr::new("/userprofile"))
        ),
        Some(
            PathBuf::from("/userprofile")
                .join("AppData")
                .join("Local")
                .join("Ocy")
                .join("notify-icon.png")
        )
    );
    assert_eq!(notify_icon_path("Ocy", None, None), None);
}
