mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestPlatformServices;
use damt_platform::{FocusState, SessionEvent};
use jiff::civil::Date;
use ocy_core::engine::{BreakEngine, BreakKind, BreakOutcome, EngineEvent, EnginePhase};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

struct MatrixFixture {
    clock: Arc<FakeClock>,
    platform: Arc<TestPlatformServices>,
    engine: BreakEngine,
    absence_credited_count: usize,
    last_absence_credited_kind: Option<BreakKind>,
    break_ended_count: usize,
    short_break_ended_completed_count: usize,
    long_break_ended_completed_count: usize,
    long_break_started_count: usize,
    long_break_warning_started_count: usize,
    event_log: Vec<String>,
}

impl MatrixFixture {
    fn today() -> Date {
        Date::constant(2026, 11, 17)
    }

    fn new() -> Self {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).expect("valid date"));
        let platform = TestPlatformServices::new();

        // C# parity: the C# engine has no natural moments; keep parity tests on C# timing.
        let mut settings = OcySettings {
            natural_moments: false, // C# parity: the C# engine has no natural moments
            ..OcySettings::default()
        };
        settings.short_breaks.interval_minutes = 10;
        settings.short_breaks.duration_seconds = 15;
        settings.long_breaks.interval_minutes = 60;
        settings.long_breaks.duration_minutes = 5;
        settings.long_breaks.warning_seconds = 60;
        settings.away_threshold_seconds = 120;
        settings.pause_during_fullscreen = true;

        let picker = Box::new(DefaultExercisePicker::new(SeededRng::new(42)));
        let engine = BreakEngine::new(
            settings,
            platform.clone(),
            clock.clone(),
            None,
            Some(picker),
            None,
            None,
        );

        Self {
            clock,
            platform,
            engine,
            absence_credited_count: 0,
            last_absence_credited_kind: None,
            break_ended_count: 0,
            short_break_ended_completed_count: 0,
            long_break_ended_completed_count: 0,
            long_break_started_count: 0,
            long_break_warning_started_count: 0,
            event_log: Vec::new(),
        }
    }

    fn record_events(&mut self, events: Vec<EngineEvent>) {
        for ev in events {
            match ev {
                EngineEvent::AbsenceCredited(kind) => {
                    self.absence_credited_count += 1;
                    self.last_absence_credited_kind = Some(kind);
                    self.event_log.push(format!("AbsenceCredited:{kind:?}"));
                }
                EngineEvent::BreakStarted(info) => {
                    if info.kind == BreakKind::Long {
                        self.long_break_started_count += 1;
                    }
                    self.event_log.push(format!("BreakStarted:{:?}", info.kind));
                }
                EngineEvent::BreakEnded(info, outcome) => {
                    self.break_ended_count += 1;
                    if info.kind == BreakKind::Short && outcome == BreakOutcome::Completed {
                        self.short_break_ended_completed_count += 1;
                    } else if info.kind == BreakKind::Long && outcome == BreakOutcome::Completed {
                        self.long_break_ended_completed_count += 1;
                    }
                    self.event_log
                        .push(format!("BreakEnded:{:?}:{outcome:?}", info.kind));
                }
                EngineEvent::LongBreakWarningStarted(_) => {
                    self.long_break_warning_started_count += 1;
                    self.event_log.push("LongBreakWarningStarted".to_owned());
                }
                EngineEvent::PhaseChanged(_) => {}
            }
        }
    }

    fn tick_seconds(&mut self, seconds: usize) {
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            let events = self.engine.tick();
            self.record_events(events);
        }
    }

    fn lock(&mut self) {
        let events = self.engine.on_session_event(SessionEvent::Locked);
        self.record_events(events);
    }

    fn unlock(&mut self) {
        let events = self.engine.on_session_event(SessionEvent::Unlocked);
        self.record_events(events);
    }

    fn suspend(&mut self) {
        let events = self.engine.on_session_event(SessionEvent::Suspending);
        self.record_events(events);
    }

    fn resume_session(&mut self) {
        let events = self.engine.on_session_event(SessionEvent::Resumed);
        self.record_events(events);
    }
}

#[test]
fn matrix_row1_working_500s_locked_60min_lands_in_working_one_absence_credited_counters_reset() {
    let mut f = MatrixFixture::new();

    f.tick_seconds(500);
    assert_eq!(f.engine.phase(), EnginePhase::Working);

    f.lock();
    f.tick_seconds(60 * 60);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.absence_credited_count, 1);
    assert_eq!(f.last_absence_credited_kind, Some(BreakKind::Long));
    assert_eq!(f.engine.since_short_break(), Duration::from_secs(1));
    assert_eq!(f.engine.since_long_break(), Duration::from_secs(1));
}

#[test]
fn matrix_row2_working_500s_locked_2min_lands_in_working_no_credit_short_reset_long_unchanged() {
    let mut f = MatrixFixture::new();

    f.tick_seconds(500);
    assert_eq!(f.engine.phase(), EnginePhase::Working);

    f.lock();
    f.tick_seconds(2 * 60);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.absence_credited_count, 0);
    assert_eq!(f.engine.since_short_break(), Duration::from_secs(1));
    assert_eq!(f.engine.since_long_break(), Duration::from_secs(501));
}

#[test]
fn matrix_row3_long_break_warning_locked_60min_lands_in_working_one_credit_no_long_break_started() {
    let mut f = MatrixFixture::new();

    while f.engine.phase() != EnginePhase::LongBreakWarning {
        f.tick_seconds(1);
    }
    assert_eq!(f.engine.phase(), EnginePhase::LongBreakWarning);

    f.lock();
    f.tick_seconds(60 * 60);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.absence_credited_count, 1);
    assert_eq!(f.long_break_started_count, 0);
    assert_eq!(
        f.engine
            .statistics()
            .get(MatrixFixture::today())
            .map_or(0, |d| d.long_completed),
        0
    );
}

#[test]
fn matrix_row4_short_break_locked_60min_break_ends_completed_then_credits_absence_long_counter_reset()
 {
    let mut f = MatrixFixture::new();

    while f.engine.phase() != EnginePhase::ShortBreak {
        f.tick_seconds(1);
    }
    assert_eq!(f.engine.phase(), EnginePhase::ShortBreak);

    f.lock();
    f.tick_seconds(60 * 60);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.short_break_ended_completed_count, 1);
    assert_eq!(f.absence_credited_count, 1);
    assert_eq!(f.last_absence_credited_kind, Some(BreakKind::Long));
    assert_eq!(f.engine.since_long_break(), Duration::from_secs(1));
    assert_eq!(f.engine.since_short_break(), Duration::from_secs(1));

    let break_ended_idx = f
        .event_log
        .iter()
        .position(|e| e == "BreakEnded:Short:Completed")
        .expect("BreakEnded:Short:Completed logged");
    let absence_idx = f
        .event_log
        .iter()
        .position(|e| e == "AbsenceCredited:Long")
        .expect("AbsenceCredited:Long logged");
    assert!(break_ended_idx < absence_idx);
}

#[test]
fn matrix_row5_long_break_start_now_locked_60min_break_ends_completed_working_long_completed_one() {
    let mut f = MatrixFixture::new();

    let (ok, events) = f.engine.start_long_break_now();
    assert!(ok);
    f.record_events(events);
    assert_eq!(f.engine.phase(), EnginePhase::LongBreak);

    f.lock();
    f.tick_seconds(60 * 60);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.long_break_ended_completed_count, 1);
    assert_eq!(
        f.engine
            .statistics()
            .get(MatrixFixture::today())
            .map_or(0, |d| d.long_completed),
        1
    );
}

#[test]
fn matrix_row6_long_break_start_now_locked_1min_still_long_break_no_break_ended() {
    let mut f = MatrixFixture::new();

    let (ok, events) = f.engine.start_long_break_now();
    assert!(ok);
    f.record_events(events);
    assert_eq!(f.engine.phase(), EnginePhase::LongBreak);

    f.lock();
    f.tick_seconds(60);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(f.break_ended_count, 0);
}

#[test]
fn matrix_row7_paused_pause_for_3h_locked_60min_stays_paused_no_credit() {
    let mut f = MatrixFixture::new();

    let events = f.engine.pause_for(Some(Duration::from_secs(3 * 3600)));
    f.record_events(events);
    assert_eq!(f.engine.phase(), EnginePhase::Paused);

    f.lock();
    f.tick_seconds(60 * 60);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Paused);
    assert_eq!(f.absence_credited_count, 0);
}

#[test]
fn matrix_row8_deferred_fullscreen_locked_60min_normal_before_unlock_working_one_credit_no_warning()
{
    let mut f = MatrixFixture::new();

    f.tick_seconds(3500);
    f.platform.focus.set_state(FocusState::FullscreenApp);
    while f.engine.phase() != EnginePhase::Deferred {
        f.tick_seconds(1);
    }
    assert_eq!(f.engine.phase(), EnginePhase::Deferred);

    f.lock();
    f.tick_seconds(60 * 60);

    f.platform.focus.set_state(FocusState::Normal);

    f.unlock();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.absence_credited_count, 1);
    assert_eq!(f.long_break_warning_started_count, 0);
    assert_eq!(f.engine.since_long_break(), Duration::from_secs(1));
    assert_eq!(f.engine.since_short_break(), Duration::from_secs(1));
}

#[test]
fn matrix_row9_short_break_suspending_60min_break_ends_completed_then_credits_absence() {
    let mut f = MatrixFixture::new();

    while f.engine.phase() != EnginePhase::ShortBreak {
        f.tick_seconds(1);
    }
    assert_eq!(f.engine.phase(), EnginePhase::ShortBreak);

    f.suspend();
    f.tick_seconds(60 * 60);

    f.resume_session();
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.short_break_ended_completed_count, 1);
    assert_eq!(f.absence_credited_count, 1);
    assert_eq!(f.last_absence_credited_kind, Some(BreakKind::Long));
    assert_eq!(f.engine.since_long_break(), Duration::from_secs(1));
    assert_eq!(f.engine.since_short_break(), Duration::from_secs(1));

    let break_ended_idx = f
        .event_log
        .iter()
        .position(|e| e == "BreakEnded:Short:Completed")
        .expect("BreakEnded:Short:Completed logged");
    let absence_idx = f
        .event_log
        .iter()
        .position(|e| e == "AbsenceCredited:Long")
        .expect("AbsenceCredited:Long logged");
    assert!(break_ended_idx < absence_idx);
}

#[test]
fn matrix_focus_not_present_short_break_60min_break_ends_completed_then_credits_absence() {
    let mut f = MatrixFixture::new();

    while f.engine.phase() != EnginePhase::ShortBreak {
        f.tick_seconds(1);
    }
    assert_eq!(f.engine.phase(), EnginePhase::ShortBreak);

    f.platform.focus.set_state(FocusState::NotPresent);
    f.tick_seconds(60 * 60);

    f.platform.focus.set_state(FocusState::Normal);
    f.tick_seconds(1);

    assert_eq!(f.engine.phase(), EnginePhase::Working);
    assert_eq!(f.short_break_ended_completed_count, 1);
    assert_eq!(f.absence_credited_count, 1);
    assert_eq!(f.last_absence_credited_kind, Some(BreakKind::Long));
    assert_eq!(f.engine.since_long_break(), Duration::ZERO);
    assert_eq!(f.engine.since_short_break(), Duration::ZERO);

    f.tick_seconds(1);
    assert_eq!(f.engine.since_long_break(), Duration::from_secs(1));
    assert_eq!(f.engine.since_short_break(), Duration::from_secs(1));

    let break_ended_idx = f
        .event_log
        .iter()
        .position(|e| e == "BreakEnded:Short:Completed")
        .expect("BreakEnded:Short:Completed logged");
    let absence_idx = f
        .event_log
        .iter()
        .position(|e| e == "AbsenceCredited:Long")
        .expect("AbsenceCredited:Long logged");
    assert!(break_ended_idx < absence_idx);
}
