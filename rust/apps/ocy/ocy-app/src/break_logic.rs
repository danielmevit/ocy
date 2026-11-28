//! Pure, testable logic for break UI presentation, theme tracking, pre-warming, and CLI flags.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use damt_platform::{AppThemeProvider, Clock};
use jiff::{SignedDuration, Timestamp};
use ocy_core::engine::{BreakKind, BreakSurface, EnginePhase, EngineSnapshot, EngineState};
use ocy_core::settings::OcySettings;

use crate::license::LicenseState;
use crate::strings::Strings;

/// Minimum interval between OS app-theme registry re-checks (matches tray 60 s cadence).
pub const APP_THEME_RECHECK_INTERVAL: SignedDuration = SignedDuration::from_secs(60);

/// Pre-create the next break window hidden when a break/warning is due within this lead time.
pub const PREWARM_LEAD_TIME: Duration = Duration::from_secs(3);

/// Drop an unused pre-warmed window once it has been sitting idle for this long.
pub const PREWARM_TTL: Duration = Duration::from_secs(10);

/// Suppression window matching `ScheduleCalculator` (short breaks yield when long break <= 2 min).
const SHORT_BREAK_SUPPRESSION_WINDOW: Duration = Duration::from_secs(120);

/// Whether a short-break (or gentle long-break) card dims every monitor.
///
/// Gentle game cards never dim, even when short breaks are configured as full-screen.
#[must_use]
pub const fn short_break_dims(short_fullscreen: bool, surface: BreakSurface) -> bool {
    short_fullscreen && !matches!(surface, BreakSurface::GentleCard)
}

/// Maps `AppThemeProvider::is_light_app_theme()` to `dark: bool`, falling back to light (`false`).
#[must_use]
pub const fn app_theme_is_dark(is_light_app_theme: Option<bool>) -> bool {
    matches!(is_light_app_theme, Some(false))
}

/// Rate-limited tracker for the OS app light/dark preference (`AppsUseLightTheme`).
pub struct AppThemeTracker {
    clock: Arc<dyn Clock>,
    last_check_utc: Option<Timestamp>,
    dark: bool,
}

impl fmt::Debug for AppThemeTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppThemeTracker")
            .field("last_check_utc", &self.last_check_utc)
            .field("dark", &self.dark)
            .finish_non_exhaustive()
    }
}

impl AppThemeTracker {
    /// Creates a tracker and performs an initial read from `provider`.
    #[must_use]
    pub fn new(provider: &dyn AppThemeProvider, clock: Arc<dyn Clock>) -> Self {
        let mut tracker = Self {
            clock,
            last_check_utc: None,
            dark: false,
        };
        tracker.refresh(provider, true);
        tracker
    }

    /// Current dark-mode flag (`false` = light fallback).
    #[must_use]
    pub const fn is_dark(&self) -> bool {
        self.dark
    }

    /// Re-queries `provider` if `force` is true or at least 60 s have elapsed since the last check.
    /// Returns `true` if `is_dark()` changed.
    pub fn refresh(&mut self, provider: &dyn AppThemeProvider, force: bool) -> bool {
        let now = self.clock.now().timestamp();
        let interval_elapsed = self
            .last_check_utc
            .is_none_or(|last| now.duration_since(last) >= APP_THEME_RECHECK_INTERVAL);

        if !force && !interval_elapsed {
            return false;
        }

        self.last_check_utc = Some(now);
        let next_dark = app_theme_is_dark(provider.is_light_app_theme());
        let changed = next_dark != self.dark;
        self.dark = next_dark;
        changed
    }
}

/// Which break card window should be pre-created hidden ahead of becoming due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrewarmKind {
    ShortBreak,
    Warning,
    LongBreak,
}

/// Pure decision logic for pre-creating and expiring hidden break card windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrewarmPlanner;

impl PrewarmPlanner {
    /// Returns the next window kind to pre-warm if a break or warning is due within [`PREWARM_LEAD_TIME`].
    #[must_use]
    pub fn decide(
        snapshot: &EngineSnapshot,
        settings: &OcySettings,
        warning_remaining: Option<Duration>,
    ) -> Option<PrewarmKind> {
        match snapshot.phase {
            EnginePhase::Working => {
                if settings.long_breaks.enabled
                    && let Some(until_long) = snapshot.time_until_long_break
                {
                    if settings.long_breaks.warning_seconds > 0 {
                        let until_warning =
                            until_long.saturating_sub(settings.long_breaks.warning_duration());
                        if until_warning <= PREWARM_LEAD_TIME {
                            return Some(PrewarmKind::Warning);
                        }
                    } else if until_long <= PREWARM_LEAD_TIME {
                        return Some(PrewarmKind::LongBreak);
                    }
                }

                if settings.short_breaks.enabled
                    && let Some(until_short) = snapshot.time_until_short_break
                {
                    let suppressed = settings.long_breaks.enabled
                        && snapshot
                            .time_until_long_break
                            .is_some_and(|l| l <= SHORT_BREAK_SUPPRESSION_WINDOW);
                    if !suppressed && until_short <= PREWARM_LEAD_TIME {
                        return Some(PrewarmKind::ShortBreak);
                    }
                }

                None
            }
            EnginePhase::LongBreakWarning => {
                let remaining = warning_remaining.or(snapshot.time_until_long_break)?;
                if remaining <= PREWARM_LEAD_TIME {
                    Some(PrewarmKind::LongBreak)
                } else {
                    None
                }
            }
            EnginePhase::ShortBreak
            | EnginePhase::LongBreak
            | EnginePhase::Deferred
            | EnginePhase::Paused
            | EnginePhase::Away => None,
        }
    }

    /// Returns `true` when an unused pre-warmed window should be dropped.
    #[must_use]
    pub fn should_drop(phase: EnginePhase, age: Duration) -> bool {
        age >= PREWARM_TTL || matches!(phase, EnginePhase::Paused | EnginePhase::Away)
    }
}

/// Button and strict-banner visibility for [`ocy_ui::WarningCard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarningButtons {
    pub show_start_now: bool,
    pub show_postpone: bool,
    pub show_skip: bool,
    pub show_strict_line: bool,
}

/// The sentence a screen reader announces when a break card appears. The cards never take focus
/// (they must not steal typing), so without this Narrator would stay silent; see
/// `win32_window::announce_to_screen_reader`. Empty parts are skipped and parts are joined as
/// sentences, without doubling punctuation the translation already ends with.
#[must_use]
pub fn break_announcement(parts: &[&str]) -> String {
    let mut out = String::new();
    for part in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if !out.is_empty() {
            let ended = out.ends_with(['.', '!', '?', ':', '…', '。', '！', '？', '।', '؟']);
            out.push_str(if ended { " " } else { ". " });
        }
        out.push_str(part);
    }
    out
}

/// Which game actions a short-break card shows: `(not during this game, take the long break)`.
///
/// Only a gentle game card shows them. "Take the long break" is only on the gentle long card.
#[must_use]
pub fn gentle_card_actions(kind: BreakKind, surface: BreakSurface) -> (bool, bool) {
    let gentle = surface == BreakSurface::GentleCard;
    (gentle, gentle && kind == BreakKind::Long)
}

/// What the warning card announces: its heading plus the buttons it actually shows.
#[must_use]
pub fn warning_announcement(
    heading: &str,
    buttons: WarningButtons,
    start_now: &str,
    postpone: &str,
    skip: &str,
    strict_line: &str,
) -> String {
    let pick = |show: bool, text: &str| if show { text.to_owned() } else { String::new() };
    let parts = [
        heading.to_owned(),
        pick(buttons.show_strict_line, strict_line),
        pick(buttons.show_start_now, start_now),
        pick(buttons.show_postpone, postpone),
        pick(buttons.show_skip, skip),
    ];
    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    break_announcement(&refs)
}

/// Computes which controls are visible on the warning card.
#[must_use]
pub const fn warning_buttons(strict_mode: bool, postpones_left: u32) -> WarningButtons {
    WarningButtons {
        show_start_now: true,
        show_postpone: !strict_mode && postpones_left > 0,
        show_skip: !strict_mode,
        show_strict_line: strict_mode,
    }
}

/// Returns `true` when the "End break" button is visible on [`ocy_ui::LongBreakCard`].
#[must_use]
pub const fn long_break_can_end(strict_mode: bool) -> bool {
    !strict_mode
}

/// Formats the warning card heading (e.g., `"Long break in 0:42"`).
#[must_use]
pub fn warning_heading(strings: &Strings, countdown: Duration) -> String {
    let formatted = strings.format_countdown_localized(countdown);
    strings.format("warning.title", &[("time", &formatted)])
}

/// Formats the warning card postpone button label (e.g., `"Postpone 3 min"`).
#[must_use]
pub fn warning_postpone_label(strings: &Strings, postpone_minutes: i32) -> String {
    let count = u64::try_from(postpone_minutes.max(0)).unwrap_or(0);
    strings.format_plural("warning.postpone", count, &[("minutes", &postpone_minutes)])
}

/// Formats the long-break countdown numeral string (e.g., `"4:12"`).
#[must_use]
pub fn long_break_countdown_text(remaining: Duration) -> String {
    Strings::format_countdown(remaining)
}

/// Formats the long-break countdown numeral string using localized `"time.countdown"`.
#[must_use]
pub fn long_break_countdown_text_localized(strings: &Strings, remaining: Duration) -> String {
    strings.format_countdown_localized(remaining)
}

/// Progress (`1.0` -> `0.0` remaining ratio) for [`ocy_ui::ShortBreakCard`], matching C#.
#[must_use]
pub fn short_break_progress(remaining: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 0.0;
    }
    let ratio = remaining.as_secs_f64() / total.as_secs_f64();
    (ratio as f32).clamp(0.0, 1.0)
}

/// Progress (`0.0` -> `1.0` elapsed ratio) for [`ocy_ui::LongBreakCard`], matching C#.
#[must_use]
pub fn long_break_progress(remaining: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 0.0;
    }
    let elapsed = total.saturating_sub(remaining);
    let ratio = elapsed.as_secs_f64() / total.as_secs_f64();
    (ratio as f32).clamp(0.0, 1.0)
}

/// Hidden `--demo-break` target for lead verification on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DemoBreak {
    Short,
    Warning,
    Long,
}

impl DemoBreak {
    /// Parses `"short"`, `"warning"`, or `"long"` (case-insensitive).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("short") {
            Some(Self::Short)
        } else if value.eq_ignore_ascii_case("warning") {
            Some(Self::Warning)
        } else if value.eq_ignore_ascii_case("long") {
            Some(Self::Long)
        } else {
            None
        }
    }
}

/// Hidden `--demo-license <state>` value for lead verification of the Store trial UI on Windows.
///
/// The flag is only honoured when the run is already isolated (`is_probe_mode` in
/// `instance.rs` or `--instance-scope`) and the binary is built with `--features store`
/// ([`demo_license_is_honoured`]); it never makes a run isolated by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DemoLicense {
    /// Active full (purchased) license.
    Full,
    /// Active trial with exactly 5 days left.
    Trial,
    /// Active trial ending in less than 24 hours, so the UI shows "1 day left".
    TrialLastDay,
    /// Expired trial (reminders suspended until purchased).
    TrialEnded,
    /// Unknown license state (behaves like an active trial).
    Unknown,
}

impl DemoLicense {
    /// Parses `"full"`, `"trial"`, `"trial-last-day"`, `"trial-ended"`, or `"unknown"`
    /// (case-insensitive).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("full") {
            Some(Self::Full)
        } else if value.eq_ignore_ascii_case("trial") {
            Some(Self::Trial)
        } else if value.eq_ignore_ascii_case("trial-last-day") {
            Some(Self::TrialLastDay)
        } else if value.eq_ignore_ascii_case("trial-ended") {
            Some(Self::TrialEnded)
        } else if value.eq_ignore_ascii_case("unknown") {
            Some(Self::Unknown)
        } else {
            None
        }
    }
}

/// Returns `true` when a parsed `--demo-license` flag is honoured: the run is isolated
/// (demo/probe flags or `--instance-scope`) and the binary is built with `--features store`.
///
/// Pure helper so the honoured-or-ignored decision can be tested; `main.rs` passes its own
/// `is_isolated` and prints one stderr line when this returns `false`.
#[must_use]
pub const fn demo_license_is_honoured(is_isolated: bool) -> bool {
    is_isolated && crate::license::STORE_FEATURE_ENABLED
}

/// Seconds left in the `--demo-license trial` demo trial (5 whole days).
const DEMO_TRIAL_SECS: i64 = 5 * 86_400;
/// Seconds left in the `--demo-license trial-last-day` demo trial (under 24 hours, so the UI
/// shows "1 day left").
const DEMO_TRIAL_LAST_DAY_SECS: i64 = 12 * 3_600;

/// Maps a demo license value plus `now` to the [`LicenseState`] the demo run starts with.
///
/// Pure: `trial` gives `trial_days_left == 5`, `trial-last-day` gives `1` (less than 24 hours
/// left), and `trial-ended` gives [`LicenseState::TrialExpired`].
#[must_use]
pub fn demo_license_state(demo: DemoLicense, now: Timestamp) -> LicenseState {
    let trial_ends_in =
        |secs: i64| Timestamp::from_second(now.as_second().saturating_add(secs)).unwrap_or(now);
    match demo {
        DemoLicense::Full => LicenseState::Full,
        DemoLicense::Trial => LicenseState::Trial {
            ends_at: trial_ends_in(DEMO_TRIAL_SECS),
        },
        DemoLicense::TrialLastDay => LicenseState::Trial {
            ends_at: trial_ends_in(DEMO_TRIAL_LAST_DAY_SECS),
        },
        DemoLicense::TrialEnded => LicenseState::TrialExpired,
        DemoLicense::Unknown => LicenseState::Unknown,
    }
}

/// Parsed CLI flags for `Ocy`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CliArgs {
    pub report_memory: bool,
    pub demo_break: Option<DemoBreak>,
    pub demo_in_secs: Option<u64>,
    pub demo_settings: bool,
    pub demo_license: Option<DemoLicense>,
    pub soak_cycles: Option<u32>,
    /// `--demo-crash-card` (or `=card`) or `--demo-crash-card=preview`: the crash-report
    /// card with a fake, already scrubbed report (isolated demo run).
    pub demo_crash_card: Option<crate::crash_card::DemoCrashCard>,
}

/// Parses command-line arguments (excluding or including argv[0]).
#[must_use]
pub fn parse_cli_args(args: impl IntoIterator<Item = String>) -> CliArgs {
    let mut out = CliArgs::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--report-memory" {
            out.report_memory = true;
        } else if arg == "--demo-settings" {
            out.demo_settings = true;
        } else if arg == "--demo-crash-card" {
            out.demo_crash_card = Some(crate::crash_card::DemoCrashCard::Card);
        } else if let Some(rest) = arg.strip_prefix("--demo-crash-card=") {
            out.demo_crash_card = crate::crash_card::DemoCrashCard::parse(rest);
        } else if arg == "--demo-license" {
            if let Some(next) = iter.next() {
                out.demo_license = DemoLicense::parse(&next);
            }
        } else if let Some(rest) = arg.strip_prefix("--demo-license=") {
            out.demo_license = DemoLicense::parse(rest);
        } else if arg == "--demo-break" {
            if let Some(next) = iter.next() {
                out.demo_break = DemoBreak::parse(&next);
            }
        } else if let Some(rest) = arg.strip_prefix("--demo-break=") {
            out.demo_break = DemoBreak::parse(rest);
        } else if arg == "--demo-in" {
            if let Some(next) = iter.next() {
                out.demo_in_secs = next.parse::<u64>().ok();
            }
        } else if let Some(rest) = arg.strip_prefix("--demo-in=") {
            out.demo_in_secs = rest.parse::<u64>().ok();
        } else if arg == "--soak" {
            if let Some(next) = iter.next() {
                out.soak_cycles = crate::soak::validate_soak_cycles(&next).ok();
            }
        } else if let Some(rest) = arg.strip_prefix("--soak=") {
            out.soak_cycles = crate::soak::validate_soak_cycles(rest).ok();
        }
    }
    out
}

/// Startup configuration computed for `--demo-break` and optional `--demo-in <secs>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoLaunchPlan {
    pub initial_state: Option<EngineState>,
    pub trigger_immediately: bool,
    pub initial_clock_offset_secs: i64,
}

/// Computes engine settings adjustments and initial state for a hidden demo run so `--demo-in N`
/// drives pre-warming and break triggering through the real `BreakEngine::tick()` path.
#[must_use]
pub fn build_demo_launch_plan(
    demo: DemoBreak,
    demo_in_secs: Option<u64>,
    settings: &mut OcySettings,
) -> DemoLaunchPlan {
    settings.clamp();
    settings.pause_during_fullscreen = false;
    settings.inactivity_tracking = false;

    let delay_secs = demo_in_secs.unwrap_or(0);
    if delay_secs == 0 {
        if demo == DemoBreak::Warning {
            settings.long_breaks.enabled = true;
            if settings.long_breaks.warning_seconds <= 0 {
                settings.long_breaks.warning_seconds = 60;
            }
            let warning_at = settings
                .long_breaks
                .interval()
                .saturating_sub(settings.long_breaks.warning_duration());
            return DemoLaunchPlan {
                initial_state: Some(EngineState {
                    since_long_break: warning_at,
                    ..Default::default()
                }),
                trigger_immediately: true,
                initial_clock_offset_secs: -1,
            };
        }
        return DemoLaunchPlan {
            initial_state: None,
            trigger_immediately: true,
            initial_clock_offset_secs: 0,
        };
    }

    let delay = Duration::from_secs(delay_secs);
    match demo {
        DemoBreak::Short => {
            settings.short_breaks.enabled = true;
            settings.long_breaks.enabled = false;
            let since_short = settings.short_breaks.interval().saturating_sub(delay);
            DemoLaunchPlan {
                initial_state: Some(EngineState {
                    since_short_break: since_short,
                    since_long_break: Duration::ZERO,
                    ..Default::default()
                }),
                trigger_immediately: false,
                initial_clock_offset_secs: 0,
            }
        }
        DemoBreak::Warning => {
            settings.short_breaks.enabled = false;
            settings.long_breaks.enabled = true;
            if settings.long_breaks.warning_seconds <= 0 {
                settings.long_breaks.warning_seconds = 60;
            }
            let warning_at = settings
                .long_breaks
                .interval()
                .saturating_sub(settings.long_breaks.warning_duration());
            DemoLaunchPlan {
                initial_state: Some(EngineState {
                    since_long_break: warning_at.saturating_sub(delay),
                    ..Default::default()
                }),
                trigger_immediately: false,
                initial_clock_offset_secs: 0,
            }
        }
        DemoBreak::Long => {
            settings.short_breaks.enabled = false;
            settings.long_breaks.enabled = true;
            settings.long_breaks.warning_seconds = 0;
            let since_long = settings.long_breaks.interval().saturating_sub(delay);
            DemoLaunchPlan {
                initial_state: Some(EngineState {
                    since_long_break: since_long,
                    ..Default::default()
                }),
                trigger_immediately: false,
                initial_clock_offset_secs: 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::short_break_dims;
    use ocy_core::engine::BreakSurface;

    #[test]
    fn gentle_card_never_dims_and_a_standard_short_dims_only_when_configured() {
        assert!(!short_break_dims(true, BreakSurface::GentleCard));
        assert!(!short_break_dims(false, BreakSurface::GentleCard));
        assert!(!short_break_dims(false, BreakSurface::Standard));
        assert!(short_break_dims(true, BreakSurface::Standard));
    }
}
