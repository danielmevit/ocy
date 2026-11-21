//! Active screen selection and overlay layout math (port of `Layout/ActiveScreenSelector.cs` and `Layout/OverlayLayout.cs`).

use damt_overlay::MonitorInfo;
use damt_platform::ScreenRect;

/// Physical screen descriptor used for layout and active-monitor selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenInfo {
    /// Full monitor bounds in physical pixels.
    pub bounds: ScreenRect,
    /// DPI scaling factor (`1.0` at 96 DPI).
    pub scaling: f64,
    /// Whether this is the primary display.
    pub is_primary: bool,
    /// Optional work area excluding taskbars/docks.
    pub working_area: Option<ScreenRect>,
}

impl ScreenInfo {
    /// Creates a `ScreenInfo` from explicit bounds, scaling, primary flag, and optional work area.
    #[must_use]
    pub const fn new(
        bounds: ScreenRect,
        scaling: f64,
        is_primary: bool,
        working_area: Option<ScreenRect>,
    ) -> Self {
        Self {
            bounds,
            scaling,
            is_primary,
            working_area,
        }
    }

    /// Convenience constructor from individual coordinates and dimensions.
    #[must_use]
    pub const fn from_xywh(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        scaling: f64,
        is_primary: bool,
    ) -> Self {
        Self {
            bounds: ScreenRect {
                x,
                y,
                width,
                height,
            },
            scaling,
            is_primary,
            working_area: None,
        }
    }

    /// Returns `working_area` when present, falling back to `bounds`.
    #[must_use]
    pub const fn effective_working_area(&self) -> ScreenRect {
        match self.working_area {
            Some(work) => work,
            None => self.bounds,
        }
    }
}

impl From<&MonitorInfo> for ScreenInfo {
    fn from(monitor: &MonitorInfo) -> Self {
        let scaling = if monitor.dpi > 0 {
            f64::from(monitor.dpi) / 96.0
        } else {
            1.0
        };
        Self {
            bounds: monitor.rect,
            scaling,
            is_primary: monitor.primary,
            working_area: Some(monitor.work_area),
        }
    }
}

/// Logical size in device-independent pixels (DIPs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DipSize {
    pub width: f64,
    pub height: f64,
}

impl DipSize {
    /// Creates a new `DipSize`.
    #[must_use]
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// Physical pixel coordinates on the virtual desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelPoint {
    pub x: i32,
    pub y: i32,
}

impl PixelPoint {
    /// Creates a new `PixelPoint`.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Computed overlay window position and logical size for one monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayBounds {
    pub position: PixelPoint,
    pub dip_size: DipSize,
    pub is_primary: bool,
}

/// Chooses the monitor index on which a break card should appear.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActiveScreenSelector;

impl ActiveScreenSelector {
    /// Returns the index in `screens` matching `active` (exact bounds, then center containment,
    /// then primary screen, else `0`).
    #[must_use]
    pub fn select(screens: &[ScreenInfo], active: Option<ScreenRect>) -> usize {
        if screens.is_empty() {
            return 0;
        }

        if let Some(act) = active {
            // 1. Prefer the screen whose physical bounds equal `active`.
            for (i, screen) in screens.iter().enumerate() {
                if screen.bounds == act {
                    return i;
                }
            }

            // 2. Otherwise the screen containing the center of `active`.
            let center_x = i64::from(act.x) + i64::from(act.width) / 2;
            let center_y = i64::from(act.y) + i64::from(act.height) / 2;

            for (i, screen) in screens.iter().enumerate() {
                let b = screen.bounds;
                let left = i64::from(b.x);
                let top = i64::from(b.y);
                let right = left + i64::from(b.width);
                let bottom = top + i64::from(b.height);
                if center_x >= left && center_x < right && center_y >= top && center_y < bottom {
                    return i;
                }
            }
        }

        // 3. Otherwise the primary screen.
        for (i, screen) in screens.iter().enumerate() {
            if screen.is_primary {
                return i;
            }
        }

        0
    }
}

/// Returns all monitors except the active monitor selected by [`ActiveScreenSelector`].
#[must_use]
pub fn non_active_monitors(
    monitors: &[MonitorInfo],
    active: Option<ScreenRect>,
) -> Vec<MonitorInfo> {
    if monitors.is_empty() {
        return Vec::new();
    }
    let screens: Vec<ScreenInfo> = monitors.iter().map(ScreenInfo::from).collect();
    let selected = ActiveScreenSelector::select(&screens, active);
    monitors
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != selected)
        .map(|(_, monitor)| *monitor)
        .collect()
}

/// Returns the active [`ScreenInfo`] chosen by [`ActiveScreenSelector`], or `None` when `monitors` is empty.
#[must_use]
pub fn select_active_screen(
    monitors: &[MonitorInfo],
    active: Option<ScreenRect>,
) -> Option<ScreenInfo> {
    if monitors.is_empty() {
        return None;
    }
    let screens: Vec<ScreenInfo> = monitors.iter().map(ScreenInfo::from).collect();
    let selected = ActiveScreenSelector::select(&screens, active);
    screens.get(selected).copied()
}

/// Computes DIP sizes and physical corner/overlay positions across monitors.
#[derive(Debug, Default, Clone, Copy)]
pub struct OverlayLayout;

impl OverlayLayout {
    /// Converts physical pixel dimensions to logical DIPs at `scaling`.
    #[must_use]
    pub fn compute_dip_size(physical_width: i32, physical_height: i32, scaling: f64) -> DipSize {
        let scale = if scaling > 0.0 { scaling } else { 1.0 };
        DipSize {
            width: f64::from(physical_width) / scale,
            height: f64::from(physical_height) / scale,
        }
    }

    /// Computes overlay bounds (physical origin + DIP size) for every screen.
    #[must_use]
    pub fn compute(screens: &[ScreenInfo]) -> Vec<OverlayBounds> {
        if screens.is_empty() {
            return Vec::new();
        }

        screens
            .iter()
            .map(|screen| OverlayBounds {
                position: PixelPoint::new(screen.bounds.x, screen.bounds.y),
                dip_size: Self::compute_dip_size(
                    screen.bounds.width,
                    screen.bounds.height,
                    screen.scaling,
                ),
                is_primary: screen.is_primary,
            })
            .collect()
    }

    /// Computes the physical top-left pixel position for a bottom-right corner window.
    #[must_use]
    pub fn compute_corner_position(
        screen: &ScreenInfo,
        window_dip_size: DipSize,
        margin_dip: f64,
    ) -> PixelPoint {
        let scaling = if screen.scaling > 0.0 {
            screen.scaling
        } else {
            1.0
        };
        let work_area = screen.effective_working_area();

        let total_width_physical = ((window_dip_size.width + margin_dip) * scaling).round() as i32;
        let total_height_physical =
            ((window_dip_size.height + margin_dip) * scaling).round() as i32;

        let x = (work_area.x + work_area.width) - total_width_physical;
        let y = (work_area.y + work_area.height) - total_height_physical;

        PixelPoint::new(x, y)
    }

    /// Converts a logical DIP size to physical pixels at `scaling` (clamped to at least 1x1).
    #[must_use]
    pub fn physical_size(dip_size: DipSize, scaling: f64) -> (u32, u32) {
        let scale = if scaling > 0.0 && scaling.is_finite() {
            scaling
        } else {
            1.0
        };
        let w = (dip_size.width * scale).round().max(1.0) as u32;
        let h = (dip_size.height * scale).round().max(1.0) as u32;
        (w, h)
    }

    /// Computes the physical top-left pixel position that centers `window_dip_size` on `screen.bounds`.
    #[must_use]
    pub fn compute_centered_position(screen: &ScreenInfo, window_dip_size: DipSize) -> PixelPoint {
        let (phys_w, phys_h) = Self::physical_size(window_dip_size, screen.scaling);
        let x = screen.bounds.x + (screen.bounds.width - phys_w as i32) / 2;
        let y = screen.bounds.y + (screen.bounds.height - phys_h as i32) / 2;
        PixelPoint::new(x, y)
    }

    /// Computes the physical top-left pixel position that centers a window of `(window_width_px,
    /// window_height_px)` inside `area`, clamping to `area`'s origin if the window is larger.
    #[must_use]
    pub fn compute_centered_rect_position(
        area: ScreenRect,
        window_width_px: u32,
        window_height_px: u32,
    ) -> PixelPoint {
        let w = i32::try_from(window_width_px).unwrap_or(i32::MAX);
        let h = i32::try_from(window_height_px).unwrap_or(i32::MAX);
        let x = area.x + (area.width - w).max(0) / 2;
        let y = area.y + (area.height - h).max(0) / 2;
        PixelPoint::new(x, y)
    }

    /// Computes the physical top-left pixel position that centers `window_dip_size` on
    /// `screen.effective_working_area()` at `screen.scaling`.
    #[must_use]
    pub fn compute_centered_work_area_position(
        screen: &ScreenInfo,
        window_dip_size: DipSize,
    ) -> PixelPoint {
        let (phys_w, phys_h) = Self::physical_size(window_dip_size, screen.scaling);
        Self::compute_centered_rect_position(screen.effective_working_area(), phys_w, phys_h)
    }

    /// Computes the logical window size for [`ocy_ui::LongBreakCard`] with `dim_backdrop = false`
    /// (480 px card + 32 px side shadow margins = 544 px, content height + 80 px vertical shadow
    /// margins for 32 px blur + 8 px y-offset), clamped to `screen` DIP bounds when smaller.
    #[must_use]
    pub fn compute_long_break_window_size(content_height_dip: f64, screen: &ScreenInfo) -> DipSize {
        let screen_dip =
            Self::compute_dip_size(screen.bounds.width, screen.bounds.height, screen.scaling);
        let desired_w = 544.0_f64;
        let desired_h = content_height_dip.max(100.0) + 80.0;
        let w = if screen_dip.width > 0.0 {
            desired_w.min(screen_dip.width)
        } else {
            desired_w
        };
        let h = if screen_dip.height > 0.0 {
            desired_h.min(screen_dip.height)
        } else {
            desired_h
        };
        DipSize::new(w, h)
    }
}

/// Default (and smallest auto-sized) logical height of the Settings window.
pub const SETTINGS_DEFAULT_HEIGHT_DIP: f64 = 640.0;
/// Logical allowance for the title bar and frame when fitting the Settings window in a work area.
pub const SETTINGS_FRAME_ALLOWANCE_DIP: f64 = 40.0;

/// Logical client height the Settings window should have so its content needs no scrolling.
///
/// * `content_dip`: height the content needs (`content-height`).
/// * `work_area_dip`: height of the monitor work area in logical pixels.
/// * `current_dip`: the window's current client height.
/// * `user_resized`: the user resized the window by hand; their size wins (the content scrolls).
///
/// Grows to fit the content up to the work area (minus the frame), and shrinks back but never below
/// the 640 px default. When the work area is too small the window fits it and the content scrolls.
#[must_use]
pub fn settings_window_height(
    content_dip: f64,
    work_area_dip: f64,
    current_dip: f64,
    user_resized: bool,
) -> f64 {
    if user_resized || !content_dip.is_finite() || !work_area_dip.is_finite() {
        return current_dip;
    }
    let cap = (work_area_dip - SETTINGS_FRAME_ALLOWANCE_DIP).max(0.0);
    content_dip.ceil().max(SETTINGS_DEFAULT_HEIGHT_DIP).min(cap)
}

/// Preferred logical width of the Settings window (`preferred-width` in `settings_v2.slint`).
pub const SETTINGS_DEFAULT_WIDTH_DIP: f64 = 560.0;

/// Logical client width for an auto-fit: the 560 px preferred width (clamped to the work area) until
/// the user resizes by hand, then their width. Never the 480 px min-width the window may report
/// while it is still being shown.
#[must_use]
pub fn settings_window_width(work_area_dip: f64, current_dip: f64, user_resized: bool) -> f64 {
    if user_resized || !work_area_dip.is_finite() {
        return current_dip;
    }
    SETTINGS_DEFAULT_WIDTH_DIP.min(work_area_dip.max(0.0))
}

/// Physical top edge for the Settings window after an auto-fit changes its outer height.
///
/// The window grows and shrinks downwards: the current top is kept, unless the new bottom would pass
/// the work-area bottom; then the top moves up by exactly the overflow, never above the work-area top.
#[must_use]
pub fn settings_window_top(current_top: i32, new_outer_height: u32, work_area: ScreenRect) -> i32 {
    let h = i32::try_from(new_outer_height).unwrap_or(i32::MAX);
    let bottom = work_area.y.saturating_add(work_area.height);
    let overflow = current_top.saturating_add(h).saturating_sub(bottom).max(0);
    current_top.saturating_sub(overflow).max(work_area.y)
}
