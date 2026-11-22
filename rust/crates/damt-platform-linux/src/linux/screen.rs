use std::fmt;
use std::sync::Mutex;

use damt_platform::{ActiveScreenProvider, ScreenRect};
use x11rb::connection::Connection as _;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use crate::pure::{IdleEnv, select_active_monitor};

struct X11ScreenState {
    conn: RustConnection,
    root: u32,
    net_active_window: Option<u32>,
}

/// Linux [`ActiveScreenProvider`] using X11 RandR + `_NET_ACTIVE_WINDOW` / pointer coordinates.
///
/// Reports `Unsupported` (`None`) on Wayland and headless sessions.
pub struct LinuxActiveScreen {
    state: Option<Mutex<X11ScreenState>>,
}

impl fmt::Debug for LinuxActiveScreen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxActiveScreen")
            .field("supported", &self.state.is_some())
            .finish()
    }
}

impl Default for LinuxActiveScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxActiveScreen {
    /// Initializes X11 RandR screen discovery when running on an X11 session.
    #[must_use]
    pub fn new() -> Self {
        let env = IdleEnv::from_env();
        let state = if env.is_x11() {
            connect_x11_screen().map(Mutex::new)
        } else {
            None
        };
        Self { state }
    }
}

impl ActiveScreenProvider for LinuxActiveScreen {
    fn is_supported(&self) -> bool {
        self.state.is_some()
    }

    fn active_screen_bounds(&self) -> Option<ScreenRect> {
        let slot = self.state.as_ref()?;
        let guard = slot.lock().ok()?;
        let monitors = query_randr_monitors(&guard.conn, guard.root);
        let focus_rect = guard
            .net_active_window
            .and_then(|atom| query_active_window_rect(&guard.conn, guard.root, atom));
        let pointer = query_pointer_pos(&guard.conn, guard.root);
        select_active_monitor(&monitors, focus_rect, pointer)
    }
}

fn connect_x11_screen() -> Option<X11ScreenState> {
    let (conn, screen_num) = RustConnection::connect(None).ok()?;
    let root = conn.setup().roots.get(screen_num)?.root;
    conn.randr_query_version(1, 5).ok()?.reply().ok()?;
    let net_active_window = conn
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.atom)
        .filter(|&a| a != 0);
    Some(X11ScreenState {
        conn,
        root,
        net_active_window,
    })
}

fn query_randr_monitors(conn: &RustConnection, root: u32) -> Vec<ScreenRect> {
    let Ok(cookie) = conn.randr_get_screen_resources_current(root) else {
        return Vec::new();
    };
    let Ok(res) = cookie.reply() else {
        return Vec::new();
    };

    let mut rects = Vec::new();
    for crtc in res.crtcs {
        let Ok(info_cookie) = conn.randr_get_crtc_info(crtc, x11rb::CURRENT_TIME) else {
            continue;
        };
        let Ok(info) = info_cookie.reply() else {
            continue;
        };
        if info.mode != 0 && info.width > 0 && info.height > 0 {
            rects.push(ScreenRect {
                x: i32::from(info.x),
                y: i32::from(info.y),
                width: i32::from(info.width),
                height: i32::from(info.height),
            });
        }
    }
    rects
}

fn query_active_window_rect(conn: &RustConnection, root: u32, atom: u32) -> Option<ScreenRect> {
    let prop = conn
        .get_property(false, root, atom, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    let win = prop.value32()?.next().filter(|&w| w != 0)?;
    let geom = conn.get_geometry(win).ok()?.reply().ok()?;
    let coords = conn
        .translate_coordinates(win, root, 0, 0)
        .ok()?
        .reply()
        .ok()?;
    Some(ScreenRect {
        x: i32::from(coords.dst_x),
        y: i32::from(coords.dst_y),
        width: i32::from(geom.width),
        height: i32::from(geom.height),
    })
}

fn query_pointer_pos(conn: &RustConnection, root: u32) -> Option<(i32, i32)> {
    let reply = conn.query_pointer(root).ok()?.reply().ok()?;
    Some((i32::from(reply.root_x), i32::from(reply.root_y)))
}
