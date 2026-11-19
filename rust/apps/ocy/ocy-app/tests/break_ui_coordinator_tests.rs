use std::sync::Arc;
use std::time::Duration;

use damt_platform::{Clock, PlatformServices, Unsupported};
use ocy_app::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
use ocy_core::engine::{BreakEngine, BreakInfo, EnginePhase, EngineSnapshot};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

#[derive(Debug, Default)]
struct RecordingBreakWindows {
    shown_short_breaks: Vec<BreakInfo>,
    shown_warnings: Vec<Duration>,
    shown_long_breaks: Vec<BreakInfo>,
    countdown_updates: Vec<EngineSnapshot>,
    close_all_call_count: usize,
}

impl BreakWindows for RecordingBreakWindows {
    fn show_short_break(&mut self, info: &BreakInfo) {
        self.shown_short_breaks.push(info.clone());
    }

    fn show_warning(&mut self, countdown: Duration) {
        self.shown_warnings.push(countdown);
    }

    fn show_long_break(&mut self, info: &BreakInfo) {
        self.shown_long_breaks.push(info.clone());
    }

    fn update_countdown(&mut self, snapshot: &EngineSnapshot) {
        self.countdown_updates.push(snapshot.clone());
    }

    fn close_all(&mut self) {
        self.close_all_call_count += 1;
    }
}

#[derive(Debug, Default)]
struct RecordingSettingsWindow {
    show_count: usize,
}

impl SettingsWindow for RecordingSettingsWindow {
    fn show(&mut self) {
        self.show_count += 1;
    }
}

fn setup() -> (
    Arc<FakeClock>,
    OcySettings,
    BreakEngine,
    BreakUiCoordinator<RecordingBreakWindows, RecordingSettingsWindow>,
) {
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).unwrap());
    let mut settings = OcySettings::default();
    settings.short_breaks.interval_minutes = 10;
    settings.short_breaks.duration_seconds = 15;
    settings.long_breaks.interval_minutes = 60;
    settings.long_breaks.duration_minutes = 5;
    settings.long_breaks.warning_seconds = 60;
    settings.long_breaks.postpone_minutes = 3;
    settings.long_breaks.max_postpones = 3;
    settings.away_threshold_seconds = 120;

    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let engine = BreakEngine::new(
        settings.clone(),
        platform,
        clock_dyn,
        None,
        Some(Box::new(DefaultExercisePicker::new(SeededRng::new(42)))),
        None,
        None,
    );
    let coordinator = BreakUiCoordinator::new(
        RecordingBreakWindows::default(),
        RecordingSettingsWindow::default(),
    );
    (clock, settings, engine, coordinator)
}

#[test]
fn flow1_short_break_shows_then_closes() {
    let (clock, _, mut engine, mut coordinator) = setup();

    // 1. Advance time until short break triggers
    for _ in 0..600 {
        clock.advance(Duration::from_secs(1));
        let events = engine.tick();
        coordinator.handle_events(&events);
        coordinator.on_tick(&engine.snapshot());
    }

    assert_eq!(engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(coordinator.break_windows().shown_short_breaks.len(), 1);
    assert!(!coordinator.break_windows().countdown_updates.is_empty());
    assert_eq!(coordinator.break_windows().close_all_call_count, 0);

    // 2. Advance time until short break finishes (15 seconds)
    for _ in 0..15 {
        clock.advance(Duration::from_secs(1));
        let events = engine.tick();
        coordinator.handle_events(&events);
        coordinator.on_tick(&engine.snapshot());
    }

    assert_eq!(engine.phase(), EnginePhase::Working);
    assert!(coordinator.break_windows().close_all_call_count >= 1);
}

#[test]
fn flow2_warning_to_long_break() {
    let (clock, mut settings, mut engine, mut coordinator) = setup();

    // Disable short breaks to isolate long break flow
    settings.short_breaks.enabled = false;
    let events = engine.update_settings(settings);
    coordinator.handle_events(&events);

    // Advance time to 59 minutes: warning begins
    for _ in 0..(59 * 60) {
        clock.advance(Duration::from_secs(1));
        let events = engine.tick();
        coordinator.handle_events(&events);
        coordinator.on_tick(&engine.snapshot());
    }

    assert_eq!(engine.phase(), EnginePhase::LongBreakWarning);
    assert_eq!(coordinator.break_windows().shown_warnings.len(), 1);
    assert_eq!(
        coordinator.break_windows().shown_warnings[0],
        Duration::from_secs(60)
    );

    // Advance 60 seconds of warning countdown
    for _ in 0..60 {
        clock.advance(Duration::from_secs(1));
        let events = engine.tick();
        coordinator.handle_events(&events);
        coordinator.on_tick(&engine.snapshot());
    }

    assert_eq!(engine.phase(), EnginePhase::LongBreak);
    assert_eq!(coordinator.break_windows().shown_long_breaks.len(), 1);
}

#[test]
fn flow3_pause_closes_everything() {
    let (_, _, mut engine, mut coordinator) = setup();

    // Start a short break
    let (started, events) = engine.start_short_break_now();
    coordinator.handle_events(&events);
    assert!(started);
    assert_eq!(engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(coordinator.break_windows().shown_short_breaks.len(), 1);

    let close_count_before = coordinator.break_windows().close_all_call_count;

    // Pause the engine
    let events = engine.pause_for(Some(Duration::from_secs(3600)));
    coordinator.handle_events(&events);

    assert_eq!(engine.phase(), EnginePhase::Paused);
    assert!(coordinator.break_windows().close_all_call_count > close_count_before);
}

#[test]
fn coordinator_show_settings_calls_settings_window() {
    let (_, _, _, mut coordinator) = setup();

    assert_eq!(coordinator.settings_window().show_count, 0);
    coordinator.show_settings();
    assert_eq!(coordinator.settings_window().show_count, 1);
}
