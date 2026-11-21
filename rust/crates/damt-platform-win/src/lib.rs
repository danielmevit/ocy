//! Windows implementations of the frozen [`damt_platform`] contracts.
//!
//! Everything in [`pure`] is plain math and mapping, compiled and tested on every target. The Win32
//! implementations live behind `#[cfg(windows)]`; each one never panics, returning the contract's documented
//! "unknown" value on failure.

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod pure;

pub use pure::{
    AutostartCommand, AutostartCommandError, ExStyle, FocusStateMapper, IdleMath, MotionMath,
    RectMath, RegistryValue, SessionEventMapper, TaskbarThemeMath,
};

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    WinActiveScreen, WinAutostart, WinFocus, WinIdle, WinMotion, WinPlatform, WinSessionEvents,
    WinTaskbarTheme,
};
