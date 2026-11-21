//! Tests for natural-moment timing (`natural_moments`): scheduled starts wait for a short
//! lull in input (up to `NATURAL_MAX_HOLD`), explicit starts never wait.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestPlatformServices;
use damt_platform::SessionEvent;
use ocy_core::engine::{
    BreakEngine, BreakKind, EngineEvent, EnginePhase, NATURAL_LULL, NATURAL_MAX_HOLD,
    NaturalHoldKind,
};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;
use proptest::prelude::*;

struct NaturalHarness {
    clock: Arc<FakeClock>,
    platform: Arc<TestPlatformServices>,
    engine: BreakEngine,
}

impl NaturalHarness {
    fn new(natural_moments: bool) -> Self {
        Self::with_settings(|settings| settings.natural_moments = natural_moments)
    }

    fn with_settings(mutate: impl FnOnce(&mut OcySettings)) -> Self {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 9, 0, 0).expect("valid date"));
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
            Some(Box::new(DefaultExercisePicker::new(SeededRng::new(7)))),
            None,
            None,
        );
        Self {
            clock,
            platform,
            engine,
        }
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

    fn hold_kind(&self) -> Option<NaturalHoldKind> {
        self.engine.snapshot().natural_hold
    }

    fn started_short(events: &[EngineEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakStarted(info) if info.kind == BreakKind::Short))
    }
}

#[test]
fn natural_moments_active_typist_held_until_first_two_second_lull() {
    let mut h = NaturalHarness::new(true);
    h.set_idle(Duration::ZERO);

    // Work up to the short-break due moment; the user types continuously.
    let events = h.tick_seconds(120);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(!NaturalHarness::started_short(&events));
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // Still typing: the hold persists.
    let events = h.tick_seconds(30);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(!NaturalHarness::started_short(&events));
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // The first 2 s lull releases the break.
    h.set_idle(NATURAL_LULL);
    let events = h.tick_seconds(1);
    assert!(NaturalHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(h.hold_kind(), None);
}

#[test]
fn natural_moments_continuous_input_starts_at_exactly_sixty_seconds() {
    let mut h = NaturalHarness::new(true);
    h.set_idle(Duration::ZERO);

    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // 59 further seconds of continuous input: still held.
    let events = h.tick_seconds(59);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(!NaturalHarness::started_short(&events));

    // Exactly 60 s after the break became due, it starts regardless of activity.
    let events = h.tick_seconds(1);
    assert!(NaturalHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(NATURAL_MAX_HOLD, Duration::from_secs(60));
}

#[test]
fn natural_moments_idle_unsupported_starts_immediately() {
    let mut h = NaturalHarness::new(true);
    h.platform.idle.set_supported(false);
    h.set_idle(Duration::ZERO);

    let events = h.tick_seconds(120);
    assert!(NaturalHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(h.hold_kind(), None);
}

#[test]
fn natural_moments_setting_off_starts_immediately() {
    let mut h = NaturalHarness::new(false);
    h.set_idle(Duration::ZERO);

    let events = h.tick_seconds(120);
    assert!(NaturalHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(h.hold_kind(), None);
}

#[test]
fn natural_moments_explicit_starts_never_hold() {
    // Explicit short break while a scheduled short break is being held.
    let mut h = NaturalHarness::new(true);
    h.set_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    let (started, events) = h.engine.start_short_break_now();
    assert!(started);
    assert!(NaturalHarness::started_short(&events));
    assert_eq!(h.hold_kind(), None);

    // Explicit long break while a scheduled short break is being held.
    let mut h = NaturalHarness::new(true);
    h.set_idle(Duration::ZERO);
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

    // Explicit long break while the scheduled long break is being held after the warning.
    let mut h = NaturalHarness::with_settings(|s| {
        s.natural_moments = true;
        s.short_breaks.enabled = false;
    });
    h.set_idle(Duration::from_secs(5)); // a lull: the warning starts on schedule
    let events = h.tick_seconds(870);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::LongBreakWarningStarted(_)))
    );
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    h.set_idle(Duration::ZERO); // active again: the long break is held at the countdown end
    let _ = h.tick_seconds(30);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreak));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

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
fn natural_moments_strict_mode_holds_but_never_skips() {
    let mut h = NaturalHarness::with_settings(|s| {
        s.natural_moments = true;
        s.strict_mode = true;
    });
    h.set_idle(Duration::ZERO);

    let _ = h.tick_seconds(120);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // Strict mode delays, never skips: the lull still releases the break.
    h.set_idle(NATURAL_LULL);
    let events = h.tick_seconds(1);
    assert!(NaturalHarness::started_short(&events));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
}

#[test]
fn natural_moments_away_lock_and_pause_drop_the_hold() {
    // Away (idle beyond the away threshold) drops the hold; normal away rules apply.
    let mut h = NaturalHarness::new(true);
    h.set_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    h.set_idle(Duration::from_secs(35));
    let _ = h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::Away);
    assert_eq!(h.hold_kind(), None);

    // Return: the away span is credited like today and a fresh hold starts at the next due.
    h.set_idle(Duration::ZERO);
    let _ = h.tick_seconds(1);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // A session lock drops the hold exactly like away.
    let mut h = NaturalHarness::new(true);
    h.set_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    let _ = h.engine.on_session_event(SessionEvent::Locked);
    assert_eq!(h.engine.phase(), EnginePhase::Away);
    assert_eq!(h.hold_kind(), None);

    // Pausing drops the hold; after resuming, the next due moment holds afresh.
    let mut h = NaturalHarness::new(true);
    h.set_idle(Duration::ZERO);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    let _ = h.engine.pause_for(Some(Duration::from_secs(60)));
    assert_eq!(h.engine.phase(), EnginePhase::Paused);
    assert_eq!(h.hold_kind(), None);

    let _ = h.engine.resume();
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    let _ = h.tick_seconds(120);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));
}

#[test]
fn natural_moments_warning_and_long_break_are_also_held() {
    // The scheduled long-break warning waits for a lull.
    let mut h = NaturalHarness::with_settings(|s| {
        s.natural_moments = true;
        s.short_breaks.enabled = false;
    });
    h.set_idle(Duration::ZERO);
    let events = h.tick_seconds(870);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::LongBreakWarningStarted(_)))
    );
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreakWarning));

    // A lull releases the warning, which then counts down normally.
    h.set_idle(NATURAL_LULL);
    let events = h.tick_seconds(1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::LongBreakWarningStarted(_)))
    );
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);

    // The long break at the end of the warning is held too while the user is active.
    h.set_idle(Duration::ZERO);
    let _ = h.tick_seconds(30);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreak));

    h.set_idle(NATURAL_LULL);
    let events = h.tick_seconds(1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakStarted(info) if info.kind == BreakKind::Long))
    );
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
}

#[test]
fn natural_moments_kind_change_keeps_the_original_hold_deadline() {
    // Short break due at 10 min; long-break warning due at 10 min + 50 s
    // (15 min interval - 250 s warning); long break due at 15 min.
    let mut h = NaturalHarness::with_settings(|s| {
        s.natural_moments = true;
        s.short_breaks.interval_minutes = 10;
        s.long_breaks.interval_minutes = 15;
        s.long_breaks.warning_seconds = 250;
    });
    h.set_idle(Duration::ZERO);

    // The short break becomes due and is held.
    let _ = h.tick_seconds(600);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // 50 s later the long-break warning becomes due: the hold switches kind but keeps
    // the original deadline instead of restarting a fresh 60 s cap.
    let events = h.tick_seconds(50);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, EngineEvent::LongBreakWarningStarted(_)))
    );
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreakWarning));

    // The warning starts within the remaining ~10 s of the original 60 s cap.
    let events = h.tick_seconds(10);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::LongBreakWarningStarted(_)))
    );
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
    assert_eq!(h.hold_kind(), None);
}

#[test]
fn natural_moments_continuous_typing_across_kind_change_totals_sixty_seconds() {
    let mut h = NaturalHarness::with_settings(|s| {
        s.natural_moments = true;
        s.short_breaks.interval_minutes = 10;
        s.long_breaks.interval_minutes = 15;
        s.long_breaks.warning_seconds = 250;
    });
    h.set_idle(Duration::ZERO);

    // The short break becomes due and is held.
    let _ = h.tick_seconds(600);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

    // Continuous typing across the kind change: the warning must start no later than
    // 60 s after the first moment something became due.
    let mut warning_started_at = 0;
    for offset in 1..=60 {
        let events = h.tick_seconds(1);
        if events
            .iter()
            .any(|e| matches!(e, EngineEvent::LongBreakWarningStarted(_)))
        {
            warning_started_at = offset;
            break;
        }
        // Mid-hold the kind switches without a fresh cap.
        if offset == 55 {
            assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreakWarning));
        }
    }
    assert_eq!(warning_started_at, 60);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
    assert_eq!(h.hold_kind(), None);

    // After the warning started, the long break at the end of its countdown holds
    // afresh with its own 60 s cap.
    let _ = h.tick_seconds(250);
    assert_eq!(h.hold_kind(), Some(NaturalHoldKind::LongBreak));
    let events = h.tick_seconds(60);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::BreakStarted(info) if info.kind == BreakKind::Long))
    );
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert_eq!(NATURAL_MAX_HOLD, Duration::from_secs(60));
}

#[test]
fn natural_moments_json_default_omitted_false_written_both_read() {
    // Default (true) is omitted so files stay byte-identical to the C# app's.
    let default_json = serde_json::to_string(&OcySettings::default()).expect("serialize");
    assert!(!default_json.contains("naturalMoments"));

    // Explicit false is written.
    let settings = OcySettings {
        natural_moments: false,
        ..OcySettings::default()
    };
    let json = serde_json::to_string(&settings).expect("serialize");
    assert!(json.contains("\"naturalMoments\":false"));

    // Reading: absent -> default true; false -> false; true -> true.
    let base = r#"{"schemaVersion":1,"language":"en"}"#;
    let parsed: OcySettings = serde_json::from_str(base).expect("parse");
    assert!(parsed.natural_moments);

    let parsed: OcySettings = serde_json::from_str(&json).expect("parse");
    assert!(!parsed.natural_moments);

    let settings = OcySettings {
        natural_moments: true,
        ..OcySettings::default()
    };
    let json = serde_json::to_string(&settings).expect("serialize");
    let parsed: OcySettings = serde_json::from_str(&json).expect("parse");
    assert!(parsed.natural_moments);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// With natural moments on, a due break starts at the first 2 s lull, or exactly
    /// 60 s after it became due — never earlier while the user is active.
    #[test]
    fn held_break_starts_only_at_lull_or_sixty_second_deadline(
        idle_pattern in prop::collection::vec(0u64..=3, 80),
    ) {
        let mut h = NaturalHarness::new(true);
        h.set_idle(Duration::ZERO);
        let _ = h.tick_seconds(120);
        prop_assert_eq!(h.hold_kind(), Some(NaturalHoldKind::ShortBreak));

        let mut started_at: Option<usize> = None;
        for (offset, idle_secs) in idle_pattern.iter().enumerate() {
            h.set_idle(Duration::from_secs(*idle_secs));
            let events = h.tick_seconds(1);
            let started = NaturalHarness::started_short(&events);
            if started {
                started_at = Some(offset + 1);
                break;
            }
            if *idle_secs >= NATURAL_LULL.as_secs() {
                // A lull must release the break on that very tick.
                prop_assert!(false, "lull at offset {} did not release the hold", offset);
            }
            prop_assert!(offset + 1 < 60, "hold exceeded the 60 s maximum");
        }

        let started_at = started_at.expect("break started within the idle pattern");
        prop_assert!(started_at <= 60);
        // If it started while the user was still active, only the deadline can have fired.
        if started_at < 60 {
            prop_assert!(idle_pattern[started_at - 1] >= NATURAL_LULL.as_secs());
        }
    }

    /// With natural moments off, the break starts exactly when due, whatever the input.
    #[test]
    fn setting_off_ignores_idle_pattern(
        idle_pattern in prop::collection::vec(0u64..=5, 120),
    ) {
        let mut h = NaturalHarness::new(false);
        let mut started_at: Option<usize> = None;
        for (offset, idle_secs) in idle_pattern.iter().enumerate() {
            h.set_idle(Duration::from_secs(*idle_secs));
            let events = h.tick_seconds(1);
            if NaturalHarness::started_short(&events) {
                started_at = Some(offset + 1);
                break;
            }
        }
        prop_assert_eq!(started_at, Some(120));
    }
}
