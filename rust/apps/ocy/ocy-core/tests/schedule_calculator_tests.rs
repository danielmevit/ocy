use std::time::Duration;

use ocy_core::schedule::ScheduleCalculator;
use ocy_core::settings::OcySettings;

#[test]
fn calculate_when_counters_zero_returns_full_intervals() {
    let mut settings = OcySettings::default();
    settings.short_breaks.interval_minutes = 10;
    settings.long_breaks.interval_minutes = 60;
    settings.long_breaks.warning_seconds = 60;

    let short_due = ScheduleCalculator::time_until_short_break(&settings, Duration::ZERO);
    let long_due = ScheduleCalculator::time_until_long_break(&settings, Duration::ZERO);
    let warning_due = ScheduleCalculator::time_until_long_break_warning(&settings, Duration::ZERO);

    assert_eq!(short_due, Some(Duration::from_secs(10 * 60)));
    assert_eq!(long_due, Some(Duration::from_secs(60 * 60)));
    assert_eq!(warning_due, Some(Duration::from_secs(59 * 60)));
}

#[test]
fn calculate_with_elapsed_time_returns_remaining_time() {
    let mut settings = OcySettings::default();
    settings.short_breaks.interval_minutes = 10;
    settings.long_breaks.interval_minutes = 60;
    settings.long_breaks.warning_seconds = 60;

    let short_due =
        ScheduleCalculator::time_until_short_break(&settings, Duration::from_secs(4 * 60));
    let long_due =
        ScheduleCalculator::time_until_long_break(&settings, Duration::from_secs(25 * 60));
    let warning_due =
        ScheduleCalculator::time_until_long_break_warning(&settings, Duration::from_secs(25 * 60));

    assert_eq!(short_due, Some(Duration::from_secs(6 * 60)));
    assert_eq!(long_due, Some(Duration::from_secs(35 * 60)));
    assert_eq!(warning_due, Some(Duration::from_secs(34 * 60)));
}

#[test]
fn calculate_when_elapsed_exceeds_interval_returns_zero() {
    let mut settings = OcySettings::default();
    settings.short_breaks.interval_minutes = 10;

    let short_due =
        ScheduleCalculator::time_until_short_break(&settings, Duration::from_secs(12 * 60));
    assert_eq!(short_due, Some(Duration::ZERO));
}

#[test]
fn calculate_when_break_disabled_returns_null() {
    let mut settings = OcySettings::default();
    settings.short_breaks.enabled = false;
    settings.long_breaks.enabled = false;

    assert_eq!(
        ScheduleCalculator::time_until_short_break(&settings, Duration::ZERO),
        None
    );
    assert_eq!(
        ScheduleCalculator::time_until_long_break(&settings, Duration::ZERO),
        None
    );
    assert_eq!(
        ScheduleCalculator::time_until_long_break_warning(&settings, Duration::ZERO),
        None
    );
}

#[test]
fn suppression_when_long_break_due_within_2_minutes_returns_true() {
    let mut settings = OcySettings::default();
    settings.long_breaks.interval_minutes = 60;

    // Long break due in 1 minute
    assert!(ScheduleCalculator::is_short_break_suppressed_by_long_break(
        &settings,
        Duration::from_secs(59 * 60),
    ));

    // Long break due in exactly 2 minutes
    assert!(ScheduleCalculator::is_short_break_suppressed_by_long_break(
        &settings,
        Duration::from_secs(58 * 60),
    ));

    // Long break due in 5 minutes
    assert!(
        !ScheduleCalculator::is_short_break_suppressed_by_long_break(
            &settings,
            Duration::from_secs(55 * 60),
        )
    );
}
