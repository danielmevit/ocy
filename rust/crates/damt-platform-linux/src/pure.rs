//! Pure Linux platform helpers: selection order, `.desktop` autostart parsing/formatting,
//! D-Bus signal mapping, EWMH/RandR math, and tray icon decoding.
//!
//! No display server, D-Bus daemon, or filesystem root is required to test anything here.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use damt_platform::{FocusState, PlatformError, ScreenRect, SessionEvent};
use thiserror::Error;
use zbus::zvariant::{OwnedValue, Value};

const LIGHT_ACTIVE_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-active-32.png");
const LIGHT_PAUSED_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-paused-32.png");
const LIGHT_ONBREAK_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-onbreak-32.png");
const LIGHT_ATTENTION_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/light/png/tray-attention-32.png");

const DARK_ACTIVE_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/dark/png/tray-active-32.png");
const DARK_PAUSED_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/dark/png/tray-paused-32.png");
const DARK_ONBREAK_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/dark/png/tray-onbreak-32.png");
const DARK_ATTENTION_PNG: &[u8] =
    include_bytes!("../../../../assets/icons/tray/dark/png/tray-attention-32.png");

/// Visual state of the Linux tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxTrayIconState {
    Active,
    Paused,
    OnBreak,
    Attention,
}

/// Session and display-server environment variables used to choose platform backends.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdleEnv {
    pub display: Option<String>,
    pub wayland_display: Option<String>,
    pub session_type: Option<String>,
    pub current_desktop: Option<String>,
}

impl IdleEnv {
    /// Captures display/session environment variables from the running process.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            display: non_empty_env("DISPLAY"),
            wayland_display: non_empty_env("WAYLAND_DISPLAY"),
            session_type: non_empty_env("XDG_SESSION_TYPE"),
            current_desktop: non_empty_env("XDG_CURRENT_DESKTOP")
                .or_else(|| non_empty_env("DESKTOP_SESSION")),
        }
    }

    /// Returns `true` when the active graphical session is Wayland.
    #[must_use]
    pub fn is_wayland(&self) -> bool {
        let session = self
            .session_type
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase);
        match session.as_deref() {
            Some("wayland") => true,
            Some("x11") => false,
            _ => has_non_empty(self.wayland_display.as_deref()),
        }
    }

    /// Returns `true` when the active graphical session is X11 (and not Wayland).
    #[must_use]
    pub fn is_x11(&self) -> bool {
        !self.is_wayland() && has_non_empty(self.display.as_deref())
    }

    /// Returns `true` when `XDG_CURRENT_DESKTOP` / `DESKTOP_SESSION` indicates GNOME, or `None`
    /// when unknown.
    #[must_use]
    pub fn desktop_is_gnome(&self) -> Option<bool> {
        let desktop = self.current_desktop.as_deref()?.trim();
        if desktop.is_empty() {
            return None;
        }
        Some(desktop.to_ascii_lowercase().contains("gnome"))
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn has_non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|v| !v.trim().is_empty())
}

/// Available Linux idle-time backend kinds, in priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdleSourceKind {
    /// X11 XScreenSaver extension (`XScreenSaverQueryInfo`).
    X11ScreenSaver,
    /// GNOME Mutter D-Bus `org.gnome.Mutter.IdleMonitor` (`GetIdletime`).
    GnomeMutterIdleMonitor,
    /// Cross-compositor `ext-idle-notify-v1` Wayland protocol.
    ExtIdleNotify,
}

/// Computes the ordered list of idle backends to try for `env`.
///
/// - On X11 (`DISPLAY` set and not a Wayland session): tries [`IdleSourceKind::X11ScreenSaver`].
/// - On Wayland: never falls back to XWayland's XScreenSaver (which misses native Wayland input);
///   tries [`IdleSourceKind::GnomeMutterIdleMonitor`] on GNOME (or unknown Wayland desktops where
///   Mutter may still answer), and returns empty on explicitly non-GNOME Wayland desktops (e.g.
///   KDE/Sway).
/// - On headless / TTY sessions: returns empty (`Unsupported`).
#[must_use]
pub fn select_idle_source_order(env: &IdleEnv) -> Vec<IdleSourceKind> {
    if env.is_wayland() {
        return match env.desktop_is_gnome() {
            Some(true) => vec![
                IdleSourceKind::GnomeMutterIdleMonitor,
                IdleSourceKind::ExtIdleNotify,
            ],
            Some(false) | None => vec![IdleSourceKind::ExtIdleNotify],
        };
    }

    if env.is_x11() {
        return vec![IdleSourceKind::X11ScreenSaver];
    }

    Vec::new()
}

/// Probes candidate idle backends in [`select_idle_source_order`] order and returns the first
/// usable one, or `None` when none are usable.
pub fn probe_idle_source<F>(env: &IdleEnv, mut probe: F) -> Option<IdleSourceKind>
where
    F: FnMut(IdleSourceKind) -> bool,
{
    select_idle_source_order(env)
        .into_iter()
        .find(|&kind| probe(kind))
}

/// Pure idle-duration conversion helpers.
#[derive(Debug, Default, Clone, Copy)]
pub struct IdleMath;

impl IdleMath {
    /// Converts an XScreenSaver `ms_since_user_input` `u32` into [`Duration`].
    #[must_use]
    pub const fn from_x11_millis(ms: u32) -> Duration {
        Duration::from_millis(ms as u64)
    }

    /// Converts a Mutter `GetIdletime` `u64` millisecond count into [`Duration`].
    #[must_use]
    pub const fn from_mutter_millis(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }
}

/// Pure event-based mapping for the Wayland `ext-idle-notify-v1` protocol.
///
/// `IdleProvider::idle_time()` is a query for time since user input, while
/// `ext-idle-notify-v1` is threshold/event based. The notifier is armed with
/// a short timeout (1 second), then an `idled` event is mapped to an idle-since
/// instant by subtracting that timeout. An `resumed` event ends the idle span;
/// elapsed time before that event remains excluded once the user is active.
#[derive(Debug, Clone)]
pub struct ExtIdleMapping {
    idle_since: Option<std::time::Instant>,
}

impl ExtIdleMapping {
    #[must_use]
    pub fn new() -> Self {
        Self { idle_since: None }
    }

    pub fn on_idled(&mut self, now: std::time::Instant, notification_timeout: Duration) {
        self.idle_since = Some(now.checked_sub(notification_timeout).unwrap_or(now));
    }

    pub fn on_resumed(&mut self) {
        self.idle_since = None;
    }

    #[must_use]
    pub fn idle_time(&self, now: std::time::Instant) -> Duration {
        self.idle_since
            .map(|since| now.saturating_duration_since(since))
            .unwrap_or(Duration::ZERO)
    }
}

impl Default for ExtIdleMapping {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed D-Bus signal relevant to [`SessionEvent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbusSignal<'a> {
    /// `org.freedesktop.login1.Manager.PrepareForSleep(start: bool)`
    LogindPrepareForSleep { start: bool },
    /// `org.freedesktop.login1.Manager.PrepareForShutdown(start: bool)`
    LogindPrepareForShutdown { start: bool },
    /// `org.freedesktop.login1.Session.Lock()`
    LogindSessionLock { path: &'a str },
    /// `org.freedesktop.login1.Session.Unlock()`
    LogindSessionUnlock { path: &'a str },
    /// `org.freedesktop.ScreenSaver.ActiveChanged(active: bool)`
    ScreenSaverActiveChanged { active: bool },
}

/// Parses raw D-Bus signal metadata `(interface, member, object_path, first_bool)` into a
/// typed [`DbusSignal`], returning `None` for unrecognized or malformed signals.
#[must_use]
pub fn parse_raw_dbus_signal<'a>(
    interface: &str,
    member: &str,
    object_path: &'a str,
    first_bool: Option<bool>,
) -> Option<DbusSignal<'a>> {
    match (interface, member) {
        ("org.freedesktop.login1.Manager", "PrepareForSleep") => {
            Some(DbusSignal::LogindPrepareForSleep { start: first_bool? })
        }
        ("org.freedesktop.login1.Manager", "PrepareForShutdown") => {
            Some(DbusSignal::LogindPrepareForShutdown { start: first_bool? })
        }
        ("org.freedesktop.login1.Session", "Lock") => {
            Some(DbusSignal::LogindSessionLock { path: object_path })
        }
        ("org.freedesktop.login1.Session", "Unlock") => {
            Some(DbusSignal::LogindSessionUnlock { path: object_path })
        }
        ("org.freedesktop.ScreenSaver", "ActiveChanged") => {
            Some(DbusSignal::ScreenSaverActiveChanged {
                active: first_bool?,
            })
        }
        _ => None,
    }
}

/// Maps a [`DbusSignal`] to a [`SessionEvent`], filtering logind `Session` signals to
/// `expected_session_path` when one was resolved.
#[must_use]
pub fn map_dbus_signal(
    signal: &DbusSignal<'_>,
    expected_session_path: Option<&str>,
) -> Option<SessionEvent> {
    match *signal {
        DbusSignal::LogindPrepareForSleep { start: true } => Some(SessionEvent::Suspending),
        DbusSignal::LogindPrepareForSleep { start: false } => Some(SessionEvent::Resumed),
        DbusSignal::LogindPrepareForShutdown { start: true } => Some(SessionEvent::LoggingOff),
        DbusSignal::LogindPrepareForShutdown { start: false } => None,
        DbusSignal::LogindSessionLock { path } => {
            if matches_session_path(path, expected_session_path) {
                Some(SessionEvent::Locked)
            } else {
                None
            }
        }
        DbusSignal::LogindSessionUnlock { path } => {
            if matches_session_path(path, expected_session_path) {
                Some(SessionEvent::Unlocked)
            } else {
                None
            }
        }
        DbusSignal::ScreenSaverActiveChanged { active: true } => Some(SessionEvent::Locked),
        DbusSignal::ScreenSaverActiveChanged { active: false } => Some(SessionEvent::Unlocked),
    }
}

fn matches_session_path(signal_path: &str, expected: Option<&str>) -> bool {
    match expected {
        Some(expected_path) => signal_path == expected_path,
        None => true,
    }
}

/// Normalizes `XDG_SESSION_ID` to a non-empty trimmed session id.
#[must_use]
pub fn resolve_logind_session_id(xdg_session_id: Option<&str>) -> Option<&str> {
    xdg_session_id.map(str::trim).filter(|id| !id.is_empty())
}

/// Error returned when an XDG autostart `.desktop` entry cannot be constructed.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum AutostartDesktopError {
    #[error("the executable path must not be empty or whitespace")]
    EmptyPath,
    #[error("the executable path must not contain control characters")]
    ControlCharacter,
}

/// Resolves `$XDG_CONFIG_HOME/autostart` (falling back to `$HOME/.config/autostart`).
#[must_use]
pub fn autostart_dir(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(value) = xdg_config_home
        && !value.is_empty()
    {
        return Some(PathBuf::from(value).join("autostart"));
    }

    if let Some(value) = home
        && !value.is_empty()
    {
        return Some(PathBuf::from(value).join(".config").join("autostart"));
    }

    None
}

/// Resolves `$XDG_CONFIG_HOME/autostart/<desktop_id>.desktop`.
#[must_use]
pub fn autostart_file_path(
    xdg_config_home: Option<&OsStr>,
    home: Option<&OsStr>,
    desktop_id: &str,
) -> Option<PathBuf> {
    let dir = autostart_dir(xdg_config_home, home)?;
    let trimmed = desktop_id.trim();
    if trimmed.is_empty() {
        return None;
    }
    let filename = if trimmed.ends_with(".desktop") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.desktop")
    };
    Some(dir.join(filename))
}

/// Escapes an `Exec=` argument per the freedesktop Desktop Entry Specification.
pub fn escape_desktop_exec_arg(arg: &str) -> Result<String, AutostartDesktopError> {
    if arg.trim().is_empty() {
        return Err(AutostartDesktopError::EmptyPath);
    }
    if arg.chars().any(|c| c == '\0' || c == '\n' || c == '\r') {
        return Err(AutostartDesktopError::ControlCharacter);
    }

    let needs_quoting = arg.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\''
                    | '\\'
                    | '>'
                    | '<'
                    | '~'
                    | '|'
                    | '&'
                    | ';'
                    | '$'
                    | '*'
                    | '?'
                    | '#'
                    | '('
                    | ')'
                    | '`'
            )
    });

    let mut escaped = String::with_capacity(arg.len() + 4);
    if needs_quoting {
        escaped.push('"');
    }
    for ch in arg.chars() {
        match ch {
            '%' => escaped.push_str("%%"),
            '"' | '$' | '`' | '\\' => {
                if needs_quoting {
                    escaped.push('\\');
                }
                escaped.push(ch);
            }
            c => escaped.push(c),
        }
    }
    if needs_quoting {
        escaped.push('"');
    }
    Ok(escaped)
}

/// Formats an XDG autostart `.desktop` file for Ocy.
pub fn build_desktop_entry(
    app_name: &str,
    comment: &str,
    icon: &str,
    exe_path: &str,
    hidden: bool,
) -> Result<String, AutostartDesktopError> {
    let exec_arg = escape_desktop_exec_arg(exe_path)?;
    let mut lines = vec![
        "[Desktop Entry]".to_owned(),
        "Type=Application".to_owned(),
        "Version=1.0".to_owned(),
        format!("Name={app_name}"),
        format!("Comment={comment}"),
        format!("Exec={exec_arg} --autostart"),
        format!("Icon={icon}"),
        "Terminal=false".to_owned(),
        "Categories=Utility;Accessibility;".to_owned(),
        "StartupNotify=false".to_owned(),
        "X-KDE-StartupNotify=false".to_owned(),
    ];
    if hidden {
        lines.push("Hidden=true".to_owned());
    } else {
        lines.push("X-GNOME-Autostart-enabled=true".to_owned());
    }
    lines.push(String::new());
    Ok(lines.join("\n"))
}

/// Parses an XDG autostart `.desktop` file and returns `true` when autostart is enabled.
///
/// Respects `Hidden=true` and `X-GNOME-Autostart-enabled=false` inside `[Desktop Entry]`.
#[must_use]
pub fn parse_desktop_entry_enabled(contents: &str) -> bool {
    let mut in_desktop_entry = false;
    let mut saw_desktop_entry = false;
    let mut is_application = true;
    let mut has_exec = false;
    let mut hidden = false;
    let mut gnome_enabled = true;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            if in_desktop_entry {
                saw_desktop_entry = true;
            }
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "Type" => {
                is_application = value == "Application";
            }
            "Exec" => {
                has_exec = !value.is_empty();
            }
            "Hidden" => {
                hidden = parse_desktop_bool(value).unwrap_or(false);
            }
            "X-GNOME-Autostart-enabled" => {
                gnome_enabled = parse_desktop_bool(value).unwrap_or(true);
            }
            _ => {}
        }
    }

    saw_desktop_entry && is_application && has_exec && !hidden && gnome_enabled
}

fn parse_desktop_bool(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("true") || value == "1" {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") || value == "0" {
        Some(false)
    } else {
        None
    }
}

/// Maps `org.freedesktop.appearance color-scheme` values from XDG Desktop Portal Settings.
///
/// Per the portal spec: `0` = no preference, `1` = prefer dark, `2` = prefer light.
#[derive(Debug, Default, Clone, Copy)]
pub struct PortalColorScheme;

impl PortalColorScheme {
    /// Converts a portal `color-scheme` `u32` into `is_light` (`Some(true)` = light,
    /// `Some(false)` = dark, `None` = no preference / unknown).
    #[must_use]
    pub const fn to_is_light(color_scheme: u32) -> Option<bool> {
        match color_scheme {
            1 => Some(false),
            2 => Some(true),
            _ => None,
        }
    }
}

/// Maps `org.gnome.desktop.interface enable-animations` into `MotionPreference::prefers_reduced_motion`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MotionMapper;

impl MotionMapper {
    /// Returns `true` only when `enable-animations` is explicitly `Some(false)`.
    #[must_use]
    pub const fn prefers_reduced(enable_animations: Option<bool>) -> bool {
        matches!(enable_animations, Some(false))
    }
}

/// Maps an X11 window's `_NET_WM_STATE` atom list to [`FocusState`].
#[must_use]
pub fn ewmh_has_fullscreen(states: &[u32], fullscreen_atom: u32) -> FocusState {
    if fullscreen_atom != 0 && states.contains(&fullscreen_atom) {
        FocusState::FullscreenApp
    } else {
        FocusState::Normal
    }
}

/// Selects the active monitor from `monitors` using `focus_rect` (center first, then largest
/// intersection), else `pointer`, else the first monitor.
#[must_use]
pub fn select_active_monitor(
    monitors: &[ScreenRect],
    focus_rect: Option<ScreenRect>,
    pointer: Option<(i32, i32)>,
) -> Option<ScreenRect> {
    if monitors.is_empty() {
        return None;
    }

    if let Some(rect) = focus_rect {
        let cx = rect.x.saturating_add(rect.width / 2);
        let cy = rect.y.saturating_add(rect.height / 2);
        if let Some(found) = monitor_containing_point(monitors, cx, cy) {
            return Some(found);
        }
        if let Some(best) = monitors
            .iter()
            .copied()
            .filter_map(|m| {
                let area = intersection_area(m, rect);
                (area > 0).then_some((area, m))
            })
            .max_by_key(|(area, _)| *area)
            .map(|(_, m)| m)
        {
            return Some(best);
        }
    }

    if let Some((px, py)) = pointer
        && let Some(found) = monitor_containing_point(monitors, px, py)
    {
        return Some(found);
    }

    monitors.first().copied()
}

fn monitor_containing_point(monitors: &[ScreenRect], x: i32, y: i32) -> Option<ScreenRect> {
    monitors.iter().copied().find(|m| {
        let right = m.x.saturating_add(m.width);
        let bottom = m.y.saturating_add(m.height);
        x >= m.x && x < right && y >= m.y && y < bottom
    })
}

fn intersection_area(a: ScreenRect, b: ScreenRect) -> i64 {
    let x1 = i64::from(a.x).max(i64::from(b.x));
    let y1 = i64::from(a.y).max(i64::from(b.y));
    let x2 = (i64::from(a.x) + i64::from(a.width)).min(i64::from(b.x) + i64::from(b.width));
    let y2 = (i64::from(a.y) + i64::from(a.height)).min(i64::from(b.y) + i64::from(b.height));
    (x2 - x1).max(0) * (y2 - y1).max(0)
}

/// Returns the embedded 32x32 PNG bytes from `assets/icons/tray` for `(state, use_light_set)`.
#[must_use]
pub const fn tray_icon_png_bytes(state: LinuxTrayIconState, use_light_set: bool) -> &'static [u8] {
    match (use_light_set, state) {
        (true, LinuxTrayIconState::Active) => LIGHT_ACTIVE_PNG,
        (true, LinuxTrayIconState::Paused) => LIGHT_PAUSED_PNG,
        (true, LinuxTrayIconState::OnBreak) => LIGHT_ONBREAK_PNG,
        (true, LinuxTrayIconState::Attention) => LIGHT_ATTENTION_PNG,
        (false, LinuxTrayIconState::Active) => DARK_ACTIVE_PNG,
        (false, LinuxTrayIconState::Paused) => DARK_PAUSED_PNG,
        (false, LinuxTrayIconState::OnBreak) => DARK_ONBREAK_PNG,
        (false, LinuxTrayIconState::Attention) => DARK_ATTENTION_PNG,
    }
}

/// Converts packed RGBA8 pixels `[R, G, B, A, ...]` into SNI ARGB32 network byte order `[A, R, G, B, ...]`.
#[must_use]
pub fn rgba_to_argb32(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
    }
    out
}

/// Decodes a PNG slice into `(width, height, argb32_data)` suitable for `ksni::Icon`.
pub fn decode_png_to_argb32(png_bytes: &[u8]) -> Result<(i32, i32, Vec<u8>), String> {
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
    let width = i32::try_from(info.width).map_err(|_| "png width overflow".to_owned())?;
    let height = i32::try_from(info.height).map_err(|_| "png height overflow".to_owned())?;
    Ok((width, height, rgba_to_argb32(&rgba)))
}

/// Decodes the embedded 32x32 tray PNG for `(state, use_light_set)` into SNI ARGB32 bytes.
pub fn decode_tray_icon_argb32(
    state: LinuxTrayIconState,
    use_light_set: bool,
) -> Result<(i32, i32, Vec<u8>), String> {
    decode_png_to_argb32(tray_icon_png_bytes(state, use_light_set))
}

/// Available Linux display-server session kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxDisplayBackend {
    /// Native X11 session (`DISPLAY` set and not Wayland).
    X11,
    /// Native Wayland session (`XDG_SESSION_TYPE=wayland` or `WAYLAND_DISPLAY` set).
    Wayland,
    /// Neither X11 nor Wayland detected (e.g. headless CI / TTY).
    Headless,
}

/// Selects the active Linux display-server backend from environment metadata.
#[must_use]
pub fn select_display_backend(env: &IdleEnv) -> LinuxDisplayBackend {
    if env.is_wayland() {
        LinuxDisplayBackend::Wayland
    } else if env.is_x11() {
        LinuxDisplayBackend::X11
    } else {
        LinuxDisplayBackend::Headless
    }
}

/// Pure window-behaviour policy derived from [`LinuxDisplayBackend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxWindowPolicy {
    pub backend: LinuxDisplayBackend,
    pub can_position_windows: bool,
    pub supports_always_on_top: bool,
    pub uses_x11_ewmh_hints: bool,
    pub uses_wayland_fullscreen_dim: bool,
    pub uses_edge_frames: bool,
}

impl LinuxWindowPolicy {
    /// Computes break-window capabilities for `backend` and whether an X11 compositing manager
    /// (`_NET_WM_CM_S<screen>`) is present.
    #[must_use]
    pub const fn for_backend(backend: LinuxDisplayBackend, x11_has_compositor: bool) -> Self {
        match backend {
            LinuxDisplayBackend::X11 => Self {
                backend,
                can_position_windows: true,
                supports_always_on_top: true,
                uses_x11_ewmh_hints: true,
                uses_wayland_fullscreen_dim: false,
                uses_edge_frames: x11_has_compositor,
            },
            LinuxDisplayBackend::Wayland => Self {
                backend,
                can_position_windows: false,
                supports_always_on_top: false,
                uses_x11_ewmh_hints: false,
                uses_wayland_fullscreen_dim: true,
                uses_edge_frames: false,
            },
            LinuxDisplayBackend::Headless => Self {
                backend,
                can_position_windows: false,
                supports_always_on_top: false,
                uses_x11_ewmh_hints: false,
                uses_wayland_fullscreen_dim: false,
                uses_edge_frames: false,
            },
        }
    }
}

/// X11 EWMH property atom for window role (`_NET_WM_WINDOW_TYPE`).
pub const X11_WM_WINDOW_TYPE_ATOM: &str = "_NET_WM_WINDOW_TYPE";
/// X11 EWMH notification window type (`_NET_WM_WINDOW_TYPE_NOTIFICATION`).
pub const X11_WM_WINDOW_TYPE_NOTIFICATION_ATOM: &str = "_NET_WM_WINDOW_TYPE_NOTIFICATION";
/// X11 EWMH window state property (`_NET_WM_STATE`).
pub const X11_WM_STATE_ATOM: &str = "_NET_WM_STATE";
/// X11 EWMH always-on-top state (`_NET_WM_STATE_ABOVE`).
pub const X11_WM_STATE_ABOVE_ATOM: &str = "_NET_WM_STATE_ABOVE";
/// X11 EWMH skip-taskbar state (`_NET_WM_STATE_SKIP_TASKBAR`).
pub const X11_WM_STATE_SKIP_TASKBAR_ATOM: &str = "_NET_WM_STATE_SKIP_TASKBAR";
/// X11 EWMH skip-pager state (`_NET_WM_STATE_SKIP_PAGER`).
pub const X11_WM_STATE_SKIP_PAGER_ATOM: &str = "_NET_WM_STATE_SKIP_PAGER";
/// X11 EWMH user-time property (`_NET_WM_USER_TIME`; `0` suppresses initial focus on map).
pub const X11_WM_USER_TIME_ATOM: &str = "_NET_WM_USER_TIME";

/// Required `_NET_WM_STATE` atom names for X11 break and overlay windows.
pub const X11_BREAK_STATE_ATOM_NAMES: [&str; 3] = [
    X11_WM_STATE_ABOVE_ATOM,
    X11_WM_STATE_SKIP_TASKBAR_ATOM,
    X11_WM_STATE_SKIP_PAGER_ATOM,
];

/// EWMH `_NET_WM_STATE_ADD` action value for `_NET_WM_STATE` client messages.
pub const NET_WM_STATE_ADD: u32 = 1;
/// EWMH normal-application source indication for `_NET_WM_STATE` client messages.
pub const NET_WM_SOURCE_APPLICATION: u32 = 1;

/// Merges `required` `_NET_WM_STATE` atoms into `existing` without duplicates and skipping `0`.
#[must_use]
pub fn merge_wm_state_atoms(existing: &[u32], required: &[u32]) -> Vec<u32> {
    let mut merged = Vec::with_capacity(existing.len() + required.len());
    for &atom in existing.iter().chain(required.iter()) {
        if atom != 0 && !merged.contains(&atom) {
            merged.push(atom);
        }
    }
    merged
}

/// Packs `_NET_WM_STATE` atoms into EWMH `_NET_WM_STATE` `ClientMessage` `data32` payloads
/// (at most two state atoms per message).
#[must_use]
pub fn net_wm_state_add_messages(atoms: &[u32]) -> Vec<[u32; 5]> {
    let valid: Vec<u32> = atoms.iter().copied().filter(|&a| a != 0).collect();
    valid
        .chunks(2)
        .map(|pair| {
            let first = pair[0];
            let second = pair.get(1).copied().unwrap_or(0);
            [
                NET_WM_STATE_ADD,
                first,
                second,
                NET_WM_SOURCE_APPLICATION,
                0,
            ]
        })
        .collect()
}

/// Raw X11 RandR CRTC geometry and connected output IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawX11Crtc {
    pub id: u32,
    pub rect: ScreenRect,
    pub outputs: Vec<u32>,
}

/// Resolved physical monitor metadata from X11 RandR + EWMH.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct X11MonitorInfo {
    pub id: u32,
    pub rect: ScreenRect,
    pub work_area: ScreenRect,
    pub dpi: u32,
    pub primary: bool,
}

/// Snapshot of X11 monitors and whether an EWMH compositing manager is active.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct X11MonitorSnapshot {
    pub monitors: Vec<X11MonitorInfo>,
    pub has_compositor: bool,
}

/// Parses `Xft.dpi` from X11 root `RESOURCE_MANAGER` text, returning a clamped DPI (`48..=480`).
#[must_use]
pub fn parse_xft_dpi(resource_manager: &str) -> Option<u32> {
    for raw_line in resource_manager.lines() {
        let line = raw_line.trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() != "Xft.dpi" {
            continue;
        }
        let parsed = value.trim().parse::<f64>().ok()?;
        if !parsed.is_finite() || parsed <= 0.0 {
            return None;
        }
        let rounded = parsed.round() as i64;
        if !(48..=480).contains(&rounded) {
            return None;
        }
        return u32::try_from(rounded).ok();
    }
    None
}

/// Extracts the `[x, y, width, height]` work area for `current_desktop` from `_NET_WORKAREA`.
#[must_use]
pub fn parse_net_workarea(values: &[u32], current_desktop: usize) -> Option<ScreenRect> {
    let base = current_desktop.checked_mul(4)?;
    let slice = values.get(base..base.checked_add(4)?)?;
    let x = i32::try_from(slice[0]).ok()?;
    let y = i32::try_from(slice[1]).ok()?;
    let width = i32::try_from(slice[2]).ok()?;
    let height = i32::try_from(slice[3]).ok()?;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(ScreenRect {
        x,
        y,
        width,
        height,
    })
}

/// Returns the intersection of two [`ScreenRect`]s when non-empty.
#[must_use]
pub fn intersect_screen_rect(a: ScreenRect, b: ScreenRect) -> Option<ScreenRect> {
    let left = i64::from(a.x).max(i64::from(b.x));
    let top = i64::from(a.y).max(i64::from(b.y));
    let right = (i64::from(a.x) + i64::from(a.width)).min(i64::from(b.x) + i64::from(b.width));
    let bottom = (i64::from(a.y) + i64::from(a.height)).min(i64::from(b.y) + i64::from(b.height));
    let width = right - left;
    let height = bottom - top;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(ScreenRect {
        x: i32::try_from(left).ok()?,
        y: i32::try_from(top).ok()?,
        width: i32::try_from(width).ok()?,
        height: i32::try_from(height).ok()?,
    })
}

/// Resolves RandR CRTCs, primary output, `_NET_WORKAREA`, and DPI into [`X11MonitorInfo`] entries.
#[must_use]
pub fn resolve_x11_monitors(
    crtcs: &[RawX11Crtc],
    primary_output: Option<u32>,
    net_workarea: Option<ScreenRect>,
    dpi: u32,
) -> Vec<X11MonitorInfo> {
    let effective_dpi = if dpi == 0 { 96 } else { dpi };
    let mut primary_matched = false;
    let mut out: Vec<X11MonitorInfo> = crtcs
        .iter()
        .filter(|c| c.rect.width > 0 && c.rect.height > 0)
        .map(|crtc| {
            let is_primary = primary_output
                .filter(|&p| p != 0)
                .is_some_and(|p| crtc.outputs.contains(&p));
            if is_primary {
                primary_matched = true;
            }
            let work_area = net_workarea
                .and_then(|wa| intersect_screen_rect(crtc.rect, wa))
                .unwrap_or(crtc.rect);
            X11MonitorInfo {
                id: crtc.id,
                rect: crtc.rect,
                work_area,
                dpi: effective_dpi,
                primary: is_primary,
            }
        })
        .collect();

    if !primary_matched && let Some(first) = out.first_mut() {
        first.primary = true;
    }
    out
}

/// Detects whether the process is running inside a Flatpak or Snap sandbox where owning a
/// well-known D-Bus name for StatusNotifierItem should be disabled.
#[must_use]
pub fn is_sandboxed_env(
    flatpak_id: Option<&str>,
    flatpak_info_exists: bool,
    proc_cgroup: Option<&str>,
) -> bool {
    if flatpak_info_exists || has_non_empty(flatpak_id) {
        return true;
    }

    proc_cgroup.is_some_and(|cgroups| {
        cgroups.lines().any(|line| {
            let Some((id, rest)) = line.split_once(':') else {
                return false;
            };
            let Some((controllers, path)) = rest.split_once(':') else {
                return false;
            };
            let snap_hierarchy = matches!(controllers, "" | "freezer" | "name=systemd");
            let snap_unit = path
                .rsplit('/')
                .next()
                .is_some_and(|unit| unit.starts_with("snap."));
            id.parse::<u32>().is_ok() && snap_hierarchy && snap_unit
        })
    })
}

/// Default path checked to detect a Flatpak sandbox container.
pub const FLATPAK_INFO_PATH: &str = "/.flatpak-info";

const PORTAL_AUTOSTART_STATE_FILENAME: &str = "autostart-portal.state";

/// Detects whether the current process is running inside Flatpak (`FLATPAK_ID` is non-empty
/// or `flatpak_info_path` exists on disk).
#[must_use]
pub fn is_flatpak_env(flatpak_id: Option<&OsStr>, flatpak_info_path: &Path) -> bool {
    let has_flatpak_id = flatpak_id.is_some_and(|v| !v.to_string_lossy().trim().is_empty());
    has_flatpak_id || flatpak_info_path.exists()
}

/// Resolves the portal autostart state file path inside the app's XDG config directory
/// (`$XDG_CONFIG_HOME/<appname>/autostart-portal.state`, falling back to
/// `$HOME/.config/<appname>/autostart-portal.state`, lowercased to match `damt_app::AppDataPaths`).
#[must_use]
pub fn portal_autostart_state_path(
    xdg_config_home: Option<&OsStr>,
    home: Option<&OsStr>,
    app_name: &str,
) -> Option<PathBuf> {
    let trimmed = app_name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let dir_name = trimmed.to_ascii_lowercase();
    if let Some(value) = xdg_config_home
        && !value.is_empty()
    {
        return Some(
            PathBuf::from(value)
                .join(dir_name)
                .join(PORTAL_AUTOSTART_STATE_FILENAME),
        );
    }
    if let Some(value) = home
        && !value.is_empty()
    {
        return Some(
            PathBuf::from(value)
                .join(".config")
                .join(dir_name)
                .join(PORTAL_AUTOSTART_STATE_FILENAME),
        );
    }
    None
}

/// Formats the confirmed Flatpak portal autostart state for local persistence.
#[must_use]
pub const fn format_portal_autostart_state(enabled: bool) -> &'static str {
    if enabled { "true\n" } else { "false\n" }
}

/// Parses a persisted Flatpak portal autostart state file.
#[must_use]
pub fn parse_portal_autostart_state(contents: &str) -> Option<bool> {
    let trimmed = contents.trim();
    if trimmed.eq_ignore_ascii_case("true")
        || trimmed == "1"
        || trimmed.eq_ignore_ascii_case("enabled")
    {
        Some(true)
    } else if trimmed.eq_ignore_ascii_case("false")
        || trimmed == "0"
        || trimmed.eq_ignore_ascii_case("disabled")
    {
        Some(false)
    } else {
        None
    }
}

/// Reads the persisted Flatpak portal autostart state, returning `false` if absent or invalid.
#[must_use]
pub fn read_portal_autostart_state(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| parse_portal_autostart_state(&contents))
        .unwrap_or(false)
}

/// Atomically writes `bytes` to `path` via a sibling temporary file and rename.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
    let Some(parent) = path.parent() else {
        return Err(PlatformError(format!(
            "autostart path has no parent directory: {}",
            path.display()
        )));
    };
    std::fs::create_dir_all(parent).map_err(|err| {
        PlatformError(format!(
            "could not create autostart directory {}: {err}",
            parent.display()
        ))
    })?;

    let tmp_name = format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("autostart"),
        std::process::id()
    );
    let tmp_path = parent.join(tmp_name);
    std::fs::write(&tmp_path, bytes).map_err(|err| {
        PlatformError(format!(
            "could not write temporary autostart entry {}: {err}",
            tmp_path.display()
        ))
    })?;
    std::fs::rename(&tmp_path, path).map_err(|err| {
        let _ = std::fs::remove_file(&tmp_path);
        PlatformError(format!(
            "could not install autostart entry {}: {err}",
            path.display()
        ))
    })
}

/// Atomically persists the confirmed Flatpak portal autostart state to `path`.
pub fn write_portal_autostart_state(path: &Path, enabled: bool) -> Result<(), PlatformError> {
    write_file_atomic(path, format_portal_autostart_state(enabled).as_bytes())
}

/// Builds the `a{sv}` options dictionary for `org.freedesktop.portal.Background.RequestBackground`.
///
/// Sets `autostart`, `dbus-activatable: false`, `handle_token`, `reason`, and (when enabling)
/// `commandline: [<command_name>]`.
#[must_use]
pub fn build_background_portal_options(
    enabled: bool,
    handle_token: &str,
    command_name: &str,
    reason: &str,
) -> HashMap<&'static str, Value<'static>> {
    let mut options = HashMap::new();
    options.insert("handle_token", Value::from(handle_token.to_owned()));
    options.insert("reason", Value::from(reason.to_owned()));
    options.insert("autostart", Value::from(enabled));
    options.insert("dbus-activatable", Value::from(false));
    if enabled {
        let cmd = if command_name.trim().is_empty() {
            "Ocy".to_owned()
        } else {
            command_name.to_owned()
        };
        options.insert("commandline", Value::from(vec![cmd]));
    }
    options
}

fn unwrap_zvariant_bool(val: &Value<'_>) -> Option<bool> {
    match val {
        Value::Bool(v) => Some(*v),
        Value::Value(inner) => unwrap_zvariant_bool(inner),
        _ => None,
    }
}

/// Extracts the `autostart` boolean from a `org.freedesktop.portal.Request::Response` results map.
#[must_use]
pub fn extract_portal_autostart_result(results: &HashMap<String, OwnedValue>) -> Option<bool> {
    results
        .get("autostart")
        .and_then(|v| unwrap_zvariant_bool(v))
}

/// Maps an `org.freedesktop.portal.Request::Response` code and `autostart` boolean into
/// `Result<(), PlatformError>`.
///
/// A non-zero response code, or `autostart: false` while enabling, returns a clear
/// [`PlatformError`].
pub fn map_background_portal_response(
    requested_enabled: bool,
    response_code: u32,
    autostart_granted: Option<bool>,
) -> Result<(), PlatformError> {
    match response_code {
        0 => {
            if requested_enabled && autostart_granted == Some(false) {
                Err(PlatformError(
                    "desktop portal denied autostart permission".to_owned(),
                ))
            } else {
                Ok(())
            }
        }
        1 => Err(PlatformError(
            "autostart permission request was cancelled".to_owned(),
        )),
        other => Err(PlatformError(format!(
            "autostart portal request failed (response code {other})"
        ))),
    }
}

/// Extracts `autostart` from `results` and maps the portal response via
/// [`map_background_portal_response`].
pub fn map_background_portal_results(
    requested_enabled: bool,
    response_code: u32,
    results: &HashMap<String, OwnedValue>,
) -> Result<(), PlatformError> {
    let autostart_granted = extract_portal_autostart_result(results);
    map_background_portal_response(requested_enabled, response_code, autostart_granted)
}
