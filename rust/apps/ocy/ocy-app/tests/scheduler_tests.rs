//! Unit tests for the adaptive host scheduler (`ocy_app::scheduler`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_platform::{Clock, FocusState, FocusStateProvider, IdleProvider, PlatformServices};
use jiff::Timestamp;
use ocy_app::scheduler::{self, ACTIVE_WAKE_INTERVAL, MAX_WAKE_DELAY, MIN_WAKE_DELAY};
use ocy_core::engine::{BreakEngine, EnginePhase, EngineSnapshot, EngineState, NaturalHoldKind};
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

    // The warning can be the nearest due moment. It is more than the pre-warm lead
    // time away, so the host wakes early at due - PREWARM_LEAD_TIME to pre-warm the
    // warning card (anything beyond the cap is capped).
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(120)),
        Some(Duration::from_secs(165)),
        Some(Duration::from_secs(4)),
    );
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        Duration::from_secs(1)
    );

    // Disabled breaks contribute nothing.
    let snap = snapshot(EnginePhase::Working, None, None, None);
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MAX_WAKE_DELAY);

    // A due moment now clamps to the minimum (the tick resolves it immediately).
    let snap = snapshot(EnginePhase::Working, Some(Duration::ZERO), None, None);
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MIN_WAKE_DELAY);
}

#[test]
fn working_wakes_for_prewarm_three_seconds_before_a_due_moment() {
    // Due in 25 s: the 10 s cap applies while the pre-warm wake (due - 3) is farther
    // out than the cap...
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(25)),
        None,
        None,
    );
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MAX_WAKE_DELAY);

    // ...still capped at 15 s away (15 - 3 = 12 > 10)...
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(15)),
        None,
        None,
    );
    assert_eq!(scheduler::next_wake_delay(&snap, ts(0)), MAX_WAKE_DELAY);

    // ...then the pre-warm wake wins at 5 s away (5 - 3 = 2)...
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(5)),
        None,
        None,
    );
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        Duration::from_secs(2)
    );

    // ...and the tick at due - 3 pre-warms; the next wake is the due moment itself.
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(3)),
        None,
        None,
    );
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        Duration::from_secs(3)
    );

    // Due in 2 s (already within the lead time): wake at due, no extra wake.
    let snap = snapshot(
        EnginePhase::Working,
        Some(Duration::from_secs(2)),
        None,
        None,
    );
    assert_eq!(
        scheduler::next_wake_delay(&snap, ts(0)),
        Duration::from_secs(2)
    );

    // The long break and the warning get the same pre-warm wake.
    for (until_long, until_warning, expected) in [
        (Some(Duration::from_secs(5)), None, Duration::from_secs(2)),
        (None, Some(Duration::from_secs(5)), Duration::from_secs(2)),
        (Some(Duration::from_secs(2)), None, Duration::from_secs(2)),
        (None, Some(Duration::from_secs(2)), Duration::from_secs(2)),
    ] {
        let snap = snapshot(EnginePhase::Working, None, until_long, until_warning);
        assert_eq!(
            scheduler::next_wake_delay(&snap, ts(0)),
            expected,
            "until_long={until_long:?}, until_warning={until_warning:?}"
        );
    }
}

#[test]
fn a_due_moment_25s_away_takes_one_extra_wake_to_prewarm() {
    // Full adaptive sequence toward a short break due in 25 s: cap, cap, pre-warm
    // wake at due - 3, due — exactly one extra wake versus the pre-R5d schedule.
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 9, 0, 0).expect("valid date"));
    let platform = TestPlatform::new();
    platform.set_idle(Duration::ZERO);
    let mut settings = OcySettings::default();
    settings.long_breaks.enabled = false;
    settings.natural_moments = false;
    let mut engine = BreakEngine::new(
        settings,
        platform.clone(),
        clock.clone(),
        None,
        Some(Box::new(DefaultExercisePicker::new(SeededRng::new(1)))),
        // Short break due in 25 s (default interval is 10 min).
        Some(EngineState {
            since_short_break: Duration::from_secs(575),
            ..Default::default()
        }),
        None,
    );

    let mut wakeups = 0u64;
    let mut elapsed = Duration::ZERO;
    while engine.snapshot().phase == EnginePhase::Working {
        let delay = scheduler::next_wake_delay(&engine.snapshot(), clock.now().timestamp());
        clock.advance(delay);
        elapsed += delay;
        let _ = engine.tick();
        wakeups += 1;
        assert!(wakeups < 10, "the break never started");
    }
    assert_eq!(elapsed, Duration::from_secs(25));
    assert_eq!(wakeups, 4, "expected cap + cap + pre-warm + due wakes");
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

// --- Host-level pre-warm test -------------------------------------------------
//
// Runs the real AppHost (engine + coordinator + recording break windows) with adaptive
// scheduler ticks toward a short break due in 25 s, and asserts the coordinator's
// break-windows seam decides the pre-warm strictly before the break is shown.

use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use damt_platform::{TaskbarThemeProvider, Unsupported};
use ocy_app::break_logic::{PREWARM_LEAD_TIME, PrewarmKind, PrewarmPlanner};
use ocy_app::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
use ocy_app::host::AppHost;
use ocy_app::strings::Strings;
use ocy_app::tray::{
    AppTheme, TrayController, TrayIconState, TrayPresentation, TrayPresenter, TraySink,
};
use ocy_core::engine::BreakInfo;

/// Records pre-warm decisions and break shows in the break-windows seam.
#[derive(Debug, Default)]
struct PrewarmRecordingWindows {
    settings: OcySettings,
    /// (event, time_until_short_break at that tick).
    events: Vec<(&'static str, Option<Duration>)>,
}

impl BreakWindows for PrewarmRecordingWindows {
    fn sync_state(&mut self, _snapshot: &EngineSnapshot, settings: &OcySettings) {
        self.settings = settings.clone();
    }

    fn on_tick(&mut self, snapshot: &EngineSnapshot) {
        if PrewarmPlanner::decide(snapshot, &self.settings, None) == Some(PrewarmKind::ShortBreak)
            && !self.events.iter().any(|(name, _)| *name == "prewarm")
        {
            self.events
                .push(("prewarm", snapshot.time_until_short_break));
        }
    }

    fn show_short_break(&mut self, _info: &BreakInfo) {
        self.events.push(("show", Some(Duration::ZERO)));
    }

    fn show_warning(&mut self, _countdown: Duration) {}

    fn show_long_break(&mut self, _info: &BreakInfo) {}

    fn update_countdown(&mut self, _snapshot: &EngineSnapshot) {}

    fn close_all(&mut self) {}
}

#[derive(Debug, Default)]
struct NullSettingsWindow;

impl SettingsWindow for NullSettingsWindow {
    fn show(&mut self) {}
}

#[derive(Debug, Default)]
struct NullTraySink;

impl TraySink for NullTraySink {
    fn set_tooltip(&mut self, _tooltip: &str) {}
    fn set_icon(&mut self, _state: TrayIconState, _use_light_set: bool) {}
    fn set_menu(&mut self, _presentation: &TrayPresentation) {}
}

#[derive(Debug)]
struct StaticTaskbarTheme;

impl TaskbarThemeProvider for StaticTaskbarTheme {
    fn is_supported(&self) -> bool {
        true
    }
    fn is_light_taskbar(&self) -> Option<bool> {
        Some(true)
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-scheduler-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn adaptive_ticks_prewarm_the_short_break_card_before_showing_it() {
    let dir = temp_dir("prewarm-host");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 9, 0, 0).expect("valid date"));
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);

    let mut settings = OcySettings::default();
    settings.long_breaks.enabled = false;
    settings.natural_moments = false;
    let engine = BreakEngine::new(
        settings,
        platform,
        Arc::clone(&clock_dyn),
        None,
        None,
        // Short break due in 25 s (default interval is 10 min).
        Some(EngineState {
            since_short_break: Duration::from_secs(575),
            ..Default::default()
        }),
        None,
    );
    let snapshot = engine.snapshot();

    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme: Rc<dyn TaskbarThemeProvider> = Rc::new(StaticTaskbarTheme);
    let tray = TrayController::new(
        presenter,
        NullTraySink,
        theme,
        clock_dyn,
        Some(AppTheme::Light),
        &snapshot,
    );
    let coordinator =
        BreakUiCoordinator::new(PrewarmRecordingWindows::default(), NullSettingsWindow);
    let error_log = damt_app::ErrorLog::new(&dir, "ocy");
    let mut host = AppHost::new(engine, coordinator, tray, error_log);

    // Drive the host with the adaptive scheduler delays until the break shows.
    let mut wakeups = 0u64;
    let mut elapsed = Duration::ZERO;
    loop {
        let delay = scheduler::next_wake_delay(&host.engine().snapshot(), clock.now().timestamp());
        clock.advance(delay);
        elapsed += delay;
        host.on_tick();
        wakeups += 1;
        assert!(wakeups < 10, "the break never started");
        let events = &host.coordinator().break_windows().events;
        if events.iter().any(|(name, _)| *name == "show") {
            break;
        }
    }

    let events = &host.coordinator().break_windows().events;
    assert_eq!(
        events[..],
        [
            ("prewarm", Some(Duration::from_secs(3))),
            ("show", Some(Duration::ZERO))
        ],
        "the pre-warm must be decided before the show"
    );
    // The pre-warm was decided within the lead window, strictly before the due moment.
    let (_, until_short_at_prewarm) = events[0];
    assert!(
        until_short_at_prewarm.is_some_and(|t| { t > Duration::ZERO && t <= PREWARM_LEAD_TIME })
    );
    // Cap + cap + pre-warm + due: the four adaptive wakes toward a 25 s due moment,
    // with the pre-warm decided on the wake at due - PREWARM_LEAD_TIME.
    assert_eq!(elapsed, Duration::from_secs(25));
    assert_eq!(wakeups, 4);

    host.shutdown();
    let _ = fs::remove_dir_all(&dir);
}
