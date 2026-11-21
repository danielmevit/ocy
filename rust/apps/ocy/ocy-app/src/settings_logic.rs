//! Pure settings mapping, localization, debounce/autostart state machine, and lifecycle logic.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use damt_app::ErrorLog;
use damt_platform::{
    ActiveScreenProvider, AppThemeProvider, AutostartService, FocusStateProvider, IdleProvider,
    MotionPreference, PlatformError, PlatformServices, TaskbarThemeProvider,
};
use ocy_core::settings::OcySettings;

use crate::break_logic::app_theme_is_dark;
use crate::coordinator::SettingsWindow;
use crate::strings::Strings;

/// Debounce delay before live settings changes are applied and persisted (matches C# `ApplyDelay`).
pub const SETTINGS_APPLY_DELAY: Duration = Duration::from_millis(400);

/// Returns the Ocy package version stripped of any `+build` metadata (mirrors C# `GetAppVersion`).
#[must_use]
pub fn app_version() -> &'static str {
    let raw = env!("CARGO_PKG_VERSION");
    raw.split('+').next().unwrap_or(raw)
}

/// All UI-backed field values of the Settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub strict_mode: bool,
    pub inactivity_tracking: bool,
    pub away_threshold: i32,
    pub pause_fullscreen: bool,
    pub window_nearby: bool,
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
            strict_mode: settings.strict_mode,
            inactivity_tracking: settings.inactivity_tracking,
            away_threshold: settings.away_threshold_seconds,
            pause_fullscreen: settings.pause_during_fullscreen,
            window_nearby: settings.has_window_nearby,
        }
    }

    /// Clones `current` and overwrites only the UI-backed fields, preserving non-UI fields
    /// (`schema_version`, `sounds`, `status_notifications`, `language`) unchanged.
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
        updated.inactivity_tracking = self.inactivity_tracking;
        updated.away_threshold_seconds = self.away_threshold;
        updated.pause_during_fullscreen = self.pause_fullscreen;
        updated.has_window_nearby = self.window_nearby;

        if autostart_supported {
            updated.start_with_system = self.start_with_system;
        }
        updated
    }
}

/// Every localized text shown in `ocy_ui::SettingsWindow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsTexts {
    pub window_title: String,
    pub general_title: String,
    pub short_breaks_title: String,
    pub long_breaks_title: String,
    pub behaviour_title: String,
    pub about_title: String,
    pub start_with_system_label: String,
    pub start_with_system_help: String,
    pub short_enabled_label: String,
    pub short_interval_label: String,
    pub short_interval_help: String,
    pub short_duration_label: String,
    pub short_duration_help: String,
    pub short_fullscreen_label: String,
    pub short_fullscreen_help: String,
    pub try_short_label: String,
    pub long_enabled_label: String,
    pub long_interval_label: String,
    pub long_interval_help: String,
    pub long_duration_label: String,
    pub long_duration_help: String,
    pub long_warning_label: String,
    pub long_warning_help: String,
    pub long_postpone_label: String,
    pub long_postpone_help: String,
    pub long_max_postpones_label: String,
    pub long_max_postpones_help: String,
    pub try_long_label: String,
    pub strict_label: String,
    pub strict_help: String,
    pub inactivity_label: String,
    pub inactivity_help: String,
    pub away_label: String,
    pub away_help: String,
    pub pause_fullscreen_label: String,
    pub pause_fullscreen_help: String,
    pub window_nearby_label: String,
    pub window_nearby_help: String,
    pub unit_minutes: String,
    pub unit_seconds: String,
    pub about_name_version: String,
    pub about_tagline: String,
}

impl SettingsTexts {
    /// Resolves all Settings window strings from `strings` using the exact `en.json` keys C# uses.
    #[must_use]
    pub fn from_strings(strings: &Strings, version: &str) -> Self {
        let app_name = strings.get("app.name");
        Self {
            window_title: strings.format("settings.windowTitle", &[("app", &app_name)]),
            general_title: strings.get("settings.section.general").to_owned(),
            short_breaks_title: strings.get("settings.section.shortBreaks").to_owned(),
            long_breaks_title: strings.get("settings.section.longBreaks").to_owned(),
            behaviour_title: strings.get("settings.section.behaviour").to_owned(),
            about_title: strings.get("settings.section.about").to_owned(),
            start_with_system_label: strings.get("settings.startWithSystem.label").to_owned(),
            start_with_system_help: strings.get("settings.startWithSystem.help").to_owned(),
            short_enabled_label: strings.get("settings.shortBreaks.enabled.label").to_owned(),
            short_interval_label: strings
                .get("settings.shortBreaks.intervalMinutes.label")
                .to_owned(),
            short_interval_help: strings
                .get("settings.shortBreaks.intervalMinutes.help")
                .to_owned(),
            short_duration_label: strings
                .get("settings.shortBreaks.durationSeconds.label")
                .to_owned(),
            short_duration_help: strings
                .get("settings.shortBreaks.durationSeconds.help")
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
            long_interval_help: strings
                .get("settings.longBreaks.intervalMinutes.help")
                .to_owned(),
            long_duration_label: strings
                .get("settings.longBreaks.durationMinutes.label")
                .to_owned(),
            long_duration_help: strings
                .get("settings.longBreaks.durationMinutes.help")
                .to_owned(),
            long_warning_label: strings
                .get("settings.longBreaks.warningSeconds.label")
                .to_owned(),
            long_warning_help: strings
                .get("settings.longBreaks.warningSeconds.help")
                .to_owned(),
            long_postpone_label: strings
                .get("settings.longBreaks.postponeMinutes.label")
                .to_owned(),
            long_postpone_help: strings
                .get("settings.longBreaks.postponeMinutes.help")
                .to_owned(),
            long_max_postpones_label: strings
                .get("settings.longBreaks.maxPostpones.label")
                .to_owned(),
            long_max_postpones_help: strings
                .get("settings.longBreaks.maxPostpones.help")
                .to_owned(),
            try_long_label: strings.get("settings.tryLongBreak").to_owned(),
            strict_label: strings.get("settings.strictMode.label").to_owned(),
            strict_help: strings.get("settings.strictMode.help").to_owned(),
            inactivity_label: strings.get("settings.inactivityTracking.label").to_owned(),
            inactivity_help: strings.get("settings.inactivityTracking.help").to_owned(),
            away_label: strings
                .get("settings.awayThresholdSeconds.label")
                .to_owned(),
            away_help: strings.get("settings.awayThresholdSeconds.help").to_owned(),
            pause_fullscreen_label: strings
                .get("settings.pauseDuringFullscreen.label")
                .to_owned(),
            pause_fullscreen_help: strings
                .get("settings.pauseDuringFullscreen.help")
                .to_owned(),
            window_nearby_label: strings.get("settings.hasWindowNearby.label").to_owned(),
            window_nearby_help: strings.get("settings.hasWindowNearby.help").to_owned(),
            unit_minutes: strings.get("unit.minutes").to_owned(),
            unit_seconds: strings.get("unit.seconds").to_owned(),
            about_name_version: strings.format(
                "about.version",
                &[("name", &app_name), ("version", &version)],
            ),
            about_tagline: strings.get("app.tagline").to_owned(),
        }
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
}

/// Pure state machine for Settings loading guards, 400 ms debouncing, and autostart error handling.
#[derive(Debug, Default)]
pub struct SettingsDebounceState {
    loading: bool,
    suppress_autostart_events: bool,
    has_pending_changes: bool,
    last_autostart_value: bool,
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
            timer_generation: 0,
        }
    }

    /// Enters load mode, suppressing change and autostart side-effects until [`Self::end_load`].
    pub fn begin_load(&mut self) {
        self.loading = true;
        self.suppress_autostart_events = true;
        self.has_pending_changes = false;
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

    /// Handles a UI `changed()` signal, applying autostart immediately if that toggle changed.
    pub fn on_changed(
        &mut self,
        autostart_supported: bool,
        current_autostart_ui: bool,
        autostart: &dyn AutostartService,
    ) -> ChangeAction {
        if self.loading || self.suppress_autostart_events {
            return ChangeAction::Ignored;
        }

        if autostart_supported && current_autostart_ui != self.last_autostart_value {
            return match autostart.set_enabled(current_autostart_ui) {
                Ok(()) => {
                    self.last_autostart_value = current_autostart_ui;
                    let generation = self.schedule_apply();
                    ChangeAction::ScheduleTimer {
                        generation,
                        clear_autostart_error: true,
                    }
                }
                Err(_) => ChangeAction::AutostartFailed {
                    revert_to: !current_autostart_ui,
                },
            };
        }

        let generation = self.schedule_apply();
        ChangeAction::ScheduleTimer {
            generation,
            clear_autostart_error: false,
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
    pub on_try_short_break: Rc<dyn Fn()>,
    pub on_try_long_break: Rc<dyn Fn()>,
    pub on_close_requested: Rc<dyn Fn()>,
}

impl fmt::Debug for SettingsUiHandlers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsUiHandlers").finish_non_exhaustive()
    }
}

/// Windowing/UI backend abstraction so Settings lifecycle, debounce, and error paths are unit-testable.
pub trait SettingsBackend {
    type Instance;

    /// Builds, populates, and shows a fresh Settings window instance.
    fn create(
        &self,
        texts: &SettingsTexts,
        values: &SettingsValues,
        show_start_with_system: bool,
        dark: bool,
        handlers: SettingsUiHandlers,
    ) -> Result<Self::Instance, String>;

    /// Un-minimizes, brings to front, and focuses an already-open instance.
    fn activate(&self, instance: &Self::Instance);

    /// Updates light/dark theme on an open instance.
    fn set_dark(&self, instance: &Self::Instance, dark: bool);

    /// Updates the taskbar-theme icon variant on an open instance.
    fn set_light_icon_set(&self, _instance: &Self::Instance, _use_light_set: bool) {}

    /// Reads the current UI control values from `instance`.
    fn read_values(&self, instance: &Self::Instance) -> SettingsValues;

    /// Updates the `start-with-system` toggle value on `instance`.
    fn set_start_with_system(&self, instance: &Self::Instance, enabled: bool);

    /// Sets or clears (`""`) the inline autostart error message on `instance`.
    fn set_start_with_system_error(&self, instance: &Self::Instance, error: &str);

    /// Sets or clears (`""`) the top-level save error message on `instance`.
    fn set_save_error(&self, instance: &Self::Instance, error: &str);

    /// Schedules a single-shot callback after `delay`.
    fn schedule_apply_timer(&self, delay: Duration, on_fire: Box<dyn FnOnce()>);

    /// Hides and schedules destruction of `instance` after the close callback unwinds.
    fn destroy_on_close(&self, instance: Self::Instance);

    /// Immediately hides and drops `instance` (during shutdown/drop).
    fn hide_and_destroy(&self, instance: Self::Instance);
}

struct ManagedSettingsInner<B: SettingsBackend> {
    backend: B,
    platform: Arc<dyn PlatformServices>,
    strings: Arc<Strings>,
    error_log: ErrorLog,
    callbacks: SettingsCallbacks,
    current_settings: RefCell<OcySettings>,
    state: RefCell<SettingsDebounceState>,
    active: RefCell<Option<B::Instance>>,
    dark_theme: Cell<bool>,
}

impl<B: SettingsBackend + 'static> ManagedSettingsInner<B> {
    fn handle_changed(self: &Rc<Self>) {
        let autostart_supported = self.platform.autostart().is_supported();
        let current_autostart_ui = {
            let active = self.active.borrow();
            let Some(inst) = active.as_ref() else {
                return;
            };
            self.backend.read_values(inst).start_with_system
        };

        let action = self.state.borrow_mut().on_changed(
            autostart_supported,
            current_autostart_ui,
            self.platform.autostart(),
        );

        match action {
            ChangeAction::Ignored => {}
            ChangeAction::AutostartFailed { revert_to } => {
                self.state.borrow_mut().set_suppress_autostart_events(true);
                if let Some(inst) = self.active.borrow().as_ref() {
                    self.backend.set_start_with_system_error(
                        inst,
                        self.strings.get("settings.autostart.failed"),
                    );
                    self.backend.set_start_with_system(inst, revert_to);
                }
                self.state.borrow_mut().set_suppress_autostart_events(false);
            }
            ChangeAction::ScheduleTimer {
                generation,
                clear_autostart_error,
            } => {
                if clear_autostart_error && let Some(inst) = self.active.borrow().as_ref() {
                    self.backend.set_start_with_system_error(inst, "");
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
                ""
            } else {
                self.strings.get("settings.saveFailed")
            };
            self.backend.set_save_error(inst, msg);
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
        let removed = self.active.borrow_mut().take();
        if let Some(inst) = removed {
            self.backend.destroy_on_close(inst);
        }
    }

    fn build_handlers(self: &Rc<Self>) -> SettingsUiHandlers {
        let weak_changed: Weak<Self> = Rc::downgrade(self);
        let weak_short: Weak<Self> = Rc::downgrade(self);
        let weak_long: Weak<Self> = Rc::downgrade(self);
        let weak_close: Weak<Self> = Rc::downgrade(self);
        SettingsUiHandlers {
            on_changed: Rc::new(move || {
                if let Some(inner) = weak_changed.upgrade() {
                    inner.handle_changed();
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
        let removed = self.inner.active.borrow_mut().take();
        if let Some(inst) = removed {
            self.inner.backend.hide_and_destroy(inst);
        }
    }
}

impl<B: SettingsBackend + 'static> ManagedSettingsWindow<B> {
    /// Creates a new [`ManagedSettingsWindow`].
    #[must_use]
    pub fn new(
        backend: B,
        platform: Arc<dyn PlatformServices>,
        strings: Arc<Strings>,
        error_log: ErrorLog,
        callbacks: SettingsCallbacks,
        initial_settings: OcySettings,
    ) -> Self {
        let dark = app_theme_is_dark(platform.app_theme().is_light_app_theme());
        Self {
            inner: Rc::new(ManagedSettingsInner {
                backend,
                platform,
                strings,
                error_log,
                callbacks,
                current_settings: RefCell::new(initial_settings),
                state: RefCell::new(SettingsDebounceState::new()),
                active: RefCell::new(None),
                dark_theme: Cell::new(dark),
            }),
        }
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
}

impl<B: SettingsBackend + 'static> SettingsWindow for ManagedSettingsWindow<B> {
    fn sync_settings(&mut self, settings: &OcySettings) {
        *self.inner.current_settings.borrow_mut() = settings.clone();
    }

    fn show(&mut self) {
        let dark = app_theme_is_dark(self.inner.platform.app_theme().is_light_app_theme());
        self.inner.dark_theme.set(dark);

        if let Some(inst) = self.inner.active.borrow().as_ref() {
            self.inner.backend.set_dark(inst, dark);
            self.inner.backend.activate(inst);
            return;
        }

        let autostart_supported = self.inner.platform.autostart().is_supported();
        let autostart_enabled = autostart_supported && self.inner.platform.autostart().is_enabled();
        let values = {
            let current = self.inner.current_settings.borrow();
            SettingsValues::from_settings(&current, autostart_supported, autostart_enabled)
        };
        let texts = SettingsTexts::from_strings(&self.inner.strings, app_version());
        let handlers = self.inner.build_handlers();

        self.inner.state.borrow_mut().begin_load();
        let created =
            self.inner
                .backend
                .create(&texts, &values, autostart_supported, dark, handlers);
        self.inner
            .state
            .borrow_mut()
            .end_load(values.start_with_system);

        match created {
            Ok(inst) => {
                *self.inner.active.borrow_mut() = Some(inst);
            }
            Err(err) => {
                self.inner
                    .error_log
                    .log(Some("settings_window_create"), &err);
            }
        }
    }
}
