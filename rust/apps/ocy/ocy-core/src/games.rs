//! Game classification and the games break policy.
//!
//! Full-screen presentations and videos keep holding breaks. A full-screen game follows
//! [`GamesMode`] (gentle by default) unless its exe is in `games_silent`, the user asked to
//! hold until that process exits, or the game is an exclusive full-screen session whose exe
//! could not be read.
//!
//! A game is the foreground exe listed in the Windows Game Bar store, or
//! `SHQueryUserNotificationState` reporting `QUNS_RUNNING_D3D_FULL_SCREEN`. Borderless games
//! that only report `QUNS_BUSY` and are not in that store stay with the video/other rule:
//! they hold breaks, and leaving them does not pull the next break forward.
//!
//! After a game hold (or an undetectable exclusive session) of at least [`MIN_PLAY_FOR_PROMPT`],
//! leaving pulls the next break forward so it is due immediately. The host still wakes on its
//! normal cadence — at most 10 s while working, 5 s while `poll_external` or deferred — and a
//! natural moment may hold the start for up to 60 s. The break therefore arrives within about
//! a minute, not on the next full interval. A sleep gap, startup gap, pause, or away period
//! abandons the streak and does not prompt.
//!
//! The gentle long-break card is a short nudge, not the real long break: it uses the short-break
//! duration and ends as `BreakOutcome::Passed` without completing or skipping a long break.
//! Starting the real long break from that card interrupts it and starts a normal dimmed long break.
//! Warning and postpone rules are unchanged. If a game hold begins while the warning is already
//! showing and the warning then elapses, the long break is deferred instead of dimming the game.
//! A video or presentation that is already in the warning still starts the normal long break.

use std::time::Duration;

use damt_platform::{
    FocusState, ForegroundObservation, QUNS_APP, QUNS_BUSY, QUNS_PRESENTATION_MODE,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};

use crate::schedule::ScheduleCalculator;
use crate::settings::{GamesMode, OcySettings};

/// `QUNS_NOT_PRESENT`.
const QUNS_NOT_PRESENT: i32 = 1;
/// `QUNS_ACCEPTS_NOTIFICATIONS` (an ordinary windowed app).
const QUNS_ACCEPTS_NOTIFICATIONS: i32 = 5;
/// `QUNS_QUIET_TIME`.
const QUNS_QUIET_TIME: i32 = 6;

/// Continuous game-hold after which leaving the game pulls the next break forward.
pub const MIN_PLAY_FOR_PROMPT: Duration = Duration::from_secs(45 * 60);

/// What the full-screen foreground app is, as far as breaks are concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullscreenClass {
    /// Not a full-screen hold and not a gentle game (includes a windowed game).
    NotFullscreen,
    /// Full-screen game.
    Game {
        /// Exe file name when it could be read.
        exe_name: Option<String>,
        /// Foreground process id when known.
        process_id: Option<u32>,
        /// Exclusive Direct3D full screen whose exe name could not be read.
        undetectable_exclusive: bool,
    },
    /// Presentation, video, or other full-screen work.
    PresentationOrOther,
}

/// Whether the current foreground app should hold breaks or show a gentle game card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameDecision {
    /// Defer a due break (fullscreen video/presentation, silent game, hold mode, or hold-until-close).
    pub break_held: bool,
    /// A detectable game in gentle mode: show the small card, never dim, never take focus.
    pub gentle_game: bool,
    /// A game hold. Ordinary video and presentation fullscreen is not this, so it does not
    /// arm the post-game prompt.
    pub suppressed_play: bool,
}

/// Which due break a post-game prompt should pull forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptPull {
    /// No enabled break to pull.
    None,
    /// Make the short break due now.
    Short,
    /// Make the long break, or its warning, due now.
    Long,
}

/// Classifies the foreground app. An observation with no notification state and no exe keeps
/// today's rule: fullscreen and presentation hold, everything else does not.
#[must_use]
pub fn classify(
    focus: FocusState,
    observation: &ForegroundObservation,
    game_bar_paths: &[String],
) -> FullscreenClass {
    if !observation_has_detail(observation) {
        return match focus {
            FocusState::FullscreenApp | FocusState::Presentation => {
                FullscreenClass::PresentationOrOther
            }
            _ => FullscreenClass::NotFullscreen,
        };
    }

    let quns = observation.notification_state;
    // A fresh accepting, quiet, or not-present reading wins over a stale fullscreen focus.
    if matches!(
        quns,
        Some(QUNS_NOT_PRESENT | QUNS_ACCEPTS_NOTIFICATIONS | QUNS_QUIET_TIME)
    ) {
        return FullscreenClass::NotFullscreen;
    }

    let presentation = quns == Some(QUNS_PRESENTATION_MODE)
        || (quns.is_none() && focus == FocusState::Presentation);
    if presentation {
        return FullscreenClass::PresentationOrOther;
    }

    let fullscreen = match quns {
        Some(state)
            if state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_APP =>
        {
            true
        }
        Some(_) => false,
        None => focus == FocusState::FullscreenApp,
    };
    if !fullscreen {
        return FullscreenClass::NotFullscreen;
    }

    let d3d = quns == Some(QUNS_RUNNING_D3D_FULL_SCREEN);
    let exe_name = observation_exe_name(observation);
    if d3d || exe_in_game_bar(observation, game_bar_paths) {
        return FullscreenClass::Game {
            undetectable_exclusive: d3d && exe_name.is_none(),
            exe_name,
            process_id: observation.process_id,
        };
    }
    FullscreenClass::PresentationOrOther
}

/// Applies settings and an active hold-until-close to a [`FullscreenClass`].
///
/// `hold_until_alive` is true only when the user asked to hold until a game process exits
/// and that process is still alive. It holds even if the user has alt-tabbed away.
#[must_use]
pub fn decide(
    class: &FullscreenClass,
    settings: &OcySettings,
    hold_until_alive: bool,
) -> GameDecision {
    if hold_until_alive {
        return GameDecision {
            break_held: true,
            gentle_game: false,
            suppressed_play: true,
        };
    }
    match class {
        FullscreenClass::NotFullscreen => GameDecision {
            break_held: false,
            gentle_game: false,
            suppressed_play: false,
        },
        FullscreenClass::PresentationOrOther => GameDecision {
            break_held: settings.pause_during_fullscreen,
            gentle_game: false,
            suppressed_play: false,
        },
        FullscreenClass::Game {
            exe_name,
            undetectable_exclusive,
            ..
        } => {
            let silent = exe_name
                .as_deref()
                .is_some_and(|name| settings.is_silent_game(name));
            let mode_hold = settings.games_mode == GamesMode::Hold;
            if silent || mode_hold || *undetectable_exclusive {
                GameDecision {
                    break_held: true,
                    gentle_game: false,
                    suppressed_play: true,
                }
            } else {
                GameDecision {
                    break_held: false,
                    gentle_game: true,
                    suppressed_play: false,
                }
            }
        }
    }
}

/// Picks the soonest enabled break to pull forward after a long game hold.
///
/// The long break (or its warning, when one is configured) wins when it is sooner than the
/// short break, or when the short break is inside the two-minute suppression window. Otherwise
/// the short break is pulled. Callers must not move a counter backwards.
#[must_use]
pub fn prompt_pull(
    settings: &OcySettings,
    since_short_break: Duration,
    since_long_break: Duration,
) -> PromptPull {
    let short_on = settings.short_breaks.enabled;
    let long_on = settings.long_breaks.enabled;
    if long_on && !short_on {
        return PromptPull::Long;
    }
    if short_on && !long_on {
        return PromptPull::Short;
    }
    if !short_on && !long_on {
        return PromptPull::None;
    }

    let until_short = settings
        .short_breaks
        .interval()
        .saturating_sub(since_short_break);
    let until_long_event =
        ScheduleCalculator::time_until_long_break_warning(settings, since_long_break)
            .or_else(|| ScheduleCalculator::time_until_long_break(settings, since_long_break))
            .unwrap_or(settings.long_breaks.interval());
    let long_sooner = until_long_event <= until_short;
    let short_suppressed =
        ScheduleCalculator::is_short_break_suppressed_by_long_break(settings, since_long_break);
    if long_sooner || short_suppressed {
        PromptPull::Long
    } else {
        PromptPull::Short
    }
}

fn observation_has_detail(observation: &ForegroundObservation) -> bool {
    observation.notification_state.is_some()
        || observation
            .exe_name
            .as_ref()
            .is_some_and(|name| !name.trim().is_empty())
        || observation
            .exe_path
            .as_ref()
            .is_some_and(|path| !path.trim().is_empty())
        || observation.process_id.is_some()
}

fn observation_exe_name(observation: &ForegroundObservation) -> Option<String> {
    observation
        .exe_name
        .as_deref()
        .map(crate::settings::exe_file_name)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            observation
                .exe_path
                .as_deref()
                .map(crate::settings::exe_file_name)
                .filter(|name| !name.is_empty())
        })
}

fn exe_in_game_bar(observation: &ForegroundObservation, game_bar_paths: &[String]) -> bool {
    game_bar_paths
        .iter()
        .any(|stored| paths_match(observation, stored))
}

fn paths_match(observation: &ForegroundObservation, stored: &str) -> bool {
    let stored_norm = normalize_exe_path(stored);
    if stored_norm.is_empty() {
        return false;
    }
    let stored_bare = !stored_norm.contains('\\');
    if let Some(path) = observation.exe_path.as_deref() {
        let norm = normalize_exe_path(path);
        if norm == stored_norm {
            return true;
        }
        if stored_bare {
            let file = norm.rsplit('\\').next().unwrap_or(&norm);
            return file == stored_norm;
        }
        return false;
    }
    if stored_bare && let Some(name) = observation.exe_name.as_deref() {
        return normalize_exe_path(name) == stored_norm;
    }
    false
}

fn normalize_exe_path(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim()
        .replace('/', "\\")
        .to_ascii_lowercase()
}
