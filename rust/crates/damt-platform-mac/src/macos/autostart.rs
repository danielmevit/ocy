//! Autostart: a documented stub for M1.

use damt_platform::{AutostartService, PlatformError};

/// macOS [`AutostartService`]: **a stub in M1**.
///
/// The real implementation registers the app bundle as a login item via
/// `SMAppService.mainApp` (`register()` / `unregister()` / `status`); that is M2 work
/// (see docs/rust/MACOS.md), because it needs a bundled `.app` to register. Until then
/// the service reports unsupported and every call is a no-op.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacAutostart;

impl AutostartService for MacAutostart {
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
