//! Screen enumeration via `NSScreen` + `CGGetActiveDisplayList` with work areas and scale factors.

use damt_platform::{ActiveScreenProvider, ScreenRect};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayPixelsHigh, CGDisplayPixelsWide, CGError,
    CGGetActiveDisplayList, CGMainDisplayID,
};

use crate::pure::{
    AppKitRect, DisplayGeometry, appkit_rect_to_cg_physical, scale_to_dpi,
    select_screen_containing_point,
};

/// Upper bound for the display list; a Mac with more than 32 active displays is not a
/// supported configuration (the API reports the true count, we just cap the query).
const MAX_DISPLAYS: usize = 32;

/// Physical monitor metadata resolved for macOS displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacMonitorInfo {
    /// Display index / identifier (0-based screen index on `NSScreen::screens`, or `CGDirectDisplayID`).
    pub id: u32,
    /// Full display bounds in physical screen pixels (y-down from main display top-left).
    pub rect: ScreenRect,
    /// Usable work area excluding menu bar and Dock in physical screen pixels.
    pub work_area: ScreenRect,
    /// Effective DPI (`round(backingScaleFactor * 96)`).
    pub dpi: u32,
    /// Whether this is the primary (menu-bar) display.
    pub primary: bool,
}

/// Enumerates all active macOS screens with full bounds, work area (`visibleFrame`), and DPI.
///
/// Uses `NSScreen::screens` when called on the main thread (providing accurate Dock/menu-bar
/// `visibleFrame` work areas and `backingScaleFactor`), falling back to `CGGetActiveDisplayList`
/// when called off the main thread.
#[must_use]
pub fn query_mac_monitors() -> Vec<MacMonitorInfo> {
    if let Some(mtm) = MainThreadMarker::new() {
        let screens = NSScreen::screens(mtm);
        if !screens.is_empty() {
            let main_frame = screens.objectAtIndex(0).frame();
            let main_height = main_frame.size.height;
            let mut out = Vec::with_capacity(screens.len());
            for (idx, screen) in screens.iter().enumerate() {
                let scale = screen.backingScaleFactor();
                let frame = screen.frame();
                let visible = screen.visibleFrame();
                let Some(rect) = appkit_rect_to_cg_physical(
                    AppKitRect {
                        x: frame.origin.x,
                        y: frame.origin.y,
                        width: frame.size.width,
                        height: frame.size.height,
                    },
                    main_height,
                    scale,
                ) else {
                    continue;
                };
                let work_area = appkit_rect_to_cg_physical(
                    AppKitRect {
                        x: visible.origin.x,
                        y: visible.origin.y,
                        width: visible.size.width,
                        height: visible.size.height,
                    },
                    main_height,
                    scale,
                )
                .unwrap_or(rect);
                out.push(MacMonitorInfo {
                    id: u32::try_from(idx).unwrap_or(0),
                    rect,
                    work_area,
                    dpi: scale_to_dpi(scale),
                    primary: idx == 0,
                });
            }
            if !out.is_empty() {
                return out;
            }
        }
    }

    let main_id = CGMainDisplayID();
    MacActiveScreen::new()
        .displays()
        .into_iter()
        .filter_map(|(id, geom)| {
            let rect = geom.physical_rect()?;
            let dpi = geom.scale_factor().map_or(96, scale_to_dpi);
            Some(MacMonitorInfo {
                id,
                rect,
                work_area: rect,
                dpi,
                primary: id == main_id,
            })
        })
        .collect()
}

/// macOS [`ActiveScreenProvider`] backed by AppKit mouse location / `NSScreen` on the main
/// thread, falling back to `CGMainDisplayID`.
///
/// On the main thread, [`ActiveScreenProvider::active_screen_bounds`] prefers the screen under
/// the mouse cursor (`NSEvent::mouseLocation`), falling back to `NSScreen::mainScreen` and
/// finally `CGMainDisplayID`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacActiveScreen;

impl MacActiveScreen {
    /// Creates the screen provider.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Enumerates the active displays with their geometry (points and pixels).
    ///
    /// Empty when the display list cannot be read (headless SSH session, and so on).
    #[must_use]
    pub fn displays(&self) -> Vec<(CGDirectDisplayID, DisplayGeometry)> {
        let mut ids = [CGDirectDisplayID::default(); MAX_DISPLAYS];
        let mut count: u32 = 0;
        // SAFETY: `ids` is a valid array with `MAX_DISPLAYS` entries matching
        // `max_displays`, and `count` is a valid out-pointer.
        let status =
            unsafe { CGGetActiveDisplayList(MAX_DISPLAYS as u32, ids.as_mut_ptr(), &mut count) };
        if status != CGError::Success {
            return Vec::new();
        }
        let count = usize::try_from(count).unwrap_or(0);
        ids.iter()
            .take(count.min(MAX_DISPLAYS))
            .filter_map(|&id| self.geometry(id).map(|geometry| (id, geometry)))
            .collect()
    }

    /// Reads one display's geometry; `None` when the display id is not valid.
    #[must_use]
    pub fn geometry(&self, id: CGDirectDisplayID) -> Option<DisplayGeometry> {
        if id == CGDirectDisplayID::default() {
            return None;
        }
        // All three bindings wrap thread-safe read-only queries; an invalid id
        // returns zeroed geometry, which `DisplayGeometry::physical_rect` rejects.
        let bounds = CGDisplayBounds(id);
        let pixel_width = CGDisplayPixelsWide(id);
        let pixel_height = CGDisplayPixelsHigh(id);
        Some(DisplayGeometry {
            origin_x: bounds.origin.x,
            origin_y: bounds.origin.y,
            width_points: bounds.size.width,
            height_points: bounds.size.height,
            pixel_width,
            pixel_height,
        })
    }
}

impl ActiveScreenProvider for MacActiveScreen {
    fn is_supported(&self) -> bool {
        true
    }

    fn active_screen_bounds(&self) -> Option<ScreenRect> {
        if let Some(mtm) = MainThreadMarker::new() {
            let screens = NSScreen::screens(mtm);
            if !screens.is_empty() {
                let main_height = screens.objectAtIndex(0).frame().size.height;
                let frames: Vec<AppKitRect> = screens
                    .iter()
                    .map(|s| {
                        let f = s.frame();
                        AppKitRect {
                            x: f.origin.x,
                            y: f.origin.y,
                            width: f.size.width,
                            height: f.size.height,
                        }
                    })
                    .collect();

                let mouse = NSEvent::mouseLocation();
                let selected_idx = select_screen_containing_point(&frames, mouse.x, mouse.y)
                    .or_else(|| {
                        let main = NSScreen::mainScreen(mtm)?;
                        let mf = main.frame();
                        frames.iter().position(|f| {
                            (f.x - mf.origin.x).abs() < 0.5
                                && (f.y - mf.origin.y).abs() < 0.5
                                && (f.width - mf.size.width).abs() < 0.5
                                && (f.height - mf.size.height).abs() < 0.5
                        })
                    })
                    .unwrap_or(0);

                let screen = screens.objectAtIndex(selected_idx);
                let scale = screen.backingScaleFactor();
                let f = screen.frame();
                if let Some(rect) = appkit_rect_to_cg_physical(
                    AppKitRect {
                        x: f.origin.x,
                        y: f.origin.y,
                        width: f.size.width,
                        height: f.size.height,
                    },
                    main_height,
                    scale,
                ) {
                    return Some(rect);
                }
            }
        }

        // Fallback off-main-thread or if NSScreen list is empty: main display.
        let main_id = CGMainDisplayID();
        self.geometry(main_id)?.physical_rect()
    }
}
