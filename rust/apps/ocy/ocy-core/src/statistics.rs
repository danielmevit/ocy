//! Daily break and activity statistics (`stats.json`).

use jiff::ToSpan;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

/// Per-day break statistics bucket keyed by local [`Date`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatistics {
    /// Local calendar date (`yyyy-MM-dd`).
    pub date: Date,
    /// Number of short breaks completed.
    #[serde(default)]
    pub short_completed: u32,
    /// Number of short breaks skipped.
    #[serde(default)]
    pub short_skipped: u32,
    /// Number of long breaks completed.
    #[serde(default)]
    pub long_completed: u32,
    /// Number of long breaks skipped.
    #[serde(default)]
    pub long_skipped: u32,
    /// Number of long break warnings postponed.
    #[serde(default)]
    pub long_postponed: u32,
    /// Number of long absences credited as long breaks.
    #[serde(default)]
    pub absences_credited: u32,
    /// Active working minutes accumulated on this date.
    #[serde(default)]
    pub active_minutes: u32,
}

impl DailyStatistics {
    /// Creates an empty statistics bucket for `date`.
    #[must_use]
    pub const fn new(date: Date) -> Self {
        Self {
            date,
            short_completed: 0,
            short_skipped: 0,
            long_completed: 0,
            long_skipped: 0,
            long_postponed: 0,
            absences_credited: 0,
            active_minutes: 0,
        }
    }
}

/// Persisted statistics container (`stats.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatisticsData {
    /// JSON schema version (currently `1`).
    pub schema_version: i32,
    /// Daily buckets in insertion order.
    pub days: Vec<DailyStatistics>,
}

impl StatisticsData {
    /// Default retention window in days (`400`).
    pub const MAX_RETENTION_DAYS: i64 = 400;

    /// Returns a mutable reference to the bucket for `date`, creating it if absent.
    pub fn get_or_create(&mut self, date: Date) -> &mut DailyStatistics {
        if let Some(idx) = self.days.iter().position(|d| d.date == date) {
            return &mut self.days[idx];
        }
        self.days.push(DailyStatistics::new(date));
        // `push` guarantees at least one element, and we avoid unwrap via `last_mut` fallback.
        let len = self.days.len();
        &mut self.days[len - 1]
    }

    /// Returns a shared reference to the bucket for `date` if present.
    #[must_use]
    pub fn get(&self, date: Date) -> Option<&DailyStatistics> {
        self.days.iter().find(|d| d.date == date)
    }

    /// Removes buckets strictly older than `today - max_days`.
    pub fn prune_retention(&mut self, today: Date, max_days: i64) {
        if let Ok(cutoff) = today.checked_sub(max_days.days()) {
            self.days.retain(|d| d.date >= cutoff);
        }
    }

    /// Prunes buckets using [`Self::MAX_RETENTION_DAYS`].
    pub fn prune_default_retention(&mut self, today: Date) {
        self.prune_retention(today, Self::MAX_RETENTION_DAYS);
    }
}

impl Default for StatisticsData {
    fn default() -> Self {
        Self {
            schema_version: 1,
            days: Vec::new(),
        }
    }
}
