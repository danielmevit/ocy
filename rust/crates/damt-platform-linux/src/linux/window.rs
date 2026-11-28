use x11rb::connection::Connection as _;
use x11rb::properties::WmHints;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, CLIENT_MESSAGE_EVENT, ClientMessageData, ClientMessageEvent, ConfigureWindowAux,
    ConnectionExt as _, EventMask, PropMode, StackMode,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::pure::{
    IdleEnv, LinuxDisplayBackend, RawX11Crtc, X11_BREAK_STATE_ATOM_NAMES, X11_BREAK_USER_TIME,
    X11_BREAK_WM_HINTS_INPUT, X11_WM_STATE_ATOM, X11_WM_USER_TIME_ATOM, X11_WM_WINDOW_TYPE_ATOM,
    X11_WM_WINDOW_TYPE_NOTIFICATION_ATOM, X11MonitorSnapshot, merge_wm_state_atoms,
    net_wm_state_add_messages, parse_net_workarea, parse_xft_dpi, resolve_x11_monitors,
    select_display_backend,
};
use damt_platform::ScreenRect;

/// Applies X11 break-window hints (`_NET_WM_WINDOW_TYPE_NOTIFICATION`, `_NET_WM_STATE_ABOVE`,
/// `_NET_WM_STATE_SKIP_TASKBAR`, `_NET_WM_STATE_SKIP_PAGER`, `WM_HINTS` `input=false`, and
/// `_NET_WM_USER_TIME = 0`) to `window_id` over a dedicated `x11rb` connection.
pub fn apply_x11_break_window_hints(window_id: u32) -> Result<(), String> {
    if window_id == 0 {
        return Err("X11 window id must be non-zero".to_owned());
    }

    let (conn, screen_num) =
        RustConnection::connect(None).map_err(|err| format!("x11rb connect failed: {err}"))?;
    let root = conn
        .setup()
        .roots
        .get(screen_num)
        .ok_or_else(|| format!("missing X11 screen {screen_num}"))?
        .root;

    let window_type_atom = intern_atom(&conn, X11_WM_WINDOW_TYPE_ATOM.as_bytes())?;
    let notification_atom = intern_atom(&conn, X11_WM_WINDOW_TYPE_NOTIFICATION_ATOM.as_bytes())?;
    let wm_state_atom = intern_atom(&conn, X11_WM_STATE_ATOM.as_bytes())?;
    let user_time_atom = intern_atom(&conn, X11_WM_USER_TIME_ATOM.as_bytes())?;

    let mut state_atoms = [0_u32; 3];
    for (idx, name) in X11_BREAK_STATE_ATOM_NAMES.iter().enumerate() {
        state_atoms[idx] = intern_atom(&conn, name.as_bytes())?;
    }

    if window_type_atom != 0 && notification_atom != 0 {
        conn.change_property32(
            PropMode::REPLACE,
            window_id,
            window_type_atom,
            AtomEnum::ATOM,
            &[notification_atom],
        )
        .map_err(|err| format!("set _NET_WM_WINDOW_TYPE failed: {err}"))?;
    }

    if wm_state_atom != 0 {
        let existing_states = read_atom_list(&conn, window_id, wm_state_atom);
        let merged = merge_wm_state_atoms(&existing_states, &state_atoms);
        conn.change_property32(
            PropMode::REPLACE,
            window_id,
            wm_state_atom,
            AtomEnum::ATOM,
            &merged,
        )
        .map_err(|err| format!("set _NET_WM_STATE property failed: {err}"))?;

        for msg in net_wm_state_add_messages(&state_atoms) {
            let event = ClientMessageEvent {
                response_type: CLIENT_MESSAGE_EVENT,
                format: 32,
                sequence: 0,
                window: window_id,
                type_: wm_state_atom,
                data: ClientMessageData::from(msg),
            };
            conn.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|err| format!("send _NET_WM_STATE client message failed: {err}"))?;
        }
    }

    let mut hints = WmHints::get(&conn, window_id)
        .ok()
        .and_then(|cookie| cookie.reply().ok().flatten())
        .unwrap_or_default();
    hints.input = Some(X11_BREAK_WM_HINTS_INPUT);
    hints
        .set(&conn, window_id)
        .map_err(|err| format!("set WM_HINTS input=false failed: {err}"))?;

    if user_time_atom != 0 {
        conn.change_property32(
            PropMode::REPLACE,
            window_id,
            user_time_atom,
            AtomEnum::CARDINAL,
            &[X11_BREAK_USER_TIME],
        )
        .map_err(|err| format!("set _NET_WM_USER_TIME failed: {err}"))?;
    }

    conn.flush()
        .map_err(|err| format!("x11rb flush failed: {err}"))?;
    // Synchronize so the X server has committed every property before the caller maps the window
    // on winit's separate X11 connection.
    conn.sync()
        .map_err(|err| format!("x11rb sync failed: {err}"))?;
    Ok(())
}

/// Raises `window_id` to the top of its X11 stacking layer without changing input focus.
pub fn raise_x11_window_above(window_id: u32) -> Result<(), String> {
    if window_id == 0 {
        return Err("X11 window id must be non-zero".to_owned());
    }
    let (conn, _screen_num) =
        RustConnection::connect(None).map_err(|err| format!("x11rb connect failed: {err}"))?;
    conn.configure_window(
        window_id,
        &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
    )
    .map_err(|err| format!("raise_x11_window_above configure failed: {err}"))?;
    conn.flush()
        .map_err(|err| format!("raise_x11_window_above flush failed: {err}"))?;
    conn.sync()
        .map_err(|err| format!("raise_x11_window_above sync failed: {err}"))?;
    Ok(())
}

/// Queries active X11 RandR monitors, `_NET_WORKAREA`, `Xft.dpi`, and whether an EWMH
/// compositing manager owns `_NET_WM_CM_S<screen>`. Returns an empty snapshot on non-X11 sessions.
#[must_use]
pub fn query_x11_monitors() -> X11MonitorSnapshot {
    if select_display_backend(&IdleEnv::from_env()) != LinuxDisplayBackend::X11 {
        return X11MonitorSnapshot::default();
    }

    let Ok((conn, screen_num)) = RustConnection::connect(None) else {
        return X11MonitorSnapshot::default();
    };
    let Some(screen) = conn.setup().roots.get(screen_num) else {
        return X11MonitorSnapshot::default();
    };
    let root = screen.root;

    let has_compositor = query_has_compositor(&conn, screen_num);

    let Ok(randr_cookie) = conn.randr_query_version(1, 5) else {
        return X11MonitorSnapshot {
            monitors: Vec::new(),
            has_compositor,
        };
    };
    if randr_cookie.reply().is_err() {
        return X11MonitorSnapshot {
            monitors: Vec::new(),
            has_compositor,
        };
    }

    let Ok(res_cookie) = conn.randr_get_screen_resources_current(root) else {
        return X11MonitorSnapshot {
            monitors: Vec::new(),
            has_compositor,
        };
    };
    let Ok(res) = res_cookie.reply() else {
        return X11MonitorSnapshot {
            monitors: Vec::new(),
            has_compositor,
        };
    };

    let mut crtcs = Vec::new();
    for crtc in res.crtcs {
        let Ok(info_cookie) = conn.randr_get_crtc_info(crtc, x11rb::CURRENT_TIME) else {
            continue;
        };
        let Ok(info) = info_cookie.reply() else {
            continue;
        };
        if info.mode != 0 && info.width > 0 && info.height > 0 {
            crtcs.push(RawX11Crtc {
                id: crtc,
                rect: ScreenRect {
                    x: i32::from(info.x),
                    y: i32::from(info.y),
                    width: i32::from(info.width),
                    height: i32::from(info.height),
                },
                outputs: info.outputs,
            });
        }
    }

    let primary_output = conn
        .randr_get_output_primary(root)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.output);
    let work_area = query_net_workarea(&conn, root);
    let dpi = query_xft_dpi(&conn, root).unwrap_or(96);

    X11MonitorSnapshot {
        monitors: resolve_x11_monitors(&crtcs, primary_output, work_area, dpi),
        has_compositor,
    }
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Result<u32, String> {
    let cookie = conn
        .intern_atom(false, name)
        .map_err(|err| format!("intern_atom send failed: {err}"))?;
    let reply = cookie
        .reply()
        .map_err(|err| format!("intern_atom reply failed: {err}"))?;
    Ok(reply.atom)
}

fn read_atom_list(conn: &RustConnection, window: u32, prop: u32) -> Vec<u32> {
    let Ok(cookie) = conn.get_property(false, window, prop, AtomEnum::ATOM, 0, 64) else {
        return Vec::new();
    };
    let Ok(reply) = cookie.reply() else {
        return Vec::new();
    };
    reply
        .value32()
        .map(|iter| iter.collect())
        .unwrap_or_default()
}

fn query_has_compositor(conn: &RustConnection, screen_num: usize) -> bool {
    let atom_name = format!("_NET_WM_CM_S{screen_num}");
    let Ok(atom) = intern_atom(conn, atom_name.as_bytes()) else {
        return false;
    };
    if atom == 0 {
        return false;
    }
    conn.get_selection_owner(atom)
        .ok()
        .and_then(|c| c.reply().ok())
        .is_some_and(|r| r.owner != 0)
}

fn query_net_workarea(conn: &RustConnection, root: u32) -> Option<ScreenRect> {
    let workarea_atom = intern_atom(conn, b"_NET_WORKAREA")
        .ok()
        .filter(|&a| a != 0)?;
    let desktop_idx = intern_atom(conn, b"_NET_CURRENT_DESKTOP")
        .ok()
        .filter(|&a| a != 0)
        .and_then(|atom| {
            conn.get_property(false, root, atom, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()?
                .value32()?
                .next()
                .and_then(|idx| usize::try_from(idx).ok())
        })
        .unwrap_or(0);

    let prop = conn
        .get_property(false, root, workarea_atom, AtomEnum::CARDINAL, 0, 128)
        .ok()?
        .reply()
        .ok()?;
    let values: Vec<u32> = prop.value32()?.collect();
    parse_net_workarea(&values, desktop_idx).or_else(|| parse_net_workarea(&values, 0))
}

fn query_xft_dpi(conn: &RustConnection, root: u32) -> Option<u32> {
    let prop = conn
        .get_property(
            false,
            root,
            AtomEnum::RESOURCE_MANAGER,
            AtomEnum::STRING,
            0,
            16_384,
        )
        .ok()?
        .reply()
        .ok()?;
    let text = String::from_utf8_lossy(&prop.value);
    parse_xft_dpi(&text)
}
