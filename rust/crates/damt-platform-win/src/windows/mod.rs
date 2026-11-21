//! Win32 implementations of the frozen contracts.

mod autostart;
mod focus;
mod idle;
mod motion;
mod notifier;
mod platform;
mod registry;
mod screen;
mod session;
mod taskbar;

pub use autostart::WinAutostart;
pub use focus::WinFocus;
pub use idle::WinIdle;
pub use motion::WinMotion;
pub use notifier::WinNotifier;
pub use platform::WinPlatform;
pub use screen::WinActiveScreen;
pub use session::WinSessionEvents;
pub use taskbar::{WinAppTheme, WinTaskbarTheme};
