//! Anonymous, opt-in usage statistics: the consent record, daily-summary events, and the
//! background batch sender (owner decision of 2026-11-26).
//!
//! Nothing is sent until the user opts in. The whole feature is inert in builds compiled
//! without `OCY_POSTHOG_KEY` ([`build_key`] is `None`): no thread, no network,
//! [`is_available`] is `false` so the Settings toggle can be hidden.
//!
//! Each send attempt loads `stats.json` (day buckets) and `usage-stats.json` (install ID,
//! opt-in date, last accepted day), builds at most one `daily_summary` event per full local
//! day (strictly after opt-in, strictly before today, newer than the last accepted day, at
//! most [`MAX_DUE_DAYS`] most recent) plus the one-time `usage_stats_enabled` event, and
//! POSTs one batch to `{endpoint}/batch/`. On a network or 5xx error it retries next hour;
//! on a 4xx it logs once and stops for the run. Declining consent deletes
//! `usage-stats.json` (install ID and any pending days gone; a later yes gets a new ID).
//!
//! Consent is **not** read from or written to `settings.json` here. The in-memory
//! `OcySettings` owned by the app host is the single source of truth: the host calls
//! [`UsageStats::apply_consent`] after every settings load and save, which updates this
//! module's in-memory consent mirror and creates/deletes `usage-stats.json`. The sender
//! reads only the mirror, never the disk.
//!
//! The pure parts (day selection, event building, deterministic UUIDs) take their inputs as
//! parameters, so tests drive them without any network.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{JoinHandle, park_timeout};
use std::time::Duration;

use damt_platform::Clock;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::{GamesMode, OcySettings, UsageStatsConsent};
use ocy_core::statistics::DailyStatistics;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::license::{LICENSE_CACHE_FILE_NAME, LicenseCache, LicenseState};
use crate::presets::{self, Preset};

/// The PostHog project API key injected at build time. `None` disables the whole feature.
pub const BUILD_KEY: Option<&str> = option_env!("OCY_POSTHOG_KEY");

/// Optional build-time override of the capture endpoint. Defaults to [`DEFAULT_ENDPOINT`].
pub const BUILD_HOST: Option<&str> = option_env!("OCY_POSTHOG_HOST");

/// The default capture endpoint (PostHog EU cloud).
pub const DEFAULT_ENDPOINT: &str = "https://eu.i.posthog.com";

/// Name of the `daily_summary` event.
pub const DAILY_EVENT: &str = "daily_summary";

/// Name of the one-time opt-in event.
pub const ENABLED_EVENT: &str = "usage_stats_enabled";

/// At most this many due days are sent per batch; older due days are skipped, not sent.
pub const MAX_DUE_DAYS: usize = 14;

/// File name of the install record (`usage-stats.json`), next to the other data files.
pub const RECORD_FILE_NAME: &str = "usage-stats.json";

/// Delay from app start to the first send attempt.
pub const INITIAL_DELAY: Duration = Duration::from_secs(60);

/// Delay between send attempts while the app runs.
pub const RETRY_DELAY: Duration = Duration::from_secs(60 * 60);

/// Timeout for the whole HTTP request.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Debug/test-only endpoint override (`OCY_STATS_ENDPOINT`); release builds ignore it
/// (`debug_assertions` is off, and this constant does not exist there). Tests pass
/// endpoints through [`SenderConfig`] instead, so parallel tests never race on one
/// environment variable.
#[cfg(debug_assertions)]
const ENDPOINT_OVERRIDE_VAR: &str = "OCY_STATS_ENDPOINT";

/// Non-Store builds report `none` for the `license` property.
const NON_STORE_LICENSE: &str = "none";

/// Mirror encoding of [`UsageStatsConsent`] for the atomic the sender reads.
const CONSENT_NOT_ASKED: u8 = 0;
const CONSENT_GRANTED: u8 = 1;
const CONSENT_DECLINED: u8 = 2;

const fn consent_to_u8(consent: UsageStatsConsent) -> u8 {
    match consent {
        UsageStatsConsent::NotAsked => CONSENT_NOT_ASKED,
        UsageStatsConsent::Granted => CONSENT_GRANTED,
        UsageStatsConsent::Declined => CONSENT_DECLINED,
    }
}

const fn consent_from_u8(value: u8) -> UsageStatsConsent {
    match value {
        CONSENT_GRANTED => UsageStatsConsent::Granted,
        CONSENT_DECLINED => UsageStatsConsent::Declined,
        _ => UsageStatsConsent::NotAsked,
    }
}

/// Returns the build-time PostHog project API key, or `None` when the build has none.
#[must_use]
pub fn build_key() -> Option<&'static str> {
    BUILD_KEY.filter(|key| !key.is_empty())
}

/// Returns `true` when this build was compiled with an analytics key, so the feature (and
/// the Settings toggle) can be offered at all.
#[must_use]
pub fn is_available() -> bool {
    build_key().is_some()
}

/// Returns the capture endpoint base URL for this build: the debug-only override when set,
/// else the build host, else [`DEFAULT_ENDPOINT`]. Host and key are kept in sync: without a
/// key nothing ever connects anyway.
#[must_use]
pub fn build_endpoint() -> String {
    #[cfg(debug_assertions)]
    if let Some(overridden) = std::env::var(ENDPOINT_OVERRIDE_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return overridden.trim().trim_end_matches('/').to_owned();
    }
    endpoint_from_host(build_key().is_some(), BUILD_HOST)
}

/// Pure core of [`build_endpoint`]: a build host that is empty or only whitespace counts as
/// unset (an empty `OCY_POSTHOG_HOST=""` in CI must not produce a `"/batch/"` URL), and a
/// trailing slash is stripped.
fn endpoint_from_host(has_key: bool, build_host: Option<&str>) -> String {
    let host = if has_key {
        build_host
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .unwrap_or(DEFAULT_ENDPOINT)
    } else {
        DEFAULT_ENDPOINT
    };
    host.trim_end_matches('/').to_owned()
}

/// The persisted install record (`usage-stats.json`). Deleted when consent leaves `Granted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatsRecord {
    /// JSON schema version (currently `1`).
    pub schema_version: i32,
    /// Random UUID v4 install ID (no person, no machine data).
    pub install_id: String,
    /// Local date the user said yes. Nothing up to and including this date is ever sent.
    pub opted_in_date: Date,
    /// Last local date whose daily summary was accepted by the server.
    #[serde(default)]
    pub last_sent_date: Option<Date>,
    /// Whether the one-time `usage_stats_enabled` event was accepted by the server.
    #[serde(default)]
    pub enabled_event_sent: bool,
}

impl UsageStatsRecord {
    const SCHEMA_VERSION: i32 = 1;

    /// Creates a fresh record for a yes given on `today`, with a new random install ID.
    fn new(today: Date) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            install_id: Uuid::new_v4().to_string(),
            opted_in_date: today,
            last_sent_date: None,
            enabled_event_sent: false,
        }
    }
}

/// Selects which days are due for sending, oldest first.
///
/// A day qualifies when it is strictly after `opted_in_date`, strictly before `today`, and
/// (when present) strictly after `last_sent_date`. At most [`MAX_DUE_DAYS`] most recent
/// qualify; older ones are skipped, not sent.
#[must_use]
pub fn select_due_days(
    opted_in_date: Date,
    last_sent_date: Option<Date>,
    today: Date,
    candidate_days: &[Date],
) -> Vec<Date> {
    let mut due: Vec<Date> = candidate_days
        .iter()
        .copied()
        .filter(|day| *day > opted_in_date && *day < today)
        .filter(|day| last_sent_date.is_none_or(|last| *day > last))
        .collect();
    due.sort_unstable();
    due.dedup();
    if due.len() > MAX_DUE_DAYS {
        let excess = due.len() - MAX_DUE_DAYS;
        due.drain(..excess);
    }
    due
}

/// The platform slice of the event payload. Everything here is coarse and anonymous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub platform: &'static str,
    /// Coarse OS version: Windows `10.0.26100`, macOS `15.2`, Linux kernel major.minor.
    pub os_version: String,
    pub app_version: &'static str,
    pub channel: &'static str,
    pub license: &'static str,
    pub ui_language: String,
    /// OS locale tag, for example `hu-HU`.
    pub locale: String,
}

impl Environment {
    /// Gathers the environment slice from the OS plus the current settings and license tag.
    #[must_use]
    pub fn gather(settings: &OcySettings, license: &'static str) -> Self {
        Self {
            platform: platform_tag(),
            os_version: os_version(),
            app_version: env!("CARGO_PKG_VERSION"),
            channel: channel_tag(),
            license,
            ui_language: settings.language.clone(),
            locale: os_locale(),
        }
    }
}

const fn platform_tag() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
}

/// Distribution channel: `store`/`direct` (Windows), `flatpak`/`linux` (Linux), `macos`.
fn channel_tag() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(feature = "store") {
            "store"
        } else {
            "direct"
        }
    } else if cfg!(target_os = "linux") {
        // In a Flatpak the session sets `FLATPAK_ID` and the sandbox root holds /.flatpak-info.
        static IS_FLATPAK: OnceLock<bool> = OnceLock::new();
        let is_flatpak = *IS_FLATPAK.get_or_init(|| {
            std::env::var_os("FLATPAK_ID").is_some_and(|value| !value.is_empty())
                || Path::new("/.flatpak-info").exists()
        });
        if is_flatpak { "flatpak" } else { "linux" }
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
}

/// Coarse OS version, never containing host or user names.
fn os_version() -> String {
    if cfg!(target_os = "windows") {
        windows_version().unwrap_or_else(|| "unknown".to_owned())
    } else if cfg!(target_os = "linux") {
        linux_version()
    } else if cfg!(target_os = "macos") {
        macos_version().unwrap_or_else(|| "unknown".to_owned())
    } else {
        "unknown".to_owned()
    }
}

#[cfg(target_os = "windows")]
fn windows_version() -> Option<String> {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid, correctly sized OSVERSIONINFOW for the duration of the call.
    let status = unsafe { RtlGetVersion(&mut info) };
    if status.is_ok() {
        Some(format!(
            "{}.{}.{}",
            info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber
        ))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn windows_version() -> Option<String> {
    None
}

/// Linux: the kernel major.minor only.
fn linux_version() -> String {
    let release = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let mut parts = release.trim().split('.');
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) if !major.is_empty() => format!("{major}.{minor}"),
        _ => "unknown".to_owned(),
    }
}

#[cfg(target_os = "macos")]
fn macos_version() -> Option<String> {
    use objc2_foundation::NSProcessInfo;

    // Both NSProcessInfo accessors are safe to call (no unsafe block needed).
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    Some(format!("{}.{}", version.majorVersion, version.minorVersion))
}

#[cfg(not(target_os = "macos"))]
fn macos_version() -> Option<String> {
    None
}

/// OS locale tag, for example `hu-HU`.
fn os_locale() -> String {
    if cfg!(target_os = "windows")
        && let Some(tag) = windows_locale()
    {
        return tag;
    }
    if cfg!(target_os = "macos")
        && let Some(tag) = macos_locale()
    {
        return tag;
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(tag) = std::env::var(key).ok().and_then(|v| normalize_locale(&v)) {
            return tag;
        }
    }
    "unknown".to_owned()
}

#[cfg(target_os = "windows")]
fn windows_locale() -> Option<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85];
    // SAFETY: `buffer` is a valid, 85-element locale buffer per the API contract.
    let copied = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if copied <= 0 {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16(&buffer[..end]).ok()
}

#[cfg(not(target_os = "windows"))]
fn windows_locale() -> Option<String> {
    None
}

/// macOS: Finder/LaunchServices launches GUI apps without `LANG`/`LC_*` in the environment,
/// so ask the OS first. The identifier looks like `hu-HU` (or `hu_HU@latin` before
/// normalizing).
#[cfg(target_os = "macos")]
fn macos_locale() -> Option<String> {
    use objc2_foundation::NSLocale;

    // `localeIdentifier` returns something like `hu-HU` (or `hu_HU@latin`); Display on
    // NSString gives the Rust string without touching the Objective-C runtime unsafely.
    let raw = NSLocale::currentLocale().localeIdentifier().to_string();
    normalize_locale(&raw)
}

#[cfg(not(target_os = "macos"))]
fn macos_locale() -> Option<String> {
    None
}

/// Normalizes a POSIX locale (`hu_HU.UTF-8`, `sr_RS@latin`) into a BCP-47-like tag
/// (`hu-HU`, `sr-RS`): the encoding suffix and the `@modifier` are stripped.
fn normalize_locale(raw: &str) -> Option<String> {
    let core = raw.split(['.', '@']).next()?.trim().replace('_', "-");
    if core.is_empty() || core == "C" || core == "POSIX" {
        return None;
    }
    Some(core)
}

/// Maps the license state to the coarse, payload-safe tag.
#[must_use]
pub fn license_tag(state: LicenseState) -> &'static str {
    match state {
        LicenseState::Full => "full",
        LicenseState::Trial { .. } => "trial",
        LicenseState::TrialExpired => "trial_ended",
        LicenseState::Unknown => "unknown",
    }
}

/// HTTP-client configuration for the sender.
#[derive(Debug, Clone)]
pub struct SenderConfig {
    /// PostHog project API key ("phc_..." shape). `None` disables the feature.
    pub api_key: Option<String>,
    /// Capture endpoint base, for example `https://eu.i.posthog.com`.
    pub endpoint: String,
    pub request_timeout: Duration,
    pub initial_delay: Duration,
    pub retry_delay: Duration,
}

impl SenderConfig {
    /// Test-friendly config: explicit key and endpoint, fast cadence, short timeouts.
    #[must_use]
    pub fn for_tests(endpoint: String) -> Self {
        Self {
            api_key: Some("phc_test".to_owned()),
            endpoint,
            request_timeout: Duration::from_secs(2),
            initial_delay: Duration::from_millis(200),
            retry_delay: Duration::from_millis(200),
        }
    }
}

impl Default for SenderConfig {
    fn default() -> Self {
        Self {
            api_key: build_key().map(str::to_owned),
            endpoint: build_endpoint(),
            request_timeout: REQUEST_TIMEOUT,
            initial_delay: INITIAL_DELAY,
            retry_delay: RETRY_DELAY,
        }
    }
}

/// The usage-stats manager owning the install record. Cheap to clone; all clones share the
/// record lock and the consent mirror, so the host's clones and the sender thread serialize
/// on the file and see the same consent.
#[derive(Clone)]
pub struct UsageStats {
    data_dir: PathBuf,
    clock: Arc<dyn Clock>,
    config: SenderConfig,
    record_lock: Arc<Mutex<()>>,
    /// Mirror of the host's in-memory `OcySettings::usage_stats`, applied through
    /// [`UsageStats::apply_consent`]. The sender reads only this, never `settings.json`.
    consent_mirror: Arc<AtomicU8>,
}

impl std::fmt::Debug for UsageStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageStats")
            .field("data_dir", &self.data_dir)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl UsageStats {
    /// Creates the manager for `data_dir` (where `settings.json` and friends live), using the
    /// build-time key and endpoint. Returns `None` in builds without a key (feature inert).
    #[must_use]
    pub fn from_build(data_dir: impl Into<PathBuf>, clock: Arc<dyn Clock>) -> Option<Self> {
        let config = SenderConfig::default();
        config.api_key.as_ref()?;
        Some(Self::with_config(data_dir, clock, config))
    }

    /// Creates the manager with an explicit config (tests, ad-hoc probes).
    #[must_use]
    pub fn with_config(
        data_dir: impl Into<PathBuf>,
        clock: Arc<dyn Clock>,
        config: SenderConfig,
    ) -> Self {
        Self {
            data_dir: data_dir.into(),
            clock,
            config,
            record_lock: Arc::new(Mutex::new(())),
            consent_mirror: Arc::new(AtomicU8::new(CONSENT_NOT_ASKED)),
        }
    }

    /// `true` when this manager can send (build has a key).
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    /// Current consent: the in-memory mirror the host last applied. This never touches
    /// `settings.json`; the host's `OcySettings` is the single source of truth.
    #[must_use]
    pub fn consent(&self) -> UsageStatsConsent {
        consent_from_u8(self.consent_mirror.load(Ordering::Acquire))
    }

    /// Applies the consent from the host's in-memory settings, which the host calls after
    /// every settings load and save ([`apply_consent_from_settings`]).
    ///
    /// - `Granted`: creates the install record when absent (new install ID, opted-in
    ///   today, nothing sent yet) and wakes the sender when this is a fresh yes, so the
    ///   opt-in event does not wait for the next hourly slot.
    /// - `Declined`/`NotAsked`: deletes `usage-stats.json` (install ID and pending days
    ///   gone; a later yes gets a new ID).
    ///
    /// The record change happens under `record_lock`, so it can never interleave with a
    /// send attempt's snapshot or advance. This method does **not** persist anything to
    /// `settings.json`; the host saves consent through its normal settings save path.
    pub fn apply_consent(&self, consent: UsageStatsConsent) {
        let was = self
            .consent_mirror
            .swap(consent_to_u8(consent), Ordering::Release);
        {
            let _guard = self
                .record_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if consent == UsageStatsConsent::Granted {
                self.ensure_record(self.today());
            } else {
                let _ = fs::remove_file(self.record_path());
            }
        }
        if consent == UsageStatsConsent::Granted && was != CONSENT_GRANTED {
            wake_sender();
        }
    }

    /// Runs one send attempt. Also the body of one sender-thread iteration.
    ///
    /// The due events and the install ID are snapshotted under `record_lock`; the lock is
    /// **released for the HTTP call**, so a consent change on the UI thread never waits on
    /// the request timeout. After the call, the lock is re-acquired and the record is
    /// advanced and saved only if consent is still `Granted` and the on-disk record still
    /// carries the snapshotted install ID: a decline or a re-grant during the flight wins,
    /// and a declined record is never resurrected or advanced.
    #[must_use]
    pub fn attempt(&self) -> AttemptOutcome {
        if self.config.api_key.is_none() || self.consent() != UsageStatsConsent::Granted {
            return AttemptOutcome::NothingDue;
        }
        let (record, due, include_enabled, events) = {
            let _guard = self
                .record_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let today = self.today();
            // The record is created only by `apply_consent(Granted)`. A missing record with
            // granted consent (for example a declined file that lost a race) sends nothing;
            // it is never recreated here.
            let Some(record) = self.load_record() else {
                return AttemptOutcome::NothingDue;
            };
            let settings = self.store().load_settings();
            let due = self.due_days(&record, today);
            let environment = Environment::gather(&settings, self.license_tag_now());
            let include_enabled = !record.enabled_event_sent;
            let mut events = Vec::new();
            if include_enabled {
                events.push(enabled_event_json(&record, &environment));
            }
            let stats = self.store().load_stats();
            for date in &due {
                if let Some(day) = stats.get(*date) {
                    events.push(daily_event_json(
                        &record,
                        *date,
                        day,
                        &settings,
                        &environment,
                    ));
                }
            }
            if events.is_empty() {
                return AttemptOutcome::NothingDue;
            }
            (record, due, include_enabled, events)
        };
        let batch = Batch {
            api_key: self.config.api_key.as_deref().unwrap_or_default(),
            batch: &events,
        };
        match self.post_batch(&batch) {
            SubmitOutcome::Accepted => (),
            SubmitOutcome::RejectedClient => return AttemptOutcome::RejectedClient,
            SubmitOutcome::RetryLater => return AttemptOutcome::RetryLater,
        }
        // Re-acquire and advance only when nothing changed underneath us.
        {
            let _guard = self
                .record_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.consent() != UsageStatsConsent::Granted {
                // The user declined while the batch was in flight. The server accepted it,
                // but the record (already deleted by the decline) stays deleted and is
                // never advanced or recreated.
                return AttemptOutcome::Sent;
            }
            let Some(mut advanced) = self.load_record() else {
                return AttemptOutcome::Sent;
            };
            if advanced.install_id != record.install_id {
                // A decline and re-grant during the flight: the new record is untainted.
                return AttemptOutcome::Sent;
            }
            // The whole batch was accepted, so every day in it (and the enabled event,
            // if included) counts as sent. Retries of the same days deduplicate
            // server-side via the deterministic event UUIDs.
            advanced.last_sent_date = due.last().copied().or(advanced.last_sent_date);
            if include_enabled {
                advanced.enabled_event_sent = true;
            }
            if let Err(err) = self.save_record(&advanced) {
                return AttemptOutcome::PersistFailed(err);
            }
        }
        AttemptOutcome::Sent
    }

    /// Days due right now according to the record and `stats.json` candidates.
    fn due_days(&self, record: &UsageStatsRecord, today: Date) -> Vec<Date> {
        let candidates: Vec<Date> = self
            .store()
            .load_stats()
            .days
            .iter()
            .map(|day| day.date)
            .collect();
        select_due_days(
            record.opted_in_date,
            record.last_sent_date,
            today,
            &candidates,
        )
    }

    fn license_tag_now(&self) -> &'static str {
        if crate::license::STORE_FEATURE_ENABLED {
            let cache = LicenseCache::new(self.data_dir.join(LICENSE_CACHE_FILE_NAME));
            let now = self.clock.now().timestamp();
            cache
                .load()
                .map(|cached| license_tag(cached.state.resolve_at(now)))
                .unwrap_or("unknown")
        } else {
            NON_STORE_LICENSE
        }
    }

    /// Starts the background sender thread (about one minute after start, then hourly).
    /// Never the UI thread, never blocking startup. No thread is spawned when the config
    /// has no key.
    #[must_use]
    pub fn start_sender(self: &Arc<Self>, error_log: damt_app::ErrorLog) -> SenderHandle {
        if self.config.api_key.is_none() {
            return SenderHandle {
                stop: Arc::new(AtomicBool::new(false)),
                thread: None,
            };
        }
        let stop = Arc::new(AtomicBool::new(false));
        let manager = Arc::clone(self);
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("usage-stats".to_owned())
            .spawn(move || sender_loop(manager, error_log, thread_stop))
            .ok();
        SenderHandle { stop, thread }
    }

    fn record_path(&self) -> PathBuf {
        self.data_dir.join(RECORD_FILE_NAME)
    }

    fn store(&self) -> AppDataStore {
        AppDataStore::new(self.data_dir.clone(), Arc::clone(&self.clock))
    }

    /// The current local date (never `NotAsked`-relevant; used for opt-in and day selection).
    fn today(&self) -> Date {
        // Infallible: `Timestamp::to_zoned_datetime` picks a disambiguation on its own.
        self.clock
            .now()
            .timestamp()
            .to_zoned_datetime(TimeZone::system())
            .date()
    }

    /// Loads the install record, or `None` when missing, unreadable, or corrupt.
    #[must_use]
    pub fn load_record(&self) -> Option<UsageStatsRecord> {
        let raw = fs::read_to_string(self.record_path()).ok()?;
        serde_json::from_str::<UsageStatsRecord>(&raw).ok()
    }

    fn ensure_record(&self, today: Date) -> UsageStatsRecord {
        match self.load_record() {
            Some(record) => record,
            None => {
                // A fresh yes (or a corrupt file): new random install ID.
                let record = UsageStatsRecord::new(today);
                let _ = self.save_record(&record);
                record
            }
        }
    }

    fn save_record(&self, record: &UsageStatsRecord) -> Result<(), io::Error> {
        let path = self.record_path();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(record)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let mut tmp = path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp_path = PathBuf::from(tmp);
        fs::write(&tmp_path, json.as_bytes())?;
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    fn post_batch(&self, batch: &Batch<'_>) -> SubmitOutcome {
        let config = ureq::config::Config::builder()
            .tls_config(tls_config())
            .timeout_global(Some(self.config.request_timeout))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let url = format!("{}/batch/", self.config.endpoint.trim_end_matches('/'));
        match agent.post(&url).send_json(batch) {
            Ok(_) => SubmitOutcome::Accepted,
            Err(ureq::Error::StatusCode(code)) if (400..500).contains(&code) => {
                SubmitOutcome::RejectedClient
            }
            Err(_) => SubmitOutcome::RetryLater,
        }
    }
}

/// The TLS backend, chosen with `cfg` at the single place the agent is built so it always
/// matches the per-platform `ureq` features in `Cargo.toml`:
/// - **Linux:** rustls (ring) with bundled webpki roots — pure Rust, so the binary does
///   not link `libssl.so.3`/`libcrypto.so.3` (R4 finding 4).
/// - **Windows/macOS:** the OS-native stack (SChannel / Security.framework) via the
///   `NativeTls` provider with the platform's own verifier and roots.
///
/// Certificate and hostname verification stay on everywhere. The provider must match the
/// compiled features exactly: ureq's default (`Rustls` + `WebPki`) panics at the first
/// `https://` request when the `rustls` feature is off, and `NativeTls` without
/// `native-tls-webpki-roots` panics unless the roots come from the platform verifier
/// (R4 finding 1).
fn tls_config() -> ureq::tls::TlsConfig {
    let builder = ureq::tls::TlsConfig::builder();
    if cfg!(target_os = "linux") {
        builder
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::WebPki)
            .build()
    } else {
        builder
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build()
    }
}

/// Outcome of one send attempt, deciding the sender thread's next cadence.
#[derive(Debug)]
pub enum AttemptOutcome {
    /// Batch accepted and the record was advanced.
    Sent,
    /// Nothing was due (or no consent / no key); no network request was made.
    NothingDue,
    /// Network or 5xx error: retry next hour.
    RetryLater,
    /// 4xx: server rejected the batch; stop for this run after logging once.
    RejectedClient,
    /// Batch was accepted but the record could not be advanced; retry next hour.
    PersistFailed(io::Error),
}

impl PartialEq for AttemptOutcome {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Sent, Self::Sent)
                | (Self::NothingDue, Self::NothingDue)
                | (Self::RetryLater, Self::RetryLater)
                | (Self::RejectedClient, Self::RejectedClient)
                | (Self::PersistFailed(_), Self::PersistFailed(_))
        )
    }
}

impl Eq for AttemptOutcome {}

/// Result of one batch POST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// 2xx: the server accepted the batch.
    Accepted,
    /// 4xx: rejected; stop sending for this run.
    RejectedClient,
    /// Network error or 5xx: retry later.
    RetryLater,
}

fn sender_loop(manager: Arc<UsageStats>, error_log: damt_app::ErrorLog, stop: Arc<AtomicBool>) {
    let mut delay = manager.config.initial_delay;
    loop {
        park_timeout(delay);
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match manager.attempt() {
            AttemptOutcome::Sent
            | AttemptOutcome::NothingDue
            | AttemptOutcome::RetryLater
            | AttemptOutcome::PersistFailed(_) => {
                delay = manager.config.retry_delay;
            }
            AttemptOutcome::RejectedClient => {
                // Log once for the run, with no event content in the log.
                error_log.log(
                    Some("usage_stats"),
                    "server rejected the usage-stats batch (HTTP 4xx); sending disabled for this run",
                );
                return;
            }
        }
    }
}

/// Handle for the background sender thread. Dropping it signals the thread to stop.
#[derive(Debug)]
pub struct SenderHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SenderHandle {
    /// Wakes the sender thread early (used when consent just became `Granted`, so the
    /// opt-in event does not wait for the next hourly slot).
    pub fn wake(&self) {
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    /// Signals the thread to stop (without joining; the app never blocks on exit).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake();
    }
}

impl Drop for SenderHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

// --- Wiring: process-wide access for the app (and AN2's Settings UI) ---

static MANAGER: OnceLock<Arc<UsageStats>> = OnceLock::new();
static SENDER: OnceLock<SenderHandle> = OnceLock::new();

/// Installs the process-wide manager and sender handle (call once from the app start).
pub fn install(manager: Arc<UsageStats>, sender: SenderHandle) {
    let _ = MANAGER.set(manager);
    let _ = SENDER.set(sender);
}

/// The process-wide manager, for the Settings UI (AN2): `consent` and `is_available`.
#[must_use]
pub fn global() -> Option<Arc<UsageStats>> {
    MANAGER.get().cloned()
}

/// Host hook: applies the consent from the in-memory `OcySettings` — the single source of
/// truth, owned by the app host and persisted through its normal settings save path — to
/// the process-wide manager. The host calls this after every settings load and save; it
/// is a no-op without a manager (keyless build, isolated run).
///
/// AN2's Settings switch and one-time ask set `usage_stats` through the normal settings
/// model and never call into this module directly.
pub fn apply_consent_from_settings(settings: &OcySettings) {
    if let Some(manager) = global() {
        manager.apply_consent(settings.usage_stats);
    }
}

/// Wakes the sender early (for example right after consent became `Granted`).
pub fn wake_sender() {
    if let Some(sender) = SENDER.get() {
        sender.wake();
    }
}

/// App-start wiring: creates the build-time manager (when the build has a key), starts its
/// sender thread, and installs both process-wide. Returns the manager for callers that
/// want a handle now; AN2's Settings toggle can also use [`global`] and [`wake_sender`].
///
/// Inert in keyless builds: no manager, no thread, nothing installed.
#[must_use]
pub fn start_from_build(
    data_dir: PathBuf,
    clock: Arc<dyn Clock>,
    error_log: damt_app::ErrorLog,
) -> Option<Arc<UsageStats>> {
    let manager = Arc::new(UsageStats::from_build(data_dir, clock)?);
    let sender = manager.start_sender(error_log);
    let stored = Arc::clone(&manager);
    install(manager, sender);
    Some(stored)
}

// --- Event payload: exactly these fields, nothing else ---

/// PostHog batch capture body: `{"api_key": ..., "batch": [...]}`.
#[derive(Debug, Serialize)]
struct Batch<'a> {
    api_key: &'a str,
    batch: &'a [serde_json::Value],
}

/// PostHog capture event: exactly `event`, `distinct_id`, `uuid`, `timestamp`, `properties`.
#[derive(Debug, Serialize)]
struct CaptureEvent {
    event: &'static str,
    distinct_id: String,
    uuid: String,
    timestamp: String,
    properties: serde_json::Value,
}

const fn preset_tag(preset: Preset) -> &'static str {
    match preset {
        Preset::Gentle => "gentle",
        Preset::Balanced => "balanced",
        Preset::Focused => "focused",
        Preset::Custom => "custom",
    }
}

const fn games_mode_tag(mode: GamesMode) -> &'static str {
    match mode {
        GamesMode::Gentle => "gentle",
        GamesMode::Hold => "hold",
    }
}

/// The namespace of compute-time event UUIDs, so Ocy event UUIDs cannot collide with other
/// tooling's v5 UUIDs.
#[must_use]
pub fn event_namespace() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"ocy-usage-stats")
}

/// Deterministic UUID v5 for a day's event: the same install ID and date always yield the
/// same UUID, so a retry of the same day deduplicates server-side instead of counting twice.
#[must_use]
pub fn daily_event_uuid(install_id: &str, date: Date) -> String {
    let name = format!("{install_id}:daily_summary:{date}");
    Uuid::new_v5(&event_namespace(), name.as_bytes()).to_string()
}

/// Deterministic UUID v5 for the one-time opt-in event.
#[must_use]
pub fn enabled_event_uuid(install_id: &str) -> String {
    let name = format!("{install_id}:usage_stats_enabled");
    Uuid::new_v5(&event_namespace(), name.as_bytes()).to_string()
}

/// The event timestamp for `date`: always noon UTC of that day, deterministic for retries.
#[must_use]
pub fn event_timestamp(date: Date) -> String {
    date.to_datetime(jiff::civil::time(12, 0, 0, 0))
        .to_zoned_datetime(TimeZone::UTC)
        .map(|zoned| zoned.timestamp().to_string())
        .unwrap_or_else(|_| "1970-01-01T12:00:00Z".to_owned())
}

/// Common, property-level flags: no person profile, no geoip, and the library tag.
fn base_properties(environment: &Environment) -> serde_json::Value {
    serde_json::json!({
        "$process_person_profile": false,
        "$geoip_disable": true,
        "$lib": "ocy",
        "platform": environment.platform,
        "os_version": environment.os_version,
        "app_version": environment.app_version,
        "channel": environment.channel,
        "license": environment.license,
        "ui_language": environment.ui_language,
        "locale": environment.locale,
    })
}

/// Properties shared by both events, then extended per event kind.
fn merge_properties(base: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let mut properties = base;
    if let (Some(target), Some(extensions)) = (properties.as_object_mut(), extra.as_object()) {
        for (key, value) in extensions {
            target.insert(key.clone(), value.clone());
        }
    }
    properties
}

/// Builds the JSON for one day's `daily_summary` event. Pure: takes the day, the current
/// settings, and the environment slice. Serializes to exactly the field list of the owner
/// decision, and never the forbidden ones (`games_silent`, paths, user or host names,
/// window or game titles).
#[must_use]
pub fn daily_event_json(
    record: &UsageStatsRecord,
    date: Date,
    day: &DailyStatistics,
    settings: &OcySettings,
    environment: &Environment,
) -> serde_json::Value {
    let properties = merge_properties(
        base_properties(environment),
        serde_json::json!({
            "short_completed": day.short_completed,
            "short_skipped": day.short_skipped,
            "long_completed": day.long_completed,
            "long_skipped": day.long_skipped,
            "long_postponed": day.long_postponed,
            "absences_credited": day.absences_credited,
            "active_minutes": day.active_minutes,
            "preset": preset_tag(presets::detect(settings)),
            "short_enabled": settings.short_breaks.enabled,
            "short_interval_min": settings.short_breaks.interval_minutes,
            "short_duration_s": settings.short_breaks.duration_seconds,
            "long_enabled": settings.long_breaks.enabled,
            "long_interval_min": settings.long_breaks.interval_minutes,
            "long_duration_min": settings.long_breaks.duration_minutes,
            "strict_mode": settings.strict_mode,
            "sounds": settings.sounds,
            "pause_during_fullscreen": settings.pause_during_fullscreen,
            "natural_moments": settings.natural_moments,
            "water_nudges": settings.water_nudges,
            "games_mode": games_mode_tag(settings.games_mode),
            "start_with_system": settings.start_with_system,
            "days_since_opt_in": days_since(date, record.opted_in_date),
        }),
    );
    // `theme` has no setting in this version; it is added only if/when the setting exists.
    serde_json::to_value(CaptureEvent {
        event: DAILY_EVENT,
        distinct_id: record.install_id.clone(),
        uuid: daily_event_uuid(&record.install_id, date),
        timestamp: event_timestamp(date),
        properties,
    })
    .unwrap_or_default()
}

/// Builds the JSON of the one-time `usage_stats_enabled` event (no counts, no snapshot).
#[must_use]
pub fn enabled_event_json(
    record: &UsageStatsRecord,
    environment: &Environment,
) -> serde_json::Value {
    let properties = base_properties(environment);
    serde_json::to_value(CaptureEvent {
        event: ENABLED_EVENT,
        distinct_id: record.install_id.clone(),
        uuid: enabled_event_uuid(&record.install_id),
        timestamp: event_timestamp(record.opted_in_date),
        properties,
    })
    .unwrap_or_default()
}

fn days_since(date: Date, opted_in_date: Date) -> i64 {
    // With `Unit::Day` as the largest unit the span is a plain day count, so `get_days` is
    // exact and can never panic (calendar-unit spans would need a reference date).
    date.since((jiff::Unit::Day, opted_in_date))
        .map(|span| i64::from(span.get_days()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::*;

    #[test]
    fn day_selection_sends_only_full_days_after_opt_in() {
        let opted_in = date(2026, 11, 10);
        let today = date(2026, 11, 14);
        let candidates = vec![
            date(2026, 11, 8),  // before opt-in: never sent
            date(2026, 11, 10), // the opt-in day itself: not a full day yet
            date(2026, 11, 11),
            date(2026, 11, 12),
            date(2026, 11, 13),
            date(2026, 11, 14), // today: still running, not a full day
        ];
        let expected = vec![date(2026, 11, 11), date(2026, 11, 12), date(2026, 11, 13)];
        assert_eq!(
            select_due_days(opted_in, None, today, &candidates),
            expected
        );
    }

    #[test]
    fn day_selection_respects_last_sent_and_caps_at_14() {
        let opted_in = date(2026, 10, 1);
        let today = date(2026, 11, 16);
        let candidates: Vec<Date> = (1..=15)
            .map(|d| date(2026, 11, d))
            .chain([date(2026, 11, 16)])
            .collect();
        // The 14-day cap keeps the most recent due days and skips the oldest (Nov 1).
        let expected: Vec<Date> = (2..=15).map(|d| date(2026, 11, d)).collect();
        assert_eq!(
            select_due_days(opted_in, None, today, &candidates),
            expected
        );

        let after_last: Vec<Date> = expected
            .iter()
            .copied()
            .filter(|day| *day > date(2026, 11, 10))
            .collect();
        assert_eq!(
            select_due_days(opted_in, Some(date(2026, 11, 10)), today, &candidates),
            after_last
        );
    }

    #[test]
    fn fresh_opt_in_sends_nothing_on_day_zero() {
        let today = date(2026, 11, 10);
        assert!(select_due_days(today, None, today, &[today]).is_empty());
    }

    #[test]
    fn day_selection_dedups_candidates() {
        let candidates = vec![date(2026, 11, 11), date(2026, 11, 11)];
        assert_eq!(
            select_due_days(date(2026, 11, 10), None, date(2026, 11, 12), &candidates),
            vec![date(2026, 11, 11)]
        );
    }

    #[test]
    fn daily_events_are_deterministic() {
        let id = "11111111-1111-4111-8111-111111111111";
        // Dedup: same ID + date always yield the same uuid and timestamp.
        assert_eq!(
            daily_event_uuid(id, date(2026, 11, 20)),
            daily_event_uuid(id, date(2026, 11, 20))
        );
        // Golden values computed independently (Python uuid5, RFC 4122 name-based SHA-1).
        assert_eq!(
            daily_event_uuid(id, date(2026, 11, 20)),
            "a6a10371-65b8-5190-8852-421df21c07ba"
        );
        assert_eq!(
            enabled_event_uuid(id),
            "37cb5470-d009-5402-9b7b-30dfd4912e14"
        );
        // Different ID or date changes the uuid.
        assert_ne!(
            daily_event_uuid(id, date(2026, 11, 20)),
            daily_event_uuid("22222222-2222-4222-8222-222222222222", date(2026, 11, 20))
        );
        assert_ne!(
            daily_event_uuid(id, date(2026, 11, 20)),
            daily_event_uuid(id, date(2026, 11, 21))
        );
    }

    #[test]
    fn event_timestamp_is_noon_utc() {
        assert_eq!(event_timestamp(date(2026, 11, 20)), "2026-11-20T12:00:00Z");
    }

    #[test]
    fn days_since_uses_calendar_days() {
        assert_eq!(days_since(date(2026, 11, 20), date(2026, 11, 19)), 1);
        assert_eq!(days_since(date(2027, 1, 2), date(2026, 11, 19)), 44);
    }

    #[test]
    fn normalize_locale_strips_encoding_and_modifier() {
        assert_eq!(normalize_locale("hu_HU.UTF-8").as_deref(), Some("hu-HU"));
        assert_eq!(normalize_locale("hu_HU").as_deref(), Some("hu-HU"));
        assert_eq!(normalize_locale("  en_US  ").as_deref(), Some("en-US"));
        // The @modifier is stripped even without an encoding suffix.
        assert_eq!(normalize_locale("sr_RS@latin").as_deref(), Some("sr-RS"));
        assert_eq!(
            normalize_locale("sr_RS.UTF-8@latin").as_deref(),
            Some("sr-RS")
        );
        // Neutral and empty values stay unknown.
        assert_eq!(normalize_locale("C"), None);
        assert_eq!(normalize_locale("C.UTF-8"), None);
        assert_eq!(normalize_locale("POSIX"), None);
        assert_eq!(normalize_locale(""), None);
        assert_eq!(normalize_locale("   "), None);
    }

    #[test]
    fn license_tag_maps_every_state() {
        use crate::license::LicenseState;
        use jiff::Timestamp;

        assert_eq!(license_tag(LicenseState::Full), "full");
        assert_eq!(
            license_tag(LicenseState::Trial {
                ends_at: Timestamp::from_second(0).expect("epoch timestamp")
            }),
            "trial"
        );
        assert_eq!(license_tag(LicenseState::TrialExpired), "trial_ended");
        assert_eq!(license_tag(LicenseState::Unknown), "unknown");
    }

    #[test]
    fn endpoint_core_treats_an_empty_host_as_unset() {
        assert_eq!(endpoint_from_host(true, None), DEFAULT_ENDPOINT);
        assert_eq!(endpoint_from_host(true, Some("")), DEFAULT_ENDPOINT);
        assert_eq!(endpoint_from_host(true, Some("   ")), DEFAULT_ENDPOINT);
        assert_eq!(
            endpoint_from_host(true, Some("https://eu.i.posthog.com")),
            "https://eu.i.posthog.com"
        );
        assert_eq!(
            endpoint_from_host(true, Some("  https://eu.i.posthog.com/  ")),
            "https://eu.i.posthog.com"
        );
        // Without a key the endpoint stays at the default: nothing ever connects anyway.
        assert_eq!(
            endpoint_from_host(false, Some("https://example.com")),
            DEFAULT_ENDPOINT
        );
    }

    #[test]
    fn golden_daily_event_has_exact_fields_and_no_forbidden_ones() {
        let install_id = "11111111-1111-4111-8111-111111111111";
        let record = UsageStatsRecord {
            schema_version: 1,
            install_id: install_id.to_owned(),
            opted_in_date: date(2026, 11, 19),
            last_sent_date: None,
            enabled_event_sent: true,
        };
        let day = DailyStatistics {
            date: date(2026, 11, 20),
            short_completed: 12,
            short_skipped: 1,
            long_completed: 2,
            long_skipped: 0,
            long_postponed: 3,
            absences_credited: 1,
            active_minutes: 240,
        };
        let settings = OcySettings {
            strict_mode: true,
            sounds: true,
            start_with_system: true,
            games_mode: GamesMode::Hold,
            language: "hu".to_owned(),
            // A sentinel that must never reach the payload: `games_silent` holds real
            // program names, and an empty list would not prove the filter works.
            games_silent: vec!["TopSecretGame.exe".to_owned()],
            ..OcySettings::default()
        };
        let environment = Environment {
            platform: "linux",
            os_version: "6.8".to_owned(),
            app_version: "0.2.3-dev",
            channel: "flatpak",
            license: "none",
            ui_language: "hu".to_owned(),
            locale: "hu-HU".to_owned(),
        };

        let event = daily_event_json(&record, day.date, &day, &settings, &environment);

        let expected_text = r#"
        {
          "event": "daily_summary",
          "distinct_id": "11111111-1111-4111-8111-111111111111",
          "uuid": "a6a10371-65b8-5190-8852-421df21c07ba",
          "timestamp": "2026-11-20T12:00:00Z",
          "properties": {
            "$process_person_profile": false,
            "$geoip_disable": true,
            "$lib": "ocy",
            "platform": "linux",
            "os_version": "6.8",
            "app_version": "0.2.3-dev",
            "channel": "flatpak",
            "license": "none",
            "ui_language": "hu",
            "locale": "hu-HU",
            "short_completed": 12,
            "short_skipped": 1,
            "long_completed": 2,
            "long_skipped": 0,
            "long_postponed": 3,
            "absences_credited": 1,
            "active_minutes": 240,
            "preset": "balanced",
            "short_enabled": true,
            "short_interval_min": 25,
            "short_duration_s": 15,
            "long_enabled": true,
            "long_interval_min": 60,
            "long_duration_min": 5,
            "strict_mode": true,
            "sounds": true,
            "pause_during_fullscreen": true,
            "natural_moments": true,
            "water_nudges": true,
            "games_mode": "hold",
            "start_with_system": true,
            "days_since_opt_in": 1
          }
        }"#;
        let expected: serde_json::Value = serde_json::from_str(expected_text).unwrap();
        assert_eq!(event, expected);

        // And the serialized text never contains forbidden content.
        let json = event.to_string();
        for forbidden in ["games_silent", "path", "user", "host", "title", "exe"] {
            assert!(
                !json.to_ascii_lowercase().contains(forbidden),
                "forbidden token {forbidden:?} in payload"
            );
        }
        // The populated sentinel is absent even case-insensitively.
        assert!(!json.to_ascii_lowercase().contains("topsecretgame"));
    }

    #[test]
    fn golden_enabled_event_has_base_properties_only() {
        const TRIAL: &str = "trial";
        let install_id = "11111111-1111-4111-8111-111111111111";
        let record = UsageStatsRecord {
            schema_version: 1,
            install_id: install_id.to_owned(),
            opted_in_date: date(2026, 11, 19),
            last_sent_date: None,
            enabled_event_sent: false,
        };
        let environment = Environment {
            platform: "windows",
            os_version: "10.0.26100".to_owned(),
            app_version: "0.2.3-dev",
            channel: "store",
            license: TRIAL,
            ui_language: "en".to_owned(),
            locale: "en-US".to_owned(),
        };

        let event = enabled_event_json(&record, &environment);

        let expected_text = r#"
        {
          "event": "usage_stats_enabled",
          "distinct_id": "11111111-1111-4111-8111-111111111111",
          "uuid": "37cb5470-d009-5402-9b7b-30dfd4912e14",
          "timestamp": "2026-11-19T12:00:00Z",
          "properties": {
            "$process_person_profile": false,
            "$geoip_disable": true,
            "$lib": "ocy",
            "platform": "windows",
            "os_version": "10.0.26100",
            "app_version": "0.2.3-dev",
            "channel": "store",
            "license": "trial",
            "ui_language": "en",
            "locale": "en-US"
          }
        }"#;
        let expected: serde_json::Value = serde_json::from_str(expected_text).unwrap();
        assert_eq!(event, expected);

        // No activity counts leak into the opt-in event.
        for count in ["short_completed", "active_minutes", "preset", "strict_mode"] {
            assert!(!event.to_string().contains(count));
        }
    }
}
