//! Application host wiring engine ticks, session events, autosave, coordinator, and tray.

use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use damt_app::ErrorLog;
use damt_platform::{Notifier, SessionEvent, Unsupported};
use ocy_core::engine::{BreakEngine, EngineEvent, EngineSnapshot};
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;

use crate::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
#[cfg(feature = "store")]
use crate::license::LicenseState;
use crate::notifications::{NotificationPresenter, StatusNotification};
use crate::tray::{PauseSource, TrayController, TraySink};

/// Autosave interval: save after this much monotonic time since the last save.
///
/// This is real time, not a tick count: the adaptive scheduler ticks about every 10 s
/// while working (5 s while away), so counting ticks would stretch the interval to
/// 10-20 minutes.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);

/// Monotonic time source for the autosave interval (a seam so tests can drive it).
pub trait MonotonicTimeSource: fmt::Debug {
    /// Returns the current monotonic instant.
    fn now(&self) -> Instant;
}

/// Production [`MonotonicTimeSource`] backed by [`Instant::now`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemMonotonicTime;

impl MonotonicTimeSource for SystemMonotonicTime {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Central host coordinating [`BreakEngine`], [`BreakUiCoordinator`], and [`TrayController`].
pub struct AppHost<W: BreakWindows, S: SettingsWindow, T: TraySink> {
    engine: BreakEngine,
    coordinator: BreakUiCoordinator<W, S>,
    tray: TrayController<T>,
    notifier: Rc<dyn Notifier>,
    notification_presenter: NotificationPresenter,
    notification_error_logged: bool,
    settings_store: Option<AppDataStore>,
    error_log: ErrorLog,
    last_language_setting: String,
    time_source: Arc<dyn MonotonicTimeSource>,
    last_save: Instant,
    shut_down: bool,
    #[cfg(feature = "store")]
    license_state: LicenseState,
}

impl<W, S, T> fmt::Debug for AppHost<W, S, T>
where
    W: BreakWindows + fmt::Debug,
    S: SettingsWindow + fmt::Debug,
    T: TraySink + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppHost")
            .field("engine", &self.engine)
            .field("coordinator", &self.coordinator)
            .field("tray", &self.tray)
            .field("last_save", &self.last_save)
            .field("shut_down", &self.shut_down)
            .finish_non_exhaustive()
    }
}

impl<W: BreakWindows, S: SettingsWindow, T: TraySink> AppHost<W, S, T> {
    /// Creates a new `AppHost` from already-initialized components (without a settings store).
    #[must_use]
    pub fn new(
        engine: BreakEngine,
        coordinator: BreakUiCoordinator<W, S>,
        tray: TrayController<T>,
        error_log: ErrorLog,
    ) -> Self {
        Self::with_store(engine, coordinator, tray, error_log, None)
    }

    /// Creates a new `AppHost` with an optional [`AppDataStore`] used by [`AppHost::save_settings`].
    #[must_use]
    pub fn with_store(
        engine: BreakEngine,
        mut coordinator: BreakUiCoordinator<W, S>,
        tray: TrayController<T>,
        error_log: ErrorLog,
        settings_store: Option<AppDataStore>,
    ) -> Self {
        let last_language_setting = engine.settings().language.clone();
        let notification_presenter =
            NotificationPresenter::with_ship_gate(Arc::clone(tray.presenter().strings()));
        coordinator.sync_state_with_stats(
            &engine.snapshot(),
            engine.settings(),
            engine.statistics(),
        );
        let time_source: Arc<dyn MonotonicTimeSource> = Arc::new(SystemMonotonicTime);
        let last_save = time_source.now();
        Self {
            engine,
            coordinator,
            tray,
            notifier: Rc::new(Unsupported),
            notification_presenter,
            notification_error_logged: false,
            settings_store,
            error_log,
            last_language_setting,
            time_source,
            last_save,
            shut_down: false,
            #[cfg(feature = "store")]
            license_state: LicenseState::Unknown,
        }
    }

    /// Replaces the default monotonic time source (tests drive the autosave interval).
    #[must_use]
    pub fn with_time_source(mut self, time_source: Arc<dyn MonotonicTimeSource>) -> Self {
        self.last_save = time_source.now();
        self.time_source = time_source;
        self
    }

    /// Installs the [`Notifier`] backend used for status notifications.
    #[must_use]
    pub fn with_notifier(mut self, notifier: Rc<dyn Notifier>) -> Self {
        self.notifier = notifier;
        self
    }

    /// Overrides the notification ship gate (for tests; production keeps [`crate::STATUS_NOTIFICATIONS_SHIPPED`]).
    #[must_use]
    pub fn with_notification_ship_gate(mut self, shipped: bool) -> Self {
        self.notification_presenter.set_shipped(shipped);
        self
    }

    /// Fires the one-shot startup status notification if eligible.
    pub fn on_startup_notification_timer(&mut self) {
        if self.shut_down {
            return;
        }

        let snapshot = self.engine.snapshot();
        if let Some(notification) = self
            .notification_presenter
            .on_startup_ready(self.engine.settings(), &snapshot)
        {
            self.dispatch_notification(&notification);
        }
    }

    fn handle_notification_events(&mut self, events: &[EngineEvent], snapshot: &EngineSnapshot) {
        if let Some(notification) =
            self.notification_presenter
                .on_engine_events(self.engine.settings(), snapshot, events)
        {
            self.dispatch_notification(&notification);
        }
    }

    fn dispatch_notification(&mut self, notification: &StatusNotification) {
        if !self.notifier.is_supported() {
            return;
        }
        if let Err(err) = self
            .notifier
            .notify(&notification.title, &notification.body)
            && !self.notification_error_logged
        {
            self.notification_error_logged = true;
            self.error_log.log_error("status_notification", &err);
        }
    }

    fn sync_coordinator_for_snapshot(&mut self, snapshot: &EngineSnapshot) {
        self.coordinator.sync_state_with_stats(
            snapshot,
            self.engine.settings(),
            self.engine.statistics(),
        );
    }

    /// Runs an engine tick (at the adaptive scheduler's cadence), updates break UI and
    /// tray, and autosaves periodically.
    pub fn on_tick(&mut self) {
        if self.shut_down {
            return;
        }

        let events = self.engine.tick();
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);

        let snapshot = self.engine.snapshot();
        self.coordinator.on_tick(&snapshot);
        self.tray.update(&snapshot);
        self.handle_notification_events(&events, &snapshot);

        // Autosave on elapsed monotonic time (not tick count): the adaptive scheduler's
        // tick cadence varies (about 10 s working, 5 s away), so ticks are not seconds.
        let now = self.time_source.now();
        if now.duration_since(self.last_save) >= AUTOSAVE_INTERVAL {
            self.last_save = now;
            if let Err(err) = self.engine.save() {
                self.error_log
                    .log_error("AppHost: periodic save failed", &err);
            }
        }
    }

    /// Forwards an OS session event onto the engine and updates coordinator and tray.
    pub fn on_session_event(&mut self, event: SessionEvent) {
        if self.shut_down {
            return;
        }

        let events = self.engine.on_session_event(event);
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);

        let snapshot = self.engine.snapshot();
        self.tray.update(&snapshot);
        self.handle_notification_events(&events, &snapshot);
    }

    /// Triggers an immediate long break from the tray menu or warning card.
    pub fn start_long_break_now(&mut self) {
        if self.shut_down {
            return;
        }

        let (_started, events) = self.engine.start_long_break_now();
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
    }

    /// Triggers an immediate short break ("try short break" / `--demo-break short`).
    pub fn start_short_break_now(&mut self) -> bool {
        if self.shut_down {
            return false;
        }

        let (started, events) = self.engine.start_short_break_now();
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
        started
    }

    /// Postpones the upcoming long break while in `LongBreakWarning`.
    pub fn postpone_long_break(&mut self) -> bool {
        if self.shut_down {
            return false;
        }

        let (postponed, events) = self.engine.postpone_long_break();
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
        postponed
    }

    /// Skips the current break or warning (refused for long breaks/warnings in strict mode).
    pub fn skip_current_break(&mut self) -> bool {
        if self.shut_down {
            return false;
        }

        let (skipped, events) = self.engine.skip_current_break();
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
        skipped
    }

    /// Pauses reminders from one of the tray pause options, remembering which option started it.
    pub fn pause_from_tray(&mut self, source: PauseSource) {
        if self.shut_down {
            return;
        }

        self.tray.record_pause(source);
        self.apply_pause(source.duration());
    }

    /// Pauses reminders for `duration`, or indefinitely when `None`.
    pub fn pause_for(&mut self, duration: Option<Duration>) {
        if self.shut_down {
            return;
        }

        match duration {
            None => self.tray.record_pause(PauseSource::UntilResumed),
            Some(_) => self.tray.clear_pause_source(),
        }
        self.apply_pause(duration);
    }

    fn apply_pause(&mut self, duration: Option<Duration>) {
        let events = self.engine.pause_for(duration);
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
    }

    /// Adds the foreground game to the silent list and saves settings.
    ///
    /// Returns false when the foreground app is not a game or its exe name could not be read.
    /// The card and the tray call this; it does not draw anything.
    pub fn hold_current_game(&mut self) -> bool {
        if self.shut_down {
            return false;
        }
        if !self.engine.silence_foreground_game() {
            return false;
        }
        self.save_settings(self.engine.settings().clone())
    }

    /// Holds breaks until the foreground game process exits.
    ///
    /// Returns false when the foreground app is not a game or its process id is unknown.
    /// The hold lives in the engine until that process exits. The card and the tray call this.
    pub fn hold_until_game_closes(&mut self) -> bool {
        if self.shut_down {
            return false;
        }
        let held = self.engine.hold_until_foreground_game_closes();
        self.tray.update(&self.engine.snapshot());
        held
    }

    /// "Not during this game" on a gentle card: silences the foreground game, then closes the
    /// card as an interrupted break (not a skip, not a postpone, no statistics).
    ///
    /// Returns false when the foreground app is not a game. The card stays open then.
    pub fn hold_current_game_and_close_card(&mut self) -> bool {
        if !self.hold_current_game() {
            return false;
        }
        let (_closed, events) = self.engine.dismiss_gentle_card();
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
        true
    }

    /// Resumes reminders from an active pause.
    pub fn resume(&mut self) {
        if self.shut_down {
            return;
        }

        self.tray.clear_pause_source();
        let events = self.engine.resume();
        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
    }

    /// Handles a left-click on the tray icon by opening or focusing Settings.
    pub fn on_tray_left_click(&mut self) {
        self.show_settings();
    }

    /// Opens or focuses the Settings window.
    pub fn show_settings(&mut self) {
        if self.shut_down {
            return;
        }

        let snapshot = self.engine.snapshot();
        self.sync_coordinator_for_snapshot(&snapshot);
        // On macOS the Settings window activates the app and makes itself key when shown
        // (`slint_settings` calls `macos_window::activate_settings_window`).
        self.coordinator.show_settings();
    }

    /// Applies updated settings to the engine and persists them via [`AppDataStore::save_settings`].
    /// Returns `false` (and logs the error) if persistence fails.
    pub fn save_settings(&mut self, updated: OcySettings) -> bool {
        if self.shut_down {
            return false;
        }

        let events = self.engine.update_settings(updated);
        let snapshot = self.engine.snapshot();
        let live_settings = self.engine.settings().clone();
        // The in-memory settings are the single source of truth for usage-stats consent:
        // keep the analytics record and the sender's consent mirror in step after every
        // save. `apply_consent_from_settings` is a no-op without an installed manager.
        crate::usage_stats::apply_consent_from_settings(&live_settings);
        if live_settings.language != self.last_language_setting {
            self.last_language_setting = live_settings.language.clone();
            self.tray.set_language(&live_settings.language);
            self.notification_presenter
                .set_strings(Arc::clone(self.tray.presenter().strings()));
        }
        self.coordinator
            .sync_state_with_stats(&snapshot, &live_settings, self.engine.statistics());
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());

        if let Some(store) = &self.settings_store
            && let Err(err) = store.save_settings(&live_settings)
        {
            self.error_log
                .log_error("AppHost: saving settings failed", &err);
            return false;
        }
        if let Err(err) = self.engine.save() {
            self.error_log
                .log_error("AppHost: saving settings failed", &err);
            return false;
        }
        true
    }

    /// Closes active overlays/windows and saves engine state and statistics. Idempotent.
    pub fn shutdown(&mut self) {
        if self.shut_down {
            return;
        }

        self.shut_down = true;
        self.coordinator.close_all();
        if let Err(err) = self.engine.save() {
            self.error_log.log_error("AppHost: quit save failed", &err);
        }
    }

    /// Shared reference to the underlying [`BreakEngine`].
    #[must_use]
    pub const fn engine(&self) -> &BreakEngine {
        &self.engine
    }

    /// Shared reference to the underlying [`TrayController`].
    #[must_use]
    pub const fn tray(&self) -> &TrayController<T> {
        &self.tray
    }

    /// Shared reference to the underlying [`BreakUiCoordinator`].
    #[must_use]
    pub const fn coordinator(&self) -> &BreakUiCoordinator<W, S> {
        &self.coordinator
    }

    /// Mutable reference to the underlying [`BreakUiCoordinator`].
    pub const fn coordinator_mut(&mut self) -> &mut BreakUiCoordinator<W, S> {
        &mut self.coordinator
    }

    /// Current Microsoft Store license state (only present in `--features store` builds).
    #[cfg(feature = "store")]
    #[must_use]
    pub const fn license_state(&self) -> LicenseState {
        self.license_state
    }

    /// Remaining trial days (`-1` when not in a trial with a known end date, `0` when expired).
    #[cfg(feature = "store")]
    #[must_use]
    pub fn trial_days_left(&self, now: jiff::Timestamp) -> i32 {
        self.license_state.trial_days_left_i32(now)
    }

    /// Updates the Store license state, suspending or resuming engine reminders and updating
    /// the tray and Settings UI immediately.
    #[cfg(feature = "store")]
    pub fn set_license_state(&mut self, state: LicenseState, now: jiff::Timestamp) {
        if self.shut_down {
            return;
        }
        self.license_state = state;
        let events = self
            .engine
            .set_reminders_suspended(!state.allows_reminders());
        let snapshot = self.engine.snapshot();
        self.coordinator
            .sync_license(state, state.trial_days_left_i32(now));
        self.sync_coordinator_for_snapshot(&snapshot);
        self.coordinator.handle_events(&events);
        self.tray.set_license_state(state);
        self.tray.update(&self.engine.snapshot());
    }
}

impl<W: BreakWindows, S: SettingsWindow, T: TraySink> Drop for AppHost<W, S, T> {
    fn drop(&mut self) {
        self.shutdown();
    }
}
