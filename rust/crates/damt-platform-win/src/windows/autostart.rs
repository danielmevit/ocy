use damt_platform::{AutostartService, PlatformError};

use super::registry::{
    READ_ACCESS, RUN_KEY, WRITE_ACCESS, create_run_key, delete_value, open_key, read_string,
    write_string,
};
use crate::AutostartCommand;

/// Per-user autostart through the HKCU Run key (C# `WindowsAutostartService`).
#[derive(Debug, Clone)]
pub struct WinAutostart {
    value_name: String,
    command: Result<String, String>,
}

impl WinAutostart {
    /// Builds the stored command from the running executable; `value_name` is chosen by the app.
    pub fn new(value_name: impl Into<String>) -> Self {
        let command = match std::env::current_exe() {
            Ok(path) => {
                AutostartCommand::build(&path.to_string_lossy()).map_err(|error| error.to_string())
            }
            Err(error) => Err(format!("could not resolve the current executable: {error}")),
        };

        Self {
            value_name: value_name.into(),
            command,
        }
    }
}

impl AutostartService for WinAutostart {
    fn is_supported(&self) -> bool {
        true
    }

    fn is_enabled(&self) -> bool {
        let Ok(command) = &self.command else {
            return false;
        };

        let Some(key) = open_key(RUN_KEY, READ_ACCESS) else {
            return false;
        };
        let stored = read_string(&key, &self.value_name);

        stored.is_some_and(|value| value.to_lowercase() == command.to_lowercase())
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        let command = self
            .command
            .as_ref()
            .map_err(|message| PlatformError(message.clone()))?;

        if enabled {
            let Some(key) = create_run_key() else {
                return Err(PlatformError(
                    "could not open the Run key for writing".to_owned(),
                ));
            };
            write_string(&key, &self.value_name, command)
                .map_err(|error| PlatformError(format!("could not write the Run value: {error:?}")))
        } else {
            let Some(key) = open_key(RUN_KEY, WRITE_ACCESS) else {
                return Ok(());
            };
            delete_value(&key, &self.value_name).map_err(|error| {
                PlatformError(format!("could not delete the Run value: {error:?}"))
            })
        }
    }
}
