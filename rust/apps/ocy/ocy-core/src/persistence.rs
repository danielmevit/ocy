//! Atomic JSON persistence for `settings.json`, `state.json`, and `stats.json`.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use damt_platform::{Clock, SystemClock};
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::engine::EngineState;
use crate::settings::OcySettings;
use crate::statistics::StatisticsData;
use crate::time::DateTimeOffset;

/// Errors that can occur when persisting Ocy data files.
#[derive(Debug, Error)]
pub enum PersistenceError {
    /// I/O error while creating directories, writing `.tmp`, or renaming into place.
    #[error("persistence I/O error: {0}")]
    Io(#[from] io::Error),
    /// JSON serialization error.
    #[error("persistence serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// The line ending [`AppDataStore`] writes into the JSON data files.
///
/// The C# app writes with .NET's `Environment.NewLine`, which is CRLF on Windows and LF
/// elsewhere; [`NewlineStyle::Platform`] matches that so the files stay byte-identical
/// to the C# app's on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewlineStyle {
    /// The platform default: CRLF on Windows, LF elsewhere.
    #[default]
    Platform,
    /// LF line endings (Unix).
    Lf,
    /// CRLF line endings (Windows).
    Crlf,
}

impl NewlineStyle {
    /// Returns the byte sequence written for each newline.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Platform => {
                if cfg!(windows) {
                    "\r\n"
                } else {
                    "\n"
                }
            }
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

/// Manages `settings.json`, `state.json`, and `stats.json` inside a single directory.
pub struct AppDataStore {
    directory: PathBuf,
    clock: Arc<dyn Clock>,
    newline: NewlineStyle,
}

impl std::fmt::Debug for AppDataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppDataStore")
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

impl AppDataStore {
    /// Creates an `AppDataStore` for `directory` using the given [`Clock`], writing the
    /// platform newline ([`NewlineStyle::Platform`]).
    #[must_use]
    pub fn new(directory: impl Into<PathBuf>, clock: Arc<dyn Clock>) -> Self {
        Self::with_newline(directory, clock, NewlineStyle::Platform)
    }

    /// Creates an `AppDataStore` for `directory` using the given [`Clock`], writing the
    /// requested line ending. Use [`NewlineStyle::Lf`] to compare byte-for-byte against
    /// the LF fixtures written by the C# app on Linux.
    #[must_use]
    pub fn with_newline(
        directory: impl Into<PathBuf>,
        clock: Arc<dyn Clock>,
        newline: NewlineStyle,
    ) -> Self {
        Self {
            directory: directory.into(),
            clock,
            newline,
        }
    }

    /// Creates an `AppDataStore` for `directory` using [`SystemClock`], writing the
    /// platform newline ([`NewlineStyle::Platform`]).
    #[must_use]
    pub fn with_system_clock(directory: impl Into<PathBuf>) -> Self {
        Self::new(directory, Arc::new(SystemClock))
    }

    /// Returns the base directory path.
    #[must_use]
    pub fn directory_path(&self) -> &Path {
        &self.directory
    }

    /// Returns the path to `settings.json`.
    #[must_use]
    pub fn settings_path(&self) -> PathBuf {
        self.directory.join("settings.json")
    }

    /// Returns the path to `state.json`.
    #[must_use]
    pub fn state_path(&self) -> PathBuf {
        self.directory.join("state.json")
    }

    /// Returns the path to `stats.json`.
    #[must_use]
    pub fn stats_path(&self) -> PathBuf {
        self.directory.join("stats.json")
    }

    /// Loads `settings.json`, returning clamped defaults if missing, unreadable, or corrupt.
    ///
    /// Corrupt JSON files are moved to `settings.json.bak-<utc-timestamp>`.
    #[must_use]
    pub fn load_settings(&self) -> OcySettings {
        let mut settings: OcySettings = self.load_json_or_default(&self.settings_path());
        settings.clamp();
        settings
    }

    /// Clamps and atomically writes `settings.json`.
    pub fn save_settings(&self, settings: &OcySettings) -> Result<(), PersistenceError> {
        let mut clamped = settings.clone();
        clamped.clamp();
        self.save_json(&self.settings_path(), &clamped)
    }

    /// Loads `state.json`, returning defaults if missing, unreadable, or corrupt.
    ///
    /// Corrupt JSON files are moved to `state.json.bak-<utc-timestamp>`.
    #[must_use]
    pub fn load_state(&self) -> EngineState {
        self.load_json_or_default(&self.state_path())
    }

    /// Atomically writes `state.json`.
    pub fn save_state(&self, state: &EngineState) -> Result<(), PersistenceError> {
        self.save_json(&self.state_path(), state)
    }

    /// Loads `stats.json`, returning defaults if missing, unreadable, or corrupt.
    ///
    /// Corrupt JSON files are moved to `stats.json.bak-<utc-timestamp>`.
    #[must_use]
    pub fn load_stats(&self) -> StatisticsData {
        self.load_json_or_default(&self.stats_path())
    }

    /// Atomically writes `stats.json`.
    pub fn save_stats(&self, stats: &StatisticsData) -> Result<(), PersistenceError> {
        self.save_json(&self.stats_path(), stats)
    }

    fn load_json_or_default<T: Default + DeserializeOwned>(&self, path: &Path) -> T {
        if !path.exists() {
            return T::default();
        }

        let json = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => return T::default(),
        };

        match serde_json::from_str::<T>(&json) {
            Ok(value) => value,
            Err(_) => {
                self.backup_corrupt_file(path);
                T::default()
            }
        }
    }

    fn save_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), PersistenceError> {
        let json = serde_json::to_string_pretty(value)?;
        // `serde_json` never emits `\r`, so every newline can be rewritten in place.
        let json = json.replace('\n', self.newline.as_str());
        self.atomic_write(path, &json)?;
        Ok(())
    }

    fn atomic_write(&self, target_path: &Path, content: &str) -> Result<(), io::Error> {
        fs::create_dir_all(&self.directory)?;
        let mut tmp_os = OsString::from(target_path.as_os_str());
        tmp_os.push(".tmp");
        let tmp_path = PathBuf::from(tmp_os);

        fs::write(&tmp_path, content.as_bytes())?;
        fs::rename(&tmp_path, target_path)?;
        Ok(())
    }

    fn backup_corrupt_file(&self, file_path: &Path) {
        let timestamp = DateTimeOffset::from_utc(self.clock.now().timestamp()).format_backup_utc();
        let base_backup = append_suffix(file_path, &format!(".bak-{timestamp}"));

        let mut backup_path = base_backup;
        let mut counter = 1u32;
        while backup_path.exists() {
            backup_path = append_suffix(file_path, &format!(".bak-{timestamp}-{counter}"));
            counter = counter.saturating_add(1);
        }

        let _ = fs::rename(file_path, &backup_path);
    }
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut os = OsString::from(path.as_os_str());
    os.push(suffix);
    PathBuf::from(os)
}
