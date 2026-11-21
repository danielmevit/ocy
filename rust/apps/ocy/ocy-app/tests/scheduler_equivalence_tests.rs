//! Equivalence test: the adaptive host scheduler produces the same break/warning starts,
//! ends, and event sequence as the previous fixed 1 s tick, with an explicit per-event
//! tolerance.
//!
//! Each seeded script (idle, focus, and session-event timelines) is simulated twice
//! against the real engine with a fake clock — once with 1 s ticks and once with the
//! adaptive `ocy_app::scheduler` delays — with natural moments on and off.
//!
//! Tolerance, per event kind:
//! - <= 1 s (`ACTIVE_WAKE_INTERVAL`) for every event whose schedule anchor is exact in
//!   both hosts: session events (dispatched at the scripted second in both), natural
//!   lulls (polled every second in both), and the initial schedule.
//! - <= 5 s (`IDLE_PHASE_WAKE_INTERVAL`) for events caused by an away return or a
//!   deferral ending that was detected at tick granularity — the away return itself
//!   (`AbsenceCredited`), the break/warning start that ends a deferral, and everything
//!   downstream: the return resets the counters, so the whole following schedule is
//!   anchored to the (up to 5 s late) detection until the next exact re-anchor (a
//!   session unlock/resume or a natural lull). The drift never accumulates: every
//!   re-anchor is the detection of the same scripted moment within the host's own
//!   granularity, so any single event stays within one granularity of its counterpart.
//!
//! Script constraints and why:
//! - All script times are whole seconds, so the 1 s grid and the adaptive cadence land
//!   on the same instants; the tolerances cover any residual drift.
//! - Away spans (idle gaps and lock durations) are >= 65 s, so every away return credits
//!   the long break and resets both counters. Shorter away spans (30-59 s) credit only the
//!   short break; the adaptive scheduler detects away entry up to 10 s late (its working
//!   cap), which would leave `since_long` up to ~9 s apart between the two runs — a real,
//!   documented granularity property, not an equivalence bug, so those spans are excluded
//!   from the generated scripts.
//! - `FocusState::NotPresent` is not generated: on Windows, lock/screen-saver away arrives
//!   as session events (delivered immediately, which re-arms the scheduler); NotPresent
//!   focus is a fallback sampled at tick granularity.
//! - No explicit user actions (skip/postpone/pause) are scripted; those re-arm the timer
//!   in both hosts and are covered by the engine's own tests.

use std::sync::Arc;
use std::time::Duration;

use damt_platform::{Clock, FocusState, FocusStateProvider, IdleProvider, PlatformServices};
use ocy_app::scheduler;
use ocy_core::engine::{BreakEngine, BreakKind, EngineEvent, EnginePhase};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

const HORIZON_SECS: u64 = 2400;
const SEEDS: &[u64] = &[
    0x0123_4567,
    0x0bad_c0de,
    0x0777_1111,
    0x0feedface,
    0x0abc_def0,
    0x0918_2736,
];

/// A tiny deterministic xorshift64* generator (proptest is not a dependency of `ocy-app`;
/// seeded scripts keep runs reproducible).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rec {
    ShortStarted,
    ShortEnded,
    LongStarted,
    LongEnded,
    WarningStarted,
    AbsenceShort,
    AbsenceLong,
}

#[derive(Debug)]
struct Timeline {
    /// Sorted seconds at which the user typed/moved (first is 0).
    inputs: Vec<u64>,
    /// Sorted `(second, state)` focus changes (initial Normal at 0).
    focus_changes: Vec<(u64, FocusState)>,
    /// Sorted `(second, event)` session events.
    session_events: Vec<(u64, damt_platform::SessionEvent)>,
}

fn generate_timeline(seed: u64) -> Timeline {
    let mut rng = Rng::new(seed);

    // Input gaps: typing (1-8 s), short pauses (10-28 s, below the 30 s away threshold),
    // and away spans (65-90 s in 5 s steps, always long-credit). The first 1000 s stay
    // away-free so the long-break warning and long break are reached and exercised.
    let mut inputs = vec![0u64];
    let mut t = 0u64;
    while t < HORIZON_SECS {
        let gap = if t < 1000 {
            match rng.below(2) {
                0 => 1 + rng.below(8),
                _ => 10 + rng.below(19),
            }
        } else {
            match rng.below(100) {
                r if r < 80 => 1 + rng.below(8),
                r if r < 96 => 10 + rng.below(19),
                _ => 65 + 5 * rng.below(6),
            }
        };
        t += gap;
        if t < HORIZON_SECS {
            inputs.push(t);
        }
    }

    // Focus: mostly Normal with occasional FullscreenApp spans (5-30 s).
    let mut focus_changes = vec![(0u64, FocusState::Normal)];
    let mut t = 0u64;
    while t < HORIZON_SECS {
        t += 60 + rng.below(180);
        if t >= HORIZON_SECS {
            break;
        }
        focus_changes.push((t, FocusState::FullscreenApp));
        let len = 5 + rng.below(26);
        if t + len < HORIZON_SECS {
            focus_changes.push((t + len, FocusState::Normal));
        }
        t += len;
    }

    // Session events: 0-2 lock/unlock pairs with long-credit spans (65-260 s, 5 s steps).
    let mut session_events = Vec::new();
    for _ in 0..rng.below(3) {
        let lock = 30 + rng.below(HORIZON_SECS.saturating_sub(400));
        let span = 65 + 5 * rng.below(20);
        if lock + span < HORIZON_SECS {
            session_events.push((lock, damt_platform::SessionEvent::Locked));
            session_events.push((lock + span, damt_platform::SessionEvent::Unlocked));
        }
    }
    session_events.sort_by_key(|(t, _)| *t);

    Timeline {
        inputs,
        focus_changes,
        session_events,
    }
}

impl Timeline {
    fn idle_at(&self, t: u64) -> Duration {
        let last = self
            .inputs
            .iter()
            .rev()
            .find(|&&x| x <= t)
            .copied()
            .unwrap_or(0);
        Duration::from_secs(t - last)
    }

    fn focus_at(&self, t: u64) -> FocusState {
        self.focus_changes
            .iter()
            .rev()
            .find(|&&(x, _)| x <= t)
            .map_or(FocusState::Normal, |(_, f)| *f)
    }
}

/// Scriptable platform: idle and focus are computed from the timeline by the runner.
#[derive(Debug)]
struct FakeIdle(atomic::AtomicU64);

impl IdleProvider for FakeIdle {
    fn is_supported(&self) -> bool {
        true
    }

    fn idle_time(&self) -> Duration {
        Duration::from_nanos(self.0.load(atomic::Ordering::Relaxed))
    }
}

#[derive(Debug)]
struct FakeFocus(std::sync::Mutex<FocusState>);

impl FocusStateProvider for FakeFocus {
    fn is_supported(&self) -> bool {
        true
    }

    fn state(&self) -> FocusState {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
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
            idle: FakeIdle(atomic::AtomicU64::new(0)),
            focus: FakeFocus(std::sync::Mutex::new(FocusState::Normal)),
        })
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

// The test file is a separate crate; `std::sync::atomic` is aliased for the short helpers.
mod atomic {
    pub use std::sync::atomic::{AtomicU64, Ordering};
}

/// A recorded engine event: what happened, when, and whether its timing is anchored to
/// a tick-granularity away return or deferral end (`loose`), which bounds its drift by
/// the idle-phase wake interval instead of the active one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimedRec {
    rec: Rec,
    at: Duration,
    loose: bool,
}

fn record_events(events: &[EngineEvent], at: Duration, loose: bool, out: &mut Vec<TimedRec>) {
    for event in events {
        let rec = match event {
            EngineEvent::BreakStarted(info) => match info.kind {
                BreakKind::Short => Rec::ShortStarted,
                BreakKind::Long => Rec::LongStarted,
            },
            EngineEvent::BreakEnded(info, _) => match info.kind {
                BreakKind::Short => Rec::ShortEnded,
                BreakKind::Long => Rec::LongEnded,
            },
            EngineEvent::LongBreakWarningStarted(_) => Rec::WarningStarted,
            EngineEvent::AbsenceCredited(kind) => match kind {
                BreakKind::Short => Rec::AbsenceShort,
                BreakKind::Long => Rec::AbsenceLong,
            },
            EngineEvent::PhaseChanged(_) => continue,
        };
        out.push(TimedRec { rec, at, loose });
    }
}

/// True if the batch contains a break or warning start (a natural hold resolving).
fn contains_start(events: &[EngineEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            EngineEvent::BreakStarted(_) | EngineEvent::LongBreakWarningStarted(_)
        )
    })
}

/// Simulates the host against the real engine: `adaptive` selects the scheduler's
/// single-shot delays, otherwise the host ticks every second. Session events are
/// dispatched at their exact scripted time in both modes (as the real hosts do) and
/// re-arm the adaptive timer.
///
/// Each recorded event is flagged `loose` when its timing is anchored to a
/// tick-granularity away return or deferral end (see the module docs); the flag drives
/// the per-event tolerance in the comparison.
fn run(timeline: &Timeline, adaptive: bool, natural_moments: bool) -> Vec<TimedRec> {
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 9, 0, 0).expect("valid date"));
    let platform = TestPlatform::new();
    let mut settings = OcySettings {
        natural_moments,
        ..OcySettings::default()
    };
    settings.short_breaks.interval_minutes = 2;
    settings.short_breaks.duration_seconds = 20;
    settings.long_breaks.interval_minutes = 15;
    settings.long_breaks.duration_minutes = 1;
    settings.long_breaks.warning_seconds = 30;
    settings.away_threshold_seconds = 30;

    let mut engine = BreakEngine::new(
        settings,
        platform.clone(),
        clock.clone(),
        None,
        Some(Box::new(DefaultExercisePicker::new(SeededRng::new(11)))),
        None,
        None,
    );

    let mut records = Vec::new();
    let mut elapsed = Duration::ZERO;
    let mut event_idx = 0usize;
    let horizon = Duration::from_secs(HORIZON_SECS);
    let mut iterations = 0u64;
    // Whether the schedule is currently anchored to a tick-granularity away return or
    // deferral end (drift bounded by the idle-phase wake interval).
    let mut anchor_loose = false;

    loop {
        iterations += 1;
        assert!(
            iterations < 10 * HORIZON_SECS,
            "scheduler simulation did not terminate"
        );

        let delay = if adaptive {
            scheduler::next_wake_delay(&engine.snapshot(), clock.now().timestamp())
        } else {
            Duration::from_secs(1)
        };
        let wake = elapsed + delay;

        // A session event before the next wake interrupts the sleep: the real host
        // dispatches it immediately and re-arms the timer.
        let next_event = timeline.session_events.get(event_idx).copied();
        if let Some((ev_secs, _)) = next_event {
            let ev_at = Duration::from_secs(ev_secs);
            if ev_at > elapsed && ev_at < wake {
                clock.advance(ev_at - elapsed);
                elapsed = ev_at;
                while let Some((t, event)) = timeline.session_events.get(event_idx).copied()
                    && Duration::from_secs(t) <= elapsed
                {
                    // Session events are dispatched at the exact scripted second in
                    // both hosts, so their events are exact — and a session away
                    // return re-anchors the schedule exactly.
                    let phase_before = engine.snapshot().phase;
                    let events = engine.on_session_event(event);
                    let returned = phase_before == EnginePhase::Away
                        && engine.snapshot().phase != phase_before;
                    record_events(&events, elapsed, false, &mut records);
                    if returned {
                        anchor_loose = false;
                    }
                    event_idx += 1;
                }
                continue;
            }
        }

        if wake >= horizon {
            break;
        }

        clock.advance(delay);
        elapsed = wake;

        // Events that coincide with the wake are dispatched before the tick.
        while let Some((t, event)) = timeline.session_events.get(event_idx).copied()
            && Duration::from_secs(t) <= elapsed
        {
            let phase_before = engine.snapshot().phase;
            let events = engine.on_session_event(event);
            let returned =
                phase_before == EnginePhase::Away && engine.snapshot().phase != phase_before;
            record_events(&events, elapsed, false, &mut records);
            if returned {
                anchor_loose = false;
            }
            event_idx += 1;
        }

        platform.idle.0.store(
            u64::try_from(timeline.idle_at(elapsed.as_secs()).as_nanos()).unwrap_or(0),
            atomic::Ordering::Relaxed,
        );
        *platform.focus.0.lock().unwrap_or_else(|e| e.into_inner()) =
            timeline.focus_at(elapsed.as_secs());

        let before = engine.snapshot();
        let events = engine.tick();
        let after = engine.snapshot();
        let batch_loose = if before.natural_hold.is_some()
            && after.natural_hold.is_none()
            && contains_start(&events)
        {
            // A natural hold resolved at a lull: both hosts poll every second, so the
            // start is lull-anchored and the schedule re-anchors exactly.
            anchor_loose = false;
            false
        } else if before.phase == EnginePhase::Away && after.phase != EnginePhase::Away {
            // An away return detected at tick granularity: the return and everything
            // downstream drift by the idle-phase wake interval.
            anchor_loose = true;
            true
        } else if before.phase == EnginePhase::Deferred && after.phase != EnginePhase::Deferred {
            // A deferral ended at tick granularity (fullscreen exit): the start it
            // releases is detection-anchored.
            anchor_loose = true;
            true
        } else {
            anchor_loose
        };
        record_events(&events, elapsed, batch_loose, &mut records);
    }

    records
}

#[test]
fn adaptive_schedule_matches_one_second_ticks_with_per_event_tolerance() {
    for &seed in SEEDS {
        let timeline = generate_timeline(seed);
        for &natural in &[true, false] {
            let fixed = run(&timeline, false, natural);
            let adaptive = run(&timeline, true, natural);

            assert_eq!(
                fixed.len(),
                adaptive.len(),
                "seed {seed}, natural {natural}: event counts differ\n{fixed:?}\n{adaptive:?}"
            );
            for (i, (a, b)) in fixed.iter().zip(&adaptive).enumerate() {
                let TimedRec {
                    rec: rec_a,
                    at: at_a,
                    loose: loose_a,
                } = *a;
                let TimedRec {
                    rec: rec_b,
                    at: at_b,
                    loose: loose_b,
                } = *b;
                assert_eq!(
                    rec_a, rec_b,
                    "seed {seed}, natural {natural}: event {i} differs\n{fixed:?}\n{adaptive:?}"
                );
                let diff = at_a.abs_diff(at_b);
                // Explicit per-event-kind tolerance: away returns and deferral ends
                // (and the schedule anchored to them) may lag by the idle-phase wake
                // interval; everything else must stay within the active interval.
                let tolerance = if loose_a || loose_b {
                    scheduler::IDLE_PHASE_WAKE_INTERVAL
                } else {
                    scheduler::ACTIVE_WAKE_INTERVAL
                };
                assert!(
                    diff <= tolerance,
                    "seed {seed}, natural {natural}: event {i} ({rec_a:?}, loose={loose_a}{loose_b}) \
                     time differs by {diff:?} (tolerance {tolerance:?})\n{fixed:?}\n{adaptive:?}"
                );
            }
        }
    }
}
