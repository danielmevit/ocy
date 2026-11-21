//! Status notification presentation, ship gate, and isolated-run no-op recorder.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_platform::{Notifier, PlatformError};
use ocy_core::engine::{BreakKind, EngineEvent, EnginePhase, EngineSnapshot};
use ocy_core::settings::OcySettings;

use crate::strings::Strings;

/// Ship gate for status notifications.
///
/// Kept `false` until the Settings window exposes the `statusNotifications` toggle and the
/// owner approves shipping the feature. While `false`, the app never sends a notification
/// and never registers the AppUserModelID.
pub const STATUS_NOTIFICATIONS_SHIPPED: bool = false;

/// AppUserModelID registered under `HKCU\Software\Classes\AppUserModelId\xyz.damt.ocy` on first send.
pub const APP_USER_MODEL_ID: &str = crate::instance::APP_ID;

/// Display name registered alongside [`APP_USER_MODEL_ID`].
pub const APP_DISPLAY_NAME: &str = "Ocy";

/// Delay after the tray icon is shown before the startup status notification fires.
pub const STARTUP_NOTIFICATION_DELAY: Duration = Duration::from_secs(3);

/// Embedded 128x128 app icon PNG written once to `%LOCALAPPDATA%\Ocy\notify-icon.png` on first send.
pub const APP_NOTIFY_ICON_PNG: &[u8] =
    include_bytes!("../../../../../assets/icons/app/png/ocy-app-128.png");

/// A formatted status notification ready to hand to a [`Notifier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusNotification {
    /// Localization key (`"notify.started"` or `"notify.absenceCredited"`).
    pub key: &'static str,
    /// Localized application title (`"Ocy"`).
    pub title: String,
    /// Localized formatted notification body.
    pub body: String,
}

/// Pure presenter deciding when a status notification should be shown and with which localized text.
#[derive(Debug)]
pub struct NotificationPresenter {
    strings: Arc<Strings>,
    shipped: bool,
    startup_sent: bool,
}

impl NotificationPresenter {
    /// Creates a presenter with an explicit ship gate (`true` in unit tests exercising behaviour).
    #[must_use]
    pub const fn new(strings: Arc<Strings>, shipped: bool) -> Self {
        Self {
            strings,
            shipped,
            startup_sent: false,
        }
    }

    /// Creates a presenter governed by [`STATUS_NOTIFICATIONS_SHIPPED`].
    #[must_use]
    pub const fn with_ship_gate(strings: Arc<Strings>) -> Self {
        Self::new(strings, STATUS_NOTIFICATIONS_SHIPPED)
    }

    /// Replaces the active [`Strings`] table (used for live language switching).
    pub fn set_strings(&mut self, strings: Arc<Strings>) {
        self.strings = strings;
    }

    /// Borrows the active [`Strings`] table.
    #[must_use]
    pub const fn strings(&self) -> &Arc<Strings> {
        &self.strings
    }

    /// Returns whether the ship gate is enabled on this presenter.
    #[must_use]
    pub const fn is_shipped(&self) -> bool {
        self.shipped
    }

    /// Overrides the ship gate (primarily for host tests).
    pub const fn set_shipped(&mut self, shipped: bool) {
        self.shipped = shipped;
    }

    /// Returns `true` once the startup notification has been emitted for this launch.
    #[must_use]
    pub const fn has_sent_startup(&self) -> bool {
        self.startup_sent
    }

    /// Evaluates the startup notification (`notify.started`) when the startup timer fires.
    ///
    /// Returns `Some(...)` at most once per launch, and only when the ship gate and
    /// `settings.status_notifications` are enabled, the engine is not in a break, strict-mode
    /// warning, paused, or away, and at least one scheduled break is enabled.
    pub fn on_startup_ready(
        &mut self,
        settings: &OcySettings,
        snapshot: &EngineSnapshot,
    ) -> Option<StatusNotification> {
        if !self.is_eligible(settings, snapshot) || self.startup_sent {
            return None;
        }

        let next_break = next_break_duration(snapshot)?;
        let time = self.strings.format_time(next_break);
        let title = self.strings.get("app.name").to_owned();
        let body = self.strings.format("notify.started", &[("time", &time)]);
        self.startup_sent = true;
        Some(StatusNotification {
            key: "notify.started",
            title,
            body,
        })
    }

    /// Evaluates engine events for a long-absence credit notification (`notify.absenceCredited`).
    #[must_use]
    pub fn on_engine_events(
        &self,
        settings: &OcySettings,
        snapshot: &EngineSnapshot,
        events: &[EngineEvent],
    ) -> Option<StatusNotification> {
        if !self.is_eligible(settings, snapshot) {
            return None;
        }

        let credited_long = events
            .iter()
            .any(|event| matches!(event, EngineEvent::AbsenceCredited(BreakKind::Long)));
        if !credited_long {
            return None;
        }

        let next_long = snapshot.time_until_long_break?;
        let time = self.strings.format_time(next_long);
        let title = self.strings.get("app.name").to_owned();
        let body = self
            .strings
            .format("notify.absenceCredited", &[("time", &time)]);
        Some(StatusNotification {
            key: "notify.absenceCredited",
            title,
            body,
        })
    }

    const fn is_eligible(&self, settings: &OcySettings, snapshot: &EngineSnapshot) -> bool {
        self.shipped && settings.status_notifications && !is_suppressed(snapshot)
    }
}

const fn is_suppressed(snapshot: &EngineSnapshot) -> bool {
    matches!(
        snapshot.phase,
        EnginePhase::ShortBreak | EnginePhase::LongBreak | EnginePhase::Paused | EnginePhase::Away
    ) || (matches!(snapshot.phase, EnginePhase::LongBreakWarning) && snapshot.strict_mode)
}

fn next_break_duration(snapshot: &EngineSnapshot) -> Option<Duration> {
    match (
        snapshot.time_until_short_break,
        snapshot.time_until_long_break,
    ) {
        (Some(short), Some(long)) => Some(short.min(long)),
        (Some(short), None) => Some(short),
        (None, Some(long)) => Some(long),
        (None, None) => None,
    }
}

/// One notification captured by [`RecordingNotifier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedNotification {
    pub title: String,
    pub body: String,
}

/// No-op [`Notifier`] recorder used in isolated runs (`--demo-*`, `--report-memory`, `--soak`,
/// `--instance-scope`) and unit tests. Never touches OS toast APIs, files, or the registry.
#[derive(Debug, Default)]
pub struct RecordingNotifier {
    supported: bool,
    recorded: Mutex<Vec<RecordedNotification>>,
}

impl RecordingNotifier {
    /// Creates an isolated recorder reporting `is_supported() == true`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            supported: true,
            recorded: Mutex::new(Vec::new()),
        }
    }

    /// Creates a recorder with a configurable `is_supported` response.
    #[must_use]
    pub const fn with_supported(supported: bool) -> Self {
        Self {
            supported,
            recorded: Mutex::new(Vec::new()),
        }
    }

    /// Returns a snapshot of all recorded notifications in emission order.
    #[must_use]
    pub fn recorded(&self) -> Vec<RecordedNotification> {
        self.recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Number of recorded notifications.
    #[must_use]
    pub fn len(&self) -> usize {
        self.recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Returns `true` when no notifications have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all recorded notifications.
    pub fn clear(&self) {
        self.recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

impl Notifier for RecordingNotifier {
    fn is_supported(&self) -> bool {
        self.supported
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), PlatformError> {
        if self.supported {
            self.recorded
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(RecordedNotification {
                    title: title.to_owned(),
                    body: body.to_owned(),
                });
        }
        Ok(())
    }
}
