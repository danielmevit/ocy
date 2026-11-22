//! Screen enumeration via `CGGetActiveDisplayList` with the backing scale factor.

use damt_platform::{ActiveScreenProvider, ScreenRect};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayPixelsHigh, CGDisplayPixelsWide, CGError,
    CGGetActiveDisplayList, CGMainDisplayID,
};

use crate::pure::DisplayGeometry;

/// Upper bound for the display list; a Mac with more than 32 active displays is not a
/// supported configuration (the API reports the true count, we just cap the query).
const MAX_DISPLAYS: usize = 32;

/// macOS [`ActiveScreenProvider`] backed by the CoreGraphics direct-display API.
///
/// [`ActiveScreenProvider::active_screen_bounds`] returns the **main display** (the one
/// with the menu bar, `CGMainDisplayID`) in physical pixels. Tracking the
/// foreground-window or cursor display needs window-server queries that M1 avoids;
/// that refinement is planned for M2 (see docs/rust/MACOS.md).
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
        // `CGMainDisplayID` is a thread-safe query with no arguments.
        let main_id = CGMainDisplayID();
        self.geometry(main_id)?.physical_rect()
    }
}
