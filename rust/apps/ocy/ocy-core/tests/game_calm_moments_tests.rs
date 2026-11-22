//! Tests for the game calm-moment rule: in a gentle game a due break starts only after
//! `GAME_CALM_LULL` of quiet from the keyboard, the mouse, and a game controller, with
//! no `NATURAL_MAX_HOLD` cap; a held start is dropped as `MissedInGame` when the next
//! break of its kind would be due. Outside games the behaviour is unchanged.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestPlatformServices;
use damt_platform::{FocusState, ForegroundObservation, QUNS_BUSY};
use ocy_core::engine::{
    BreakEngine, BreakKind, BreakOutcome, BreakSurface, EngineEvent, EnginePhase, GAME_CALM_LULL,
    NATURAL_LULL, NATURAL_MAX_HOLD, NaturalHoldKind,
};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

const GAME_PATH: &str = r"D:\Games\Demo\Demo.exe";

struct GameHarness {
    clock: Arc<FakeClock>,
    platform: Arc<TestPlatformServices>,
    engine: BreakEngine,
}

impl GameHarness {
    fn new(mutate: impl FnOnce(&mut OcySettings)) -> Self {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 22, 9, 0, 0).expect("valid date"));
        let platform = TestPlatformServices::new();
        let mut settings = OcySettings::default();
        settings.short_breaks.interval_minutes = 2;
        settings.short_breaks.duration_seconds = 20;
        settings.long_breaks.interval_minutes = 15;
        settings.long_breaks.duration_minutes = 1;
        settings.long_breaks.warning_seconds = 30;
        settings.away_threshold_seconds = 30;
        mutate(&mut settings);

        let engine = BreakEngine::new(
            settings,
            platform.clone(),
            clock.clone(),
            None,
            Some(Box::new(DefaultExercisePicker::new(SeededRng::new(11)))),
            None,
            None,
        );
        Self {
            clock,
            platform,
            engine,
        }
    }

    /// Puts a detectable, non-silent game in gentle mode in the foreground.
    fn play_game(&self) {
        self.platform.focus.set_state(FocusState::FullscreenApp);
        self.platform
            .foreground
            .set_observation(ForegroundObservation {
                notification_state: Some(QUNS_BUSY),
                exe_name: Some("Demo.exe".to_owned()),
                exe_path: Some(GAME_PATH.to_owned()),
                process_id: Some(42),
            });
        self.platform
            .foreground
            .set_game_bar_paths(vec![GAME_PATH.to_owned()]);
    }

    /// Advances the fake clock by `seconds` in 1 s steps, ticking once per step.
    fn tick_seconds(&mut self, seconds: usize) -> Vec<EngineEvent> {
        let mut all = Vec::new();
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            all.extend(self.engine.tick());
        }
        all
    }

    fn set_idle(&self, idle: Duration) {
        self.platform.idle.set_idle_time(idle);
    }

    fn set_controller_idle(&self, idle: Duration) {
        self.platform.controller.set_idle_time(idle);
    }

    fn hold_kind(&self) -> Option<NaturalHoldKind> {
        self.engine.snapshot().natural_hold
    }

    fn started_short(events: &[EngineEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakStarted(info) if info.kind == BreakKind::Short))
    }

    fn missed_in_game(events: &[EngineEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, EngineEvent::BreakEnded(_, BreakOutcome::MissedInGame)))
            .count()
    }
}

#[test]
fn game_constant_input_holds_past_sixty_seconds_and_first_calm_starts() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);

    // The short break becomes due and is held: keyboard and controller are both active.
    let events = h.tick_seconds(120);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(!GameHarness::started_short(&events));
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // Constant input far past NATURAL_MAX_HOLD: still held, no forced show in a game.
    let events = h.tick_seconds(90);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(!GameHarness::started_short(&events));
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));
    assert_eq!(NATURAL_MAX_HOLD, Duration::from_secs(60));

    // The first calm moment (3 s of quiet everywhere) starts the gentle card.
    h.set_idle(GAME_CALM_LULL);
    h.set_controller_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(h.hold_kind(), None);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakStarted(info)
            if info.kind == BreakKind::Short && info.surface == BreakSurface::GentleCard
    )));
}

#[test]
fn game_requires_three_seconds_not_the_two_second_lull() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // The outside-game lull (2 s) is not calm enough in a game.
    h.set_idle(NATURAL_LULL);
    h.set_controller_idle(Duration::from_secs(60));
    let events = h.tick_seconds(1);
    assert!(!GameHarness::started_short(&events));
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // 3 s of keyboard/mouse quiet with a quiet controller starts it.
    h.set_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
    assert_eq!(GAME_CALM_LULL, Duration::from_secs(3));
}

#[test]
fn game_controller_activity_alone_holds_the_card() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    // The keyboard and mouse are calm (but not away-idle); only the controller is in use.
    h.set_idle(Duration::from_secs(10));
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    let events = h.tick_seconds(90);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(!GameHarness::started_short(&events));

    // The controller going quiet is what releases the card.
    h.set_controller_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
}

#[test]
fn game_controller_quiet_below_three_seconds_still_holds() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::from_secs(10));
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // 2 s of controller quiet is not yet a calm moment.
    h.set_controller_idle(NATURAL_LULL);
    let events = h.tick_seconds(1);
    assert!(!GameHarness::started_short(&events));
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    h.set_controller_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
}

#[test]
fn game_unsupported_controller_does_not_block_the_card() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.platform.controller.set_supported(false);
    h.set_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // Without controller visibility, keyboard/mouse quiet alone is the calm moment.
    h.set_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
}

#[test]
fn game_calm_required_even_with_natural_moments_off() {
    let mut h = GameHarness::new(|s| s.natural_moments = false);
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);

    // The setting does not turn the game rule off: the start is held while active...
    let events = h.tick_seconds(120);
    assert!(!GameHarness::started_short(&events));
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // ...and the calm moment still releases it.
    h.set_idle(GAME_CALM_LULL);
    h.set_controller_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
}

#[test]
fn game_held_break_dropped_when_next_break_comes_due() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // One full short interval (2 min) of constant input after the break became due:
    // the held one is dropped quietly with its own outcome, exactly once.
    let events = h.tick_seconds(120);
    assert_eq!(GameHarness::missed_in_game(&events), 1);
    assert!(!GameHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    // The next break is due now and waits for its own calm moment (fresh hold).
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));
    assert_eq!(h.engine.since_short_break(), Duration::from_secs(120));

    // It is not a skip and not a postpone: no skip statistics changed.
    let stats = h.engine.statistics();
    assert!(stats.days.is_empty() || stats.days[0].short_skipped == 0);

    // The next one starts at its own calm moment.
    h.set_idle(GAME_CALM_LULL);
    h.set_controller_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
    assert_eq!(GameHarness::missed_in_game(&events), 0);
}

#[test]
fn game_held_break_keeps_waiting_before_the_next_break_is_due() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);

    // 119 s of constant input after the due moment: still within the interval, no drop.
    let events = h.tick_seconds(119);
    assert_eq!(GameHarness::missed_in_game(&events), 0);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));
}

#[test]
fn game_held_long_break_also_waits_and_is_missed_after_its_interval() {
    let mut h = GameHarness::new(|s| {
        s.short_breaks.enabled = false;
        s.long_breaks.warning_seconds = 0;
    });
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);

    // The long break becomes due (15 min) and is held in the game.
    let _ = h.tick_seconds(900);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreak));
    assert_eq!(h.engine.phase(), EnginePhase::Working);

    // A full long interval of constant input later it is dropped as missed, and the
    // next long break is due immediately, waiting for its own calm moment.
    let events = h.tick_seconds(900);
    assert_eq!(GameHarness::missed_in_game(&events), 1);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreak));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_long_break(), Duration::from_secs(900));

    // The calm moment starts the gentle long card.
    h.set_idle(GAME_CALM_LULL);
    h.set_controller_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakStarted(info)
            if info.kind == BreakKind::Long && info.surface == BreakSurface::GentleCard
    )));
}

#[test]
fn game_held_warning_after_countdown_is_missed_and_returns_to_working() {
    let mut h = GameHarness::new(|s| s.short_breaks.enabled = false);
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);

    // The warning becomes due (15 min - 30 s) and is held.
    let _ = h.tick_seconds(870);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreakWarning));
    assert_eq!(h.engine.phase(), EnginePhase::Working);

    // A calm moment starts the warning, which counts down while the user is calm...
    h.set_idle(GAME_CALM_LULL);
    h.set_controller_idle(GAME_CALM_LULL);
    let events = h.tick_seconds(1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::LongBreakWarningStarted(_)))
    );
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // ...the user resumes: the long break at the end of the countdown is held, with
    // no 60 s cap.
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(30);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreak));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // One long interval later the held long break is dropped as missed; the engine
    // returns to Working and the next cycle waits for its own calm moment.
    let events = h.tick_seconds(900);
    assert_eq!(GameHarness::missed_in_game(&events), 1);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    // The next cycle begins: the warning is due again and waits for its own calm moment.
    let _ = h.tick_seconds(1);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreakWarning));
}

#[test]
fn game_explicit_starts_are_never_held() {
    // Explicit short break while a scheduled short break is held in a game.
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    let (started, events) = h.engine.start_short_break_now();
    assert!(started);
    assert!(GameHarness::started_short(&events));
    assert_eq!(h.hold_kind(), None);

    // Explicit long break while a scheduled short break is held in a game.
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    let (started, events) = h.engine.start_long_break_now();
    assert!(started);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakStarted(info) if info.kind == BreakKind::Long))
    );
    assert_eq!(h.hold_kind(), None);
}

#[test]
fn outside_game_sixty_second_cap_still_applies_and_controller_is_ignored() {
    // No game in the foreground; the controller is in constant use.
    let mut h = GameHarness::new(|_| {});
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // 59 s of continuous input: still held.
    let events = h.tick_seconds(59);
    assert!(!GameHarness::started_short(&events));

    // Exactly 60 s after the break became due it starts anyway: outside games the
    // controller does not extend the wait and the cap forces the show.
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
}

#[test]
fn outside_game_two_second_lull_still_starts_and_misses_never_happen() {
    let mut h = GameHarness::new(|_| {});
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);

    // The outside-game lull is still 2 s, controller or not.
    h.set_idle(NATURAL_LULL);
    let events = h.tick_seconds(1);
    assert!(GameHarness::started_short(&events));

    // And a held start outside games is never dropped as MissedInGame.
    let mut h = GameHarness::new(|_| {});
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    let events = h.tick_seconds(120);
    assert!(GameHarness::started_short(&events));
    assert_eq!(GameHarness::missed_in_game(&events), 0);
}

#[test]
fn game_left_mid_hold_falls_back_to_the_outside_game_cap() {
    let mut h = GameHarness::new(|_| {});
    h.play_game();
    h.set_idle(Duration::ZERO);
    h.set_controller_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // The user leaves the game mid-hold with input still flowing: the outside-game
    // 60 s cap applies to the existing hold, so the break starts rather than waiting
    // for a game calm moment that no longer applies.
    h.platform.focus.set_state(FocusState::Normal);
    h.platform
        .foreground
        .set_observation(ForegroundObservation::unknown());
    h.platform.foreground.set_game_bar_paths(Vec::new());
    let events = h.tick_seconds(60);
    assert!(GameHarness::started_short(&events));
    assert_eq!(GameHarness::missed_in_game(&events), 0);
}
