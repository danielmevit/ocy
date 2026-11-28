//! Linux implementations of the frozen [`damt_platform`] contracts.
//!
//! Compiled only on `target_os = "linux"` (empty on other targets). Everything in [`pure`] is
//! plain mapping, parsing, and selection logic that runs without a display server or D-Bus daemon.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

#[cfg(target_os = "linux")]
pub mod pure;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::{
    LinuxActiveScreen, LinuxAppTheme, LinuxAutostart, LinuxFocus, LinuxIdle, LinuxMotion,
    LinuxNotifier, LinuxPlatform, LinuxSessionEvents, LinuxTaskbarTheme, LinuxTray,
    LinuxTrayAction, LinuxTrayConfig, LinuxTrayHandle, LinuxTrayIconState, LinuxTrayMenuItem,
    LinuxTrayMenuSlot, LinuxTrayPresentation, apply_x11_break_window_hints, query_x11_monitors,
    raise_x11_window_above,
};
#[cfg(target_os = "linux")]
pub use pure::{
    LinuxDisplayBackend, LinuxWindowPolicy, X11MonitorInfo, X11MonitorSnapshot,
    select_display_backend,
};
