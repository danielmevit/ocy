//! Pure schedule due-time calculations.

use std::time::Duration;

use crate::settings::OcySettings;

/// Suppression window: short breaks are silently reset when a long break is due within 2 minutes.
const SHORT_BREAK_SUPPRESSION_WINDOW: Duration = Duration::from_secs(120);

/// Pure helper for computing remaining active times until breaks are due.
#[derive(Debug)]
pub struct ScheduleCalculator;

impl ScheduleCalculator {
    /// Calculates remaining active time until the next short break is due.
    ///
    /// Returns `None` if short breaks are disabled.
    #[must_use]
    pub fn time_until_short_break(
        settings: &OcySettings,
        since_short_break: Duration,
    ) -> Option<Duration> {
        if !settings.short_breaks.enabled {
            return None;
        }
        Some(
            settings
                .short_breaks
                .interval()
                .saturating_sub(since_short_break),
        )
    }

    /// Calculates remaining active time until the next long break is due.
    ///
    /// Returns `None` if long breaks are disabled.
    #[must_use]
    pub fn time_until_long_break(
        settings: &OcySettings,
        since_long_break: Duration,
    ) -> Option<Duration> {
        if !settings.long_breaks.enabled {
            return None;
        }
        Some(
            settings
                .long_breaks
                .interval()
                .saturating_sub(since_long_break),
        )
    }

    /// Calculates remaining active time until the next long break warning begins.
    ///
    /// Returns `None` if long breaks are disabled or `warning_seconds <= 0`.
    #[must_use]
    pub fn time_until_long_break_warning(
        settings: &OcySettings,
        since_long_break: Duration,
    ) -> Option<Duration> {
        if !settings.long_breaks.enabled || settings.long_breaks.warning_seconds <= 0 {
            return None;
        }
        let warning_at = settings
            .long_breaks
            .interval()
            .saturating_sub(settings.long_breaks.warning_duration());
        Some(warning_at.saturating_sub(since_long_break))
    }

    /// Returns `true` if a short break should be suppressed because a long break is due within 2 minutes.
    #[must_use]
    pub fn is_short_break_suppressed_by_long_break(
        settings: &OcySettings,
        since_long_break: Duration,
    ) -> bool {
        if !settings.long_breaks.enabled {
            return false;
        }
        let time_until_long = settings
            .long_breaks
            .interval()
            .saturating_sub(since_long_break);
        time_until_long <= SHORT_BREAK_SUPPRESSION_WINDOW
    }
}
