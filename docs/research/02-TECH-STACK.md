# Tech stack decision (ADR-001)

Decided by the lead (Claude Code, Opus 5.5) on 2026-11-17. Status: **accepted**. The owner may revisit it.

## Decision

**C# / .NET 10 (LTS) + Avalonia UI 12.1**, with a UI-free shared core library.

| Layer | Project | Depends on |
| --- | --- | --- |
| Platform contracts (frozen by the lead) | `src/Ocy.Abstractions` | nothing |
| Engine, settings, exercises, statistics | `src/Ocy.Core` | Abstractions |
| Windows services (idle, fullscreen, autostart, session) | `src/Ocy.Platform.Windows` | Abstractions |
| Desktop app (tray, overlays, settings UI) | `src/Ocy.Desktop` | Core, Platform.* , Avalonia |
| Later: `Ocy.Platform.MacOS`, `Ocy.Platform.Linux`, `Ocy.Mobile` (Android/iOS heads) | | Core |

## Why

1. **One language for desktop and mobile.** Avalonia targets Windows, macOS and Linux (X11, plus native Wayland since 12.1) and also Android and iOS. Avalonia 12 rebuilt the Android side, and 12.1 adds mobile haptics and sound. The core is plain .NET, so any future mobile head reuses it unchanged.
2. **It matches the owner's craft.** Their shipped desktop apps are C#/.NET (WPF), and so are their Microsoft Store/MSIX experience and build habits. XAML knowledge carries over.
3. **The agent workers can verify their work.** The .NET SDK runs natively in WSL/Linux, so core logic and headless UI tests build and run there. A Windows `win-x64` self-contained build can be published from Linux because Avalonia has no WPF dependency. The Windows-only P/Invoke code compiles on Linux and is guarded by `OperatingSystem.IsWindows()`. By contrast, a WPF project forces `dotnet.exe` from WSL, which hung Gemini workers in earlier rounds (see GOTCHAS).
4. **A light all-day tray app.** It is native rendering (Skia), not a browser engine. Trimming and NativeAOT are available later.
5. **Licence.** Avalonia is MIT. No paid add-ons are needed.
6. **.NET 10 is the current LTS** (supported until 2028-11-14). .NET 8 reached end of support on 2026-11-10 **(verified, dotnet release index)**.

## Alternatives considered

| Option | Desktop | Mobile | Why not chosen |
| --- | --- | --- | --- |
| **Tauri 2** (Rust + web UI) | Win/macOS/Linux, tray, multi-window | iOS/Android since 2.0 | Two languages (Rust + TS) plus IPC. WebView2/WebKitGTK rendering differs per OS. Building Windows from WSL needs cargo-xwin or MSVC. A strong runner-up. |
| **Flutter** | Win/macOS/Linux | best-in-class | Desktop multi-window, which we need for one overlay per monitor, is still immature. Tray and idle need third-party plugins or FFI. No SDK installed. |
| **Qt 6** (C++/QML) | excellent | via QML | Heavy toolchain (online installer + account on Windows). The C++ code that agents write is harder to verify. The existing port uses Widgets, not QML. |
| **Electron** | Win/macOS/Linux | none | Heavy for an all-day tray app. No mobile. |
| **.NET MAUI** | Win/macOS only (no official Linux) | iOS/Android | Fails the Linux requirement. |
| **Compose Multiplatform** | Win/macOS/Linux via JVM | iOS/Android | JVM memory footprint for a tray app. No JDK installed. |
| **WPF** (owner's usual) | Windows only | none | Fails cross-platform. |

## Capability matrix (what each OS needs from `Ocy.Abstractions`)

| Capability | Windows (Phase 1) | macOS (Phase 3) | Linux X11 (Phase 3) | Linux Wayland (Phase 3) | Android/iOS (Phase 4) |
| --- | --- | --- | --- | --- | --- |
| Tray / menu bar | Avalonia `TrayIcon` | Avalonia `TrayIcon` (NSStatusItem) | SNI via D-Bus (GNOME needs the AppIndicator extension) | same as X11 | n/a (notifications) |
| Overlay on every monitor | topmost borderless window per screen | NSWindow at screen-saver level | override-redirect or fullscreen per screen | compositor decides. Fullscreen per output works; forced always-on-top does not. **Research spike.** | impossible over other apps (Android needs SYSTEM_ALERT_WINDOW; iOS never). Use in-app plus notifications. |
| Idle time | `GetLastInputInfo` | `CGEventSourceSecondsSinceLastEventType` | XScreenSaver extension | `ext-idle-notify-v1`, or Mutter IdleMonitor D-Bus on GNOME. **Spike.** | n/a |
| Fullscreen app / presentation | `SHQueryUserNotificationState` | CGWindowList heuristics | `_NET_WM_STATE_FULLSCREEN` | generally not observable | n/a |
| Call / meeting (Phase 2) | ConsentStore registry (mic/camera in use) | `AVCaptureDevice` / mic-in-use heuristics | PipeWire / PulseAudio source-outputs | same | n/a |
| Autostart | HKCU `Run` key (unpackaged); `StartupTask` (MSIX) | `SMAppService` login item | XDG `~/.config/autostart/*.desktop` | same | n/a |
| Lock / sleep | `SystemEvents.SessionSwitch` / `PowerModeChanged`, plus a tick-gap fallback | distributed notifications | logind D-Bus | logind D-Bus | app lifecycle |
| Notifications | Avalonia window toasts (own UI) | own UI | own UI | own UI | local notifications (iOS caps pending notifications at 64) |

The core engine also detects sleep and suspend **without** OS events. If two consecutive ticks are far apart, the gap counts as away time. Every platform gets this for free.

## Mobile foundation (Phase 4 design hooks, built in now)

- `Ocy.Core` has no UI, OS or Avalonia dependency. It targets plain `net10.0`.
- Time is injected through `System.TimeProvider`. Tests use `FakeTimeProvider` from `Microsoft.Extensions.TimeProvider.Testing`.
- The schedule logic exposes "next short and long break due times" as a pure calculation. A mobile app uses that same calculation to schedule local notifications.
- Exercise content is data (ids + localisation keys + durations) shared by every head.

## Versions pinned on 2026-11-17 **(verified, NuGet + dotnet release index)**

- .NET SDK 10.0.404 (LTS; `global.json` rolls forward on feature bands)
- Avalonia 12.1.6

## Sources

- [What's new in Avalonia 12.1](https://avaloniaui.net/blog/release-12-1), [Avalonia 12](https://avaloniaui.net/blog/avalonia-12), [Avalonia supported platforms](https://docs.avaloniaui.net/docs/supported-platforms)
- [.NET release index](https://dotnetcli.blob.core.windows.net/dotnet/release-metadata/releases-index.json), [NuGet Avalonia](https://api.nuget.org/v3-flatcontainer/avalonia/index.json)
