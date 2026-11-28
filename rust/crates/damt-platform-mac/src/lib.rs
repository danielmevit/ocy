//! macOS implementations of the frozen [`damt_platform`] contracts.
//!
//! The OS layer ([`macos`]) compiles only on `target_os = "macos"` and is empty elsewhere.
//! Everything in [`pure`] is plain mapping, parsing, and scale maths with no Apple
//! framework dependency, so it compiles and its unit tests run on every target
//! (including the Linux CI job and this WSL workspace).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod pure;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::{
    MacActiveScreen, MacAppTheme, MacAutostart, MacFocus, MacForeground, MacIdle, MacMonitorInfo,
    MacMotion, MacPlatform, MacSessionEvents, MacTaskbarTheme, MacTrayConfig, MacTrayHandle,
    query_mac_monitors,
};
#[cfg(target_os = "macos")]
pub use pure::{MacTrayAction, MacTrayIconState, MacTrayMenuItem, MacTrayPresentation};
