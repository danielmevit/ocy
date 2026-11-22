//! Differential-testing harness for `ocy-core` mirroring `rust/tools/diff/csharp-harness`.

use std::env;
use std::fs;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_platform::{
    Clock, FocusState, FocusStateProvider, IdleProvider, PlatformServices, SessionEvent,
};
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};
use ocy_core::engine::{
    BreakEngine, BreakKind, BreakOutcome, CurrentBreakInfo, EngineEvent, EnginePhase, EngineState,
};
use ocy_core::exercises::{DefaultExercisePicker, RandomSource};
use ocy_core::settings::OcySettings;
use ocy_core::statistics::DailyStatistics;
use ocy_core::time::{FakeClock, Paused};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug)]
struct FakeIdleProvider {
    supported: AtomicBool,
    idle_nanos: AtomicU64,
}

impl FakeIdleProvider {
    fn new(supported: bool) -> Self {
        Self {
            supported: AtomicBool::new(supported),
            idle_nanos: AtomicU64::new(0),
        }
    }

    fn set_idle_ms(&self, ms: u64) {
        self.idle_nanos
            .store(ms.saturating_mul(1_000_000), Ordering::Relaxed);
    }
}

impl IdleProvider for FakeIdleProvider {
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
struct FakeFocusStateProvider {
    supported: AtomicBool,
    state: Mutex<FocusState>,
}

impl FakeFocusStateProvider {
    fn new(supported: bool) -> Self {
        Self {
            supported: AtomicBool::new(supported),
            state: Mutex::new(FocusState::Normal),
        }
    }

    fn set_state(&self, state: FocusState) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
    }
}

impl FocusStateProvider for FakeFocusStateProvider {
    fn is_supported(&self) -> bool {
        self.supported.load(Ordering::Relaxed)
    }

    fn state(&self) -> FocusState {
        if !self.is_supported() {
            return FocusState::Unknown;
        }
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Debug)]
struct FakePlatformServices {
    idle: FakeIdleProvider,
    focus: FakeFocusStateProvider,
}

impl FakePlatformServices {
    fn new(idle_supported: bool, focus_supported: bool) -> Arc<Self> {
        Arc::new(Self {
            idle: FakeIdleProvider::new(idle_supported),
            focus: FakeFocusStateProvider::new(focus_supported),
        })
    }
}

impl PlatformServices for FakePlatformServices {
    fn idle(&self) -> &dyn IdleProvider {
        &self.idle
    }

    fn focus(&self) -> &dyn FocusStateProvider {
        &self.focus
    }
}

#[derive(Debug, Clone, Copy)]
struct ZeroRng;

impl RandomSource for ZeroRng {
    fn next_usize(&mut self, _upper_bound: usize) -> usize {
        0
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scenario {
    initial_utc: String,
    time_zone: String,
    settings: Option<OcySettings>,
    initial_state: Option<EngineState>,
    idle_supported: Option<bool>,
    focus_supported: Option<bool>,
    steps: Vec<ScenarioStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioStep {
    advance_ms: u64,
    idle_ms: u64,
    focus: String,
    action: Option<StepAction>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StepAction {
    None,
    Tick,
    StartLongNow,
    StartShortNow,
    Postpone,
    Skip,
    Pause { minutes: Option<u64> },
    Resume,
    Session { kind: String },
    UpdateSettings { settings: OcySettings },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutCurrentBreak {
    kind: &'static str,
    exercise_id: String,
    total_ms: i64,
    remaining_ms: i64,
    suggestion_exercise_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exercise_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion_exercise_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutTodayStats {
    date: String,
    short_completed: u32,
    short_skipped: u32,
    long_completed: u32,
    long_skipped: u32,
    long_postponed: u32,
    absences_credited: u32,
    active_minutes: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutSnapshot {
    phase: &'static str,
    time_until_short_break_ms: Option<i64>,
    time_until_long_break_ms: Option<i64>,
    current_break: Option<OutCurrentBreak>,
    paused_until: Option<String>,
    postpones_left: u32,
    strict_mode: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutStepLine {
    step: usize,
    phase: &'static str,
    since_short_ms: i64,
    since_long_ms: i64,
    postpones_used: u32,
    paused_until: Option<String>,
    current_break: Option<OutCurrentBreak>,
    action_result: Option<bool>,
    events: Vec<OutEvent>,
    today_stats: OutTodayStats,
    snapshot: OutSnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutFinalLine {
    exported_state: Value,
}

fn phase_str(phase: EnginePhase) -> &'static str {
    match phase {
        EnginePhase::Working => "Working",
        EnginePhase::LongBreakWarning => "LongBreakWarning",
        EnginePhase::ShortBreak => "ShortBreak",
        EnginePhase::LongBreak => "LongBreak",
        EnginePhase::Deferred => "Deferred",
        EnginePhase::Paused => "Paused",
        EnginePhase::Away => "Away",
    }
}

fn kind_str(kind: BreakKind) -> &'static str {
    match kind {
        BreakKind::Short => "Short",
        BreakKind::Long => "Long",
    }
}

fn outcome_str(outcome: BreakOutcome) -> &'static str {
    match outcome {
        BreakOutcome::Completed => "Completed",
        BreakOutcome::Skipped => "Skipped",
        BreakOutcome::Interrupted => "Interrupted",
        BreakOutcome::Passed => "Passed",
        BreakOutcome::MissedInGame => "MissedInGame",
    }
}

fn parse_focus(s: &str) -> Result<FocusState, String> {
    match s {
        "Unknown" => Ok(FocusState::Unknown),
        "Normal" => Ok(FocusState::Normal),
        "FullscreenApp" => Ok(FocusState::FullscreenApp),
        "Presentation" => Ok(FocusState::Presentation),
        "QuietTime" => Ok(FocusState::QuietTime),
        "NotPresent" => Ok(FocusState::NotPresent),
        _ => Err(format!("unknown focus state: {s}")),
    }
}

fn parse_session_event(s: &str) -> Result<SessionEvent, String> {
    match s {
        "Locked" => Ok(SessionEvent::Locked),
        "Unlocked" => Ok(SessionEvent::Unlocked),
        "Suspending" => Ok(SessionEvent::Suspending),
        "Resumed" => Ok(SessionEvent::Resumed),
        "LoggingOff" => Ok(SessionEvent::LoggingOff),
        _ => Err(format!("unknown session event: {s}")),
    }
}

fn to_ms(d: Duration) -> i64 {
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

fn format_paused(paused: Option<Paused>) -> Option<String> {
    paused.map(|p| match p {
        Paused::UntilResumed => "until_resumed".to_owned(),
        Paused::Until(dto) => {
            let dt = Offset::UTC.to_datetime(dto.timestamp());
            let ms = dt.subsec_nanosecond().max(0) / 1_000_000;
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                dt.year(),
                dt.month(),
                dt.day(),
                dt.hour(),
                dt.minute(),
                dt.second(),
                ms
            )
        }
    })
}

fn map_current_break(cb: &CurrentBreakInfo) -> OutCurrentBreak {
    OutCurrentBreak {
        kind: kind_str(cb.kind),
        exercise_id: cb.exercise_id.clone(),
        total_ms: to_ms(cb.total_duration),
        remaining_ms: to_ms(cb.remaining_duration),
        suggestion_exercise_ids: cb.suggestion_exercise_ids.clone(),
    }
}

fn map_event(ev: &EngineEvent) -> OutEvent {
    match ev {
        EngineEvent::PhaseChanged(snap) => OutEvent {
            event_type: "PhaseChanged",
            phase: Some(phase_str(snap.phase)),
            kind: None,
            outcome: None,
            exercise_id: None,
            total_ms: None,
            suggestion_exercise_ids: None,
            duration_ms: None,
        },
        EngineEvent::BreakStarted(info) => OutEvent {
            event_type: "BreakStarted",
            phase: None,
            kind: Some(kind_str(info.kind)),
            outcome: None,
            exercise_id: Some(info.exercise_id.clone()),
            total_ms: Some(to_ms(info.total_duration)),
            suggestion_exercise_ids: info.suggestion_exercise_ids.clone(),
            duration_ms: None,
        },
        EngineEvent::BreakEnded(info, outcome) => OutEvent {
            event_type: "BreakEnded",
            phase: None,
            kind: Some(kind_str(info.kind)),
            outcome: Some(outcome_str(*outcome)),
            exercise_id: Some(info.exercise_id.clone()),
            total_ms: Some(to_ms(info.total_duration)),
            suggestion_exercise_ids: info.suggestion_exercise_ids.clone(),
            duration_ms: None,
        },
        EngineEvent::LongBreakWarningStarted(cd) => OutEvent {
            event_type: "LongBreakWarningStarted",
            phase: None,
            kind: None,
            outcome: None,
            exercise_id: None,
            total_ms: None,
            suggestion_exercise_ids: None,
            duration_ms: Some(to_ms(*cd)),
        },
        EngineEvent::AbsenceCredited(kind) => OutEvent {
            event_type: "AbsenceCredited",
            phase: None,
            kind: Some(kind_str(*kind)),
            outcome: None,
            exercise_id: None,
            total_ms: None,
            suggestion_exercise_ids: None,
            duration_ms: None,
        },
    }
}

fn run(scenario_path: &str) -> Result<(), String> {
    let raw = fs::read_to_string(scenario_path)
        .map_err(|e| format!("failed to read {scenario_path}: {e}"))?;
    let scenario: Scenario =
        serde_json::from_str(&raw).map_err(|e| format!("invalid scenario JSON: {e}"))?;

    let initial_ts: Timestamp = scenario
        .initial_utc
        .parse()
        .map_err(|e| format!("invalid initialUtc: {e}"))?;
    let tz = TimeZone::get(&scenario.time_zone)
        .map_err(|e| format!("invalid timeZone {}: {e}", scenario.time_zone))?;
    let initial_zoned = initial_ts.to_zoned_datetime(tz);

    let clock = Arc::new(FakeClock::new(initial_zoned));
    let platform = FakePlatformServices::new(
        scenario.idle_supported.unwrap_or(true),
        scenario.focus_supported.unwrap_or(true),
    );
    let mut settings = scenario.settings.unwrap_or_default();
    // The C# engine has no natural moments; force them off so both harnesses behave
    // identically (the C# reader ignores the unknown property in scenario JSON).
    settings.natural_moments = false;
    let picker = Box::new(DefaultExercisePicker::new(ZeroRng));

    let mut engine = BreakEngine::new(
        settings,
        platform.clone(),
        clock.clone(),
        None,
        Some(picker),
        scenario.initial_state,
        None,
    );

    for (step_idx, step) in scenario.steps.into_iter().enumerate() {
        if step.advance_ms > 0 {
            clock.advance(Duration::from_millis(step.advance_ms));
        }
        platform.idle.set_idle_ms(step.idle_ms);
        platform.focus.set_state(parse_focus(&step.focus)?);

        let (action_result, events) = match step.action {
            None | Some(StepAction::None) => (None, Vec::new()),
            Some(StepAction::Tick) => (None, engine.tick()),
            Some(StepAction::StartLongNow) => {
                let (ok, ev) = engine.start_long_break_now();
                (Some(ok), ev)
            }
            Some(StepAction::StartShortNow) => {
                let (ok, ev) = engine.start_short_break_now();
                (Some(ok), ev)
            }
            Some(StepAction::Postpone) => {
                let (ok, ev) = engine.postpone_long_break();
                (Some(ok), ev)
            }
            Some(StepAction::Skip) => {
                let (ok, ev) = engine.skip_current_break();
                (Some(ok), ev)
            }
            Some(StepAction::Pause { minutes }) => {
                let dur = minutes.map(|m| Duration::from_secs(m.saturating_mul(60)));
                (None, engine.pause_for(dur))
            }
            Some(StepAction::Resume) => (None, engine.resume()),
            Some(StepAction::Session { kind }) => {
                let ev_kind = parse_session_event(&kind)?;
                (None, engine.on_session_event(ev_kind))
            }
            Some(StepAction::UpdateSettings { settings }) => {
                let mut settings = settings;
                settings.natural_moments = false;
                (None, engine.update_settings(settings))
            }
        };

        let local_date = clock.now().date();
        let day_stats = engine
            .statistics()
            .get(local_date)
            .cloned()
            .unwrap_or_else(|| DailyStatistics::new(local_date));

        let snap = engine.snapshot();
        let line = OutStepLine {
            step: step_idx,
            phase: phase_str(engine.phase()),
            since_short_ms: to_ms(engine.since_short_break()),
            since_long_ms: to_ms(engine.since_long_break()),
            postpones_used: engine.postpones_used(),
            paused_until: format_paused(engine.paused_until()),
            current_break: engine.current_break().map(map_current_break),
            action_result,
            events: events.iter().map(map_event).collect(),
            today_stats: OutTodayStats {
                date: format!(
                    "{:04}-{:02}-{:02}",
                    local_date.year(),
                    local_date.month(),
                    local_date.day()
                ),
                short_completed: day_stats.short_completed,
                short_skipped: day_stats.short_skipped,
                long_completed: day_stats.long_completed,
                long_skipped: day_stats.long_skipped,
                long_postponed: day_stats.long_postponed,
                absences_credited: day_stats.absences_credited,
                active_minutes: day_stats.active_minutes,
            },
            snapshot: OutSnapshot {
                phase: phase_str(snap.phase),
                time_until_short_break_ms: snap.time_until_short_break.map(to_ms),
                time_until_long_break_ms: snap.time_until_long_break.map(to_ms),
                current_break: snap.current_break.as_ref().map(map_current_break),
                paused_until: format_paused(snap.paused_until_utc),
                postpones_left: snap.postpones_left,
                strict_mode: snap.strict_mode,
            },
        };

        let serialized = serde_json::to_string(&line)
            .map_err(|e| format!("failed to serialize step line: {e}"))?;
        println!("{serialized}");
    }

    let exported = engine.export_state();
    let exported_val = serde_json::to_value(&exported)
        .map_err(|e| format!("failed to serialize exported state: {e}"))?;
    let final_line = OutFinalLine {
        exported_state: exported_val,
    };
    let serialized = serde_json::to_string(&final_line)
        .map_err(|e| format!("failed to serialize final line: {e}"))?;
    println!("{serialized}");

    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: diff_harness <scenario.json>");
        return ExitCode::from(2);
    }

    match run(&args[1]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("diff_harness error: {err}");
            ExitCode::FAILURE
        }
    }
}
