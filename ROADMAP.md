# Roadmap

## Phase 1 — Windows MVP (current)
- [x] Research foundation, ADR, naming, architecture, frozen platform contracts
- [x] Wave 1: `Ocy.Core` engine + settings + exercises + statistics (tests)
- [x] Wave 1: `Ocy.Platform.Windows` (idle, fullscreen, autostart, session events)
- [x] Wave 1: original exercise and message copy (en)
- [x] Wave 2: `Ocy.Desktop` (tray, short-break popup, warning, long-break overlay on every monitor, settings)
- [x] Windows smoke on the owner's PC (`--smoke`, `--screens`, double launch)
- [x] 0.1.1: active-monitor breaks + edge frame, long-break card and settings redesign, UI fixes (Opus review)
- [ ] Owner hands-on test of 0.1.1
- [ ] Owner decision: product differentiators (see START_HERE)

## Phase 2 — Windows polish and distribution
- Statistics view; call/meeting detection (mic/camera in use); themes; soft sounds
- Localisation: en, mk, cs, sk, sr, hr, bs, sl, bg, sq, …
- Packaging: MSIX (Microsoft Store, `startupTask`) + direct installer with updates; trimming/NativeAOT
- App icon and brand (owner)

## Phase 3 — macOS and Linux
- `Ocy.Platform.MacOS`, `Ocy.Platform.Linux` (X11 first)
- Wayland overlay/idle spike (`ext-idle-notify-v1`, per-output fullscreen)
- macOS signing/notarization (needs Apple Developer Program)

## Phase 4 — Mobile companion
- `Ocy.Mobile` (Avalonia Android/iOS) on `Ocy.Core`: scheduled notifications from `ScheduleCalculator`,
  exercise player, shared statistics format
