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

/// Serde helper: skips serialization for `true` booleans (used by `natural_moments` and
/// `water_nudges`), so files written with defaults stay byte-identical to older ones.
fn is_true(value: &bool) -> bool {
    *value
}

/// How breaks behave while a game is the full-screen foreground app.
///
/// Missing, null, and unrecognised values deserialize as [`GamesMode::Gentle`], so an old
/// settings file or a bad field does not fail the load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GamesMode {
    /// Short breaks, and a brief long-break card, stay small. This is the default.
    #[default]
    Gentle,
    /// Hold breaks for every game, the way a presentation holds them.
    Hold,
}

impl GamesMode {
    fn is_gentle(mode: &Self) -> bool {
        matches!(mode, Self::Gentle)
    }
}

/// User consent for the anonymous usage statistics feature.
///
/// Missing, null, and unrecognised values deserialize as [`UsageStatsConsent::NotAsked`], so an
/// old settings file (or a future value an older build does not know) never fails the load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageStatsConsent {
    /// The user has not been asked yet (default; also used when the build has no analytics key).
    #[default]
    NotAsked,
    /// The user opted in; anonymous daily summaries may be sent.
    Granted,
    /// The user declined (or opted out); nothing is sent and any install record is deleted.
    Declined,
}

impl UsageStatsConsent {
    fn is_not_asked(consent: &Self) -> bool {
        matches!(consent, Self::NotAsked)
    }
}

fn deserialize_usage_stats_consent<'de, D>(deserializer: D) -> Result<UsageStatsConsent, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let consent = match value {
        serde_json::Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "granted" => UsageStatsConsent::Granted,
            "declined" => UsageStatsConsent::Declined,
            _ => UsageStatsConsent::NotAsked,
        },
        _ => UsageStatsConsent::NotAsked,
    };
    Ok(consent)
}

fn deserialize_games_mode<'de, D>(deserializer: D) -> Result<GamesMode, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let mode = match value {
        serde_json::Value::String(text) if text.trim().eq_ignore_ascii_case("hold") => {
            GamesMode::Hold
        }
        _ => GamesMode::Gentle,
    };
    Ok(mode)
}

fn deserialize_games_silent<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let names = match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                serde_json::Value::String(text) => Some(text),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(names)
}

/// Maximum number of silent-game exe names kept after clamping.
pub const MAX_SILENT_GAMES: usize = 64;

/// Maximum length of one silent-game exe file name.
pub const MAX_EXE_NAME_LEN: usize = 260;

/// File name of an exe path or bare name. Slashes, backslashes, and surrounding quotes are removed.
#[must_use]
pub fn exe_file_name(path_or_name: &str) -> String {
    let trimmed = path_or_name.trim().trim_matches('"').trim();
    trimmed
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(trimmed)
        .trim()
        .to_owned()
}

/// Case-insensitive exe file-name comparison. Paths are reduced to the file name first.
#[must_use]
pub fn exe_names_equal(left: &str, right: &str) -> bool {
    exe_file_name(left).eq_ignore_ascii_case(&exe_file_name(right))
}

fn clamp_games_silent(names: &mut Vec<String>) {
    let mut kept = Vec::new();
    for raw in names.drain(..) {
        let name = exe_file_name(&raw);
        if name.is_empty() || name.len() > MAX_EXE_NAME_LEN {
            continue;
        }
        if kept
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        kept.push(name);
        if kept.len() == MAX_SILENT_GAMES {
            break;
        }
    }
    *names = kept;
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
            interval_minutes: 25,
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
    /// JSON schema version (currently `2`).
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
    /// Lets long breaks occasionally suggest having a glass of water (never two long breaks
    /// in a row, no tracking or goals). Defaults to `true` and is omitted from the JSON while
    /// `true`, so files written with defaults stay byte-identical to older versions'.
    #[serde(skip_serializing_if = "is_true")]
    pub water_nudges: bool,
    /// BCP-47 language tag (e.g. `"en"`, `"mk"`).
    #[serde(deserialize_with = "deserialize_null_default")]
    pub language: String,
    /// Break behaviour for a full-screen game. Omitted from JSON while [`GamesMode::Gentle`].
    #[serde(
        default,
        skip_serializing_if = "GamesMode::is_gentle",
        deserialize_with = "deserialize_games_mode"
    )]
    pub games_mode: GamesMode,
    /// Exe file names that always hold breaks. Omitted from JSON while empty.
    ///
    /// Comparison is case-insensitive. Stored spelling is whatever the user (or Game Bar path)
    /// first supplied, after clamping.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "deserialize_games_silent"
    )]
    pub games_silent: Vec<String>,
    /// Consent for anonymous usage statistics. Omitted from JSON while [`UsageStatsConsent::NotAsked`],
    /// so files written with the default stay byte-identical to older versions'.
    #[serde(
        default,
        skip_serializing_if = "UsageStatsConsent::is_not_asked",
        deserialize_with = "deserialize_usage_stats_consent"
    )]
    pub usage_stats: UsageStatsConsent,
}

impl OcySettings {
    /// Current JSON schema version written by this version of Ocy.
    pub const CURRENT_SCHEMA_VERSION: i32 = 2;
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
        clamp_games_silent(&mut self.games_silent);
    }

    /// Whether `exe_name` is on the silent-game list (case-insensitive file name).
    #[must_use]
    pub fn is_silent_game(&self, exe_name: &str) -> bool {
        self.games_silent
            .iter()
            .any(|listed| exe_names_equal(listed, exe_name))
    }
}

impl Default for OcySettings {
    fn default() -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
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
            water_nudges: true,
            language: "en".to_owned(),
            games_mode: GamesMode::Gentle,
            games_silent: Vec::new(),
            usage_stats: UsageStatsConsent::NotAsked,
        }
    }
}
