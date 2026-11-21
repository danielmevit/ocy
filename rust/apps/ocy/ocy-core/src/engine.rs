//! Ocy's core break engine, state, snapshots, and events.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use damt_platform::{Clock, FocusState, PlatformServices, SessionEvent};
use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::exercises::{DefaultExercisePicker, ExercisePicker};
use crate::persistence::{AppDataStore, PersistenceError};
use crate::schedule::ScheduleCalculator;
use crate::settings::OcySettings;
use crate::statistics::StatisticsData;
use crate::time::{DateTimeOffset, Paused};

/// Tick gaps strictly greater than 15 seconds are treated as sleep/away gaps.
const SLEEP_GAP_THRESHOLD: Duration = Duration::from_secs(15);

/// Idle time at which the user counts as paused long enough for a natural-moment start.
pub const NATURAL_LULL: Duration = Duration::from_secs(2);

/// Maximum time a scheduled start is held while waiting for a natural moment.
pub const NATURAL_MAX_HOLD: Duration = Duration::from_secs(60);

/// Kind of break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BreakKind {
    /// Short eye-rest break.
    Short,
    /// Long stretch and eye-rest break.
    Long,
}

/// Reason a running break ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BreakOutcome {
    /// Break ran to completion.
    Completed,
    /// Break was skipped by the user.
    Skipped,
    /// Break was interrupted (e.g. by pausing).
    Interrupted,
}

/// Current phase of [`BreakEngine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnginePhase {
    /// Active working time counting toward breaks.
    Working,
    /// Pre-break warning countdown before a long break.
    LongBreakWarning,
    /// Short break in progress.
    ShortBreak,
    /// Long break in progress.
    LongBreak,
    /// Long break deferred because a fullscreen or presentation app is active.
    Deferred,
    /// Breaks paused by the user.
    Paused,
    /// User is away (idle, locked, suspended, or screen saver).
    Away,
}

/// Metadata for a break that started or ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakInfo {
    /// Kind of break.
    pub kind: BreakKind,
    /// Primary exercise identifier.
    pub exercise_id: String,
    /// Total configured duration of the break.
    pub total_duration: Duration,
    /// Optional suggestion list (e.g. body + eyes for a long break).
    pub suggestion_exercise_ids: Option<Vec<String>>,
}

/// Live state of an in-progress break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentBreakInfo {
    /// Kind of break.
    pub kind: BreakKind,
    /// Primary exercise identifier.
    pub exercise_id: String,
    /// Total configured duration of the break.
    pub total_duration: Duration,
    /// Remaining break duration.
    pub remaining_duration: Duration,
    /// Optional suggestion list (e.g. body + eyes for a long break).
    pub suggestion_exercise_ids: Option<Vec<String>>,
}

impl CurrentBreakInfo {
    fn to_break_info(&self) -> BreakInfo {
        BreakInfo {
            kind: self.kind,
            exercise_id: self.exercise_id.clone(),
            total_duration: self.total_duration,
            suggestion_exercise_ids: self.suggestion_exercise_ids.clone(),
        }
    }
}

/// Persisted engine state (`state.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineState {
    /// JSON schema version (currently `1`).
    pub schema_version: i32,
    /// Active working time accumulated since the last short break.
    #[serde(with = "crate::time::dotnet_timespan")]
    pub since_short_break: Duration,
    /// Active working time accumulated since the last long break.
    #[serde(with = "crate::time::dotnet_timespan")]
    pub since_long_break: Duration,
    /// Number of postpones used in the current long-break cycle.
    pub postpones_used: u32,
    /// Optional pause deadline (or [`Paused::UntilResumed`]).
    #[serde(with = "crate::time::dotnet_paused_opt")]
    pub paused_until_utc: Option<Paused>,
    /// Timestamp of the last engine tick.
    #[serde(with = "crate::time::dotnet_datetime_offset_opt")]
    pub last_tick_utc: Option<DateTimeOffset>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            since_short_break: Duration::ZERO,
            since_long_break: Duration::ZERO,
            postpones_used: 0,
            paused_until_utc: None,
            last_tick_utc: None,
        }
    }
}

/// Kind of scheduled start that is currently held for a natural moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NaturalHoldKind {
    /// A scheduled short break is due and being held.
    ShortBreak,
    /// The scheduled long-break warning is due and being held.
    LongBreakWarning,
    /// A scheduled long break is due and being held.
    LongBreak,
}

/// A scheduled start held while waiting for a natural moment (see `natural_moments`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NaturalHold {
    /// Kind of scheduled start being held.
    kind: NaturalHoldKind,
    /// Instant the start first became due (the hold deadline is `due_at + NATURAL_MAX_HOLD`).
    due_at: Timestamp,
}

/// Point-in-time snapshot of [`BreakEngine`] state for hosts and UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSnapshot {
    /// Current phase.
    pub phase: EnginePhase,
    /// Remaining active time until short break, or `None` if disabled.
    pub time_until_short_break: Option<Duration>,
    /// Remaining active time until long break, or `None` if disabled.
    pub time_until_long_break: Option<Duration>,
    /// Remaining active time until the long-break warning begins, or `None` if disabled
    /// or no warning is configured.
    pub time_until_long_break_warning: Option<Duration>,
    /// Details of the currently running break, if any.
    pub current_break: Option<CurrentBreakInfo>,
    /// Active pause deadline, if paused.
    pub paused_until_utc: Option<Paused>,
    /// Remaining postpones allowed in the current long-break cycle.
    pub postpones_left: u32,
    /// Whether strict mode is active.
    pub strict_mode: bool,
    /// Kind of scheduled start currently held for a natural moment, if any.
    pub natural_hold: Option<NaturalHoldKind>,
}

/// Events emitted by [`BreakEngine`] methods in exact C# event order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// Raised whenever the phase changes or settings are updated.
    PhaseChanged(EngineSnapshot),
    /// Raised when a short or long break begins.
    BreakStarted(BreakInfo),
    /// Raised when a short or long break finishes, is skipped, or is interrupted.
    BreakEnded(BreakInfo, BreakOutcome),
    /// Raised when the long-break pre-warning countdown begins.
    LongBreakWarningStarted(Duration),
    /// Raised when a long absence is credited as a break.
    AbsenceCredited(BreakKind),
}

/// Core state machine for Ocy's break schedule, away detection, pauses, and statistics.
pub struct BreakEngine {
    platform: Arc<dyn PlatformServices>,
    clock: Arc<dyn Clock>,
    exercise_picker: Box<dyn ExercisePicker>,
    data_store: Option<AppDataStore>,
    stats: StatisticsData,

    settings: OcySettings,
    phase: EnginePhase,
    since_short_break: Duration,
    since_long_break: Duration,
    postpones_used: u32,
    paused_until: Option<Paused>,
    last_tick_utc: Option<DateTimeOffset>,
    away_started_utc: Option<Timestamp>,
    is_session_away: bool,
    is_focus_away: bool,
    warning_countdown_remaining: Duration,
    current_break: Option<CurrentBreakInfo>,
    natural_hold: Option<NaturalHold>,
    previous_short_exercise_id: Option<&'static str>,
    previous_body_exercise_id: Option<&'static str>,
    previous_eyes_exercise_id: Option<&'static str>,
    active_seconds_accumulator: f64,
    pending_startup_gap: Option<Duration>,
}

impl fmt::Debug for BreakEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BreakEngine")
            .field("phase", &self.phase)
            .field("since_short_break", &self.since_short_break)
            .field("since_long_break", &self.since_long_break)
            .field("postpones_used", &self.postpones_used)
            .field("paused_until", &self.paused_until)
            .field("last_tick_utc", &self.last_tick_utc)
            .field("current_break", &self.current_break)
            .finish_non_exhaustive()
    }
}

impl Drop for BreakEngine {
    fn drop(&mut self) {
        let _ = self.save();
    }
}

impl BreakEngine {
    /// Creates a new `BreakEngine` mirroring the C# `BreakEngine` constructor.
    #[must_use]
    pub fn new(
        mut settings: OcySettings,
        platform: Arc<dyn PlatformServices>,
        clock: Arc<dyn Clock>,
        data_store: Option<AppDataStore>,
        exercise_picker: Option<Box<dyn ExercisePicker>>,
        initial_state: Option<EngineState>,
        initial_stats: Option<StatisticsData>,
    ) -> Self {
        settings.clamp();
        let picker = exercise_picker.unwrap_or_else(|| Box::new(DefaultExercisePicker::default()));

        let stats = initial_stats.unwrap_or_else(|| {
            data_store
                .as_ref()
                .map_or_else(StatisticsData::default, AppDataStore::load_stats)
        });
        let state_to_load =
            initial_state.or_else(|| data_store.as_ref().map(AppDataStore::load_state));

        let mut engine = Self {
            platform,
            clock,
            exercise_picker: picker,
            data_store,
            stats,
            settings,
            phase: EnginePhase::Working,
            since_short_break: Duration::ZERO,
            since_long_break: Duration::ZERO,
            postpones_used: 0,
            paused_until: None,
            last_tick_utc: None,
            away_started_utc: None,
            is_session_away: false,
            is_focus_away: false,
            warning_countdown_remaining: Duration::ZERO,
            current_break: None,
            natural_hold: None,
            previous_short_exercise_id: None,
            previous_body_exercise_id: None,
            previous_eyes_exercise_id: None,
            active_seconds_accumulator: 0.0,
            pending_startup_gap: None,
        };

        engine.initialize_state(state_to_load);
        engine
    }

    /// Convenience constructor with default picker and no persistence or initial state.
    #[must_use]
    pub fn with_defaults(
        settings: OcySettings,
        platform: Arc<dyn PlatformServices>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::new(settings, platform, clock, None, None, None, None)
    }

    /// Current engine phase.
    #[must_use]
    pub const fn phase(&self) -> EnginePhase {
        self.phase
    }

    /// Current clamped settings.
    #[must_use]
    pub const fn settings(&self) -> &OcySettings {
        &self.settings
    }

    /// Accumulated daily statistics.
    #[must_use]
    pub const fn statistics(&self) -> &StatisticsData {
        &self.stats
    }

    /// Active working time since the last short break.
    #[must_use]
    pub const fn since_short_break(&self) -> Duration {
        self.since_short_break
    }

    /// Active working time since the last long break.
    #[must_use]
    pub const fn since_long_break(&self) -> Duration {
        self.since_long_break
    }

    /// Number of postpones used in the current long-break cycle.
    #[must_use]
    pub const fn postpones_used(&self) -> u32 {
        self.postpones_used
    }

    /// Active pause target, if paused.
    #[must_use]
    pub const fn paused_until(&self) -> Option<Paused> {
        self.paused_until
    }

    /// Timestamp of the most recent tick or initialization.
    #[must_use]
    pub const fn last_tick_utc(&self) -> Option<DateTimeOffset> {
        self.last_tick_utc
    }

    /// Active break information, if currently in a short or long break.
    #[must_use]
    pub const fn current_break(&self) -> Option<&CurrentBreakInfo> {
        self.current_break.as_ref()
    }

    /// Returns a point-in-time snapshot of the engine state.
    #[must_use]
    pub fn snapshot(&self) -> EngineSnapshot {
        let time_until_short =
            ScheduleCalculator::time_until_short_break(&self.settings, self.since_short_break);
        let time_until_long =
            ScheduleCalculator::time_until_long_break(&self.settings, self.since_long_break);
        let time_until_long_warning = ScheduleCalculator::time_until_long_break_warning(
            &self.settings,
            self.since_long_break,
        );
        let max_postpones =
            u32::try_from(self.settings.long_breaks.max_postpones.max(0)).unwrap_or(0);
        let postpones_left = max_postpones.saturating_sub(self.postpones_used);

        EngineSnapshot {
            phase: self.phase,
            time_until_short_break: time_until_short,
            time_until_long_break: time_until_long,
            time_until_long_break_warning: time_until_long_warning,
            current_break: self.current_break.clone(),
            paused_until_utc: self.paused_until,
            postpones_left,
            strict_mode: self.settings.strict_mode,
            natural_hold: self.natural_hold.map(|hold| hold.kind),
        }
    }

    /// Exports the current persistable [`EngineState`].
    #[must_use]
    pub fn export_state(&self) -> EngineState {
        EngineState {
            schema_version: 1,
            since_short_break: self.since_short_break,
            since_long_break: self.since_long_break,
            postpones_used: self.postpones_used,
            paused_until_utc: self.paused_until,
            last_tick_utc: self.last_tick_utc,
        }
    }

    /// Saves `state.json` and prunes/saves `stats.json` if an [`AppDataStore`] was provided.
    pub fn save(&mut self) -> Result<(), PersistenceError> {
        let state = self.export_state();
        let today = self.local_date();
        self.stats.prune_default_retention(today);

        if let Some(store) = &self.data_store {
            store.save_state(&state)?;
            store.save_stats(&self.stats)?;
        }
        Ok(())
    }

    /// Advances the engine by one tick using the injected [`Clock`] and [`PlatformServices`].
    pub fn tick(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let now_zoned = self.clock.now();
        let now_ts = now_zoned.timestamp();
        let now_utc = DateTimeOffset::from_utc(now_ts);
        let local_date = now_zoned.date();

        // Rule 6: An active pause survives restarts, sleep gaps, and session events.
        if self.phase == EnginePhase::Paused {
            self.pending_startup_gap = None;
            self.last_tick_utc = Some(now_utc);
            if let Some(paused) = self.paused_until
                && paused.is_expired(now_ts)
            {
                self.end_pause(&mut events);
            }
            return events;
        }

        if let Some(gap) = self.pending_startup_gap.take() {
            self.last_tick_utc = Some(now_utc);
            self.apply_return_span(gap, now_ts, local_date, &mut events);
            return events;
        }

        let Some(last_tick) = self.last_tick_utc else {
            self.last_tick_utc = Some(now_utc);
            return events;
        };

        let dt_signed = now_ts.duration_since(last_tick.timestamp());
        self.last_tick_utc = Some(now_utc);

        if dt_signed <= SignedDuration::ZERO {
            return events;
        }
        let Ok(dt) = Duration::try_from(dt_signed) else {
            return events;
        };

        // Rule 1: Gap = sleep. If dt > 15s, treat whole dt as away span and do not add to counters.
        if dt > SLEEP_GAP_THRESHOLD {
            if self.phase == EnginePhase::ShortBreak || self.phase == EnginePhase::LongBreak {
                let focus_in_break = self.current_focus_state();
                if focus_in_break == FocusState::NotPresent {
                    self.is_focus_away = true;
                }
                self.update_break(dt, now_ts, local_date, &mut events);
                return events;
            }

            // Rule 8: A gap while already Away extends the same away span.
            if self.phase == EnginePhase::Away {
                return events;
            }

            if self.phase == EnginePhase::LongBreakWarning {
                self.warning_countdown_remaining = Duration::ZERO;
            }

            self.apply_return_span(dt, now_ts, local_date, &mut events);
            return events;
        }

        // Check focus for NotPresent (session lock / screen saver)
        let focus = self.current_focus_state();
        if focus == FocusState::NotPresent {
            if !self.is_focus_away {
                self.is_focus_away = true;
                if matches!(
                    self.phase,
                    EnginePhase::Working | EnginePhase::Deferred | EnginePhase::LongBreakWarning
                ) {
                    self.enter_away(now_ts, &mut events);
                }
            }
        } else if self.is_focus_away {
            self.is_focus_away = false;
            if !self.is_session_away
                && self.phase != EnginePhase::ShortBreak
                && self.phase != EnginePhase::LongBreak
            {
                self.return_from_away(now_ts, local_date, &mut events);
                return events;
            }
        }

        // Rule 3: Inside a break
        if self.phase == EnginePhase::ShortBreak || self.phase == EnginePhase::LongBreak {
            self.update_break(dt, now_ts, local_date, &mut events);
            return events;
        }

        if self.is_session_away || self.is_focus_away {
            return events;
        }

        // Rule 4: Away / Inactivity
        if self.phase == EnginePhase::Away {
            let idle = if self.platform.idle().is_supported() {
                self.platform.idle().idle_time()
            } else {
                Duration::ZERO
            };
            if idle < self.settings.away_threshold() {
                self.return_from_away(now_ts, local_date, &mut events);
            }
            return events;
        }

        if self.settings.inactivity_tracking && self.platform.idle().is_supported() {
            let idle = self.platform.idle().idle_time();
            if idle >= self.settings.away_threshold()
                && matches!(
                    self.phase,
                    EnginePhase::Working | EnginePhase::Deferred | EnginePhase::LongBreakWarning
                )
            {
                let started = subtract_duration(now_ts, idle);
                self.enter_away(started, &mut events);
                return events;
            }
        }

        // LongBreakWarning
        if self.phase == EnginePhase::LongBreakWarning {
            if dt >= self.warning_countdown_remaining {
                self.warning_countdown_remaining = Duration::ZERO;
                if self.hold_scheduled_start(NaturalHoldKind::LongBreak, now_ts) {
                    return events;
                }
                self.start_long_break(&mut events);
            } else {
                self.warning_countdown_remaining -= dt;
            }
            return events;
        }

        // Deferred
        if self.phase == EnginePhase::Deferred {
            let is_fullscreen_now = self.is_fullscreen_focus(focus);
            if !is_fullscreen_now {
                if self.settings.long_breaks.warning_seconds > 0 {
                    if self.hold_scheduled_start(NaturalHoldKind::LongBreakWarning, now_ts) {
                        return events;
                    }
                    self.start_long_break_warning(&mut events);
                } else {
                    if self.hold_scheduled_start(NaturalHoldKind::LongBreak, now_ts) {
                        return events;
                    }
                    self.start_long_break(&mut events);
                }
            }
            return events;
        }

        // Rule 5: Working
        if self.phase == EnginePhase::Working {
            self.since_short_break += dt;
            self.since_long_break += dt;
            self.active_seconds_accumulator += dt.as_secs_f64();
            while self.active_seconds_accumulator >= 60.0 {
                self.active_seconds_accumulator -= 60.0;
                self.stats.get_or_create(local_date).active_minutes += 1;
            }

            let is_fullscreen = self.is_fullscreen_focus(focus);

            // Check long break warning due
            if self.settings.long_breaks.enabled {
                let warning_at = self
                    .settings
                    .long_breaks
                    .interval()
                    .saturating_sub(self.settings.long_breaks.warning_duration());
                if self.since_long_break >= warning_at {
                    if is_fullscreen {
                        self.transition_to(EnginePhase::Deferred, &mut events);
                        return events;
                    }

                    if self.settings.long_breaks.warning_seconds > 0 {
                        if self.hold_scheduled_start(NaturalHoldKind::LongBreakWarning, now_ts) {
                            return events;
                        }
                        self.start_long_break_warning(&mut events);
                        return events;
                    }

                    if self.since_long_break >= self.settings.long_breaks.interval() {
                        if self.hold_scheduled_start(NaturalHoldKind::LongBreak, now_ts) {
                            return events;
                        }
                        self.start_long_break(&mut events);
                        return events;
                    }
                }
            }

            // Check short break due
            if self.settings.short_breaks.enabled
                && self.since_short_break >= self.settings.short_breaks.interval()
            {
                if is_fullscreen {
                    self.since_short_break = Duration::ZERO;
                    return events;
                }

                if ScheduleCalculator::is_short_break_suppressed_by_long_break(
                    &self.settings,
                    self.since_long_break,
                ) {
                    self.since_short_break = Duration::ZERO;
                    return events;
                }

                if self.hold_scheduled_start(NaturalHoldKind::ShortBreak, now_ts) {
                    return events;
                }

                self.start_short_break(&mut events);
            }
        }

        events
    }

    /// Starts a long break immediately if in `Working`, `LongBreakWarning`, or `Deferred`.
    pub fn start_long_break_now(&mut self) -> (bool, Vec<EngineEvent>) {
        if matches!(
            self.phase,
            EnginePhase::Working | EnginePhase::LongBreakWarning | EnginePhase::Deferred
        ) {
            let mut events = Vec::new();
            self.start_long_break(&mut events);
            (true, events)
        } else {
            (false, Vec::new())
        }
    }

    /// Starts a short break immediately ("try short break") if in `Working` or `Deferred`.
    pub fn start_short_break_now(&mut self) -> (bool, Vec<EngineEvent>) {
        if matches!(self.phase, EnginePhase::Working | EnginePhase::Deferred) {
            let mut events = Vec::new();
            self.start_short_break(&mut events);
            (true, events)
        } else {
            (false, Vec::new())
        }
    }

    /// Postpones the upcoming long break while in `LongBreakWarning`.
    pub fn postpone_long_break(&mut self) -> (bool, Vec<EngineEvent>) {
        if self.phase != EnginePhase::LongBreakWarning || self.settings.strict_mode {
            return (false, Vec::new());
        }

        let max_postpones =
            u32::try_from(self.settings.long_breaks.max_postpones.max(0)).unwrap_or(0);
        if self.postpones_used >= max_postpones {
            return (false, Vec::new());
        }

        self.postpones_used += 1;
        self.natural_hold = None;
        let today = self.local_date();
        self.stats.get_or_create(today).long_postponed += 1;

        self.since_long_break = self
            .settings
            .long_breaks
            .interval()
            .saturating_sub(self.settings.long_breaks.postpone_duration());

        let mut events = Vec::new();
        self.transition_to(EnginePhase::Working, &mut events);
        (true, events)
    }

    /// Skips the current break or warning (refused for long breaks/warnings in strict mode).
    pub fn skip_current_break(&mut self) -> (bool, Vec<EngineEvent>) {
        let now_zoned = self.clock.now();
        let now_ts = now_zoned.timestamp();
        let local_date = now_zoned.date();
        let mut events = Vec::new();

        match self.phase {
            EnginePhase::ShortBreak => {
                let break_info = self.current_break.as_ref().map_or_else(
                    || BreakInfo {
                        kind: BreakKind::Short,
                        exercise_id: String::new(),
                        total_duration: self.settings.short_breaks.duration(),
                        suggestion_exercise_ids: None,
                    },
                    CurrentBreakInfo::to_break_info,
                );

                self.current_break = None;
                self.since_short_break = Duration::ZERO;
                self.stats.get_or_create(local_date).short_skipped += 1;
                events.push(EngineEvent::BreakEnded(break_info, BreakOutcome::Skipped));

                if self.is_session_away || self.is_focus_away {
                    self.enter_away(now_ts, &mut events);
                } else {
                    self.transition_to(EnginePhase::Working, &mut events);
                }
                (true, events)
            }
            EnginePhase::LongBreak => {
                if self.settings.strict_mode {
                    return (false, events);
                }

                let break_info = self.current_break.as_ref().map_or_else(
                    || BreakInfo {
                        kind: BreakKind::Long,
                        exercise_id: String::new(),
                        total_duration: self.settings.long_breaks.duration(),
                        suggestion_exercise_ids: None,
                    },
                    CurrentBreakInfo::to_break_info,
                );

                self.current_break = None;
                self.since_long_break = Duration::ZERO;
                self.since_short_break = Duration::ZERO;
                self.postpones_used = 0;
                self.stats.get_or_create(local_date).long_skipped += 1;
                events.push(EngineEvent::BreakEnded(break_info, BreakOutcome::Skipped));

                if self.is_session_away || self.is_focus_away {
                    self.enter_away(now_ts, &mut events);
                } else {
                    self.transition_to(EnginePhase::Working, &mut events);
                }
                (true, events)
            }
            EnginePhase::LongBreakWarning => {
                if self.settings.strict_mode {
                    return (false, events);
                }

                self.warning_countdown_remaining = Duration::ZERO;
                self.natural_hold = None;
                self.since_long_break = Duration::ZERO;
                self.since_short_break = Duration::ZERO;
                self.postpones_used = 0;
                self.stats.get_or_create(local_date).long_skipped += 1;
                self.transition_to(EnginePhase::Working, &mut events);
                (true, events)
            }
            _ => (false, events),
        }
    }

    /// Pauses breaks for `duration`, or indefinitely when `duration` is `None`.
    pub fn pause_for(&mut self, duration: Option<Duration>) -> Vec<EngineEvent> {
        let now_utc = DateTimeOffset::from_utc(self.clock.now().timestamp());
        self.paused_until = Some(match duration {
            Some(d) => match now_utc.checked_add(d) {
                Some(until) => Paused::Until(until),
                None => Paused::UntilResumed,
            },
            None => Paused::UntilResumed,
        });

        self.natural_hold = None;

        let mut events = Vec::new();
        if self.phase == EnginePhase::ShortBreak || self.phase == EnginePhase::LongBreak {
            let fallback_kind = if self.phase == EnginePhase::ShortBreak {
                BreakKind::Short
            } else {
                BreakKind::Long
            };
            let break_info = self.current_break.as_ref().map_or_else(
                || BreakInfo {
                    kind: fallback_kind,
                    exercise_id: String::new(),
                    total_duration: Duration::ZERO,
                    suggestion_exercise_ids: None,
                },
                CurrentBreakInfo::to_break_info,
            );
            self.current_break = None;
            events.push(EngineEvent::BreakEnded(
                break_info,
                BreakOutcome::Interrupted,
            ));
        } else {
            self.current_break = None;
        }

        self.transition_to(EnginePhase::Paused, &mut events);
        events
    }

    /// Ends an active pause and resets counters to `Working`.
    pub fn resume(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        if self.phase == EnginePhase::Paused {
            self.end_pause(&mut events);
        }
        events
    }

    /// Handles an OS session event (`Locked`, `Unlocked`, `Suspending`, `Resumed`).
    pub fn on_session_event(&mut self, event: SessionEvent) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let now_zoned = self.clock.now();
        let now_ts = now_zoned.timestamp();
        let local_date = now_zoned.date();

        match event {
            SessionEvent::Locked | SessionEvent::Suspending => {
                if !self.is_session_away {
                    self.is_session_away = true;
                    if matches!(
                        self.phase,
                        EnginePhase::Working
                            | EnginePhase::Deferred
                            | EnginePhase::LongBreakWarning
                    ) {
                        self.enter_away(now_ts, &mut events);
                    }
                }
            }
            SessionEvent::Unlocked | SessionEvent::Resumed => {
                if self.is_session_away {
                    self.is_session_away = false;
                    if self.phase == EnginePhase::ShortBreak || self.phase == EnginePhase::LongBreak
                    {
                        return events;
                    }
                    if !self.is_focus_away {
                        self.return_from_away(now_ts, local_date, &mut events);
                    }
                }
            }
            SessionEvent::LoggingOff => {}
        }

        events
    }

    /// Applies updated settings immediately, clamping them and emitting [`EngineEvent::PhaseChanged`].
    pub fn update_settings(&mut self, mut settings: OcySettings) -> Vec<EngineEvent> {
        settings.clamp();
        self.settings = settings;
        self.natural_hold = None;
        vec![EngineEvent::PhaseChanged(self.snapshot())]
    }

    fn initialize_state(&mut self, state: Option<EngineState>) {
        let now_ts = self.clock.now().timestamp();
        let now_utc = DateTimeOffset::from_utc(now_ts);

        let Some(state) = state else {
            self.last_tick_utc = Some(now_utc);
            return;
        };

        self.since_short_break = state.since_short_break;
        self.since_long_break = state.since_long_break;
        self.postpones_used = state.postpones_used;
        self.paused_until = state.paused_until_utc;

        if let Some(paused) = self.paused_until {
            if paused.is_expired(now_ts) {
                self.paused_until = None;
                self.since_short_break = Duration::ZERO;
                self.since_long_break = Duration::ZERO;
                self.postpones_used = 0;
            } else {
                self.phase = EnginePhase::Paused;
            }
        } else if let Some(last_tick) = state.last_tick_utc {
            let signed_gap = now_ts.duration_since(last_tick.timestamp());
            if let Ok(gap) = Duration::try_from(signed_gap)
                && gap > SLEEP_GAP_THRESHOLD
            {
                self.pending_startup_gap = Some(gap);
            }
        }

        self.last_tick_utc = Some(now_utc);
    }

    fn current_focus_state(&self) -> FocusState {
        if self.platform.focus().is_supported() {
            self.platform.focus().state()
        } else {
            FocusState::Unknown
        }
    }

    fn is_fullscreen_focus(&self, focus: FocusState) -> bool {
        self.settings.pause_during_fullscreen
            && matches!(focus, FocusState::FullscreenApp | FocusState::Presentation)
    }

    /// Decides whether a scheduled start of `kind` should be held for a natural moment.
    ///
    /// Returns `true` when the start must wait (the hold was created or extended): the
    /// `natural_moments` setting is on, the idle provider is supported, and the user is
    /// still active (idle < [`NATURAL_LULL`]). The start happens at the first tick with a
    /// lull, or once [`NATURAL_MAX_HOLD`] has passed since the start became due.
    /// Explicit starts never pass through here and are never held.
    fn hold_scheduled_start(&mut self, kind: NaturalHoldKind, now_ts: Timestamp) -> bool {
        if !self.settings.natural_moments || !self.platform.idle().is_supported() {
            return false;
        }
        if self.platform.idle().idle_time() >= NATURAL_LULL {
            return false;
        }

        match self.natural_hold {
            Some(hold) if hold.kind == kind => {
                let elapsed = now_ts.duration_since(hold.due_at);
                SignedDuration::try_from(NATURAL_MAX_HOLD).is_ok_and(|max| elapsed < max)
            }
            _ => {
                self.natural_hold = Some(NaturalHold {
                    kind,
                    due_at: now_ts,
                });
                true
            }
        }
    }

    fn start_short_break(&mut self, events: &mut Vec<EngineEvent>) {
        self.natural_hold = None;
        let exercise = self.exercise_picker.pick_short_exercise(
            self.settings.has_window_nearby,
            self.previous_short_exercise_id,
        );
        self.previous_short_exercise_id = Some(exercise.id);
        let duration = self.settings.short_breaks.duration();

        self.current_break = Some(CurrentBreakInfo {
            kind: BreakKind::Short,
            exercise_id: exercise.id.to_owned(),
            total_duration: duration,
            remaining_duration: duration,
            suggestion_exercise_ids: None,
        });
        self.transition_to(EnginePhase::ShortBreak, events);
        events.push(EngineEvent::BreakStarted(BreakInfo {
            kind: BreakKind::Short,
            exercise_id: exercise.id.to_owned(),
            total_duration: duration,
            suggestion_exercise_ids: None,
        }));
    }

    fn start_long_break_warning(&mut self, events: &mut Vec<EngineEvent>) {
        self.natural_hold = None;
        let countdown = self.settings.long_breaks.warning_duration();
        self.warning_countdown_remaining = countdown;
        self.transition_to(EnginePhase::LongBreakWarning, events);
        events.push(EngineEvent::LongBreakWarningStarted(countdown));
    }

    fn start_long_break(&mut self, events: &mut Vec<EngineEvent>) {
        self.natural_hold = None;
        let (body_ex, eyes_ex) = self.exercise_picker.pick_long_exercises(
            self.settings.has_window_nearby,
            self.previous_eyes_exercise_id,
            self.previous_body_exercise_id,
        );
        self.previous_eyes_exercise_id = Some(eyes_ex.id);
        self.previous_body_exercise_id = Some(body_ex.id);

        let suggestions = vec![body_ex.id.to_owned(), eyes_ex.id.to_owned()];
        let duration = self.settings.long_breaks.duration();

        self.current_break = Some(CurrentBreakInfo {
            kind: BreakKind::Long,
            exercise_id: body_ex.id.to_owned(),
            total_duration: duration,
            remaining_duration: duration,
            suggestion_exercise_ids: Some(suggestions.clone()),
        });
        self.transition_to(EnginePhase::LongBreak, events);
        events.push(EngineEvent::BreakStarted(BreakInfo {
            kind: BreakKind::Long,
            exercise_id: body_ex.id.to_owned(),
            total_duration: duration,
            suggestion_exercise_ids: Some(suggestions),
        }));
    }

    fn update_break(
        &mut self,
        dt: Duration,
        now_ts: Timestamp,
        local_date: Date,
        events: &mut Vec<EngineEvent>,
    ) {
        let Some(current) = self.current_break.take() else {
            if self.is_session_away || self.is_focus_away {
                self.enter_away(now_ts, events);
            } else {
                self.transition_to(EnginePhase::Working, events);
            }
            return;
        };

        if dt >= current.remaining_duration {
            let overshoot = dt - current.remaining_duration;
            let break_ended_utc = subtract_duration(now_ts, overshoot);
            let break_info = current.to_break_info();

            let today = self.stats.get_or_create(local_date);
            match current.kind {
                BreakKind::Short => {
                    self.since_short_break = Duration::ZERO;
                    today.short_completed += 1;
                }
                BreakKind::Long => {
                    self.since_long_break = Duration::ZERO;
                    self.since_short_break = Duration::ZERO;
                    self.postpones_used = 0;
                    today.long_completed += 1;
                }
            }

            events.push(EngineEvent::BreakEnded(break_info, BreakOutcome::Completed));

            if self.is_session_away || self.is_focus_away {
                self.enter_away(break_ended_utc, events);
            } else {
                self.transition_to(EnginePhase::Working, events);
            }
        } else {
            self.current_break = Some(CurrentBreakInfo {
                remaining_duration: current.remaining_duration - dt,
                ..current
            });
        }
    }

    fn enter_away(&mut self, away_started_utc: Timestamp, events: &mut Vec<EngineEvent>) {
        if self.phase == EnginePhase::Away {
            return;
        }
        self.away_started_utc = Some(away_started_utc);
        self.natural_hold = None;
        self.warning_countdown_remaining = Duration::ZERO;
        self.transition_to(EnginePhase::Away, events);
    }

    fn return_from_away(
        &mut self,
        now_ts: Timestamp,
        local_date: Date,
        events: &mut Vec<EngineEvent>,
    ) {
        let away = self.away_started_utc.map_or(Duration::ZERO, |started| {
            Duration::try_from(now_ts.duration_since(started)).unwrap_or(Duration::ZERO)
        });
        self.away_started_utc = None;
        self.last_tick_utc = Some(DateTimeOffset::from_utc(now_ts));

        if self.phase != EnginePhase::Away {
            return;
        }

        self.apply_return_rules(away, local_date, events);
        self.transition_to(EnginePhase::Working, events);
    }

    fn apply_return_span(
        &mut self,
        away: Duration,
        now_ts: Timestamp,
        local_date: Date,
        events: &mut Vec<EngineEvent>,
    ) {
        self.natural_hold = None;
        self.apply_return_rules(away, local_date, events);

        if self.settings.inactivity_tracking && self.platform.idle().is_supported() {
            let idle = self.platform.idle().idle_time();
            if idle >= self.settings.away_threshold() {
                let started = subtract_duration(now_ts, idle);
                self.enter_away(started, events);
                return;
            }
        }

        let focus = self.current_focus_state();
        if focus == FocusState::NotPresent {
            self.is_focus_away = true;
            self.enter_away(now_ts, events);
            return;
        }

        self.transition_to(EnginePhase::Working, events);
    }

    fn apply_return_rules(
        &mut self,
        away: Duration,
        local_date: Date,
        events: &mut Vec<EngineEvent>,
    ) {
        if away >= self.settings.long_breaks.duration() {
            self.since_short_break = Duration::ZERO;
            self.since_long_break = Duration::ZERO;
            self.postpones_used = 0;
            self.stats.get_or_create(local_date).absences_credited += 1;
            events.push(EngineEvent::AbsenceCredited(BreakKind::Long));
        } else if away >= self.settings.short_breaks.duration() {
            self.since_short_break = Duration::ZERO;
        }
    }

    fn end_pause(&mut self, events: &mut Vec<EngineEvent>) {
        self.paused_until = None;
        self.since_short_break = Duration::ZERO;
        self.since_long_break = Duration::ZERO;
        self.postpones_used = 0;
        self.transition_to(EnginePhase::Working, events);
    }

    fn transition_to(&mut self, new_phase: EnginePhase, events: &mut Vec<EngineEvent>) {
        if self.phase == new_phase {
            return;
        }
        self.phase = new_phase;
        events.push(EngineEvent::PhaseChanged(self.snapshot()));
    }

    fn local_date(&self) -> Date {
        self.clock.now().date()
    }
}

fn subtract_duration(ts: Timestamp, d: Duration) -> Timestamp {
    SignedDuration::try_from(d)
        .ok()
        .and_then(|sd| ts.checked_sub(sd).ok())
        .unwrap_or(ts)
}
