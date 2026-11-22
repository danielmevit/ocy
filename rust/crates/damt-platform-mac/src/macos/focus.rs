//! Focus and foreground-app detection.
//!
//! macOS has no public equivalent of `SHQueryUserNotificationState`: without the
//! Accessibility permission there is no reliable, complete full-screen/presentation
//! signal. M1 reports the focus state honestly as `Unknown` and exposes the frontmost
//! application (bundle id, name, pid) through [`MacForeground`], which needs no
//! permission. A `CGWindowList`-based full-screen heuristic is deferred to M2.

use std::fmt;

use damt_platform::{FocusState, FocusStateProvider, ForegroundObservation, ForegroundProbe};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};

use crate::pure::frontmost_observation;

/// macOS [`FocusStateProvider`]: **unsupported in M1**.
///
/// Full-screen and presentation-mode detection would need window-server queries
/// (`CGWindowListCopyWindowInfo`) or the Accessibility permission; until that is
/// prototyped (M2), the state is honestly `Unknown`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacFocus;

impl FocusStateProvider for MacFocus {
    fn is_supported(&self) -> bool {
        false
    }

    fn state(&self) -> FocusState {
        FocusState::Unknown
    }
}

/// macOS [`ForegroundProbe`]: the frontmost application via `NSWorkspace`, without any
/// permission.
///
/// `NSWorkspace` is main-thread API, so [`ForegroundProbe::observe`] answers `unknown`
/// when called off the main thread; `process_alive` answers `true` there instead (the
/// contract prefers not ending a hold early over a wrong "gone" answer).
pub struct MacForeground;

impl fmt::Debug for MacForeground {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MacForeground")
    }
}

impl Default for MacForeground {
    fn default() -> Self {
        Self::new()
    }
}

impl MacForeground {
    /// Creates the foreground probe.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ForegroundProbe for MacForeground {
    fn is_supported(&self) -> bool {
        true
    }

    fn observe(&self) -> ForegroundObservation {
        // NSWorkspace is main-thread-only API; anything else reports "nothing known".
        if MainThreadMarker::new().is_none() {
            return ForegroundObservation::unknown();
        }
        let workspace = NSWorkspace::sharedWorkspace();
        let Some(app) = workspace.frontmostApplication() else {
            return ForegroundObservation::unknown();
        };
        let bundle_id = app.bundleIdentifier().map(|value| value.to_string());
        let name = app.localizedName().map(|value| value.to_string());
        frontmost_observation(
            bundle_id.as_deref(),
            name.as_deref(),
            Some(app.processIdentifier()),
        )
    }

    fn game_bar_exe_paths(&self) -> Vec<String> {
        // No macOS equivalent of the Windows Game Bar game list exists.
        Vec::new()
    }

    fn process_alive(&self, process_id: u32) -> bool {
        // NSWorkspace/NSRunningApplication are main-thread API; off the main thread we
        // cannot tell, and the contract says to answer `true` in that case.
        if MainThreadMarker::new().is_none() {
            return true;
        }
        let Ok(pid) = i32::try_from(process_id) else {
            return false;
        };
        match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            Some(app) => !app.isTerminated(),
            // Not found: the process is gone (or was never ours to see).
            None => false,
        }
    }
}
