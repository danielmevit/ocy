//! Hidden `--soak <n>` break-window leak probe, CLI parsing, and slope maths.

use std::cell::RefCell;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use damt_platform::{Clock, SystemClock};
use jiff::{SignedDuration, ZonedDateTime};
use ocy_core::engine::EnginePhase;
use ocy_core::settings::OcySettings;
use slint::Timer;

use crate::coordinator::{BreakUiCoordinator, OverlayBreakWindows, SettingsWindow};
use crate::host::AppHost;
use crate::slint_breaks::SlintBreakWindows;
use crate::tray::TraySink;

/// Minimum allowed `--soak` cycle count.
pub const SOAK_MIN_CYCLES: u32 = 1;
/// Maximum allowed `--soak` cycle count.
pub const SOAK_MAX_CYCLES: u32 = 200;

/// Error returned when `--soak` is missing its value, not an integer, or outside `1..=200`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoakArgError {
    /// `--soak` appeared at the end of the argument list with no following value.
    MissingValue,
    /// The `--soak` value could not be parsed as an unsigned integer.
    InvalidValue(String),
    /// The parsed `--soak` value was outside `1..=200`.
    OutOfRange(u32),
}

impl fmt::Display for SoakArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingValue => f.write_str("missing value for --soak"),
            Self::InvalidValue(raw) => write!(
                f,
                "invalid --soak value {raw:?}: expected an integer in {SOAK_MIN_CYCLES}..={SOAK_MAX_CYCLES}"
            ),
            Self::OutOfRange(n) => write!(
                f,
                "invalid --soak value {n}: expected {SOAK_MIN_CYCLES}..={SOAK_MAX_CYCLES}"
            ),
        }
    }
}

impl std::error::Error for SoakArgError {}

/// Validates a `--soak` cycle count string, requiring an integer in `1..=200`.
pub fn validate_soak_cycles(raw: &str) -> Result<u32, SoakArgError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(SoakArgError::InvalidValue(raw.to_owned()));
    }
    let parsed = trimmed
        .parse::<u32>()
        .map_err(|_| SoakArgError::InvalidValue(raw.to_owned()))?;
    if !(SOAK_MIN_CYCLES..=SOAK_MAX_CYCLES).contains(&parsed) {
        return Err(SoakArgError::OutOfRange(parsed));
    }
    Ok(parsed)
}

/// Parses and validates `--soak <n>` / `--soak=<n>` from CLI arguments (excluding `argv[0]`).
pub fn parse_soak_cycles<I, S>(args: I) -> Result<Option<u32>, SoakArgError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cycles = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        if arg == "--demo-break" || arg == "--demo-in" || arg == "--instance-scope" {
            let _ = iter.next();
        } else if arg == "--soak" {
            let Some(next) = iter.next() else {
                return Err(SoakArgError::MissingValue);
            };
            cycles = Some(validate_soak_cycles(next.as_ref())?);
        } else if let Some(rest) = arg.strip_prefix("--soak=") {
            cycles = Some(validate_soak_cycles(rest)?);
        }
    }
    Ok(cycles)
}

/// Thread-safe offset clock layered on top of [`SystemClock`], used by `--demo-break` and `--soak`.
#[derive(Debug)]
pub struct OffsetClock {
    offset_secs: AtomicI64,
}

impl OffsetClock {
    /// Creates an [`OffsetClock`] starting at `offset_secs` relative to [`SystemClock`].
    #[must_use]
    pub const fn new(offset_secs: i64) -> Self {
        Self {
            offset_secs: AtomicI64::new(offset_secs),
        }
    }

    /// Sets the clock offset in seconds.
    pub fn set_offset_secs(&self, secs: i64) {
        self.offset_secs.store(secs, Ordering::Relaxed);
    }

    /// Advances the clock offset by `delta_secs` seconds.
    pub fn advance_secs(&self, delta_secs: i64) {
        self.offset_secs.fetch_add(delta_secs, Ordering::Relaxed);
    }
}

impl Clock for OffsetClock {
    fn now(&self) -> ZonedDateTime {
        let now = SystemClock.now();
        let secs = self.offset_secs.load(Ordering::Relaxed);
        if secs == 0 {
            return now;
        }
        now.checked_add(SignedDuration::from_secs(secs))
            .unwrap_or(now)
    }
}

/// Configures isolated settings so `--soak` drives every break phase reliably without user holds.
pub fn build_soak_settings(settings: &mut OcySettings) {
    settings.clamp();
    settings.pause_during_fullscreen = false;
    settings.inactivity_tracking = false;
    settings.natural_moments = false;
    settings.strict_mode = false;
    settings.short_breaks.enabled = true;
    settings.short_breaks.interval_minutes = 60;
    settings.long_breaks.enabled = true;
    settings.long_breaks.interval_minutes = 15;
    settings.long_breaks.warning_seconds = 60;
}

/// Queries the current process's `(private_bytes, working_set_bytes)`.
#[must_use]
pub fn process_memory_bytes() -> (u64, u64) {
    #[cfg(windows)]
    {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;

        let mut pmc = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: `GetCurrentProcess` returns a valid pseudo-handle and `pmc` is an aligned
        // out-parameter with `cb` initialized to `size_of::<PROCESS_MEMORY_COUNTERS_EX>()`.
        let res = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&raw mut pmc).cast::<PROCESS_MEMORY_COUNTERS>(),
                pmc.cb,
            )
        };
        if res.is_ok() {
            (pmc.PrivateUsage as u64, pmc.WorkingSetSize as u64)
        } else {
            (0, 0)
        }
    }

    #[cfg(not(windows))]
    {
        let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
            return (0, 0);
        };
        let mut rss_kb = 0u64;
        let mut anon_kb = 0u64;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                rss_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("RssAnon:") {
                anon_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
        (anon_kb * 1024, rss_kb * 1024)
    }
}

/// Measurements captured after a single completed soak cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoakCycleSample {
    pub cycle: u32,
    pub private_bytes: u64,
    pub working_set_bytes: u64,
    pub live_break_windows: usize,
    pub art_cache_entries: usize,
    pub overlays: usize,
}

/// Aggregated statistics printed at the end of a `--soak` run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoakSummary {
    pub cycles: usize,
    pub first_private: u64,
    pub last_private: u64,
    pub max_private: u64,
    pub growth_after_cycle3_per_cycle: f64,
}

/// Computes the ordinary least-squares slope (bytes per cycle) of `private_bytes` over cycles
/// `3..=n` (1-indexed, i.e. `private_bytes[2..]`). Returns `0.0` when fewer than 2 points exist.
#[must_use]
pub fn least_squares_slope_after_cycle3(private_bytes: &[u64]) -> f64 {
    let Some(tail) = private_bytes.get(2..) else {
        return 0.0;
    };
    if tail.len() < 2 {
        return 0.0;
    }

    let m = tail.len() as f64;
    let mean_x = (m - 1.0) / 2.0;
    let mean_y = tail.iter().map(|&y| y as f64).sum::<f64>() / m;

    let mut numerator = 0.0_f64;
    let mut denominator = 0.0_f64;
    for (idx, &y) in tail.iter().enumerate() {
        let dx = idx as f64 - mean_x;
        let dy = y as f64 - mean_y;
        numerator += dx * dy;
        denominator += dx * dx;
    }

    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

/// Summarizes a sequence of per-cycle `private_bytes` samples.
#[must_use]
pub fn summarize_soak(private_samples: &[u64]) -> SoakSummary {
    let cycles = private_samples.len();
    let first_private = private_samples.first().copied().unwrap_or(0);
    let last_private = private_samples.last().copied().unwrap_or(0);
    let max_private = private_samples.iter().copied().max().unwrap_or(0);
    let growth_after_cycle3_per_cycle = least_squares_slope_after_cycle3(private_samples);
    SoakSummary {
        cycles,
        first_private,
        last_private,
        max_private,
        growth_after_cycle3_per_cycle,
    }
}

/// Formats one cycle output line for `--soak`.
#[must_use]
pub fn format_soak_cycle_line(sample: &SoakCycleSample) -> String {
    format!(
        "Ocy soak cycle={} private_bytes={} working_set_bytes={} live_break_windows={} art_cache_entries={} overlays={}",
        sample.cycle,
        sample.private_bytes,
        sample.working_set_bytes,
        sample.live_break_windows,
        sample.art_cache_entries,
        sample.overlays,
    )
}

/// Formats the final summary line for `--soak`.
#[must_use]
pub fn format_soak_summary_line(summary: &SoakSummary) -> String {
    format!(
        "Ocy soak summary cycles={} first_private={} last_private={} max_private={} growth_after_cycle3_per_cycle={:.2}",
        summary.cycles,
        summary.first_private,
        summary.last_private,
        summary.max_private,
        summary.growth_after_cycle3_per_cycle,
    )
}

/// Per-phase durations used by the soak driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoakTimings {
    pub prewarm_lead: Duration,
    pub short_animate: Duration,
    pub warning_show: Duration,
    pub long_show: Duration,
    pub settle: Duration,
}

impl SoakTimings {
    /// Production `--soak` timings: ~1.5 s short, ~1 s warning, ~1.5 s long, ~300 ms settle.
    pub const LIVE: Self = Self {
        prewarm_lead: Duration::from_millis(100),
        short_animate: Duration::from_millis(1500),
        warning_show: Duration::from_millis(1000),
        long_show: Duration::from_millis(1500),
        settle: Duration::from_millis(300),
    };
}

/// Callbacks and environment hooks injected into [`start_soak_probe`].
pub struct SoakHooks {
    pub advance_clock_secs: Rc<dyn Fn(i64)>,
    pub on_schedule_changed: Rc<dyn Fn()>,
    pub sample_memory: Rc<dyn Fn() -> (u64, u64)>,
    pub on_cycle: Rc<dyn Fn(SoakCycleSample)>,
    pub on_done: Rc<dyn Fn(SoakSummary)>,
}

impl fmt::Debug for SoakHooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoakHooks").finish_non_exhaustive()
    }
}

struct SoakRunner<S: SettingsWindow + 'static, T: TraySink + 'static> {
    host: Weak<RefCell<AppHost<OverlayBreakWindows<SlintBreakWindows>, S, T>>>,
    timings: SoakTimings,
    total_cycles: u32,
    hooks: SoakHooks,
    samples: RefCell<Vec<u64>>,
}

impl<S: SettingsWindow + 'static, T: TraySink + 'static> SoakRunner<S, T> {
    fn with_host<R>(
        &self,
        f: impl FnOnce(&mut AppHost<OverlayBreakWindows<SlintBreakWindows>, S, T>) -> R,
    ) -> Option<R> {
        let rc = self.host.upgrade()?;
        let Ok(mut host) = rc.try_borrow_mut() else {
            return None;
        };
        Some(f(&mut host))
    }

    fn start_cycle(self: &Rc<Self>, cycle: u32) {
        // 1. Pre-warm short break.
        let prewarmed = self.with_host(|h| {
            h.coordinator_mut()
                .break_windows_mut()
                .inner_mut()
                .prewarm_short_break_now();
        });
        if prewarmed.is_none() {
            return;
        }

        let runner = Rc::clone(self);
        Timer::single_shot(self.timings.prewarm_lead, move || {
            runner.show_short_break(cycle);
        });
    }

    fn show_short_break(self: &Rc<Self>, cycle: u32) {
        let started = self.with_host(|h| {
            let _ = h.start_short_break_now();
        });
        if started.is_none() {
            return;
        }
        (self.hooks.on_schedule_changed)();

        let runner = Rc::clone(self);
        Timer::single_shot(self.timings.short_animate, move || {
            runner.close_short_via_x(cycle);
        });
    }

    fn close_short_via_x(self: &Rc<Self>, cycle: u32) {
        let card = self
            .with_host(|h| h.coordinator().break_windows().inner().active_short_card())
            .flatten();
        if let Some(card) = card {
            card.invoke_close_clicked();
        } else {
            self.with_host(AppHost::skip_current_break);
        }

        let runner = Rc::clone(self);
        Timer::single_shot(Duration::ZERO, move || {
            runner.trigger_and_show_warning(cycle);
        });
    }

    fn trigger_and_show_warning(self: &Rc<Self>, cycle: u32) {
        let triggered = self.with_host(|h| {
            let mut steps = 0u32;
            while h.engine().phase() == EnginePhase::Working && steps < 200 {
                let snap = h.engine().snapshot();
                let step_secs = match snap.time_until_long_break_warning {
                    Some(until) if until > Duration::from_secs(3) => {
                        let target = until.saturating_sub(Duration::from_secs(3));
                        ceil_secs(target).clamp(1, 10)
                    }
                    Some(until) => ceil_secs(until).clamp(1, 10),
                    None => 10,
                };
                let delta = i64::try_from(step_secs).unwrap_or(10);
                (self.hooks.advance_clock_secs)(delta);
                h.on_tick();
                steps = steps.saturating_add(1);
            }
        });
        if triggered.is_none() {
            return;
        }
        (self.hooks.on_schedule_changed)();

        let runner = Rc::clone(self);
        Timer::single_shot(self.timings.warning_show, move || {
            runner.start_long_via_warning(cycle);
        });
    }

    fn start_long_via_warning(self: &Rc<Self>, cycle: u32) {
        let card = self
            .with_host(|h| {
                h.coordinator()
                    .break_windows()
                    .inner()
                    .active_warning_card()
            })
            .flatten();
        if let Some(card) = card {
            card.invoke_start_now();
        } else {
            self.with_host(AppHost::start_long_break_now);
        }

        let runner = Rc::clone(self);
        Timer::single_shot(self.timings.long_show, move || {
            runner.end_long_via_button(cycle);
        });
    }

    fn end_long_via_button(self: &Rc<Self>, cycle: u32) {
        let card = self
            .with_host(|h| h.coordinator().break_windows().inner().active_long_card())
            .flatten();
        if let Some(card) = card {
            card.invoke_end_break();
        } else {
            self.with_host(AppHost::skip_current_break);
        }

        let runner = Rc::clone(self);
        Timer::single_shot(self.timings.settle, move || {
            runner.finish_cycle(cycle);
        });
    }

    fn finish_cycle(self: &Rc<Self>, cycle: u32) {
        let Some((live_break_windows, art_cache_entries, overlays)) = self.with_host(|h| {
            let bw = h.coordinator().break_windows();
            (
                bw.inner().live_break_windows(),
                bw.inner().art_cache_entries(),
                bw.live_overlay_count(),
            )
        }) else {
            return;
        };

        let (private_bytes, working_set_bytes) = (self.hooks.sample_memory)();
        self.samples.borrow_mut().push(private_bytes);

        (self.hooks.on_cycle)(SoakCycleSample {
            cycle,
            private_bytes,
            working_set_bytes,
            live_break_windows,
            art_cache_entries,
            overlays,
        });

        if cycle < self.total_cycles {
            self.start_cycle(cycle + 1);
        } else {
            let summary = summarize_soak(&self.samples.borrow());
            (self.hooks.on_done)(summary);
        }
    }
}

const fn ceil_secs(d: Duration) -> u64 {
    d.as_secs()
        .saturating_add(if d.subsec_nanos() > 0 { 1 } else { 0 })
}

/// Starts an asynchronous soak run of `total_cycles` back-to-back break cycles on the Slint loop.
pub fn start_soak_probe<S, T>(
    host: Weak<RefCell<AppHost<OverlayBreakWindows<SlintBreakWindows>, S, T>>>,
    timings: SoakTimings,
    total_cycles: u32,
    hooks: SoakHooks,
) where
    S: SettingsWindow + 'static,
    T: TraySink + 'static,
{
    let runner = Rc::new(SoakRunner {
        host,
        timings,
        total_cycles,
        hooks,
        samples: RefCell::new(Vec::with_capacity(total_cycles as usize)),
    });
    if total_cycles == 0 {
        let summary = summarize_soak(&[]);
        (runner.hooks.on_done)(summary);
        return;
    }
    runner.start_cycle(1);
}

/// Borrow helper for [`BreakUiCoordinator`] callers needing access to [`AppHost`] without holding
/// the borrow across Slint callbacks.
pub fn coordinator_is_idle<S: SettingsWindow, T: TraySink>(
    coordinator: &BreakUiCoordinator<OverlayBreakWindows<SlintBreakWindows>, S>,
    _tray_marker: Option<&T>,
) -> bool {
    let bw = coordinator.break_windows();
    bw.inner().live_break_windows() == 0
        && bw.inner().art_cache_entries() == 0
        && bw.live_overlay_count() == 0
}
