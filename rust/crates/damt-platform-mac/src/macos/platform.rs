//! The composed macOS platform service bundle (mirrors `LinuxPlatform` / `WinPlatform`).

use damt_platform::{
    ActiveScreenProvider, AppThemeProvider, AutostartService, FocusStateProvider, ForegroundProbe,
    IdleProvider, MotionPreference, Notifier, PlatformServices, TaskbarThemeProvider, Unsupported,
};

use super::{
    MacActiveScreen, MacAppTheme, MacAutostart, MacFocus, MacForeground, MacIdle, MacMotion,
    MacSessionEvents, MacTaskbarTheme,
};

/// All macOS platform services, composed once at startup (mirrors `LinuxPlatform`).
///
/// The session event source is not part of the [`PlatformServices`] trait; get it with
/// [`MacPlatform::session`] before handing the bundle to the host.
#[derive(Debug)]
pub struct MacPlatform {
    idle: MacIdle,
    focus: MacFocus,
    foreground: MacForeground,
    autostart: MacAutostart,
    session: MacSessionEvents,
    active_screen: MacActiveScreen,
    motion: MacMotion,
    taskbar_theme: MacTaskbarTheme,
    app_theme: MacAppTheme,
}

impl Default for MacPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl MacPlatform {
    /// Creates the macOS platform service bundle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            idle: MacIdle,
            focus: MacFocus,
            foreground: MacForeground,
            autostart: MacAutostart,
            session: MacSessionEvents::new(),
            active_screen: MacActiveScreen::new(),
            motion: MacMotion::new(),
            taskbar_theme: MacTaskbarTheme::new(),
            app_theme: MacAppTheme::new(),
        }
    }

    /// The session event source (lock/unlock/sleep/wake), for `start()` wiring.
    pub fn session(&mut self) -> &mut MacSessionEvents {
        &mut self.session
    }
}

impl PlatformServices for MacPlatform {
    fn idle(&self) -> &dyn IdleProvider {
        &self.idle
    }

    fn focus(&self) -> &dyn FocusStateProvider {
        &self.focus
    }

    fn foreground(&self) -> &dyn ForegroundProbe {
        &self.foreground
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
        // OS notifications are M2 work (UNUserNotificationCenter); until then the
        // in-app cards and tray carry the status.
        &Unsupported
    }
}
