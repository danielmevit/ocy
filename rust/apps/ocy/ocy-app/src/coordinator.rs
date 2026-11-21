//! Break window coordination and native overlays (port of `Coordinating/BreakUiCoordinator.cs`).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use damt_overlay::{Dims, EdgeFrames, enumerate_monitors};
use damt_platform::{Clock, PlatformServices, SystemClock};
use ocy_core::engine::{BreakInfo, BreakKind, EngineEvent, EnginePhase, EngineSnapshot};
use ocy_core::settings::OcySettings;

use crate::break_logic::AppThemeTracker;
use crate::layout::non_active_monitors;
use crate::tokens::{
    EDGE_FRAME_THICKNESS_DIP, attention_frame_color, edge_frame_color, overlay_dim_color,
};

/// UI surface for displaying break cards and overlays (port of `IBreakWindows`).
pub trait BreakWindows {
    /// Synchronizes current engine snapshot and settings before events or ticks are processed.
    fn sync_state(&mut self, _snapshot: &EngineSnapshot, _settings: &OcySettings) {}

    /// Notifies the break UI on every 1 s engine tick (for pre-warming and 60 s theme checks).
    fn on_tick(&mut self, _snapshot: &EngineSnapshot) {}

    /// Displays the short-break card and any secondary-monitor edge frames.
    fn show_short_break(&mut self, info: &BreakInfo);

    /// Displays the pre-long-break warning card and any secondary-monitor edge frames.
    fn show_warning(&mut self, countdown: Duration);

    /// Displays the long-break card and full-monitor dims.
    fn show_long_break(&mut self, info: &BreakInfo);

    /// Updates the visible countdown/progress on the active break or warning card.
    fn update_countdown(&mut self, snapshot: &EngineSnapshot);

    /// Closes all active break/warning cards and overlays.
    fn close_all(&mut self);
}

/// UI surface for displaying the Settings window (port of `ISettingsWindow`).
pub trait SettingsWindow {
    /// Synchronizes current engine settings before `show()` or after external updates.
    fn sync_settings(&mut self, _settings: &OcySettings) {}

    /// Opens or focuses the Settings window.
    fn show(&mut self);
}

/// Log-only stub implementation of [`BreakWindows`] while `ocy-ui` cards are built in parallel.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullBreakWindows;

impl BreakWindows for NullBreakWindows {
    fn show_short_break(&mut self, info: &BreakInfo) {
        eprintln!(
            "NullBreakWindows: show_short_break(exercise={}, duration={}s)",
            info.exercise_id,
            info.total_duration.as_secs()
        );
    }

    fn show_warning(&mut self, countdown: Duration) {
        eprintln!(
            "NullBreakWindows: show_warning(countdown={}s)",
            countdown.as_secs()
        );
    }

    fn show_long_break(&mut self, info: &BreakInfo) {
        eprintln!(
            "NullBreakWindows: show_long_break(exercise={}, duration={}s)",
            info.exercise_id,
            info.total_duration.as_secs()
        );
    }

    fn update_countdown(&mut self, _snapshot: &EngineSnapshot) {}

    fn close_all(&mut self) {
        eprintln!("NullBreakWindows: close_all");
    }
}

/// Log-only stub implementation of [`SettingsWindow`] while `ocy-ui` is built in parallel.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSettingsWindow;

impl SettingsWindow for NullSettingsWindow {
    fn show(&mut self) {
        eprintln!("NullSettingsWindow: show");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeFrameKind {
    ShortBreak,
    Warning,
}

/// [`BreakWindows`] wrapper that drives real native [`EdgeFrames`] and [`Dims`] overlays while
/// delegating card windows to an inner [`BreakWindows`].
pub struct OverlayBreakWindows<W: BreakWindows> {
    inner: W,
    platform: Arc<dyn PlatformServices>,
    edge_frames: Option<EdgeFrames>,
    edge_frame_kind: Option<EdgeFrameKind>,
    dims: Option<Dims>,
    theme_tracker: AppThemeTracker,
    short_break_fullscreen: bool,
}

impl<W: BreakWindows + fmt::Debug> fmt::Debug for OverlayBreakWindows<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OverlayBreakWindows")
            .field("inner", &self.inner)
            .field("edge_frames", &self.edge_frames)
            .field("dims", &self.dims)
            .field("dark_theme", &self.theme_tracker.is_dark())
            .field("short_break_fullscreen", &self.short_break_fullscreen)
            .finish_non_exhaustive()
    }
}

impl<W: BreakWindows> OverlayBreakWindows<W> {
    /// Wraps `inner` with real Win32 overlays queried through `platform`.
    #[must_use]
    pub fn new(inner: W, platform: Arc<dyn PlatformServices>) -> Self {
        Self::with_clock(inner, platform, Arc::new(SystemClock))
    }

    /// Wraps `inner` with an explicit [`Clock`] for deterministic theme re-check tests.
    #[must_use]
    pub fn with_clock(
        inner: W,
        platform: Arc<dyn PlatformServices>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let theme_tracker = AppThemeTracker::new(platform.app_theme(), clock);
        Self {
            inner,
            platform,
            edge_frames: None,
            edge_frame_kind: None,
            dims: None,
            theme_tracker,
            short_break_fullscreen: false,
        }
    }

    /// Returns a shared reference to the wrapped inner [`BreakWindows`].
    #[must_use]
    pub const fn inner(&self) -> &W {
        &self.inner
    }

    /// Returns a mutable reference to the wrapped inner [`BreakWindows`].
    pub const fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    fn close_overlays(&mut self) {
        self.edge_frames = None;
        self.edge_frame_kind = None;
        self.dims = None;
    }

    fn refresh_theme(&mut self) {
        if self.theme_tracker.refresh(self.platform.app_theme(), false)
            && let (Some(frames), Some(kind)) = (&mut self.edge_frames, self.edge_frame_kind)
        {
            let dark = self.theme_tracker.is_dark();
            let color = match kind {
                EdgeFrameKind::ShortBreak => edge_frame_color(dark),
                EdgeFrameKind::Warning => attention_frame_color(dark),
            };
            frames.set_color(color);
        }
    }

    fn show_edge_frames(&mut self, color: damt_overlay::Rgba, kind: EdgeFrameKind) {
        let monitors = enumerate_monitors();
        if monitors.is_empty() {
            return;
        }
        let active_bounds = self.platform.active_screen().active_screen_bounds();
        let other_monitors = non_active_monitors(&monitors, active_bounds);
        if !other_monitors.is_empty() {
            self.edge_frames = Some(EdgeFrames::show(
                &other_monitors,
                color,
                EDGE_FRAME_THICKNESS_DIP,
            ));
            self.edge_frame_kind = Some(kind);
        }
    }

    fn show_dims_all_monitors(&mut self) {
        let monitors = enumerate_monitors();
        if !monitors.is_empty() {
            self.dims = Some(Dims::show(
                &monitors,
                overlay_dim_color(self.theme_tracker.is_dark()),
            ));
        }
    }
}

impl<W: BreakWindows> BreakWindows for OverlayBreakWindows<W> {
    fn sync_state(&mut self, snapshot: &EngineSnapshot, settings: &OcySettings) {
        self.short_break_fullscreen = settings.short_breaks.fullscreen;
        self.inner.sync_state(snapshot, settings);
    }

    fn on_tick(&mut self, snapshot: &EngineSnapshot) {
        self.refresh_theme();
        self.inner.on_tick(snapshot);
    }

    fn show_short_break(&mut self, info: &BreakInfo) {
        self.close_all();
        self.refresh_theme();
        if self.short_break_fullscreen {
            self.show_dims_all_monitors();
        } else {
            self.show_edge_frames(
                edge_frame_color(self.theme_tracker.is_dark()),
                EdgeFrameKind::ShortBreak,
            );
        }
        self.inner.show_short_break(info);
    }

    fn show_warning(&mut self, countdown: Duration) {
        self.close_all();
        self.refresh_theme();
        self.show_edge_frames(
            attention_frame_color(self.theme_tracker.is_dark()),
            EdgeFrameKind::Warning,
        );
        self.inner.show_warning(countdown);
    }

    fn show_long_break(&mut self, info: &BreakInfo) {
        self.close_all();
        self.refresh_theme();
        self.show_dims_all_monitors();
        self.inner.show_long_break(info);
    }

    fn update_countdown(&mut self, snapshot: &EngineSnapshot) {
        self.inner.update_countdown(snapshot);
    }

    fn close_all(&mut self) {
        self.close_overlays();
        self.inner.close_all();
    }
}

/// Coordinates engine events and tick updates against [`BreakWindows`] and [`SettingsWindow`].
#[derive(Debug)]
pub struct BreakUiCoordinator<W: BreakWindows, S: SettingsWindow> {
    break_windows: W,
    settings_window: S,
}

impl<W: BreakWindows, S: SettingsWindow> BreakUiCoordinator<W, S> {
    /// Creates a coordinator managing `break_windows` and `settings_window`.
    #[must_use]
    pub fn new(break_windows: W, settings_window: S) -> Self {
        Self {
            break_windows,
            settings_window,
        }
    }

    /// Dispatches a slice of [`EngineEvent`]s emitted by `ocy_core::engine::BreakEngine`.
    pub fn handle_events(&mut self, events: &[EngineEvent]) {
        for event in events {
            match event {
                EngineEvent::BreakStarted(info) => match info.kind {
                    BreakKind::Short => self.break_windows.show_short_break(info),
                    BreakKind::Long => self.break_windows.show_long_break(info),
                },
                EngineEvent::LongBreakWarningStarted(countdown) => {
                    self.break_windows.show_warning(*countdown);
                }
                EngineEvent::BreakEnded(_, _) => {
                    self.break_windows.close_all();
                }
                EngineEvent::PhaseChanged(snapshot) => {
                    if matches!(
                        snapshot.phase,
                        EnginePhase::Working | EnginePhase::Paused | EnginePhase::Away
                    ) {
                        self.break_windows.close_all();
                    }
                }
                EngineEvent::AbsenceCredited(_) => {}
            }
        }
    }

    /// Synchronizes snapshot and settings to the break windows and settings surfaces.
    pub fn sync_state(&mut self, snapshot: &EngineSnapshot, settings: &OcySettings) {
        self.break_windows.sync_state(snapshot, settings);
        self.settings_window.sync_settings(settings);
    }

    /// Updates pre-warming, theme checks, and active countdown displays on each engine tick.
    pub fn on_tick(&mut self, snapshot: &EngineSnapshot) {
        self.break_windows.on_tick(snapshot);
        if matches!(
            snapshot.phase,
            EnginePhase::ShortBreak | EnginePhase::LongBreak | EnginePhase::LongBreakWarning
        ) {
            self.break_windows.update_countdown(snapshot);
        }
    }

    /// Opens or focuses the Settings window.
    pub fn show_settings(&mut self) {
        self.settings_window.show();
    }

    /// Closes all break and warning windows/overlays.
    pub fn close_all(&mut self) {
        self.break_windows.close_all();
    }

    /// Returns a shared reference to the inner [`BreakWindows`].
    #[must_use]
    pub const fn break_windows(&self) -> &W {
        &self.break_windows
    }

    /// Returns a mutable reference to the inner [`BreakWindows`].
    pub const fn break_windows_mut(&mut self) -> &mut W {
        &mut self.break_windows
    }

    /// Returns a shared reference to the inner [`SettingsWindow`].
    #[must_use]
    pub const fn settings_window(&self) -> &S {
        &self.settings_window
    }
}
