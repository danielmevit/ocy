use damt_platform::{
    ActiveScreenProvider, AppThemeProvider, AutostartService, FocusStateProvider, IdleProvider,
    MotionPreference, Notifier, PlatformServices, TaskbarThemeProvider, Unsupported,
};

use super::{
    LinuxActiveScreen, LinuxAppTheme, LinuxAutostart, LinuxFocus, LinuxIdle, LinuxMotion,
    LinuxNotifier, LinuxSessionEvents, LinuxTaskbarTheme,
};

/// All Linux platform services, composed once at startup (mirrors `WinPlatform`).
#[derive(Debug)]
pub struct LinuxPlatform {
    idle: LinuxIdle,
    focus: LinuxFocus,
    autostart: LinuxAutostart,
    session: LinuxSessionEvents,
    active_screen: LinuxActiveScreen,
    motion: LinuxMotion,
    taskbar_theme: LinuxTaskbarTheme,
    app_theme: LinuxAppTheme,
    notifier: Option<LinuxNotifier>,
}

impl LinuxPlatform {
    /// Creates a `LinuxPlatform` using `app_name` (e.g. `"Ocy"`) for the autostart `.desktop`.
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            idle: LinuxIdle::new(),
            focus: LinuxFocus::new(),
            autostart: LinuxAutostart::new(app_name),
            session: LinuxSessionEvents::new(),
            active_screen: LinuxActiveScreen::new(),
            motion: LinuxMotion::new(),
            taskbar_theme: LinuxTaskbarTheme::new(),
            app_theme: LinuxAppTheme::new(),
            notifier: None,
        }
    }

    /// Configures the XDG autostart `.desktop` file stem (e.g. `"xyz.damt.ocy"`).
    #[must_use]
    pub fn with_desktop_id(mut self, desktop_id: impl Into<String>) -> Self {
        self.autostart = self.autostart.with_desktop_id(desktop_id);
        self
    }

    /// Attaches a [`LinuxNotifier`] to this platform service bundle.
    #[must_use]
    pub fn with_notifier(mut self, notifier: LinuxNotifier) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// The session event source, which is not part of the [`PlatformServices`] trait.
    pub fn session(&mut self) -> &mut LinuxSessionEvents {
        &mut self.session
    }
}

impl PlatformServices for LinuxPlatform {
    fn idle(&self) -> &dyn IdleProvider {
        &self.idle
    }

    fn focus(&self) -> &dyn FocusStateProvider {
        &self.focus
    }

    fn autostart(&self) -> &dyn AutostartService {
        &self.autostart
    }

    fn active_screen(&self) -> &dyn ActiveScreenProvider {
        &self.active_screen
    }

    fn motion(&self) -> &dyn MotionPreference {
        &self.motion
    }

    fn taskbar_theme(&self) -> &dyn TaskbarThemeProvider {
        &self.taskbar_theme
    }

    fn app_theme(&self) -> &dyn AppThemeProvider {
        &self.app_theme
    }

    fn notifier(&self) -> &dyn Notifier {
        match &self.notifier {
            Some(notifier) => notifier,
            None => &Unsupported,
        }
    }
}
