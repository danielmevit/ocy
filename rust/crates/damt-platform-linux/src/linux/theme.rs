use std::fmt;

use damt_platform::{AppThemeProvider, MotionPreference, TaskbarThemeProvider};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

use crate::pure::{MotionMapper, PortalColorScheme};

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SETTINGS_IFACE: &str = "org.freedesktop.portal.Settings";

/// Linux [`AppThemeProvider`] backed by the XDG Desktop Portal `color-scheme` setting.
pub struct LinuxAppTheme {
    conn: Option<Connection>,
}

impl fmt::Debug for LinuxAppTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxAppTheme")
            .field("supported", &self.conn.is_some())
            .finish()
    }
}

impl Default for LinuxAppTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxAppTheme {
    /// Connects to the session D-Bus to read `org.freedesktop.portal.Settings`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            conn: connect_portal_session(),
        }
    }
}

impl AppThemeProvider for LinuxAppTheme {
    fn is_supported(&self) -> bool {
        self.conn
            .as_ref()
            .and_then(read_color_scheme)
            .is_some_and(|raw| PortalColorScheme::to_is_light(raw).is_some())
    }

    fn is_light_app_theme(&self) -> Option<bool> {
        let raw = read_color_scheme(self.conn.as_ref()?)?;
        PortalColorScheme::to_is_light(raw)
    }
}

/// Linux [`TaskbarThemeProvider`] using the same XDG Desktop Portal `color-scheme` setting
/// so live tray icon set refreshes stay consistent with the desktop appearance.
pub struct LinuxTaskbarTheme {
    conn: Option<Connection>,
}

impl fmt::Debug for LinuxTaskbarTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxTaskbarTheme")
            .field("supported", &self.conn.is_some())
            .finish()
    }
}

impl Default for LinuxTaskbarTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxTaskbarTheme {
    /// Connects to the session D-Bus to read `org.freedesktop.portal.Settings`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            conn: connect_portal_session(),
        }
    }
}

impl TaskbarThemeProvider for LinuxTaskbarTheme {
    fn is_supported(&self) -> bool {
        self.conn
            .as_ref()
            .and_then(read_color_scheme)
            .is_some_and(|raw| PortalColorScheme::to_is_light(raw).is_some())
    }

    fn is_light_taskbar(&self) -> Option<bool> {
        let raw = read_color_scheme(self.conn.as_ref()?)?;
        PortalColorScheme::to_is_light(raw)
    }
}

/// Linux [`MotionPreference`] querying GNOME's `enable-animations` key via the Settings portal.
pub struct LinuxMotion {
    conn: Option<Connection>,
}

impl fmt::Debug for LinuxMotion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxMotion")
            .field("supported", &self.conn.is_some())
            .finish()
    }
}

impl Default for LinuxMotion {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxMotion {
    /// Connects to the session D-Bus to query `org.freedesktop.portal.Settings`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            conn: connect_portal_session(),
        }
    }
}

impl MotionPreference for LinuxMotion {
    fn is_supported(&self) -> bool {
        self.conn
            .as_ref()
            .and_then(read_enable_animations)
            .is_some()
    }

    fn prefers_reduced_motion(&self) -> bool {
        let enabled = self.conn.as_ref().and_then(read_enable_animations);
        MotionMapper::prefers_reduced(enabled)
    }
}

fn connect_portal_session() -> Option<Connection> {
    let conn = Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &(PORTAL_DEST,),
        )
        .ok()?;
    let has_owner = reply.body().deserialize::<bool>().unwrap_or(false);
    has_owner.then_some(conn)
}

fn read_color_scheme(conn: &Connection) -> Option<u32> {
    let val = read_portal_setting(conn, "org.freedesktop.appearance", "color-scheme")?;
    unwrap_u32(&val)
}

fn read_enable_animations(conn: &Connection) -> Option<bool> {
    let val = read_portal_setting(conn, "org.gnome.desktop.interface", "enable-animations")?;
    unwrap_bool(&val)
}

fn read_portal_setting(conn: &Connection, namespace: &str, key: &str) -> Option<OwnedValue> {
    if let Ok(reply) = conn.call_method(
        Some(PORTAL_DEST),
        PORTAL_PATH,
        Some(PORTAL_SETTINGS_IFACE),
        "ReadOne",
        &(namespace, key),
    ) && let Ok(val) = reply.body().deserialize::<OwnedValue>()
    {
        return Some(val);
    }

    let reply = conn
        .call_method(
            Some(PORTAL_DEST),
            PORTAL_PATH,
            Some(PORTAL_SETTINGS_IFACE),
            "Read",
            &(namespace, key),
        )
        .ok()?;
    reply.body().deserialize::<OwnedValue>().ok()
}

fn unwrap_u32(val: &Value<'_>) -> Option<u32> {
    match val {
        Value::U32(v) => Some(*v),
        Value::Value(inner) => unwrap_u32(inner),
        _ => None,
    }
}

fn unwrap_bool(val: &Value<'_>) -> Option<bool> {
    match val {
        Value::Bool(v) => Some(*v),
        Value::Value(inner) => unwrap_bool(inner),
        _ => None,
    }
}
