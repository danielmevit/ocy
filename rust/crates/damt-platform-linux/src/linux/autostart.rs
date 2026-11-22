use std::path::PathBuf;

use damt_platform::{AutostartService, PlatformError};

use crate::pure::{autostart_file_path, build_desktop_entry, parse_desktop_entry_enabled};

const DEFAULT_COMMENT: &str = "Calm break reminders for your eyes";
const DEFAULT_DESKTOP_ID: &str = "xyz.damt.ocy";

/// Per-user XDG autostart via `$XDG_CONFIG_HOME/autostart/<desktop_id>.desktop`.
#[derive(Debug, Clone)]
pub struct LinuxAutostart {
    app_name: String,
    desktop_id: String,
    exe_path: Result<String, String>,
    user_file: Option<PathBuf>,
}

impl LinuxAutostart {
    /// Creates an autostart service targeting `$XDG_CONFIG_HOME/autostart/xyz.damt.ocy.desktop`.
    pub fn new(app_name: impl Into<String>) -> Self {
        let app_name = app_name.into();
        let desktop_id = DEFAULT_DESKTOP_ID.to_owned();
        let exe_path = match std::env::current_exe() {
            Ok(path) => Ok(path.to_string_lossy().into_owned()),
            Err(err) => Err(format!("could not resolve the current executable: {err}")),
        };
        let user_file = autostart_file_path(
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
            &desktop_id,
        );
        Self {
            app_name,
            desktop_id,
            exe_path,
            user_file,
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
        self.user_file.is_some()
    }

    fn is_enabled(&self) -> bool {
        let Some(path) = &self.user_file else {
            return false;
        };
        if let Ok(contents) = std::fs::read_to_string(path) {
            return parse_desktop_entry_enabled(&contents);
        }
        false
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
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
            write_atomic(path, contents.as_bytes())
        } else if self.system_file_exists() {
            let exe = self
                .exe_path
                .as_ref()
                .map_err(|msg| PlatformError(msg.clone()))?;
            let contents =
                build_desktop_entry(&self.app_name, DEFAULT_COMMENT, &self.desktop_id, exe, true)
                    .map_err(|err| PlatformError(err.to_string()))?;
            write_atomic(path, contents.as_bytes())
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

fn write_atomic(path: &PathBuf, bytes: &[u8]) -> Result<(), PlatformError> {
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
