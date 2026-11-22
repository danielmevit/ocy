# Linux implementation plan (Flathub first)

This document is the plan for Ocy on Linux. It is written from the current code: the frozen contracts in
`rust/crates/damt-platform/src/lib.rs`, the Windows implementations in `rust/crates/damt-platform-win/src/windows/`,
and how `rust/apps/ocy/ocy-app/src/` composes them (`host.rs`, `coordinator.rs`, `tray.rs`, `win32_window.rs`,
`layout.rs`, `instance.rs`, `notifications.rs`). No app code changes are part of this task.

The plan targets **Flathub first** (Flatpak), with a plain tarball as a secondary distribution. Linux is not one
platform: every capability below is given for **X11** and for **Wayland**, separately for **GNOME** and **KDE**,
because the answers differ. Facts are cited; anything not verified in this repo is marked **unverified** — the
spike (L1) must confirm it before we rely on it.


> **Lead review note (2026-11-21), to verify in the spike:**
> - **KDE Plasma's KWin does implement the wlr layer-shell protocol** (`zwlr_layer_shell_v1`, around Plasma 5.20+).
>   The "impossible on KDE" cells for always-on-top / non-focus break cards are likely wrong for KWin. Only GNOME
>   (Mutter) lacks layer-shell. Verify on current Plasma 6 and prefer layer-shell there.
> - **GNOME exposes idle time** over D-Bus (`org.gnome.Mutter.IdleMonitor`), and newer compositors implement
>   `ext-idle-notify-v1`. Check both before treating idle as unavailable.
> - Treat every "impossible" in the matrix as **to be verified** until the M0 spike confirms it on real desktops.

## 0. What the app needs (from the code)

| Frozen trait (`damt-platform`) | Windows impl | What Linux must provide |
| --- | --- | --- |
| `IdleProvider` | `GetLastInputInfo` | time since last input |
| `FocusStateProvider` | `SHQueryUserNotificationState` | fullscreen / presentation / quiet / not-present |
| `SessionEventSource` | `WM_WTSSESSION_CHANGE` + `WM_POWERBROADCAST` | lock/unlock, suspend/resume |
| `AutostartService` | HKCU Run key / `StartupTask` | start on login |
| `ActiveScreenProvider` | `MonitorFromWindow`/`MonitorFromPoint` | bounds of the monitor in use |
| `MotionPreference`, `TaskbarThemeProvider`, `AppThemeProvider` | registry / `AppsUseLightTheme` | reduced motion, light/dark |
| `Notifier` | (Windows notifications) | status notifications |
| — (`damt-overlay`) | native layered Win32 windows | edge frames, long-break dim, per monitor |
| — (`damt-app::SingleInstance`) | named mutex + event | lock file (already exists for Unix) |
| — (`damt-app` paths) | `%APPDATA%` | `$XDG_CONFIG_HOME/ocy` (already implemented) |
| — (`slint_tray.rs`) | Slint `SystemTrayIcon` | StatusNotifierItem tray + menu |

The engine already degrades safely: `FocusState::Unknown` never defers a break, and an unsupported
`IdleProvider` just means no away-credit by idle. That is the shape of the Wayland fallbacks.

## 1. Capability matrix (summary)

| Capability | X11 | Wayland (GNOME) | Wayland (KDE) |
| --- | --- | --- | --- |
| Idle time | ✅ XScreenSaver extension | ✅ ext-idle-notify-v1 (GNOME 46+, **unverified**) or Mutter IdleMonitor D-Bus | ✅ ext-idle-notify-v1 (Plasma 5.27+, **unverified**) |
| Lock/unlock | ✅ logind D-Bus | ✅ logind D-Bus | ✅ logind D-Bus |
| Sleep/resume | ✅ logind D-Bus | ✅ logind D-Bus | ✅ logind D-Bus |
| Fullscreen detection | ✅ EWMH `_NET_WM_STATE_FULLSCREEN` | ❌ impossible | ❌ impossible |
| Presentation mode | ❌ no signal on Linux | ❌ impossible | ❌ impossible |
| Tray + menu | ✅ StatusNotifierItem | ✅ SNI (needs the AppIndicator extension) | ✅ SNI (native) |
| Break card: never focus | ✅ (`WM_HINTS` `input=false` + `_NET_WM_USER_TIME=0` + `NOTIFICATION` type; WM check pending) | ✅ don't request xdg-activation (compositor check pending) | ✅ don't request xdg-activation (compositor check pending) |
| Break card: always on top | ✅ `_NET_WM_STATE_ABOVE` + `_NET_WM_WINDOW_TYPE_NOTIFICATION` | ❌ impossible (no layer-shell in GNOME) | ❌ impossible (no layer-shell in KWin) |
| Click-through overlay | ✅ `set_cursor_hittest(false)` / XShape (`_NET_WM_CM_S<n>` composited) | ✅ empty `wl_surface` input region (`set_cursor_hittest(false)`) | ✅ empty `wl_surface` input region (`set_cursor_hittest(false)`) |
| Position on a monitor | ✅ full control (RandR + `_NET_WORKAREA` + `Xft.dpi`) | ❌ only "fullscreen on output N" | ❌ only "fullscreen on output N" |
| Know the *active* monitor | ✅ `_NET_ACTIVE_WINDOW` | ❌ impossible | ❌ impossible |
| Autostart | ✅ `~/.config/autostart` .desktop | ✅ Background portal (in Flatpak) | ✅ Background portal (in Flatpak) |
| Notifications | ✅ org.freedesktop.Notifications | ✅ same | ✅ same |
| Single instance | ✅ lock file (exists) | ✅ same | ✅ same |
| Settings path | ✅ `$XDG_CONFIG_HOME/ocy` | ✅ same | ✅ same |
| Light/dark theme | ✅ Settings portal / gsettings | ✅ same | ✅ same |
| Fonts | ✅ runtime fonts + fontconfig | ✅ same | ✅ same |

## 2. Idle time

### X11
- **API:** the XScreenSaver extension — `XScreenSaverQueryInfo` returns `XScreenSaverInfo.idle`, milliseconds
  since the last keyboard/pointer input, exactly the semantic of `IdleProvider::idle_time`.
  Source: https://man.archlinux.org/man/XScreenSaverQueryInfo.3 and the extension spec
  https://www.x.org/releases/current/doc/xextproto/saver.html
- **Crate:** `x11rb` (pure Rust, generated protocol bindings include the screensaver extension):
  https://docs.rs/x11rb — **unverified** that the screensaver module covers `QueryInfo` with the info struct;
  confirm in the spike.
- **Flatpak:** works as-is; the app talks to the X server through the sandbox's X11 socket
  (`--socket=fallback-x11 --share=ipc`). No portal exists or is needed.

### Wayland (GNOME and KDE)
There is **no queryable global idle time on Wayland** — clients cannot see each other's input. Two ways in:

- **ext-idle-notify-v1** (the intended cross-compositor answer, staged in wayland-protocols):
  the client arms a timer; the compositor sends `idle` when the user has been inactive that long and `resume`
  on the next activity. Protocol:
  https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-idle-notify/ext-idle-notify-v1.xml
  - Support: wlroots compositors yes; **KWin since Plasma 5.27 (believed, unverified)**; **Mutter since
    GNOME 46 (believed, unverified)**. The spike must print the bound protocols per compositor.
  - It is threshold-based, not a query. To implement `idle_time()` we arm a short timer (e.g. 1 s) only while
    the engine needs precision (the away threshold and the natural-moment lull wait), and reconstruct the
    span: idle began at `idle_event_time − timeout`, ended at `resume_event_time`. For the away rule this is
    exact enough; a 1 s re-arm loop only runs while waiting for a lull, so the idle-CPU budget survives.
- **GNOME fallback: Mutter IdleMonitor D-Bus** — `org.gnome.Mutter.IdleMonitor.GetIdletime()` on
  `/org/gnome/Mutter/IdleMonitor/Core` (the same service gnome-shell's own JS uses). It is a queryable idle
  time. It is **not a documented stable API**; it is what Caffeine-style extensions use. **Unverified** that
  Flatpak's session-bus proxy permits the `org.gnome.Mutter` name — the manifest requests
  `--talk-name=org.gnome.Mutter`, and the spike must confirm.
  Source: mutter source tree, https://gitlab.gnome.org/GNOME/mutter (search `IdleMonitor`).
- **KDE fallback:** `org.freedesktop.ScreenSaver.GetSessionIdleTime` is implemented by KDE's screensaver
  service (**believed, unverified**); GNOME does **not** implement that method (only `GetActive`). Do not
  build on it; treat ext-idle-notify as the KDE answer.
- **Flatpak:** the Wayland socket is passed through directly (no proxy), so protocol support depends purely
  on the host compositor. D-Bus fallbacks go over the session bus (see permissions in §13).
- **Last-resort fallback:** `is_supported() == false` — the engine then never enters Away by idle (lock/unlock
  away credit still works). Degraded but honest.

## 3. Session lock/unlock and sleep/resume

- **API (both X11 and Wayland, both desktops):** systemd-logind D-Bus on the **system bus**:
  - `org.freedesktop.login1.Manager`: `PrepareForSleep(boolean start)` → `Suspending`/`Resumed`.
  - The session object (`/org/freedesktop/login1/session/...`) emits `Lock` / `Unlock` → `Locked`/`Unlocked`.
  Source: https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html
- **Crate:** `zbus` (pure Rust D-Bus): https://docs.rs/zbus
- **Flatpak permission:** the system bus is filtered; the manifest needs
  `--system-talk-name=org.freedesktop.login1`. This is a common, accepted permission (used for
  suspend/resume and battery apps). **Unverified** whether the Lock/Unlock *session* signals arrive without
  also matching the session path — the spike must observe a real lock/unlock inside the sandbox.
- **Session-bus alternative for lock/unlock:** `org.freedesktop.ScreenSaver.ActiveChanged` (implemented by
  GNOME's shell and by KDE; **believed, unverified for both**) — useful if the logind permission is refused
  in review. Sleep/resume has no session-bus equivalent; the engine's "dt > 15 s = sleep gap" rule
  (ARCHITECTURE §3.2 rule 1) is the built-in fallback and already credits the gap correctly.
- **Non-logind systems** (e.g. elogind, no systemd): `is_supported() == false`; the sleep-gap rule covers
  resume, lock events are lost. Acceptable for the Flatpak target (Flathub effectively assumes systemd hosts).

## 4. Full-screen / presentation / game detection

- **X11:** EWMH over the root window — read `_NET_ACTIVE_WINDOW`, then check the window's
  `_NET_WM_STATE_FULLSCREEN` atom (games and video players set it; SDL/GL fullscreen windows do too).
  Map to `FocusState::FullscreenApp`. Source: https://specifications.freedesktop.org/wm-spec/latest/
  Crate: `x11rb`. Works inside Flatpak (X11 socket). Caveat: a fullscreen *window* is not proof of a
  fullscreen *app* in the Windows `SHQueryUserNotificationState` sense, and some games unmap/override —
  treat as heuristic, same as every Linux break reminder.
- **Presentation mode:** Linux has **no equivalent signal on any display server** — there is no
  `QUNS_PRESENTATION_MODE`. We never report `Presentation` on Linux. (Windows gets it from the OS; Linux
  apps like Impress don't broadcast it.)
- **Wayland (GNOME and KDE):** **impossible.** A Wayland client cannot enumerate or inspect other windows;
  there is no portal for it either. Fallback: report `FocusState::Unknown` → the engine does not defer breaks
  during fullscreen on Wayland. Mitigations to consider later (not MVP): an idle-based heuristic (a user
  presenting is often mouse-idle but that conflicts with away-credit), or a manual "don't interrupt me for
  the next N minutes" tray item. This is the single biggest functional gap on Wayland and we state it in the
  Flathub description rather than fake it.

## 5. Tray icon and its menu

- **API:** StatusNotifierItem (SNI) + the DBusMenu protocol, registered with `org.kde.StatusNotifierWatcher`.
  Sources: https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/ and
  https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/StatusNotifierItem/ (the menu spec is
  the `com.canonical.dbusmenu` protocol referenced there).
- **Desktop support:** KDE Plasma ships SNI natively. **GNOME does not** — the user needs the
  AppIndicator/KStatusNotifierItem support extension: https://extensions.gnome.org/extension/615/appindicator-support/
  We must detect a missing watcher and show a status message (and keep the app fully usable without a tray:
  Settings must be reachable another way — e.g. autostart + a notification action, or a `.desktop` launch
  opens Settings when the instance is already running, which the single-instance signal already does).
- **Crate:** first choice is Slint's `SystemTrayIcon` (what `slint_tray.rs` already uses) — **its Linux
  support must be verified in the spike** (docs: https://docs.slint.dev/latest/docs/slint/ — the element is
  documented as Windows/macOS in older releases; Linux status **unverified**). Fallback: `ksni`, a pure-Rust
  SNI + DBusMenu implementation with no GTK dependency: https://docs.rs/ksni — it fits the no-GTK Flatpak
  runtime and the menu model in `tray.rs` maps cleanly onto it. (`tray-icon` pulls GTK on Linux; avoid.)
- **Flatpak:** SNI registration goes over the session bus and works in the sandbox (many Flathub tray apps
  do this); the manifest adds `--talk-name=org.kde.StatusNotifierWatcher` explicitly even if the default
  session-bus policy already allows it (**unverified** which names Flatpak allows by default).
- **Icon theme:** the 4 states × light/dark PNG sets already exist; on Linux pick the set from the color-scheme
  portal (§11) instead of the Windows taskbar registry.

## 6. Break cards: never take focus, stay on top

### X11 (implemented in `L2a` — pure helpers tested; live WM check pending)
- `ocy_app::linux_window` (the Linux twin of `win32_window.rs`) + `damt_platform_linux::apply_x11_break_window_hints`:
  - Before map: `WindowAttributesExtX11::with_x11_window_type(vec![WindowType::Notification])` and `.with_active(false)`.
  - After winit window creation: extracts the X11 window ID from `raw-window-handle` (`Xlib` / `Xcb`) and uses a separate `x11rb` connection to set:
    - `_NET_WM_WINDOW_TYPE` = `_NET_WM_WINDOW_TYPE_NOTIFICATION` (no taskbar entry, no Alt+Tab, WM keeps above);
    - `_NET_WM_STATE` property and `_NET_WM_STATE` root `ClientMessage`s (`_NET_WM_STATE_ADD`): `_NET_WM_STATE_ABOVE`, `_NET_WM_STATE_SKIP_TASKBAR`, `_NET_WM_STATE_SKIP_PAGER`;
    - ICCCM `WM_HINTS` `input = false` (never request keyboard focus) plus `_NET_WM_USER_TIME = 0` (suppress focus on initial map).
- WM-dependent behaviour (e.g. tiling WMs like i3/sway) remains an inherent X11 caveat; needs a real X11 WM pass.
- **Flatpak:** works through the X11 socket.

### Wayland (GNOME and KDE — implemented in `L2a` — pure helpers tested; live compositor check pending)
- **Never take focus:** implemented via `.with_active(false)` and never requesting `xdg_activation_v1` tokens or user attention (`xdg-activation`: https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/xdg-activation/xdg-activation-v1.xml). Compositor check still pending on GNOME Mutter and KDE KWin.
- **Always on top: impossible on GNOME and KDE.** The layer-shell protocol
  (https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml)
  is supported by wlroots compositors (Sway, Hyprland, …), but **neither Mutter nor KWin implements it** for regular clients, and winit creates `xdg_toplevel` windows rather than layer-shell surfaces.
- **Implemented behaviour (per card type):**
  - **Long-break dim (and `shortBreaks.fullscreen` dim):** `LinuxOverlays::show_dims` creates one `LinuxDimOverlay` borderless fullscreen window per winit output (`Fullscreen::Borderless(Some(monitor))`, enumerated via `winit::window::Window::available_monitors()`) with an empty `wl_surface` input region via `winit::window::Window::set_cursor_hittest(false)` (winit creates an empty `WlRegion` and calls `wl_surface.set_input_region(Some(region))`, applied immediately and reinforced on `RedrawRequested` so the first frame commit carries it). All dim windows are shown before `LongBreakCard` / `ShortBreakCard` (`dim_backdrop = false`, input-accepting); stacking between our own windows is compositor behaviour and **needs a real desktop check** on GNOME and KDE.
  - **Short-break card / warning:** normal small `xdg_toplevel` window with no activation request. **Documented limitation:** it may be covered by other windows and cannot be placed on a specific monitor.
  - **Edge frames on the other monitors:** **omitted on Wayland** (cannot position windows across outputs); the fullscreen dim covers long breaks and fullscreen short breaks.
- **XWayland trick (spike only):** an override-redirect X11 window under XWayland is unmanaged and some
  compositors place such surfaces above managed windows. Not used in `L2a`; mutter has been tightening this.

## 7. Multi-monitor placement

- **X11 (implemented in `L2a` — pure helpers tested; live multi-monitor check pending):**
  `damt_platform_linux::query_x11_monitors` queries RandR CRTCs, `randr_get_output_primary`, `_NET_WORKAREA` (for `_NET_CURRENT_DESKTOP`), root `RESOURCE_MANAGER` `Xft.dpi`, and `_NET_WM_CM_S<screen>` compositor presence. `linux_window::enumerate_monitors` feeds those into `select_active_screen` alongside `ActiveScreenProvider::active_screen_bounds()` (`_NET_ACTIVE_WINDOW` / pointer), placing cards on the active monitor's working area or centre. Click-through edge frames (`LinuxEdgeFrameOverlay` + `set_cursor_hittest(false)` XShape input region) are shown on non-active monitors only when an X11 compositing manager is active (`_NET_WM_CM_S<screen>` owned), avoiding opaque black overlays on uncomposited desktops.
- **Wayland (GNOME and KDE):**
  - A client **cannot position a window** and **cannot know which monitor has the focused window** (no global
    coordinates, no window list). Both are protocol-level, not a missing crate.
  - **Implemented in `L2a`:** `linux_window::enumerate_monitors` returns empty on Wayland; long-break and fullscreen short-break dims enumerate `winit` `available_monitors()` and open one borderless fullscreen click-through window per output. Small cards open as normal windows placed by the compositor; edge frames are dropped on Wayland.

### L2a status — verified by tests vs. needs a real desktop check
- **Verified by unit tests (`cargo test --workspace`):**
  - X11 vs. Wayland vs. Headless backend detection (`select_display_backend`, including `DISPLAY=:0` under Wayland).
  - Window policy matrix (`LinuxWindowPolicy`), `WindowAttributePlan`, and `LinuxOverlayMode` selection.
  - Raw window handle X11 ID extraction (`Xlib` / `Xcb` -> `Some(u32)`, `Wayland` -> `None`).
  - EWMH `_NET_WM_WINDOW_TYPE_NOTIFICATION`, `_NET_WM_STATE` atom merging (`_NET_WM_STATE_ABOVE`, `_NET_WM_STATE_SKIP_TASKBAR`, `_NET_WM_STATE_SKIP_PAGER`), and `_NET_WM_STATE_ADD` `ClientMessage` packing.
  - `Xft.dpi` parsing, `_NET_WORKAREA` desktop indexing + monitor intersection, RandR primary-output resolution, and `X11MonitorInfo` -> `damt_overlay::MonitorInfo` conversion.
  - Wayland per-output dim window assignment (`plan_wayland_dim_outputs`).
  - Headless soak/integration lifecycle (`SlintBreakWindows` + `OverlayBreakWindows` creating and closing overlays with 0 leaked windows).
- **Needs a real desktop check (not run here; no GUI launched in WSLg):**
  - **CI smoke (`L3s`, `.github/workflows/linux-smoke.yml`):** `x11-smoke` launches `--demo-break short|warning|long`, `--demo-settings`, and `--report-memory` (asserting idle `private_mb` ≤ 15 MB) under `Xvfb` + `openbox` + a focused `xterm`, checks mapped windows, no focus theft, and `long` card-above-dim stacking in `_NET_CLIENT_LIST_STACKING`, uploads screenshots/logs (`ocy-linux-smoke`), and runs a best-effort headless `weston` check for `--demo-break short`; `tarball` builds and uploads `Ocy-<version>-linux-x86_64.tar.gz` + `.sha256` (`ocy-linux-tarball`).
  - **Still needs a real GNOME / KDE / multi-monitor desktop pass:**
    - X11 (GNOME on X11, KDE on X11, XFCE, and a tiling WM like i3): confirm `_NET_WM_WINDOW_TYPE_NOTIFICATION` + `_NET_WM_STATE_ABOVE`/`SKIP_TASKBAR`/`SKIP_PAGER` + `WM_HINTS.input=false` keep break cards above, off the taskbar/Alt+Tab, clickable without stealing focus, placed on the active monitor's corner/centre, and XShape edge frames render transparently without capturing clicks.
    - Wayland (GNOME Mutter and KDE KWin): confirm cards never steal focus (`with_active(false)`), `LinuxDimOverlay` goes borderless fullscreen on every connected `wl_output`, `set_cursor_hittest(false)` makes dims click-through, `LongBreakCard` stacks above the dim window so "End break" is visible and clickable, and SNI tray / Mutter idle / Background portal autostart work in a real desktop session.

## 8. Autostart

- **Outside Flatpak (tarball):** write `~/.config/autostart/xyz.damt.ocy.desktop` per the autostart spec:
  https://specifications.freedesktop.org/autostart-spec/latest/ — the direct equivalent of the HKCU Run key.
- **Inside Flatpak:** the app's `~/.config` is the sandbox's `~/.var/app/<id>/config`, which the desktop's
  session **does not read** for autostart. The correct mechanism is the **Background portal**:
  `org.freedesktop.portal.Background.RequestBackground` with `autostart=true` and a `reason`; `SetBackground`
  clears it. The portal writes the autostart entry on the host side.
  Source: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Background.html
  Portals need no finish-args. **Unverified:** whether GNOME's and KDE's portal backends both honour
  `autostart` (they are supposed to; the spike must toggle it on both).
- The `AutostartService` implementation picks: portal when `FLATPAK_ID`/`container` says Flatpak, `.desktop`
  file otherwise. `--autostart` argument handling stays as on Windows.

## 9. Notifications

- **API:** `org.freedesktop.Notifications` (`Notify`), spec:
  https://specifications.freedesktop.org/notification-spec/latest/
- **Crate:** `notify-rust` (pure Rust, D-Bus): https://docs.rs/notify-rust
- **Flatpak:** the session-bus name is allowed by default (**believed**; the manifest lists
  `--talk-name=org.freedesktop.Notifications` explicitly). One real gotcha: the notification daemon runs on
  the host and **cannot read icon paths inside the sandbox** — use the app icon *name* (installed into
  `/app/share/icons/hicolor` by the manifest) rather than a file path, or the icon silently drops.
  **Unverified** per desktop; spike item.
- The ship gate `STATUS_NOTIFICATIONS_SHIPPED` stays `false` until the owner approves, same as Windows.

## 10. Single instance

- `damt_app::SingleInstance` already has the Unix path: a lock file in the app's data directory plus a signal
  channel, and `instance.rs` resolves `xyz.damt.ocy`. Inside Flatpak the data dir is
  `~/.var/app/xyz.damt.ocy/config/ocy` — persistent and per-app, so the lock works unchanged.
- Second launch → signal → first instance opens Settings: the existing channel works; **verify** the
  signal path under the Flatpak sandbox (it is a file in the same sandbox, so it should; **unverified**).
- Optional later: D-Bus activation (`DBusActivatable=true` in the `.desktop`) so `flatpak run` hands off
  through the session bus. Not needed for MVP.

## 11. Settings path, theme, motion

- **Settings path:** `$XDG_CONFIG_HOME/ocy` (default `~/.config/ocy`) — `damt-app` already implements this
  (`rust/crates/damt-app/src/paths.rs`). Inside Flatpak it becomes `~/.var/app/xyz.damt.ocy/config/ocy`
  automatically. No code change expected; verify in the spike.
- **Light/dark (`AppThemeProvider`, tray set):** the XDG desktop portal
  `org.freedesktop.portal.Settings.ReadSetting("org.freedesktop.appearance", "color-scheme")` returns
  `prefer-dark`/`default`. Works on GNOME and KDE, inside and outside Flatpak (portals live on the host
  session bus). Sources: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html
  and https://developer.gnome.org/documentation/guides/dark-style.html
  Outside a portal session, read `org.gnome.desktop.interface color-scheme` / KDE's equivalent directly via
  `zbus` (**KDE key unverified**).
- **Taskbar theme:** on Linux there is no taskbar registry; the tray set follows the same color-scheme value
  (GNOME top bar is always dark-ish; KDE follows the theme). Map `TaskbarThemeProvider` to the same portal
  value, or leave `None` and let the existing fallback in `tray.rs` pick from the app theme.
- **Reduced motion (`MotionPreference`):** GNOME has `org.gnome.desktop.interface enable-animations`; KDE has
  animation speed settings. There is **no portal** for it. Read the GNOME key via `zbus` when present;
  otherwise `false` (animations allowed). **Unverified** on KDE; acceptable to ship as always-false there.

## 12. Fonts

- Slint's software renderer picks system fonts through its own font database (fontdb) — on Linux it scans
  the usual fontconfig directories. **Unverified** how Slint resolves families inside the Flatpak runtime;
  spike item.
- **Flatpak runtime choice:** `org.freedesktop.Platform` (no GTK, smallest) ships DejaVu and the basics;
  `org.gnome.Platform` adds Cantarell and more scripts. If the i18n languages (Cyrillic mk, cs/sk
  diacritics, …) miss glyphs, bundle the needed font in `/app/share/fonts` — the same decision as the
  Windows fallback-font risk in PLAN.md §11. Runtime font lists:
  https://docs.flatpak.org/en/latest/available-runtimes.html
- The design font stack must be checked against what the runtime actually provides; do not assume
  a user font is present.

## 13. Flatpak permissions summary (draft)

| finish-arg | Why | Needed? |
| --- | --- | --- |
| `--socket=wayland` | UI on Wayland | yes |
| `--socket=fallback-x11 --share=ipc` | UI + idle + fullscreen detection on X11 | yes |
| `--system-talk-name=org.freedesktop.login1` | lock/unlock + suspend/resume signals | yes (sleep-gap rule is the fallback if refused) |
| `--talk-name=org.kde.StatusNotifierWatcher` | tray registration | probably default-allowed; list explicitly |
| `--talk-name=org.freedesktop.Notifications` | status notifications | probably default-allowed; list explicitly |
| `--talk-name=org.gnome.Mutter` | idle fallback on GNOME < 46 | only if the spike proves it works |
| (none) | autostart, theme, color-scheme | via portals, no finish-args |

No filesystem permissions, no device access (software renderer needs no GPU), no network. This is a
small, reviewable permission set — good for Flathub review.

## 14. Phased plan

### L0 — this plan (done in this task)
`docs/rust/LINUX.md`, the draft manifest in `packaging/linux/`, a changelog entry. No app code.

### L1 — spike (prove the unverified facts)
A scratch binary (not shipped) that prints, per session type (X11, GNOME Wayland, KDE Wayland):
- bound Wayland protocols (is `ext_idle_notify_v1` there?), idle events reconstructed vs. a stopwatch;
- logind Lock/Unlock/PrepareForSleep received inside the Flatpak sandbox;
- SNI registration + menu on KDE and on GNOME with/without the AppIndicator extension; Slint
  `SystemTrayIcon` on Linux vs `ksni`;
- card window: no-activation mapping, stacking vs. other windows, fullscreen-per-output dim with empty
  input region (click-through), pointer-enter "last output" heuristic;
- X11: `_NET_WM_STATE_FULLSCREEN` detection, window-type/above properties honoured by the installed WMs;
- portals: Background autostart toggle, color-scheme read; notification icon name resolution;
- fonts present in the runtime for the shipped languages.
**Exit:** a go/no-go table like R0's, updating this document's "unverified" marks.
**Effort:** 2–3 days.

### L2 — `damt-platform-linux` crate (X11 complete, Wayland best-effort)
- New crate `rust/crates/damt-platform-linux` implementing the frozen traits: idle (XScreenSaver /
  ext-idle-notify / Mutter D-Bus), session (logind via zbus), focus (X11 EWMH; `Unknown` on Wayland),
  autostart (portal / .desktop), active screen (X11; primary on Wayland), theme (portal), notifier
  (notify-rust). Pure mapping helpers unit-tested like `damt-platform-win/src/pure.rs`.
- CI: a Linux job already exists; add the crate's tests there.
**Effort:** 4–6 days.

### L3 — app integration (MVP: X11 full + GNOME Wayland reduced)
- Wire `damt-platform-linux` into `ocy-app` behind `cfg(target_os = "linux")` (the `win32_window.rs`
  role/styling code is Windows-only; the Linux path needs its own window-role module).
- X11: full parity — active-monitor card, edge frames, always-on-top, no taskbar button.
- Wayland: no-activation cards, fullscreen dim per output with click-through input region, no edge frames,
  no fullscreen deferral; tray via SNI; document the differences in the README/Flathub description.
- `--report-memory`/smoke runs on Linux; budget checks (idle RAM ≤ 15 MB must hold — the Flatpak sandbox
  adds nothing to the process itself). CI smoke + tarball build added in `.github/workflows/linux-smoke.yml` (`L3s`).
**Effort:** 5–8 days.

### L4 — KDE pass
- Re-run the L1 matrix on Plasma; fix SNI quirks, color-scheme, autostart portal, KWin focus behaviour.
**Effort:** 2–3 days (mostly hands-on; depends on L1 findings).

### L5 — Flatpak packaging + Flathub submission
- **Done (`L5a`):**
  - Finalized `packaging/linux/xyz.damt.ocy.yml` targeting `org.freedesktop.Platform` / `org.freedesktop.Sdk` `26.08` with `org.freedesktop.Sdk.Extension.rust-stable` (Rust 1.98.0, satisfying `rust/rust-toolchain.toml` `1.95.0`), offline `cargo --offline build --release --locked -p ocy-app`, `/app/bin/Ocy`, `.desktop`, metainfo, and hicolor icons (`64x64`, `128x128`, `256x256`, `512x512` rendered from SVG when `rsvg-convert` is present, and `scalable` SVG).
  - Updated `packaging/linux/xyz.damt.ocy.desktop` (`Exec=Ocy`, `Icon=xyz.damt.ocy`, `StartupWMClass=Ocy` to match the X11 `WM_CLASS` emitted from `argv[0]`).
  - Updated `packaging/linux/xyz.damt.ocy.metainfo.xml` (release `0.2.2`, OARS `1.1`, homepage, developer `Daniel Mevit`, and screenshots pointing at `main` raw URLs).
  - Added `.github/workflows/linux-flatpak.yml` to validate `.desktop` + metainfo, generate `cargo-sources.json` via `flatpak-cargo-generator.py`, build the bundle, and upload artifact `ocy-linux-flatpak` (`Ocy-<version>-x86_64.flatpak`).
- **Tester install (from the `ocy-linux-flatpak` CI artifact):**
  ```sh
  flatpak install --user ./Ocy-<version>-x86_64.flatpak
  flatpak run xyz.damt.ocy
  ```
- **Still to do before Flathub submission:**
  - **Commit `cargo-sources.json`:** CI generates `packaging/linux/cargo-sources.json` dynamically from `rust/Cargo.lock`; Flathub builds offline from the manifest repo and will require `cargo-sources.json` to be committed alongside the manifest.
  - **Wayland `app_id` / `StartupWMClass`:** `ocy-app` does not currently call `slint::set_xdg_app_id("xyz.damt.ocy")` (or winit `with_name`), so X11 sets `WM_CLASS=("Ocy", "Ocy")` and Wayland leaves `xdg_toplevel.app_id` unset. Add `slint::set_xdg_app_id(ocy_app::instance::APP_ID)` in Rust and update `StartupWMClass` so Wayland compositors associate windows with `xyz.damt.ocy.desktop`.
  - **Flatpak autostart:** `LinuxAutostart` currently writes `$XDG_CONFIG_HOME/autostart/xyz.damt.ocy.desktop`, which stays inside `~/.var/app/xyz.damt.ocy/config/autostart` in Flatpak; implement the `org.freedesktop.portal.Background` path (§8) when running inside Flatpak.
  - **Desktop testing & Flathub review:** test `--user` install and runtime behaviour on GNOME and KDE (including `org.gnome.Mutter.IdleMonitor` and `org.freedesktop.login1` sandbox permissions), run `flatpak-builder-lint`, verify screenshot `width`/`height` attributes, and resolve `xyz.damt.ocy` reverse-DNS domain proof before submitting: https://docs.flathub.org/for-app-authors/submission
**Effort:** 2–3 days plus review wait.

## 15. Honest effort estimate

| Phase | Days |
| --- | --- |
| L0 plan (this) | 1 |
| L1 spike | 2–3 |
| L2 platform crate | 4–6 |
| L3 app integration | 5–8 |
| L4 KDE pass | 2–3 |
| L5 Flatpak/Flathub | 2–3 |
| **Total** | **16–24 working days** (3–5 weeks with reviews) |

The wide range is honest: L1 can move items between "works" and "impossible" (tray on GNOME without the
extension, Mutter idle D-Bus in the sandbox, stacking of our own windows). The X11 path is low-risk; the
Wayland path is mostly *known-limited*, not unknown — the limits are protocol-level and listed above.

## 16. The biggest risks

1. **Wayland has no fullscreen detection and no always-on-top** — the two features that make Ocy polite on
   Windows. Nothing can fix this on GNOME/KDE; we ship the degraded behaviour and say so. (Protocol-level;
   layer-shell is wlroots-only.)
2. **Idle time on Wayland is compositor-dependent** (ext-idle-notify version support, Mutter D-Bus access
   in the sandbox). Worst case: no away-credit by idle on some systems.
3. **GNOME tray needs a third-party extension** — a stock GNOME user has no tray at all; the app must stay
   usable without it.
4. **Flathub app-ID ownership** (`xyz.damt`) is unverified and could force a rename.
5. **Spike-level unknowns** (Slint `SystemTrayIcon` on Linux, notification icons from the sandbox, our own
   windows' stacking) — all cheap to test, all capable of changing the L3 design.

## 17. Source index

- X11 screensaver ext: https://man.archlinux.org/man/XScreenSaverQueryInfo.3 · https://www.x.org/releases/current/doc/xextproto/saver.html
- EWMH (window states, active window): https://specifications.freedesktop.org/wm-spec/latest/
- ext-idle-notify-v1: https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-idle-notify/ext-idle-notify-v1.xml
- xdg-activation: https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/xdg-activation/xdg-activation-v1.xml
- wlr-layer-shell (wlroots only): https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml
- logind D-Bus: https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html
- StatusNotifierItem: https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/
- Notifications: https://specifications.freedesktop.org/notification-spec/latest/
- Autostart spec: https://specifications.freedesktop.org/autostart-spec/latest/
- Background portal: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Background.html
- Settings portal (color-scheme): https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html · https://developer.gnome.org/documentation/guides/dark-style.html
- Flatpak sandbox permissions: https://docs.flatpak.org/en/latest/sandbox-permissions.html
- Flathub submission: https://docs.flathub.org/for-app-authors/submission · metainfo: https://docs.flathub.org/for-app-authors/metainfo-guidelines/
- GNOME AppIndicator extension: https://extensions.gnome.org/extension/615/appindicator-support/
- Crates: x11rb https://docs.rs/x11rb · zbus https://docs.rs/zbus · ksni https://docs.rs/ksni · notify-rust https://docs.rs/notify-rust · smithay-client-toolkit https://docs.rs/smithay-client-toolkit (only if a layer-shell path is ever added) · Slint https://docs.slint.dev
