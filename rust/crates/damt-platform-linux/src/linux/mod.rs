//! Linux OS implementations of the frozen [`damt_platform`] contracts plus the SNI tray.

mod autostart;
mod focus;
mod idle;
mod notifier;
mod platform;
mod screen;
mod session;
mod theme;
mod tray;
mod window;

pub use autostart::LinuxAutostart;
pub use focus::LinuxFocus;
pub use idle::LinuxIdle;
pub use notifier::LinuxNotifier;
pub use platform::LinuxPlatform;
pub use screen::LinuxActiveScreen;
pub use session::LinuxSessionEvents;
pub use theme::{LinuxAppTheme, LinuxMotion, LinuxTaskbarTheme};
pub use tray::{
    LinuxTray, LinuxTrayAction, LinuxTrayConfig, LinuxTrayHandle, LinuxTrayMenuItem,
    LinuxTrayMenuSlot, LinuxTrayPresentation,
};
pub use window::{apply_x11_break_window_hints, query_x11_monitors};

pub use crate::pure::LinuxTrayIconState;
