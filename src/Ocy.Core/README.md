# Ocy.Core

`Ocy.Core` is the headless, cross-platform eye break engine for Ocy. It manages screen time tracking,
break schedules, inactivity/away detection, exercise selection, statistics, and local persistence without any
UI or OS-specific dependencies.

## Key Types

### 1. `BreakEngine`
The central state machine and coordinator.
- **Phase**: current `EnginePhase` (`Working`, `LongBreakWarning`, `ShortBreak`, `LongBreak`, `Deferred`, `Paused`, `Away`).
- **Snapshot**: immutable `EngineSnapshot` containing phase, remaining time until breaks, current break details, pause info, and strict mode state.
- **Settings**: active `OcySettings`.
- **Statistics**: in-memory `StatisticsData` (daily buckets).

### 2. Commands
All engine operations run on a single UI thread (no internal timers or threads):
- `Tick()`: Advances active screen time, countdowns, and checks due rules. Host calls this approximately once per second.
- `StartLongBreakNow()`: Immediately transitions from `Working`, `LongBreakWarning`, or `Deferred` into `LongBreak`.
- `StartShortBreakNow()`: Immediately starts a short break if in `Working` or `Deferred` (returns `false` and changes nothing in any other phase).
- `PostponeLongBreak()`: In `LongBreakWarning`, postpones the long break by `postponeMinutes` (returns `false` in strict mode or when `maxPostpones` reached).
- `SkipCurrentBreak()`: Skips the active break or warning. Refused for long breaks/warnings in strict mode.
- `PauseFor(TimeSpan? duration)`: Pauses screen time tracking until duration elapses or until `Resume()` is called.
- `Resume()`: Resumes from a pause, resetting active screen time counters.
- `OnSessionEvent(SessionEventKind)`: Handles OS lock/unlock and suspend/resume events.
- `UpdateSettings(OcySettings)`: Applies updated settings immediately without resetting running counters.
- `Save()`: Persists current engine state and statistics via `AppDataStore`.

### 3. Events
- `PhaseChanged(EngineSnapshot snapshot)`: Raised whenever the engine phase changes.
- `BreakStarted(BreakInfo info)`: Raised when a short or long break begins.
- `BreakEnded(BreakInfo info, BreakOutcome outcome)`: Raised when a break completes, is skipped, or is interrupted (e.g. by `PauseFor` during an active break). Outcomes: `Completed`, `Skipped`, `Interrupted`.
- `LongBreakWarningStarted(TimeSpan countdown)`: Raised when the long break warning countdown begins.
- `AbsenceCredited(BreakKind kind)`: Raised when a return from away credits a break without user intervention.

### 4. Settings (`OcySettings`)
- `ShortBreaks`: `Enabled`, `IntervalMinutes` (2–60), `DurationSeconds` (5–120), `Fullscreen`. Note: `shortBreaks.fullscreen` is a host display setting (dimmed full screen vs. small corner window) and is not read by the core engine.
- `LongBreaks`: `Enabled`, `IntervalMinutes` (15–240), `DurationMinutes` (1–30), `WarningSeconds` (0–300), `PostponeMinutes` (1–30), `MaxPostpones` (0–10).
- `StrictMode`: When enabled, skips and postpones of long breaks are rejected.
- `InactivityTracking`: Away tracking threshold (`AwayThresholdSeconds`, 30–1800).
- `PauseDuringFullscreen`: When true, fullscreen apps defer breaks to `Deferred`.
- `HasWindowNearby`: Filters out exercises requiring a window when false.

### 5. Exercises (`ExerciseCatalogue`, `ExercisePicker`)
- Pre-populated catalogue with 12 exercises (9 Eyes, 3 Body).
- `IExercisePicker` provides deterministic, randomized selection avoiding immediate repetitions and respecting window proximity.

### 6. Persistence (`AppDataStore`)
- Stores `settings.json`, `state.json`, and `stats.json` in the specified directory.
- Atomic writes (`.tmp` -> overwrite move).
- Automatic corrupt file backup (`*.bak-<timestamp>`) and default fallback.
- Retains up to 400 days of statistics.

### 7. Engine Rules (Rules 5–8)
Active screen time is accumulated only in `Working`, and `Deferred` is entered exclusively when a break becomes due while a fullscreen application or presentation is active; returning from away, a sleep gap, or a restart always lands in `Working` (rule 5). An active pause survives restarts, sleep gaps, and session events until resumed or expired (rule 6). Session lock during a break allows the break countdown to continue to completion (rule 7). A tick gap while already `Away` extends the existing away span from the original `awayStartedUtc` and credits at most one absence upon return (rule 8).

---

## Wiring `Tick()` in a Host Application

In Avalonia desktop or any UI host:

```csharp
using Avalonia.Threading;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Core.Settings;

// 1. Storage & Settings
var dataDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Ocy");
var store = new AppDataStore(dataDir);
var settings = store.LoadSettings();

// 2. Platform Services (e.g. Windows platform services or fallback)
IPlatformServices platformServices = OperatingSystem.IsWindows()
    ? new WindowsPlatformServices()
    : UnsupportedPlatformServices.Instance;

// 3. Create Engine
var engine = new BreakEngine(
    settings: settings,
    platformServices: platformServices,
    timeProvider: TimeProvider.System,
    dataStore: store);

// 4. Subscribe to Events
engine.PhaseChanged += snapshot =>
{
    // Update tray icon, tooltip, or window state
};

engine.BreakStarted += info =>
{
    // Show short break overlay or full long break overlay
};

engine.BreakEnded += (info, outcome) =>
{
    // Close overlays
};

engine.LongBreakWarningStarted += countdown =>
{
    // Show non-activating warning window
};

// 5. Wire Session Events (marshal to UI thread if needed)
platformServices.Session.SessionChanged += (s, ev) =>
{
    Dispatcher.UIThread.Post(() => engine.OnSessionEvent(ev));
};
platformServices.Session.Start();

// 6. Host 1-second UI Timer
var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
timer.Tick += (s, e) => engine.Tick();
timer.Start();
```
