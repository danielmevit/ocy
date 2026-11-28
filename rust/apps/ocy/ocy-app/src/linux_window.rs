//! Linux X11/Wayland window role attributes, EWMH/ICCCM hints, and overlay windows for break cards.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use damt_overlay::{MonitorId, MonitorInfo, Rgba};
use damt_platform::ScreenRect;
use damt_platform_linux::pure::{IdleEnv, x11_restack_above_target};
use damt_platform_linux::{
    LinuxDisplayBackend, LinuxWindowPolicy, X11MonitorInfo, apply_x11_break_window_hints,
    query_x11_monitors, raise_x11_window_above, select_display_backend,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::event::WindowEvent;
use slint::winit_030::winit::monitor::MonitorHandle;
use slint::winit_030::winit::platform::wayland::MonitorHandleExtWayland;
use slint::winit_030::winit::platform::x11::{WindowAttributesExtX11, WindowType};
use slint::winit_030::winit::window::{Fullscreen, WindowAttributes, WindowLevel};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};

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
        export component LinuxDimOverlay inherits Window {
            in property <color> dim-color: #000000c7;
            no-frame: true;
            always-on-top: true;
            background: root.dim-color;
        }

        export component LinuxEdgeFrameOverlay inherits Window {
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

use overlay_ui::{LinuxDimOverlay, LinuxEdgeFrameOverlay};

/// Internal role override for Linux-only overlay windows created via Slint/winit.
#[derive(Debug, Clone, Default)]
enum LinuxOverlayRole {
    #[default]
    None,
    WaylandDim(Option<MonitorHandle>),
    X11Overlay,
}

static WINIT_X11_ACTIVE: AtomicBool = AtomicBool::new(false);

thread_local! {
    static NEXT_LINUX_OVERLAY_ROLE: RefCell<LinuxOverlayRole> =
        const { RefCell::new(LinuxOverlayRole::None) };
    static ACTIVE_BREAK_CARD_XID: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Records whether the live winit backend is running on X11 (versus Wayland or headless tests).
pub fn init_winit_backend() {
    let is_x11 = select_display_backend(&IdleEnv::from_env()) == LinuxDisplayBackend::X11;
    WINIT_X11_ACTIVE.store(is_x11, Ordering::Relaxed);
}

/// Returns `true` when the live winit backend is running on X11.
#[must_use]
pub fn is_winit_x11_active() -> bool {
    WINIT_X11_ACTIVE.load(Ordering::Relaxed)
}

/// Pure helper returning `true` when a Slint window's first `show()` must be deferred until its
/// unmapped winit/X11 window exists so `WM_HINTS` and `_NET_WM_USER_TIME` are set before mapping.
#[must_use]
pub const fn should_defer_x11_show(winit_x11_active: bool, has_winit_window: bool) -> bool {
    winit_x11_active && !has_winit_window
}

/// Returns `true` when `window` needs deferred first `show()` on X11.
#[must_use]
pub fn should_defer_x11_card_show(window: &slint::Window) -> bool {
    should_defer_x11_show(is_winit_x11_active(), window.has_winit_window())
}

/// Records the active X11 break-card window ID and raises it above any existing overlays.
pub fn register_x11_break_card(window: &slint::Window) {
    if !is_winit_x11_active() {
        return;
    }
    let xid = extract_x11_window_id_and_sync(window);
    ACTIVE_BREAK_CARD_XID.with(|cell| cell.set(xid));
    if let Some(card_xid) = xid {
        let _ = raise_x11_window_above(card_xid);
    }
}

/// Clears the active X11 break-card window ID when break cards are hidden or closed.
pub fn clear_x11_break_card() {
    ACTIVE_BREAK_CARD_XID.with(|cell| cell.set(None));
}

struct LinuxOverlayRoleGuard {
    previous: LinuxOverlayRole,
}

impl LinuxOverlayRoleGuard {
    fn new(role: LinuxOverlayRole) -> Self {
        let previous = NEXT_LINUX_OVERLAY_ROLE.with(|cell| cell.replace(role));
        Self { previous }
    }
}

impl Drop for LinuxOverlayRoleGuard {
    fn drop(&mut self) {
        let prev = std::mem::take(&mut self.previous);
        NEXT_LINUX_OVERLAY_ROLE.with(|cell| {
            *cell.borrow_mut() = prev;
        });
    }
}

/// Overlay kind requested by [`OverlayBreakWindows`](crate::coordinator::OverlayBreakWindows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxOverlayKind {
    Dim,
    EdgeFrame,
}

/// Strategy selected for a Linux break overlay on the active display server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxOverlayMode {
    /// One borderless fullscreen window per Wayland output with an empty `wl_surface` input region.
    WaylandFullscreenPerOutput,
    /// One positioned/sized window per X11 monitor with EWMH notification hints and XShape pass-through.
    X11PositionedPerMonitor,
    /// Overlay is omitted (e.g. edge frames on Wayland, uncomposited X11 edge frames, or headless).
    None,
}

/// Pure decision helper choosing how `kind` should be presented on `backend`.
#[must_use]
pub const fn decide_overlay_mode(
    backend: LinuxDisplayBackend,
    kind: LinuxOverlayKind,
    x11_has_compositor: bool,
) -> LinuxOverlayMode {
    let policy = LinuxWindowPolicy::for_backend(backend, x11_has_compositor);
    match kind {
        LinuxOverlayKind::Dim => {
            if policy.uses_wayland_fullscreen_dim {
                LinuxOverlayMode::WaylandFullscreenPerOutput
            } else if policy.can_position_windows {
                LinuxOverlayMode::X11PositionedPerMonitor
            } else {
                LinuxOverlayMode::None
            }
        }
        LinuxOverlayKind::EdgeFrame => {
            if policy.uses_edge_frames {
                LinuxOverlayMode::X11PositionedPerMonitor
            } else {
                LinuxOverlayMode::None
            }
        }
    }
}

/// Pure decision helper describing which X11/Wayland attributes a Slint/winit window needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowAttributePlan {
    pub never_activate: bool,
    pub x11_notification_type: bool,
    pub borderless_fullscreen: bool,
    pub transparent: bool,
}

/// Computes [`WindowAttributePlan`] for a break card on `backend`.
#[must_use]
pub const fn break_card_attribute_plan(backend: LinuxDisplayBackend) -> WindowAttributePlan {
    match backend {
        LinuxDisplayBackend::X11 => WindowAttributePlan {
            never_activate: true,
            x11_notification_type: true,
            borderless_fullscreen: false,
            transparent: false,
        },
        LinuxDisplayBackend::Wayland | LinuxDisplayBackend::Headless => WindowAttributePlan {
            never_activate: true,
            x11_notification_type: false,
            borderless_fullscreen: false,
            transparent: false,
        },
    }
}

/// Extracts the X11 window ID (`u32`) from a `raw-window-handle` Xlib or Xcb handle.
#[must_use]
pub fn extract_x11_window_id_from_raw(raw: RawWindowHandle) -> Option<u32> {
    match raw {
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok().filter(|&id| id != 0),
        RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
        _ => None,
    }
}

/// Returns `true` when `raw` represents a Wayland surface handle.
#[must_use]
pub const fn is_wayland_window_handle(raw: RawWindowHandle) -> bool {
    matches!(raw, RawWindowHandle::Wayland(_))
}

/// Converts a [`damt_platform_linux::X11MonitorInfo`] into [`damt_overlay::MonitorInfo`].
#[must_use]
pub const fn monitor_info_from_x11(info: &X11MonitorInfo) -> MonitorInfo {
    MonitorInfo {
        id: MonitorId::new(info.id as isize),
        rect: info.rect,
        work_area: info.work_area,
        dpi: info.dpi,
        primary: info.primary,
    }
}

/// Pure assignment helper for Wayland per-output dim windows: given an initial window whose
/// `current_monitor_id` may or may not already be known and the full list of `output_ids`,
/// returns `(target_for_first_window, remaining_output_indices)`.
#[must_use]
pub fn plan_wayland_dim_outputs(
    current_output_id: Option<u32>,
    output_ids: &[u32],
) -> (Option<usize>, Vec<usize>) {
    if output_ids.is_empty() {
        return (None, Vec::new());
    }

    let first_idx = current_output_id
        .and_then(|id| output_ids.iter().position(|&out| out == id))
        .unwrap_or(0);
    let remaining = (0..output_ids.len())
        .filter(|&idx| idx != first_idx)
        .collect();
    let needs_explicit_assign = current_output_id.is_none_or(|id| output_ids[first_idx] != id);
    (
        if needs_explicit_assign {
            Some(first_idx)
        } else {
            None
        },
        remaining,
    )
}

/// Applies Linux-specific winit [`WindowAttributes`] for the active role before window creation.
#[must_use]
pub fn apply_window_attributes_for_role(mut attrs: WindowAttributes) -> WindowAttributes {
    let overlay_role = NEXT_LINUX_OVERLAY_ROLE.with(|cell| cell.borrow().clone());
    match overlay_role {
        LinuxOverlayRole::WaylandDim(monitor) => {
            // Wayland dim: never request activation, borderless fullscreen on the target output.
            attrs
                .with_active(false)
                .with_decorations(false)
                .with_transparent(true)
                .with_fullscreen(Some(Fullscreen::Borderless(monitor)))
        }
        LinuxOverlayRole::X11Overlay => attrs
            .with_visible(false)
            .with_active(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_x11_window_type(vec![WindowType::Notification]),
        LinuxOverlayRole::None => match current_window_role() {
            WindowRole::Normal => attrs,
            WindowRole::BreakCard => {
                let backend = select_display_backend(&IdleEnv::from_env());
                let plan = break_card_attribute_plan(backend);
                if backend == LinuxDisplayBackend::X11 {
                    attrs = attrs.with_visible(false);
                }
                if plan.never_activate {
                    attrs = attrs.with_active(false);
                }
                if plan.x11_notification_type {
                    attrs = attrs.with_x11_window_type(vec![WindowType::Notification]);
                }
                attrs
            }
        },
    }
}

/// Applies X11 EWMH/ICCCM break-card hints (`_NET_WM_WINDOW_TYPE_NOTIFICATION`,
/// `_NET_WM_STATE_ABOVE`, `_NET_WM_STATE_SKIP_TASKBAR`, `_NET_WM_STATE_SKIP_PAGER`,
/// `WM_HINTS` `input=false`) once the underlying winit window exists.
///
/// On Wayland, cards intentionally never request activation and stay normal toplevel windows
/// (documented limitation: GNOME/KDE Wayland lack `wlr-layer-shell`).
pub fn install_break_window_role(window: &slint::Window) -> bool {
    let backend = select_display_backend(&IdleEnv::from_env());
    if backend == LinuxDisplayBackend::Headless {
        return true;
    }

    let handle_kind = window
        .with_winit_window(|winit_win| {
            let Ok(handle) = winit_win.window_handle() else {
                return None;
            };
            let raw = handle.as_raw();
            if let Some(xid) = extract_x11_window_id_from_raw(raw) {
                Some(Ok(xid))
            } else if is_wayland_window_handle(raw) {
                Some(Err(()))
            } else {
                None
            }
        })
        .flatten();

    match handle_kind {
        Some(Ok(xid)) => {
            let _ = apply_x11_break_window_hints(xid);
            true
        }
        Some(Err(())) => true,
        None => false,
    }
}

/// Enumerates physical monitors on X11 via `damt-platform-linux` RandR.
///
/// Returns an empty list on Wayland (no global coordinate space) and headless sessions.
#[must_use]
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    if select_display_backend(&IdleEnv::from_env()) != LinuxDisplayBackend::X11 {
        return Vec::new();
    }
    query_x11_monitors()
        .monitors
        .iter()
        .map(monitor_info_from_x11)
        .collect()
}

/// Manages Linux-native break overlays (Wayland per-output fullscreen dims with empty input
/// regions; X11 per-monitor dims and composited click-through edge frames).
#[derive(Default)]
pub struct LinuxOverlays {
    dims: Vec<LinuxDimOverlay>,
    edge_frames: Vec<LinuxEdgeFrameOverlay>,
    generation: Rc<Cell<u64>>,
}

impl fmt::Debug for LinuxOverlays {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxOverlays")
            .field("dims", &self.dims.len())
            .field("edge_frames", &self.edge_frames.len())
            .finish()
    }
}

impl Drop for LinuxOverlays {
    fn drop(&mut self) {
        self.close();
    }
}

impl LinuxOverlays {
    /// Creates an empty [`LinuxOverlays`] container.
    #[must_use]
    pub fn new() -> Self {
        Self {
            dims: Vec::new(),
            edge_frames: Vec::new(),
            generation: Rc::new(Cell::new(0)),
        }
    }

    fn bump_generation(&self) -> u64 {
        let next = self.generation.get().wrapping_add(1);
        self.generation.set(next);
        next
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
        let _ = self.bump_generation();
        for dim in self.dims.drain(..) {
            let _ = dim.hide();
        }
        for frame in self.edge_frames.drain(..) {
            let _ = frame.hide();
        }
    }

    /// Updates the accent border colour on any active X11 edge frames.
    pub fn set_edge_frame_color(&mut self, color: Rgba) {
        let slint_col = slint_color(color);
        for frame in &self.edge_frames {
            frame.set_border_color(slint_col);
        }
    }

    /// Shows full-screen dim windows according to the active Linux display server.
    pub fn show_dims(&mut self, color: Rgba) {
        self.close();
        let backend = select_display_backend(&IdleEnv::from_env());
        match decide_overlay_mode(backend, LinuxOverlayKind::Dim, false) {
            LinuxOverlayMode::WaylandFullscreenPerOutput => {
                self.show_wayland_dims(color);
            }
            LinuxOverlayMode::X11PositionedPerMonitor => {
                self.show_x11_dims(color);
            }
            LinuxOverlayMode::None => {}
        }
    }

    /// Shows click-through edge frames on `monitors` (X11 with active compositor only; no-op on Wayland).
    pub fn show_edge_frames(&mut self, monitors: &[MonitorInfo], color: Rgba, thickness_dip: u32) {
        let generation = self.bump_generation();
        for frame in self.edge_frames.drain(..) {
            let _ = frame.hide();
        }
        if monitors.is_empty() {
            return;
        }
        let backend = select_display_backend(&IdleEnv::from_env());
        if backend != LinuxDisplayBackend::X11 {
            return;
        }
        let snapshot = query_x11_monitors();
        if decide_overlay_mode(
            backend,
            LinuxOverlayKind::EdgeFrame,
            snapshot.has_compositor,
        ) != LinuxOverlayMode::X11PositionedPerMonitor
        {
            return;
        }

        let slint_col = slint_color(color);
        for monitor in monitors {
            if monitor.rect.width <= 0 || monitor.rect.height <= 0 {
                continue;
            }
            let _guard = LinuxOverlayRoleGuard::new(LinuxOverlayRole::X11Overlay);
            let Ok(frame) = LinuxEdgeFrameOverlay::new() else {
                continue;
            };
            frame.set_border_color(slint_col);
            frame.set_border_width_px(thickness_dip as f32);
            if show_x11_overlay(&frame, monitor.rect, generation, &self.generation) {
                self.edge_frames.push(frame);
            }
        }
    }

    fn show_wayland_dims(&mut self, color: Rgba) {
        let slint_col = slint_color(color);
        let _guard = LinuxOverlayRoleGuard::new(LinuxOverlayRole::WaylandDim(None));
        let Ok(first) = LinuxDimOverlay::new() else {
            return;
        };
        first.set_dim_color(slint_col);
        attach_wayland_click_through_hook(first.window());
        if first.show().is_err() {
            return;
        }

        let probe = first.window().with_winit_window(|w| {
            let _ = w.set_cursor_hittest(false);
            let monitors: Vec<MonitorHandle> = w.available_monitors().collect();
            let current_id = w.current_monitor().map(|m| m.native_id());
            let output_ids: Vec<u32> = monitors
                .iter()
                .map(MonitorHandleExtWayland::native_id)
                .collect();
            let (assign_first, remaining) = plan_wayland_dim_outputs(current_id, &output_ids);
            if let Some(idx) = assign_first
                && let Some(mon) = monitors.get(idx).cloned()
            {
                w.set_fullscreen(Some(Fullscreen::Borderless(Some(mon))));
            }
            w.request_redraw();
            (monitors, remaining)
        });

        self.dims.push(first);

        let Some((monitors, remaining)) = probe else {
            // Non-winit platform (e.g., HeadlessPlatform in unit/soak tests): nothing else to spawn.
            return;
        };

        for idx in remaining {
            let Some(monitor) = monitors.get(idx).cloned() else {
                continue;
            };
            let _guard = LinuxOverlayRoleGuard::new(LinuxOverlayRole::WaylandDim(Some(monitor)));
            let Ok(dim) = LinuxDimOverlay::new() else {
                continue;
            };
            dim.set_dim_color(slint_col);
            attach_wayland_click_through_hook(dim.window());
            if dim.show().is_ok() {
                dim.window().with_winit_window(|w| {
                    let _ = w.set_cursor_hittest(false);
                    w.request_redraw();
                });
                self.dims.push(dim);
            }
        }
    }

    fn show_x11_dims(&mut self, color: Rgba) {
        let generation = self.generation.get();
        let snapshot = query_x11_monitors();
        let slint_col = slint_color(color);
        for monitor in snapshot.monitors {
            if monitor.rect.width <= 0 || monitor.rect.height <= 0 {
                continue;
            }
            let _guard = LinuxOverlayRoleGuard::new(LinuxOverlayRole::X11Overlay);
            let Ok(dim) = LinuxDimOverlay::new() else {
                continue;
            };
            dim.set_dim_color(slint_col);
            if show_x11_overlay(&dim, monitor.rect, generation, &self.generation) {
                self.dims.push(dim);
            }
        }
    }
}

fn slint_color(color: Rgba) -> slint::Color {
    slint::Color::from_argb_u8(color.a, color.r, color.g, color.b)
}

fn attach_wayland_click_through_hook(window: &slint::Window) {
    let applied = Rc::new(Cell::new(false));
    window.on_winit_window_event(move |win, event| {
        if !applied.get()
            && matches!(event, WindowEvent::RedrawRequested)
            && win
                .with_winit_window(|w| {
                    let _ = w.set_cursor_hittest(false);
                })
                .is_some()
        {
            applied.set(true);
        }
        EventResult::Propagate
    });
}

fn extract_x11_window_id_and_sync(window: &slint::Window) -> Option<u32> {
    window
        .with_winit_window(|w| {
            let _ = w.inner_position();
            let handle = w.window_handle().ok()?;
            extract_x11_window_id_from_raw(handle.as_raw())
        })
        .flatten()
}

fn apply_x11_overlay_before_show(window: &slint::Window) -> bool {
    let xid = window
        .with_winit_window(|w| {
            let _ = w.set_cursor_hittest(false);
            let _ = w.inner_position();
            let handle = w.window_handle().ok()?;
            extract_x11_window_id_from_raw(handle.as_raw())
        })
        .flatten();
    if let Some(xid) = xid {
        let _ = apply_x11_break_window_hints(xid);
        true
    } else {
        false
    }
}

fn restack_after_x11_overlay_show(window: &slint::Window) {
    if let Some(overlay_xid) = extract_x11_window_id_and_sync(window)
        && let Some(card_xid) =
            x11_restack_above_target(ACTIVE_BREAK_CARD_XID.with(Cell::get), overlay_xid)
    {
        let _ = raise_x11_window_above(card_xid);
    }
}

fn show_x11_overlay<C: ComponentHandle + 'static>(
    overlay: &C,
    rect: ScreenRect,
    generation: u64,
    gen_cell: &Rc<Cell<u64>>,
) -> bool {
    let win = overlay.window();
    win.set_position(PhysicalPosition::new(rect.x, rect.y));
    win.set_size(PhysicalSize::new(
        u32::try_from(rect.width).unwrap_or(1),
        u32::try_from(rect.height).unwrap_or(1),
    ));

    if should_defer_x11_show(is_winit_x11_active(), win.has_winit_window()) {
        let weak = overlay.as_weak();
        let gen_ref = Rc::clone(gen_cell);
        if slint::spawn_local(async move {
            if gen_ref.get() != generation {
                return;
            }
            let Some(overlay) = weak.upgrade() else {
                return;
            };
            let winit_res = overlay.window().winit_window().await;
            drop(winit_res);
            if gen_ref.get() != generation {
                return;
            }
            apply_x11_overlay_before_show(overlay.window());
            if overlay.show().is_ok() {
                restack_after_x11_overlay_show(overlay.window());
            }
        })
        .is_ok()
        {
            return true;
        }
    }

    if is_winit_x11_active() {
        apply_x11_overlay_before_show(win);
    }
    if overlay.show().is_ok() {
        if is_winit_x11_active() {
            restack_after_x11_overlay_show(overlay.window());
        }
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use damt_overlay::{MonitorId, MonitorInfo};
    use damt_platform::ScreenRect;
    use damt_platform_linux::{LinuxDisplayBackend, X11MonitorInfo};
    use raw_window_handle::{
        RawWindowHandle, WaylandWindowHandle, XcbWindowHandle, XlibWindowHandle,
    };

    use super::{
        LinuxOverlayKind, LinuxOverlayMode, WindowAttributePlan, break_card_attribute_plan,
        decide_overlay_mode, extract_x11_window_id_from_raw, is_wayland_window_handle,
        monitor_info_from_x11, plan_wayland_dim_outputs, should_defer_x11_show,
    };

    #[test]
    fn raw_handle_extracts_xlib_and_xcb_ids_and_detects_wayland() {
        let xlib = RawWindowHandle::Xlib(XlibWindowHandle::new(0x0040_002A));
        assert_eq!(extract_x11_window_id_from_raw(xlib), Some(0x0040_002A));
        assert!(!is_wayland_window_handle(xlib));

        let xlib_zero = RawWindowHandle::Xlib(XlibWindowHandle::new(0));
        assert_eq!(extract_x11_window_id_from_raw(xlib_zero), None);

        let xcb = RawWindowHandle::Xcb(XcbWindowHandle::new(
            NonZeroU32::new(0x0080_0005).expect("non-zero"),
        ));
        assert_eq!(extract_x11_window_id_from_raw(xcb), Some(0x0080_0005));

        let wayland =
            RawWindowHandle::Wayland(WaylandWindowHandle::new(std::ptr::NonNull::dangling()));
        assert_eq!(extract_x11_window_id_from_raw(wayland), None);
        assert!(is_wayland_window_handle(wayland));
    }

    #[test]
    fn overlay_mode_and_attribute_plan_match_x11_vs_wayland_rules() {
        assert_eq!(
            decide_overlay_mode(LinuxDisplayBackend::Wayland, LinuxOverlayKind::Dim, true),
            LinuxOverlayMode::WaylandFullscreenPerOutput
        );
        assert_eq!(
            decide_overlay_mode(
                LinuxDisplayBackend::Wayland,
                LinuxOverlayKind::EdgeFrame,
                true
            ),
            LinuxOverlayMode::None
        );
        assert_eq!(
            decide_overlay_mode(LinuxDisplayBackend::X11, LinuxOverlayKind::Dim, false),
            LinuxOverlayMode::X11PositionedPerMonitor
        );
        assert_eq!(
            decide_overlay_mode(LinuxDisplayBackend::X11, LinuxOverlayKind::EdgeFrame, true),
            LinuxOverlayMode::X11PositionedPerMonitor
        );
        assert_eq!(
            decide_overlay_mode(LinuxDisplayBackend::X11, LinuxOverlayKind::EdgeFrame, false),
            LinuxOverlayMode::None
        );
        assert_eq!(
            decide_overlay_mode(LinuxDisplayBackend::Headless, LinuxOverlayKind::Dim, true),
            LinuxOverlayMode::None
        );

        assert_eq!(
            break_card_attribute_plan(LinuxDisplayBackend::X11),
            WindowAttributePlan {
                never_activate: true,
                x11_notification_type: true,
                borderless_fullscreen: false,
                transparent: false,
            }
        );
        assert_eq!(
            break_card_attribute_plan(LinuxDisplayBackend::Wayland),
            WindowAttributePlan {
                never_activate: true,
                x11_notification_type: false,
                borderless_fullscreen: false,
                transparent: false,
            }
        );
    }

    #[test]
    fn wayland_dim_output_assignment_covers_every_output_without_duplicates() {
        assert_eq!(plan_wayland_dim_outputs(None, &[]), (None, Vec::new()));
        assert_eq!(
            plan_wayland_dim_outputs(None, &[10, 20]),
            (Some(0), vec![1])
        );
        assert_eq!(
            plan_wayland_dim_outputs(Some(20), &[10, 20, 30]),
            (None, vec![0, 2])
        );
        assert_eq!(
            plan_wayland_dim_outputs(Some(99), &[10, 20]),
            (Some(0), vec![1])
        );
    }

    #[test]
    fn x11_monitor_converts_to_damt_overlay_monitor_info() {
        let x11 = X11MonitorInfo {
            id: 42,
            rect: ScreenRect {
                x: 1920,
                y: 0,
                width: 2560,
                height: 1440,
            },
            work_area: ScreenRect {
                x: 1920,
                y: 32,
                width: 2560,
                height: 1408,
            },
            dpi: 144,
            primary: true,
        };
        assert_eq!(
            monitor_info_from_x11(&x11),
            MonitorInfo {
                id: MonitorId::new(42),
                rect: x11.rect,
                work_area: x11.work_area,
                dpi: 144,
                primary: true,
            }
        );
    }

    #[test]
    fn x11_show_deferral_applies_only_to_uncreated_winit_x11_windows() {
        assert!(should_defer_x11_show(true, false));
        assert!(!should_defer_x11_show(true, true));
        assert!(!should_defer_x11_show(false, false));
        assert!(!should_defer_x11_show(false, true));
    }
}
