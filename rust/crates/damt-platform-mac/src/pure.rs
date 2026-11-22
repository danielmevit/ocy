//! Pure macOS platform helpers: idle-time conversion, notification-name mapping,
//! `AppleInterfaceStyle` and reduce-motion parsing, display scale and AppKit-to-CG
//! placement maths, frontmost-app mapping, `SMAppService` status/version checks,
//! tray menu mapping, and template-icon derivation.
//!
//! Nothing here touches an Apple framework, so it compiles and is unit-tested on
//! every target (the macOS OS layer in [`crate::macos`] does the framework calls).

use std::io::Cursor;
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

// --- M2: AppKit / CG placement maths ---

/// Rectangle in AppKit global screen coordinates (points, origin at bottom-left of main
/// screen, y increasing upwards), as returned by `NSScreen::frame` / `visibleFrame`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppKitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Converts an [`AppKitRect`] (y-up from bottom-left of main screen, in points) into a
/// physical-pixel [`ScreenRect`] (y-down from top-left of main screen) using
/// `main_screen_height_points` and `backing_scale_factor`.
#[must_use]
pub fn appkit_rect_to_cg_physical(
    rect: AppKitRect,
    main_screen_height_points: f64,
    backing_scale_factor: f64,
) -> Option<ScreenRect> {
    if !rect.x.is_finite()
        || !rect.y.is_finite()
        || !rect.width.is_finite()
        || !rect.height.is_finite()
        || !main_screen_height_points.is_finite()
        || !backing_scale_factor.is_finite()
        || rect.width <= 0.0
        || rect.height <= 0.0
        || main_screen_height_points <= 0.0
        || backing_scale_factor <= 0.0
    {
        return None;
    }

    let cg_y_points = main_screen_height_points - (rect.y + rect.height);
    Some(ScreenRect {
        x: scaled_to_i32(rect.x, backing_scale_factor)?,
        y: scaled_to_i32(cg_y_points, backing_scale_factor)?,
        width: scaled_to_i32(rect.width, backing_scale_factor)?,
        height: scaled_to_i32(rect.height, backing_scale_factor)?,
    })
}

/// Converts an AppKit `backingScaleFactor` (`1.0`, `2.0`, etc.) into an effective DPI (`96` = 1x).
#[must_use]
pub fn scale_to_dpi(backing_scale_factor: f64) -> u32 {
    if !backing_scale_factor.is_finite() || backing_scale_factor <= 0.0 {
        return 96;
    }
    let scaled = (backing_scale_factor * 96.0).round();
    if (48.0..=960.0).contains(&scaled) {
        scaled as u32
    } else {
        96
    }
}

/// Finds the index of the first screen in `frames` whose AppKit bounds contain `(px, py)`.
#[must_use]
pub fn select_screen_containing_point(frames: &[AppKitRect], px: f64, py: f64) -> Option<usize> {
    if !px.is_finite() || !py.is_finite() {
        return None;
    }
    frames
        .iter()
        .position(|f| px >= f.x && px < f.x + f.width && py >= f.y && py < f.y + f.height)
}

// --- M2: SMAppService autostart version & status mapping ---

/// macOS operating system version tuple (`NSProcessInfo.operatingSystemVersion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MacOsVersion {
    pub major: isize,
    pub minor: isize,
    pub patch: isize,
}

/// Minimum macOS major version supporting `SMAppService.mainApp` (macOS 13 Ventura).
pub const SM_APP_SERVICE_MIN_MAJOR: isize = 13;

/// Returns `true` when `SMAppService.mainApp` is available on this OS version and runtime.
#[must_use]
pub const fn is_sm_app_service_supported(version: MacOsVersion, class_available: bool) -> bool {
    version.major >= SM_APP_SERVICE_MIN_MAJOR && class_available
}

/// Typed `SMAppServiceStatus` values from `ServiceManagement/SMAppService.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SmAppServiceState {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
    Unknown(isize),
}

impl SmAppServiceState {
    /// Whether the login item is actively enabled (`SMAppServiceStatusEnabled`).
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Maps a raw `NSInteger` `SMAppServiceStatus` onto [`SmAppServiceState`].
#[must_use]
pub const fn map_sm_app_service_status(raw: isize) -> SmAppServiceState {
    match raw {
        0 => SmAppServiceState::NotRegistered,
        1 => SmAppServiceState::Enabled,
        2 => SmAppServiceState::RequiresApproval,
        3 => SmAppServiceState::NotFound,
        other => SmAppServiceState::Unknown(other),
    }
}

// --- M2: macOS menu-bar icon & menu presentation ---

const TRAY_ACTIVE_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-active-32.png");
const TRAY_PAUSED_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-paused-32.png");
const TRAY_ONBREAK_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-onbreak-32.png");
const TRAY_ATTENTION_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-attention-32.png");

/// Visual state of the macOS menu-bar tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacTrayIconState {
    Active,
    Paused,
    OnBreak,
    Attention,
}

/// Returns the embedded 32x32 tray PNG bytes used to derive the template icon for `state`.
#[must_use]
pub const fn tray_icon_png_bytes(state: MacTrayIconState) -> &'static [u8] {
    match state {
        MacTrayIconState::Active => TRAY_ACTIVE_PNG,
        MacTrayIconState::Paused => TRAY_PAUSED_PNG,
        MacTrayIconState::OnBreak => TRAY_ONBREAK_PNG,
        MacTrayIconState::Attention => TRAY_ATTENTION_PNG,
    }
}

/// Converts packed RGBA8 pixels `[R, G, B, A, ...]` into a monochrome black+alpha template mask
/// `[0, 0, 0, A, ...]` suitable for `NSImage::setTemplate(true)`.
#[must_use]
pub fn rgba_to_template_mask(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&[0, 0, 0, px[3]]);
    }
    out
}

/// Decodes a PNG slice into `(width, height, rgba8_bytes)`.
pub fn decode_png_to_rgba(png_bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder
        .read_info()
        .map_err(|err| format!("png header decode: {err}"))?;
    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| "png output buffer size overflow".to_owned())?;
    let mut buf = vec![0_u8; buf_size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|err| format!("png frame decode: {err}"))?;
    let bytes = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((bytes.len() / 3) * 4);
            for px in bytes.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity((bytes.len() / 2) * 4);
            for px in bytes.chunks_exact(2) {
                out.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(bytes.len() * 4);
            for &g in bytes {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        png::ColorType::Indexed => {
            return Err("unexpected indexed color after EXPAND transformation".to_owned());
        }
    };
    Ok((info.width, info.height, rgba))
}

/// Encodes packed RGBA8 pixels into a PNG byte buffer.
pub fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "png dimensions overflow".to_owned())?;
    if rgba.len() != expected {
        return Err(format!(
            "rgba length {} does not match {}x{}x4 ({})",
            rgba.len(),
            width,
            height,
            expected
        ));
    }

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|err| format!("png header encode: {err}"))?;
        writer
            .write_image_data(rgba)
            .map_err(|err| format!("png frame encode: {err}"))?;
    }
    Ok(out)
}

/// Derives a monochrome black+alpha template PNG from the embedded tray art for `state`.
pub fn template_tray_icon_png(state: MacTrayIconState) -> Result<Vec<u8>, String> {
    let (w, h, rgba) = decode_png_to_rgba(tray_icon_png_bytes(state))?;
    let mask = rgba_to_template_mask(&rgba);
    encode_rgba_png(w, h, &mask)
}

/// Menu slot identifiers matching `ocy-app/src/tray.rs` top-to-bottom order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacTrayMenuSlot {
    PauseStatus,
    TakeShortBreakNow,
    TakeLongBreakNow,
    HoldUntilGameCloses,
    Pause1Hour,
    Pause3Hours,
    PauseUntilResumed,
    Resume,
    Settings,
    Quit,
}

/// Presentation state of a single macOS tray menu item.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MacTrayMenuItem {
    pub text: String,
    pub is_visible: bool,
    pub is_enabled: bool,
    pub is_checked: bool,
}

/// Complete tray menu + tooltip presentation handed to `MacTrayHandle`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MacTrayPresentation {
    pub tooltip: String,
    pub pause_status: MacTrayMenuItem,
    pub take_short_break_now: MacTrayMenuItem,
    pub take_long_break_now: MacTrayMenuItem,
    pub hold_until_game_closes: MacTrayMenuItem,
    pub pause_1_hour: MacTrayMenuItem,
    pub pause_3_hours: MacTrayMenuItem,
    pub pause_until_resumed: MacTrayMenuItem,
    pub resume: MacTrayMenuItem,
    pub settings: MacTrayMenuItem,
    pub quit: MacTrayMenuItem,
}

/// User actions triggered from the macOS menu-bar menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacTrayAction {
    TakeShortBreakNow,
    TakeLongBreakNow,
    HoldUntilGameCloses,
    Pause1Hour,
    Pause3Hours,
    PauseUntilResumed,
    Resume,
    Settings,
    Quit,
}

/// Maps a [`MacTrayAction`] to a stable `NSMenuItem.tag` integer (`1..=9`).
#[must_use]
pub const fn tag_for_action(action: MacTrayAction) -> isize {
    match action {
        MacTrayAction::TakeShortBreakNow => 1,
        MacTrayAction::TakeLongBreakNow => 2,
        MacTrayAction::HoldUntilGameCloses => 3,
        MacTrayAction::Pause1Hour => 4,
        MacTrayAction::Pause3Hours => 5,
        MacTrayAction::PauseUntilResumed => 6,
        MacTrayAction::Resume => 7,
        MacTrayAction::Settings => 8,
        MacTrayAction::Quit => 9,
    }
}

/// Maps an `NSMenuItem.tag` integer back to [`MacTrayAction`].
#[must_use]
pub const fn action_for_tag(tag: isize) -> Option<MacTrayAction> {
    match tag {
        1 => Some(MacTrayAction::TakeShortBreakNow),
        2 => Some(MacTrayAction::TakeLongBreakNow),
        3 => Some(MacTrayAction::HoldUntilGameCloses),
        4 => Some(MacTrayAction::Pause1Hour),
        5 => Some(MacTrayAction::Pause3Hours),
        6 => Some(MacTrayAction::PauseUntilResumed),
        7 => Some(MacTrayAction::Resume),
        8 => Some(MacTrayAction::Settings),
        9 => Some(MacTrayAction::Quit),
        _ => None,
    }
}

/// One rendered entry in the macOS `NSMenu` (matching `TRAY_MENU_ORDER`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacTrayRenderedEntry {
    Item {
        slot: MacTrayMenuSlot,
        action: Option<MacTrayAction>,
        item: MacTrayMenuItem,
    },
    Separator,
}

/// Maps [`MacTrayPresentation`] into the ordered `NSMenu` entries (10 items + 2 separators).
///
/// Mirrors `SlintTraySink` for `HoldUntilGameCloses` visibility (`is_visible && is_enabled`),
/// keeps `PauseStatus` non-clickable (`action: None`, `is_enabled: false`), and preserves the
/// exact slot order from `ocy-app/src/tray.rs`.
#[must_use]
pub fn plan_tray_menu_entries(presentation: &MacTrayPresentation) -> Vec<MacTrayRenderedEntry> {
    let mut hold = presentation.hold_until_game_closes.clone();
    hold.is_visible = hold.is_visible && hold.is_enabled;

    vec![
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::PauseStatus,
            action: None,
            item: MacTrayMenuItem {
                text: presentation.pause_status.text.clone(),
                is_visible: presentation.pause_status.is_visible,
                is_enabled: false,
                is_checked: false,
            },
        },
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::TakeShortBreakNow,
            action: Some(MacTrayAction::TakeShortBreakNow),
            item: presentation.take_short_break_now.clone(),
        },
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::TakeLongBreakNow,
            action: Some(MacTrayAction::TakeLongBreakNow),
            item: presentation.take_long_break_now.clone(),
        },
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::HoldUntilGameCloses,
            action: Some(MacTrayAction::HoldUntilGameCloses),
            item: hold,
        },
        MacTrayRenderedEntry::Separator,
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::Pause1Hour,
            action: Some(MacTrayAction::Pause1Hour),
            item: presentation.pause_1_hour.clone(),
        },
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::Pause3Hours,
            action: Some(MacTrayAction::Pause3Hours),
            item: presentation.pause_3_hours.clone(),
        },
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::PauseUntilResumed,
            action: Some(MacTrayAction::PauseUntilResumed),
            item: presentation.pause_until_resumed.clone(),
        },
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::Resume,
            action: Some(MacTrayAction::Resume),
            item: presentation.resume.clone(),
        },
        MacTrayRenderedEntry::Separator,
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::Settings,
            action: Some(MacTrayAction::Settings),
            item: presentation.settings.clone(),
        },
        MacTrayRenderedEntry::Item {
            slot: MacTrayMenuSlot::Quit,
            action: Some(MacTrayAction::Quit),
            item: presentation.quit.clone(),
        },
    ]
}
