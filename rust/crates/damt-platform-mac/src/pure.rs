//! Pure macOS platform helpers: idle-time conversion, notification-name mapping,
//! `AppleInterfaceStyle` and reduce-motion parsing, display scale maths, and
//! frontmost-app observation mapping.
//!
//! Nothing here touches an Apple framework, so it compiles and is unit-tested on
//! every target (the macOS OS layer in [`crate::macos`] does the framework calls).

use std::time::Duration;

use damt_platform::{ForegroundObservation, ScreenRect, SessionEvent};

/// `kCGAnyInputEventType` (`(CGEventType)~0`): any keyboard, mouse, or tablet event.
///
/// The objc2-core-graphics bindings skip this constant, so it is defined here from the
/// documented value. Passed to `CGEventSourceSecondsSinceLastEventType` together with
/// `CGEventSourceStateID::CombinedSessionState` (`kCGEventSourceStateCombinedSessionState`).
pub const K_CG_ANY_INPUT_EVENT_TYPE: u32 = u32::MAX;

/// Distributed notification posted by the loginwindow/SessionFoundation when the
/// screen is locked (documented in docs/rust/MACOS.md; verify on hardware).
pub const SCREEN_IS_LOCKED_NOTIFICATION: &str = "com.apple.screenIsLocked";

/// Distributed notification posted when the screen is unlocked.
pub const SCREEN_IS_UNLOCKED_NOTIFICATION: &str = "com.apple.screenIsUnlocked";

/// `NSWorkspaceWillSleepNotification` name (the workspace notification center posts it).
pub const WORKSPACE_WILL_SLEEP_NOTIFICATION: &str = "NSWorkspaceWillSleepNotification";

/// `NSWorkspaceDidWakeNotification` name.
pub const WORKSPACE_DID_WAKE_NOTIFICATION: &str = "NSWorkspaceDidWakeNotification";

/// Pure idle-duration conversion for `CGEventSourceSecondsSinceLastEventType`.
#[derive(Debug, Default, Clone, Copy)]
pub struct IdleMath;

impl IdleMath {
    /// Converts the `CFTimeInterval` seconds reported by
    /// `CGEventSourceSecondsSinceLastEventType` into a [`Duration`].
    ///
    /// Negative results (no event of the type has been received yet) and NaN become
    /// zero; positive values too large for a [`Duration`] (including infinity)
    /// saturate at [`Duration::MAX`].
    #[must_use]
    pub fn from_seconds(seconds: f64) -> Duration {
        if seconds.is_nan() || seconds <= 0.0 {
            return Duration::ZERO;
        }
        // Duration::from_secs_f64 returns an error only on negative/NaN/overflow,
        // all of which are handled above; saturate instead of failing.
        match Duration::try_from_secs_f64(seconds) {
            Ok(duration) => duration,
            Err(_) => Duration::MAX,
        }
    }
}

/// Maps the `AppleInterfaceStyle` preference value onto "is light appearance".
///
/// `None` (the key is absent) means the light appearance: macOS only writes the key
/// when the user picks Dark (or an accent style that implies it). `"Dark"` maps to
/// dark, `"Light"` to light, and any other value is reported unknown rather than
/// guessed.
#[must_use]
pub fn parse_apple_interface_style(value: Option<&str>) -> Option<bool> {
    match value.map(str::trim) {
        None => Some(true),
        Some("Dark") => Some(false),
        Some("Light") => Some(true),
        Some(_) => None,
    }
}

/// Maps the `reduceMotion` preference (`com.apple.universalaccess` domain) onto
/// [`damt_platform::MotionPreference::prefers_reduced_motion`].
///
/// `None` (absent) and `Some(false)` mean motion is allowed; only an explicit
/// `Some(true)` reduces motion.
#[must_use]
pub fn parse_reduce_motion(value: Option<bool>) -> bool {
    matches!(value, Some(true))
}

/// Maps a distributed notification name onto a [`SessionEvent`] (`None`: not ours).
#[must_use]
pub fn map_distributed_notification(name: &str) -> Option<SessionEvent> {
    match name {
        SCREEN_IS_LOCKED_NOTIFICATION => Some(SessionEvent::Locked),
        SCREEN_IS_UNLOCKED_NOTIFICATION => Some(SessionEvent::Unlocked),
        _ => None,
    }
}

/// Maps an `NSWorkspace` notification name onto a [`SessionEvent`] (`None`: not ours).
#[must_use]
pub fn map_workspace_notification(name: &str) -> Option<SessionEvent> {
    match name {
        WORKSPACE_WILL_SLEEP_NOTIFICATION => Some(SessionEvent::Suspending),
        WORKSPACE_DID_WAKE_NOTIFICATION => Some(SessionEvent::Resumed),
        _ => None,
    }
}

/// Geometry of one CG direct display: global bounds in points plus the backing pixel
/// size, as returned by `CGDisplayBounds` / `CGDisplayPixelsWide` / `CGDisplayPixelsHigh`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayGeometry {
    /// Global x origin in points (top-left of the main display is (0, 0)).
    pub origin_x: f64,
    /// Global y origin in points.
    pub origin_y: f64,
    /// Width in points.
    pub width_points: f64,
    /// Height in points.
    pub height_points: f64,
    /// Backing width in physical pixels.
    pub pixel_width: usize,
    /// Backing height in physical pixels.
    pub pixel_height: usize,
}

impl DisplayGeometry {
    /// Backing scale factor (Retina: 2.0, non-scaled: 1.0, scaled: fractional).
    ///
    /// `None` when the point size is not positive or not finite, or the pixel size
    /// overflows the conversion.
    #[must_use]
    pub fn scale_factor(&self) -> Option<f64> {
        self.axis_scale(self.width_points, self.pixel_width)
    }

    /// The display bounds in physical screen pixels (the coordinate kind of the
    /// [`ScreenRect`] contract), or `None` when the geometry is unusable.
    ///
    /// Each axis uses its own scale (width/height can round differently on scaled
    /// displays); values are rounded to the nearest pixel and clamped into `i32`.
    #[must_use]
    pub fn physical_rect(&self) -> Option<ScreenRect> {
        if !self.origin_x.is_finite()
            || !self.origin_y.is_finite()
            || !self.width_points.is_finite()
            || !self.height_points.is_finite()
        {
            return None;
        }
        let scale_x = self.axis_scale(self.width_points, self.pixel_width)?;
        let scale_y = self.axis_scale(self.height_points, self.pixel_height)?;
        Some(ScreenRect {
            x: scaled_to_i32(self.origin_x, scale_x)?,
            y: scaled_to_i32(self.origin_y, scale_y)?,
            width: scaled_to_i32(self.width_points, scale_x)?,
            height: scaled_to_i32(self.height_points, scale_y)?,
        })
    }

    fn axis_scale(&self, points: f64, pixels: usize) -> Option<f64> {
        if !points.is_finite() || points <= 0.0 || pixels == 0 {
            return None;
        }
        let pixels = f64::from(u32::try_from(pixels).ok()?);
        Some(pixels / points)
    }
}

/// Scales `points` by `scale`, rounds to the nearest integer, and clamps into `i32`.
fn scaled_to_i32(points: f64, scale: f64) -> Option<i32> {
    let scaled = points * scale;
    if !scaled.is_finite() {
        return None;
    }
    let rounded = scaled.round();
    if rounded >= i32::MIN as f64 && rounded <= i32::MAX as f64 {
        Some(rounded as i32)
    } else {
        None
    }
}

/// Builds a [`ForegroundObservation`] from `NSWorkspace.frontmostApplication` data.
///
/// `exe_name` is the bundle identifier when present (e.g. `com.apple.Safari`), else the
/// localized application name; `exe_path` stays `None` because reading other processes'
/// paths needs the Accessibility permission, which M1 does not require. A negative pid
/// (none / error) yields `process_id: None`.
#[must_use]
pub fn frontmost_observation(
    bundle_id: Option<&str>,
    app_name: Option<&str>,
    pid: Option<i32>,
) -> ForegroundObservation {
    let exe_name = bundle_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .or_else(|| app_name.map(str::trim).filter(|name| !name.is_empty()))
        .map(str::to_owned);
    ForegroundObservation {
        notification_state: None,
        exe_name,
        exe_path: None,
        process_id: pid.and_then(|pid| u32::try_from(pid).ok()),
    }
}
