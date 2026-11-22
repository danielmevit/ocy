# macOS implementation plan

This document describes the planned macOS implementation for Ocy. It is a planning
document for migration work; M0 adds no macOS-only behaviour beyond the build and
test coverage.

The Rust workspace is already structured so platform-independent crates compile on
every target. Windows-only implementations are isolated behind `#[cfg(windows)]`,
and macOS currently uses `damt_platform::Unsupported` plus the existing non-Windows
single-instance and application-support path stubs. The native `x86_64-apple-darwin`
target check passes in this workspace (`cargo check --workspace --all-targets --locked
--target x86_64-apple-darwin`); native `macos-latest` CI is the final compile/test
gate.

## Platform capabilities

| Capability | macOS approach | Rust and Apple crates/API |
| --- | --- | --- |
| Idle time | Read the I/O HID idle property from IOKit and convert it to milliseconds for `IIdleProvider`/the existing idle contract. Start with `IORegistryEntryCreateCFProperties` on the service matching `IOHIDSystem`, then read `HIDIdleTime`. Validate units and overflow on macOS hardware. | IOKit (`IOHIDSystem`, `HIDIdleTime`, `IORegistryEntryCreateCFProperties`) via `objc2`/core-foundation bindings or a narrow FFI; no high-level Rust crate is assumed. Exact Rust wrapper: **unverified**. |
| Lock/sleep/wake | Subscribe to `NSWorkspace` notifications for screen locking/unlocking and sleep/wake, and map them to `SessionEventKind`. Add readiness and shutdown cleanup. Test physical lock, user switch, sleep, lid close, and wake. | `NSWorkspace`: `NSWorkspaceDidWakeNotification`, `NSWorkspaceWillSleepNotification`, `NSWorkspaceSessionDidBecomeActiveNotification`, `NSWorkspaceSessionDidResignActiveNotification`; use `objc2-app-kit` and `objc2-foundation`. Crate/API bindings: **unverified**. |
| Full-screen/presentation detection | Before implementation, prototype an observable public-API signal. `NSWorkspace.didActivateApplicationNotification`, app presentation options, active `NSScreen`, and `CGWindowListCopyWindowInfo` can contribute context, but no public API is known to be a direct, complete equivalent of Windows `SHQueryUserNotificationState`. | `NSWorkspace`, `NSApplicationPresentationOptions`, `NSScreen`, and `CGWindowListCopyWindowInfo`; exact fullscreen/window-state mapping: **unverified**. |
| Menu-bar icon | Build a native status item and menu. Use a maintained cross-platform tray abstraction where it fits the winit event loop; retain native fallback for edge behavior. | `tray-icon` supports macOS and says icon creation must happen on the main thread while the event loop is running: https://docs.rs/tray-icon/latest/tray_icon/. Native fallback: `NSStatusItem`/`NSStatusBar` via `objc2-app-kit`; exact bindings: **unverified**. |
| Break cards without focus; full-screen spaces | Keep break cards non-activating and borderless/auxiliary. Obtain the `NSWindow` or create an `NSPanel` from the existing winit window, set a non-activating policy, high window level, and `canJoinAllSpaces`/`fullScreenAuxiliary` collection behavior. Verify mouse and keyboard behavior, menu-bar coexistence, Mission Control, and each macOS full-screen space. | winit raw-window handle: `RawWindowHandle` integration and macOS `NSWindow`: https://docs.rs/winit/latest/winit/. `NSPanel`, `setBecomesKeyOnlyIfNeeded`, `setLevel`, `NSWindowCollectionBehavior`, and `NSStatusItem` belong to AppKit: https://developer.apple.com/documentation/appkit/nspanel and https://developer.apple.com/documentation/appkit/nswindow. Rust objc2 conversion/API names: **unverified**. |
| Multi-monitor placement | Enumerate `NSScreen.screens`, obtain visible frames, map points and DPI coordinate conventions, and select the active or primary screen. Follow winit monitor APIs for placement where they expose the correct NSScreen. Recheck when screens, spaces, or menu-bar/Dock configuration changes. | winit monitor APIs plus `NSScreen` and `NSScreen.mainScreen`/`screens`: https://developer.apple.com/documentation/appkit/nsscreen. Rust coordinate mapping: **unverified**. |
| Login item | Implement a settings toggle using the modern login-item service. It should target the app bundle, not an embedded helper unless the app later needs one. Check registration state and surface authorization/error states. | `SMAppService.mainApp`, `register()`, `unregister()`, `status` via ServiceManagement: https://developer.apple.com/documentation/servicemanagement/smappservice. `objc2-service-management` bindings/API names: **unverified**. |
| Notifications | Request notification authorization once and send local notifications for status/long-break events according to settings. Reuse in-app cards for the break itself; notifications should not be used to replace the card or steal focus. | `UNUserNotificationCenter`, `UNMutableNotificationContent`, and `UNNotificationRequest`: https://developer.apple.com/documentation/usernotifications/unusernotificationcenter. Rust bindings: `objc2-user-notifications`/`objc2-foundation`, exact API names: **unverified**. |
| Single instance | The existing Unix lock-file implementation already provides advisory single-instance exclusion and can use `~/Library/Application Support/Ocy/.ocy.lock`. It does not yet forward a second launch to the first instance. A macOS activation phase should use an inter-process event and move the UI to settings/focus only on explicit second launch. | Existing `damt-app::SingleInstance`; candidate implementation: `NSDistributedNotificationCenter` or a local Mach port, exact Rust API: **unverified**. |
| Settings path | Use `~/Library/Application Support/Ocy` for settings, state, statistics, logs, and demo data. `damt-app::AppDataPaths` already implements this by reading `HOME`. | Existing `damt-app` path helper; Apple convention: https://developer.apple.com/documentation/foundation/nssearchpathdirectory/application_support |
| Fonts | Prefer native SF/NSFont text metrics and Slint's existing font pipeline for the shipped UI. Bundle only fonts that are intentionally licensed; do not assume a user font is present. Verify SF, CJK, fallback, and system text-size behavior. | AppKit `NSFont` and Core Text: https://developer.apple.com/documentation/appkit/nsfont; Slint font loading: https://slint.dev/docs/rust/slint/backend_winit/; exact integration: **unverified**. |

## Distribution without the Apple $99 programme

An unsigned, self-built `.app` in a zip or DMG is the first community distribution
format. macOS can show **"Apple could not verify 'Ocy.app' is free of malware"** or
**"The application cannot be opened because the developer cannot be verified"**,
depending on macOS and quarantine state. Apple also warns about downloaded unsigned
software and recommends not overriding security for untrusted apps:
https://support.apple.com/en-us/102445

For a first run, users may Control/right-click the app, choose **Open**, confirm the
warning, and only then be able to open it normally. This user override is separate
from Apple notarization. Unsigned builds are not notarized and are not given a
Developer ID signature. Apple notarization and signing later require an Apple
Developer membership; see:
https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution

Distribution packaging should preserve quarantine metadata from GitHub/download,
provide a `sha256`, and document both the right-click flow and the privacy/security
warning. The zip should contain only `Ocy.app`; a DMG may be used later for clearer
drag-to-Applications UX.

**Homebrew:** unsigned cask acceptance is **unverified** from the documents available
for M0. Homebrew's current Acceptable Casks policy says macOS executable artifacts
must pass Homebrew's Gatekeeper checks and may not require Gatekeeper/SIP to be
disabled: https://docs.brew.sh/Acceptable-Casks. It does not in this policy clearly
answer whether every possible unsigned upstream app is accepted, and acceptance may
depend on audit/build details. Do not publish a cask claim until a maintainer or a
concrete acceptance/audit run verifies it.

## Phased plan

### M0: groundwork (this change)
- Keep shared Rust crates compiling on macOS.
- Add a native `macos-latest` build/test/clippy job.
- Record this plan and leave macOS behavior unchanged.

**Effort:** 1-2 engineering days, mostly CI and platform-build diagnosis.

### M1: read-only platform probes
- Add a macOS `damt-platform-mac` implementation or a separate macOS module.
- Implement idle, lock/sleep/wake, screen enumeration, and non-Windows smoke output.
- Add unit tests for pure mapping helpers and manual hardware test evidence.

**Effort:** 3-5 engineering days.

#### M1 status (implemented; hardware checks pending)

`rust/crates/damt-platform-mac` now exists, modelled on `damt-platform-linux` (`pure.rs` +
`macos/` OS modules + `tests/pure.rs`). The OS layer compiles only on
`target_os = "macos"`; the pure helpers (idle conversion, notification-name mapping,
`AppleInterfaceStyle`/reduce-motion parsing, display scale maths, frontmost-app mapping)
compile everywhere and their 22 unit tests run on Linux and in CI. Bindings are
`objc2` 0.6.5 / `objc2-app-kit` / `objc2-core-graphics` / `objc2-foundation` 0.3.2 +
`block2` 0.6.2, all pinned exactly, all MIT (see `docs/rust/LICENSES.md` §5). They are
pure-Rust `extern "C"` bindings: `cargo check -p damt-platform-mac -p ocy-app --target
aarch64-apple-darwin --locked` passes on Linux with **no Apple SDK and no
SDK-requiring build script**.

| Contract | Implementation | Honest limitation |
| --- | --- | --- |
| `IdleProvider` | `CGEventSourceSecondsSinceLastEventType` on the combined session state, any input event (`kCGAnyInputEventType`, defined in `pure` because the bindings skip it). | None known; verify units on hardware. |
| `SessionEventSource` | Lock/unlock: distributed notifications `com.apple.screenIsLocked` / `com.apple.screenIsUnlocked` (selector-based observer, `objc2::define_class!`). Sleep/wake: `NSWorkspaceWillSleepNotification` / `NSWorkspaceDidWakeNotification` (block observers). Observers are removed in `Drop`. | Delivery needs the running main run loop (the app's event loop); `start` must be called from the main thread. |
| `ActiveScreenProvider` | `CGGetActiveDisplayList` + `CGDisplayBounds` (points) + `CGDisplayPixelsWide/High` (pixels) → physical-pixel `ScreenRect` via the tested scale maths; `active_screen_bounds` returns the **main display**. | Foreground-window/cursor display tracking is M2. |
| `AppThemeProvider` / `TaskbarThemeProvider` | `AppleInterfaceStyle` via `NSUserDefaults.standardUserDefaults` (absent = light, `Dark` = dark, unknown value = unknown). The menu bar follows the same appearance. | Verify `Light` is actually written on newer macOS. |
| `MotionPreference` | `reduceMotion` in the `com.apple.universalaccess` domain via `NSUserDefaults(suiteNamed:)`. | The key name needs hardware verification. |
| `ForegroundProbe` | `NSWorkspace.frontmostApplication` → bundle id / app name / pid; `process_alive` via `NSRunningApplication`. Main-thread-guarded (answers `unknown` / `true` off-thread). | No `exe_path` (needs Accessibility); no Game Bar equivalent. |
| `FocusStateProvider` | **Unsupported** → `Unknown`. | No public `SHQueryUserNotificationState` equivalent without Accessibility; a `CGWindowList` heuristic is deferred to M2. |
| `AutostartService` | Documented stub (unsupported, no-op). | `SMAppService.mainApp` login items come in M2 (need a bundled `.app`). |
| `Notifier` | Unsupported (default). | `UNUserNotificationCenter` is M2. |

`ocy-app` composes `MacPlatform` behind `cfg(target_os = "macos")` exactly the way L1
wired Linux (platform services + session-event dispatch only; the tray and UI stay the
existing Slint path). Windows behaviour is unchanged (everything is target-gated).

#### Manual checks for someone with a Mac (M1 acceptance)

Run `cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test
--workspace --locked` on macOS first (the `macos` CI job is the compile gate), then
check on hardware:

1. **Idle:** with the app running, confirm the idle time grows while the user is away
   and resets on any keyboard/trackpad input (e.g. via a debug log or the `--smoke`
   output once wired). Check it also counts input in a full-screen app.
2. **Lock/unlock:** lock the screen (Ctrl-Cmd-Q); confirm a `Locked` event arrives on
   unlock order (event on lock, event on unlock), also with a screen-saver lock and a
   fast-user-switch away and back.
3. **Sleep/wake:** close the lid / Apple menu → Sleep, then wake; confirm
   `Suspending` then `Resumed` events.
4. **Screens:** on a Retina and an external scaled display, confirm the reported
   physical rect matches the real pixel size and the scale factor is 2.0 / fractional
   as expected; confirm the main display is the one with the menu bar.
5. **Theme:** switch System Settings → Appearance between Light and Dark; confirm the
   app theme flips (also check whether `AppleInterfaceStyle` reads `Light` explicitly
   on this macOS version — the code treats absent as light).
6. **Reduce motion:** toggle Accessibility → Display → Reduce motion; confirm
   `prefers_reduced_motion` follows it (this also verifies the `reduceMotion` key in
   `com.apple.universalaccess`).
7. **Frontmost app:** activate Safari/Terminal; confirm the probe reports their bundle
   id and pid from the main thread, and `process_alive` flips to `false` after the app
   quits.
8. **Cleanup:** quit the app; confirm no crash from observer removal in `Drop` (the
   `ObserverState` `Send`/`Sync` assertion is sound only if removal is safe from the
   dropping thread — if the drop happens off-main, verify or move removal to the main
   thread in M2).

### M2: native UI shell
- Implement menu-bar icon, non-activating break cards, full-screen-space behavior,
  multi-monitor placement, notifications, login item, and settings path.
- Use native NSPanel/objc2 integration only after M1 probes establish stable behavior.
- Add manual test matrix for screen lock, sleep/wake, displays, Spaces, Mission Control,
  and accessibility.

**Effort:** 3-6 engineering weeks for a first usable implementation, including UI
and native integration/debugging.

#### M2 status (implemented; compile-checked only — needs a Mac check)

Nobody in this workspace has a Mac, so **every runtime behaviour below is compile-checked
for `aarch64-apple-darwin` and unit-tested only in pure Rust on Linux; everything needs a
Mac check on real hardware**.

| Surface | Implementation | Compile-only vs tested | Honest limitation / check needed |
| --- | --- | --- | --- |
| Menu-bar icon (`NSStatusItem` + `NSMenu`) | `damt-platform-mac/src/macos/tray.rs` (`MacTrayHandle`) + `MacTraySink` in `ocy-app/src/main.rs`, behind `cfg(target_os = "macos")`. Mirrors `TrayPresenter` (`tray.rs`): all 10 items + 2 separators, visibility, enabled flags, checkmarks, tooltip, and `HoldUntilGameCloses`. Uses `menu.setAutoenablesItems(false)` and `MacTrayTarget` (`define_class!`). | Menu ordering, item visibility/enabled rules, action tags, and black+alpha `32x32` PNG template mask derivation are unit-tested on Linux (`damt-platform-mac/tests/pure.rs`). AppKit calls are **compile-checked only**. | Template icons are rendered at `16x16` pt (`32x32` px @2x) with `setTemplate(true)` — verify crispness and light/dark menu-bar tinting on Retina and 1x displays. |
| Break cards, dims, and edge frames (`NSWindow`) | `ocy-app/src/macos_window.rs`: pre-creation winit attributes (`with_active(false)`, `AlwaysOnTop`, borderless/transparent/no-shadow for overlays) + post-creation `RawWindowHandle::AppKit` → `NSView` → `NSWindow` setup (`setLevel`, `setCollectionBehavior(CanJoinAllSpaces \| FullScreenAuxiliary \| Stationary)`, `setIgnoresMouseEvents`, `orderFrontRegardless`). Levels come from `damt_platform_mac::pure::window_level_for_role`: `BreakCard` = `1001` (`NSScreenSaverWindowLevel + 1`), `Dim` = `1000` (`NSScreenSaverWindowLevel`), `EdgeFrame` = `3` (`NSFloatingWindowLevel`), `Normal` = `0` (`NSNormalWindowLevel`). | Window-level mapping and ordering (`BreakCard > Dim > Normal`, `EdgeFrame < BreakCard`), collection-behavior bitmask, `AppKit` handle extraction, and `MonitorInfo` mapping are unit-tested on Linux (`damt-platform-mac/tests/pure.rs` + `macos_window::tests`). AppKit calls are **compile-checked only**. | **Focus / `makeKeyAndOrderFront` / `canBecomeKeyWindow` check needed:** Slint's `card.show()` (`i-slint-backend-winit 1.19`) delegates to `winit::Window::set_visible(true)`, which in `winit 0.30` unconditionally calls `NSWindow::makeKeyAndOrderFront(None)` (ignoring `with_active(false)` because Slint strips `visible = false` during `ensure_window`), and `winit`'s `WinitWindow` subclass overrides `canBecomeKeyWindow` / `canBecomeMainWindow` to return `true` unconditionally (`NSWindow` has no non-subclassing setter, and `NonactivatingPanel` applies only to `NSPanel`). `apply_ns_window_policy` installs the role and calls `orderFrontRegardless()` with `ActivationPolicy::Accessory` and `with_activate_ignoring_other_apps(false)`. Verify on macOS whether `makeKeyAndOrderFront` / `canBecomeKeyWindow == true` ever steals focus or key status (both while another app is active and while Ocy's Settings/menu is active); if it does, a custom `WinitWindow`/`NSPanel` subclass or Objective-C swizzle will be required. Also visually confirm `BreakCard` (`1001`) renders cleanly in front of `Dim` (`1000`). |
| Active screen & multi-display placement | `MacActiveScreen` uses `NSEvent::mouseLocation` against `NSScreen::screens` (fallback `mainScreen` / `CGMainDisplayID`). `query_mac_monitors` reads `NSScreen::frame`, `visibleFrame` (excluding menu bar & Dock), and `backingScaleFactor` → DPI, flipping AppKit bottom-left coordinates to CG top-left physical pixels. `MacosOverlays` shows dims across all displays and edge frames on non-active displays. | AppKit-to-CG coordinate flip, Retina/mixed-DPI scaling, and point-in-screen selection are unit-tested on Linux. Live `NSScreen` queries are **compile-checked only**. | Verify mixed-DPI secondary display positioning (`winit` `set_outer_position` scales by the window's current `scale_factor`). |
| No Dock icon | `configure_event_loop_builder` sets `ActivationPolicy::Accessory` and `with_activate_ignoring_other_apps(false)` on winit's `EventLoopBuilder`, and `ensure_accessory_activation_policy()` sets `NSApplicationActivationPolicy::Accessory` at startup. | Policy choice helper tested on Linux; AppKit/winit calls are **compile-checked only**. | `LSUIElement = true` in `Info.plist` still comes with `.app` bundling in M3. |
| Autostart (`SMAppService.mainApp`) | `damt-platform-mac/src/macos/autostart.rs` (`MacAutostart`) links `ServiceManagement`, checks `NSProcessInfo.operatingSystemVersion().majorVersion >= 13` and `AnyClass::get(c"SMAppService")`, and calls `mainAppService` `status` / `registerAndReturnError:` / `unregisterAndReturnError:`. Reports `is_supported() == false` on macOS 12 and earlier. | Version check (`>= 13` + class presence) and `SMAppServiceStatus` mapping (`0..=3`) are unit-tested on Linux. Objective-C `msg_send!` calls are **compile-checked only**. | `SMAppService.mainApp` registration requires a bundled `.app` (M3); unbundled `cargo run` may surface a LaunchServices error in Settings. |
| Deferred to M2 follow-up / M3 | `MacFocus` (`CGWindowListCopyWindowInfo` fullscreen heuristic), `UNUserNotificationCenter` (`STATUS_NOTIFICATIONS_SHIPPED` is still `false` everywhere, and `UNUserNotificationCenter` requires a bundled `.app`), `.app` bundle + `LSUIElement` + single-instance second-launch activation. | — | See M3 and `M1` limitation notes. |

#### Manual test checklist for a Mac tester (M2 acceptance)

First run `cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked` natively on macOS, then launch Ocy (`cargo run -p ocy-app` and, once M3 exists, the bundled `Ocy.app`) and verify every item:

1. **No Dock icon & non-stealing startup:**
   - Launch Ocy while Terminal/Finder is active. Confirm **no Dock icon** appears, no app switcher (`Cmd-Tab`) entry appears, and Terminal/Finder keeps keyboard focus.
2. **Menu-bar icon & menu parity:**
   - Confirm the Ocy sprout template icon appears in the menu bar and adapts cleanly to both Light and Dark menu bars (and wallpaper tinting) on Retina and 1x displays.
   - Click the menu-bar icon: confirm all items match `TrayPresenter` (`Take short break now`, `Take long break now`, `Pause for 1 hour`, `Pause for 3 hours`, `Pause until resumed`, `Settings`, `Quit`, plus `Pause status` and `Resume` while paused).
   - While paused (1h / 3h / until resumed), confirm the pause status line shows as disabled/non-clickable, the matching pause option shows a checkmark, `Resume` appears and works, and the menu-bar icon switches to the paused/on-break/attention states.
   - With a known game running in front (not on the silent list), confirm `Hold breaks until I close this game` appears, works when clicked, and hides otherwise.
3. **Focus never stolen (short break, warning, long break):**
   - Type continuously in TextEdit or Terminal while triggering `--demo-break short`, `--demo-break warning`, and `--demo-break long` (or "Take short/long break now").
   - Confirm Ocy never becomes the active menu-bar application, your cursor/typing in TextEdit/Terminal is uninterrupted, and clicking buttons on the break card (`End break`, `Postpone`, `Start now`, `Not during this game`) works without needing to click twice to focus first.
   - **Check `makeKeyAndOrderFront` / `canBecomeKeyWindow`:** Slint's `card.show()` calls winit `makeKeyAndOrderFront(None)` and `WinitWindow::canBecomeKeyWindow` returns `true` (cannot be overridden without subclassing/swizzling). Trigger a break both while another app is active AND while Ocy's Settings window or menu-bar menu is open, and confirm whether the break card ever steals key focus or activates Ocy unexpectedly.
4. **Cards over fullscreen apps, Spaces, and Mission Control:**
   - Open a native macOS fullscreen app (e.g. Safari, Keynote, or a video player in its own fullscreen Space).
   - Trigger a short break, warning, and long break: confirm the card (and dim/edge frames) appear **over the fullscreen app** without switching Spaces back to the desktop.
   - Swipe between Spaces (`Ctrl-Left/Right`) and open Mission Control (`Ctrl-Up`): confirm cards stay stationary across Spaces (`canJoinAllSpaces | stationary | fullScreenAuxiliary`) and do not break Mission Control.
5. **Dims, click-through, and z-order:**
   - During a long break (or short break with fullscreen dim enabled), confirm the dim (`NSScreenSaverWindowLevel` = `1000`) covers the screen(s) including menu bar/Dock area, mouse clicks pass through the dim (`ignoresMouseEvents = true`), and the `LongBreakCard` / `ShortBreakCard` (`1001`, `NSScreenSaverWindowLevel + 1`) renders crisply **in front of** the dim (not darkened behind it), while `EdgeFrame` (`NSFloatingWindowLevel` = `3`) stays below cards.
   - Confirm `Cmd-Q` / `Cmd-W` / red close button cannot dismiss a break card while a break is active (`should_allow_close` guard).
6. **Multiple displays:**
   - Connect an external monitor (ideally mixed DPI, e.g. Retina built-in + 1x/1.5x external).
   - Move the mouse cursor to the built-in display and trigger a short break: confirm the card opens in the bottom-right work area (above Dock, below menu bar) of the built-in display, and the external display shows the click-through accent edge frame (clicks pass through the frame to windows underneath).
   - Move the mouse cursor to the external display and repeat: confirm the card opens on the external display and the edge frame moves to the built-in display.
   - Trigger a long break: confirm dims cover **both** displays and the long-break card is centered on the active display.
7. **Settings window & login item (`SMAppService.mainApp`):**
   - Click `Settings` in the menu-bar menu: confirm the Settings window opens, comes to the front, receives keyboard focus, and centers on the active display's work area.
   - On macOS 13+ (with a bundled `.app` once M3 is ready): toggle `Start with system`, confirm it appears under System Settings → General → Login Items, toggle off and confirm removal, and test reboot/re-login auto-launch. On unbundled `cargo run`, confirm any LaunchServices error is surfaced gracefully in the UI without crashing.
   - On macOS 12 or earlier (if available): confirm `Start with system` is hidden/unsupported (`AutostartService::is_supported() == false`).
8. **Sleep, wake, and screen lock:**
   - Lock screen (`Ctrl-Cmd-Q`), wait > `awayThreshold`, unlock: confirm away/absence crediting behaves properly and menu-bar countdown updates.
   - Sleep (lid close or Apple menu → Sleep) and wake: confirm `Suspending`/`Resumed` events update timers cleanly without stuck break overlays or crashes.
   - Quit from the menu-bar `Quit` item: confirm the status item disappears cleanly with no crash on exit.

### M3: unsigned community release
- Create `.app` bundling, zip/DMG, checksums, release instructions, and smoke scripts.
- Add architecture-specific builds and test on Intel and Apple Silicon Macs.
- Review Gatekeeper/permissions behavior. Obtain an Apple Developer account only if
  signing/notarization becomes a release requirement.

**Effort:** 1-3 engineering weeks; depends heavily on packaging and hardware coverage.

#### M3 status (CI packaging implemented; hardware acceptance pending)

The macOS packaging workflow builds `aarch64-apple-darwin` and `x86_64-apple-darwin`
release binaries, merges them with `lipo`, creates `Ocy.app`, ad-hoc signs it, and
publishes an unsigned universal zip plus SHA-256 checksum. The app uses the
`xyz.damt.ocy` bundle identifier and the menu-bar-only `LSUIElement` setting.

CI smoke runs on `macos-latest` and proves:

- short, warning, and long-break demo cards create on-screen Ocy windows;
- short and warning cards do not take frontmost-app focus;
- long-break dim and card layers follow the 1000/1001 separation;
- settings still launch in demo mode;
- settings is frontmost on macOS after `--demo-settings`;
- short and warning card margins are checked against a solid CI backdrop;
- window probe JSON and optional screenshots are retained as artifacts.

CI does not prove hardware behavior that requires a local Mac. A human tester still
needs to verify Retina and non-Retina placement, multiple monitors and Spaces,
Mission Control, fullscreen apps, mouse and keyboard behavior, menu-bar coexistence,
sleep/wake, lock/unlock, and actual Gatekeeper behavior. The first macOS run may
require an explicit trust override.

For an unsigned download:

1. Right-click `Ocy.app` in Finder, choose **Open**, then confirm the warning.
2. Alternatively, remove the downloaded quarantine attribute in Terminal:
   `xattr -dr com.apple.quarantine Ocy.app`.

The build sets `LSMinimumSystemVersion` to `11.0`. This follows the universal binary
floor: the Rust toolchain's Apple Silicon target requires macOS 11 or newer, while
the Intel target can run on older releases; a universal binary therefore cannot
advertise a lower floor than 11.0. The build also sets
`MACOSX_DEPLOYMENT_TARGET=11.0`.

#### M4 status (Settings activation and card margins)

Settings is the only macOS window role that activates the accessory app. Its pure
role decision is `Normal => true`; break cards, warning cards, dim overlays, and edge
frames remain `false`. On macOS 14+, `NSApplication.activate()` is used; older macOS
runs use `activateIgnoringOtherApps:YES`.

Break-card windows are transparent and use no native `NSWindow` shadow. Slint already
draws the card shadow inside the 392 DIP window, so native shadow was disabled to
avoid a second dark shadow/rectangle around the margins. The native window is also
non-opaque with a clear background. CI places a pink backdrop behind short and warning
probes and samples 3 points inside the top-left card window margin.

## Honest estimate

The remaining macOS work is not a compile-only task. A developer should expect
multiple native-framework integration loops and a manual hardware matrix. The
planning estimate for an initial community-usable macOS build is **4-9 engineering
weeks** after M0, with the widest uncertainty in full-screen panels, lock/sleep
events, and single-instance activation. Unsigned distribution itself is simpler,
but Gatekeeper/Homebrew behavior should remain explicitly unverified until tested.
