use std::collections::HashMap;
use std::fmt;

use damt_platform::{Notifier, PlatformError};
use zbus::blocking::Connection;
use zbus::zvariant::Value;

const NOTIFY_DEST: &str = "org.freedesktop.Notifications";
const NOTIFY_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFY_IFACE: &str = "org.freedesktop.Notifications";

/// Linux [`Notifier`] calling `org.freedesktop.Notifications.Notify` over the session D-Bus.
pub struct LinuxNotifier {
    app_name: String,
    app_icon: String,
    conn: Option<Connection>,
}

impl fmt::Debug for LinuxNotifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxNotifier")
            .field("app_name", &self.app_name)
            .field("app_icon", &self.app_icon)
            .field("supported", &self.conn.is_some())
            .finish()
    }
}

impl LinuxNotifier {
    /// Creates a notifier targeting `org.freedesktop.Notifications` with `app_name` and `app_icon`.
    pub fn new(app_name: impl Into<String>, app_icon: impl Into<String>) -> Self {
        let conn = Connection::session().ok();
        Self {
            app_name: app_name.into(),
            app_icon: app_icon.into(),
            conn,
        }
    }
}

impl Notifier for LinuxNotifier {
    fn is_supported(&self) -> bool {
        self.conn.is_some()
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), PlatformError> {
        let Some(conn) = &self.conn else {
            return Ok(());
        };
        let actions: Vec<&str> = Vec::new();
        let hints: HashMap<&str, Value<'_>> = HashMap::new();
        let _ = conn
            .call_method(
                Some(NOTIFY_DEST),
                NOTIFY_PATH,
                Some(NOTIFY_IFACE),
                "Notify",
                &(
                    self.app_name.as_str(),
                    0_u32,
                    self.app_icon.as_str(),
                    title,
                    body,
                    actions,
                    hints,
                    -1_i32,
                ),
            )
            .map_err(|err| PlatformError(format!("org.freedesktop.Notifications.Notify: {err}")))?;
        Ok(())
    }
}
