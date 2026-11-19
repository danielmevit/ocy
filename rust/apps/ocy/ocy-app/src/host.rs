//! Application host wiring engine ticks, session events, autosave, coordinator, and tray.

use std::fmt;
use std::time::Duration;

use damt_app::ErrorLog;
use damt_platform::SessionEvent;
use ocy_core::engine::BreakEngine;
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;

use crate::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
use crate::tray::{TrayController, TraySink};

/// Autosave interval in seconds (1 tick = 1 second).
const AUTOSAVE_INTERVAL_SECONDS: u32 = 60;

/// Central host coordinating [`BreakEngine`], [`BreakUiCoordinator`], and [`TrayController`].
pub struct AppHost<W: BreakWindows, S: SettingsWindow, T: TraySink> {
    engine: BreakEngine,
    coordinator: BreakUiCoordinator<W, S>,
    tray: TrayController<T>,
    settings_store: Option<AppDataStore>,
    error_log: ErrorLog,
    seconds_since_save: u32,
    shut_down: bool,
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
            .field("seconds_since_save", &self.seconds_since_save)
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
        coordinator.sync_state(&engine.snapshot(), engine.settings());
        Self {
            engine,
            coordinator,
            tray,
            settings_store,
            error_log,
            seconds_since_save: 0,
            shut_down: false,
        }
    }

    /// Runs the 1 s engine tick, updates break UI and tray, and autosaves every 60 s.
    pub fn on_tick(&mut self) {
        if self.shut_down {
            return;
        }

        let events = self.engine.tick();
        let snapshot = self.engine.snapshot();
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
        self.coordinator.handle_events(&events);

        let snapshot = self.engine.snapshot();
        self.coordinator.on_tick(&snapshot);
        self.tray.update(&snapshot);

        self.seconds_since_save = self.seconds_since_save.saturating_add(1);
        if self.seconds_since_save >= AUTOSAVE_INTERVAL_SECONDS {
            self.seconds_since_save = 0;
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
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
        self.coordinator.handle_events(&events);

        let snapshot = self.engine.snapshot();
        self.tray.update(&snapshot);
    }

    /// Triggers an immediate long break from the tray menu or warning card.
    pub fn start_long_break_now(&mut self) {
        if self.shut_down {
            return;
        }

        let (_started, events) = self.engine.start_long_break_now();
        let snapshot = self.engine.snapshot();
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
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
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
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
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
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
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
        skipped
    }

    /// Pauses reminders for `duration`, or indefinitely when `None`.
    pub fn pause_for(&mut self, duration: Option<Duration>) {
        if self.shut_down {
            return;
        }

        let events = self.engine.pause_for(duration);
        let snapshot = self.engine.snapshot();
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
    }

    /// Resumes reminders from an active pause.
    pub fn resume(&mut self) {
        if self.shut_down {
            return;
        }

        let events = self.engine.resume();
        let snapshot = self.engine.snapshot();
        self.coordinator
            .sync_state(&snapshot, self.engine.settings());
        self.coordinator.handle_events(&events);
        self.tray.update(&self.engine.snapshot());
    }

    /// Opens or focuses the Settings window.
    pub fn show_settings(&mut self) {
        if self.shut_down {
            return;
        }

        self.coordinator
            .sync_state(&self.engine.snapshot(), self.engine.settings());
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
        self.coordinator.sync_state(&snapshot, &live_settings);
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
}

impl<W: BreakWindows, S: SettingsWindow, T: TraySink> Drop for AppHost<W, S, T> {
    fn drop(&mut self) {
        self.shutdown();
    }
}
