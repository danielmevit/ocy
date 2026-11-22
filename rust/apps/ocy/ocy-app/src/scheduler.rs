//! Adaptive host scheduler: computes the next single-shot wake delay from an engine
//! snapshot, so the app wakes only when something can happen instead of every second.
//!
//! The delay is the minimum of:
//! - the time until the next due moment (short break, long-break warning, long break,
//!   pause expiry) **or, when that moment is more than [`PREWARM_LEAD_TIME`] away, the
//!   moment minus [`PREWARM_LEAD_TIME`]** — the pre-warm wake, so the next card is
//!   created hidden before it becomes due (at most one extra wake per break),
//! - [`ACTIVE_WAKE_INTERVAL`] while a break or warning window is visible or a
//!   natural-moment hold is active (their countdowns and lull checks need 1 s),
//! - [`IDLE_PHASE_WAKE_INTERVAL`] while `Away` or `Deferred`, and while `Working` with
//!   `poll_external` (a game hold whose end the OS does not notify). The return is
//!   accepted up to 5 s late. A post-game break can then wait one more natural moment
//!   (at most 60 s), so it arrives within about a minute,
//! - [`MAX_WAKE_DELAY`] otherwise, which keeps every tick gap far below the engine's
//!   15 s sleep-gap threshold (rule 1: gap = sleep) without changing that rule,
//! - and never less than [`MIN_WAKE_DELAY`], to avoid busy loops.
//!
//! Session events (unlock, resume) are delivered immediately by the OS and re-arm the
//! timer, so a locked-screen return is still noticed at once; only idle returns
//! (typing resumes without a lock) and fullscreen exits are polled at the 5 s cadence.

use std::time::Duration;

use jiff::Timestamp;
use ocy_core::engine::{EnginePhase, EngineSnapshot};
use ocy_core::time::Paused;

use crate::break_logic::PREWARM_LEAD_TIME;
use crate::license::LicenseState;

/// Shortest wake delay; used when a due moment is now (the tick resolves it immediately).
pub const MIN_WAKE_DELAY: Duration = Duration::from_millis(50);

/// Wake interval while a window is visible, a natural-moment hold is active, or the
/// engine polls an external state change (away return, deferred fullscreen exit).
pub const ACTIVE_WAKE_INTERVAL: Duration = Duration::from_secs(1);

/// Wake interval while `Away` or `Deferred`: the long idle phases, where the only
/// thing that can happen is an external state change (input resumes, fullscreen
/// exits) that the OS does not notify us about. It is noticed up to 5 s late —
/// negligible for a schedule measured in minutes, and far below the engine's 15 s
/// sleep-gap threshold (rule 1: gap = sleep).
pub const IDLE_PHASE_WAKE_INTERVAL: Duration = Duration::from_secs(5);

/// Longest wake delay while working (idle re-checks and away detection granularity).
pub const MAX_WAKE_DELAY: Duration = Duration::from_secs(10);

/// The delay at which to wake for a due moment so its card can be pre-warmed: at the
/// due moment itself when it is within [`PREWARM_LEAD_TIME`], otherwise one wake
/// earlier at `due - PREWARM_LEAD_TIME` (at most one extra wake per break).
fn prewarm_wake_delay(until_due: Duration) -> Duration {
    if until_due > PREWARM_LEAD_TIME {
        until_due - PREWARM_LEAD_TIME
    } else {
        until_due
    }
}

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
            } else if snapshot.poll_external {
                // A game hold is active before the long break is due. Poll so leaving
                // the game is noticed within 5 s (then a natural moment may hold 60 s).
                IDLE_PHASE_WAKE_INTERVAL
            } else {
                let mut delay = MAX_WAKE_DELAY;
                if let Some(until_short) = snapshot.time_until_short_break {
                    delay = delay.min(prewarm_wake_delay(until_short));
                }
                if let Some(until_long) = snapshot.time_until_long_break {
                    delay = delay.min(prewarm_wake_delay(until_long));
                }
                if let Some(until_warning) = snapshot.time_until_long_break_warning {
                    delay = delay.min(prewarm_wake_delay(until_warning));
                }
                delay
            }
        }
        // Break/warning windows are visible (their countdowns update every second) and
        // natural-moment holds re-check the lull every second. Away and Deferred are
        // the long idle phases: nothing but an external state change (input resumes,
        // fullscreen exits) can happen, with no OS event to wake us, so they are
        // polled at the coarser IDLE_PHASE_WAKE_INTERVAL cadence — the return is
        // noticed up to 5 s late (session unlock/resume events bypass this entirely:
        // they are delivered immediately and re-arm the timer). This keeps adaptive
        // and 1 s timing equivalent within 5 s for away returns and deferral ends and
        // within 1 s for everything else; the working phase still meets the
        // <= 6 wake-ups/minute budget, and the idle phases <= 12.
        EnginePhase::ShortBreak | EnginePhase::LongBreak | EnginePhase::LongBreakWarning => {
            ACTIVE_WAKE_INTERVAL
        }
        EnginePhase::Deferred | EnginePhase::Away => IDLE_PHASE_WAKE_INTERVAL,
    };
    delay.max(MIN_WAKE_DELAY)
}

/// Computes the next wake delay while respecting Store [`LicenseState`].
///
/// When reminders are disallowed (`TrialExpired`), no break or pre-warm wakes are scheduled.
#[must_use]
pub fn next_wake_delay_for_license(
    snapshot: &EngineSnapshot,
    now: Timestamp,
    license: LicenseState,
) -> Duration {
    if !license.allows_reminders() {
        return MAX_WAKE_DELAY;
    }
    next_wake_delay(snapshot, now)
}
