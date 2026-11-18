# Ocy architecture

Owner of this document: the lead. Workers implement against it. **Do not change a frozen contract
(marked FROZEN) in a worker task.** If it blocks you, say so in your report and stop that part.

## 1. Solution layout

```
repo/
  global.json                     .NET SDK 10.0.x
  Directory.Build.props           shared: net10.0, nullable, warnings-as-errors, metadata
  Directory.Packages.props        central package versions (add new packages HERE only)
  src/
    Ocy.Abstractions/         FROZEN platform contracts (interfaces + enums only)
    Ocy.Core/                 engine, settings, exercises, statistics, persistence (no UI/OS deps)
    Ocy.Platform.Windows/     Windows implementations of the contracts (P/Invoke)
    Ocy.Desktop/              Avalonia app: tray, overlays, settings window, composition root
  tests/
    Ocy.Core.Tests/           xUnit, FakeTimeProvider
    Ocy.Platform.Windows.Tests/  xUnit for the pure helpers (run on Linux)
    Ocy.Desktop.Tests/        Avalonia.Headless.XUnit (Phase 1 wave 2)
```

Rules:
- `Ocy.Core` references only `Ocy.Abstractions` and BCL packages. It has no Avalonia,
  P/Invoke or file-path guessing. Paths are passed in.
- Every project targets plain `net10.0`. Windows-only code is marked
  `[SupportedOSPlatform("windows")]` and called only behind `OperatingSystem.IsWindows()`, so the whole
  solution builds and tests on Linux/WSL.
- Use `LibraryImport` (source-generated P/Invoke), not `DllImport`.
- Package versions live only in `Directory.Packages.props`.

## 2. Platform contracts — FROZEN (`src/Ocy.Abstractions`)

See the source files. Summary:

| Interface | Purpose | Windows implementation (Phase 1) |
| --- | --- | --- |
| `IUserIdleProvider` | time since the last keyboard/mouse input | `user32!GetLastInputInfo` vs `Environment.TickCount` (uint wraparound-safe) |
| `IFocusStateProvider` | is a fullscreen app / presentation / quiet time active? | `shell32!SHQueryUserNotificationState` |
| `IAutostartService` | start with the OS | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `Ocy` = `"<exe>" --autostart` |
| `ISessionEventSource` | lock / unlock / suspend / resume | `Microsoft.Win32.SystemEvents.SessionSwitch` + `PowerModeChanged` |
| `IPlatformServices` | bundles the four above | `WindowsPlatformServices` |

`SHQueryUserNotificationState` mapping (QUERY_USER_NOTIFICATION_STATE → `FocusState`):

| Value | Name | FocusState |
| --- | --- | --- |
| 1 | QUNS_NOT_PRESENT (screen saver / locked / fast user switching) | `NotPresent` |
| 2 | QUNS_BUSY (fullscreen app) | `FullscreenApp` |
| 3 | QUNS_RUNNING_D3D_FULL_SCREEN | `FullscreenApp` |
| 4 | QUNS_PRESENTATION_MODE | `Presentation` |
| 5 | QUNS_ACCEPTS_NOTIFICATIONS | `Normal` |
| 6 | QUNS_QUIET_TIME | `QuietTime` |
| 7 | QUNS_APP (fullscreen Store app) | `FullscreenApp` |
| HRESULT failure / other | | `Unknown` |

## 3. Core engine (`src/Ocy.Core`)

### 3.1 Settings (`OcySettings`, JSON, camelCase, `schemaVersion: 1`)

| Key | Default | Allowed |
| --- | --- | --- |
| `shortBreaks.enabled` | true | |
| `shortBreaks.intervalMinutes` | 10 | 2–60 |
| `shortBreaks.durationSeconds` | 15 | 5–120 |
| `shortBreaks.fullscreen` | false | |
| `longBreaks.enabled` | true | |
| `longBreaks.intervalMinutes` | 60 | 15–240 |
| `longBreaks.durationMinutes` | 5 | 1–30 |
| `longBreaks.warningSeconds` | 60 | 0–300 (0 = no warning) |
| `longBreaks.postponeMinutes` | 3 | 1–30 |
| `longBreaks.maxPostpones` | 3 | 0–10 |
| `strictMode` | false | |
| `sounds` | false | (sounds are opt-in) |
| `inactivityTracking` | true | |
| `awayThresholdSeconds` | 120 | 30–1800 |
| `pauseDuringFullscreen` | true | |
| `hasWindowNearby` | true | |
| `statusNotifications` | true | |
| `startWithSystem` | false | |
| `language` | "en" | BCP-47 tag |

Out-of-range values are clamped on load. Unknown keys are ignored. A missing file means defaults.
A corrupt file is renamed to `settings.json.bak-<utc-timestamp>` and defaults are used.
Writes are atomic: write `*.tmp`, then `File.Move(overwrite: true)`.

### 3.2 Engine model

The engine counts **active screen time**, not wall-clock time. The host calls `Tick()` about once per
second. The engine reads time only from the injected `TimeProvider`.

State (persisted): `sinceShortBreak`, `sinceLongBreak` (active time since each break), `postponesUsed`,
`pausedUntilUtc` (null = not paused; `DateTimeOffset.MaxValue` = until resumed), `lastTickUtc`.

Phases (`EnginePhase`): `Working`, `LongBreakWarning`, `ShortBreak`, `LongBreak`, `Deferred`, `Paused`, `Away`.

Per `Tick()` (with `now = time.GetUtcNow()`, `dt = now - lastTickUtc`):
1. **Gap = sleep.** If `dt > 15 s`, treat the whole `dt` as away time (rule 4 below) and do not add it to the counters.
2. **Paused.** If paused and `now < pausedUntilUtc`, do nothing. When the pause ends, reset both counters and
   `postponesUsed`, then go to `Working`.
3. **Inside a break.** Count down the remaining break time. At zero the break completes (stats), the counters
   reset (a long break resets both, a short break resets the short one) and the phase goes to `Working`.
4. **Away.** If `inactivityTracking` and idle ≥ `awayThresholdSeconds`, enter `Away`. The counters do not grow
   while away. On return (idle < threshold), with `away` = the idle span measured from the last input:
   - `away ≥ longBreaks.duration`: credit a long break. Reset both counters, `stats.absencesCredited++`,
     raise `AbsenceCredited(Long)`.
   - `away ≥ shortBreaks.duration`: reset the short counter only.
   A session lock (`ISessionEventSource` Locked → Unlocked) and `FocusState.NotPresent` count as away for
   their whole span.
5. **Working.** Add `dt` to both counters, then check due times:
   - If `sinceLongBreak ≥ interval − warning` and the warning is not yet shown → `LongBreakWarning` (a countdown
     of `warningSeconds`). When the countdown reaches zero → `LongBreak`.
   - If `sinceShortBreak ≥ shortInterval` → `ShortBreak`, **unless** a long break is due within 2 minutes or a
     warning is showing. In that case reset the short counter silently.
   - **Fullscreen deferral:** if `pauseDuringFullscreen` and the focus state is `FullscreenApp` or `Presentation` when
     a break becomes due, go to `Deferred`. Short breaks are dropped (the short counter resets). A long break
     waits until the focus state is normal, then starts its warning.
   - A disabled break type never fires.
   - `Deferred` is entered **only** when a break becomes due during fullscreen. Returning from Away, a sleep gap or
     a restart lands in `Working`. It stays `Paused` if a pause is active, and re-enters `Away` (a fresh span) if the
     user is still idle or not present. It never lands in `Deferred` or a warning. The Working rule defers if a break
     then becomes due.
     Counters keep growing in a fullscreen app while nothing is due.
6. **Paused survives everything.** A restart, sleep gap or session event never ends an active pause. Only
   `Resume()` or `now ≥ pausedUntilUtc` ends it. A pause that expired during downtime ends with the normal pause-end
   reset.
7. **Session lock during a break.** The break keeps counting down and completes (the user is away, which is the
   point of the break). Any lock time after completion is away time. **Unlocking or resuming while the break is
   still running changes nothing:** the break continues to its normal end. A lock/unlock must never end, skip or
   discard a break. This matters especially in strict mode.
8. **A gap while already Away** extends the same away span (measured from the original `awayStartedUtc`). It is
   never credited twice.

`shortBreaks.fullscreen` is a **host display setting**: dimmed full screen vs the small corner window. The engine
does not read it.

Commands (all thread-safe for one UI thread; no internal timers):

| Command | Behaviour |
| --- | --- |
| `Tick()` | as above |
| `StartLongBreakNow()` | from Working/Warning/Deferred → `LongBreak` |
| `StartShortBreakNow()` | "try short break" → `ShortBreak` |
| `PostponeLongBreak()` | in Warning, unless strict or `postponesUsed ≥ maxPostpones`: due again in `postponeMinutes`, `postponesUsed++`; returns bool |
| `SkipCurrentBreak()` | short: allowed. Long or warning: refused in strict mode. Resets the matching counters, stats `*Skipped++`; returns bool |
| `PauseFor(TimeSpan? duration)` | null = until `Resume()` |
| `Resume()` | ends a pause (counters reset as in rule 2) |
| `OnSessionEvent(SessionEventKind)` | Locked/Suspending → away begins; Unlocked/Resumed → away ends (rule 4) |
| `UpdateSettings(OcySettings)` | applies immediately. Running counters are kept; a running break keeps its duration |

Events: `PhaseChanged(EngineSnapshot)`, `BreakStarted(BreakInfo)`, `BreakEnded(BreakInfo, BreakOutcome)`,
`LongBreakWarningStarted(TimeSpan countdown)`, `AbsenceCredited(BreakKind)`.

`EngineSnapshot`: Phase, TimeUntilShortBreak, TimeUntilLongBreak (null when disabled), CurrentBreak
(kind, exercise id, total, remaining), PausedUntilUtc, PostponesLeft, StrictMode.

`ScheduleCalculator` (pure, static): given settings + counters → the next short and long due times. The
engine and the future mobile notification scheduler use it.

### 3.3 Exercises

A catalogue in code: id, `ExerciseKind` (Eyes/Body), `RequiresWindow`, suggested duration and localisation keys
`exercise.<id>.title` / `exercise.<id>.body`. The picker is random with an injectable `Random`. It never repeats
the previous id and skips `RequiresWindow` exercises when `hasWindowNearby` is false.
Short breaks pick Eyes exercises. Long breaks pick one Body plus one Eyes suggestion.

Initial ids: `blink`, `close-tight`, `roll`, `look-up-down`, `look-left-right`, `near-far`, `window-far`
(RequiresWindow), `palming`, `figure-eight`, `neck-shoulders` (Body), `stand-walk` (Body), `stretch-arms` (Body).

### 3.4 Statistics

Daily buckets keyed by **local** date (`TimeProvider.LocalTimeZone`): shortCompleted, shortSkipped,
longCompleted, longSkipped, longPostponed, absencesCredited, activeMinutes. JSON file, keep 400 days, atomic
writes.

### 3.5 Persistence

`AppDataStore(string directory)` owns `settings.json`, `state.json` and `stats.json`. The host passes the
directory:

- Windows: `%APPDATA%\Ocy`
- macOS: `~/Library/Application Support/Ocy`
- Linux: `$XDG_CONFIG_HOME/ocy`

On startup, the gap since `lastTickUtc` is applied as away time (rule 4), so a reboot does not reset the
counters. A long absence counts as a break (EyeLeo parity).

## 4. Desktop app (`src/Ocy.Desktop`) — Phase 1 wave 2

- Composition root: settings → engine → platform services (Windows when `OperatingSystem.IsWindows()`, else
  null/unsupported implementations) → a UI timer (1 s) calling `engine.Tick()` on the UI thread.
- Tray: tooltip with the status, a state icon (active / paused / break) and a menu as in product research §4.
- Short-break window: small, topmost, **never takes focus** (`ShowActivated=false`), bottom-right of the
  **active screen's** working area. The active screen comes from `IPlatformServices.ActiveScreen` (the foreground
  window's monitor, else the cursor's), falling back to primary. Optional fullscreen dim mode.
- **Edge indicator (0.1.1):** while a short break or warning is shown, every *other* screen gets a subtle accent
  frame. It is one transparent full-screen `EdgeFrameWindow` per screen with a 3 DIP border, made click-through via
  `IPlatformServices.WindowStyler`. It never activates and closes with the break. Every break window has an Alt+F4
  close guard; only `CloseAll()` closes them.
- Long-break overlay: one borderless topmost window per `Screens.All`, a dimmed backdrop, a centred card with a
  countdown and suggestions, and Skip (hidden in strict mode).
- Warning window: small, topmost, non-activating, on the active screen (same rule), with a countdown and Start now /
  Postpone / Skip.
- Settings window: the settings from §3.1 (Sounds, Status notifications and Language are hidden until Phase 2),
  live apply (400 ms debounce → `engine.UpdateSettings` + `store.SaveSettings`), section-header enable toggles, and
  "Try a short/long break" links. Strings come through the `{loc:Str key}` markup extension.
- Single instance: named mutex (Windows) / lock file. A second launch signals the first to open settings.
- Sounds: soft, low, short, non-urgent (see DESIGN_SYSTEM). Off by default.
- `--smoke` CLI flag: no UI. Prints a JSON line with version, OS, idle ms, focus state, autostart state and
  screen count, then exits 0. Used for verification on Windows.
