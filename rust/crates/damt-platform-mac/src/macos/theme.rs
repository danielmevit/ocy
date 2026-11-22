//! Theme and motion preferences via `NSUserDefaults`.

use std::fmt;

use damt_platform::{AppThemeProvider, MotionPreference, TaskbarThemeProvider};
use objc2::AllocAnyThread;
use objc2_foundation::{NSString, NSUserDefaults};

use crate::pure::{parse_apple_interface_style, parse_reduce_motion};

/// The `AppleInterfaceStyle` key in the global domain: `Dark` when the user picked the
/// dark appearance, absent (or `Light`) otherwise.
const APPLE_INTERFACE_STYLE_KEY: &str = "AppleInterfaceStyle";

/// The preference domain that holds the Accessibility "Reduce motion" setting.
const UNIVERSAL_ACCESS_DOMAIN: &str = "com.apple.universalaccess";

/// The `reduceMotion` key inside [`UNIVERSAL_ACCESS_DOMAIN`] (verify on hardware; see
/// docs/rust/MACOS.md manual checks).
const REDUCE_MOTION_KEY: &str = "reduceMotion";

/// Reads the `AppleInterfaceStyle` value from the standard (global-including) defaults.
fn read_interface_style() -> Option<String> {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(APPLE_INTERFACE_STYLE_KEY);
    defaults.stringForKey(&key).map(|value| value.to_string())
}

/// macOS [`AppThemeProvider`]: the system appearance via `AppleInterfaceStyle`.
pub struct MacAppTheme;

impl fmt::Debug for MacAppTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MacAppTheme")
    }
}

impl Default for MacAppTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl MacAppTheme {
    /// Creates the app-theme provider.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl AppThemeProvider for MacAppTheme {
    fn is_supported(&self) -> bool {
        true
    }

    fn is_light_app_theme(&self) -> Option<bool> {
        parse_apple_interface_style(read_interface_style().as_deref())
    }
}

/// macOS [`TaskbarThemeProvider`]: the menu bar follows the same system appearance as
/// app content, so this reads the same `AppleInterfaceStyle` value.
pub struct MacTaskbarTheme;

impl fmt::Debug for MacTaskbarTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MacTaskbarTheme")
    }
}

impl Default for MacTaskbarTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl MacTaskbarTheme {
    /// Creates the taskbar-theme provider.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl TaskbarThemeProvider for MacTaskbarTheme {
    fn is_supported(&self) -> bool {
        true
    }

    fn is_light_taskbar(&self) -> Option<bool> {
        parse_apple_interface_style(read_interface_style().as_deref())
    }
}

/// macOS [`MotionPreference`]: the Accessibility "Reduce motion" setting, read from the
/// `com.apple.universalaccess` defaults domain.
pub struct MacMotion;

impl fmt::Debug for MacMotion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MacMotion")
    }
}

impl Default for MacMotion {
    fn default() -> Self {
        Self::new()
    }
}

impl MacMotion {
    /// Creates the motion-preference provider.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl MotionPreference for MacMotion {
    fn is_supported(&self) -> bool {
        true
    }

    fn prefers_reduced_motion(&self) -> bool {
        let domain = NSString::from_str(UNIVERSAL_ACCESS_DOMAIN);
        let Some(defaults) =
            NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&domain))
        else {
            return false;
        };
        let key = NSString::from_str(REDUCE_MOTION_KEY);
        parse_reduce_motion(Some(defaults.boolForKey(&key)))
    }
}
