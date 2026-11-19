//! Adaptive host scheduler: computes the next single-shot wake delay from an engine
//! snapshot, so the app wakes only when something can happen instead of every second.
//!
//! The delay is the minimum of:
//! - the time until the next due moment (short break, long-break warning, long break,
//!   pause expiry),
//! - [`ACTIVE_WAKE_INTERVAL`] while a break or warning window is visible, while a
//!   natural-moment hold is active, or while the engine waits on an external state
//!   change it cannot be notified about (away return, fullscreen exit),
//! - [`MAX_WAKE_DELAY`] otherwise, which keeps every tick gap far below the engine's
//!   15 s sleep-gap threshold (rule 1: gap = sleep) without changing that rule,
//! - and never less than [`MIN_WAKE_DELAY`], to avoid busy loops.

use std::time::Duration;

use jiff::Timestamp;
use ocy_core::engine::{EnginePhase, EngineSnapshot};
use ocy_core::time::Paused;

/// Shortest wake delay; used when a due moment is now (the tick resolves it immediately).
pub const MIN_WAKE_DELAY: Duration = Duration::from_millis(50);

/// Wake interval while a window is visible, a natural-moment hold is active, or the
/// engine polls an external state change (away return, deferred fullscreen exit).
pub const ACTIVE_WAKE_INTERVAL: Duration = Duration::from_secs(1);

/// Longest wake delay while working (idle re-checks and away detection granularity).
pub const MAX_WAKE_DELAY: Duration = Duration::from_secs(10);

/// Computes the delay the host should sleep before the next engine tick.
///
/// `now` is the current instant (the engine clock's UTC timestamp); it is only used to
/// measure the remaining time until a pause expires.
#[must_use]
pub fn next_wake_delay(snapshot: &EngineSnapshot, now: Timestamp) -> Duration {
    let delay = match snapshot.phase {
        EnginePhase::Paused => {
            let mut delay = MAX_WAKE_DELAY;
            if let Some(Paused::Until(until)) = snapshot.paused_until_utc {
                let remaining = until.timestamp().duration_since(now);
                match Duration::try_from(remaining) {
                    Ok(remaining) if remaining > Duration::ZERO => {
                        delay = delay.min(remaining);
                    }
                    // Already expired: wake at the minimum so the pause ends promptly.
                    _ => return MIN_WAKE_DELAY,
                }
            }
            delay
        }
        EnginePhase::Working => {
            if snapshot.natural_hold.is_some() {
                // A scheduled start is due and waiting for a lull: re-check every second.
                ACTIVE_WAKE_INTERVAL
            } else {
                let mut delay = MAX_WAKE_DELAY;
                if let Some(until_short) = snapshot.time_until_short_break {
                    delay = delay.min(until_short);
                }
                if let Some(until_long) = snapshot.time_until_long_break {
                    delay = delay.min(until_long);
                }
                if let Some(until_warning) = snapshot.time_until_long_break_warning {
                    delay = delay.min(until_warning);
                }
                delay
            }
        }
        // Break/warning windows are visible (their countdowns update every second), and
        // Away/Deferred transitions come from outside (input resumes, fullscreen exits)
        // with no OS event to wake us, so they are polled at the same 1 s granularity
        // as the previous fixed tick. This keeps adaptive and 1 s timing equivalent
        // within 1 s; the working phase still meets the <= 6 wake-ups/minute budget.
        EnginePhase::ShortBreak
        | EnginePhase::LongBreak
        | EnginePhase::LongBreakWarning
        | EnginePhase::Deferred
        | EnginePhase::Away => ACTIVE_WAKE_INTERVAL,
    };
    delay.max(MIN_WAKE_DELAY)
}
