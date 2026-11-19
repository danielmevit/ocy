//! Ocy desktop application library modules.
//!
//! Pure coordination, tray presentation, localization, and layout logic live here so every unit
//! can be compiled and tested on Linux as well as Windows.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod art;
pub mod break_logic;
pub mod coordinator;
pub mod host;
pub mod layout;
pub mod scheduler;
pub mod settings_logic;
pub mod slint_breaks;
pub mod slint_settings;
pub mod slint_tray;
pub mod strings;
pub mod tokens;
pub mod tray;
pub mod win32_window;
