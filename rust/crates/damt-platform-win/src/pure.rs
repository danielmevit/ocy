//! Pure ports of the C# platform helpers: no Windows calls, so they are unit-tested on every target.

use std::time::Duration;

use damt_platform::{FocusState, ScreenRect, SessionEvent};
use thiserror::Error;

/// Wrap-safe idle-time math (C# `IdleMath`).
#[derive(Debug, Default, Clone, Copy)]
pub struct IdleMath;

impl IdleMath {
    /// Milliseconds between `now_ticks` and `last_input_ticks`, wrapping like the 32-bit tick counter.
    pub fn compute(now_ticks: u32, last_input_ticks: u32) -> Duration {
        Duration::from_millis(u64::from(now_ticks.wrapping_sub(last_input_ticks)))
    }
}

/// Mapping for the Windows "Animation effects" preference (C# `MotionMath`).
#[derive(Debug, Default, Clone, Copy)]
pub struct MotionMath;

impl MotionMath {
    /// False when the call failed (unknown means animations allowed), true only when the value is 0
    /// ("Animation effects" off).
    pub fn reduced(call_succeeded: bool, client_area_animation: i32) -> bool {
        call_succeeded && client_area_animation == 0
    }
}

/// A registry value as far as the taskbar helper cares: a DWORD or anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryValue {
    /// A `REG_DWORD` value.
    Dword(u32),
    /// Any other registry type (string, QWORD, binary, ...).
    Other,
}

/// Mapping for the Windows taskbar theme registry value (C# `TaskbarThemeMath`).
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskbarThemeMath;

impl TaskbarThemeMath {
    /// Maps the `SystemUsesLightTheme` DWORD: 1 = light, 0 = dark. Missing, any other value or any
    /// non-DWORD type means unknown.
    pub fn from_registry_value(value: Option<RegistryValue>) -> Option<bool> {
        match value {
            Some(RegistryValue::Dword(1)) => Some(true),
            Some(RegistryValue::Dword(0)) => Some(false),
            _ => None,
        }
    }
}

/// Maps `SHQueryUserNotificationState` results to [`FocusState`] (C# `FocusStateMapper`).
#[derive(Debug, Default, Clone, Copy)]
pub struct FocusStateMapper;

impl FocusStateMapper {
    /// `hresult` is the call's HRESULT; negative means the query failed.
    pub fn map(hresult: i32, quns: i32) -> FocusState {
        if hresult < 0 {
            return FocusState::Unknown;
        }

        match quns {
            1 => FocusState::NotPresent,
            2 | 3 | 7 => FocusState::FullscreenApp,
            4 => FocusState::Presentation,
            5 => FocusState::Normal,
            6 => FocusState::QuietTime,
            _ => FocusState::Unknown,
        }
    }
}

/// Maps Windows session and power notifications to [`SessionEvent`] (C# `SessionEventMapper`).
#[derive(Debug, Default, Clone, Copy)]
pub struct SessionEventMapper;

impl SessionEventMapper {
    /// `WM_WTSSESSION_CHANGE` reason: 7 = lock, 8 = unlock, 6 = logoff.
    pub fn from_session_switch(reason: i32) -> Option<SessionEvent> {
        match reason {
            7 => Some(SessionEvent::Locked),
            8 => Some(SessionEvent::Unlocked),
            6 => Some(SessionEvent::LoggingOff),
            _ => None,
        }
    }

    /// Power mode: 1 = resume, 3 = suspend.
    pub fn from_power_mode(mode: i32) -> Option<SessionEvent> {
        match mode {
            1 => Some(SessionEvent::Resumed),
            3 => Some(SessionEvent::Suspending),
            _ => None,
        }
    }
}

/// Converts Win32 `RECT` bounds to [`ScreenRect`] (C# `RectMath`).
#[derive(Debug, Default, Clone, Copy)]
pub struct RectMath;

impl RectMath {
    /// Left/top/right/bottom to `x`/`y`/`width`/`height`.
    pub fn to_screen_rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            x: left,
            y: top,
            width: right.wrapping_sub(left),
            height: bottom.wrapping_sub(top),
        }
    }
}

/// Why an autostart command could not be built.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum AutostartCommandError {
    /// The executable path was empty or only whitespace.
    #[error("the path must not be empty or whitespace")]
    EmptyPath,
    /// The executable path contained a quote character.
    #[error("the path must not contain quote characters")]
    ContainsQuote,
}

/// Builds the command line stored in the Windows Run key (C# `AutostartCommand`).
#[derive(Debug, Default, Clone, Copy)]
pub struct AutostartCommand;

impl AutostartCommand {
    /// Quotes the executable path and appends ` --autostart`.
    pub fn build(exe_path: &str) -> Result<String, AutostartCommandError> {
        if exe_path.trim().is_empty() {
            return Err(AutostartCommandError::EmptyPath);
        }

        if exe_path.contains('"') {
            return Err(AutostartCommandError::ContainsQuote);
        }

        Ok(format!("\"{exe_path}\" --autostart"))
    }
}

/// Extended-window-style bit math (C# `ExStyle`).
#[derive(Debug, Default, Clone, Copy)]
pub struct ExStyle;

impl ExStyle {
    const WS_EX_TRANSPARENT: i32 = 0x20;
    const WS_EX_TOOLWINDOW: i32 = 0x80;
    const WS_EX_LAYERED: i32 = 0x0008_0000;
    const WS_EX_NOACTIVATE: i32 = 0x0800_0000;

    /// Adds the click-through, tool-window, layered and no-activate bits to `current`.
    pub fn click_through(current: i32) -> i32 {
        current
            | Self::WS_EX_TRANSPARENT
            | Self::WS_EX_TOOLWINDOW
            | Self::WS_EX_LAYERED
            | Self::WS_EX_NOACTIVATE
    }
}
