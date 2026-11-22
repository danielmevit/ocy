//! macOS OS implementations of the frozen [`damt_platform`] contracts.
//!
//! Every Apple framework call lives in this module tree; the mapping/parsing logic it
//! needs is in [`crate::pure`] and unit-tested on all targets.

mod autostart;
mod focus;
mod idle;
mod platform;
mod screen;
mod session;
mod theme;
mod tray;

pub use autostart::MacAutostart;
pub use focus::{MacFocus, MacForeground};
pub use idle::MacIdle;
pub use platform::MacPlatform;
pub use screen::{MacActiveScreen, MacMonitorInfo, query_mac_monitors};
pub use session::MacSessionEvents;
pub use theme::{MacAppTheme, MacMotion, MacTaskbarTheme};
pub use tray::{MacTrayConfig, MacTrayHandle};
