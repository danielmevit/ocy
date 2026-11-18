# Changelog

## 0.1.1 — 2026-11-17 (alpha preview)

### Changed
- **Breaks appear on the monitor you're working on**: the one with the active window, or the mouse if no window is
  active. Other monitors get a subtle, click-through accent frame while a short break or warning is shown.
  The long-break card also moves to the active monitor.
- **Long-break card redesigned:**
  - a calm numeric countdown
  - clear title and text
  - two exercise suggestions side by side
  - a progress line, and "End break"
- **Settings redesigned:**
  - compact rows, with the on/off switches in the section headers
  - rows of a switched-off section are greyed out, not hidden
  - "Try a break" links inside their sections
  - changes apply instantly (no Save button)
  - a window icon
- The About section shows only the name, version and tagline.

### Fixed
- Break dims and cards were drawn without real transparency, which would have made them black on Windows.
- Buttons turned grey on hover.
- Alt+F4 could close a break window, including in strict mode.
- The full-screen short break now uses the active monitor.
- Sizing on mixed-DPI monitors.
- The tray icon now has readable variants for a light taskbar.
- Smaller polish: the short-break close button, its progress track, keyboard access to the long break, and the
  time units now come from the strings file.

### Verified
- 215 automated tests (Core 67, Windows 45, Desktop 103).
- On the owner's Windows 11 PC (3 monitors), spikes proved:
  - the OS minimum window size makes thin strip windows impossible, so a full-screen frame is used instead
  - the transparency setting is required
  - click-through rendering works

## 0.1.0-dev.1 — 2026-11-17 (first Windows preview, not released)

The first runnable Windows build: a tray app with short and long breaks. Built through Unio. Every change had an
independent review from a different AI lab and a lead assessment.

### Added
- **Break engine (`Ocy.Core`)**
  - counts active screen time (not wall-clock time): short breaks, long breaks with a warning, postpone, skip,
    strict mode, and pause for 1 h / 3 h / until resumed
  - away detection: idle, lock, sleep and screen saver count as rest, and a long enough absence counts as the long
    break
  - fullscreen and presentation deferral
  - pauses survive restarts, sleep and lock; a lock never ends or skips a running break (strict-mode safe)
  - settings, an exercise catalogue (12 original exercises), daily statistics, and atomic JSON persistence with
    corrupt-file backup
  - a lock/unlock matrix test across every phase
- **Windows services (`Ocy.Platform.Windows`):** idle time (`GetLastInputInfo`), fullscreen and presentation state
  (`SHQueryUserNotificationState`), start with Windows (HKCU Run key), and lock/sleep events (`SystemEvents`).
- **Desktop app (`Ocy.Desktop`, Avalonia 12)**
  - tray icon and menu, plus a short-break popup that never steals focus
  - a long-break warning with Start now / Postpone / Skip
  - a dimmed long-break overlay on **every monitor** (mixed DPI, negative coordinates)
  - a settings window with "Try a break"
  - light/dark theme from the system, with design tokens tested against the theme resources
  - single instance (a second launch opens Settings)
  - `--smoke` and `--screens` self-checks, and a capped `ocy-errors.log`
- **Content:** original English UI copy (110 strings).
- **Docs:** product research, the stack decision, architecture with engine rules 1–8, the design system, and the AI
  workflow (Fable/Opus lead plus Unio workers).

### Verified
- 169 automated tests (Core 67, Windows 35, Desktop 67).
- Windows 11 on the owner's PC:
  - `--smoke` (real idle, focus and autostart)
  - `--screens` (3 monitors, including negative coordinates)
  - a normal and a rapid double launch
  - the autosave location

### Fixed after the preview build
- A Desktop test race (it failed once in about 20 runs): the static error-log directory was removed and Desktop tests
  now run serially (OC04e). The full suite passed 5× in a row in verify and 3× on dev.

### Known issues
- The untrimmed self-contained build is about 205 MB, and the app uses about 70 MB of RAM while idle. Trimming/AOT
  is planned for Phase 2.
- Sounds, status notifications, languages other than English, the statistics view and call detection are not built
  yet (the settings are hidden).
