mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{TempDir, TestPlatformServices};
use jiff::ToSpan;
use jiff::civil::Date;
use ocy_core::engine::BreakEngine;
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;
use ocy_core::statistics::StatisticsData;
use ocy_core::time::FakeClock;

#[test]
fn daily_buckets_keyed_by_local_date() {
    let mut stats = StatisticsData::default();
    let today = Date::constant(2026, 11, 17);
    let tomorrow = Date::constant(2026, 11, 18);

    {
        let bucket_today = stats.get_or_create(today);
        bucket_today.short_completed += 3;
        bucket_today.long_completed += 1;
        bucket_today.active_minutes += 45;
    }

    {
        let bucket_tomorrow = stats.get_or_create(tomorrow);
        bucket_tomorrow.short_skipped += 1;
        bucket_tomorrow.active_minutes += 10;
    }

    assert_eq!(stats.days.len(), 2);
    assert_eq!(stats.get_or_create(today).short_completed, 3);
    assert_eq!(stats.get_or_create(today).long_completed, 1);
    assert_eq!(stats.get_or_create(today).active_minutes, 45);
    assert_eq!(stats.get_or_create(tomorrow).short_skipped, 1);
    assert_eq!(stats.get_or_create(tomorrow).active_minutes, 10);
}

#[test]
fn retention_prunes_buckets_older_than_400_days() {
    let mut stats = StatisticsData::default();
    let today = Date::constant(2026, 11, 17);

    let ancient_date = today.checked_sub(405.days()).expect("valid date");
    let border_date = today.checked_sub(400.days()).expect("valid date");
    let recent_date = today.checked_sub(10.days()).expect("valid date");

    stats.get_or_create(ancient_date).short_completed = 10;
    stats.get_or_create(border_date).short_completed = 5;
    stats.get_or_create(recent_date).short_completed = 8;
    stats.get_or_create(today).short_completed = 2;

    assert_eq!(stats.days.len(), 4);

    stats.prune_retention(today, 400);

    assert_eq!(stats.days.len(), 3);
    assert!(stats.get(ancient_date).is_none());
    assert!(stats.get(border_date).is_some());
    assert!(stats.get(recent_date).is_some());
    assert!(stats.get(today).is_some());
}

#[test]
fn statistics_atomic_save_and_load_round_trip() {
    let dir = TempDir::new("stats-roundtrip");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 10, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    let mut stats = StatisticsData::default();
    let today = Date::constant(2026, 11, 17);
    {
        let bucket = stats.get_or_create(today);
        bucket.short_completed = 5;
        bucket.short_skipped = 1;
        bucket.long_completed = 2;
        bucket.long_skipped = 0;
        bucket.long_postponed = 3;
        bucket.absences_credited = 1;
        bucket.active_minutes = 120;
    }

    store.save_stats(&stats).expect("save stats");

    assert!(store.stats_path().exists());
    let mut tmp = store.stats_path().as_os_str().to_os_string();
    tmp.push(".tmp");
    assert!(!std::path::PathBuf::from(tmp).exists());

    let mut loaded = store.load_stats();
    assert_eq!(loaded.days.len(), 1);
    let loaded_bucket = loaded.get_or_create(today);
    assert_eq!(loaded_bucket.short_completed, 5);
    assert_eq!(loaded_bucket.short_skipped, 1);
    assert_eq!(loaded_bucket.long_completed, 2);
    assert_eq!(loaded_bucket.long_postponed, 3);
    assert_eq!(loaded_bucket.absences_credited, 1);
    assert_eq!(loaded_bucket.active_minutes, 120);
}

#[test]
fn statistics_engine_uses_local_timezone_date() {
    // UTC is 2026-11-17T23:30:00Z, local (+02:00) is 2026-11-18T01:30:00+02:00.
    let clock = Arc::new(FakeClock::from_fixed_offset(2026, 11, 18, 1, 30, 0, 2).expect("valid"));
    let platform = TestPlatformServices::new();
    let mut engine = BreakEngine::with_defaults(OcySettings::default(), platform, clock.clone());

    for _ in 0..60 {
        clock.advance(Duration::from_secs(1));
        engine.tick();
    }

    let local_date = Date::constant(2026, 11, 18);
    let utc_date = Date::constant(2026, 11, 17);
    assert_eq!(
        engine
            .statistics()
            .get(local_date)
            .map(|d| d.active_minutes),
        Some(1)
    );
    assert!(engine.statistics().get(utc_date).is_none());
}
