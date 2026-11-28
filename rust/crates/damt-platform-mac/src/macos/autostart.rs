//! Autostart via `SMAppService.mainApp` on macOS 13+ (unsupported on older macOS).

use damt_platform::{AutostartService, PlatformError};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSError, NSProcessInfo};

use crate::pure::{
    MacOsVersion, SmAppServiceState, is_sm_app_service_supported, map_sm_app_service_status,
};

// `ServiceManagement.framework` has existed since macOS 10.6; linking it lets the Objective-C
// runtime resolve `SMAppService` dynamically on macOS 13+ while returning `None` on macOS <= 12.
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

/// macOS [`AutostartService`] backed by `SMAppService.mainApp` (macOS 13+ Ventura and newer).
///
/// On macOS 12 and earlier (or when the `SMAppService` class is absent), [`AutostartService::is_supported`]
/// returns `false`, [`AutostartService::is_enabled`] returns `false`, and
/// [`AutostartService::set_enabled`] is a no-op `Ok(())`.
///
/// Note: on macOS 13+, `SMAppService.mainApp` expects a bundled `.app` (M3 packaging); calling
/// `registerAndReturnError:` from a bare unbundled binary may return an OS permission/launch-services
/// error, which is surfaced via [`PlatformError`].
#[derive(Debug, Default, Clone, Copy)]
pub struct MacAutostart;

impl MacAutostart {
    fn main_app_service() -> Option<Retained<AnyObject>> {
        let info = NSProcessInfo::processInfo();
        let ver = info.operatingSystemVersion();
        let cls = AnyClass::get(c"SMAppService");
        if !is_sm_app_service_supported(
            MacOsVersion {
                major: ver.majorVersion,
                minor: ver.minorVersion,
                patch: ver.patchVersion,
            },
            cls.is_some(),
        ) {
            return None;
        }
        let cls = cls?;
        // SAFETY: `SMAppService` is present on macOS 13+ (verified above) and defines the
        // class property `+[SMAppService mainAppService]` returning an `SMAppService *`.
        unsafe { msg_send![cls, mainAppService] }
    }
}

impl AutostartService for MacAutostart {
    fn is_supported(&self) -> bool {
        Self::main_app_service().is_some()
    }

    fn is_enabled(&self) -> bool {
        let Some(service) = Self::main_app_service() else {
            return false;
        };
        // SAFETY: `service` is a valid `SMAppService` instance; `-[SMAppService status]`
        // returns `SMAppServiceStatus` (`NSInteger` / `isize`).
        let status: isize = unsafe { msg_send![&*service, status] };
        map_sm_app_service_status(status).is_enabled()
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        let Some(service) = Self::main_app_service() else {
            return Ok(());
        };
        if enabled {
            // SAFETY: `service` is a valid `SMAppService` instance; `registerAndReturnError:`
            // follows Cocoa's `(NSError **) -> BOOL` out-error convention.
            let res: Result<(), Retained<NSError>> =
                unsafe { msg_send![&*service, registerAndReturnError: _] };
            res.map_err(|err| {
                PlatformError(format!(
                    "SMAppService.mainApp register failed: {}",
                    err.localizedDescription()
                ))
            })
        } else {
            // SAFETY: `service` is a valid `SMAppService` instance; reading `status` before
            // unregistering avoids spurious LaunchServices errors when already unregistered.
            let status: isize = unsafe { msg_send![&*service, status] };
            if map_sm_app_service_status(status) == SmAppServiceState::NotRegistered {
                return Ok(());
            }
            // SAFETY: `service` is a valid `SMAppService` instance; `unregisterAndReturnError:`
            // follows Cocoa's `(NSError **) -> BOOL` out-error convention.
            let res: Result<(), Retained<NSError>> =
                unsafe { msg_send![&*service, unregisterAndReturnError: _] };
            res.map_err(|err| {
                PlatformError(format!(
                    "SMAppService.mainApp unregister failed: {}",
                    err.localizedDescription()
                ))
            })
        }
    }
}
