use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use damt_platform::{
    Clock, Notifier, PlatformError, PlatformServices, SessionEvent, TaskbarThemeProvider,
};
use ocy_app::STATUS_NOTIFICATIONS_SHIPPED;
use ocy_app::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
use ocy_app::host::AppHost;
use ocy_app::notifications::{NotificationPresenter, RecordingNotifier};
use ocy_app::strings::Strings;
use ocy_app::tray::{
    AppTheme, TrayController, TrayIconState, TrayPresentation, TrayPresenter, TraySink,
};
use ocy_core::engine::{
    BreakEngine, BreakInfo, BreakKind, BreakOutcome, EngineEvent, EnginePhase, EngineSnapshot,
};
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

#[derive(Debug, Default)]
struct DummyBreakWindows;

impl BreakWindows for DummyBreakWindows {
    fn show_short_break(&mut self, _info: &BreakInfo) {}
    fn show_warning(&mut self, _countdown: Duration) {}
    fn show_long_break(&mut self, _info: &BreakInfo) {}
    fn update_countdown(&mut self, _snapshot: &EngineSnapshot) {}
    fn close_all(&mut self) {}
}

#[derive(Debug, Default)]
struct DummySettingsWindow;

impl SettingsWindow for DummySettingsWindow {
    fn show(&mut self) {}
}

#[derive(Debug, Default)]
struct DummyTraySink;

impl TraySink for DummyTraySink {
    fn set_tooltip(&mut self, _tooltip: &str) {}
    fn set_icon(&mut self, _state: TrayIconState, _use_light_set: bool) {}
    fn set_menu(&mut self, _presentation: &TrayPresentation) {}
}

#[derive(Debug)]
struct StaticTaskbarTheme(Option<bool>);

impl TaskbarThemeProvider for StaticTaskbarTheme {
    fn is_supported(&self) -> bool {
        true
    }
    fn is_light_taskbar(&self) -> Option<bool> {
        self.0
    }
}

/// Mock platform whose notifier lazily registers an AUMID on the first actual `notify()` call.
#[derive(Debug)]
struct MockPlatformWithLazyAumid {
    registration_attempts: AtomicUsize,
    notify_calls: AtomicUsize,
    registered: AtomicBool,
    sent: std::sync::Mutex<Vec<(String, String)>>,
    fail_with: Option<String>,
}

impl MockPlatformWithLazyAumid {
    fn new() -> Self {
        Self {
            registration_attempts: AtomicUsize::new(0),
            notify_calls: AtomicUsize::new(0),
            registered: AtomicBool::new(false),
            sent: std::sync::Mutex::new(Vec::new()),
            fail_with: None,
        }
    }

    fn failing(message: &str) -> Self {
        Self {
            fail_with: Some(message.to_owned()),
            ..Self::new()
        }
    }

    fn registration_attempts(&self) -> usize {
        self.registration_attempts.load(Ordering::SeqCst)
    }

    fn notify_calls(&self) -> usize {
        self.notify_calls.load(Ordering::SeqCst)
    }

    fn sent(&self) -> Vec<(String, String)> {
        self.sent.lock().unwrap().clone()
    }
}

impl Notifier for MockPlatformWithLazyAumid {
    fn is_supported(&self) -> bool {
        true
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), PlatformError> {
        self.notify_calls.fetch_add(1, Ordering::SeqCst);
        if !self.registered.swap(true, Ordering::SeqCst) {
            self.registration_attempts.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(err) = &self.fail_with {
            return Err(PlatformError(err.clone()));
        }
        self.sent
            .lock()
            .unwrap()
            .push((title.to_owned(), body.to_owned()));
        Ok(())
    }
}

impl PlatformServices for MockPlatformWithLazyAumid {
    fn notifier(&self) -> &dyn Notifier {
        self
    }
}

struct ArcMockNotifier(Arc<MockPlatformWithLazyAumid>);

impl Notifier for ArcMockNotifier {
    fn is_supported(&self) -> bool {
        self.0.is_supported()
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), PlatformError> {
        self.0.notify(title, body)
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-notify-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn working_snapshot(short_secs: Option<u64>, long_secs: Option<u64>) -> EngineSnapshot {
    EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: short_secs.map(Duration::from_secs),
        time_until_long_break: long_secs.map(Duration::from_secs),
        time_until_long_break_warning: long_secs.map(|s| Duration::from_secs(s.saturating_sub(60))),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        natural_hold: None,
    }
}

fn build_host(
    dir: &PathBuf,
    settings: OcySettings,
    clock: Arc<FakeClock>,
    mock_platform: Arc<MockPlatformWithLazyAumid>,
    shipped: bool,
) -> AppHost<DummyBreakWindows, DummySettingsWindow, DummyTraySink> {
    let clock_dyn: Arc<dyn Clock> = clock;
    let store = AppDataStore::new(dir, Arc::clone(&clock_dyn));
    let platform_dyn: Arc<dyn PlatformServices> = Arc::clone(&mock_platform) as _;
    let engine = BreakEngine::new(
        settings.clone(),
        platform_dyn,
        Arc::clone(&clock_dyn),
        Some(store),
        None,
        None,
        None,
    );
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_language(&settings.language));
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme: Rc<dyn TaskbarThemeProvider> = Rc::new(StaticTaskbarTheme(Some(true)));
    let tray = TrayController::new(
        presenter,
        DummyTraySink,
        theme,
        clock_dyn,
        Some(AppTheme::Light),
        &snapshot,
    );
    let coordinator = BreakUiCoordinator::new(DummyBreakWindows, DummySettingsWindow);
    let error_log = damt_app::ErrorLog::new(dir, "ocy");
    let store_for_host = AppDataStore::new(dir, Arc::new(damt_platform::SystemClock));

    AppHost::with_store(engine, coordinator, tray, error_log, Some(store_for_host))
        .with_notifier(Rc::new(ArcMockNotifier(mock_platform)))
        .with_notification_ship_gate(shipped)
}

#[test]
fn ship_gate_constant_defaults_to_false_and_blocks_all_notifications() {
    const { assert!(!STATUS_NOTIFICATIONS_SHIPPED) };

    let strings = Arc::new(Strings::from_embedded().unwrap());
    let mut presenter = NotificationPresenter::with_ship_gate(strings);
    let settings = OcySettings {
        status_notifications: true,
        ..OcySettings::default()
    };
    let snap = working_snapshot(Some(600), Some(3600));

    assert_eq!(presenter.on_startup_ready(&settings, &snap), None);
    assert_eq!(
        presenter.on_engine_events(
            &settings,
            &snap,
            &[EngineEvent::AbsenceCredited(BreakKind::Long)]
        ),
        None
    );
}

#[test]
fn presenter_setting_off_emits_nothing() {
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let mut presenter = NotificationPresenter::new(strings, true);
    let settings = OcySettings {
        status_notifications: false,
        ..OcySettings::default()
    };
    let snap = working_snapshot(Some(600), Some(3600));

    assert_eq!(presenter.on_startup_ready(&settings, &snap), None);
    assert!(!presenter.has_sent_startup());
    assert_eq!(
        presenter.on_engine_events(
            &settings,
            &snap,
            &[EngineEvent::AbsenceCredited(BreakKind::Long)]
        ),
        None
    );
}

#[test]
fn presenter_startup_fires_once_with_expected_key_and_next_break() {
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let mut presenter = NotificationPresenter::new(strings, true);
    let settings = OcySettings::default();
    let snap = working_snapshot(Some(600), Some(3600));

    let first = presenter.on_startup_ready(&settings, &snap).unwrap();
    assert_eq!(first.key, "notify.started");
    assert_eq!(first.title, "Ocy");
    assert_eq!(first.body, "Ocy is ready. Your next break is in 10 min.");
    assert!(presenter.has_sent_startup());

    assert_eq!(presenter.on_startup_ready(&settings, &snap), None);
}

#[test]
fn presenter_absence_event_notifies_and_ignores_unrelated_events() {
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = NotificationPresenter::new(strings, true);
    let settings = OcySettings::default();
    let snap = working_snapshot(Some(600), Some(3600));

    let unrelated = [
        EngineEvent::PhaseChanged(snap.clone()),
        EngineEvent::BreakStarted(BreakInfo {
            kind: BreakKind::Short,
            exercise_id: "blink".to_owned(),
            total_duration: Duration::from_secs(15),
            suggestion_exercise_ids: None,
        }),
        EngineEvent::BreakEnded(
            BreakInfo {
                kind: BreakKind::Short,
                exercise_id: "blink".to_owned(),
                total_duration: Duration::from_secs(15),
                suggestion_exercise_ids: None,
            },
            BreakOutcome::Completed,
        ),
        EngineEvent::LongBreakWarningStarted(Duration::from_secs(60)),
    ];
    assert_eq!(
        presenter.on_engine_events(&settings, &snap, &unrelated),
        None
    );

    let credited = presenter
        .on_engine_events(
            &settings,
            &snap,
            &[
                EngineEvent::AbsenceCredited(BreakKind::Long),
                EngineEvent::PhaseChanged(snap.clone()),
            ],
        )
        .unwrap();
    assert_eq!(credited.key, "notify.absenceCredited");
    assert_eq!(credited.title, "Ocy");
    assert_eq!(
        credited.body,
        "Your time away counted as a long break. The next one is in 1 h."
    );
}

#[test]
fn presenter_suppresses_during_breaks_strict_warning_paused_and_away() {
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let mut presenter = NotificationPresenter::new(strings, true);
    let settings = OcySettings::default();
    let absence = [EngineEvent::AbsenceCredited(BreakKind::Long)];

    for phase in [
        EnginePhase::ShortBreak,
        EnginePhase::LongBreak,
        EnginePhase::Paused,
        EnginePhase::Away,
    ] {
        let mut snap = working_snapshot(Some(600), Some(3600));
        snap.phase = phase;
        assert_eq!(
            presenter.on_startup_ready(&settings, &snap),
            None,
            "startup must be suppressed in {phase:?}"
        );
        assert_eq!(
            presenter.on_engine_events(&settings, &snap, &absence),
            None,
            "absence must be suppressed in {phase:?}"
        );
    }

    // Strict-mode warning suppresses notifications, non-strict warning does not.
    let mut strict_warning = working_snapshot(Some(60), Some(60));
    strict_warning.phase = EnginePhase::LongBreakWarning;
    strict_warning.strict_mode = true;
    assert_eq!(presenter.on_startup_ready(&settings, &strict_warning), None);
    assert_eq!(
        presenter.on_engine_events(&settings, &strict_warning, &absence),
        None
    );

    let mut non_strict_warning = strict_warning;
    non_strict_warning.strict_mode = false;
    assert!(
        presenter
            .on_startup_ready(&settings, &non_strict_warning)
            .is_some()
    );
}

#[test]
fn aumid_registration_is_never_attempted_when_setting_or_ship_gate_is_off() {
    for (shipped, setting_on) in [(false, true), (true, false), (false, false)] {
        let dir = temp_dir("no-reg");
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 20, 10, 0, 0).unwrap());
        let mock = Arc::new(MockPlatformWithLazyAumid::new());
        let settings = OcySettings {
            status_notifications: setting_on,
            ..OcySettings::default()
        };

        let mut host = build_host(&dir, settings, clock.clone(), Arc::clone(&mock), shipped);

        // Startup timer fires.
        host.on_startup_notification_timer();
        // Long absence credited via session lock/unlock (> 5 min).
        host.on_session_event(SessionEvent::Locked);
        clock.advance(Duration::from_secs(360));
        host.on_session_event(SessionEvent::Unlocked);

        assert_eq!(
            mock.registration_attempts(),
            0,
            "AUMID registration must never run when shipped={shipped} setting={setting_on}"
        );
        assert_eq!(mock.notify_calls(), 0);
        assert!(mock.sent().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn host_triggers_startup_once_absence_credited_and_live_language_switch() {
    let dir = temp_dir("host-flow");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 20, 10, 0, 0).unwrap());
    let mock = Arc::new(MockPlatformWithLazyAumid::new());
    let settings = OcySettings {
        status_notifications: true,
        language: "en".to_owned(),
        ..OcySettings::default()
    };

    let mut host = build_host(
        &dir,
        settings.clone(),
        clock.clone(),
        Arc::clone(&mock),
        true,
    );

    // No registration at host construction.
    assert_eq!(mock.registration_attempts(), 0);

    // Startup notification fires once, triggering lazy registration once.
    host.on_startup_notification_timer();
    host.on_startup_notification_timer();
    assert_eq!(mock.registration_attempts(), 1);
    assert_eq!(mock.notify_calls(), 1);

    // Switch language to German live via save_settings.
    let updated = OcySettings {
        language: "de".to_owned(),
        ..settings
    };
    assert!(host.save_settings(updated));

    // Long absence via session lock >= 5 min emits localized German notification.
    host.on_session_event(SessionEvent::Locked);
    clock.advance(Duration::from_secs(360));
    host.on_session_event(SessionEvent::Unlocked);

    assert_eq!(mock.registration_attempts(), 1);
    let sent = mock.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[0],
        (
            "Ocy".to_owned(),
            "Ocy is ready. Your next break is in 25 min.".to_owned()
        )
    );
    assert_eq!(
        sent[1],
        (
            "Ocy".to_owned(),
            "Deine Abwesenheit wurde als lange Pause angerechnet. Die nächste folgt in 1 Std."
                .to_owned()
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn host_logs_notification_failure_once_and_continues() {
    let dir = temp_dir("log-once");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 20, 10, 0, 0).unwrap());
    let mock = Arc::new(MockPlatformWithLazyAumid::failing("toast unavailable"));
    let settings = OcySettings {
        status_notifications: true,
        ..OcySettings::default()
    };

    let mut host = build_host(&dir, settings, clock.clone(), Arc::clone(&mock), true);

    host.on_startup_notification_timer();
    host.on_session_event(SessionEvent::Locked);
    clock.advance(Duration::from_secs(360));
    host.on_session_event(SessionEvent::Unlocked);

    assert_eq!(mock.notify_calls(), 2);
    let log_text = fs::read_to_string(dir.join("ocy-errors.log")).unwrap();
    let matches = log_text.matches("status_notification").count();
    assert_eq!(
        matches, 1,
        "expected exactly 1 logged failure, got:\n{log_text}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn recording_notifier_acts_as_isolated_no_op_recorder() {
    let recorder = RecordingNotifier::new();
    assert!(recorder.is_supported());
    assert!(recorder.is_empty());

    recorder.notify("Ocy", "Test body").unwrap();
    assert_eq!(recorder.len(), 1);
    assert_eq!(recorder.recorded()[0].title, "Ocy");
    assert_eq!(recorder.recorded()[0].body, "Test body");

    recorder.clear();
    assert!(recorder.is_empty());
}
