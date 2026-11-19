mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestPlatformServices;
use damt_platform::{FocusState, SessionEvent};
use jiff::civil::Date;
use ocy_core::engine::{
    BreakEngine, BreakInfo, BreakKind, BreakOutcome, EngineEvent, EnginePhase, EngineState,
};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::{FakeClock, Paused};

struct EngineHarness {
    clock: Arc<FakeClock>,
    platform: Arc<TestPlatformServices>,
    settings: OcySettings,
    engine: BreakEngine,
}

impl EngineHarness {
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
        settings.long_breaks.postpone_minutes = 3;
        settings.long_breaks.max_postpones = 3;
        settings.away_threshold_seconds = 120;
        settings.pause_during_fullscreen = true;

        let picker = Box::new(DefaultExercisePicker::new(SeededRng::new(42)));
        let engine = BreakEngine::new(
            settings.clone(),
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
            settings,
            engine,
        }
    }

    fn tick_seconds(&mut self, seconds: usize) -> Vec<EngineEvent> {
        let mut all = Vec::new();
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            all.extend(self.engine.tick());
        }
        all
    }

    fn today() -> Date {
        Date::constant(2026, 11, 17)
    }
}

#[test]
fn requirement1_short_break_fires_completes_and_resets_counter() {
    let mut h = EngineHarness::new();

    let events = h.tick_seconds(600);
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    let started: Option<&BreakInfo> = events.iter().find_map(|e| match e {
        EngineEvent::BreakStarted(info) => Some(info),
        _ => None,
    });
    let started = started.expect("BreakStarted raised");
    assert_eq!(started.kind, BreakKind::Short);
    assert_eq!(started.total_duration, Duration::from_secs(15));

    let events = h.tick_seconds(15);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    let ended = events.iter().find_map(|e| match e {
        EngineEvent::BreakEnded(info, outcome) => Some((info, *outcome)),
        _ => None,
    });
    let (_ended_info, outcome) = ended.expect("BreakEnded raised");
    assert_eq!(outcome, BreakOutcome::Completed);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(10 * 60));
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.short_completed),
        Some(1)
    );
}

#[test]
fn requirement2_long_warning_and_break_fires_completes_and_resets_both_counters() {
    let mut h = EngineHarness::new();
    h.settings.short_breaks.enabled = false;
    h.engine.update_settings(h.settings.clone());

    let events = h.tick_seconds(59 * 60);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
    assert!(
        events.contains(&EngineEvent::LongBreakWarningStarted(Duration::from_secs(
            60
        )))
    );

    let events = h.tick_seconds(60);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    let started = events
        .iter()
        .find_map(|e| match e {
            EngineEvent::BreakStarted(info) => Some(info),
            _ => None,
        })
        .expect("BreakStarted raised");
    assert_eq!(started.kind, BreakKind::Long);
    assert_eq!(started.total_duration, Duration::from_secs(5 * 60));

    let events = h.tick_seconds(5 * 60);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakEnded(_, BreakOutcome::Completed)))
    );
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.long_completed),
        Some(1)
    );
}

#[test]
fn requirement3_short_break_suppressed_when_long_break_due_within_2_min() {
    let h = EngineHarness::new();
    let custom_state = EngineState {
        since_short_break: Duration::from_secs(9 * 60 + 59),
        since_long_break: Duration::from_secs(58 * 60 + 30),
        last_tick_utc: Some(h.clock.utc_now()),
        ..EngineState::default()
    };

    let mut engine = BreakEngine::new(
        h.settings.clone(),
        h.platform.clone(),
        h.clock.clone(),
        None,
        None,
        Some(custom_state),
        None,
    );

    h.clock.advance(Duration::from_secs(1));
    let events = engine.tick();

    assert!(!events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakStarted(info) if info.kind == BreakKind::Short
    )));
    assert_eq!(engine.since_short_break(), Duration::ZERO);
}

#[test]
fn requirement4_idle_detection_and_return_crediting() {
    let mut h = EngineHarness::new();
    h.tick_seconds(300);

    h.platform.idle.set_idle_time(Duration::from_secs(120));
    h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::Away);

    h.platform.idle.set_idle_time(Duration::from_secs(300));
    h.clock.advance(Duration::from_secs(180));
    let events = h.engine.tick();
    assert!(events.is_empty());
    assert_eq!(h.engine.phase(), EnginePhase::Away);

    h.platform.idle.set_idle_time(Duration::ZERO);
    let events = h.tick_seconds(1);

    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(events.contains(&EngineEvent::AbsenceCredited(BreakKind::Long)));
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.absences_credited),
        Some(1)
    );
}

#[test]
fn requirement4b_idle_return_after_short_duration_resets_only_short_counter() {
    let mut h = EngineHarness::new();
    h.tick_seconds(300);
    let since_long_before = h.engine.since_long_break();

    h.platform.idle.set_idle_time(Duration::from_secs(120));
    h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::Away);

    h.platform.idle.set_idle_time(Duration::from_secs(130));
    h.clock.advance(Duration::from_secs(10));
    h.engine.tick();

    h.platform.idle.set_idle_time(Duration::ZERO);
    let events = h.tick_seconds(1);

    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::AbsenceCredited(_)))
    );
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), since_long_before);
}

#[test]
fn requirement5_sleep_gap_treated_as_away() {
    let mut h = EngineHarness::new();
    h.tick_seconds(300);
    let since_long_before = h.engine.since_long_break();

    h.clock.advance(Duration::from_secs(60));
    h.engine.tick();

    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), since_long_before);
}

#[test]
fn requirement6_fullscreen_deferral_short_dropped_long_waits_then_warns() {
    let mut h = EngineHarness::new();
    h.platform.focus.set_state(FocusState::FullscreenApp);

    h.tick_seconds(600);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);

    h.tick_seconds(49 * 60);
    assert_eq!(h.engine.phase(), EnginePhase::Deferred);

    h.tick_seconds(100);
    assert_eq!(h.engine.phase(), EnginePhase::Deferred);

    h.platform.focus.set_state(FocusState::Normal);
    h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
}

#[test]
fn requirement7_postpone_enforces_max_postpones_and_strict_mode() {
    let mut h = EngineHarness::new();
    h.settings.short_breaks.enabled = false;
    h.engine.update_settings(h.settings.clone());

    h.tick_seconds(59 * 60);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // Postpone 1
    assert!(h.engine.postpone_long_break().0);
    assert_eq!(h.engine.postpones_used(), 1);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(57 * 60));

    h.tick_seconds(120);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // Postpone 2
    assert!(h.engine.postpone_long_break().0);
    assert_eq!(h.engine.postpones_used(), 2);

    h.tick_seconds(120);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // Postpone 3
    assert!(h.engine.postpone_long_break().0);
    assert_eq!(h.engine.postpones_used(), 3);

    h.tick_seconds(120);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // Postpone 4: refused (max_postpones reached)
    assert!(!h.engine.postpone_long_break().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // Strict mode refusal
    h.settings.strict_mode = true;
    h.engine.update_settings(h.settings.clone());
    assert!(!h.engine.postpone_long_break().0);
}

#[test]
fn requirement8_skip_refused_for_long_in_strict_mode_allowed_for_short() {
    let mut h = EngineHarness::new();
    h.settings.strict_mode = true;
    h.engine.update_settings(h.settings.clone());

    // 1. In ShortBreak: allowed even in strict mode
    assert!(h.engine.start_short_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert!(h.engine.skip_current_break().0);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.short_skipped),
        Some(1)
    );

    // 2. In LongBreak: refused in strict mode
    assert!(h.engine.start_long_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert!(!h.engine.skip_current_break().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);

    // 3. In LongBreak without strict mode: allowed
    h.settings.strict_mode = false;
    h.engine.update_settings(h.settings.clone());
    assert!(h.engine.skip_current_break().0);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.long_skipped),
        Some(1)
    );
}

#[test]
fn requirement9_pause_for_duration_and_resume() {
    let mut h = EngineHarness::new();
    h.tick_seconds(300);

    h.engine.pause_for(Some(Duration::from_secs(30 * 60)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.clock.advance(Duration::from_secs(10 * 60));
    h.engine.tick();
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.clock.advance(Duration::from_secs(21 * 60));
    h.engine.tick();
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);

    h.tick_seconds(100);
    h.engine.pause_for(None);
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.clock.advance(Duration::from_secs(2 * 3600));
    h.engine.tick();
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.engine.resume();
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);
}

#[test]
fn requirement10_session_locked_to_unlocked_counts_as_away() {
    let mut h = EngineHarness::new();
    h.tick_seconds(300);

    h.engine.on_session_event(SessionEvent::Locked);
    assert_eq!(h.engine.phase(), EnginePhase::Away);

    h.clock.advance(Duration::from_secs(10 * 60));
    let events = h.engine.on_session_event(SessionEvent::Unlocked);

    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(events.contains(&EngineEvent::AbsenceCredited(BreakKind::Long)));
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);
}

#[test]
fn requirement16_disabled_breaks_never_fire() {
    let mut h = EngineHarness::new();
    h.settings.short_breaks.enabled = false;
    h.settings.long_breaks.enabled = false;
    h.engine.update_settings(h.settings.clone());

    let events = h.tick_seconds(7200);
    assert!(!events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakStarted(_) | EngineEvent::LongBreakWarningStarted(_)
    )));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
}

#[test]
fn defect_a_return_from_away_into_fullscreen_app_lands_in_working_counters_grow_no_warning_on_normal()
 {
    let mut h = EngineHarness::new();
    h.tick_seconds(300);

    h.engine.on_session_event(SessionEvent::Locked);
    assert_eq!(h.engine.phase(), EnginePhase::Away);

    h.clock.advance(Duration::from_secs(30 * 60));
    h.platform.focus.set_state(FocusState::FullscreenApp);
    h.engine.on_session_event(SessionEvent::Unlocked);

    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);

    h.tick_seconds(60);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(60));

    h.platform.focus.set_state(FocusState::Normal);
    h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
}

#[test]
fn defect_b_restart_during_active_pause_3_hours_stays_paused() {
    let mut h = EngineHarness::new();
    h.engine.pause_for(Some(Duration::from_secs(3 * 3600)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    let state = h.engine.export_state();
    h.clock.advance(Duration::from_secs(120));

    let mut engine2 = BreakEngine::new(
        h.settings.clone(),
        h.platform.clone(),
        h.clock.clone(),
        None,
        None,
        Some(state.clone()),
        None,
    );
    assert_eq!(engine2.phase(), EnginePhase::Paused);

    engine2.tick();
    assert_eq!(engine2.phase(), EnginePhase::Paused);
    assert_eq!(engine2.paused_until(), state.paused_until_utc);
}

#[test]
fn defect_b_restart_during_active_pause_indefinite_stays_paused() {
    let mut h = EngineHarness::new();
    h.engine.pause_for(None);
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    let state = h.engine.export_state();
    h.clock.advance(Duration::from_secs(600));

    let mut engine2 = BreakEngine::new(
        h.settings.clone(),
        h.platform.clone(),
        h.clock.clone(),
        None,
        None,
        Some(state),
        None,
    );
    assert_eq!(engine2.phase(), EnginePhase::Paused);

    engine2.tick();
    assert_eq!(engine2.phase(), EnginePhase::Paused);
    assert_eq!(engine2.paused_until(), Some(Paused::UntilResumed));
}

#[test]
fn defect_b_restart_after_pause_expired_transitions_to_working_with_counters_reset() {
    let mut h = EngineHarness::new();
    h.tick_seconds(300);

    h.engine.pause_for(Some(Duration::from_secs(600)));
    let state = h.engine.export_state();

    h.clock.advance(Duration::from_secs(1200));

    let mut engine2 = BreakEngine::new(
        h.settings.clone(),
        h.platform.clone(),
        h.clock.clone(),
        None,
        None,
        Some(state),
        None,
    );
    assert_eq!(engine2.phase(), EnginePhase::Working);
    assert_eq!(engine2.since_short_break(), Duration::ZERO);
    assert_eq!(engine2.since_long_break(), Duration::ZERO);
    assert_eq!(engine2.paused_until(), None);

    engine2.tick();
    assert_eq!(engine2.phase(), EnginePhase::Working);
}

#[test]
fn defect_c_start_short_break_now_refused_during_long_break_and_paused_allowed_in_working() {
    let mut h = EngineHarness::new();

    // 1. Refused during LongBreak
    assert!(h.engine.start_long_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);

    assert!(!h.engine.start_short_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.kind),
        Some(BreakKind::Long)
    );

    // 2. Refused during Paused
    h.engine.pause_for(Some(Duration::from_secs(3600)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert!(!h.engine.start_short_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    // 3. Allowed in Working
    h.engine.resume();
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.start_short_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
}

#[test]
fn defect_l1_gap_while_already_away_credits_absence_only_once() {
    let mut h = EngineHarness::new();
    h.tick_seconds(60);

    h.platform.idle.set_idle_time(Duration::from_secs(180));
    let events = h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::Away);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::AbsenceCredited(_)))
    );

    h.clock.advance(Duration::from_secs(7 * 60));
    h.platform.idle.set_idle_time(Duration::from_secs(10 * 60));
    let events = h.engine.tick();
    assert_eq!(h.engine.phase(), EnginePhase::Away);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::AbsenceCredited(_)))
    );

    h.platform.idle.set_idle_time(Duration::ZERO);
    let events = h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    let credit_count = events
        .iter()
        .filter(|e| matches!(e, EngineEvent::AbsenceCredited(BreakKind::Long)))
        .count();
    assert_eq!(credit_count, 1);
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.absences_credited),
        Some(1)
    );
}

#[test]
fn defect_i1_pause_for_during_short_break_raises_break_ended_interrupted_no_stats_incremented() {
    let mut h = EngineHarness::new();

    assert!(h.engine.start_short_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);

    let events = h.engine.pause_for(Some(Duration::from_secs(15 * 60)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded(info, BreakOutcome::Interrupted) if info.kind == BreakKind::Short
    )));

    let stats = h.engine.statistics().get(EngineHarness::today());
    assert_eq!(stats.map_or(0, |d| d.short_completed), 0);
    assert_eq!(stats.map_or(0, |d| d.short_skipped), 0);
}

#[test]
fn defect_i1_pause_for_during_long_break_raises_break_ended_interrupted_no_stats_incremented() {
    let mut h = EngineHarness::new();

    assert!(h.engine.start_long_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);

    let events = h.engine.pause_for(Some(Duration::from_secs(3600)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded(info, BreakOutcome::Interrupted) if info.kind == BreakKind::Long
    )));

    let stats = h.engine.statistics().get(EngineHarness::today());
    assert_eq!(stats.map_or(0, |d| d.long_completed), 0);
    assert_eq!(stats.map_or(0, |d| d.long_skipped), 0);
}

#[test]
fn defect_unlock_unlock_during_long_break_continues_and_completes_normally() {
    let mut h = EngineHarness::new();

    assert!(h.engine.start_long_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.remaining_duration),
        Some(Duration::from_secs(300))
    );

    let mut ended_events = Vec::new();
    ended_events.extend(h.tick_seconds(10));
    ended_events.extend(h.engine.on_session_event(SessionEvent::Locked));
    ended_events.extend(h.tick_seconds(60));

    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.remaining_duration),
        Some(Duration::from_secs(230))
    );

    ended_events.extend(h.engine.on_session_event(SessionEvent::Unlocked));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.remaining_duration),
        Some(Duration::from_secs(230))
    );
    assert!(
        !ended_events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakEnded(_, _)))
    );

    let final_events = h.tick_seconds(230);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.current_break().is_none());
    assert!(final_events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded(info, BreakOutcome::Completed) if info.kind == BreakKind::Long
    )));
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.long_completed),
        Some(1)
    );
}

#[test]
fn defect_unlock_unlock_during_long_break_in_strict_mode_does_not_discard_break() {
    let mut h = EngineHarness::new();
    h.settings.strict_mode = true;
    h.engine.update_settings(h.settings.clone());

    assert!(h.engine.start_long_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);

    let mut events = Vec::new();
    events.extend(h.tick_seconds(15));
    events.extend(h.engine.on_session_event(SessionEvent::Locked));
    events.extend(h.tick_seconds(30));
    events.extend(h.engine.on_session_event(SessionEvent::Unlocked));

    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.remaining_duration),
        Some(Duration::from_secs(255))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakEnded(_, _)))
    );

    let final_events = h.tick_seconds(255);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.current_break().is_none());
    assert!(final_events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded(info, BreakOutcome::Completed) if info.kind == BreakKind::Long
    )));
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.long_completed),
        Some(1)
    );
}

#[test]
fn defect_unlock_unlock_during_short_break_continues_and_completes_normally() {
    let mut h = EngineHarness::new();

    assert!(h.engine.start_short_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.remaining_duration),
        Some(Duration::from_secs(15))
    );

    let mut events = Vec::new();
    events.extend(h.tick_seconds(5));
    events.extend(h.engine.on_session_event(SessionEvent::Locked));
    events.extend(h.tick_seconds(3));
    events.extend(h.engine.on_session_event(SessionEvent::Unlocked));

    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.remaining_duration),
        Some(Duration::from_secs(7))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakEnded(_, _)))
    );

    let final_events = h.tick_seconds(7);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.current_break().is_none());
    assert!(final_events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded(info, BreakOutcome::Completed) if info.kind == BreakKind::Short
    )));
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.short_completed),
        Some(1)
    );
}

#[test]
fn defect_unlock_resumed_during_long_break_break_continues() {
    let mut h = EngineHarness::new();

    assert!(h.engine.start_long_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);

    let mut events = Vec::new();
    events.extend(h.tick_seconds(20));
    events.extend(h.engine.on_session_event(SessionEvent::Suspending));
    events.extend(h.tick_seconds(40));
    events.extend(h.engine.on_session_event(SessionEvent::Resumed));

    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(
        h.engine.current_break().map(|b| b.remaining_duration),
        Some(Duration::from_secs(240))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakEnded(_, _)))
    );

    let final_events = h.tick_seconds(240);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.current_break().is_none());
    assert!(final_events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded(info, BreakOutcome::Completed) if info.kind == BreakKind::Long
    )));
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.long_completed),
        Some(1)
    );
}

#[test]
fn defect_unlock_lock_during_long_break_break_completes_while_locked_then_unlock_lands_in_working()
{
    let mut h = EngineHarness::new();

    assert!(h.engine.start_long_break_now().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);

    let mut all_events = Vec::new();
    all_events.extend(h.tick_seconds(10));
    all_events.extend(h.engine.on_session_event(SessionEvent::Locked));
    all_events.extend(h.tick_seconds(290));

    assert_eq!(h.engine.phase(), EnginePhase::Away);
    assert!(h.engine.current_break().is_none());
    assert_eq!(
        all_events
            .iter()
            .filter(|e| matches!(e, EngineEvent::BreakEnded(_, _)))
            .count(),
        1
    );
    assert!(all_events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded(info, BreakOutcome::Completed) if info.kind == BreakKind::Long
    )));
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map(|d| d.long_completed),
        Some(1)
    );

    all_events.extend(h.tick_seconds(60));
    all_events.extend(h.engine.on_session_event(SessionEvent::Unlocked));

    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(
        all_events
            .iter()
            .filter(|e| matches!(e, EngineEvent::BreakEnded(_, _)))
            .count(),
        1
    );

    h.tick_seconds(10);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(10));
    assert_eq!(h.engine.since_short_break(), Duration::from_secs(10));
}

#[test]
fn defect_pause_pause_1_hour_locked_and_unlocked_stays_paused_paused_until_utc_unchanged() {
    let mut h = EngineHarness::new();
    h.engine.pause_for(Some(Duration::from_secs(3600)));
    let expected_paused_until = h.engine.paused_until();
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.tick_seconds(5);
    h.engine.on_session_event(SessionEvent::Locked);
    h.tick_seconds(30);
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.engine.on_session_event(SessionEvent::Unlocked);
    h.tick_seconds(1);

    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert_eq!(h.engine.paused_until(), expected_paused_until);
}

#[test]
fn defect_pause_pause_1_hour_suspending_and_resumed_stays_paused() {
    let mut h = EngineHarness::new();
    h.engine.pause_for(Some(Duration::from_secs(3600)));
    let expected_paused_until = h.engine.paused_until();
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.tick_seconds(5);
    h.engine.on_session_event(SessionEvent::Suspending);
    h.tick_seconds(30);
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.engine.on_session_event(SessionEvent::Resumed);
    h.tick_seconds(1);

    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert_eq!(h.engine.paused_until(), expected_paused_until);
}

#[test]
fn defect_pause_pause_until_resumed_locked_and_unlocked_stays_paused() {
    let mut h = EngineHarness::new();
    h.engine.pause_for(None);
    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert_eq!(h.engine.paused_until(), Some(Paused::UntilResumed));

    h.tick_seconds(5);
    h.engine.on_session_event(SessionEvent::Locked);
    h.tick_seconds(30);
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.engine.on_session_event(SessionEvent::Unlocked);
    h.tick_seconds(1);

    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert_eq!(h.engine.paused_until(), Some(Paused::UntilResumed));
}

#[test]
fn defect_pause_pause_expires_while_locked_with_ticks_then_unlock_lands_in_working_counters_reset_no_absence_credit()
 {
    let mut h = EngineHarness::new();
    h.tick_seconds(60);
    assert_eq!(h.engine.since_short_break(), Duration::from_secs(60));
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(60));

    h.engine.pause_for(Some(Duration::from_secs(600)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.tick_seconds(5);
    h.engine.on_session_event(SessionEvent::Locked);

    let mut events = h.tick_seconds(900);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.paused_until(), None);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);

    events.extend(h.engine.on_session_event(SessionEvent::Unlocked));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.paused_until(), None);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::AbsenceCredited(_)))
    );
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map_or(0, |d| d.absences_credited),
        0
    );

    h.tick_seconds(10);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_short_break(), Duration::from_secs(10));
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(10));
}

#[test]
fn defect_pause_pause_expires_while_locked_without_ticks_then_unlock_lands_in_working_counters_reset_no_absence_credit()
 {
    let mut h = EngineHarness::new();
    h.tick_seconds(60);

    h.engine.pause_for(Some(Duration::from_secs(600)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);

    h.tick_seconds(5);
    h.engine.on_session_event(SessionEvent::Locked);

    h.clock.advance(Duration::from_secs(15 * 60));
    let mut events = h.engine.on_session_event(SessionEvent::Unlocked);
    events.extend(h.engine.tick());

    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.paused_until(), None);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::AbsenceCredited(_)))
    );
    assert_eq!(
        h.engine
            .statistics()
            .get(EngineHarness::today())
            .map_or(0, |d| d.absences_credited),
        0
    );

    h.tick_seconds(10);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_short_break(), Duration::from_secs(10));
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(10));
}
