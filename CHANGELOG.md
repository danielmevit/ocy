# Changelog

## 0.2.0 — 2026-11-20 (alpha, the Rust rebuild)

Ocy is rebuilt from the ground up in Rust. It does everything 0.1.2 did, with a much lighter footprint, and a few
things it couldn't. Your settings and stats carry over automatically (same files in `%APPDATA%\Ocy`).

### Changed
- **One small file:**
  - The download is a single `Ocy.exe` of about 12 MB (plus licence files) instead of a ~70 MB zip with 230 files.
  - Unzip and run; there's nothing to install.
- **Much lighter on your PC:**
  - about 3.7 MB of memory while it waits (0.1.x: about 130 MB)
  - about 6 checks a minute while you work (0.1.x: every second)
- **Breaks appear instantly:** the next card is prepared a few seconds ahead and painted in about 10–20 ms.
- **Breaks never take focus or add a taskbar button:** typing in another app is never interrupted by a window
  grabbing the keyboard.

### Added
- **Natural-moment timing:** when a break is due, Ocy waits up to a minute for a short pause in your typing or mouse
  use, so it doesn't interrupt mid-sentence.
- **"Take a short break now"** in the tray menu, next to "Take a long break now".
- **Left-click the tray icon to open Settings.** Right-click still opens the menu.
- **The pause menu shows what you chose:** a checkmark on the active option, and a line such as "Paused until
  15:40 · 1 h 15 min left".
- **Screen-reader support** (Windows UI Automation / Narrator) for Settings and the break cards.
- **"Try a short break" and "Try a long break"** in Settings now look like links.
- **The window icon follows your taskbar theme:** the sprout with the bright outline on dark taskbars.

### Fixed
- Buttons and switches react to the first click.

### For developers
- **The app** is a Rust workspace (`rust/`) with a reusable kit (`damt-*` crates) and the Ocy crates (`ocy-core`,
  `ocy-ui`, `ocy-app`).
- **Engine parity with 0.1.x:** every C# engine test was ported, plus a differential harness against the C# engine.
- **Release tooling:** a deterministic zip, `THIRD-PARTY-NOTICES.txt` for all bundled crates
  (`docs/rust/LICENSES.md`), and a Windows release CI job with a 15 MB size budget.
- **Hidden probes for testing:** `--demo-break`, `--demo-in`, `--demo-settings`, `--report-memory` and `--soak`,
  isolated from a running Ocy.
- The C# 0.1.x source stays in the repository for reference.

## 0.1.2 — 2026-11-18 (alpha, first public release)

### Added
- **Hand-drawn exercise illustrations:** all 12 exercises now show Ocy, the sprout-headed mascot. They animate in a few
  stepped frames, like a flip book, on the short-break card and as suggestions on the long-break card. If Windows
  "Animation effects" is turned off, they hold still.
- **The long-break warning uses a warm clay colour.** Clay means "something needs your attention": the warning title,
  the frames on your other monitors during the warning, the tray icon during the warning, and a "Strict mode is on"
  note when postponing and skipping are off. Everything else stays calm green.
- **New app and tray icons:**
  - The app icon is Ocy's face at larger sizes and the sprout at small sizes (16–32 px), where a face isn't legible.
  - The tray shows only the sprout, in four states: running, paused (a drooping sprout), on a break, and the clay
    warning.
  - There are separate sets for light and dark taskbars, chosen from your Windows taskbar setting, not the app
    theme.
- **A website** at https://danielmevit.github.io/ocy/ and a public repository (AGPL-3.0).

### For developers
- `Ocy.exe --render-screens=<folder>` renders the real cards and settings window to PNG in light and dark (used for the
  README and the website), with pixel-level tests.
- `tools/illustrations/svg2axaml.py` and `tools/icons/pack_tray_ico.py` turn the art into app resources, with
  `--check` modes.

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
