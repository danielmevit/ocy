//! macOS AppKit window role attributes, non-activating `NSWindow` configuration, activation
//! policy (`Accessory`, no Dock icon), and overlay windows (`MacosOverlays`) for break cards.
//!
//! Pure policy, level, collection-behavior, handle-extraction, and monitor-mapping helpers
//! compile on every target so their unit tests run on Linux and in CI; all `objc2-app-kit`
//! calls live in the `#[cfg(target_os = "macos")]` `macos_impl` submodule.

use std::ffi::c_void;

use damt_overlay::{MonitorId, MonitorInfo};
use damt_platform::ScreenRect;
use raw_window_handle::RawWindowHandle;

/// `NSWindowCollectionBehaviorCanJoinAllSpaces` (`1 << 0`).
pub const NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES: usize = 1 << 0;
/// `NSWindowCollectionBehaviorStationary` (`1 << 4`).
pub const NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY: usize = 1 << 4;
/// `NSWindowCollectionBehaviorFullScreenAuxiliary` (`1 << 8`).
pub const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY: usize = 1 << 8;

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
    /// `NSFloatingWindowLevel` (`3`): floats above normal and auxiliary fullscreen windows.
    Floating,
    /// `NSScreenSaverWindowLevel` (`1000`): covers desktop, menu bar, and Dock.
    ScreenSaver,
}

impl MacWindowLevel {
    /// Numeric `NSWindowLevel` (`NSInteger`) value defined by AppKit.
    #[must_use]
    pub const fn as_raw(self) -> isize {
        match self {
            Self::Floating => 3,
            Self::ScreenSaver => 1000,
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
            level: MacWindowLevel::Floating,
            collection_behavior_bits,
            ignores_mouse_events: false,
            order_front_regardless: true,
            never_activate: true,
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

#[cfg(target_os = "macos")]
mod macos_impl {
    use std::cell::Cell;
    use std::fmt;
    use std::rc::Rc;

    use damt_overlay::{MonitorInfo, Rgba};
    use damt_platform::ScreenRect;
    use damt_platform_mac::{MacMonitorInfo, query_mac_monitors};
    use objc2::MainThreadMarker;
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
    use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};

    use super::{
        MacActivationPolicyChoice, MacWindowKind, extract_ns_view_ptr, monitor_info_from_mac_parts,
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
            export component MacDimOverlay inherits Window {
                in property <color> dim-color: #000000c7;
                no-frame: true;
                always-on-top: true;
                background: root.dim-color;
            }

            export component MacEdgeFrameOverlay inherits Window {
                in property <color> border-color: #2f7d628c;
                in property <float> border-width-px: 3.0;
                no-frame: true;
                always-on-top: true;
                background: transparent;

                Rectangle {
                    width: 100%;
                    height: 100%;
                    background: transparent;
                    border-width: root.border-width-px * 1px;
                    border-color: root.border-color;
                }
            }
        }
    }

    use overlay_ui::{MacDimOverlay, MacEdgeFrameOverlay};

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
                        .with_has_shadow(policy.has_shadow)
                        .with_window_level(WindowLevel::AlwaysOnTop)
                }
            },
        }
    }

    /// Applies AppKit `NSWindow` level, collection behavior, mouse pass-through, and
    /// non-activating `orderFrontRegardless` to a Slint window.
    pub fn apply_ns_window_policy(window: &slint::Window, kind: MacWindowKind) -> bool {
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
                apply_policy_to_ns_window(&ns_window, policy);
                true
            })
            .unwrap_or(false)
    }

    fn apply_policy_to_ns_window(ns_window: &NSWindow, policy: super::MacWindowPolicy) {
        ns_window.setLevel(policy.level.as_raw());
        ns_window.setCollectionBehavior(NSWindowCollectionBehavior::from_bits_retain(
            policy.collection_behavior_bits,
        ));
        ns_window.setIgnoresMouseEvents(policy.ignores_mouse_events);
        if policy.order_front_regardless {
            ns_window.orderFrontRegardless();
        }
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

    /// Manages macOS-native break overlays (`MacDimOverlay` and click-through `MacEdgeFrameOverlay`).
    #[derive(Default)]
    pub struct MacosOverlays {
        dims: Vec<MacDimOverlay>,
        edge_frames: Vec<MacEdgeFrameOverlay>,
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
            let slint_col = slint_color(color);
            for frame in &self.edge_frames {
                frame.set_border_color(slint_col);
            }
        }

        /// Shows non-activating, click-through full-monitor dim overlays across all displays.
        pub fn show_dims(&mut self, color: Rgba) {
            self.close();
            let monitors = enumerate_monitors();
            let slint_col = slint_color(color);
            for monitor in monitors {
                if monitor.rect.width <= 0 || monitor.rect.height <= 0 {
                    continue;
                }
                let _guard = MacosOverlayRoleGuard::new(MacosOverlayRole::Dim);
                let Ok(dim) = MacDimOverlay::new() else {
                    continue;
                };
                dim.set_dim_color(slint_col);
                configure_overlay_window(&dim, monitor.rect, MacWindowKind::Dim);
                if dim.show().is_ok() {
                    apply_ns_window_policy(dim.window(), MacWindowKind::Dim);
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
            let slint_col = slint_color(color);
            for monitor in monitors {
                if monitor.rect.width <= 0 || monitor.rect.height <= 0 {
                    continue;
                }
                let _guard = MacosOverlayRoleGuard::new(MacosOverlayRole::EdgeFrame);
                let Ok(frame) = MacEdgeFrameOverlay::new() else {
                    continue;
                };
                frame.set_border_color(slint_col);
                frame.set_border_width_px(thickness_dip as f32);
                configure_overlay_window(&frame, monitor.rect, MacWindowKind::EdgeFrame);
                if frame.show().is_ok() {
                    apply_ns_window_policy(frame.window(), MacWindowKind::EdgeFrame);
                    self.edge_frames.push(frame);
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
                && apply_ns_window_policy(w, kind)
            {
                applied.set(true);
            }
            EventResult::Propagate
        });
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::{
    MacosOverlays, apply_ns_window_policy, apply_window_attributes_for_role,
    configure_event_loop_builder, ensure_accessory_activation_policy, enumerate_monitors,
    install_break_window_role, monitor_info_from_mac,
};

#[cfg(test)]
mod tests {
    use std::ptr::NonNull;

    use damt_overlay::{MonitorId, MonitorInfo};
    use damt_platform::ScreenRect;
    use raw_window_handle::{AppKitWindowHandle, RawWindowHandle, XlibWindowHandle};

    use super::{
        MacActivationPolicyChoice, MacWindowKind, MacWindowLevel,
        NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES,
        NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY,
        NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY, extract_ns_view_ptr, monitor_info_from_mac_parts,
        overlay_collection_behavior_bits, startup_activation_policy, window_policy_for_kind,
    };

    #[test]
    fn window_level_values_match_appkit_constants() {
        assert_eq!(MacWindowLevel::Floating.as_raw(), 3);
        assert_eq!(MacWindowLevel::ScreenSaver.as_raw(), 1000);
    }

    #[test]
    fn window_policy_sets_floating_for_cards_and_screen_saver_for_dims() {
        let expected_behavior = NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES
            | NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY
            | NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY;
        assert_eq!(overlay_collection_behavior_bits(), expected_behavior);

        let card = window_policy_for_kind(MacWindowKind::BreakCard);
        assert_eq!(card.level, MacWindowLevel::Floating);
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
