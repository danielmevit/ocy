use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use damt_platform::{AutostartService, PlatformError};
use futures_lite::{StreamExt, future};
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{AsyncDrop, MatchRule};

use crate::pure::{
    FLATPAK_INFO_PATH, autostart_file_path, build_background_portal_options, build_desktop_entry,
    is_flatpak_env, map_background_portal_results, parse_desktop_entry_enabled,
    portal_autostart_state_path, read_portal_autostart_state, write_file_atomic,
    write_portal_autostart_state,
};

const DEFAULT_COMMENT: &str = "Calm break reminders for your eyes";
const DEFAULT_DESKTOP_ID: &str = "xyz.damt.ocy";

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_BACKGROUND_IFACE: &str = "org.freedesktop.portal.Background";
const PORTAL_REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
const PORTAL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const PORTAL_ENABLE_REASON: &str =
    "Start Ocy automatically when you log in to remind you to take eye breaks.";
const PORTAL_DISABLE_REASON: &str = "Stop starting Ocy automatically when you log in.";

static PORTAL_HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Linux autostart service.
///
/// - **Outside Flatpak:** writes/removes `$XDG_CONFIG_HOME/autostart/<desktop_id>.desktop`.
/// - **Inside Flatpak (`FLATPAK_ID` set or `/.flatpak-info` present):** calls the XDG
///   `org.freedesktop.portal.Background.RequestBackground` portal via D-Bus.
///
/// # Limitation (Flatpak portal)
///
/// `org.freedesktop.portal.Background` provides no query method to inspect whether autostart is
/// currently enabled on the host. Inside Flatpak, [`AutostartService::is_enabled`] therefore
/// reads the last confirmed portal result cached in the app's sandbox config directory, and
/// reports `false` if no confirmed state has been recorded yet.
#[derive(Debug, Clone)]
pub struct LinuxAutostart {
    app_name: String,
    desktop_id: String,
    exe_path: Result<String, String>,
    user_file: Option<PathBuf>,
    use_portal: bool,
    portal_state_file: Option<PathBuf>,
}

impl LinuxAutostart {
    /// Creates an autostart service targeting `$XDG_CONFIG_HOME/autostart/xyz.damt.ocy.desktop`
    /// or, inside Flatpak, the XDG Background portal.
    pub fn new(app_name: impl Into<String>) -> Self {
        let app_name = app_name.into();
        let desktop_id = DEFAULT_DESKTOP_ID.to_owned();
        let exe_path = match std::env::current_exe() {
            Ok(path) => Ok(path.to_string_lossy().into_owned()),
            Err(err) => Err(format!("could not resolve the current executable: {err}")),
        };
        let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME");
        let home = std::env::var_os("HOME");
        let user_file =
            autostart_file_path(xdg_config_home.as_deref(), home.as_deref(), &desktop_id);
        let use_portal = is_flatpak_env(
            std::env::var_os("FLATPAK_ID").as_deref(),
            Path::new(FLATPAK_INFO_PATH),
        );
        let portal_state_file =
            portal_autostart_state_path(xdg_config_home.as_deref(), home.as_deref(), &app_name);
        Self {
            app_name,
            desktop_id,
            exe_path,
            user_file,
            use_portal,
            portal_state_file,
        }
    }

    /// Overrides the `.desktop` file stem (without `.desktop`), e.g. `"xyz.damt.ocy"`.
    #[must_use]
    pub fn with_desktop_id(mut self, desktop_id: impl Into<String>) -> Self {
        let id = desktop_id.into();
        if !id.trim().is_empty() {
            self.desktop_id = id;
            self.user_file = autostart_file_path(
                std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                std::env::var_os("HOME").as_deref(),
                &self.desktop_id,
            );
        }
        self
    }

    /// Creates an autostart service with an explicit `.desktop` path and executable (for tests).
    #[must_use]
    pub fn with_paths(
        app_name: impl Into<String>,
        desktop_id: impl Into<String>,
        user_file: PathBuf,
        exe_path: impl Into<String>,
    ) -> Self {
        Self {
            app_name: app_name.into(),
            desktop_id: desktop_id.into(),
            exe_path: Ok(exe_path.into()),
            user_file: Some(user_file),
            use_portal: false,
            portal_state_file: None,
        }
    }

    fn system_file_exists(&self) -> bool {
        let filename = if self.desktop_id.ends_with(".desktop") {
            self.desktop_id.clone()
        } else {
            format!("{}.desktop", self.desktop_id)
        };
        PathBuf::from("/etc/xdg/autostart").join(filename).is_file()
    }
}

impl AutostartService for LinuxAutostart {
    fn is_supported(&self) -> bool {
        if self.use_portal {
            self.portal_state_file.is_some()
        } else {
            self.user_file.is_some()
        }
    }

    fn is_enabled(&self) -> bool {
        if self.use_portal {
            return self
                .portal_state_file
                .as_deref()
                .is_some_and(read_portal_autostart_state);
        }

        let Some(path) = &self.user_file else {
            return false;
        };
        if let Ok(contents) = std::fs::read_to_string(path) {
            return parse_desktop_entry_enabled(&contents);
        }
        false
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        if self.use_portal {
            let state_path = self.portal_state_file.as_deref().ok_or_else(|| {
                PlatformError("could not resolve config directory for autostart state".to_owned())
            })?;
            request_background_portal_autostart(&self.app_name, enabled)?;
            return write_portal_autostart_state(state_path, enabled);
        }

        let Some(path) = &self.user_file else {
            return Ok(());
        };

        if enabled {
            let exe = self
                .exe_path
                .as_ref()
                .map_err(|msg| PlatformError(msg.clone()))?;
            let contents = build_desktop_entry(
                &self.app_name,
                DEFAULT_COMMENT,
                &self.desktop_id,
                exe,
                false,
            )
            .map_err(|err| PlatformError(err.to_string()))?;
            write_file_atomic(path, contents.as_bytes())
        } else if self.system_file_exists() {
            let exe = self
                .exe_path
                .as_ref()
                .map_err(|msg| PlatformError(msg.clone()))?;
            let contents =
                build_desktop_entry(&self.app_name, DEFAULT_COMMENT, &self.desktop_id, exe, true)
                    .map_err(|err| PlatformError(err.to_string()))?;
            write_file_atomic(path, contents.as_bytes())
        } else {
            match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(PlatformError(format!(
                    "could not remove autostart entry {}: {err}",
                    path.display()
                ))),
            }
        }
    }
}

fn request_background_portal_autostart(app_name: &str, enabled: bool) -> Result<(), PlatformError> {
    let conn = Connection::session().map_err(|err| {
        PlatformError(format!(
            "could not connect to session D-Bus for autostart portal: {err}"
        ))
    })?;

    // Subscribe to Request::Response before invoking RequestBackground so fast portal responses
    // cannot race ahead of our signal match rule.
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(PORTAL_REQUEST_IFACE)
        .and_then(|b| b.member("Response"))
        .map(zbus::match_rule::Builder::build)
        .map_err(|err| {
            PlatformError(format!(
                "failed to build D-Bus match rule for portal response: {err}"
            ))
        })?;
    let iter = MessageIterator::for_match_rule(rule, &conn, Some(8)).map_err(|err| {
        PlatformError(format!(
            "could not subscribe to desktop portal response signal: {err}"
        ))
    })?;

    let seq = PORTAL_HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let handle_token = format!("ocy_autostart_{}_{}", std::process::id(), seq);
    let reason = if enabled {
        PORTAL_ENABLE_REASON
    } else {
        PORTAL_DISABLE_REASON
    };
    let options = build_background_portal_options(enabled, &handle_token, app_name, reason);

    let proxy =
        Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, PORTAL_BACKGROUND_IFACE).map_err(|err| {
            PlatformError(format!(
                "could not create Background portal proxy on session D-Bus: {err}"
            ))
        })?;

    let request_path: OwnedObjectPath =
        proxy
            .call("RequestBackground", &("", &options))
            .map_err(|err| {
                PlatformError(format!(
                    "desktop portal RequestBackground call failed: {err}"
                ))
            })?;

    let deadline = Instant::now() + PORTAL_RESPONSE_TIMEOUT;
    let mut stream = iter.into_inner();
    async_io::block_on(async {
        let out = loop {
            if Instant::now() >= deadline {
                break Err(PlatformError(
                    "timed out waiting for desktop portal autostart response".to_owned(),
                ));
            }
            let next_msg = future::or(async { Some(stream.next().await) }, async {
                async_io::Timer::at(deadline).await;
                None
            })
            .await;

            match next_msg {
                None => {
                    break Err(PlatformError(
                        "timed out waiting for desktop portal autostart response".to_owned(),
                    ));
                }
                Some(None) => {
                    break Err(PlatformError(
                        "session D-Bus closed while waiting for desktop portal response".to_owned(),
                    ));
                }
                Some(Some(Err(err))) => {
                    break Err(PlatformError(format!(
                        "failed reading desktop portal response signal: {err}"
                    )));
                }
                Some(Some(Ok(msg))) => {
                    let matches_path = msg
                        .header()
                        .path()
                        .is_some_and(|p| p.as_str() == request_path.as_str());
                    if !matches_path {
                        continue;
                    }
                    break match msg
                        .body()
                        .deserialize::<(u32, HashMap<String, OwnedValue>)>()
                    {
                        Ok((response_code, results)) => {
                            map_background_portal_results(enabled, response_code, &results)
                        }
                        Err(err) => Err(PlatformError(format!(
                            "invalid desktop portal Response payload: {err}"
                        ))),
                    };
                }
            }
        };
        stream.async_drop().await;
        out
    })
}
