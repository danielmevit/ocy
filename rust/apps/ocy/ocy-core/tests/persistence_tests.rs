mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use common::TempDir;
use damt_platform::Unsupported;
use jiff::civil::Date;
use ocy_core::engine::{BreakEngine, BreakKind, EngineEvent, EngineState};
use ocy_core::persistence::{AppDataStore, NewlineStyle};
use ocy_core::settings::OcySettings;
use ocy_core::statistics::StatisticsData;
use ocy_core::time::{DateTimeOffset, FakeClock, PAUSE_FOREVER_SENTINEL, Paused};

fn fixture_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/csharp")
        .join(rel)
}

#[test]
fn engine_state_round_trip_save_and_load() {
    let dir = TempDir::new("persist-state-rt");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 8, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock.clone());

    let now_utc = clock.utc_now();
    let state = EngineState {
        schema_version: 1,
        since_short_break: Duration::from_secs(8 * 60),
        since_long_break: Duration::from_secs(42 * 60),
        postpones_used: 2,
        paused_until_utc: Some(Paused::Until(
            now_utc
                .checked_add(Duration::from_secs(3600))
                .expect("valid add"),
        )),
        last_tick_utc: Some(now_utc),
    };

    store.save_state(&state).expect("save state");

    assert!(store.state_path().exists());
    let mut tmp = store.state_path().as_os_str().to_os_string();
    tmp.push(".tmp");
    assert!(!PathBuf::from(tmp).exists());

    let loaded = store.load_state();
    assert_eq!(loaded.since_short_break, Duration::from_secs(8 * 60));
    assert_eq!(loaded.since_long_break, Duration::from_secs(42 * 60));
    assert_eq!(loaded.postpones_used, 2);
    assert_eq!(loaded.paused_until_utc, state.paused_until_utc);
    assert_eq!(loaded.last_tick_utc, state.last_tick_utc);
}

#[test]
fn corrupt_state_file_backed_up_and_defaults_returned() {
    let dir = TempDir::new("persist-state-corrupt");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 8, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    fs::write(store.state_path(), "{ invalid json state !!!").expect("write corrupt");

    let loaded = store.load_state();
    assert_eq!(loaded.since_short_break, Duration::ZERO);
    assert_eq!(loaded.since_long_break, Duration::ZERO);

    let bak_files: Vec<_> = fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("state.json.bak-")
        })
        .collect();
    assert_eq!(bak_files.len(), 1);
    let bak_name = bak_files[0].file_name().to_string_lossy().into_owned();
    assert!(bak_name.contains("20261117T080000Z"));
}

#[test]
fn engine_restart_after_long_gap_credits_long_break() {
    let dir = TempDir::new("persist-restart-long");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 8, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock.clone());

    let mut settings = OcySettings::default();
    settings.short_breaks.duration_seconds = 15;
    settings.long_breaks.duration_minutes = 5;

    let t0 = clock.utc_now();
    let saved_state = EngineState {
        since_short_break: Duration::from_secs(5 * 60),
        since_long_break: Duration::from_secs(35 * 60),
        last_tick_utc: Some(t0),
        ..EngineState::default()
    };
    store.save_state(&saved_state).expect("save state");

    // Advance fake clock by 10 minutes (gap = 10 min >= 5 min long break duration)
    clock.advance(Duration::from_secs(10 * 60));

    let mut engine = BreakEngine::new(
        settings,
        Arc::new(Unsupported),
        clock,
        Some(AppDataStore::new(
            dir.path(),
            Arc::new(FakeClock::from_utc(2026, 11, 17, 8, 10, 0).expect("valid")),
        )),
        None,
        None,
        None,
    );

    let events = engine.tick();
    assert!(events.contains(&EngineEvent::AbsenceCredited(BreakKind::Long)));
    assert_eq!(engine.since_short_break(), Duration::ZERO);
    assert_eq!(engine.since_long_break(), Duration::ZERO);
    assert_eq!(
        engine
            .statistics()
            .get(Date::constant(2026, 11, 17))
            .map(|d| d.absences_credited),
        Some(1)
    );
}

#[test]
fn engine_restart_after_short_gap_resets_only_short_counter() {
    let dir = TempDir::new("persist-restart-short");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 8, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock.clone());

    let mut settings = OcySettings::default();
    settings.short_breaks.duration_seconds = 15;
    settings.long_breaks.duration_minutes = 5;

    let t0 = clock.utc_now();
    let saved_state = EngineState {
        since_short_break: Duration::from_secs(5 * 60),
        since_long_break: Duration::from_secs(35 * 60),
        last_tick_utc: Some(t0),
        ..EngineState::default()
    };
    store.save_state(&saved_state).expect("save state");

    // Advance fake clock by 30 seconds (30s >= 15s short duration, < 5 min long duration)
    clock.advance(Duration::from_secs(30));

    let mut engine = BreakEngine::new(
        settings,
        Arc::new(Unsupported),
        clock.clone(),
        Some(AppDataStore::new(dir.path(), clock)),
        None,
        None,
        None,
    );

    let events = engine.tick();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::AbsenceCredited(_)))
    );
    assert_eq!(engine.since_short_break(), Duration::ZERO);
    assert_eq!(engine.since_long_break(), Duration::from_secs(35 * 60));
}

#[test]
fn defect_d_directory_at_settings_path_returns_defaults_without_renaming() {
    let dir = TempDir::new("persist-dir-settings");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 8, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    fs::create_dir_all(store.settings_path()).expect("create directory at settings.json");

    let settings = store.load_settings();
    assert_eq!(settings.short_breaks.interval_minutes, 10);
    assert_eq!(settings.long_breaks.interval_minutes, 60);
    assert!(store.settings_path().is_dir());

    let bak_count = fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("settings.json.bak-")
        })
        .count();
    assert_eq!(bak_count, 0);
}

#[test]
fn defect_d_directory_at_state_and_stats_path_returns_defaults_without_renaming() {
    let dir = TempDir::new("persist-dir-state-stats");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 8, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    fs::create_dir_all(store.state_path()).expect("create directory at state.json");
    fs::create_dir_all(store.stats_path()).expect("create directory at stats.json");

    let state = store.load_state();
    let stats = store.load_stats();

    assert_eq!(state.since_short_break, Duration::ZERO);
    assert_eq!(state.since_long_break, Duration::ZERO);
    assert!(store.state_path().is_dir());

    assert!(stats.days.is_empty());
    assert!(store.stats_path().is_dir());

    let bak_count = fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
        .count();
    assert_eq!(bak_count, 0);
}

#[test]
fn csharp_fixture_defaults_round_trip_byte_identical() {
    let dir = TempDir::new("fixture-defaults");
    for file in ["settings.json", "state.json", "stats.json"] {
        fs::copy(
            fixture_path(&format!("defaults/{file}")),
            dir.path().join(file),
        )
        .expect("copy fixture");
    }

    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).expect("valid"));
    let store = AppDataStore::with_newline(dir.path(), clock, NewlineStyle::Lf);

    let settings = store.load_settings();
    assert_eq!(settings, OcySettings::default());
    store.save_settings(&settings).expect("save");
    assert_eq!(
        fs::read(store.settings_path()).expect("read saved"),
        fs::read(fixture_path("defaults/settings.json")).expect("read fixture")
    );

    let state = store.load_state();
    assert_eq!(state, EngineState::default());
    store.save_state(&state).expect("save");
    assert_eq!(
        fs::read(store.state_path()).expect("read saved"),
        fs::read(fixture_path("defaults/state.json")).expect("read fixture")
    );

    let stats = store.load_stats();
    assert_eq!(stats, StatisticsData::default());
    store.save_stats(&stats).expect("save");
    assert_eq!(
        fs::read(store.stats_path()).expect("read saved"),
        fs::read(fixture_path("defaults/stats.json")).expect("read fixture")
    );
}

#[test]
fn csharp_fixture_custom_round_trip_byte_identical() {
    let dir = TempDir::new("fixture-custom");
    for file in ["settings.json", "state.json", "stats.json"] {
        fs::copy(
            fixture_path(&format!("custom/{file}")),
            dir.path().join(file),
        )
        .expect("copy fixture");
    }

    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).expect("valid"));
    let store = AppDataStore::with_newline(dir.path(), clock, NewlineStyle::Lf);

    let settings = store.load_settings();
    assert!(!settings.short_breaks.enabled);
    assert_eq!(settings.short_breaks.interval_minutes, 7);
    assert_eq!(settings.short_breaks.duration_seconds, 25);
    assert!(settings.short_breaks.fullscreen);
    assert!(settings.long_breaks.enabled);
    assert_eq!(settings.long_breaks.interval_minutes, 45);
    assert_eq!(settings.long_breaks.duration_minutes, 8);
    assert_eq!(settings.long_breaks.warning_seconds, 30);
    assert_eq!(settings.long_breaks.postpone_minutes, 5);
    assert_eq!(settings.long_breaks.max_postpones, 2);
    assert!(settings.strict_mode);
    assert!(settings.sounds);
    assert!(!settings.inactivity_tracking);
    assert_eq!(settings.away_threshold_seconds, 300);
    assert!(!settings.pause_during_fullscreen);
    assert!(!settings.has_window_nearby);
    assert!(!settings.status_notifications);
    assert!(settings.start_with_system);
    assert_eq!(settings.language, "mk");

    store.save_settings(&settings).expect("save");
    assert_eq!(
        fs::read(store.settings_path()).expect("read saved"),
        fs::read(fixture_path("custom/settings.json")).expect("read fixture")
    );

    let state = store.load_state();
    assert_eq!(
        state.since_short_break,
        Duration::new(7 * 60 + 11, 250_000_000)
    );
    assert_eq!(state.since_long_break, Duration::from_secs(37 * 60 + 30));
    assert_eq!(state.postpones_used, 1);
    assert_eq!(
        state.paused_until_utc,
        Some(Paused::Until(
            "2026-11-19T14:30:00+00:00"
                .parse::<DateTimeOffset>()
                .expect("valid")
        ))
    );
    assert_eq!(
        state.last_tick_utc,
        Some(
            "2026-11-19T13:05:07.123+00:00"
                .parse::<DateTimeOffset>()
                .expect("valid")
        )
    );

    store.save_state(&state).expect("save");
    assert_eq!(
        fs::read(store.state_path()).expect("read saved"),
        fs::read(fixture_path("custom/state.json")).expect("read fixture")
    );

    let stats = store.load_stats();
    assert_eq!(stats.days.len(), 2);
    assert_eq!(stats.days[0].date, Date::constant(2026, 11, 18));
    assert_eq!(stats.days[0].short_completed, 21);
    assert_eq!(stats.days[0].short_skipped, 3);
    assert_eq!(stats.days[0].long_completed, 4);
    assert_eq!(stats.days[0].long_skipped, 1);
    assert_eq!(stats.days[0].long_postponed, 2);
    assert_eq!(stats.days[0].absences_credited, 5);
    assert_eq!(stats.days[0].active_minutes, 412);
    assert_eq!(stats.days[1].date, Date::constant(2026, 11, 19));
    assert_eq!(stats.days[1].short_completed, 6);
    assert_eq!(stats.days[1].active_minutes, 95);

    store.save_stats(&stats).expect("save");
    assert_eq!(
        fs::read(store.stats_path()).expect("read saved"),
        fs::read(fixture_path("custom/stats.json")).expect("read fixture")
    );
}

#[test]
fn csharp_fixture_paused_indefinitely_round_trip_byte_identical() {
    let dir = TempDir::new("fixture-paused-indef");
    fs::copy(
        fixture_path("paused-indefinitely/state.json"),
        dir.path().join("state.json"),
    )
    .expect("copy fixture");

    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).expect("valid"));
    let store = AppDataStore::with_newline(dir.path(), clock, NewlineStyle::Lf);

    let state = store.load_state();
    assert_eq!(state.paused_until_utc, Some(Paused::UntilResumed));
    assert_eq!(
        state.paused_until_utc.expect("paused").to_string(),
        PAUSE_FOREVER_SENTINEL
    );
    assert_eq!(
        state.last_tick_utc,
        Some(
            "2026-11-19T08:00:00+01:00"
                .parse::<DateTimeOffset>()
                .expect("valid")
        )
    );

    store.save_state(&state).expect("save");
    assert_eq!(
        fs::read(store.state_path()).expect("read saved"),
        fs::read(fixture_path("paused-indefinitely/state.json")).expect("read fixture")
    );
}

#[test]
fn dotnet_timespan_and_datetime_offset_edge_cases() {
    use ocy_core::time::{format_dotnet_timespan, parse_dotnet_timespan};

    // Days + zero subseconds
    let d1 = Duration::from_secs(86_400 + 2 * 3600 + 3 * 60 + 4);
    assert_eq!(format_dotnet_timespan(d1), "1.02:03:04");
    assert_eq!(parse_dotnet_timespan("1.02:03:04").expect("valid"), d1);

    // 7 fractional digits preserved even with trailing zeros
    let d2 = Duration::new(7 * 60 + 11, 250_000_000);
    assert_eq!(format_dotnet_timespan(d2), "00:07:11.2500000");
    assert_eq!(
        parse_dotnet_timespan("00:07:11.25").expect("valid short frac"),
        d2
    );

    // Negative / invalid TimeSpan rejected
    assert!(parse_dotnet_timespan("-00:01:00").is_err());
    assert!(parse_dotnet_timespan("25:00:00").is_err());
    assert!(parse_dotnet_timespan("00:60:00").is_err());

    // Negative offset and Z parsing on DateTimeOffset
    let neg: DateTimeOffset = "2026-11-19T08:00:00.500-05:30".parse().expect("valid");
    assert_eq!(neg.to_string(), "2026-11-19T08:00:00.5-05:30");

    let z: DateTimeOffset = "2026-11-19T13:05:07.1230000Z".parse().expect("valid");
    assert_eq!(z.to_string(), "2026-11-19T13:05:07.123+00:00");
}

#[test]
fn state_and_stats_ignore_unknown_keys_and_use_defaults() {
    let dir = TempDir::new("unknown-keys-state-stats");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).expect("valid"));
    let store = AppDataStore::new(dir.path(), clock);

    fs::write(
        store.state_path(),
        r#"{"futureField": true, "postponesUsed": 2}"#,
    )
    .expect("write state");
    let state = store.load_state();
    assert_eq!(state.schema_version, 1);
    assert_eq!(state.postpones_used, 2);
    assert_eq!(state.since_short_break, Duration::ZERO);

    fs::write(
        store.stats_path(),
        r#"{"futureField": 123, "days": [{"date": "2026-11-19", "shortCompleted": 4, "extra": "x"}]}"#,
    )
    .expect("write stats");
    let stats = store.load_stats();
    assert_eq!(stats.schema_version, 1);
    assert_eq!(stats.days.len(), 1);
    assert_eq!(stats.days[0].short_completed, 4);
    assert_eq!(stats.days[0].long_completed, 0);
}

#[test]
fn crlf_style_writes_crlf_files_and_round_trips() {
    let dir = TempDir::new("persist-crlf-style");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).expect("valid"));
    let store = AppDataStore::with_newline(dir.path(), clock, NewlineStyle::Crlf);

    let mut settings = OcySettings::default();
    settings.short_breaks.interval_minutes = 7;
    store.save_settings(&settings).expect("save settings");
    store
        .save_state(&EngineState::default())
        .expect("save state");

    for path in [store.settings_path(), store.state_path()] {
        let text = fs::read_to_string(&path).expect("read saved");
        assert!(
            text.contains("\r\n"),
            "CRLF line endings in {}",
            path.display()
        );
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                assert!(
                    index > 0 && text.as_bytes()[index - 1] == b'\r',
                    "no bare LF in {}",
                    path.display()
                );
            }
        }
    }

    // Both files load back unchanged.
    let loaded = store.load_settings();
    assert_eq!(loaded.short_breaks.interval_minutes, 7);
    assert_eq!(store.load_state(), EngineState::default());
}

#[test]
fn reads_both_crlf_and_lf_files() {
    let dir = TempDir::new("persist-read-crlf-lf");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).expect("valid"));
    let store = AppDataStore::new(dir.path(), clock);

    let lf_text = fs::read_to_string(fixture_path("custom/settings.json")).expect("read fixture");
    assert!(lf_text.contains('\n') && !lf_text.contains('\r'));

    // CRLF variant of the same file (what the C# app writes on Windows).
    let crlf_text = lf_text.replace('\n', "\r\n");
    fs::write(store.settings_path(), crlf_text).expect("write crlf");
    let from_crlf = store.load_settings();
    assert!(!from_crlf.short_breaks.enabled);
    assert_eq!(from_crlf.short_breaks.interval_minutes, 7);
    assert_eq!(from_crlf.short_breaks.duration_seconds, 25);
    assert_eq!(from_crlf.language, "mk");

    // LF variant (what the C# app writes on Linux) still loads.
    fs::write(store.settings_path(), lf_text).expect("write lf");
    let from_lf = store.load_settings();
    assert_eq!(from_lf, from_crlf);
}

#[test]
fn platform_style_matches_dotnet_environment_new_line() {
    let expected = if cfg!(windows) { "\r\n" } else { "\n" };
    assert_eq!(NewlineStyle::Platform.as_str(), expected);
    assert_eq!(NewlineStyle::Lf.as_str(), "\n");
    assert_eq!(NewlineStyle::Crlf.as_str(), "\r\n");
}
