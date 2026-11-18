# Gotchas

## Environment
- **WSL has no .NET SDK by default.** Install the .NET 10 SDK user-locally (no sudo):
  `curl -sSL https://dot.net/v1/dotnet-install.sh | bash -s -- --channel 10.0 --install-dir ~/.dotnet`.
  Workers run with `PATH=$HOME/.dotnet:$PATH DOTNET_ROOT=$HOME/.dotnet` (set in `coord/agents.conf`).
- The Windows side has only the .NET 8 SDK (`dotnet.exe`, end of support 2026-11-10). Install the .NET 10 SDK
  on Windows (`winget install Microsoft.DotNet.SDK.10`) before building on Windows itself. A self-contained
  `win-x64` publish from WSL runs on Windows without any SDK.
- **Never run Windows programs (`dotnet.exe`, `cmd.exe`) from a Gemini/agy worker.** Its command tool
  backgrounds them after ~10 s with an empty log and then idles until timeout. Workers build with Linux
  `dotnet` only. The lead runs Windows smoke checks.
- The repo lives on `/mnt/d` (9P). Builds and restores are slower than on ext4. Pass `-nodeReuse:false` and
  `-p:UseSharedCompilation=false` in agent builds so no MSBuild node keeps output pipes open.
- `git config core.filemode false` is set (WSL on an NTFS drive). `.gitattributes` forces LF.

## Avalonia 12
- This is Avalonia **12.x**, not 11. Check APIs against 12 (compiled bindings by default, DevTools replaces
  Diagnostics). Don't paste 11-era samples blindly; let the compiler confirm.
- `Window.SystemDecorations` is **obsolete** in 12 (warnings are errors here). Use
  `WindowDecorations = WindowDecorations.None`. (Lead spike, 2026-11-17)
- Calling `desktop.Shutdown()` synchronously during startup, before the main loop runs, throws "Dispatcher shut
  down". Post it: `Dispatcher.UIThread.Post(() => desktop.Shutdown(0))`.
- A self-contained `win-x64` publish from WSL works: the exe runs on Windows and `Screens` sees every monitor. The
  owner's PC has **3 monitors** (1920x1080 + 2× 1920x1200, scale 1). The untrimmed publish is ~205 MB.
- `Screen.Bounds` is in **physical pixels**. Position per-monitor windows with `PixelPoint` and size them as
  `Bounds / Scaling` (or position first, then go FullScreen). The naive version breaks on mixed-DPI setups.
- TrayIcon is declared in `App.axaml` (`TrayIcon.Icons`). On Linux it needs a StatusNotifierItem host.
- `Window.ShowActivated = false` + `Topmost = true` for windows that must not steal focus.

## Unio coordination (lead)
- **Don't move `dev` while another branch's review is in flight.** The review is bound to the base and comes back
  stale (`candidate_changed_during_review`).
- The Vibe adapter prints a JSON summary on stdout, not the reviewer's text, so `unio review ... vibeglm` records
  `unknown_verdict`. The review text is in `coord/proposals/vibe/<run>/native-stdout.log`. Use agy/opencode for
  native reviews.
- Never edit a task file after dispatch (Unio treats it as tampering).
- **Avalonia build telemetry** (`Avalonia.BuildServices`) keeps a `dotnet` process alive in the build directory for
  about 10 s after every build. Unio then sees the worker as busy and refuses a review started straight after
  `unio verify`. Set `AVALONIA_TELEMETRY_OPTOUT=1` (done in `coord/agents.conf`; also export it in lead shells).
- Run Windows checks of a worker's build from a **copy outside `wt/`**, never from the worker's publish folder.
- `timeout` on the WSL side does not stop a Windows GUI exe. Stop it with `taskkill.exe /PID <pid> /F`.
- Stacked tasks on one branch: merge an accepted earlier task before verifying the next one, otherwise the next
  verify reports the earlier task's allowed files as a scope violation.

## Windows platform
- `GetLastInputInfo().dwTime` and `Environment.TickCount` are 32-bit tick counts. Compute idle with unsigned
  wraparound: `unchecked((uint)Environment.TickCount - lii.dwTime)`.
- `SHQueryUserNotificationState` returns `QUNS_BUSY` for many fullscreen windows (browser video, games).
  It's a heuristic.
- Autostart via the HKCU `Run` key is for the **unpackaged** exe. An MSIX/Store build must use the
  `windows.startupTask` extension instead (Phase 2).
