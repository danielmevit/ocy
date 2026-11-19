//! Native layered Win32 overlays for the damt.xyz apps.
//!
//! The crate provides:
//! - [`EdgeFrames`]: one click-through layered window per monitor, painted with a scaled border.
//! - [`Dims`]: one input-blocking, non-activating layered window per monitor, painted a flat colour.
//! - [`enumerate_monitors`]: physical monitor bounds, work areas, DPI and a stable handle id.
//!
//! # Threading
//!
//! Every window is created, updated and destroyed on the calling thread. Call [`EdgeFrames::show`],
//! [`EdgeFrames::set_color`] and [`Dims::show`] on the UI thread, and drop the returned handles on the same
//! thread. The API does not move windows between threads for you.
//!
//! # DPI
//!
//! Window positions and sizes are physical pixels taken from [`MonitorInfo::rect`]. The resulting window rect
//! equals the monitor rect exactly. Per-monitor DPI v2 awareness is the app manifest's job; the app must be
//! per-monitor-v2 aware before it asks for overlays on mixed-DPI desktops. Border thickness is given in DIPs and
//! scaled by each monitor's effective DPI.
//!
//! # Platforms
//!
//! The crate is Windows-first. On other targets [`enumerate_monitors`] returns an empty list and
//! [`EdgeFrames::show`] / [`Dims::show`] create no windows. The pure helpers (colour, premultiplied pixels and
//! border geometry) are compiled and tested on every target.

#![deny(clippy::unwrap_used, clippy::expect_used)]

mod pure;

#[cfg(windows)]
mod windows;

pub use pure::{
    ColorParseError, PixelRect, Rgba, border_rects, fill_border, fill_solid, premultiplied_argb,
    scaled_length,
};

use damt_platform::ScreenRect;

/// A stable monitor handle id. On Windows this is the `HMONITOR` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MonitorId(isize);

impl MonitorId {
    /// Wrap a platform handle value.
    pub const fn new(raw: isize) -> Self {
        Self(raw)
    }

    /// The raw platform handle value.
    pub const fn raw(self) -> isize {
        self.0
    }
}

/// A physical monitor as the overlay crate needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MonitorInfo {
    /// Stable platform handle id.
    pub id: MonitorId,
    /// Full monitor bounds in physical virtual-desktop pixels.
    pub rect: ScreenRect,
    /// Work area in physical virtual-desktop pixels (excludes the taskbar).
    pub work_area: ScreenRect,
    /// Effective DPI (96 is 100 %).
    pub dpi: u32,
    /// Whether this is the primary monitor.
    pub primary: bool,
}

/// Native window diagnostics for smoke checks and examples.
///
/// The rect is in physical screen pixels. `ex_style` is the Win32 extended style word; on non-Windows
/// targets there are no windows and the list is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowInfo {
    /// Native window handle value (`HWND` on Windows).
    pub handle: isize,
    /// Window rect in physical screen pixels.
    pub rect: ScreenRect,
    /// Extended window style bits (`GWL_EXSTYLE`) on Windows, zero elsewhere.
    pub ex_style: u32,
}

/// Enumerate the active monitors.
///
/// Returns an empty list on non-Windows targets. Order follows `EnumDisplayMonitors`.
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    #[cfg(windows)]
    {
        windows::enumerate_monitors()
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Click-through edge frames, one layered window per requested monitor.
///
/// The border is [`thickness_dip`](EdgeFrames::show) scaled by each monitor's DPI; the inside is fully
/// transparent and input passes through. Dropping the value destroys every window.
#[derive(Debug)]
pub struct EdgeFrames {
    color: Rgba,
    #[cfg(windows)]
    frames: Vec<windows::FrameWindow>,
}

impl EdgeFrames {
    /// Create one click-through layered window per monitor in `monitors`.
    ///
    /// Pass the monitors you want framed (for example every non-primary monitor during a short break). Windows
    /// that cannot be created are skipped; inspect [`len`](Self::len) or [`window_info`](Self::window_info).
    pub fn show(monitors: &[MonitorInfo], color: Rgba, thickness_dip: u32) -> Self {
        #[cfg(windows)]
        {
            let frames = monitors
                .iter()
                .filter_map(|monitor| windows::FrameWindow::new(monitor, color, thickness_dip))
                .collect();

            Self { color, frames }
        }

        #[cfg(not(windows))]
        {
            let _ = (monitors, thickness_dip);
            Self { color }
        }
    }

    /// Change the border colour in place. The window handles are not recreated.
    pub fn set_color(&mut self, color: Rgba) {
        self.color = color;

        #[cfg(windows)]
        for frame in &self.frames {
            let _ = frame.apply_color(color);
        }
    }

    /// The current border colour.
    pub const fn color(&self) -> Rgba {
        self.color
    }

    /// Number of windows that were created successfully.
    pub fn len(&self) -> usize {
        #[cfg(windows)]
        {
            self.frames.len()
        }

        #[cfg(not(windows))]
        {
            0
        }
    }

    /// Whether no window was created.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Native window diagnostics (empty on non-Windows targets).
    pub fn window_info(&self) -> Vec<WindowInfo> {
        #[cfg(windows)]
        {
            self.frames.iter().map(windows::FrameWindow::info).collect()
        }

        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}

impl Drop for EdgeFrames {
    fn drop(&mut self) {
        // Each `NativeWindow` destroys its own HWND, so clearing now makes destruction explicit.
        #[cfg(windows)]
        self.frames.clear();
    }
}

/// Full-monitor dims, one layered window per requested monitor.
///
/// The window covers the monitor, blocks input (no `WS_EX_TRANSPARENT`), never activates
/// (`WS_EX_NOACTIVATE`) and never gets a taskbar button (`WS_EX_TOOLWINDOW`). Dropping the value destroys
/// every window.
#[derive(Debug)]
pub struct Dims {
    color: Rgba,
    #[cfg(windows)]
    windows: Vec<windows::DimWindow>,
}

impl Dims {
    /// Cover each monitor in `monitors` with a flat, input-blocking layered window.
    pub fn show(monitors: &[MonitorInfo], color: Rgba) -> Self {
        #[cfg(windows)]
        {
            let windows = monitors
                .iter()
                .filter_map(|monitor| windows::DimWindow::new(monitor, color))
                .collect();

            Self { color, windows }
        }

        #[cfg(not(windows))]
        {
            let _ = monitors;
            Self { color }
        }
    }

    /// The dim colour.
    pub const fn color(&self) -> Rgba {
        self.color
    }

    /// Number of windows that were created successfully.
    pub fn len(&self) -> usize {
        #[cfg(windows)]
        {
            self.windows.len()
        }

        #[cfg(not(windows))]
        {
            0
        }
    }

    /// Whether no window was created.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Native window diagnostics (empty on non-Windows targets).
    pub fn window_info(&self) -> Vec<WindowInfo> {
        #[cfg(windows)]
        {
            self.windows.iter().map(windows::DimWindow::info).collect()
        }

        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}

impl Drop for Dims {
    fn drop(&mut self) {
        #[cfg(windows)]
        self.windows.clear();
    }
}
