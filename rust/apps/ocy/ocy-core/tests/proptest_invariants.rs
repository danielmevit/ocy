mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestPlatformServices;
use damt_platform::{FocusState, SessionEvent};
use ocy_core::engine::{BreakEngine, EngineEvent, EnginePhase};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Action {
    Tick(u64),
    StartShort,
    StartLong,
    Postpone,
    Skip,
    Pause(Option<u64>),
    Resume,
    Session(SessionEvent),
    SetIdle(u64),
    SetFocus(FocusState),
    ToggleStrict(bool),
}

fn arb_session_event() -> impl Strategy<Value = SessionEvent> {
    prop_oneof![
        Just(SessionEvent::Locked),
        Just(SessionEvent::Unlocked),
        Just(SessionEvent::Suspending),
        Just(SessionEvent::Resumed),
        Just(SessionEvent::LoggingOff),
    ]
}

fn arb_focus_state() -> impl Strategy<Value = FocusState> {
    prop_oneof![
        Just(FocusState::Normal),
        Just(FocusState::FullscreenApp),
        Just(FocusState::Presentation),
        Just(FocusState::QuietTime),
        Just(FocusState::NotPresent),
        Just(FocusState::Unknown),
    ]
}

fn arb_action() -> impl Strategy<Value = Action> {
    prop_oneof![
        (1u64..=1200).prop_map(Action::Tick),
        Just(Action::StartShort),
        Just(Action::StartLong),
        Just(Action::Postpone),
        Just(Action::Skip),
        prop_oneof![Just(None), (30u64..=3600).prop_map(Some)].prop_map(Action::Pause),
        Just(Action::Resume),
        arb_session_event().prop_map(Action::Session),
        (0u64..=600).prop_map(Action::SetIdle),
        arb_focus_state().prop_map(Action::SetFocus),
        any::<bool>().prop_map(Action::ToggleStrict),
    ]
}

#[derive(Debug, Clone, Copy)]
enum AwayTrigger {
    Idle,
    Locked,
    Suspending,
    FocusNotPresent,
}

fn arb_away_trigger() -> impl Strategy<Value = AwayTrigger> {
    prop_oneof![
        Just(AwayTrigger::Idle),
        Just(AwayTrigger::Locked),
        Just(AwayTrigger::Suspending),
        Just(AwayTrigger::FocusNotPresent),
    ]
}

proptest! {
    #[test]
    fn proptest_counters_never_go_negative_and_stay_consistent(
        actions in prop::collection::vec(arb_action(), 1..80)
    ) {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).expect("valid"));
        let platform = TestPlatformServices::new();
        // C# parity: the C# engine has no natural moments; keep parity tests on C# timing.
        let mut settings = OcySettings {
            natural_moments: false, // C# parity: the C# engine has no natural moments
            ..OcySettings::default()
        };
        let mut engine = BreakEngine::with_defaults(settings.clone(), platform.clone(), clock.clone());

        let mut prev_active_minutes = 0u32;
        let mut prev_absences_credited = 0u32;

        for action in actions {
            match action {
                Action::Tick(secs) => {
                    clock.advance(Duration::from_secs(secs));
                    engine.tick();
                }
                Action::StartShort => {
                    engine.start_short_break_now();
                }
                Action::StartLong => {
                    engine.start_long_break_now();
                }
                Action::Postpone => {
                    engine.postpone_long_break();
                }
                Action::Skip => {
                    engine.skip_current_break();
                }
                Action::Pause(opt_secs) => {
                    engine.pause_for(opt_secs.map(Duration::from_secs));
                }
                Action::Resume => {
                    engine.resume();
                }
                Action::Session(ev) => {
                    engine.on_session_event(ev);
                }
                Action::SetIdle(secs) => {
                    platform.idle.set_idle_time(Duration::from_secs(secs));
                }
                Action::SetFocus(state) => {
                    platform.focus.set_state(state);
                }
                Action::ToggleStrict(strict) => {
                    settings.strict_mode = strict;
                    engine.update_settings(settings.clone());
                }
            }

            let snap = engine.snapshot();
            if let Some(short_rem) = snap.time_until_short_break {
                prop_assert!(short_rem <= engine.settings().short_breaks.interval());
            }
            if let Some(long_rem) = snap.time_until_long_break {
                prop_assert!(long_rem <= engine.settings().long_breaks.interval());
            }
            if let Some(cur) = &snap.current_break {
                prop_assert!(cur.remaining_duration <= cur.total_duration);
                prop_assert!(cur.remaining_duration > Duration::ZERO);
            }

            let total_active: u32 = engine.statistics().days.iter().map(|d| d.active_minutes).sum();
            let total_absences: u32 = engine.statistics().days.iter().map(|d| d.absences_credited).sum();
            prop_assert!(total_active >= prev_active_minutes);
            prop_assert!(total_absences >= prev_absences_credited);
            prev_active_minutes = total_active;
            prev_absences_credited = total_absences;
        }
    }

    #[test]
    fn proptest_active_pause_survives_session_events_and_gaps(
        pause_secs in prop_oneof![Just(None), (120u64..=7200).prop_map(Some)],
        steps in prop::collection::vec((1u64..=300, arb_session_event(), arb_focus_state(), 0u64..=600), 1..30),
        use_resume in any::<bool>()
    ) {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).expect("valid"));
        let platform = TestPlatformServices::new();
        let mut engine = BreakEngine::with_defaults(
            // C# parity: no natural moments in the C# engine
            OcySettings {
                natural_moments: false,
                ..OcySettings::default()
            },
            platform.clone(),
            clock.clone(),
        );

        engine.pause_for(pause_secs.map(Duration::from_secs));
        let expected_until = engine.paused_until();
        prop_assert_eq!(engine.phase(), EnginePhase::Paused);

        let mut elapsed = 0u64;
        for (dt, session_ev, focus, idle) in steps {
            if let Some(limit) = pause_secs
                && elapsed + dt >= limit
            {
                break;
            }
            elapsed += dt;
            clock.advance(Duration::from_secs(dt));
            platform.focus.set_state(focus);
            platform.idle.set_idle_time(Duration::from_secs(idle));
            let s_events = engine.on_session_event(session_ev);
            let t_events = engine.tick();

            prop_assert_eq!(engine.phase(), EnginePhase::Paused);
            prop_assert_eq!(engine.paused_until(), expected_until);
            prop_assert!(!s_events.iter().any(|e| matches!(e, EngineEvent::AbsenceCredited(_))));
            prop_assert!(!t_events.iter().any(|e| matches!(e, EngineEvent::AbsenceCredited(_))));
        }

        if use_resume || pause_secs.is_none() {
            engine.resume();
            prop_assert_eq!(engine.phase(), EnginePhase::Working);
            prop_assert_eq!(engine.paused_until(), None);
            prop_assert_eq!(engine.since_short_break(), Duration::ZERO);
            prop_assert_eq!(engine.since_long_break(), Duration::ZERO);
        } else if let Some(limit) = pause_secs {
            let remaining = limit - elapsed;
            clock.advance(Duration::from_secs(remaining));
            engine.tick();
            prop_assert_eq!(engine.phase(), EnginePhase::Working);
            prop_assert_eq!(engine.paused_until(), None);
            prop_assert_eq!(engine.since_short_break(), Duration::ZERO);
            prop_assert_eq!(engine.since_long_break(), Duration::ZERO);
        }
    }

    #[test]
    fn proptest_absence_never_credited_twice_for_one_away_span(
        work_secs in 1u64..=500,
        trigger in arb_away_trigger(),
        away_gaps in prop::collection::vec(1u64..=1200, 1..20)
    ) {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).expect("valid"));
        let platform = TestPlatformServices::new();
        let mut engine = BreakEngine::with_defaults(
            // C# parity: no natural moments in the C# engine
            OcySettings {
                natural_moments: false,
                ..OcySettings::default()
            },
            platform.clone(),
            clock.clone(),
        );

        for _ in 0..work_secs {
            clock.advance(Duration::from_secs(1));
            engine.tick();
        }

        fn count_credits(events: &[EngineEvent]) -> usize {
            events
                .iter()
                .filter(|e| matches!(e, EngineEvent::AbsenceCredited(_)))
                .count()
        }

        let mut credit_events = 0usize;

        match trigger {
            AwayTrigger::Idle => {
                platform.idle.set_idle_time(Duration::from_secs(120));
                clock.advance(Duration::from_secs(1));
                credit_events += count_credits(&engine.tick());
            }
            AwayTrigger::Locked => {
                credit_events += count_credits(&engine.on_session_event(SessionEvent::Locked));
            }
            AwayTrigger::Suspending => {
                credit_events += count_credits(&engine.on_session_event(SessionEvent::Suspending));
            }
            AwayTrigger::FocusNotPresent => {
                platform.focus.set_state(FocusState::NotPresent);
                clock.advance(Duration::from_secs(1));
                credit_events += count_credits(&engine.tick());
            }
        }

        prop_assert_eq!(engine.phase(), EnginePhase::Away);
        prop_assert_eq!(credit_events, 0);

        let mut total_idle = 120u64;
        for gap in away_gaps {
            clock.advance(Duration::from_secs(gap));
            total_idle += gap;
            if matches!(trigger, AwayTrigger::Idle) {
                platform.idle.set_idle_time(Duration::from_secs(total_idle));
            }
            credit_events += count_credits(&engine.tick());
            prop_assert_eq!(engine.phase(), EnginePhase::Away);
            prop_assert_eq!(credit_events, 0);
        }

        match trigger {
            AwayTrigger::Idle => {
                platform.idle.set_idle_time(Duration::ZERO);
                clock.advance(Duration::from_secs(1));
                credit_events += count_credits(&engine.tick());
            }
            AwayTrigger::Locked => {
                credit_events += count_credits(&engine.on_session_event(SessionEvent::Unlocked));
            }
            AwayTrigger::Suspending => {
                credit_events += count_credits(&engine.on_session_event(SessionEvent::Resumed));
            }
            AwayTrigger::FocusNotPresent => {
                platform.focus.set_state(FocusState::Normal);
                clock.advance(Duration::from_secs(1));
                credit_events += count_credits(&engine.tick());
            }
        }

        prop_assert_eq!(engine.phase(), EnginePhase::Working);
        prop_assert!(credit_events <= 1);
    }
}
