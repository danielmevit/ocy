//! Settings model, allowed ranges, clamping, and JSON serialization.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// Serde helper: skips serialization for `false` booleans (used by `natural_moments`).
fn is_true(value: &bool) -> bool {
    *value
}

/// Configuration for short eye-rest breaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShortBreakSettings {
    /// Whether short breaks are enabled.
    pub enabled: bool,
    /// Active working interval between short breaks, in minutes (2–60).
    pub interval_minutes: i32,
    /// Duration of a short break, in seconds (5–120).
    pub duration_seconds: i32,
    /// Host display setting: dimmed fullscreen vs small corner window.
    pub fullscreen: bool,
}

impl ShortBreakSettings {
    /// Minimum allowed short-break interval in minutes.
    pub const MIN_INTERVAL_MINUTES: i32 = 2;
    /// Maximum allowed short-break interval in minutes.
    pub const MAX_INTERVAL_MINUTES: i32 = 60;
    /// Minimum allowed short-break duration in seconds.
    pub const MIN_DURATION_SECONDS: i32 = 5;
    /// Maximum allowed short-break duration in seconds.
    pub const MAX_DURATION_SECONDS: i32 = 120;

    /// Returns the short-break interval as a [`Duration`].
    #[must_use]
    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::try_from(self.interval_minutes.max(0)).unwrap_or(0) * 60)
    }

    /// Returns the short-break duration as a [`Duration`].
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_secs(u64::try_from(self.duration_seconds.max(0)).unwrap_or(0))
    }

    /// Clamps numeric fields into their allowed ranges.
    pub fn clamp(&mut self) {
        self.interval_minutes = self
            .interval_minutes
            .clamp(Self::MIN_INTERVAL_MINUTES, Self::MAX_INTERVAL_MINUTES);
        self.duration_seconds = self
            .duration_seconds
            .clamp(Self::MIN_DURATION_SECONDS, Self::MAX_DURATION_SECONDS);
    }
}

impl Default for ShortBreakSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 10,
            duration_seconds: 15,
            fullscreen: false,
        }
    }
}

/// Configuration for long breaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LongBreakSettings {
    /// Whether long breaks are enabled.
    pub enabled: bool,
    /// Active working interval between long breaks, in minutes (15–240).
    pub interval_minutes: i32,
    /// Duration of a long break, in minutes (1–30).
    pub duration_minutes: i32,
    /// Pre-break warning countdown, in seconds (0–300; 0 disables warning).
    pub warning_seconds: i32,
    /// How long a postpone delays the long break, in minutes (1–30).
    pub postpone_minutes: i32,
    /// Maximum number of postpones allowed per long-break cycle (0–10).
    pub max_postpones: i32,
}

impl LongBreakSettings {
    /// Minimum allowed long-break interval in minutes.
    pub const MIN_INTERVAL_MINUTES: i32 = 15;
    /// Maximum allowed long-break interval in minutes.
    pub const MAX_INTERVAL_MINUTES: i32 = 240;
    /// Minimum allowed long-break duration in minutes.
    pub const MIN_DURATION_MINUTES: i32 = 1;
    /// Maximum allowed long-break duration in minutes.
    pub const MAX_DURATION_MINUTES: i32 = 30;
    /// Minimum allowed warning countdown in seconds.
    pub const MIN_WARNING_SECONDS: i32 = 0;
    /// Maximum allowed warning countdown in seconds.
    pub const MAX_WARNING_SECONDS: i32 = 300;
    /// Minimum allowed postpone duration in minutes.
    pub const MIN_POSTPONE_MINUTES: i32 = 1;
    /// Maximum allowed postpone duration in minutes.
    pub const MAX_POSTPONE_MINUTES: i32 = 30;
    /// Minimum allowed postpone limit.
    pub const MIN_MAX_POSTPONES: i32 = 0;
    /// Maximum allowed postpone limit.
    pub const MAX_MAX_POSTPONES: i32 = 10;

    /// Returns the long-break interval as a [`Duration`].
    #[must_use]
    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::try_from(self.interval_minutes.max(0)).unwrap_or(0) * 60)
    }

    /// Returns the long-break duration as a [`Duration`].
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_secs(u64::try_from(self.duration_minutes.max(0)).unwrap_or(0) * 60)
    }

    /// Returns the warning countdown duration as a [`Duration`].
    #[must_use]
    pub fn warning_duration(&self) -> Duration {
        Duration::from_secs(u64::try_from(self.warning_seconds.max(0)).unwrap_or(0))
    }

    /// Returns the postpone delay duration as a [`Duration`].
    #[must_use]
    pub fn postpone_duration(&self) -> Duration {
        Duration::from_secs(u64::try_from(self.postpone_minutes.max(0)).unwrap_or(0) * 60)
    }

    /// Clamps numeric fields into their allowed ranges.
    pub fn clamp(&mut self) {
        self.interval_minutes = self
            .interval_minutes
            .clamp(Self::MIN_INTERVAL_MINUTES, Self::MAX_INTERVAL_MINUTES);
        self.duration_minutes = self
            .duration_minutes
            .clamp(Self::MIN_DURATION_MINUTES, Self::MAX_DURATION_MINUTES);
        self.warning_seconds = self
            .warning_seconds
            .clamp(Self::MIN_WARNING_SECONDS, Self::MAX_WARNING_SECONDS);
        self.postpone_minutes = self
            .postpone_minutes
            .clamp(Self::MIN_POSTPONE_MINUTES, Self::MAX_POSTPONE_MINUTES);
        self.max_postpones = self
            .max_postpones
            .clamp(Self::MIN_MAX_POSTPONES, Self::MAX_MAX_POSTPONES);
    }
}

impl Default for LongBreakSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 60,
            duration_minutes: 5,
            warning_seconds: 60,
            postpone_minutes: 3,
            max_postpones: 3,
        }
    }
}

/// Top-level user settings for Ocy (`settings.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OcySettings {
    /// JSON schema version (currently `1`).
    pub schema_version: i32,
    /// Short-break settings.
    #[serde(deserialize_with = "deserialize_null_default")]
    pub short_breaks: ShortBreakSettings,
    /// Long-break settings.
    #[serde(deserialize_with = "deserialize_null_default")]
    pub long_breaks: LongBreakSettings,
    /// Refuses skipping or postponing long breaks when `true`.
    pub strict_mode: bool,
    /// Enables opt-in break sounds.
    pub sounds: bool,
    /// Enables automatic away detection via OS idle time.
    pub inactivity_tracking: bool,
    /// Idle duration required before entering `Away`, in seconds (30–1800).
    pub away_threshold_seconds: i32,
    /// Defers breaks while a fullscreen or presentation app is active.
    pub pause_during_fullscreen: bool,
    /// Whether window-looking eye exercises are eligible.
    pub has_window_nearby: bool,
    /// Enables status notifications.
    pub status_notifications: bool,
    /// Starts Ocy automatically on sign-in.
    pub start_with_system: bool,
    /// Waits for a short lull in keyboard/mouse input (up to 60 s) before a scheduled
    /// break, warning, or long break starts, so it never interrupts mid-sentence.
    ///
    /// Defaults to `true` and is omitted from the JSON while `true`, so files Rust writes
    /// with defaults stay byte-identical to the C# app's (the C# reader ignores unknown
    /// properties).
    #[serde(skip_serializing_if = "is_true")]
    pub natural_moments: bool,
    /// BCP-47 language tag (e.g. `"en"`, `"mk"`).
    #[serde(deserialize_with = "deserialize_null_default")]
    pub language: String,
}

impl OcySettings {
    /// Minimum allowed away threshold in seconds.
    pub const MIN_AWAY_THRESHOLD_SECONDS: i32 = 30;
    /// Maximum allowed away threshold in seconds.
    pub const MAX_AWAY_THRESHOLD_SECONDS: i32 = 1800;

    /// Returns the away threshold as a [`Duration`].
    #[must_use]
    pub fn away_threshold(&self) -> Duration {
        Duration::from_secs(u64::try_from(self.away_threshold_seconds.max(0)).unwrap_or(0))
    }

    /// Clamps all settings into their valid ranges and fills a blank language with `"en"`.
    pub fn clamp(&mut self) {
        self.short_breaks.clamp();
        self.long_breaks.clamp();
        self.away_threshold_seconds = self.away_threshold_seconds.clamp(
            Self::MIN_AWAY_THRESHOLD_SECONDS,
            Self::MAX_AWAY_THRESHOLD_SECONDS,
        );
        if self.language.trim().is_empty() {
            self.language = "en".to_owned();
        }
    }
}

impl Default for OcySettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            short_breaks: ShortBreakSettings::default(),
            long_breaks: LongBreakSettings::default(),
            strict_mode: false,
            sounds: false,
            inactivity_tracking: true,
            away_threshold_seconds: 120,
            pause_during_fullscreen: true,
            has_window_nearby: true,
            status_notifications: true,
            start_with_system: false,
            natural_moments: true,
            language: "en".to_owned(),
        }
    }
}
