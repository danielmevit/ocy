//! Today's break counters (`taken` and `skipped`) for Settings V2.

use damt_platform::Clock;
use jiff::civil::Date;
use ocy_core::engine::BreakEngine;
use ocy_core::statistics::{DailyStatistics, StatisticsData};

/// Summary counts for the Today card in Settings V2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TodaySummary {
    /// Total breaks completed today (`short_completed + long_completed`).
    pub taken: u32,
    /// Total breaks skipped today (`short_skipped + long_skipped`).
    pub skipped: u32,
}

impl TodaySummary {
    /// Computes today's summary from a single [`DailyStatistics`] bucket.
    #[must_use]
    pub const fn from_daily(daily: Option<&DailyStatistics>) -> Self {
        match daily {
            Some(day) => Self {
                taken: day.short_completed.saturating_add(day.long_completed),
                skipped: day.short_skipped.saturating_add(day.long_skipped),
            },
            None => Self {
                taken: 0,
                skipped: 0,
            },
        }
    }

    /// Computes the summary for `local_date` from `stats`.
    #[must_use]
    pub fn from_statistics(stats: &StatisticsData, local_date: Date) -> Self {
        Self::from_daily(stats.get(local_date))
    }

    /// Computes today's summary from `engine`'s statistics using `clock`'s local calendar date.
    #[must_use]
    pub fn for_today(engine: &BreakEngine, clock: &dyn Clock) -> Self {
        Self::from_statistics(engine.statistics(), clock.now().date())
    }

    /// Saturating `i32` conversion for `SettingsWindowV2.today-taken`.
    #[must_use]
    pub fn taken_i32(self) -> i32 {
        i32::try_from(self.taken).unwrap_or(i32::MAX)
    }

    /// Saturating `i32` conversion for `SettingsWindowV2.today-skipped`.
    #[must_use]
    pub fn skipped_i32(self) -> i32 {
        i32::try_from(self.skipped).unwrap_or(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use damt_platform::Unsupported;
    use ocy_core::settings::OcySettings;
    use ocy_core::time::FakeClock;
    use std::sync::Arc;

    #[test]
    fn sums_short_and_long_completed_and_skipped_for_local_date() {
        let mut stats = StatisticsData::default();
        let yesterday = Date::constant(2026, 11, 19);
        let today = Date::constant(2026, 11, 20);

        let y = stats.get_or_create(yesterday);
        y.short_completed = 10;
        y.long_completed = 2;
        y.short_skipped = 3;
        y.long_skipped = 1;

        let t = stats.get_or_create(today);
        t.short_completed = 7;
        t.long_completed = 2;
        t.short_skipped = 1;
        t.long_skipped = 3;
        t.absences_credited = 4;
        t.long_postponed = 5;

        // 2026-11-19T23:30:00Z in +02:00 is 2026-11-20T01:30:00+02:00 locally!
        let clock = Arc::new(
            FakeClock::from_fixed_offset(2026, 11, 20, 1, 30, 0, 2).expect("valid local time"),
        );
        let engine = BreakEngine::new(
            OcySettings::default(),
            Arc::new(Unsupported),
            clock.clone(),
            None,
            None,
            None,
            Some(stats),
        );

        let summary = TodaySummary::for_today(&engine, clock.as_ref());
        assert_eq!(
            summary,
            TodaySummary {
                taken: 9,
                skipped: 4
            }
        );
        assert_eq!(summary.taken_i32(), 9);
        assert_eq!(summary.skipped_i32(), 4);
    }

    #[test]
    fn missing_local_date_returns_zeros() {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 20, 10, 0, 0).expect("valid utc time"));
        let engine = BreakEngine::with_defaults(
            OcySettings::default(),
            Arc::new(Unsupported),
            clock.clone(),
        );
        assert_eq!(
            TodaySummary::for_today(&engine, clock.as_ref()),
            TodaySummary {
                taken: 0,
                skipped: 0
            }
        );
    }
}
