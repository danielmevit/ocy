//! Ocy desktop application library modules.
//!
//! Pure coordination, tray presentation, localization, and layout logic live here so every unit
//! can be compiled and tested on Linux as well as Windows.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod art;
pub mod break_logic;
pub mod build_id;
pub mod coordinator;
pub mod crash_reports;
pub mod crash_sender;
pub mod host;
pub mod instance;
pub mod language_choice;
pub mod layout;
pub mod license;
#[cfg(target_os = "linux")]
pub mod linux_window;
pub mod macos_window;
pub mod mode_art;
pub mod notifications;
pub mod posthog;
pub mod presets;
pub mod scheduler;
pub mod settings_logic;
pub mod slint_breaks;
pub mod slint_settings;
pub mod slint_tray;
pub mod soak;
pub mod strings;
pub mod timeline;
pub mod today;
pub mod tokens;
pub mod tray;
pub mod usage_stats;
pub mod win32_window;

pub use license::STORE_FEATURE_ENABLED;
pub use notifications::STATUS_NOTIFICATIONS_SHIPPED;
