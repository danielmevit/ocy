//! macOS AppKit window role attributes, non-activating `NSWindow` configuration, activation
//! policy (`Accessory`, no Dock icon), and overlay windows (`MacosOverlays`) for break cards.
//!
//! Pure policy, level, collection-behavior, handle-extraction, and monitor-mapping helpers
//! compile on every target so their unit tests run on Linux and in CI; all `objc2-app-kit`
//! calls live in the `#[cfg(target_os = "macos")]` `macos_impl` submodule.

use std::ffi::c_void;

use damt_overlay::{MonitorId, MonitorInfo, Rgba};
use damt_platform::ScreenRect;
use raw_window_handle::RawWindowHandle;

/// `NSWindowCollectionBehaviorCanJoinAllSpaces` (`1 << 0`).
pub const NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES: usize = 1 << 0;
/// `NSWindowCollectionBehaviorStationary` (`1 << 4`).
pub const NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY: usize = 1 << 4;
/// `NSWindowCollectionBehaviorFullScreenAuxiliary` (`1 << 8`).
pub const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY: usize = 1 << 8;

/// `NSFloatingWindowLevel` (`3`): floats above normal windows while staying below cards/dims.
pub const NS_FLOATING_WINDOW_LEVEL: isize = 3;
/// `NSScreenSaverWindowLevel` (`1000`): covers desktop, menu bar, and Dock.
pub const NS_SCREEN_SAVER_WINDOW_LEVEL: isize = 1000;
/// Break card window level (`1001`, strictly above [`NS_SCREEN_SAVER_WINDOW_LEVEL`]).
///
/// AppKit orders windows **by level first**, then front-to-back within each level.
/// Dim overlays sit at [`NS_SCREEN_SAVER_WINDOW_LEVEL`] (`1000`), so every break card
/// (short, warning, and long) must sit strictly above them (`1001`) or the dim draws
/// over the card.
pub const BREAK_CARD_WINDOW_LEVEL: isize = NS_SCREEN_SAVER_WINDOW_LEVEL + 1;

/// Role of a macOS break or overlay window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacWindowKind {
    /// Short-break, warning, or long-break card window.
    BreakCard,
    /// Full-monitor dim backdrop window.
    Dim,
    /// Click-through border indicator window on non-active displays.
    EdgeFrame,
}

/// Target AppKit `NSWindowLevel` bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacWindowLevel {
    /// `NSFloatingWindowLevel` (`3`): floats above normal windows while staying below cards/dims.
    Floating,
    /// `NSScreenSaverWindowLevel` (`1000`): covers desktop, menu bar, and Dock.
    ScreenSaver,
    /// `NSScreenSaverWindowLevel + 1` (`1001`): strictly above dim overlays so break cards
    /// (short, warning, and long) always render in front of dims.
    AboveScreenSaver,
}

impl MacWindowLevel {
    /// Numeric `NSWindowLevel` (`NSInteger`) value defined by AppKit.
    #[must_use]
    pub const fn as_raw(self) -> isize {
        match self {
            Self::Floating => NS_FLOATING_WINDOW_LEVEL,
            Self::ScreenSaver => NS_SCREEN_SAVER_WINDOW_LEVEL,
            Self::AboveScreenSaver => BREAK_CARD_WINDOW_LEVEL,
        }
    }
}

/// Pure AppKit window policy computed for a [`MacWindowKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacWindowPolicy {
    pub level: MacWindowLevel,
    pub collection_behavior_bits: usize,
    pub ignores_mouse_events: bool,
    pub order_front_regardless: bool,
    pub never_activate: bool,
    pub has_shadow: bool,
}

/// Collection-behavior bitmask shared by break cards, dims, and edge frames so they stay
/// pinned across all Spaces and appear over native macOS fullscreen apps without participating
/// in Exposé/Spaces tiling.
#[must_use]
pub const fn overlay_collection_behavior_bits() -> usize {
    NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES
        | NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY
        | NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY
}

/// Pure policy mapping from [`MacWindowKind`] to [`MacWindowPolicy`].
#[must_use]
pub const fn window_policy_for_kind(kind: MacWindowKind) -> MacWindowPolicy {
    let collection_behavior_bits = overlay_collection_behavior_bits();
    match kind {
        MacWindowKind::BreakCard => MacWindowPolicy {
            level: MacWindowLevel::AboveScreenSaver,
            collection_behavior_bits,
            ignores_mouse_events: false,
            order_front_regardless: true,
            never_activate: true,
            // The window is exactly the card on macOS, so AppKit draws the card shadow.
            has_shadow: true,
        },
        MacWindowKind::Dim => MacWindowPolicy {
            level: MacWindowLevel::ScreenSaver,
            collection_behavior_bits,
            ignores_mouse_events: true,
            order_front_regardless: true,
            never_activate: true,
            has_shadow: false,
        },
        MacWindowKind::EdgeFrame => MacWindowPolicy {
            level: MacWindowLevel::Floating,
            collection_behavior_bits,
            ignores_mouse_events: true,
            order_front_regardless: true,
            never_activate: true,
            has_shadow: false,
        },
    }
}

/// Target `NSApplicationActivationPolicy` for Ocy at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacActivationPolicyChoice {
    Regular,
    Accessory,
    Prohibited,
}

/// Ocy runs as a menu-bar accessory with no Dock icon (`LSUIElement` is added in M3 packaging).
#[must_use]
pub const fn startup_activation_policy() -> MacActivationPolicyChoice {
    MacActivationPolicyChoice::Accessory
}

/// Extracts the raw `NSView *` pointer from a `raw-window-handle` AppKit handle.
#[must_use]
pub fn extract_ns_view_ptr(raw: RawWindowHandle) -> Option<*mut c_void> {
    match raw {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
        _ => None,
    }
}

/// Converts macOS monitor geometry fields into [`damt_overlay::MonitorInfo`].
#[must_use]
pub const fn monitor_info_from_mac_parts(
    id: u32,
    rect: ScreenRect,
    work_area: ScreenRect,
    dpi: u32,
    primary: bool,
) -> MonitorInfo {
    MonitorInfo {
        id: MonitorId::new(id as isize),
        rect,
        work_area,
        dpi,
        primary,
    }
}

/// Transparent margin (logical px) the card components keep around the card for the Slint drop
/// shadow on Windows and Linux (`card-inset` default in `ocy.slint`).
pub const DEFAULT_CARD_INSET_DIP: f64 = 16.0;
/// Width (logical px) of the short-break and warning card itself, without margins.
pub const SHORT_CARD_WIDTH_DIP: f64 = 360.0;
/// Width (logical px) of the long-break card itself, without margins.
pub const LONG_CARD_WIDTH_DIP: f64 = 480.0;
/// Corner radius (logical px) of `Card` (`Theme.radius-card`); macOS rounds the window natively.
pub const CARD_CORNER_RADIUS_DIP: f64 = 8.0;

/// Card inset for a platform. macOS gets `0`: Slint's software renderer on macOS has no per-pixel
/// alpha (softbuffer's CoreGraphics backend uses `kCGImageAlphaNoneSkipFirst`), so a transparent
/// margin would paint black. The window is exactly the card there.
#[must_use]
pub const fn card_inset_dip(macos: bool) -> f64 {
    if macos { 0.0 } else { DEFAULT_CARD_INSET_DIP }
}

/// Logical window width for the short-break and warning cards (392 with margins, 360 without).
#[must_use]
pub const fn short_card_window_width_dip(macos: bool) -> f64 {
    SHORT_CARD_WIDTH_DIP + 2.0 * card_inset_dip(macos)
}

/// Extra corner margin (logical px) the host adds when placing a short or warning card, so the
/// card itself lands at the same spot whether or not the window carries the transparent margin.
#[must_use]
pub const fn card_corner_margin_dip(macos: bool) -> f64 {
    DEFAULT_CARD_INSET_DIP - card_inset_dip(macos)
}

/// macOS long-break card window size: exactly the 480 px card, clamped to the screen (logical px).
/// A non-positive screen extent means "unknown" and leaves that axis unclamped.
#[must_use]
pub fn mac_long_card_window_size(
    content_height_dip: f64,
    screen_width_dip: f64,
    screen_height_dip: f64,
) -> (f64, f64) {
    let h = content_height_dip.max(100.0);
    let w = if screen_width_dip > 0.0 {
        LONG_CARD_WIDTH_DIP.min(screen_width_dip)
    } else {
        LONG_CARD_WIDTH_DIP
    };
    let h = if screen_height_dip > 0.0 {
        h.min(screen_height_dip)
    } else {
        h
    };
    (w, h)
}

/// Splits a translucent colour into its opaque RGB and an `NSWindow.alphaValue` in `0.0..=1.0`.
///
/// macOS paints dims and edge strips opaque and lets the window server apply the alpha.
#[must_use]
pub fn split_alpha(color: Rgba) -> (Rgba, f64) {
    (
        Rgba {
            r: color.r,
            g: color.g,
            b: color.b,
            a: 255,
        },
        f64::from(color.a) / 255.0,
    )
}

/// Physical edge-frame thickness for a monitor DPI (96 DPI = 1x), at least 1 px.
#[must_use]
pub fn edge_strip_thickness_px(thickness_dip: u32, dpi: u32) -> i32 {
    let dpi = if dpi == 0 { 96 } else { dpi };
    let px = (f64::from(thickness_dip) * f64::from(dpi) / 96.0).round();
    // `px` is a small positive whole number; clamp keeps the cast in range.
    #[allow(clippy::cast_possible_truncation)]
    let px = px.clamp(1.0, 10_000.0) as i32;
    px
}

/// The four edge-frame strips (top, bottom, left, right) for `monitor` with `thickness` physical
/// pixels. Top and bottom span the full width; left and right fit between them, so no pixel is
/// covered twice (a doubled corner would look darker at partial alpha). Empty when the monitor
/// is too small for the frame.
#[must_use]
pub fn edge_strip_rects(monitor: ScreenRect, thickness: i32) -> Vec<ScreenRect> {
    if thickness <= 0 || monitor.width <= 0 || monitor.height <= 0 {
        return Vec::new();
    }
    let t = thickness.min(monitor.width).min(monitor.height / 2);
    if t <= 0 {
        return Vec::new();
    }
    let mut rects = vec![
        ScreenRect {
            x: monitor.x,
            y: monitor.y,
            width: monitor.width,
            height: t,
        },
        ScreenRect {
            x: monitor.x,
            y: monitor.y + monitor.height - t,
            width: monitor.width,
            height: t,
        },
    ];
    let side_h = monitor.height - 2 * t;
    let side_w = t.min(monitor.width / 2);
    if side_h > 0 && side_w > 0 {
        rects.push(ScreenRect {
            x: monitor.x,
            y: monitor.y + t,
            width: side_w,
            height: side_h,
        });
        rects.push(ScreenRect {
            x: monitor.x + monitor.width - side_w,
            y: monitor.y + t,
            width: side_w,
            height: side_h,
        });
    }
    rects
}

#[cfg(target_os = "macos")]
mod macos_impl {
    use std::cell::Cell;
    use std::fmt;
    use std::rc::Rc;

    use damt_overlay::{MonitorInfo, Rgba};
    use damt_platform::ScreenRect;
    use damt_platform_mac::pure::{
        MacWindowRole, window_level_for_role, window_role_should_activate,
    };
    use damt_platform_mac::{MacMonitorInfo, query_mac_monitors};
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{Message, class, msg_send};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSView, NSWindow, NSWindowCollectionBehavior,
    };
    use raw_window_handle::HasWindowHandle;
    use slint::winit_030::winit::event::WindowEvent;
    use slint::winit_030::winit::platform::macos::{
        ActivationPolicy, EventLoopBuilderExtMacOS, WindowAttributesExtMacOS,
    };
    use slint::winit_030::winit::window::{WindowAttributes, WindowLevel};
    use slint::winit_030::{EventLoopBuilder, EventResult, WinitWindowAccessor};
    use slint::{ComponentHandle, PhysicalPosition, PhysicalSize, Timer};

    use super::{
        CARD_CORNER_RADIUS_DIP, MacActivationPolicyChoice, MacWindowKind, edge_strip_rects,
        edge_strip_thickness_px, extract_ns_view_ptr, monitor_info_from_mac_parts, split_alpha,
        startup_activation_policy, window_policy_for_kind,
    };
    use crate::win32_window::{WindowRole, current_window_role};

    #[allow(
        clippy::all,
        clippy::pedantic,
        clippy::unwrap_used,
        clippy::expect_used,
        missing_debug_implementations
    )]
    mod overlay_ui {
        slint::slint! {
            #[edition = "1.19"]
            // A solid, opaque window: the dim shade or one edge-frame strip. macOS's software
            // renderer drops per-pixel alpha, so the translucency comes from
            // `NSWindow.alphaValue` instead (see `split_alpha`).
            export component MacDimOverlay inherits Window {
                in property <color> dim-color: #000000;
                no-frame: true;
                always-on-top: true;
                background: root.dim-color;
            }
        }
    }

    use overlay_ui::MacDimOverlay;

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    enum MacosOverlayRole {
        #[default]
        None,
        Dim,
        EdgeFrame,
    }

    thread_local! {
        static NEXT_MACOS_OVERLAY_ROLE: Cell<MacosOverlayRole> =
            const { Cell::new(MacosOverlayRole::None) };
    }

    struct MacosOverlayRoleGuard {
        previous: MacosOverlayRole,
    }

    impl MacosOverlayRoleGuard {
        fn new(role: MacosOverlayRole) -> Self {
            let previous = NEXT_MACOS_OVERLAY_ROLE.with(|cell| cell.replace(role));
            Self { previous }
        }
    }

    impl Drop for MacosOverlayRoleGuard {
        fn drop(&mut self) {
            let prev = self.previous;
            NEXT_MACOS_OVERLAY_ROLE.with(|cell| cell.set(prev));
        }
    }

    /// Configures the winit macOS event-loop builder for a menu-bar accessory (no Dock icon,
    /// no focus steal at startup).
    pub fn configure_event_loop_builder(builder: &mut EventLoopBuilder) {
        let policy = match startup_activation_policy() {
            MacActivationPolicyChoice::Regular => ActivationPolicy::Regular,
            MacActivationPolicyChoice::Accessory => ActivationPolicy::Accessory,
            MacActivationPolicyChoice::Prohibited => ActivationPolicy::Prohibited,
        };
        builder.with_activation_policy(policy);
        builder.with_activate_ignoring_other_apps(false);
    }

    /// Explicitly sets `NSApplication.activationPolicy = .accessory` on the main thread so
    /// unbundled runs have no Dock icon even before `LSUIElement` is bundled in M3.
    pub fn ensure_accessory_activation_policy() -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let app = NSApplication::sharedApplication(mtm);
        let policy = match startup_activation_policy() {
            MacActivationPolicyChoice::Regular => NSApplicationActivationPolicy::Regular,
            MacActivationPolicyChoice::Accessory => NSApplicationActivationPolicy::Accessory,
            MacActivationPolicyChoice::Prohibited => NSApplicationActivationPolicy::Prohibited,
        };
        app.setActivationPolicy(policy)
    }

    /// Applies macOS-specific winit [`WindowAttributes`] for the active role before window creation.
    #[must_use]
    pub fn apply_window_attributes_for_role(attrs: WindowAttributes) -> WindowAttributes {
        match NEXT_MACOS_OVERLAY_ROLE.with(Cell::get) {
            MacosOverlayRole::Dim => {
                let policy = window_policy_for_kind(MacWindowKind::Dim);
                attrs
                    .with_active(!policy.never_activate)
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_has_shadow(policy.has_shadow)
                    .with_window_level(WindowLevel::AlwaysOnTop)
            }
            MacosOverlayRole::EdgeFrame => {
                let policy = window_policy_for_kind(MacWindowKind::EdgeFrame);
                attrs
                    .with_active(!policy.never_activate)
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_has_shadow(policy.has_shadow)
                    .with_window_level(WindowLevel::AlwaysOnTop)
            }
            MacosOverlayRole::None => match current_window_role() {
                WindowRole::Normal => attrs,
                WindowRole::BreakCard => {
                    let policy = window_policy_for_kind(MacWindowKind::BreakCard);
                    attrs
                        .with_active(!policy.never_activate)
                        .with_transparent(true)
                        .with_has_shadow(policy.has_shadow)
                        .with_window_level(WindowLevel::AlwaysOnTop)
                }
            },
        }
    }

    /// Applies AppKit `NSWindow` level, collection behavior, mouse pass-through, and
    /// non-activating `orderFrontRegardless` to a Slint window.
    pub fn apply_ns_window_policy(window: &slint::Window, kind: MacWindowKind) -> bool {
        apply_ns_window_policy_with_alpha(window, kind, None)
    }

    /// [`apply_ns_window_policy`] plus `NSWindow.alphaValue` when `alpha` is set (dims, strips).
    fn apply_ns_window_policy_with_alpha(
        window: &slint::Window,
        kind: MacWindowKind,
        alpha: Option<f64>,
    ) -> bool {
        let Some(_mtm) = MainThreadMarker::new() else {
            return false;
        };
        let policy = window_policy_for_kind(kind);
        window
            .with_winit_window(|winit_win| {
                if policy.ignores_mouse_events {
                    let _ = winit_win.set_cursor_hittest(false);
                }
                let Ok(handle) = winit_win.window_handle() else {
                    return false;
                };
                let Some(ns_view_ptr) = extract_ns_view_ptr(handle.as_raw()) else {
                    return false;
                };
                if ns_view_ptr.is_null() {
                    return false;
                }
                // SAFETY: `ns_view_ptr` comes from winit's `RawWindowHandle::AppKit` on the
                // main thread while `winit_win` is alive; it points to a live `NSView` owned
                // by the `NSWindow`.
                let ns_view: &NSView = unsafe { &*ns_view_ptr.cast::<NSView>() };
                let Some(ns_window) = ns_view.window() else {
                    return false;
                };
                apply_policy_to_ns_window(&ns_window, ns_view, kind, policy);
                if let Some(alpha) = alpha {
                    ns_window.setAlphaValue(alpha.clamp(0.0, 1.0));
                }
                true
            })
            .unwrap_or(false)
    }

    const fn mac_role_for_kind(kind: MacWindowKind) -> MacWindowRole {
        match kind {
            MacWindowKind::BreakCard => MacWindowRole::BreakCard,
            MacWindowKind::Dim => MacWindowRole::Dim,
            MacWindowKind::EdgeFrame => MacWindowRole::EdgeFrame,
        }
    }

    fn apply_policy_to_ns_window(
        ns_window: &NSWindow,
        ns_view: &NSView,
        kind: MacWindowKind,
        policy: super::MacWindowPolicy,
    ) {
        let level = window_level_for_role(mac_role_for_kind(kind));
        debug_assert_eq!(level, policy.level.as_raw());
        ns_window.setLevel(level);
        ns_window.setCollectionBehavior(NSWindowCollectionBehavior::from_bits_retain(
            policy.collection_behavior_bits,
        ));
        ns_window.setIgnoresMouseEvents(policy.ignores_mouse_events);
        ns_window.setHasShadow(policy.has_shadow);
        ns_window.setOpaque(false);
        set_clear_background(ns_window);
        if kind == MacWindowKind::BreakCard {
            round_content_view_corners(ns_view);
        }
        if policy.order_front_regardless {
            ns_window.orderFrontRegardless();
        }
        if policy.has_shadow {
            schedule_invalidate_shadow(ns_window);
        }
    }

    /// Sets the window background to `[NSColor clearColor]` so the masked-off card corners show
    /// what is behind the window (`nil` would restore the default grey window background).
    fn set_clear_background(ns_window: &NSWindow) {
        // SAFETY: `+[NSColor clearColor]` takes no arguments and returns a non-nil autoreleased
        // `NSColor *`; `msg_send!` retains it into `Retained<AnyObject>`.
        let clear: Option<Retained<AnyObject>> = unsafe { msg_send![class!(NSColor), clearColor] };
        if let Some(clear) = clear {
            // SAFETY: `-[NSWindow setBackgroundColor:]` takes one `NSColor *`; `clear` is a live
            // `NSColor` and we are on the main thread.
            let _: () = unsafe { msg_send![ns_window, setBackgroundColor: &*clear] };
        }
    }

    /// Makes the content view layer-backed and rounds and clips its layer to the card radius.
    ///
    /// softbuffer draws into a sublayer of this root layer, so `masksToBounds` clips the card
    /// pixels to the rounded shape and the window shadow follows it.
    fn round_content_view_corners(ns_view: &NSView) {
        // SAFETY: `-[NSView setWantsLayer:]` takes one `BOOL`; `ns_view` is a live view on the
        // main thread (softbuffer makes the same call on this view).
        let _: () = unsafe { msg_send![ns_view, setWantsLayer: Bool::YES] };
        // SAFETY: `-[NSView layer]` takes no arguments and returns a nullable `CALayer *`.
        let layer: Option<Retained<AnyObject>> = unsafe { msg_send![ns_view, layer] };
        let Some(layer) = layer else {
            return;
        };
        // SAFETY: `-[CALayer setCornerRadius:]` takes one `CGFloat` (`f64` on 64-bit macOS).
        let _: () = unsafe { msg_send![&*layer, setCornerRadius: CARD_CORNER_RADIUS_DIP] };
        // SAFETY: `-[CALayer setMasksToBounds:]` takes one `BOOL`.
        let _: () = unsafe { msg_send![&*layer, setMasksToBounds: Bool::YES] };
    }

    /// Recomputes the window shadow from the painted (rounded) content once frames have been
    /// presented; the shadow AppKit computes before the first paint has the wrong shape.
    fn schedule_invalidate_shadow(ns_window: &NSWindow) {
        for delay_ms in [60_u64, 400] {
            let window: Retained<NSWindow> = ns_window.retain();
            Timer::single_shot(std::time::Duration::from_millis(delay_ms), move || {
                window.invalidateShadow();
            });
        }
    }

    /// Activates the application and makes a Settings window key on macOS.
    pub fn activate_settings_window(window: &slint::Window) -> bool {
        let Some(_mtm) = MainThreadMarker::new() else {
            return false;
        };
        if !window_role_should_activate(MacWindowRole::Normal) {
            return false;
        }

        let app = NSApplication::sharedApplication(_mtm);
        if objc2::available!(macos = 14.0) {
            app.activate();
        } else {
            // SAFETY: `activateIgnoringOtherApps` is the macOS 11-13 fallback for
            // activating this accessory app when Settings is explicitly opened.
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }

        window
            .with_winit_window(|winit_win| {
                let Ok(handle) = winit_win.window_handle() else {
                    return false;
                };
                let Some(ns_view_ptr) = extract_ns_view_ptr(handle.as_raw()) else {
                    return false;
                };
                if ns_view_ptr.is_null() {
                    return false;
                }
                // SAFETY: `ns_view_ptr` comes from winit's live `NSView` on the main
                // thread and belongs to the Settings window owned by `winit_win`.
                let ns_view: &NSView = unsafe { &*ns_view_ptr.cast::<NSView>() };
                let Some(ns_window) = ns_view.window() else {
                    return false;
                };
                ns_window.makeKeyAndOrderFront(None);
                ns_window.makeMainWindow();
                true
            })
            .unwrap_or(false)
    }

    /// Activates Ocy as an explicit result of opening Settings, then promotes the
    /// application's current key window for keyboard focus.
    pub fn activate_macos_application() -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        if !window_role_should_activate(MacWindowRole::Normal) {
            return false;
        }

        let app = NSApplication::sharedApplication(mtm);
        if objc2::available!(macos = 14.0) {
            app.activate();
        } else {
            // SAFETY: `activateIgnoringOtherApps` is the macOS 11-13 fallback for
            // activating this accessory app when Settings is explicitly opened.
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }

        if let Some(window) = app.keyWindow() {
            window.makeKeyAndOrderFront(None);
            window.makeMainWindow();
        }
        true
    }

    /// Configures a break/warning card window on macOS so it floats over fullscreen Spaces
    /// without activating Ocy.
    pub fn install_break_window_role(window: &slint::Window) -> bool {
        apply_ns_window_policy(window, MacWindowKind::BreakCard)
    }

    /// Converts a [`MacMonitorInfo`] into [`MonitorInfo`].
    #[must_use]
    pub const fn monitor_info_from_mac(info: &MacMonitorInfo) -> MonitorInfo {
        monitor_info_from_mac_parts(info.id, info.rect, info.work_area, info.dpi, info.primary)
    }

    /// Enumerates physical macOS monitors with work areas and DPI.
    #[must_use]
    pub fn enumerate_monitors() -> Vec<MonitorInfo> {
        query_mac_monitors()
            .iter()
            .map(monitor_info_from_mac)
            .collect()
    }

    /// Manages macOS-native break overlays: full-monitor dims, and click-through edge frames
    /// drawn as four opaque strip windows per monitor (translucency via `NSWindow.alphaValue`).
    #[derive(Default)]
    pub struct MacosOverlays {
        dims: Vec<MacDimOverlay>,
        edge_frames: Vec<MacDimOverlay>,
    }

    impl fmt::Debug for MacosOverlays {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("MacosOverlays")
                .field("dims", &self.dims.len())
                .field("edge_frames", &self.edge_frames.len())
                .finish()
        }
    }

    impl Drop for MacosOverlays {
        fn drop(&mut self) {
            self.close();
        }
    }

    impl MacosOverlays {
        /// Creates an empty [`MacosOverlays`] container.
        #[must_use]
        pub const fn new() -> Self {
            Self {
                dims: Vec::new(),
                edge_frames: Vec::new(),
            }
        }

        /// Number of currently live dim + edge-frame overlay windows.
        #[must_use]
        pub fn len(&self) -> usize {
            self.dims.len() + self.edge_frames.len()
        }

        /// Returns `true` when no overlay windows are open.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }

        /// Closes and drops all active dim and edge-frame overlay windows.
        pub fn close(&mut self) {
            for dim in self.dims.drain(..) {
                let _ = dim.hide();
            }
            for frame in self.edge_frames.drain(..) {
                let _ = frame.hide();
            }
        }

        /// Updates the accent border colour on any active edge frames.
        pub fn set_edge_frame_color(&mut self, color: Rgba) {
            let (opaque, alpha) = split_alpha(color);
            let slint_col = slint_color(opaque);
            for strip in &self.edge_frames {
                strip.set_dim_color(slint_col);
                let _ = apply_ns_window_policy_with_alpha(
                    strip.window(),
                    MacWindowKind::EdgeFrame,
                    Some(alpha),
                );
            }
        }

        /// Shows non-activating, click-through full-monitor dim overlays across all displays.
        pub fn show_dims(&mut self, color: Rgba) {
            self.close();
            let monitors = enumerate_monitors();
            let (opaque, alpha) = split_alpha(color);
            let slint_col = slint_color(opaque);
            for monitor in monitors {
                if monitor.rect.width <= 0 || monitor.rect.height <= 0 {
                    continue;
                }
                let _guard = MacosOverlayRoleGuard::new(MacosOverlayRole::Dim);
                let Ok(dim) = MacDimOverlay::new() else {
                    continue;
                };
                dim.set_dim_color(slint_col);
                configure_overlay_window(&dim, monitor.rect, MacWindowKind::Dim, alpha);
                if dim.show().is_ok() {
                    apply_ns_window_policy_with_alpha(
                        dim.window(),
                        MacWindowKind::Dim,
                        Some(alpha),
                    );
                    self.dims.push(dim);
                }
            }
        }

        /// Shows click-through accent edge frames on `monitors` (non-active displays).
        pub fn show_edge_frames(
            &mut self,
            monitors: &[MonitorInfo],
            color: Rgba,
            thickness_dip: u32,
        ) {
            for frame in self.edge_frames.drain(..) {
                let _ = frame.hide();
            }
            if monitors.is_empty() {
                return;
            }
            let (opaque, alpha) = split_alpha(color);
            let slint_col = slint_color(opaque);
            for monitor in monitors {
                let thickness = edge_strip_thickness_px(thickness_dip, monitor.dpi);
                for rect in edge_strip_rects(monitor.rect, thickness) {
                    let _guard = MacosOverlayRoleGuard::new(MacosOverlayRole::EdgeFrame);
                    let Ok(strip) = MacDimOverlay::new() else {
                        continue;
                    };
                    strip.set_dim_color(slint_col);
                    configure_overlay_window(&strip, rect, MacWindowKind::EdgeFrame, alpha);
                    if strip.show().is_ok() {
                        apply_ns_window_policy_with_alpha(
                            strip.window(),
                            MacWindowKind::EdgeFrame,
                            Some(alpha),
                        );
                        self.edge_frames.push(strip);
                    }
                }
            }
        }
    }

    fn slint_color(color: Rgba) -> slint::Color {
        slint::Color::from_argb_u8(color.a, color.r, color.g, color.b)
    }

    fn configure_overlay_window<C: ComponentHandle>(
        component: &C,
        rect: ScreenRect,
        kind: MacWindowKind,
        alpha: f64,
    ) {
        let win = component.window();
        win.set_position(PhysicalPosition::new(rect.x, rect.y));
        win.set_size(PhysicalSize::new(
            u32::try_from(rect.width).unwrap_or(1),
            u32::try_from(rect.height).unwrap_or(1),
        ));

        let applied = Rc::new(Cell::new(false));
        win.on_winit_window_event(move |w, event| {
            if (!applied.get() || matches!(event, WindowEvent::RedrawRequested))
                && !applied.get()
                && apply_ns_window_policy_with_alpha(w, kind, Some(alpha))
            {
                applied.set(true);
            }
            EventResult::Propagate
        });
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::{
    MacosOverlays, activate_macos_application, activate_settings_window, apply_ns_window_policy,
    apply_window_attributes_for_role, configure_event_loop_builder,
    ensure_accessory_activation_policy, enumerate_monitors, install_break_window_role,
    monitor_info_from_mac,
};

#[cfg(test)]
mod tests {
    use std::ptr::NonNull;

    use damt_overlay::{MonitorId, MonitorInfo};
    use damt_platform::ScreenRect;
    use raw_window_handle::{AppKitWindowHandle, RawWindowHandle, XlibWindowHandle};

    use super::{
        BREAK_CARD_WINDOW_LEVEL, MacActivationPolicyChoice, MacWindowKind, MacWindowLevel,
        NS_FLOATING_WINDOW_LEVEL, NS_SCREEN_SAVER_WINDOW_LEVEL,
        NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES,
        NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY,
        NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY, extract_ns_view_ptr, monitor_info_from_mac_parts,
        overlay_collection_behavior_bits, startup_activation_policy, window_policy_for_kind,
    };
    use super::{
        card_corner_margin_dip, card_inset_dip, edge_strip_rects, edge_strip_thickness_px,
        mac_long_card_window_size, short_card_window_width_dip, split_alpha,
    };
    use damt_overlay::Rgba;

    const fn rect(x: i32, y: i32, width: i32, height: i32) -> ScreenRect {
        ScreenRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn card_inset_is_zero_only_on_macos_and_keeps_the_card_in_place() {
        assert!((card_inset_dip(false) - 16.0).abs() < f64::EPSILON);
        assert!(card_inset_dip(true).abs() < f64::EPSILON);
        assert!((short_card_window_width_dip(false) - 392.0).abs() < f64::EPSILON);
        assert!((short_card_window_width_dip(true) - 360.0).abs() < f64::EPSILON);
        assert!(card_corner_margin_dip(false).abs() < f64::EPSILON);
        assert!((card_corner_margin_dip(true) - 16.0).abs() < f64::EPSILON);
        // The card's right edge sits 16 px from the work-area corner either way.
        for mac in [false, true] {
            let card_right_gap = card_corner_margin_dip(mac) + card_inset_dip(mac);
            assert!((card_right_gap - 16.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn mac_long_card_window_is_the_card_clamped_to_the_screen() {
        assert_eq!(
            mac_long_card_window_size(319.0, 1024.0, 768.0),
            (480.0, 319.0)
        );
        assert_eq!(mac_long_card_window_size(50.0, 0.0, 0.0), (480.0, 100.0));
        assert_eq!(
            mac_long_card_window_size(900.0, 400.0, 600.0),
            (400.0, 600.0)
        );
    }

    #[test]
    fn split_alpha_keeps_rgb_opaque_and_moves_alpha_to_the_window() {
        let (opaque, alpha) = split_alpha(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0xc7,
        });
        assert_eq!(
            opaque,
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert!((alpha - 199.0 / 255.0).abs() < 1e-9);
        let (opaque, alpha) = split_alpha(Rgba {
            r: 0x2f,
            g: 0x7d,
            b: 0x62,
            a: 0x8c,
        });
        assert_eq!(
            (opaque.r, opaque.g, opaque.b, opaque.a),
            (0x2f, 0x7d, 0x62, 255)
        );
        assert!((alpha - 140.0 / 255.0).abs() < 1e-9);
        assert!(
            (split_alpha(Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 255
            })
            .1 - 1.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn edge_strip_thickness_scales_with_dpi() {
        assert_eq!(edge_strip_thickness_px(3, 96), 3);
        assert_eq!(edge_strip_thickness_px(3, 192), 6);
        assert_eq!(edge_strip_thickness_px(3, 0), 3);
        assert_eq!(edge_strip_thickness_px(0, 96), 1);
    }

    #[test]
    fn edge_strips_cover_the_border_once_without_overlap() {
        let monitor = rect(1024, -100, 1920, 1080);
        let strips = edge_strip_rects(monitor, 6);
        assert_eq!(
            strips,
            vec![
                rect(1024, -100, 1920, 6),
                rect(1024, 974, 1920, 6),
                rect(1024, -94, 6, 1068),
                rect(2938, -94, 6, 1068),
            ]
        );
        let area: i64 = strips
            .iter()
            .map(|r| i64::from(r.width) * i64::from(r.height))
            .sum();
        assert_eq!(area, 2 * 1920 * 6 + 2 * 6 * 1068);
        for (i, a) in strips.iter().enumerate() {
            for b in &strips[i + 1..] {
                let overlap_x = a.x < b.x + b.width && b.x < a.x + a.width;
                let overlap_y = a.y < b.y + b.height && b.y < a.y + a.height;
                assert!(!(overlap_x && overlap_y), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn edge_strips_degenerate_inputs() {
        assert!(edge_strip_rects(rect(0, 0, 0, 100), 3).is_empty());
        assert!(edge_strip_rects(rect(0, 0, 100, 100), 0).is_empty());
        assert!(edge_strip_rects(rect(0, 0, 100, 1), 3).is_empty());
        // Too short for side strips: top and bottom only.
        assert_eq!(edge_strip_rects(rect(0, 0, 100, 6), 3).len(), 2);
    }

    #[test]
    fn window_level_values_match_appkit_constants() {
        assert_eq!(NS_FLOATING_WINDOW_LEVEL, 3);
        assert_eq!(NS_SCREEN_SAVER_WINDOW_LEVEL, 1000);
        assert_eq!(BREAK_CARD_WINDOW_LEVEL, 1001);
        assert_eq!(MacWindowLevel::Floating.as_raw(), NS_FLOATING_WINDOW_LEVEL);
        assert_eq!(
            MacWindowLevel::ScreenSaver.as_raw(),
            NS_SCREEN_SAVER_WINDOW_LEVEL
        );
        assert_eq!(
            MacWindowLevel::AboveScreenSaver.as_raw(),
            BREAK_CARD_WINDOW_LEVEL
        );
    }

    #[test]
    fn window_policy_keeps_break_cards_above_dims_and_edge_frames() {
        let expected_behavior = NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES
            | NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY
            | NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY;
        assert_eq!(overlay_collection_behavior_bits(), expected_behavior);

        let card = window_policy_for_kind(MacWindowKind::BreakCard);
        assert_eq!(card.level, MacWindowLevel::AboveScreenSaver);
        assert_eq!(card.collection_behavior_bits, expected_behavior);
        assert!(!card.ignores_mouse_events);
        assert!(card.order_front_regardless);
        assert!(card.never_activate);
        assert!(card.has_shadow);

        let dim = window_policy_for_kind(MacWindowKind::Dim);
        assert_eq!(dim.level, MacWindowLevel::ScreenSaver);
        assert_eq!(dim.collection_behavior_bits, expected_behavior);
        assert!(dim.ignores_mouse_events);
        assert!(dim.order_front_regardless);
        assert!(dim.never_activate);
        assert!(!dim.has_shadow);

        let frame = window_policy_for_kind(MacWindowKind::EdgeFrame);
        assert_eq!(frame.level, MacWindowLevel::Floating);
        assert_eq!(frame.collection_behavior_bits, expected_behavior);
        assert!(frame.ignores_mouse_events);
        assert!(frame.order_front_regardless);
        assert!(frame.never_activate);
        assert!(!frame.has_shadow);

        assert!(card.level.as_raw() > dim.level.as_raw());
        assert!(frame.level.as_raw() < card.level.as_raw());
    }

    #[test]
    fn startup_policy_hides_dock_icon_via_accessory() {
        assert_eq!(
            startup_activation_policy(),
            MacActivationPolicyChoice::Accessory
        );
    }

    #[test]
    fn extract_ns_view_ptr_accepts_appkit_and_rejects_other_handles() {
        let ns_view = NonNull::dangling();
        let appkit = RawWindowHandle::AppKit(AppKitWindowHandle::new(ns_view));
        assert_eq!(extract_ns_view_ptr(appkit), Some(ns_view.as_ptr()));

        let xlib = RawWindowHandle::Xlib(XlibWindowHandle::new(0x1234));
        assert_eq!(extract_ns_view_ptr(xlib), None);
    }

    #[test]
    fn mac_monitor_converts_to_damt_overlay_monitor_info() {
        let rect = ScreenRect {
            x: 0,
            y: 0,
            width: 3024,
            height: 1964,
        };
        let work_area = ScreenRect {
            x: 0,
            y: 76,
            width: 3024,
            height: 1748,
        };
        assert_eq!(
            monitor_info_from_mac_parts(0, rect, work_area, 192, true),
            MonitorInfo {
                id: MonitorId::new(0),
                rect,
                work_area,
                dpi: 192,
                primary: true,
            }
        );
    }
}
