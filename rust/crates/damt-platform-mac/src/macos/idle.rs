//! Idle time via `CGEventSourceSecondsSinceLastEventType`.

use std::time::Duration;

use damt_platform::IdleProvider;
use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};

use crate::pure::{IdleMath, K_CG_ANY_INPUT_EVENT_TYPE};

/// macOS [`IdleProvider`]: `CGEventSourceSecondsSinceLastEventType` on the combined
/// session state (`kCGEventSourceStateCombinedSessionState`) for any input event
/// (`kCGAnyInputEventType`), so it covers keyboard, mouse, and trackpad across all
/// event sources of the login session.
///
/// The call needs no permission and works from any thread.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacIdle;

impl IdleProvider for MacIdle {
    fn is_supported(&self) -> bool {
        true
    }

    fn idle_time(&self) -> Duration {
        // The objc2-core-graphics binding wraps this thread-safe read-only query in a
        // safe function; the state id and event type are well-known constants.
        let seconds = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            CGEventType(K_CG_ANY_INPUT_EVENT_TYPE),
        );
        IdleMath::from_seconds(seconds)
    }
}
