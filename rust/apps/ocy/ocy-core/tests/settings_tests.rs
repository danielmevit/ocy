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

    assert_eq!(settings.schema_version, 2);

    // Short breaks (default interval deliberately diverged from C# 0.1.x's 10 min in P1-preset-retune)
    assert!(settings.short_breaks.enabled);
    assert_eq!(settings.short_breaks.interval_minutes, 25);
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
    assert_eq!(settings.short_breaks.interval_minutes, 25);
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
    assert_eq!(settings.short_breaks.interval_minutes, 25);

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

#[test]
fn load_settings_migrates_old_balanced_defaults_to_new_balanced() {
    let dir = TempDir::new("settings-migrate-old-balanced");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    // Exact old default cadence (short 15 s every 10 min, long 5 min every 60 min) migrates to 25 min.
    let old_defaults = r#"{
        "schemaVersion": 1,
        "shortBreaks": {
            "enabled": true,
            "intervalMinutes": 10,
            "durationSeconds": 15,
            "fullscreen": false
        },
        "longBreaks": {
            "enabled": true,
            "intervalMinutes": 60,
            "durationMinutes": 5,
            "warningSeconds": 60,
            "postponeMinutes": 3,
            "maxPostpones": 3
        },
        "language": "en"
    }"#;
    fs::write(store.settings_path(), old_defaults).expect("write old defaults");

    let migrated = store.load_settings();
    assert_eq!(migrated.schema_version, 2);
    assert_eq!(migrated.short_breaks.interval_minutes, 25);
    assert_eq!(migrated.short_breaks.duration_seconds, 15);
    assert_eq!(migrated.long_breaks.interval_minutes, 60);
    assert_eq!(migrated.long_breaks.duration_minutes, 5);
    assert_eq!(migrated, OcySettings::default());

    // Load -> save -> load round-trip persists schemaVersion 2 and remains stable.
    store.save_settings(&migrated).expect("save migrated");
    let saved_json = fs::read_to_string(store.settings_path()).expect("read saved");
    assert!(saved_json.contains("\"schemaVersion\": 2"));
    let reloaded = store.load_settings();
    assert_eq!(reloaded, migrated);

    // Non-preset fields stay untouched when migrating the old Balanced cadence.
    let old_balanced_custom_lang = r#"{
        "schemaVersion": 1,
        "shortBreaks": { "intervalMinutes": 10, "durationSeconds": 15 },
        "longBreaks": { "intervalMinutes": 60, "durationMinutes": 5 },
        "strictMode": true,
        "language": "de"
    }"#;
    fs::write(store.settings_path(), old_balanced_custom_lang).expect("write old balanced");
    let migrated_custom = store.load_settings();
    assert_eq!(migrated_custom.schema_version, 2);
    assert_eq!(migrated_custom.short_breaks.interval_minutes, 25);
    assert!(migrated_custom.strict_mode);
    assert_eq!(migrated_custom.language, "de");
}

#[test]
fn load_settings_preserves_version_2_with_old_balanced_cadence() {
    let dir = TempDir::new("settings-v2-keeps-10min");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    // In 0.2.1+ (schemaVersion 2), a user who explicitly sets 15 s / 10 min / 5 min / 60 min
    // must keep 10 min across restarts.
    let v2_explicit_ten_min = r#"{
        "schemaVersion": 2,
        "shortBreaks": {
            "enabled": true,
            "intervalMinutes": 10,
            "durationSeconds": 15,
            "fullscreen": false
        },
        "longBreaks": {
            "enabled": true,
            "intervalMinutes": 60,
            "durationMinutes": 5,
            "warningSeconds": 60,
            "postponeMinutes": 3,
            "maxPostpones": 3
        },
        "language": "en"
    }"#;
    fs::write(store.settings_path(), v2_explicit_ten_min).expect("write v2");

    let loaded = store.load_settings();
    assert_eq!(loaded.schema_version, 2);
    assert_eq!(loaded.short_breaks.interval_minutes, 10);
    assert_eq!(loaded.short_breaks.duration_seconds, 15);
    assert_eq!(loaded.long_breaks.interval_minutes, 60);
    assert_eq!(loaded.long_breaks.duration_minutes, 5);

    // Load -> save -> load round-trip stays stable at 10 min and schemaVersion 2.
    store.save_settings(&loaded).expect("save v2");
    let reloaded = store.load_settings();
    assert_eq!(reloaded, loaded);
    assert_eq!(reloaded.short_breaks.interval_minutes, 10);
    assert_eq!(reloaded.schema_version, 2);
}

#[test]
fn load_settings_leaves_near_miss_and_custom_intervals_untouched() {
    let dir = TempDir::new("settings-migrate-near-miss");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    for (short_dur, short_int, long_dur, long_int) in [
        (15, 11, 5, 60),  // near-miss short interval (11 min instead of 10)
        (20, 10, 5, 60),  // near-miss short duration (20 s instead of 15)
        (15, 10, 5, 45),  // near-miss long interval (45 min instead of 60)
        (15, 10, 10, 60), // near-miss long duration (10 min instead of 5)
    ] {
        let json = format!(
            r#"{{
                "schemaVersion": 1,
                "shortBreaks": {{ "intervalMinutes": {short_int}, "durationSeconds": {short_dur} }},
                "longBreaks": {{ "intervalMinutes": {long_int}, "durationMinutes": {long_dur} }}
            }}"#
        );
        fs::write(store.settings_path(), json).expect("write near miss");
        let loaded = store.load_settings();
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.short_breaks.duration_seconds, short_dur);
        assert_eq!(loaded.short_breaks.interval_minutes, short_int);
        assert_eq!(loaded.long_breaks.duration_minutes, long_dur);
        assert_eq!(loaded.long_breaks.interval_minutes, long_int);

        // Save -> load round-trip keeps values and schemaVersion 2.
        store.save_settings(&loaded).expect("save near miss");
        let reloaded = store.load_settings();
        assert_eq!(reloaded, loaded);
    }
}

#[test]
fn downgrade_safety_v2_json_loads_without_resetting_values() {
    let dir = TempDir::new("settings-downgrade-safety");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 12, 0, 0).expect("valid date"));
    let store = AppDataStore::new(dir.path(), clock);

    // 0.2.1 settings with schemaVersion 2 and user-customized values (including 10 min short).
    let mut settings = OcySettings {
        strict_mode: true,
        away_threshold_seconds: 240,
        language: "mk".to_owned(),
        ..OcySettings::default()
    };
    settings.short_breaks.interval_minutes = 10;
    settings.short_breaks.duration_seconds = 20;
    settings.long_breaks.interval_minutes = 50;

    store.save_settings(&settings).expect("save v2 settings");
    let json = fs::read_to_string(store.settings_path()).expect("read v2 json");
    assert!(json.contains("\"schemaVersion\": 2"));

    // 0.2.0 (fc73452) deserialized OcySettings via serde_json::from_str::<OcySettings> and called
    // clamp() with no check or rejection on schema_version != 1. Simulate that exact load path
    // here (raw deserialization + clamp, without 0.2.1's migration) and verify all user values
    // survive intact.
    let mut v020_loaded: OcySettings =
        serde_json::from_str(&json).expect("0.2.0 deserializer accepts schemaVersion 2");
    v020_loaded.clamp();
    assert_eq!(v020_loaded.schema_version, 2);
    assert_eq!(v020_loaded.short_breaks.interval_minutes, 10);
    assert_eq!(v020_loaded.short_breaks.duration_seconds, 20);
    assert_eq!(v020_loaded.long_breaks.interval_minutes, 50);
    assert!(v020_loaded.strict_mode);
    assert_eq!(v020_loaded.away_threshold_seconds, 240);
    assert_eq!(v020_loaded.language, "mk");
}
