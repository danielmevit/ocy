//! Unit tests for the adaptive host scheduler (`ocy_app::scheduler`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_platform::{Clock, FocusState, FocusStateProvider, IdleProvider, PlatformServices};
use jiff::Timestamp;
use ocy_app::scheduler::{self, ACTIVE_WAKE_INTERVAL, MAX_WAKE_DELAY, MIN_WAKE_DELAY};
use ocy_core::engine::{BreakEngine, EnginePhase, EngineSnapshot, NaturalHoldKind};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::{DateTimeOffset, FakeClock, Paused};

/// A minimal scripted platform: controllable idle and focus, like ocy-core's test fake.
#[derive(Debug)]
struct FakeIdle {
    idle_nanos: AtomicU64,
    supported: AtomicBool,
}

impl FakeIdle {
    fn set_idle(&self, idle: Duration) {
        self.idle_nanos.store(
            u64::try_from(idle.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
}

impl IdleProvider for FakeIdle {
    fn is_supported(&self) -> bool {
        self.supported.load(Ordering::Relaxed)
    }

    fn idle_time(&self) -> Duration {
        if !self.is_supported() {
            return Duration::ZERO;
        }
        Duration::from_nanos(self.idle_nanos.load(Ordering::Relaxed))
    }
}

#[derive(Debug)]
struct FakeFocus {
    state: Mutex<FocusState>,
}

impl FocusStateProvider for FakeFocus {
    fn is_supported(&self) -> bool {
        true
    }

    fn state(&self) -> FocusState {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Debug)]
struct TestPlatform {
    idle: FakeIdle,
    focus: FakeFocus,
}

impl TestPlatform {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            idle: FakeIdle {
                idle_nanos: AtomicU64::new(0),
                supported: AtomicBool::new(true),
            },
            focus: FakeFocus {
                state: Mutex::new(FocusState::Normal),
            },
        })
    }

    fn set_idle(&self, idle: Duration) {
        self.idle.set_idle(idle);
    }
}

impl PlatformServices for TestPlatform {
    fn idle(&self) -> &dyn IdleProvider {
        &self.idle
    }

    fn focus(&self) -> &dyn FocusStateProvider {
        &self.focus
    }
}

fn ts(secs: i64) -> Timestamp {
    Timestamp::from_second(secs).expect("valid timestamp")
}

fn snapshot(
    phase: EnginePhase,
    until_short: Option<Duration>,
    until_long: Option<Duration>,
    until_warning: Option<Duration>,
) -> EngineSnapshot {
    EngineSnapshot {
        phase,
        time_until_short_break: until_short,
        time_until_long_break: until_long,
        time_until_long_break_warning: until_warning,
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        natural_hold: None,
    }
}

#[test]
fn working_sleeps_until_the_next_due_moment_capped_at_ten_seconds() {
    // Nothing due soon: the 10 s cap applies (idle re-checks, away detection).
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(600)),
        Some(Duration::from_secs(3600)),
        Some(Duration::from_secs(3540)),
    );
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MAX_WAKE_DELAY);

    // The nearest due moment wins.
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(3)),
        Some(Duration::from_secs(3600)),
        Some(Duration::from_secs(3540)),
    );
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        Duration::from_secs(3)
    );

    // The warning can be the nearest due moment (anything beyond the cap is capped).
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(120)),
        Some(Duration::from_secs(165)),
        Some(Duration::from_secs(4)),
    );
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        Duration::from_secs(4)
    );

    // Disabled breaks contribute nothing.
    let snap = snapshot(EnginePhase::Working, None, None, None);
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MAX_WAKE_DELAY);

    // A due moment now clamps to the minimum (the tick resolves it immediately).
    let snap = snapshot(EnginePhase::Working, Some(Duration::ZERO), None, None);
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MIN_WAKE_DELAY);
}

#[test]
fn visible_windows_and_holds_tick_every_second() {
    for phase in [
        EnginePhase::ShortBreak,
        EnginePhase::LongBreak,
        EnginePhase::LongBreakWarning,
        EnginePhase::Deferred,
        EnginePhase::Away,
    ] {
        let snap = snapshot(phase, None, None, None);
        assert_eq!(
            scheduler::next_wake_delay(&snap, ts(0)),
            ACTIVE_WAKE_INTERVAL,
            "phase {phase:?}"
        );
    }

    // A natural-moment hold re-checks the lull every second.
    let snap = EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: Some(Duration::ZERO),
        time_until_long_break: Some(Duration::from_secs(3600)),
        time_until_long_break_warning: Some(Duration::from_secs(3540)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        natural_hold: Some(NaturalHoldKind::ShortBreak),
    };
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        ACTIVE_WAKE_INTERVAL
    );
}

#[test]
fn paused_sleeps_until_expiry_capped_at_ten_seconds() {
    let mut snap = snapshot(EnginePhase::Paused, None, None, None);
    snap.paused_until_utc = Some(Paused::Until(DateTimeOffset::from_utc(ts(3))));
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        Duration::from_secs(3)
    );

    // Far-future expiry: the cap applies.
    snap.paused_until_utc = Some(Paused::Until(DateTimeOffset::from_utc(ts(3600))));
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MAX_WAKE_DELAY);

    // Indefinite pause: nothing can happen except a resume (which re-arms).
    snap.paused_until_utc = Some(Paused::UntilResumed);
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MAX_WAKE_DELAY);

    // An expiry that already passed wakes at the minimum so the pause ends promptly.
    snap.paused_until_utc = Some(Paused::Until(DateTimeOffset::from_utc(ts(10))));
    assert_eq!(scheduler::next_wake_delay(&snap, ts(20)), MIN_WAKE_DELAY);
}

#[test]
fn a_plain_working_minute_takes_at_most_six_wakeups() {
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 9, 0, 0).expect("valid date"));
    let platform = TestPlatform::new();
    platform.set_idle(Duration::ZERO);
    let settings = OcySettings {
        natural_moments: true,
        ..OcySettings::default()
    };
    let mut engine = BreakEngine::new(
        settings,
        platform.clone(),
        clock.clone(),
        None,
        Some(Box::new(DefaultExercisePicker::new(SeededRng::new(1)))),
        None,
        None,
    );

    // Simulate one minute of continuous typing with the adaptive schedule.
    let mut wakeups = 0u64;
    let mut elapsed = Duration::ZERO;
    while elapsed < Duration::from_secs(60) {
        let delay = scheduler::next_wake_delay(&engine.snapshot(), clock.now().timestamp());
        clock.advance(delay);
        elapsed += delay;
        let _ = engine.tick();
        wakeups += 1;
    }
    assert!(
        wakeups <= 6,
        "a plain working minute took {wakeups} wake-ups"
    );
}
