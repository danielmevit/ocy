//! XInput controller-activity polling for the game calm-moment rule (Rust-only).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use damt_platform::ControllerIdleProvider;
use windows::Win32::UI::Input::XboxController::{XINPUT_GAMEPAD, XINPUT_STATE, XInputGetState};

use crate::pure::{ControllerMath, GamepadSample, GamepadSignature};

/// XInput user-index slots (`XUSER_MAX_INDEX + 1`).
const XUSER_MAX_COUNT: u32 = 4;
/// `ERROR_SUCCESS`: the XInput slot is connected and its state was read.
const ERROR_SUCCESS: u32 = 0;

/// Time since the last significant controller input, polled on demand.
///
/// The engine calls [`ControllerIdleProvider::idle_time`] at its tick rate and only while
/// a gentle game is in the foreground, so this polls XInput only then. No hooks, no raw
/// input, no key data: only the XInput state of the four slots is read, and only the
/// *fact* of input (see [`ControllerMath::is_input`]) is kept — never which buttons.
#[derive(Debug, Default)]
pub struct WinControllerIdle {
    state: Mutex<PollState>,
}

/// Per-slot signatures of the previous poll plus the last input instant.
#[derive(Debug, Default)]
struct PollState {
    signatures: [Option<GamepadSignature>; XUSER_MAX_COUNT as usize],
    last_input: Option<Instant>,
}

impl ControllerIdleProvider for WinControllerIdle {
    fn is_supported(&self) -> bool {
        true
    }

    fn idle_time(&self) -> Duration {
        let now = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut input = false;
        for index in 0..XUSER_MAX_COUNT {
            let mut xinput = XINPUT_STATE::default();
            // SAFETY: `xinput` is a valid, properly aligned XINPUT_STATE that outlives the call.
            if unsafe { XInputGetState(index, &mut xinput) } != ERROR_SUCCESS {
                continue; // Slot not connected.
            }
            let sample = gamepad_sample(&xinput.Gamepad);
            if ControllerMath::is_input(state.signatures[index as usize], &sample) {
                input = true;
            }
            state.signatures[index as usize] = Some(ControllerMath::signature(&sample));
        }
        if input {
            state.last_input = Some(now);
        }
        match state.last_input {
            Some(last_input) => now.duration_since(last_input),
            // No controller has ever been seen connected: no controller input is possible.
            None => Duration::MAX,
        }
    }
}

/// Reduces one `XINPUT_GAMEPAD` to the raw values the calm-moment rule reads.
fn gamepad_sample(gamepad: &XINPUT_GAMEPAD) -> GamepadSample {
    GamepadSample {
        buttons: gamepad.wButtons.0,
        trigger_left: gamepad.bLeftTrigger,
        trigger_right: gamepad.bRightTrigger,
        thumb_left_x: gamepad.sThumbLX,
        thumb_left_y: gamepad.sThumbLY,
        thumb_right_x: gamepad.sThumbRX,
        thumb_right_y: gamepad.sThumbRY,
    }
}
