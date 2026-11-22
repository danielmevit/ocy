use std::fmt;
use std::sync::Mutex;

use damt_platform::{FocusState, FocusStateProvider};
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use crate::pure::{IdleEnv, ewmh_has_fullscreen};

struct X11FocusState {
    conn: RustConnection,
    root: u32,
    net_active_window: u32,
    net_wm_state: u32,
    net_wm_state_fullscreen: u32,
}

/// Linux [`FocusStateProvider`] using X11 EWMH (`_NET_ACTIVE_WINDOW` + `_NET_WM_STATE_FULLSCREEN`).
///
/// Reports `Unsupported` (`FocusState::Unknown`) on Wayland and headless sessions.
pub struct LinuxFocus {
    state: Option<Mutex<X11FocusState>>,
}

impl fmt::Debug for LinuxFocus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxFocus")
            .field("supported", &self.state.is_some())
            .finish()
    }
}

impl Default for LinuxFocus {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxFocus {
    /// Initializes X11 EWMH focus tracking when running on an X11 session.
    #[must_use]
    pub fn new() -> Self {
        let env = IdleEnv::from_env();
        let state = if env.is_x11() {
            connect_x11_focus().map(Mutex::new)
        } else {
            None
        };
        Self { state }
    }
}

impl FocusStateProvider for LinuxFocus {
    fn is_supported(&self) -> bool {
        self.state.is_some()
    }

    fn state(&self) -> FocusState {
        let Some(slot) = &self.state else {
            return FocusState::Unknown;
        };
        let Ok(guard) = slot.lock() else {
            return FocusState::Unknown;
        };
        query_active_window_focus(&guard).unwrap_or(FocusState::Unknown)
    }
}

fn connect_x11_focus() -> Option<X11FocusState> {
    let (conn, screen_num) = RustConnection::connect(None).ok()?;
    let root = conn.setup().roots.get(screen_num)?.root;
    let net_active_window = intern_atom(&conn, b"_NET_ACTIVE_WINDOW")?;
    let net_wm_state = intern_atom(&conn, b"_NET_WM_STATE")?;
    let net_wm_state_fullscreen = intern_atom(&conn, b"_NET_WM_STATE_FULLSCREEN")?;
    Some(X11FocusState {
        conn,
        root,
        net_active_window,
        net_wm_state,
        net_wm_state_fullscreen,
    })
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Option<u32> {
    let reply = conn.intern_atom(false, name).ok()?.reply().ok()?;
    (reply.atom != 0).then_some(reply.atom)
}

fn query_active_window_focus(state: &X11FocusState) -> Option<FocusState> {
    let active_reply = state
        .conn
        .get_property(
            false,
            state.root,
            state.net_active_window,
            AtomEnum::WINDOW,
            0,
            1,
        )
        .ok()?
        .reply()
        .ok()?;
    let active_window = active_reply.value32()?.next().filter(|&w| w != 0)?;

    let wm_state_reply = state
        .conn
        .get_property(
            false,
            active_window,
            state.net_wm_state,
            AtomEnum::ATOM,
            0,
            64,
        )
        .ok()?
        .reply()
        .ok()?;
    let atoms: Vec<u32> = wm_state_reply.value32()?.collect();
    Some(ewmh_has_fullscreen(&atoms, state.net_wm_state_fullscreen))
}
