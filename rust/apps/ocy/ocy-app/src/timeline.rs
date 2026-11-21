//! Two-hour upcoming-breaks timeline projection for Settings V2.
//!
//! Simulates continuous activity from now while reproducing the real [`ocy_core::engine::BreakEngine`]
//! state transitions:
//! - [`ocy_core::engine::BreakEngine::tick`]: while `Working`, checks long-break warning/start
//!   before short breaks; suppresses short breaks within 2 minutes of a long break via
//!   [`ocy_core::schedule::ScheduleCalculator::is_short_break_suppressed_by_long_break`]; pauses
//!   both working counters while `LongBreakWarning` counts down.
//! - [`ocy_core::engine::BreakEngine::update_break`]: neither working counter advances while a
//!   break is running; completing a `Short` break resets `since_short_break` only, while completing a
//!   `Long` break resets **both** `since_long_break` and `since_short_break` to zero.
//! - [`ocy_core::engine::BreakEngine::end_pause`]: when a timed pause expires, resets **both**
//!   `since_short_break` and `since_long_break` to zero.

use std::time::Duration;

use damt_platform::Clock;
use jiff::SignedDuration;
use ocy_core::engine::{BreakKind, EnginePhase, EngineSnapshot};
use ocy_core::schedule::ScheduleCalculator;
use ocy_core::settings::OcySettings;
use ocy_core::time::Paused;
use ocy_ui::{TimelineMark as UiTimelineMark, TimelineMarkKind as UiTimelineMarkKind};

/// Total horizon covered by the Settings V2 timeline (`2 hours`).
pub const TIMELINE_SPAN: Duration = Duration::from_secs(2 * 60 * 60);

/// Number of 30-minute tick labels shown after `now`.
pub const TICK_LABEL_COUNT: usize = 4;

/// A single break mark along the 2-hour timeline (`position` in `0.0..=1.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineMark {
    pub kind: BreakKind,
    pub position: f32,
}

impl From<TimelineMark> for UiTimelineMark {
    fn from(mark: TimelineMark) -> Self {
        Self {
            kind: match mark.kind {
                BreakKind::Short => UiTimelineMarkKind::ShortBreak,
                BreakKind::Long => UiTimelineMarkKind::LongBreak,
            },
            position: mark.position,
        }
    }
}

/// Timeline marks plus the 4 local `HH:mm` labels at +30, +60, +90, and +120 minutes.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineProjection {
    pub marks: Vec<TimelineMark>,
    pub labels: [String; TICK_LABEL_COUNT],
}

impl TimelineProjection {
    /// Converts the projected marks into `ocy_ui::TimelineMark` values for `SettingsWindowV2`.
    #[must_use]
    pub fn to_ui_marks(&self) -> Vec<UiTimelineMark> {
        self.marks.iter().copied().map(Into::into).collect()
    }
}

/// Computes the 4 local-clock `HH:mm` labels at +30, +60, +90, and +120 minutes from `clock`.
#[must_use]
pub fn time_labels(clock: &dyn Clock) -> [String; TICK_LABEL_COUNT] {
    let now = clock.now();
    [30_i64, 60, 90, 120].map(|minutes| {
        let at = now
            .checked_add(SignedDuration::from_mins(minutes))
            .unwrap_or_else(|_| now.clone());
        format!("{:02}:{:02}", at.hour(), at.minute())
    })
}

/// Projects upcoming breaks over the next 2 hours assuming continuous activity from now.
#[must_use]
pub fn project(
    snapshot: &EngineSnapshot,
    settings: &OcySettings,
    clock: &dyn Clock,
) -> TimelineProjection {
    let labels = time_labels(clock);
    let mut clamped = settings.clone();
    clamped.clamp();

    let short_enabled = clamped.short_breaks.enabled;
    let long_enabled = clamped.long_breaks.enabled;
    if !short_enabled && !long_enabled {
        return TimelineProjection {
            marks: Vec::new(),
            labels,
        };
    }

    let short_interval = clamped.short_breaks.interval();
    let short_duration = clamped.short_breaks.duration();
    let long_interval = clamped.long_breaks.interval();
    let long_duration = clamped.long_breaks.duration();
    let warning_duration = clamped.long_breaks.warning_duration();
    let warning_at = long_interval.saturating_sub(warning_duration);

    let mut marks = Vec::new();
    let mut t = Duration::ZERO;
    let mut since_short = Duration::ZERO;
    let mut since_long = Duration::ZERO;

    match snapshot.phase {
        EnginePhase::Paused => match snapshot.paused_until_utc {
            None | Some(Paused::UntilResumed) => {
                // Indefinite pause: no upcoming breaks can be scheduled.
                return TimelineProjection { marks, labels };
            }
            Some(Paused::Until(until)) => {
                let remaining =
                    Duration::try_from(until.timestamp().duration_since(clock.now().timestamp()))
                        .unwrap_or(Duration::ZERO);
                if remaining >= TIMELINE_SPAN {
                    return TimelineProjection { marks, labels };
                }
                // `BreakEngine::end_pause` resets both `since_short_break` and `since_long_break`
                // to zero when a timed pause expires.
                t = remaining;
            }
        },
        EnginePhase::ShortBreak => {
            let remaining = snapshot
                .current_break
                .as_ref()
                .map_or(Duration::ZERO, |b| b.remaining_duration);
            if remaining >= TIMELINE_SPAN {
                return TimelineProjection { marks, labels };
            }
            // `BreakEngine::update_break` (`BreakKind::Short`): `since_short_break` resets to zero
            // when the break ends; `since_long_break` does not advance during the break.
            t = remaining;
            since_short = Duration::ZERO;
            since_long = if long_enabled {
                long_interval
                    .saturating_sub(snapshot.time_until_long_break.unwrap_or(long_interval))
            } else {
                Duration::ZERO
            };
        }
        EnginePhase::LongBreak => {
            let remaining = snapshot
                .current_break
                .as_ref()
                .map_or(Duration::ZERO, |b| b.remaining_duration);
            if remaining >= TIMELINE_SPAN {
                return TimelineProjection { marks, labels };
            }
            // `BreakEngine::update_break` (`BreakKind::Long`): both counters reset to zero.
            t = remaining;
        }
        EnginePhase::LongBreakWarning => {
            // `BreakEngine::tick` (`LongBreakWarning` branch): working counters are paused while
            // the warning counts down to zero, then the long break starts and resets both counters.
            if long_enabled {
                let until_long = snapshot.time_until_long_break.unwrap_or(warning_duration);
                t += until_long;
                if t > TIMELINE_SPAN {
                    return TimelineProjection { marks, labels };
                }
                marks.push(TimelineMark {
                    kind: BreakKind::Long,
                    position: to_position(t),
                });
                t += long_duration;
            } else {
                since_short = if short_enabled {
                    short_interval
                        .saturating_sub(snapshot.time_until_short_break.unwrap_or(short_interval))
                } else {
                    Duration::ZERO
                };
            }
        }
        EnginePhase::Working | EnginePhase::Deferred | EnginePhase::Away => {
            since_short = if short_enabled {
                short_interval
                    .saturating_sub(snapshot.time_until_short_break.unwrap_or(short_interval))
            } else {
                Duration::ZERO
            };
            since_long = if long_enabled {
                long_interval
                    .saturating_sub(snapshot.time_until_long_break.unwrap_or(long_interval))
            } else {
                Duration::ZERO
            };
        }
    }

    // Working-phase simulation loop mirroring `BreakEngine::tick` + `BreakEngine::update_break`.
    while t <= TIMELINE_SPAN {
        let dt_short = short_enabled.then(|| short_interval.saturating_sub(since_short));
        let dt_warning = long_enabled.then(|| warning_at.saturating_sub(since_long));

        match (dt_short, dt_warning) {
            (None, None) => break,
            (Some(ds), Some(dw)) if ds < dw => {
                t += ds;
                since_short += ds;
                since_long += ds;

                if ScheduleCalculator::is_short_break_suppressed_by_long_break(&clamped, since_long)
                {
                    // `BreakEngine::tick` silently resets `since_short_break` without starting a break.
                    since_short = Duration::ZERO;
                    continue;
                }

                if t > TIMELINE_SPAN {
                    break;
                }
                marks.push(TimelineMark {
                    kind: BreakKind::Short,
                    position: to_position(t),
                });
                t += short_duration;
                since_short = Duration::ZERO;
            }
            (_, Some(dw)) => {
                // `BreakEngine::tick` checks long break warning/start before short breaks, so ties
                // (`dw == ds`) resolve to the long break cycle.
                t += dw;
                t += warning_duration;
                if t > TIMELINE_SPAN {
                    break;
                }
                marks.push(TimelineMark {
                    kind: BreakKind::Long,
                    position: to_position(t),
                });
                t += long_duration;
                // `BreakEngine::update_break` (`BreakKind::Long`): resets both counters.
                since_short = Duration::ZERO;
                since_long = Duration::ZERO;
            }
            (Some(ds), None) => {
                t += ds;
                if t > TIMELINE_SPAN {
                    break;
                }
                marks.push(TimelineMark {
                    kind: BreakKind::Short,
                    position: to_position(t),
                });
                t += short_duration;
                since_short = Duration::ZERO;
            }
        }
    }

    TimelineProjection { marks, labels }
}

fn to_position(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f64() / TIMELINE_SPAN.as_secs_f64()).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use damt_platform::{FocusState, FocusStateProvider, IdleProvider, PlatformServices};
    use ocy_core::engine::{BreakEngine, EngineEvent};
    use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
    use ocy_core::time::FakeClock;
    use std::sync::Arc;

    #[derive(Debug)]
    struct ActivePlatform;

    impl IdleProvider for ActivePlatform {
        fn is_supported(&self) -> bool {
            true
        }

        fn idle_time(&self) -> Duration {
            Duration::ZERO
        }
    }

    impl FocusStateProvider for ActivePlatform {
        fn is_supported(&self) -> bool {
            true
        }

        fn state(&self) -> FocusState {
            FocusState::Normal
        }
    }

    impl PlatformServices for ActivePlatform {
        fn idle(&self) -> &dyn IdleProvider {
            self
        }

        fn focus(&self) -> &dyn FocusStateProvider {
            self
        }
    }

    fn run_engine_and_compare(
        mut settings: OcySettings,
        setup: impl FnOnce(&mut BreakEngine, &FakeClock),
    ) {
        // Keep natural moments off so continuous typing (idle=0) doesn't insert synthetic 60s holds
        // on every single break across 2 hours.
        settings.natural_moments = false;

        let clock =
            Arc::new(FakeClock::from_fixed_offset(2026, 11, 20, 14, 30, 0, 1).expect("clock"));
        let platform = Arc::new(ActivePlatform);
        let picker = Box::new(DefaultExercisePicker::new(SeededRng::new(7)));
        let mut engine = BreakEngine::new(
            settings.clone(),
            platform,
            Arc::clone(&clock) as Arc<dyn Clock>,
            None,
            Some(picker),
            None,
            None,
        );

        setup(&mut engine, &clock);

        let snap = engine.snapshot();
        let projected = project(&snap, engine.settings(), clock.as_ref());

        let expected_labels: [String; TICK_LABEL_COUNT] = [30_i64, 60, 90, 120].map(|m| {
            let at = clock
                .now()
                .checked_add(SignedDuration::from_mins(m))
                .expect("valid time");
            format!("{:02}:{:02}", at.hour(), at.minute())
        });
        assert_eq!(projected.labels, expected_labels);

        let mut actual_starts: Vec<(BreakKind, Duration)> = Vec::new();
        let total_secs = TIMELINE_SPAN.as_secs();
        for sec in 1..=total_secs {
            clock.advance(Duration::from_secs(1));
            for event in engine.tick() {
                if let EngineEvent::BreakStarted(info) = event {
                    actual_starts.push((info.kind, Duration::from_secs(sec)));
                }
            }
        }

        assert_eq!(
            projected.marks.len(),
            actual_starts.len(),
            "mark count mismatch: projected={:?}, actual={:?}",
            projected.marks,
            actual_starts
        );

        for (mark, (actual_kind, actual_at)) in projected.marks.iter().zip(actual_starts.iter()) {
            assert_eq!(mark.kind, *actual_kind);
            let projected_secs = f64::from(mark.position) * TIMELINE_SPAN.as_secs_f64();
            let actual_secs = actual_at.as_secs_f64();
            assert!(
                (projected_secs - actual_secs).abs() <= 60.0,
                "mark {mark:?} at {projected_secs}s vs actual {actual_kind:?} at {actual_secs}s"
            );
        }
    }

    #[test]
    fn simulates_balanced_defaults_against_real_engine_for_2_hours() {
        run_engine_and_compare(OcySettings::default(), |_, _| {});
    }

    #[test]
    fn simulates_long_break_resetting_short_counter_with_non_divisible_intervals() {
        let mut s = OcySettings::default();
        s.short_breaks.interval_minutes = 17;
        s.short_breaks.duration_seconds = 20;
        s.long_breaks.interval_minutes = 45;
        s.long_breaks.duration_minutes = 5;
        s.long_breaks.warning_seconds = 60;
        run_engine_and_compare(s, |_, _| {});
    }

    #[test]
    fn simulates_short_break_suppression_near_long_break_with_zero_warning() {
        // Short at 29m (lasts 15s -> 29m15s), next short due at since_long=58m (2m before 60m)
        // must be suppressed by `is_short_break_suppressed_by_long_break`.
        let mut s = OcySettings::default();
        s.short_breaks.interval_minutes = 29;
        s.short_breaks.duration_seconds = 15;
        s.long_breaks.interval_minutes = 60;
        s.long_breaks.duration_minutes = 5;
        s.long_breaks.warning_seconds = 0;
        run_engine_and_compare(s, |_, _| {});
    }

    #[test]
    fn simulates_from_paused_until_time() {
        run_engine_and_compare(OcySettings::default(), |engine, clock| {
            // Work 7 minutes first so counters are non-zero before pausing; end_pause must reset them.
            for _ in 0..(7 * 60) {
                clock.advance(Duration::from_secs(1));
                let _ = engine.tick();
            }
            let _ = engine.pause_for(Some(Duration::from_secs(30 * 60)));
        });
    }

    #[test]
    fn simulates_from_in_progress_short_break() {
        let mut s = OcySettings::default();
        s.short_breaks.interval_minutes = 10;
        run_engine_and_compare(s, |engine, clock| {
            for _ in 0..(10 * 60 + 5) {
                clock.advance(Duration::from_secs(1));
                let _ = engine.tick();
            }
            assert_eq!(engine.phase(), EnginePhase::ShortBreak);
        });
    }

    #[test]
    fn simulates_from_in_progress_long_break() {
        run_engine_and_compare(OcySettings::default(), |engine, clock| {
            let _ = engine.start_long_break_now();
            clock.advance(Duration::from_secs(90));
            let _ = engine.tick();
            assert_eq!(engine.phase(), EnginePhase::LongBreak);
        });
    }

    #[test]
    fn simulates_from_long_break_warning() {
        let mut s = OcySettings::default();
        s.short_breaks.interval_minutes = 10;
        run_engine_and_compare(s, |engine, clock| {
            // 10 min short interval: 5 short breaks (75s) + 59 min working = 3615s reaches LongBreakWarning.
            for _ in 0..3615 {
                clock.advance(Duration::from_secs(1));
                let _ = engine.tick();
            }
            assert_eq!(engine.phase(), EnginePhase::LongBreakWarning);
        });
    }

    #[test]
    fn paused_indefinitely_produces_no_marks_and_preserves_labels() {
        let clock = FakeClock::from_utc(2026, 11, 20, 23, 15, 0).expect("clock");
        let snap = EngineSnapshot {
            phase: EnginePhase::Paused,
            time_until_short_break: Some(Duration::from_secs(300)),
            time_until_long_break: Some(Duration::from_secs(1800)),
            time_until_long_break_warning: None,
            current_break: None,
            paused_until_utc: Some(Paused::UntilResumed),
            postpones_left: 3,
            strict_mode: false,
            natural_hold: None,
        };
        let out = project(&snap, &OcySettings::default(), &clock);
        assert!(out.marks.is_empty());
        assert_eq!(out.labels, ["23:45", "00:15", "00:45", "01:15"]);
    }

    #[test]
    fn disabled_break_kinds_emit_no_marks_of_that_kind() {
        let mut no_short = OcySettings::default();
        no_short.short_breaks.enabled = false;
        run_engine_and_compare(no_short, |_, _| {});

        let mut no_long = OcySettings::default();
        no_long.long_breaks.enabled = false;
        run_engine_and_compare(no_long, |_, _| {});

        let mut neither = OcySettings::default();
        neither.short_breaks.enabled = false;
        neither.long_breaks.enabled = false;
        let clock = FakeClock::from_utc(2026, 11, 20, 10, 0, 0).expect("clock");
        let snap = EngineSnapshot {
            phase: EnginePhase::Working,
            time_until_short_break: None,
            time_until_long_break: None,
            time_until_long_break_warning: None,
            current_break: None,
            paused_until_utc: None,
            postpones_left: 3,
            strict_mode: false,
            natural_hold: None,
        };
        let out = project(&snap, &neither, &clock);
        assert!(out.marks.is_empty());
    }
}
