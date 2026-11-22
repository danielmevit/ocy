use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use damt_platform::Clock;
use damt_platform_win::{RawStoreLicense, StoreLicenseMapper};
use jiff::Timestamp;
use ocy_app::STORE_FEATURE_ENABLED;
use ocy_app::license::{
    CachedLicense, LICENSE_RECHECK_INTERVAL, LicenseCache, LicenseManager, LicenseState,
    StoreLicenseTexts, format_trial_days_left, map_raw_license,
};
use ocy_app::scheduler;
use ocy_app::strings::Strings;
use ocy_core::engine::{BreakEngine, EngineEvent, EnginePhase, EngineSnapshot};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-store-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn winrt_ticks_for_timestamp(ts: Timestamp) -> i64 {
    StoreLicenseMapper::WINRT_UNIX_EPOCH_TICKS
        + ts.as_second() * StoreLicenseMapper::TICKS_PER_SECOND
}

#[test]
fn state_mapping_covers_full_trial_expired_and_unknown() {
    let now = Timestamp::from_second(1_760_000_000).unwrap();

    // Full
    let full = map_raw_license(
        RawStoreLicense {
            is_active: true,
            is_trial: false,
            expiration_universal_time: i64::MAX,
            trial_time_remaining_100ns: 0,
        },
        now,
        None,
    );
    assert_eq!(full, LicenseState::Full);
    assert!(full.allows_reminders());
    assert_eq!(full.trial_days_left(now), None);
    assert_eq!(full.trial_days_left_i32(now), -1);

    // 15-day active trial
    let ends_at = Timestamp::from_second(now.as_second() + 15 * 86_400).unwrap();
    let trial = map_raw_license(
        RawStoreLicense {
            is_active: true,
            is_trial: true,
            expiration_universal_time: winrt_ticks_for_timestamp(ends_at),
            trial_time_remaining_100ns: 15 * 86_400 * StoreLicenseMapper::TICKS_PER_SECOND,
        },
        now,
        None,
    );
    assert_eq!(trial, LicenseState::Trial { ends_at });
    assert!(trial.allows_reminders());
    assert_eq!(trial.trial_days_left(now), Some(15));

    // < 24h remaining rounds up to 1 day left
    let almost_done = Timestamp::from_second(ends_at.as_second() - 3600).unwrap();
    assert_eq!(trial.trial_days_left(almost_done), Some(1));

    // Expired trial
    let expired = map_raw_license(
        RawStoreLicense {
            is_active: false,
            is_trial: true,
            expiration_universal_time: winrt_ticks_for_timestamp(now),
            trial_time_remaining_100ns: 0,
        },
        now,
        None,
    );
    assert_eq!(expired, LicenseState::TrialExpired);
    assert!(!expired.allows_reminders());
    assert_eq!(expired.trial_days_left(now), Some(0));
    assert_eq!(expired.trial_days_left_i32(now), 0);

    // Unknown on first run
    let unknown = map_raw_license(
        RawStoreLicense {
            is_active: false,
            is_trial: false,
            expiration_universal_time: 0,
            trial_time_remaining_100ns: 0,
        },
        now,
        None,
    );
    assert_eq!(unknown, LicenseState::Unknown);
    assert!(unknown.allows_reminders());
    assert_eq!(unknown.trial_days_left(now), None);
}

#[test]
fn offline_cache_fallback_preserves_valid_licenses_and_treats_first_run_offline_as_trial() {
    let dir = temp_dir("cache");
    let cache_path = dir.join("license.json");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 21, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();

    // 1. First run offline (no cache file) -> Unknown = behave as Trial.
    let mut mgr = LicenseManager::new(Some(cache_path.clone()), Arc::clone(&clock_dyn));
    assert_eq!(mgr.state(), LicenseState::Unknown);
    assert!(mgr.state().allows_reminders());
    let state = mgr.on_query_result(Err("offline".to_owned()));
    assert_eq!(state, LicenseState::Unknown);
    assert!(state.allows_reminders());
    assert!(!cache_path.exists());

    // 2. Trial retrieved online -> cached; going offline later keeps Trial while before ends_at.
    let now = clock.now().timestamp();
    let ends_at = Timestamp::from_second(now.as_second() + 15 * 86_400).unwrap();
    let state = mgr.on_query_result(Ok(RawStoreLicense {
        is_active: true,
        is_trial: true,
        expiration_universal_time: winrt_ticks_for_timestamp(ends_at),
        trial_time_remaining_100ns: 15 * 86_400 * StoreLicenseMapper::TICKS_PER_SECOND,
    }));
    assert_eq!(state, LicenseState::Trial { ends_at });
    assert!(cache_path.exists());

    // Restart while offline after 5 days -> still Trial with 10 days left.
    clock.advance(Duration::from_secs(5 * 86_400));
    let mut restarted = LicenseManager::new(Some(cache_path.clone()), Arc::clone(&clock_dyn));
    assert_eq!(restarted.state(), LicenseState::Trial { ends_at });
    assert_eq!(
        restarted.state().trial_days_left(clock.now().timestamp()),
        Some(10)
    );
    let offline_trial = restarted.on_query_result(Err("network down".to_owned()));
    assert_eq!(offline_trial, LicenseState::Trial { ends_at });
    assert!(offline_trial.allows_reminders());

    // 3. Offline once 15-day ends_at has passed -> resolves to TrialExpired locally.
    clock.advance(Duration::from_secs(10 * 86_400));
    let expired_offline = restarted.on_query_result(Err("still offline".to_owned()));
    assert_eq!(expired_offline, LicenseState::TrialExpired);
    assert!(!expired_offline.allows_reminders());

    // 4. Purchase / Full license -> cached; going offline later NEVER locks out Full user.
    let full = restarted.on_purchase_succeeded();
    assert_eq!(full, LicenseState::Full);
    let reloaded = LicenseManager::new(Some(cache_path.clone()), Arc::clone(&clock_dyn));
    assert_eq!(reloaded.state(), LicenseState::Full);

    let mut reloaded_mut = reloaded;
    clock.advance(Duration::from_secs(30 * 86_400));
    let still_full = reloaded_mut.on_query_result(Err("offline forever".to_owned()));
    assert_eq!(still_full, LicenseState::Full);
    assert!(still_full.allows_reminders());

    // 5. Corrupt cache file falls back cleanly to Unknown without panicking.
    fs::write(&cache_path, "{corrupt json").unwrap();
    let corrupt_mgr = LicenseManager::new(Some(cache_path.clone()), Arc::clone(&clock_dyn));
    assert_eq!(corrupt_mgr.state(), LicenseState::Unknown);
    assert!(corrupt_mgr.state().allows_reminders());

    // Verify LicenseCache roundtrip directly.
    let cache = LicenseCache::new(&cache_path);
    cache
        .save(&CachedLicense {
            schema_version: 1,
            state: LicenseState::Trial { ends_at },
            checked_at: now,
        })
        .unwrap();
    assert_eq!(
        cache.load().map(|c| c.state),
        Some(LicenseState::Trial { ends_at })
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn recheck_interval_is_due_after_four_hours() {
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 21, 10, 0, 0).unwrap());
    let mut mgr = LicenseManager::new(None, Arc::clone(&clock) as Arc<dyn Clock>);
    assert!(mgr.is_recheck_due());

    mgr.mark_check_started();
    assert!(!mgr.is_recheck_due());

    clock.advance(LICENSE_RECHECK_INTERVAL - Duration::from_secs(1));
    assert!(!mgr.is_recheck_due());

    clock.advance(Duration::from_secs(1));
    assert!(mgr.is_recheck_due());
}

#[test]
fn trial_expired_scheduler_and_engine_produce_no_breaks() {
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 21, 10, 0, 0).unwrap());
    let platform: Arc<dyn damt_platform::PlatformServices> = Arc::new(damt_platform::Unsupported);
    let mut engine = BreakEngine::with_defaults(
        OcySettings::default(),
        platform,
        Arc::clone(&clock) as Arc<dyn Clock>,
    );

    // Simulate TrialExpired suspending reminders.
    let expired = LicenseState::TrialExpired;
    assert!(!expired.allows_reminders());
    let _ = engine.set_reminders_suspended(!expired.allows_reminders());

    let snap = engine.snapshot();
    assert_eq!(snap.time_until_short_break, None);
    assert_eq!(snap.time_until_long_break, None);
    assert_eq!(snap.time_until_long_break_warning, None);
    assert_eq!(
        scheduler::next_wake_delay_for_license(&snap, clock.now().timestamp(), expired),
        scheduler::MAX_WAKE_DELAY
    );

    // Even a synthetic snapshot with due timers produces no pre-warm or short wake when TrialExpired.
    let active_snap = EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: Some(Duration::from_secs(1)),
        time_until_long_break: Some(Duration::from_secs(2)),
        time_until_long_break_warning: Some(Duration::from_secs(1)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        natural_hold: None,
        poll_external: false,
        game_in_front: false,
    };
    assert_eq!(
        scheduler::next_wake_delay_for_license(&active_snap, clock.now().timestamp(), expired),
        scheduler::MAX_WAKE_DELAY
    );

    // Tick across 90 minutes -> zero breaks or warnings produced.
    for _ in 0..(90 * 60) {
        clock.advance(Duration::from_secs(1));
        let events = engine.tick();
        assert!(
            !events.iter().any(|e| matches!(
                e,
                EngineEvent::BreakStarted(_) | EngineEvent::LongBreakWarningStarted(_)
            )),
            "unexpected break event while TrialExpired: {events:?}"
        );
    }

    // Buying resumes reminders immediately.
    let _ = engine.set_reminders_suspended(!LicenseState::Full.allows_reminders());
    assert!(engine.start_short_break_now().0);
    assert_eq!(engine.phase(), EnginePhase::ShortBreak);
}

#[test]
fn store_strings_and_plurals_exist_in_en_json_and_contain_no_github_links() {
    let strings = Strings::from_embedded().unwrap();
    let texts = StoreLicenseTexts::from_strings(&strings, Some(1));
    assert_eq!(texts.trial_ended_title, "Your trial has ended.");
    assert_eq!(
        texts.trial_ended_body,
        "If you'd like to keep using Ocy, please buy it."
    );
    assert_eq!(texts.buy_label, "Buy Ocy");
    assert_eq!(texts.trial_days_left_text, "1 day left in your trial");

    assert_eq!(
        format_trial_days_left(&strings, 0),
        "0 days left in your trial"
    );
    assert_eq!(
        format_trial_days_left(&strings, 1),
        "1 day left in your trial"
    );
    assert_eq!(
        format_trial_days_left(&strings, 15),
        "15 days left in your trial"
    );

    for s in [
        &texts.trial_ended_title,
        &texts.trial_ended_body,
        &texts.buy_label,
        &texts.trial_days_left_text,
    ] {
        assert!(!s.to_ascii_lowercase().contains("github"));
    }
}

#[cfg(not(feature = "store"))]
#[test]
fn feature_off_build_has_no_store_code_paths() {
    const {
        assert!(!STORE_FEATURE_ENABLED);
        assert!(!cfg!(feature = "store"));
    }

    let dir = temp_dir("feature-off");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 21, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let platform: Arc<dyn damt_platform::PlatformServices> = Arc::new(damt_platform::Unsupported);
    let engine = BreakEngine::with_defaults(OcySettings::default(), platform, clock_dyn);
    assert!(!engine.reminders_suspended());
    assert!(!dir.join("license.json").exists());
    let _ = fs::remove_dir_all(dir);
}

#[cfg(feature = "store")]
#[test]
fn store_feature_host_tray_and_settings_wire_license_state_and_buy() {
    use std::cell::Cell;
    use std::rc::Rc;

    use damt_app::ErrorLog;
    use damt_platform::{PlatformServices, TaskbarThemeProvider, Unsupported};
    use ocy_app::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
    use ocy_app::host::AppHost;
    use ocy_app::tray::{
        AppTheme, TrayController, TrayIconState, TrayPresentation, TrayPresenter, TraySink,
    };

    #[derive(Debug, Default)]
    struct DummyBreakWindows {
        close_count: usize,
    }
    impl BreakWindows for DummyBreakWindows {
        fn show_short_break(&mut self, _info: &ocy_core::engine::BreakInfo) {}
        fn show_warning(&mut self, _countdown: Duration) {}
        fn show_long_break(&mut self, _info: &ocy_core::engine::BreakInfo) {}
        fn update_countdown(&mut self, _snapshot: &EngineSnapshot) {}
        fn close_all(&mut self) {
            self.close_count += 1;
        }
    }

    #[derive(Debug, Default)]
    struct DummySettingsWindow {
        synced: Vec<(LicenseState, i32)>,
    }
    impl SettingsWindow for DummySettingsWindow {
        fn sync_license(&mut self, state: LicenseState, trial_days_left: i32) {
            self.synced.push((state, trial_days_left));
        }
        fn show(&mut self) {}
    }

    #[derive(Debug, Default)]
    struct DummyTraySink {
        last_tooltip: String,
        last_presentation: Option<TrayPresentation>,
    }
    impl TraySink for DummyTraySink {
        fn set_tooltip(&mut self, tooltip: &str) {
            self.last_tooltip = tooltip.to_owned();
        }
        fn set_icon(&mut self, _state: TrayIconState, _use_light_set: bool) {}
        fn set_menu(&mut self, presentation: &TrayPresentation) {
            self.last_presentation = Some(presentation.clone());
        }
    }

    #[derive(Debug)]
    struct LightTaskbar;
    impl TaskbarThemeProvider for LightTaskbar {
        fn is_supported(&self) -> bool {
            true
        }
        fn is_light_taskbar(&self) -> Option<bool> {
            Some(true)
        }
    }

    const {
        assert!(STORE_FEATURE_ENABLED);
    }

    let dir = temp_dir("host-store");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 21, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine =
        BreakEngine::with_defaults(OcySettings::default(), platform, Arc::clone(&clock_dyn));
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let tray = TrayController::new(
        presenter,
        DummyTraySink::default(),
        Rc::new(LightTaskbar),
        Arc::clone(&clock_dyn),
        Some(AppTheme::Light),
        &snapshot,
    );
    let coordinator =
        BreakUiCoordinator::new(DummyBreakWindows::default(), DummySettingsWindow::default());
    let error_log = ErrorLog::new(&dir, "ocy");
    let mut host = AppHost::new(engine, coordinator, tray, error_log);

    let now = clock.now().timestamp();
    let ends_at = Timestamp::from_second(now.as_second() + 7 * 86_400).unwrap();
    host.set_license_state(LicenseState::Trial { ends_at }, now);
    assert_eq!(host.trial_days_left(now), 7);
    assert!(!host.engine().reminders_suspended());

    // TrialExpired disables tray "Take break now" items, updates tooltip, suspends engine.
    host.set_license_state(LicenseState::TrialExpired, now);
    assert!(host.engine().reminders_suspended());
    assert_eq!(host.trial_days_left(now), 0);
    assert!(!host.start_short_break_now());
    assert_eq!(host.tray().sink().last_tooltip, "Your trial has ended.");
    let menu = host.tray().sink().last_presentation.as_ref().unwrap();
    assert!(!menu.take_short_break_now.is_enabled);
    assert!(!menu.take_long_break_now.is_enabled);

    // Full license immediately resumes reminders and re-enables tray break actions.
    host.set_license_state(LicenseState::Full, now);
    assert!(!host.engine().reminders_suspended());
    assert_eq!(host.trial_days_left(now), -1);
    let menu_full = host.tray().sink().last_presentation.as_ref().unwrap();
    assert!(menu_full.take_short_break_now.is_enabled);
    assert!(menu_full.take_long_break_now.is_enabled);
    assert!(host.start_short_break_now());

    // Check buy callback plumbing on ManagedSettingsWindow.
    let bought = Rc::new(Cell::new(false));
    let bought_clone = Rc::clone(&bought);
    let mut mgr = ocy_app::settings_logic::ManagedSettingsWindow::with_clock(
        DummyBackend,
        Arc::new(Unsupported),
        clock_dyn,
        Arc::new(Strings::from_embedded().unwrap()),
        ErrorLog::new(&dir, "ocy"),
        ocy_app::settings_logic::SettingsCallbacks {
            on_save: Rc::new(|_| true),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        OcySettings::default(),
    );
    mgr.set_on_buy(Rc::new(move || bought_clone.set(true)));
    mgr.buy();
    assert!(bought.get());

    struct DummyBackend;
    impl ocy_app::settings_logic::SettingsBackend for DummyBackend {
        type Instance = ();
        fn create(
            &self,
            _params: ocy_app::settings_logic::SettingsCreateParams<'_>,
            _handlers: ocy_app::settings_logic::SettingsUiHandlers,
        ) -> Result<Self::Instance, String> {
            Ok(())
        }
        fn activate(&self, _instance: &Self::Instance) {}
        fn set_dark(&self, _instance: &Self::Instance, _dark: bool) {}
        fn read_values(
            &self,
            _instance: &Self::Instance,
        ) -> ocy_app::settings_logic::SettingsValues {
            ocy_app::settings_logic::SettingsValues::from_settings(
                &OcySettings::default(),
                false,
                false,
            )
        }
        fn set_start_with_system(&self, _instance: &Self::Instance, _enabled: bool) {}
        fn set_start_with_system_error(&self, _instance: &Self::Instance, _error: &str) {}
        fn set_save_error(&self, _instance: &Self::Instance, _error: &str) {}
        fn schedule_apply_timer(&self, _delay: Duration, _on_fire: Box<dyn FnOnce()>) {}
        fn destroy_on_close(&self, _instance: Self::Instance) {}
        fn hide_and_destroy(&self, _instance: Self::Instance) {}
    }

    let _ = fs::remove_dir_all(dir);
}
