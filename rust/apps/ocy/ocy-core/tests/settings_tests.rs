mod common;

use std::fs;
use std::sync::Arc;

use common::TempDir;
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::{LongBreakSettings, OcySettings, ShortBreakSettings};
use ocy_core::time::FakeClock;

#[test]
fn settings_defaults_match_architecture_specification() {
    let mut settings = OcySettings::default();
    settings.clamp();

    assert_eq!(settings.schema_version, 1);

    // Short breaks
    assert!(settings.short_breaks.enabled);
    assert_eq!(settings.short_breaks.interval_minutes, 10);
    assert_eq!(settings.short_breaks.duration_seconds, 15);
    assert!(!settings.short_breaks.fullscreen);

    // Long breaks
    assert!(settings.long_breaks.enabled);
    assert_eq!(settings.long_breaks.interval_minutes, 60);
    assert_eq!(settings.long_breaks.duration_minutes, 5);
    assert_eq!(settings.long_breaks.warning_seconds, 60);
    assert_eq!(settings.long_breaks.postpone_minutes, 3);
    assert_eq!(settings.long_breaks.max_postpones, 3);

    // General
    assert!(!settings.strict_mode);
    assert!(!settings.sounds);
    assert!(settings.inactivity_tracking);
    assert_eq!(settings.away_threshold_seconds, 120);
    assert!(settings.pause_during_fullscreen);
    assert!(settings.has_window_nearby);
    assert!(settings.status_notifications);
    assert!(!settings.start_with_system);
    assert_eq!(settings.language, "en");
}

#[test]
fn settings_clamping_enforces_allowed_ranges() {
    let mut settings = OcySettings {
        short_breaks: ShortBreakSettings {
            interval_minutes: 1,   // min 2
            duration_seconds: 200, // max 120
            ..ShortBreakSettings::default()
        },
        long_breaks: LongBreakSettings {
            interval_minutes: 500, // max 240
            duration_minutes: 0,   // min 1
            warning_seconds: 500,  // max 300
            postpone_minutes: 50,  // max 30
            max_postpones: 20,     // max 10
            ..LongBreakSettings::default()
        },
        away_threshold_seconds: 10, // min 30
        language: "   ".to_owned(), // fallback "en"
        ..OcySettings::default()
    };

    settings.clamp();

    assert_eq!(settings.short_breaks.interval_minutes, 2);
    assert_eq!(settings.short_breaks.duration_seconds, 120);
    assert_eq!(settings.long_breaks.interval_minutes, 240);
    assert_eq!(settings.long_breaks.duration_minutes, 1);
    assert_eq!(settings.long_breaks.warning_seconds, 300);
    assert_eq!(settings.long_breaks.postpone_minutes, 30);
    assert_eq!(settings.long_breaks.max_postpones, 10);
    assert_eq!(settings.away_threshold_seconds, 30);
    assert_eq!(settings.language, "en");
}

#[test]
fn load_settings_when_missing_returns_defaults() {
    let dir = TempDir::new("settings-missing");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    let settings = store.load_settings();
    assert_eq!(settings.short_breaks.interval_minutes, 10);
    assert_eq!(settings.long_breaks.interval_minutes, 60);
}

#[test]
fn load_settings_ignores_unknown_keys() {
    let dir = TempDir::new("settings-unknown");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    let json = r#"{
        "schemaVersion": 1,
        "futureUnrecognizedProperty": 9999,
        "anotherUnknown": "abc",
        "shortBreaks": {
            "enabled": true,
            "intervalMinutes": 15,
            "durationSeconds": 25,
            "extraField": true
        }
    }"#;
    fs::write(store.settings_path(), json).expect("write settings");

    let loaded = store.load_settings();
    assert_eq!(loaded.short_breaks.interval_minutes, 15);
    assert_eq!(loaded.short_breaks.duration_seconds, 25);
}

#[test]
fn load_settings_corrupt_file_is_backed_up_and_defaults_used() {
    let dir = TempDir::new("settings-corrupt");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    let corrupt = "{ this is not valid json! :::";
    fs::write(store.settings_path(), corrupt).expect("write corrupt settings");

    let settings = store.load_settings();
    assert_eq!(settings.short_breaks.interval_minutes, 10);

    let bak_files: Vec<_> = fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("settings.json.bak-")
        })
        .collect();
    assert_eq!(bak_files.len(), 1);
    let bak_name = bak_files[0].file_name().to_string_lossy().into_owned();
    assert!(bak_name.contains("20261117T120000Z"));
    assert_eq!(
        fs::read_to_string(bak_files[0].path()).expect("read bak"),
        corrupt
    );
    assert!(!store.settings_path().exists());
}

#[test]
fn settings_atomic_save_round_trip_saves_and_loads_accurately() {
    let dir = TempDir::new("settings-roundtrip");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    let mut settings = OcySettings {
        strict_mode: true,
        sounds: true,
        away_threshold_seconds: 180,
        language: "de".to_owned(),
        ..OcySettings::default()
    };
    settings.short_breaks.interval_minutes = 12;
    settings.long_breaks.interval_minutes = 45;

    store.save_settings(&settings).expect("save settings");

    assert!(store.settings_path().exists());
    let mut tmp = store.settings_path().as_os_str().to_os_string();
    tmp.push(".tmp");
    assert!(!std::path::PathBuf::from(tmp).exists());

    let loaded = store.load_settings();
    assert!(loaded.strict_mode);
    assert!(loaded.sounds);
    assert_eq!(loaded.away_threshold_seconds, 180);
    assert_eq!(loaded.language, "de");
    assert_eq!(loaded.short_breaks.interval_minutes, 12);
    assert_eq!(loaded.long_breaks.interval_minutes, 45);
}
