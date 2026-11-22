//! Pure settings mapping, localization, debounce/autostart state machine, and lifecycle logic.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use damt_app::ErrorLog;
use damt_platform::{
    ActiveScreenProvider, AppThemeProvider, AutostartService, Clock, FocusStateProvider,
    IdleProvider, MotionPreference, PlatformError, PlatformServices, SystemClock,
    TaskbarThemeProvider,
};
use jiff::SignedDuration;
use ocy_core::engine::{EnginePhase, EngineSnapshot};
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::{GamesMode, OcySettings};
use ocy_core::statistics::StatisticsData;
use ocy_core::time::Paused;

use crate::break_logic::app_theme_is_dark;
use crate::coordinator::SettingsWindow;
use crate::language_choice;
#[cfg(feature = "store")]
use crate::license::{LicenseState, StoreLicenseTexts};
use crate::presets::{self, Preset};
use crate::strings::{Strings, font_family_for};
use crate::timeline::{self, TICK_LABEL_COUNT, TimelineMark};
use crate::today::TodaySummary;

/// Default settings for a fresh app install (`language: "auto"` to follow the Windows UI language).
#[must_use]
pub fn default_app_settings() -> OcySettings {
    OcySettings {
        language: "auto".to_owned(),
        ..OcySettings::default()
    }
}

/// Loads `settings.json` for an existing install (preserving its `"language"` value), or returns
/// [`default_app_settings`] (`language: "auto"`) on a fresh install where `settings.json` is absent.
#[must_use]
pub fn load_app_settings(store: &AppDataStore) -> OcySettings {
    if !store.settings_path().exists() {
        return default_app_settings();
    }
    store.load_settings()
}

/// Debounce delay before live settings changes are applied and persisted (matches C# `ApplyDelay`).
pub const SETTINGS_APPLY_DELAY: Duration = Duration::from_millis(400);

/// Refresh cadence for the Settings V2 timeline and today card while the window is open.
pub const SETTINGS_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Suppression window matching `ScheduleCalculator` (short breaks yield when long break <= 2 min).
const SHORT_BREAK_SUPPRESSION_WINDOW: Duration = Duration::from_secs(120);

/// Returns the Ocy package version stripped of any `+build` metadata (mirrors C# `GetAppVersion`).
#[must_use]
pub fn app_version() -> &'static str {
    let raw = env!("CARGO_PKG_VERSION");
    raw.split('+').next().unwrap_or(raw)
}

/// All UI-backed field values of the Settings window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsValues {
    pub start_with_system: bool,
    pub short_enabled: bool,
    pub short_interval: i32,
    pub short_duration: i32,
    pub short_fullscreen: bool,
    pub long_enabled: bool,
    pub long_interval: i32,
    pub long_duration: i32,
    pub long_warning: i32,
    pub long_postpone: i32,
    pub long_max_postpones: i32,
    pub water_nudges: bool,
    pub strict_mode: bool,
    pub inactivity_tracking: bool,
    pub away_threshold: i32,
    pub pause_fullscreen: bool,
    pub window_nearby: bool,
    pub status_notifications: bool,
    /// Games: `false` gentle cards (default), `true` hold breaks.
    pub games_hold: bool,
    /// Exe names that always hold breaks, in stored order.
    pub games_silent: Vec<String>,
}

impl SettingsValues {
    /// Projects `settings` and platform autostart state into UI control values (mirrors C# `LoadFrom`).
    #[must_use]
    pub fn from_settings(
        settings: &OcySettings,
        autostart_supported: bool,
        autostart_enabled: bool,
    ) -> Self {
        Self {
            start_with_system: if autostart_supported {
                autostart_enabled
            } else {
                settings.start_with_system
            },
            short_enabled: settings.short_breaks.enabled,
            short_interval: settings.short_breaks.interval_minutes,
            short_duration: settings.short_breaks.duration_seconds,
            short_fullscreen: settings.short_breaks.fullscreen,
            long_enabled: settings.long_breaks.enabled,
            long_interval: settings.long_breaks.interval_minutes,
            long_duration: settings.long_breaks.duration_minutes,
            long_warning: settings.long_breaks.warning_seconds,
            long_postpone: settings.long_breaks.postpone_minutes,
            long_max_postpones: settings.long_breaks.max_postpones,
            water_nudges: settings.water_nudges,
            strict_mode: settings.strict_mode,
            inactivity_tracking: settings.inactivity_tracking,
            away_threshold: settings.away_threshold_seconds,
            pause_fullscreen: settings.pause_during_fullscreen,
            window_nearby: settings.has_window_nearby,
            status_notifications: settings.status_notifications,
            games_hold: settings.games_mode == GamesMode::Hold,
            games_silent: settings.games_silent.clone(),
        }
    }

    /// Clones `current` and overwrites only the UI-backed fields, preserving non-UI fields
    /// (`schema_version`, `sounds`, `language`, and `status_notifications` while hidden)
    /// unchanged.
    #[must_use]
    pub fn apply_to(&self, current: &OcySettings, autostart_supported: bool) -> OcySettings {
        let mut updated = current.clone();
        updated.short_breaks.enabled = self.short_enabled;
        updated.short_breaks.interval_minutes = self.short_interval;
        updated.short_breaks.duration_seconds = self.short_duration;
        updated.short_breaks.fullscreen = self.short_fullscreen;

        updated.long_breaks.enabled = self.long_enabled;
        updated.long_breaks.interval_minutes = self.long_interval;
        updated.long_breaks.duration_minutes = self.long_duration;
        updated.long_breaks.warning_seconds = self.long_warning;
        updated.long_breaks.postpone_minutes = self.long_postpone;
        updated.long_breaks.max_postpones = self.long_max_postpones;

        updated.strict_mode = self.strict_mode;
        updated.water_nudges = self.water_nudges;
        updated.inactivity_tracking = self.inactivity_tracking;
        updated.away_threshold_seconds = self.away_threshold;
        updated.pause_during_fullscreen = self.pause_fullscreen;
        updated.has_window_nearby = self.window_nearby;
        updated.games_mode = if self.games_hold {
            GamesMode::Hold
        } else {
            GamesMode::Gentle
        };
        updated.games_silent.clone_from(&self.games_silent);

        if autostart_supported {
            updated.start_with_system = self.start_with_system;
        }
        if crate::STATUS_NOTIFICATIONS_SHIPPED {
            updated.status_notifications = self.status_notifications;
        }
        updated
    }
}

fn format_preset_summary(strings: &Strings, preset: Preset) -> String {
    let Some(values) = preset.values() else {
        return String::new();
    };
    let short = strings.format(
        "settings.v2.preset.shortSummary",
        &[
            ("seconds", &values.short_duration_seconds),
            ("minutes", &values.short_interval_minutes),
        ],
    );
    let long = strings.format(
        "settings.v2.preset.longSummary",
        &[
            ("duration", &values.long_duration_minutes),
            ("minutes", &values.long_interval_minutes),
        ],
    );
    format!("{short}\n{long}")
}

/// Every localized text shown in `ocy_ui::SettingsWindowV2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsTexts {
    pub font_family: String,
    pub window_title: String,
    pub today_title: String,
    pub today_taken_label: String,
    pub today_skipped_label: String,
    pub timeline_title: String,
    pub timeline_short_label: String,
    pub timeline_long_label: String,
    pub now_label: String,
    pub preset_gentle_label: String,
    pub preset_balanced_label: String,
    pub preset_focused_label: String,
    pub preset_custom_label: String,
    pub preset_gentle_summary: String,
    pub preset_balanced_summary: String,
    pub preset_focused_summary: String,
    pub customize_title: String,
    pub general_title: String,
    pub tab_breaks_label: String,
    pub tab_strictness_label: String,
    pub tab_quiet_label: String,
    pub start_with_system_label: String,
    pub start_with_system_help: String,
    pub short_enabled_label: String,
    pub short_interval_label: String,
    pub short_duration_label: String,
    pub short_fullscreen_label: String,
    pub short_fullscreen_help: String,
    pub try_short_label: String,
    pub long_enabled_label: String,
    pub long_interval_label: String,
    pub long_duration_label: String,
    pub long_warning_label: String,
    pub long_postpone_label: String,
    pub long_max_postpones_label: String,
    pub try_long_label: String,
    pub water_nudges_label: String,
    pub water_nudges_help: String,
    pub strict_label: String,
    pub strict_help: String,
    pub inactivity_label: String,
    pub inactivity_help: String,
    pub away_label: String,
    pub pause_fullscreen_label: String,
    pub pause_fullscreen_help: String,
    pub games_title: String,
    pub games_mode_model: Vec<String>,
    pub games_gentle_help: String,
    pub games_hold_help: String,
    pub games_silent_title: String,
    pub games_silent_empty: String,
    pub window_nearby_label: String,
    pub window_nearby_help: String,
    pub language_label: String,
    pub sounds_label: String,
    pub sounds_help: String,
    pub notifications_label: String,
    pub notifications_help: String,
    pub about_name_version: String,
    pub about_tagline: String,
    pub language_model: Vec<String>,
}

impl SettingsTexts {
    /// Resolves all Settings V2 strings and language picker entries from `strings`.
    #[must_use]
    pub fn from_strings(strings: &Strings, version: &str) -> Self {
        let app_name = strings.get("app.name");
        Self {
            font_family: font_family_for(strings.language()).to_owned(),
            window_title: strings.format("settings.windowTitle", &[("app", &app_name)]),
            today_title: strings.get("settings.v2.today.title").to_owned(),
            today_taken_label: strings.get("settings.v2.today.taken").to_owned(),
            today_skipped_label: strings.get("settings.v2.today.skipped").to_owned(),
            timeline_title: strings.get("settings.v2.timeline.title").to_owned(),
            timeline_short_label: strings.get("settings.v2.timeline.shortBreaks").to_owned(),
            timeline_long_label: strings.get("settings.v2.timeline.longBreaks").to_owned(),
            now_label: strings.get("settings.v2.timeline.now").to_owned(),
            preset_gentle_label: strings.get("settings.v2.preset.gentle").to_owned(),
            preset_balanced_label: strings.get("settings.v2.preset.balanced").to_owned(),
            preset_focused_label: strings.get("settings.v2.preset.focused").to_owned(),
            preset_custom_label: strings.get("settings.v2.preset.custom").to_owned(),
            preset_gentle_summary: format_preset_summary(strings, Preset::Gentle),
            preset_balanced_summary: format_preset_summary(strings, Preset::Balanced),
            preset_focused_summary: format_preset_summary(strings, Preset::Focused),
            customize_title: strings.get("settings.v2.section.customize").to_owned(),
            general_title: strings.get("settings.section.general").to_owned(),
            tab_breaks_label: strings.get("settings.v2.tab.breaks").to_owned(),
            tab_strictness_label: strings.get("settings.v2.tab.strictness").to_owned(),
            tab_quiet_label: strings.get("settings.v2.tab.quietTimes").to_owned(),
            start_with_system_label: strings.get("settings.startWithSystem.label").to_owned(),
            start_with_system_help: strings.get("settings.startWithSystem.help").to_owned(),
            short_enabled_label: strings.get("settings.shortBreaks.enabled.label").to_owned(),
            short_interval_label: strings
                .get("settings.shortBreaks.intervalMinutes.label")
                .to_owned(),
            short_duration_label: strings
                .get("settings.shortBreaks.durationSeconds.label")
                .to_owned(),
            short_fullscreen_label: strings
                .get("settings.shortBreaks.fullscreen.label")
                .to_owned(),
            short_fullscreen_help: strings
                .get("settings.shortBreaks.fullscreen.help")
                .to_owned(),
            try_short_label: strings.get("settings.tryShortBreak").to_owned(),
            long_enabled_label: strings.get("settings.longBreaks.enabled.label").to_owned(),
            long_interval_label: strings
                .get("settings.longBreaks.intervalMinutes.label")
                .to_owned(),
            long_duration_label: strings
                .get("settings.longBreaks.durationMinutes.label")
                .to_owned(),
            long_warning_label: strings
                .get("settings.longBreaks.warningSeconds.label")
                .to_owned(),
            long_postpone_label: strings
                .get("settings.longBreaks.postponeMinutes.label")
                .to_owned(),
            long_max_postpones_label: strings
                .get("settings.longBreaks.maxPostpones.label")
                .to_owned(),
            try_long_label: strings.get("settings.tryLongBreak").to_owned(),
            water_nudges_label: strings.get("settings.waterNudges.label").to_owned(),
            water_nudges_help: strings.get("settings.waterNudges.help").to_owned(),
            strict_label: strings.get("settings.strictMode.label").to_owned(),
            strict_help: strings.get("settings.strictMode.help").to_owned(),
            inactivity_label: strings.get("settings.inactivityTracking.label").to_owned(),
            inactivity_help: strings.get("settings.inactivityTracking.help").to_owned(),
            away_label: strings
                .get("settings.awayThresholdSeconds.label")
                .to_owned(),
            pause_fullscreen_label: strings
                .get("settings.pauseDuringFullscreen.label")
                .to_owned(),
            pause_fullscreen_help: strings
                .get("settings.pauseDuringFullscreen.help")
                .to_owned(),
            games_title: strings.get("settings.section.games").to_owned(),
            games_mode_model: vec![
                strings.get("settings.games.gentle").to_owned(),
                strings.get("settings.games.hold").to_owned(),
            ],
            games_gentle_help: strings.get("settings.games.gentleHelp").to_owned(),
            games_hold_help: strings.get("settings.games.holdHelp").to_owned(),
            games_silent_title: strings.get("settings.games.silent").to_owned(),
            games_silent_empty: strings.get("settings.games.silentEmpty").to_owned(),
            window_nearby_label: strings.get("settings.hasWindowNearby.label").to_owned(),
            window_nearby_help: strings.get("settings.hasWindowNearby.help").to_owned(),
            language_label: strings.get("settings.language.label").to_owned(),
            sounds_label: strings.get("settings.sounds.label").to_owned(),
            sounds_help: strings.get("settings.sounds.help").to_owned(),
            notifications_label: strings.get("settings.statusNotifications.label").to_owned(),
            notifications_help: strings.get("settings.statusNotifications.help").to_owned(),
            about_name_version: strings.format(
                "about.version",
                &[("name", &app_name), ("version", &version)],
            ),
            about_tagline: strings.get("app.tagline").to_owned(),
            language_model: language_choice::dropdown_model(
                strings.get(language_choice::automatic_language_key()),
            ),
        }
    }
}

/// Formats `every {minutes} min` for Settings V2 interval sliders.
#[must_use]
pub fn format_slider_every_minutes(strings: &Strings, minutes: i32) -> String {
    strings.format("settings.v2.value.everyMinutes", &[("minutes", &minutes)])
}

/// Formats `{minutes} min` for Settings V2 duration sliders.
#[must_use]
pub fn format_slider_minutes(strings: &Strings, minutes: i32) -> String {
    strings.format("settings.v2.value.minutes", &[("minutes", &minutes)])
}

/// Formats `{seconds} s` for Settings V2 duration sliders.
#[must_use]
pub fn format_slider_seconds(strings: &Strings, seconds: i32) -> String {
    strings.format("settings.v2.value.seconds", &[("seconds", &seconds)])
}

/// Formats CLDR-pluralized `{count} time(s)` for Settings V2 postpone-count slider.
#[must_use]
pub fn format_slider_times(strings: &Strings, count: i32) -> String {
    let n = u64::try_from(count.max(0)).unwrap_or(0);
    strings.format_plural("settings.v2.value.times", n, &[("count", &count)])
}

/// Formats a seconds value as minutes when an exact positive multiple of 60, else seconds.
#[must_use]
pub fn format_slider_duration(strings: &Strings, seconds: i32) -> String {
    if seconds >= 60 && seconds % 60 == 0 {
        format_slider_minutes(strings, seconds / 60)
    } else {
        format_slider_seconds(strings, seconds)
    }
}

/// Formats the Settings V2 header subtitle (`next-break-text`) from `snapshot`.
#[must_use]
pub fn format_next_break_text(
    snapshot: &EngineSnapshot,
    strings: &Strings,
    clock: &dyn Clock,
) -> String {
    match snapshot.phase {
        EnginePhase::Paused => match snapshot.paused_until_utc {
            None | Some(Paused::UntilResumed) => {
                strings.get("tray.menu.pausedIndefinitely").to_owned()
            }
            Some(Paused::Until(until)) => {
                let current_zoned = clock.now();
                let now_ts = current_zoned.timestamp();
                let remaining = until
                    .timestamp()
                    .duration_since(now_ts)
                    .max(SignedDuration::ZERO);
                let time_text = if remaining < SignedDuration::from_mins(1) {
                    strings.get("time.underOneMinute").to_owned()
                } else {
                    strings.format_time_signed(remaining)
                };
                let local_until = until
                    .timestamp()
                    .to_zoned_datetime(current_zoned.time_zone().clone());
                let clock_text = format!("{:02}:{:02}", local_until.hour(), local_until.minute());
                strings.format(
                    "tray.menu.pausedUntil",
                    &[("clock", &clock_text), ("time", &time_text)],
                )
            }
        },
        EnginePhase::ShortBreak | EnginePhase::LongBreak => {
            strings.get("tray.tooltip.onBreak").to_owned()
        }
        EnginePhase::LongBreakWarning => {
            let Some(until_long) = snapshot.time_until_long_break else {
                return String::new();
            };
            let delta = SignedDuration::try_from(until_long).unwrap_or(SignedDuration::ZERO);
            let now = clock.now();
            let at = now.checked_add(delta).unwrap_or(now);
            let clock_text = format!("{:02}:{:02}", at.hour(), at.minute());
            strings.format("settings.v2.header.nextLong", &[("clock", &clock_text)])
        }
        EnginePhase::Working | EnginePhase::Deferred | EnginePhase::Away => {
            let long_comes_first = match (
                snapshot.time_until_short_break,
                snapshot.time_until_long_break,
            ) {
                (None, Some(_)) => true,
                (Some(s), Some(l)) => {
                    l <= s || l.saturating_sub(s) <= SHORT_BREAK_SUPPRESSION_WINDOW
                }
                _ => false,
            };
            if long_comes_first && let Some(until_long) = snapshot.time_until_long_break {
                let delta = SignedDuration::try_from(until_long).unwrap_or(SignedDuration::ZERO);
                let now = clock.now();
                let at = now.checked_add(delta).unwrap_or(now);
                let clock_text = format!("{:02}:{:02}", at.hour(), at.minute());
                return strings.format("settings.v2.header.nextLong", &[("clock", &clock_text)]);
            }
            if let Some(until_short) = snapshot.time_until_short_break {
                let time_text = strings.format_time(until_short);
                return strings.format("settings.v2.header.nextShort", &[("time", &time_text)]);
            }
            String::new()
        }
    }
}

/// Live header, 2-hour timeline projection, and today counters displayed by Settings V2.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsLiveView {
    pub next_break_text: String,
    pub timeline_marks: Vec<TimelineMark>,
    pub timeline_labels: [String; TICK_LABEL_COUNT],
    pub today: TodaySummary,
}

/// Computes the live header, timeline projection, and today counters for Settings V2.
#[must_use]
pub fn build_live_view(
    snapshot: &EngineSnapshot,
    preview_settings: &OcySettings,
    stats: &StatisticsData,
    strings: &Strings,
    clock: &dyn Clock,
) -> SettingsLiveView {
    let projection = timeline::project(snapshot, preview_settings, clock);
    SettingsLiveView {
        next_break_text: format_next_break_text(snapshot, strings, clock),
        timeline_marks: projection.marks,
        timeline_labels: projection.labels,
        today: TodaySummary::from_statistics(stats, clock.now().date()),
    }
}

fn fallback_snapshot(settings: &OcySettings) -> EngineSnapshot {
    let mut clamped = settings.clone();
    clamped.clamp();
    EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: clamped
            .short_breaks
            .enabled
            .then(|| clamped.short_breaks.interval()),
        time_until_long_break: clamped
            .long_breaks
            .enabled
            .then(|| clamped.long_breaks.interval()),
        time_until_long_break_warning: (clamped.long_breaks.enabled
            && clamped.long_breaks.warning_seconds > 0)
            .then(|| {
                clamped
                    .long_breaks
                    .interval()
                    .saturating_sub(clamped.long_breaks.warning_duration())
            }),
        current_break: None,
        paused_until_utc: None,
        postpones_left: u32::try_from(clamped.long_breaks.max_postpones.max(0)).unwrap_or(0),
        strict_mode: clamped.strict_mode,
        natural_hold: None,
        poll_external: false,
        game_in_front: false,
    }
}

/// Safe in-memory [`AutostartService`] used under `--demo-break`, `--demo-settings`, and
/// `--report-memory` so test/demo runs never touch the Windows HKCU Run key.
#[derive(Debug)]
pub struct NoopAutostart {
    supported: bool,
    enabled: AtomicBool,
}

impl Default for NoopAutostart {
    fn default() -> Self {
        Self::new(true, false)
    }
}

impl NoopAutostart {
    /// Creates an in-memory no-op autostart service.
    #[must_use]
    pub const fn new(supported: bool, initial_enabled: bool) -> Self {
        Self {
            supported,
            enabled: AtomicBool::new(initial_enabled),
        }
    }
}

impl AutostartService for NoopAutostart {
    fn is_supported(&self) -> bool {
        self.supported
    }

    fn is_enabled(&self) -> bool {
        self.supported && self.enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        if self.supported {
            self.enabled.store(enabled, Ordering::Relaxed);
        }
        Ok(())
    }
}

/// [`PlatformServices`] wrapper that replaces only `autostart()` with [`NoopAutostart`].
pub struct DemoPlatformServices<P> {
    inner: P,
    autostart: NoopAutostart,
}

impl<P> fmt::Debug for DemoPlatformServices<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DemoPlatformServices")
            .field("autostart", &self.autostart)
            .finish_non_exhaustive()
    }
}

impl<P: PlatformServices> DemoPlatformServices<P> {
    /// Wraps `inner` with an in-memory [`NoopAutostart`] so the real startup registry key is never touched.
    #[must_use]
    pub const fn new(inner: P) -> Self {
        Self {
            inner,
            autostart: NoopAutostart::new(true, false),
        }
    }

    /// Borrows the wrapped platform services.
    #[must_use]
    pub const fn inner(&self) -> &P {
        &self.inner
    }
}

impl<P: PlatformServices> PlatformServices for DemoPlatformServices<P> {
    fn idle(&self) -> &dyn IdleProvider {
        self.inner.idle()
    }

    fn focus(&self) -> &dyn FocusStateProvider {
        self.inner.focus()
    }

    fn autostart(&self) -> &dyn AutostartService {
        &self.autostart
    }

    fn active_screen(&self) -> &dyn ActiveScreenProvider {
        self.inner.active_screen()
    }

    fn motion(&self) -> &dyn MotionPreference {
        self.inner.motion()
    }

    fn taskbar_theme(&self) -> &dyn TaskbarThemeProvider {
        self.inner.taskbar_theme()
    }

    fn app_theme(&self) -> &dyn AppThemeProvider {
        self.inner.app_theme()
    }
}

/// Outcome of processing a UI change event in [`SettingsDebounceState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeAction {
    /// Event occurred during initial load or programmatic revert; ignored.
    Ignored,
    /// Schedule (or restart) the 400 ms single-shot save timer with `generation`.
    ScheduleTimer {
        generation: u64,
        clear_autostart_error: bool,
    },
    /// `AutostartService::set_enabled` failed; revert UI toggle to `revert_to` without scheduling a save.
    AutostartFailed { revert_to: bool },
    /// `AutostartService::set_enabled_or_defer` deferred the change (Flatpak Background portal):
    /// the platform flow runs off the UI thread and the result arrives later via
    /// [`SettingsDebounceState::on_autostart_result`]. The toggle stays where the user put it.
    AutostartPending { requested: bool },
}

/// Pure state machine for Settings loading guards, 400 ms debouncing, and autostart error handling.
#[derive(Debug, Default)]
pub struct SettingsDebounceState {
    loading: bool,
    suppress_autostart_events: bool,
    has_pending_changes: bool,
    last_autostart_value: bool,
    /// The requested value of a deferred autostart change that has not completed yet.
    autostart_pending: Option<bool>,
    timer_generation: u64,
}

impl SettingsDebounceState {
    /// Creates an idle debounce state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            loading: false,
            suppress_autostart_events: false,
            has_pending_changes: false,
            last_autostart_value: false,
            autostart_pending: None,
            timer_generation: 0,
        }
    }

    /// Enters load mode, suppressing change and autostart side-effects until [`Self::end_load`].
    /// A reload also makes any in-flight deferred autostart result stale.
    pub fn begin_load(&mut self) {
        self.loading = true;
        self.suppress_autostart_events = true;
        self.has_pending_changes = false;
        self.autostart_pending = None;
        self.timer_generation = self.timer_generation.wrapping_add(1);
    }

    /// Finishes loading with the initial autostart toggle state.
    pub fn end_load(&mut self, autostart_value: bool) {
        self.last_autostart_value = autostart_value;
        self.suppress_autostart_events = false;
        self.loading = false;
    }

    /// Sets or clears programmatic autostart suppression (e.g. while reverting a failed toggle).
    pub fn set_suppress_autostart_events(&mut self, suppress: bool) {
        self.suppress_autostart_events = suppress;
    }

    /// True while a change is waiting for the 400 ms debounce timer or an immediate flush.
    #[must_use]
    pub const fn has_pending_changes(&self) -> bool {
        self.has_pending_changes
    }

    /// Current timer generation (incremented on every restart, flush, or load).
    #[must_use]
    pub const fn timer_generation(&self) -> u64 {
        self.timer_generation
    }

    /// True while a deferred autostart change is in flight (its result has not arrived yet).
    #[must_use]
    pub const fn is_autostart_pending(&self) -> bool {
        self.autostart_pending.is_some()
    }

    /// Handles a UI `changed()` signal, applying autostart immediately if that toggle changed.
    ///
    /// `done` is the completion callback for a deferred autostart change
    /// ([`AutostartService::set_enabled_or_defer`]); every synchronous path drops it unused.
    pub fn on_changed(
        &mut self,
        autostart_supported: bool,
        current_autostart_ui: bool,
        autostart: &dyn AutostartService,
        done: Box<dyn FnOnce(Result<(), PlatformError>) + Send>,
    ) -> ChangeAction {
        if self.loading || self.suppress_autostart_events {
            return ChangeAction::Ignored;
        }

        if autostart_supported && current_autostart_ui != self.last_autostart_value {
            if self.autostart_pending.is_some() {
                // A deferred change is already in flight; further autostart toggles are
                // ignored until its result arrives (the portal dialog may still be showing).
                return ChangeAction::Ignored;
            }
            return match autostart.set_enabled_or_defer(current_autostart_ui, done) {
                Some(Ok(())) => {
                    self.last_autostart_value = current_autostart_ui;
                    let generation = self.schedule_apply();
                    ChangeAction::ScheduleTimer {
                        generation,
                        clear_autostart_error: true,
                    }
                }
                Some(Err(_)) => ChangeAction::AutostartFailed {
                    revert_to: !current_autostart_ui,
                },
                None => {
                    self.autostart_pending = Some(current_autostart_ui);
                    ChangeAction::AutostartPending {
                        requested: current_autostart_ui,
                    }
                }
            };
        }

        let generation = self.schedule_apply();
        ChangeAction::ScheduleTimer {
            generation,
            clear_autostart_error: false,
        }
    }

    /// Feeds the result of a deferred autostart change back into the machine.
    ///
    /// `requested` is the value that was asked for when the change was deferred. A stale
    /// result (the window reloaded, or the pending request no longer matches) is ignored.
    pub fn on_autostart_result(
        &mut self,
        requested: bool,
        result: Result<(), PlatformError>,
    ) -> ChangeAction {
        if self.autostart_pending != Some(requested) {
            return ChangeAction::Ignored;
        }
        self.autostart_pending = None;
        match result {
            Ok(()) => {
                self.last_autostart_value = requested;
                let generation = self.schedule_apply();
                ChangeAction::ScheduleTimer {
                    generation,
                    clear_autostart_error: true,
                }
            }
            Err(_) => ChangeAction::AutostartFailed {
                revert_to: !requested,
            },
        }
    }

    fn schedule_apply(&mut self) -> u64 {
        self.has_pending_changes = true;
        self.timer_generation = self.timer_generation.wrapping_add(1);
        self.timer_generation
    }

    /// Called when the 400 ms timer fires for `generation`. Returns `true` if a save should run now.
    pub fn on_timer_fired(&mut self, generation: u64) -> bool {
        if generation != self.timer_generation {
            return false;
        }
        if !self.has_pending_changes {
            return false;
        }
        self.has_pending_changes = false;
        true
    }

    /// Cancels any active debounce timer and returns `true` if a pending change must be saved now.
    pub fn flush_if_pending(&mut self) -> bool {
        self.timer_generation = self.timer_generation.wrapping_add(1);
        if !self.has_pending_changes {
            return false;
        }
        self.has_pending_changes = false;
        true
    }

    /// Invalidates any outstanding timer without flushing (used on teardown/drop).
    pub fn cancel_timer(&mut self) {
        self.timer_generation = self.timer_generation.wrapping_add(1);
    }
}

/// UI-thread callback receiving the final result of a deferred autostart change.
type AutostartResultSink = Box<dyn FnOnce(Result<(), PlatformError>)>;

thread_local! {
    /// UI-thread sink for the result of a deferred autostart change.
    /// [`ManagedSettingsInner::handle_changed`] registers it right before calling
    /// [`AutostartService::set_enabled_or_defer`]; the UI-thread half of the `done` callback
    /// (built by [`SettingsBackend::autostart_result_done`]) takes it and invokes it. A single
    /// slot is enough: only one deferred change can be in flight, and while one is, the slot
    /// is not overwritten.
    static AUTOSTART_RESULT_SINK: RefCell<Option<AutostartResultSink>> =
        RefCell::new(None);
}

/// Registers the UI-thread sink invoked with the result of a deferred autostart change.
pub(crate) fn register_autostart_result_sink(sink: AutostartResultSink) {
    AUTOSTART_RESULT_SINK.with(|slot| *slot.borrow_mut() = Some(sink));
}

/// Invokes and clears the registered autostart result sink, if any. Must run on the thread
/// that registered the sink (the UI thread); a no-op when no sink is registered (stale result).
pub(crate) fn invoke_registered_autostart_result_sink(result: Result<(), PlatformError>) {
    AUTOSTART_RESULT_SINK.with(|slot| {
        if let Some(sink) = slot.borrow_mut().take() {
            sink(result);
        }
    });
}

/// Host callbacks invoked by the Settings window.
#[derive(Clone)]
pub struct SettingsCallbacks {
    pub on_save: Rc<dyn Fn(OcySettings) -> bool>,
    pub on_try_short_break: Rc<dyn Fn()>,
    pub on_try_long_break: Rc<dyn Fn()>,
}

impl fmt::Debug for SettingsCallbacks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsCallbacks").finish_non_exhaustive()
    }
}

/// UI event handlers passed from [`ManagedSettingsWindow`] to a [`SettingsBackend`].
#[derive(Clone)]
pub struct SettingsUiHandlers {
    pub on_changed: Rc<dyn Fn()>,
    pub on_preset_selected: Rc<dyn Fn(i32)>,
    pub on_language_selected: Rc<dyn Fn(i32)>,
    pub on_try_short_break: Rc<dyn Fn()>,
    pub on_try_long_break: Rc<dyn Fn()>,
    pub on_close_requested: Rc<dyn Fn()>,
    #[cfg(feature = "store")]
    pub on_buy: Rc<dyn Fn()>,
}

impl fmt::Debug for SettingsUiHandlers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsUiHandlers").finish_non_exhaustive()
    }
}

/// Initialization payload passed to [`SettingsBackend::create`].
#[derive(Debug, Clone)]
pub struct SettingsCreateParams<'a> {
    pub texts: &'a SettingsTexts,
    pub values: &'a SettingsValues,
    pub preset: i32,
    pub language_index: i32,
    pub live_view: &'a SettingsLiveView,
    pub strings: &'a Arc<Strings>,
    pub show_start_with_system: bool,
    pub dark: bool,
    #[cfg(feature = "store")]
    pub license_state: &'static str,
    #[cfg(feature = "store")]
    pub trial_days_left: i32,
    #[cfg(feature = "store")]
    pub store_texts: &'a StoreLicenseTexts,
}

/// Windowing/UI backend abstraction so Settings lifecycle, debounce, and error paths are unit-testable.
pub trait SettingsBackend {
    type Instance;

    /// Builds, populates, and shows a fresh Settings window instance.
    fn create(
        &self,
        params: SettingsCreateParams<'_>,
        handlers: SettingsUiHandlers,
    ) -> Result<Self::Instance, String>;

    /// Un-minimizes, brings to front, and focuses an already-open instance.
    fn activate(&self, instance: &Self::Instance);

    /// Refills localized text properties, font, language model/index, and formatter strings on an open instance.
    fn set_texts(
        &self,
        _instance: &Self::Instance,
        _texts: &SettingsTexts,
        _language_index: i32,
        _strings: &Arc<Strings>,
    ) {
    }

    /// Updates light/dark theme (and theme-aware assets) on an open instance.
    fn set_dark(&self, instance: &Self::Instance, dark: bool);

    /// Updates the taskbar-theme icon variant on an open instance.
    fn set_light_icon_set(&self, _instance: &Self::Instance, _use_light_set: bool) {}

    /// Reads the current UI control values from `instance`.
    fn read_values(&self, instance: &Self::Instance) -> SettingsValues;

    /// Updates the selected `preset` index (`0..=3`) on an open instance.
    fn set_preset(&self, _instance: &Self::Instance, _preset: i32) {}

    /// Applies preset-controlled break slider values and `preset` index on an open instance.
    fn apply_preset_values(
        &self,
        _instance: &Self::Instance,
        _preset: i32,
        _values: &SettingsValues,
    ) {
    }

    /// Replaces the silent-games list on an open instance (a game was added from a break card).
    fn set_games_silent(&self, _instance: &Self::Instance, _names: &[String]) {}

    /// Updates the selected `language-index` on an open instance.
    fn set_language_index(&self, _instance: &Self::Instance, _index: i32) {}

    /// Updates the header next-break text, timeline marks/labels, and today counters on an open instance.
    fn set_live_view(&self, _instance: &Self::Instance, _live_view: &SettingsLiveView) {}

    /// Updates the `start-with-system` toggle value on `instance`.
    fn set_start_with_system(&self, instance: &Self::Instance, enabled: bool);

    /// Sets or clears (`""`) the inline autostart error message on `instance`.
    fn set_start_with_system_error(&self, instance: &Self::Instance, error: &str);

    /// Sets or clears (`""`) the top-level save error message on `instance`.
    fn set_save_error(&self, instance: &Self::Instance, error: &str);

    /// Schedules a single-shot callback after `delay`.
    fn schedule_apply_timer(&self, delay: Duration, on_fire: Box<dyn FnOnce()>);

    /// Builds the completion callback handed to [`AutostartService::set_enabled_or_defer`]
    /// when an autostart change may complete asynchronously (Linux Flatpak Background portal).
    ///
    /// The returned callback can be invoked from any thread (it is `Send`) and must marshal
    /// the result back to the UI thread, where it invokes the sink most recently registered
    /// with [`register_autostart_result_sink`]. The default invokes the sink inline and is
    /// therefore only correct when `done` is called on the UI thread (unit tests,
    /// single-threaded hosts); the Slint backend overrides it with `slint::invoke_from_event_loop`.
    fn autostart_result_done(&self) -> Box<dyn FnOnce(Result<(), PlatformError>) + Send> {
        Box::new(invoke_registered_autostart_result_sink)
    }

    /// Starts (or replaces) the periodic 30 s refresh timer while the window is open.
    fn start_refresh_timer(&self, _interval: Duration, _on_tick: Box<dyn Fn()>) {}

    /// Stops any active periodic refresh timer.
    fn stop_refresh_timer(&self) {}

    /// Hides and schedules destruction of `instance` after the close callback unwinds.
    fn destroy_on_close(&self, instance: Self::Instance);

    /// Immediately hides and drops `instance` (during shutdown/drop).
    fn hide_and_destroy(&self, instance: Self::Instance);

    /// Updates Store license status (`license-state`, `trial-days-left`, and localized texts)
    /// on an open instance.
    #[cfg(feature = "store")]
    fn set_license_status(
        &self,
        _instance: &Self::Instance,
        _license_state: &'static str,
        _trial_days_left: i32,
        _store_texts: &StoreLicenseTexts,
    ) {
    }
}

struct ManagedSettingsInner<B: SettingsBackend> {
    backend: B,
    platform: Arc<dyn PlatformServices>,
    clock: Arc<dyn Clock>,
    strings: RefCell<Arc<Strings>>,
    current_language: RefCell<String>,
    error_log: ErrorLog,
    callbacks: SettingsCallbacks,
    current_settings: RefCell<OcySettings>,
    last_snapshot: RefCell<Option<EngineSnapshot>>,
    last_stats: RefCell<StatisticsData>,
    state: RefCell<SettingsDebounceState>,
    active: RefCell<Option<B::Instance>>,
    dark_theme: Cell<bool>,
    #[cfg(feature = "store")]
    license_state: Cell<LicenseState>,
    #[cfg(feature = "store")]
    on_buy: RefCell<Option<Rc<dyn Fn()>>>,
}

impl<B: SettingsBackend + 'static> ManagedSettingsInner<B> {
    fn preview_settings_for(&self, values: &SettingsValues) -> OcySettings {
        let autostart_supported = self.platform.autostart().is_supported();
        let current = self.current_settings.borrow();
        values.apply_to(&current, autostart_supported)
    }

    fn compute_live_view(&self, preview_settings: &OcySettings) -> SettingsLiveView {
        #[cfg(feature = "store")]
        if self.license_state.get().is_trial_expired() {
            let stats = self.last_stats.borrow();
            let strings = self.strings.borrow();
            return SettingsLiveView {
                next_break_text: strings.get("store.trialEnded.title").to_owned(),
                timeline_marks: Vec::new(),
                timeline_labels: timeline::time_labels(self.clock.as_ref()),
                today: TodaySummary::from_statistics(&stats, self.clock.now().date()),
            };
        }

        let snapshot_guard = self.last_snapshot.borrow();
        let owned_fallback;
        let snapshot = match snapshot_guard.as_ref() {
            Some(snap) => snap,
            None => {
                owned_fallback = fallback_snapshot(preview_settings);
                &owned_fallback
            }
        };
        let stats = self.last_stats.borrow();
        let strings = self.strings.borrow();
        build_live_view(
            snapshot,
            preview_settings,
            &stats,
            &strings,
            self.clock.as_ref(),
        )
    }

    fn refresh_open_live_view(&self) {
        let preview_settings = {
            let active = self.active.borrow();
            let Some(inst) = active.as_ref() else {
                return;
            };
            if self.state.borrow().has_pending_changes() {
                let values = self.backend.read_values(inst);
                self.preview_settings_for(&values)
            } else {
                self.current_settings.borrow().clone()
            }
        };
        let live_view = self.compute_live_view(&preview_settings);
        if let Some(inst) = self.active.borrow().as_ref() {
            self.backend.set_live_view(inst, &live_view);
        }
    }

    fn apply_language_change(&self, language_setting: &str, log_unknown: bool) {
        *self.current_language.borrow_mut() = language_setting.to_owned();
        let updated_strings = Arc::new(Strings::from_language(language_setting));
        if let Some(rtl) = updated_strings.rtl_fallback_from() {
            self.error_log.log(
                Some("i18n"),
                &format!("RTL language {rtl:?} is not supported yet; falling back to English"),
            );
        }
        *self.strings.borrow_mut() = Arc::clone(&updated_strings);
        if self.active.borrow().is_some() {
            let texts = SettingsTexts::from_strings(&updated_strings, app_version());
            let err_log = log_unknown.then_some(&self.error_log);
            let language_index = language_choice::index_for_i32(language_setting, err_log);
            if let Some(inst) = self.active.borrow().as_ref() {
                self.backend
                    .set_texts(inst, &texts, language_index, &updated_strings);
                #[cfg(feature = "store")]
                {
                    let state = self.license_state.get();
                    let days_left = state.trial_days_left_i32(self.clock.now().timestamp());
                    let store_texts = StoreLicenseTexts::from_strings(
                        &updated_strings,
                        (days_left >= 0).then_some(days_left),
                    );
                    self.backend.set_license_status(
                        inst,
                        state.ui_state_tag(),
                        days_left,
                        &store_texts,
                    );
                }
            }
            self.refresh_open_live_view();
        }
    }

    fn handle_changed(self: &Rc<Self>) {
        let autostart_supported = self.platform.autostart().is_supported();
        let values = {
            let active = self.active.borrow();
            let Some(inst) = active.as_ref() else {
                return;
            };
            self.backend.read_values(inst)
        };

        // Deferred-autostart plumbing: register the UI-thread result sink (unless a deferred
        // change is already in flight — its sink must survive until the result arrives) and
        // get the `Send` completion callback from the backend (the Slint backend marshals
        // through `slint::invoke_from_event_loop`). Every synchronous path drops `done` unused.
        if !self.state.borrow().is_autostart_pending() {
            let weak = Rc::downgrade(self);
            let requested = values.start_with_system;
            register_autostart_result_sink(Box::new(move |result| {
                if let Some(inner) = weak.upgrade() {
                    inner.handle_autostart_result(requested, result);
                }
            }));
        }
        let done = self.backend.autostart_result_done();

        let action = self.state.borrow_mut().on_changed(
            autostart_supported,
            values.start_with_system,
            self.platform.autostart(),
            done,
        );
        self.apply_change_action(action);
    }

    /// Applies a [`ChangeAction`] produced by the debounce state machine (UI updates + save timer).
    fn apply_change_action(self: &Rc<Self>, action: ChangeAction) {
        match action {
            ChangeAction::Ignored => {}
            // Calm pending: leave the toggle where the user put it; nothing else happens
            // until the deferred result arrives in `handle_autostart_result`. No new strings.
            ChangeAction::AutostartPending { .. } => {}
            ChangeAction::AutostartFailed { revert_to } => {
                self.state.borrow_mut().set_suppress_autostart_events(true);
                if let Some(inst) = self.active.borrow().as_ref() {
                    let msg = self
                        .strings
                        .borrow()
                        .get("settings.autostart.failed")
                        .to_owned();
                    self.backend.set_start_with_system_error(inst, &msg);
                    self.backend.set_start_with_system(inst, revert_to);
                }
                self.state.borrow_mut().set_suppress_autostart_events(false);
            }
            ChangeAction::ScheduleTimer {
                generation,
                clear_autostart_error,
            } => {
                let values = {
                    let active = self.active.borrow();
                    let Some(inst) = active.as_ref() else {
                        return;
                    };
                    self.backend.read_values(inst)
                };
                let preview = self.preview_settings_for(&values);
                let detected_preset = presets::detect(&preview).index();
                let live_view = self.compute_live_view(&preview);
                if let Some(inst) = self.active.borrow().as_ref() {
                    if clear_autostart_error {
                        self.backend.set_start_with_system_error(inst, "");
                    }
                    self.backend.set_preset(inst, detected_preset);
                    self.backend.set_live_view(inst, &live_view);
                }
                let weak = Rc::downgrade(self);
                self.backend.schedule_apply_timer(
                    SETTINGS_APPLY_DELAY,
                    Box::new(move || {
                        if let Some(inner) = weak.upgrade() {
                            inner.handle_timer_fired(generation);
                        }
                    }),
                );
            }
        }
    }

    /// Feeds the result of a deferred autostart change (marshaled back to the UI thread by
    /// the `done` callback) into the state machine and applies the resulting action.
    fn handle_autostart_result(
        self: &Rc<Self>,
        requested: bool,
        result: Result<(), PlatformError>,
    ) {
        let action = self
            .state
            .borrow_mut()
            .on_autostart_result(requested, result);
        self.apply_change_action(action);
    }

    fn handle_preset_selected(self: &Rc<Self>, index: i32) {
        let Some(preset) = Preset::from_index(index) else {
            return;
        };
        if preset == Preset::Custom {
            return;
        }
        {
            let active = self.active.borrow();
            let Some(inst) = active.as_ref() else {
                return;
            };
            let autostart_supported = self.platform.autostart().is_supported();
            let values = self.backend.read_values(inst);
            let edited = self.preview_settings_for(&values);
            let applied = presets::apply(preset, &edited);
            let updated_values = SettingsValues::from_settings(
                &applied,
                autostart_supported,
                values.start_with_system,
            );
            self.backend
                .apply_preset_values(inst, preset.index(), &updated_values);
        }
        self.handle_changed();
    }

    fn handle_language_selected(&self, index: i32) {
        let tag = language_choice::language_for_i32(index);
        let (updated, prev_lang) = {
            let active = self.active.borrow();
            let Some(inst) = active.as_ref() else {
                return;
            };
            let values = self.backend.read_values(inst);
            let prev = self.current_settings.borrow().language.clone();
            let mut updated = self.preview_settings_for(&values);
            updated.language = tag.to_owned();
            (updated, prev)
        };

        // Any pending debounce changes are included in `updated` and saved immediately.
        let _ = self.state.borrow_mut().flush_if_pending();

        let ok = (self.callbacks.on_save)(updated.clone());
        if ok {
            *self.current_settings.borrow_mut() = updated.clone();
            if *self.current_language.borrow() != updated.language {
                self.apply_language_change(&updated.language, false);
            }
            if let Some(inst) = self.active.borrow().as_ref() {
                self.backend.set_save_error(inst, "");
            }
        } else if let Some(inst) = self.active.borrow().as_ref() {
            let prev_idx = language_choice::index_for_i32(&prev_lang, None);
            self.backend.set_language_index(inst, prev_idx);
            let msg = self.strings.borrow().get("settings.saveFailed").to_owned();
            self.backend.set_save_error(inst, &msg);
        }
    }

    fn handle_timer_fired(&self, generation: u64) {
        let should_save = self.state.borrow_mut().on_timer_fired(generation);
        if should_save {
            self.perform_save();
        }
    }

    fn flush_pending(&self) {
        let should_save = self.state.borrow_mut().flush_if_pending();
        if should_save {
            self.perform_save();
        }
    }

    fn perform_save(&self) {
        let values = {
            let active = self.active.borrow();
            let Some(inst) = active.as_ref() else {
                return;
            };
            self.backend.read_values(inst)
        };

        let autostart_supported = self.platform.autostart().is_supported();
        let updated = {
            let current = self.current_settings.borrow();
            values.apply_to(&current, autostart_supported)
        };

        let ok = (self.callbacks.on_save)(updated.clone());
        if ok {
            *self.current_settings.borrow_mut() = updated;
        }

        if let Some(inst) = self.active.borrow().as_ref() {
            let msg = if ok {
                String::new()
            } else {
                self.strings.borrow().get("settings.saveFailed").to_owned()
            };
            self.backend.set_save_error(inst, &msg);
        }
    }

    fn handle_try_short_break(&self) {
        self.flush_pending();
        (self.callbacks.on_try_short_break)();
    }

    fn handle_try_long_break(&self) {
        self.flush_pending();
        (self.callbacks.on_try_long_break)();
    }

    fn handle_close_requested(&self) {
        self.flush_pending();
        self.state.borrow_mut().cancel_timer();
        self.backend.stop_refresh_timer();
        let removed = self.active.borrow_mut().take();
        if let Some(inst) = removed {
            self.backend.destroy_on_close(inst);
        }
    }

    #[cfg(feature = "store")]
    fn handle_buy(&self) {
        let cb = self.on_buy.borrow().clone();
        if let Some(cb) = cb {
            cb();
        }
    }

    fn build_handlers(self: &Rc<Self>) -> SettingsUiHandlers {
        let weak_changed: Weak<Self> = Rc::downgrade(self);
        let weak_preset: Weak<Self> = Rc::downgrade(self);
        let weak_lang: Weak<Self> = Rc::downgrade(self);
        let weak_short: Weak<Self> = Rc::downgrade(self);
        let weak_long: Weak<Self> = Rc::downgrade(self);
        let weak_close: Weak<Self> = Rc::downgrade(self);
        #[cfg(feature = "store")]
        let weak_buy: Weak<Self> = Rc::downgrade(self);
        SettingsUiHandlers {
            on_changed: Rc::new(move || {
                if let Some(inner) = weak_changed.upgrade() {
                    inner.handle_changed();
                }
            }),
            on_preset_selected: Rc::new(move |i| {
                if let Some(inner) = weak_preset.upgrade() {
                    inner.handle_preset_selected(i);
                }
            }),
            on_language_selected: Rc::new(move |i| {
                if let Some(inner) = weak_lang.upgrade() {
                    inner.handle_language_selected(i);
                }
            }),
            on_try_short_break: Rc::new(move || {
                if let Some(inner) = weak_short.upgrade() {
                    inner.handle_try_short_break();
                }
            }),
            on_try_long_break: Rc::new(move || {
                if let Some(inner) = weak_long.upgrade() {
                    inner.handle_try_long_break();
                }
            }),
            on_close_requested: Rc::new(move || {
                if let Some(inner) = weak_close.upgrade() {
                    inner.handle_close_requested();
                }
            }),
            #[cfg(feature = "store")]
            on_buy: Rc::new(move || {
                if let Some(inner) = weak_buy.upgrade() {
                    inner.handle_buy();
                }
            }),
        }
    }
}

/// Single-instance Settings window controller backed by a [`SettingsBackend`].
pub struct ManagedSettingsWindow<B: SettingsBackend> {
    inner: Rc<ManagedSettingsInner<B>>,
}

impl<B: SettingsBackend + 'static> fmt::Debug for ManagedSettingsWindow<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedSettingsWindow")
            .field("is_open", &self.is_open())
            .field("has_pending_changes", &self.has_pending_changes())
            .finish_non_exhaustive()
    }
}

impl<B: SettingsBackend> Drop for ManagedSettingsWindow<B> {
    fn drop(&mut self) {
        self.inner.state.borrow_mut().cancel_timer();
        self.inner.backend.stop_refresh_timer();
        let removed = self.inner.active.borrow_mut().take();
        if let Some(inst) = removed {
            self.inner.backend.hide_and_destroy(inst);
        }
    }
}

impl<B: SettingsBackend + 'static> ManagedSettingsWindow<B> {
    /// Creates a new [`ManagedSettingsWindow`] backed by [`SystemClock`].
    #[must_use]
    pub fn new(
        backend: B,
        platform: Arc<dyn PlatformServices>,
        strings: Arc<Strings>,
        error_log: ErrorLog,
        callbacks: SettingsCallbacks,
        initial_settings: OcySettings,
    ) -> Self {
        Self::with_clock(
            backend,
            platform,
            Arc::new(SystemClock),
            strings,
            error_log,
            callbacks,
            initial_settings,
        )
    }

    /// Creates a new [`ManagedSettingsWindow`] using an explicit [`Clock`].
    #[must_use]
    pub fn with_clock(
        backend: B,
        platform: Arc<dyn PlatformServices>,
        clock: Arc<dyn Clock>,
        strings: Arc<Strings>,
        error_log: ErrorLog,
        callbacks: SettingsCallbacks,
        initial_settings: OcySettings,
    ) -> Self {
        let dark = app_theme_is_dark(platform.app_theme().is_light_app_theme());
        let initial_language = initial_settings.language.clone();
        Self {
            inner: Rc::new(ManagedSettingsInner {
                backend,
                platform,
                clock,
                strings: RefCell::new(strings),
                current_language: RefCell::new(initial_language),
                error_log,
                callbacks,
                current_settings: RefCell::new(initial_settings),
                last_snapshot: RefCell::new(None),
                last_stats: RefCell::new(StatisticsData::default()),
                state: RefCell::new(SettingsDebounceState::new()),
                active: RefCell::new(None),
                dark_theme: Cell::new(dark),
                #[cfg(feature = "store")]
                license_state: Cell::new(LicenseState::Unknown),
                #[cfg(feature = "store")]
                on_buy: RefCell::new(None),
            }),
        }
    }

    /// Registers the Store purchase callback invoked by `buy()`.
    #[cfg(feature = "store")]
    pub fn set_on_buy(&mut self, on_buy: Rc<dyn Fn()>) {
        *self.inner.on_buy.borrow_mut() = Some(on_buy);
    }

    /// Current Store license state exposed to the UI.
    #[cfg(feature = "store")]
    #[must_use]
    pub fn license_state(&self) -> LicenseState {
        self.inner.license_state.get()
    }

    /// Current trial days left (`-1` when not in a trial with a known end date, `0` when expired).
    #[cfg(feature = "store")]
    #[must_use]
    pub fn trial_days_left(&self) -> i32 {
        self.inner
            .license_state
            .get()
            .trial_days_left_i32(self.inner.clock.now().timestamp())
    }

    /// Triggers the Store purchase flow (`buy()` UI callback).
    #[cfg(feature = "store")]
    pub fn buy(&self) {
        self.inner.handle_buy();
    }

    /// Returns `true` if a Settings window instance is currently open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.inner.active.borrow().is_some()
    }

    /// Returns `true` if changes are pending debounce/save.
    #[must_use]
    pub fn has_pending_changes(&self) -> bool {
        self.inner.state.borrow().has_pending_changes()
    }

    /// Applies an updated OS dark-mode flag to the open Settings window, if any.
    pub fn set_dark(&mut self, dark: bool) {
        self.inner.dark_theme.set(dark);
        if let Some(inst) = self.inner.active.borrow().as_ref() {
            self.inner.backend.set_dark(inst, dark);
        }
    }

    /// Applies an updated taskbar-theme icon flag to the open Settings window, if any.
    pub fn set_light_icon_set(&mut self, use_light_set: bool) {
        if let Some(inst) = self.inner.active.borrow().as_ref() {
            self.inner.backend.set_light_icon_set(inst, use_light_set);
        }
    }

    /// Borrows the underlying backend (for unit tests).
    #[must_use]
    pub fn backend(&self) -> &B {
        &self.inner.backend
    }

    /// Runs `f` with the active backend instance if the Settings window is currently open.
    pub fn with_active_instance<R>(&self, f: impl FnOnce(&B::Instance) -> R) -> Option<R> {
        self.inner.active.borrow().as_ref().map(f)
    }
}

impl<B: SettingsBackend + 'static> SettingsWindow for ManagedSettingsWindow<B> {
    fn sync_settings(&mut self, settings: &OcySettings) {
        let silent_changed =
            self.inner.current_settings.borrow().games_silent != settings.games_silent;
        *self.inner.current_settings.borrow_mut() = settings.clone();
        if silent_changed && let Some(inst) = self.inner.active.borrow().as_ref() {
            self.inner
                .backend
                .set_games_silent(inst, &settings.games_silent);
        }
        let lang_changed = settings.language != *self.inner.current_language.borrow();
        if lang_changed {
            self.inner.apply_language_change(&settings.language, true);
        }
    }

    fn sync_live_status(&mut self, snapshot: &EngineSnapshot, stats: &StatisticsData) {
        *self.inner.last_snapshot.borrow_mut() = Some(snapshot.clone());
        *self.inner.last_stats.borrow_mut() = stats.clone();
        self.inner.refresh_open_live_view();
    }

    #[cfg(feature = "store")]
    fn sync_license(&mut self, state: LicenseState, trial_days_left: i32) {
        self.inner.license_state.set(state);
        if let Some(inst) = self.inner.active.borrow().as_ref() {
            let strings = self.inner.strings.borrow();
            let store_texts = StoreLicenseTexts::from_strings(
                &strings,
                (trial_days_left >= 0).then_some(trial_days_left),
            );
            self.inner.backend.set_license_status(
                inst,
                state.ui_state_tag(),
                trial_days_left,
                &store_texts,
            );
        }
        self.inner.refresh_open_live_view();
    }

    fn show(&mut self) {
        let dark = app_theme_is_dark(self.inner.platform.app_theme().is_light_app_theme());
        self.inner.dark_theme.set(dark);

        if let Some(inst) = self.inner.active.borrow().as_ref() {
            self.inner.backend.set_dark(inst, dark);
            self.inner.refresh_open_live_view();
            self.inner.backend.activate(inst);
            return;
        }

        let autostart_supported = self.inner.platform.autostart().is_supported();
        let autostart_enabled = autostart_supported && self.inner.platform.autostart().is_enabled();
        let (values, preset, language_index, live_view) = {
            let current = self.inner.current_settings.borrow();
            let values =
                SettingsValues::from_settings(&current, autostart_supported, autostart_enabled);
            let preset = presets::detect(&current).index();
            let language_index =
                language_choice::index_for_i32(&current.language, Some(&self.inner.error_log));
            let live_view = self.inner.compute_live_view(&current);
            (values, preset, language_index, live_view)
        };
        let strings = Arc::clone(&self.inner.strings.borrow());
        let texts = SettingsTexts::from_strings(&strings, app_version());
        let handlers = self.inner.build_handlers();
        #[cfg(feature = "store")]
        let (license_state, trial_days_left, store_texts) = {
            let state = self.inner.license_state.get();
            let days = state.trial_days_left_i32(self.inner.clock.now().timestamp());
            let texts = StoreLicenseTexts::from_strings(&strings, (days >= 0).then_some(days));
            (state.ui_state_tag(), days, texts)
        };

        self.inner.state.borrow_mut().begin_load();
        let created = self.inner.backend.create(
            SettingsCreateParams {
                texts: &texts,
                values: &values,
                preset,
                language_index,
                live_view: &live_view,
                strings: &strings,
                show_start_with_system: autostart_supported,
                dark,
                #[cfg(feature = "store")]
                license_state,
                #[cfg(feature = "store")]
                trial_days_left,
                #[cfg(feature = "store")]
                store_texts: &store_texts,
            },
            handlers,
        );
        self.inner
            .state
            .borrow_mut()
            .end_load(values.start_with_system);

        match created {
            Ok(inst) => {
                *self.inner.active.borrow_mut() = Some(inst);
                let weak = Rc::downgrade(&self.inner);
                self.inner.backend.start_refresh_timer(
                    SETTINGS_REFRESH_INTERVAL,
                    Box::new(move || {
                        if let Some(inner) = weak.upgrade() {
                            inner.refresh_open_live_view();
                        }
                    }),
                );
            }
            Err(err) => {
                self.inner
                    .error_log
                    .log(Some("settings_window_create"), &err);
            }
        }
    }
}
