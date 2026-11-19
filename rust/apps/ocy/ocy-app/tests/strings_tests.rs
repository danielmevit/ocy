use std::time::Duration;

use jiff::SignedDuration;
use ocy_app::strings::Strings;

#[test]
fn get_existing_key_returns_value() {
    let strings = Strings::from_embedded().unwrap();
    assert_eq!(strings.get("app.name"), "Ocy");
    assert_eq!(strings.get("tray.menu.settings"), "Settings");
    assert_eq!(strings.get("warning.strictOn"), "Strict mode is on");
}

#[test]
fn get_missing_key_returns_key_itself() {
    let strings = Strings::from_embedded().unwrap();
    let missing_key = "this.key.does.not.exist";
    assert_eq!(strings.get(missing_key), missing_key);
}

#[test]
fn format_replaces_named_placeholders() {
    let strings = Strings::from_embedded().unwrap();
    let result = strings.format("longbreak.countdown", &[("time", &"23 min")]);
    assert_eq!(result, "Back to your screen in 23 min");

    let postpone_result = strings.format("warning.postpone", &[("minutes", &3)]);
    assert_eq!(postpone_result, "Postpone 3 min");
}

#[test]
fn format_missing_key_replaces_named_placeholders_in_key() {
    let strings = Strings::from_embedded().unwrap();
    let result = strings.format("custom {name} reminder", &[("name", &"water")]);
    assert_eq!(result, "custom water reminder");
}

#[test]
fn time_text_under_one_minute() {
    let cases = [
        (0, "0:00"),
        (-10, "0:00"),
        (5, "0:05"),
        (42, "0:42"),
        (59, "0:59"),
    ];
    for (secs, expected) in cases {
        let duration = SignedDuration::from_secs(secs);
        assert_eq!(Strings::time_text_signed(duration), expected);
    }
}

#[test]
fn time_text_one_minute_or_more_under_one_hour() {
    let cases = [
        (60, "1 min"),
        (119, "1 min"),
        (120, "2 min"),
        (23 * 60, "23 min"),
        (59 * 60 + 59, "59 min"),
    ];
    for (secs, expected) in cases {
        let duration = Duration::from_secs(secs);
        assert_eq!(Strings::time_text(duration), expected);
    }
}

#[test]
fn time_text_one_hour_or_more() {
    let cases = [
        (60 * 60, "1 h 00 min"),
        (65 * 60, "1 h 05 min"),
        (123 * 60, "2 h 03 min"),
        (10 * 3600 + 45 * 60, "10 h 45 min"),
    ];
    for (secs, expected) in cases {
        let duration = Duration::from_secs(secs);
        assert_eq!(Strings::time_text(duration), expected);
    }
}

#[test]
fn format_time_uses_strings_units() {
    let strings = Strings::from_embedded().unwrap();
    assert_eq!(strings.get("time.hours"), "h");
    assert_eq!(strings.get("time.minutes"), "min");
    assert_eq!(
        strings.format_time(Duration::from_secs(65 * 60)),
        "1 h 05 min"
    );
    assert_eq!(strings.format_time(Duration::from_secs(15 * 60)), "15 min");
}

#[test]
fn settings_window_title_formats_with_app_name() {
    let strings = Strings::from_embedded().unwrap();
    let app_name = strings.get("app.name");
    let title = strings.format("settings.windowTitle", &[("app", &app_name)]);
    assert_eq!(title, "Ocy \u{2014} Settings");
}

#[test]
fn key_count_matches_expected_113() {
    let strings = Strings::from_embedded().unwrap();
    assert_eq!(strings.key_count(), 113);
}

#[test]
fn format_countdown_separate_checks() {
    let cases = [(5, "0:05"), (263, "4:23"), (1800, "30:00")];
    for (secs, expected) in cases {
        assert_eq!(
            Strings::format_countdown(Duration::from_secs(secs)),
            expected
        );
    }
}
