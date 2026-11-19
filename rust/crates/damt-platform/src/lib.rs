//! FROZEN platform contracts, lead-owned. The Rust mirror of `src/Ocy.Abstractions/PlatformContracts.cs`.
//!
//! Workers implement these traits (Windows in R2, macOS/Linux later); they must not change them. Every method is
//! infallible unless it returns `Result`: on error, an implementation returns the documented "unknown" value.

use std::time::Duration;

/// Error from a platform call that can fail (only autostart today).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PlatformError(pub String);

/// The current time, injectable for tests (C# `TimeProvider`).
pub trait Clock: Send + Sync {
    /// Now, in the user's local time zone. `now().timestamp()` is the UTC instant.
    fn now(&self) -> jiff::ZonedDateTime;
}

/// The real clock: system time in the system time zone.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> jiff::ZonedDateTime {
        jiff::ZonedDateTime::now()
    }
}

/// Time since the user's last keyboard or mouse input.
pub trait IdleProvider {
    /// False when this platform cannot report idle time (the engine then never enters Away by idle).
    fn is_supported(&self) -> bool;
    /// Idle time; zero when unsupported or on error. Never negative.
    fn idle_time(&self) -> Duration;
}

/// What the user is doing on screen, as far as breaks are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FocusState {
    #[default]
    Unknown,
    Normal,
    FullscreenApp,
    Presentation,
    QuietTime,
    NotPresent,
}

pub trait FocusStateProvider {
    fn is_supported(&self) -> bool;
    /// Current state; `Unknown` when unsupported or on error.
    fn state(&self) -> FocusState;
}

/// Start the app when the user signs in.
pub trait AutostartService {
    fn is_supported(&self) -> bool;
    fn is_enabled(&self) -> bool;
    /// Enable or disable autostart for the current user. No-op (Ok) when unsupported.
    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError>;
}

/// OS session events. Delivered on an arbitrary thread; the host marshals them to the engine's thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionEvent {
    Locked,
    Unlocked,
    Suspending,
    Resumed,
    LoggingOff,
}

pub trait SessionEventSource: Send {
    fn is_supported(&self) -> bool;
    /// Begin delivering events to `sink`. Idempotent.
    fn start(&mut self, sink: Box<dyn Fn(SessionEvent) + Send + Sync>);
}

/// A rectangle in physical screen pixels (virtual-desktop coordinates; may be negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Which monitor the user is working on right now.
pub trait ActiveScreenProvider {
    fn is_supported(&self) -> bool;
    /// Bounds of the monitor with the foreground window, else the one under the cursor. `None` if unknown.
    fn active_screen_bounds(&self) -> Option<ScreenRect>;
}

/// The OS-level animation preference (Windows: "Animation effects").
pub trait MotionPreference {
    fn is_supported(&self) -> bool;
    /// True when the OS asks apps to minimise animation; false when allowed or unknown.
    fn prefers_reduced_motion(&self) -> bool;
}

/// The taskbar / notification-area colour theme (can differ from the app theme on Windows).
pub trait TaskbarThemeProvider {
    fn is_supported(&self) -> bool;
    /// `Some(true)` = light taskbar, `Some(false)` = dark, `None` = unknown.
    fn is_light_taskbar(&self) -> Option<bool>;
}

/// The OS light/dark preference for app content (Windows: "Choose your app mode", `AppsUseLightTheme`).
/// Can differ from the taskbar theme. Added 2026-11-19 (R3c kit gap), FROZEN.
pub trait AppThemeProvider {
    fn is_supported(&self) -> bool;
    /// `Some(true)` = light apps, `Some(false)` = dark apps, `None` = unknown.
    fn is_light_app_theme(&self) -> Option<bool>;
}

/// The set of platform services an app composes at startup. Defaults are "unsupported".
pub trait PlatformServices {
    fn idle(&self) -> &dyn IdleProvider {
        &Unsupported
    }
    fn focus(&self) -> &dyn FocusStateProvider {
        &Unsupported
    }
    fn autostart(&self) -> &dyn AutostartService {
        &Unsupported
    }
    fn active_screen(&self) -> &dyn ActiveScreenProvider {
        &Unsupported
    }
    fn motion(&self) -> &dyn MotionPreference {
        &Unsupported
    }
    fn taskbar_theme(&self) -> &dyn TaskbarThemeProvider {
        &Unsupported
    }
    fn app_theme(&self) -> &dyn AppThemeProvider {
        &Unsupported
    }
}

/// The "unsupported" fallback for every contract (C# `Unsupported*`).
#[derive(Debug, Default, Clone, Copy)]
pub struct Unsupported;

impl IdleProvider for Unsupported {
    fn is_supported(&self) -> bool {
        false
    }
    fn idle_time(&self) -> Duration {
        Duration::ZERO
    }
}
impl FocusStateProvider for Unsupported {
    fn is_supported(&self) -> bool {
        false
    }
    fn state(&self) -> FocusState {
        FocusState::Unknown
    }
}
impl AutostartService for Unsupported {
    fn is_supported(&self) -> bool {
        false
    }
    fn is_enabled(&self) -> bool {
        false
    }
    fn set_enabled(&self, _enabled: bool) -> Result<(), PlatformError> {
        Ok(())
    }
}
impl ActiveScreenProvider for Unsupported {
    fn is_supported(&self) -> bool {
        false
    }
    fn active_screen_bounds(&self) -> Option<ScreenRect> {
        None
    }
}
impl MotionPreference for Unsupported {
    fn is_supported(&self) -> bool {
        false
    }
    fn prefers_reduced_motion(&self) -> bool {
        false
    }
}
impl TaskbarThemeProvider for Unsupported {
    fn is_supported(&self) -> bool {
        false
    }
    fn is_light_taskbar(&self) -> Option<bool> {
        None
    }
}
impl AppThemeProvider for Unsupported {
    fn is_supported(&self) -> bool {
        false
    }
    fn is_light_app_theme(&self) -> Option<bool> {
        None
    }
}
impl PlatformServices for Unsupported {}
