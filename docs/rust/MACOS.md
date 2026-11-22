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

### M3: unsigned community release
- Create `.app` bundling, zip/DMG, checksums, release instructions, and smoke scripts.
- Add architecture-specific builds and test on Intel and Apple Silicon Macs.
- Review Gatekeeper/permissions behavior. Obtain an Apple Developer account only if
  signing/notarization becomes a release requirement.

**Effort:** 1-3 engineering weeks; depends heavily on packaging and hardware coverage.

## Honest estimate

The remaining macOS work is not a compile-only task. A developer should expect
multiple native-framework integration loops and a manual hardware matrix. The
planning estimate for an initial community-usable macOS build is **4-9 engineering
weeks** after M0, with the widest uncertainty in full-screen panels, lock/sleep
events, and single-instance activation. Unsigned distribution itself is simpler,
but Gatekeeper/Homebrew behavior should remain explicitly unverified until tested.
