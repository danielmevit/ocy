use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use damt_platform::IdleProvider;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::wl_registry::{self as wl_registry, WlRegistry},
    protocol::wl_seat::{Event as SeatEvent, WlSeat},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{Event as IdleNotificationEvent, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};
use x11rb::connection::Connection as _;
use x11rb::protocol::screensaver;
use x11rb::rust_connection::RustConnection;
use zbus::blocking::Connection as DbusConnection;

use crate::pure::{ExtIdleMapping, IdleEnv, IdleMath, IdleSourceKind, probe_idle_source};

const MUTTER_DEST: &str = "org.gnome.Mutter.IdleMonitor";
const MUTTER_PATH: &str = "/org/gnome/Mutter/IdleMonitor/Core";
const MUTTER_IFACE: &str = "org.gnome.Mutter.IdleMonitor";

struct X11IdleState {
    conn: RustConnection,
    root: u32,
}

enum IdleBackend {
    X11(Box<Mutex<Option<X11IdleState>>>),
    Mutter(DbusConnection),
    ExtIdleNotify(Arc<ExtIdleState>),
    Unsupported,
}

struct ExtIdleState {
    mapping: Arc<Mutex<ExtIdleMapping>>,
    stop: Arc<AtomicBool>,
}

struct IdleNotifyData {
    mapping: Arc<Mutex<ExtIdleMapping>>,
    notifier: Option<ExtIdleNotifierV1>,
    seat: Option<WlSeat>,
    notification: Option<ExtIdleNotificationV1>,
}

/// Linux [`IdleProvider`] using X11 XScreenSaver, GNOME Mutter D-Bus, or
/// `ext-idle-notify-v1` on Wayland.
pub struct LinuxIdle {
    backend: IdleBackend,
}

impl fmt::Debug for LinuxIdle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match &self.backend {
            IdleBackend::X11(_) => "X11ScreenSaver",
            IdleBackend::Mutter(_) => "GnomeMutterIdleMonitor",
            IdleBackend::ExtIdleNotify(_) => "ExtIdleNotify",
            IdleBackend::Unsupported => "Unsupported",
        };
        f.debug_struct("LinuxIdle").field("backend", &kind).finish()
    }
}

impl Default for LinuxIdle {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxIdle {
    /// Probes available idle backends in priority order for the current session.
    #[must_use]
    pub fn new() -> Self {
        let env = IdleEnv::from_env();
        let mut x11_state = None;
        let mut mutter_conn = None;
        let mut ext_idle_state = None;

        let chosen = probe_idle_source(&env, |kind| match kind {
            IdleSourceKind::X11ScreenSaver => {
                if let Some(state) = connect_x11_screensaver() {
                    x11_state = Some(state);
                    true
                } else {
                    false
                }
            }
            IdleSourceKind::GnomeMutterIdleMonitor => {
                if let Some(conn) = connect_mutter_idle() {
                    mutter_conn = Some(conn);
                    true
                } else {
                    false
                }
            }
            IdleSourceKind::ExtIdleNotify => {
                if let Some(state) = connect_ext_idle_notify() {
                    ext_idle_state = Some(state);
                    true
                } else {
                    false
                }
            }
        });

        let backend = match chosen {
            Some(IdleSourceKind::X11ScreenSaver) => {
                IdleBackend::X11(Box::new(Mutex::new(x11_state)))
            }
            Some(IdleSourceKind::GnomeMutterIdleMonitor) => match mutter_conn {
                Some(conn) => IdleBackend::Mutter(conn),
                None => IdleBackend::Unsupported,
            },
            Some(IdleSourceKind::ExtIdleNotify) => match ext_idle_state {
                Some(state) => IdleBackend::ExtIdleNotify(state),
                None => IdleBackend::Unsupported,
            },
            None => IdleBackend::Unsupported,
        };

        Self { backend }
    }
}

impl IdleProvider for LinuxIdle {
    fn is_supported(&self) -> bool {
        !matches!(self.backend, IdleBackend::Unsupported)
    }

    fn idle_time(&self) -> Duration {
        match &self.backend {
            IdleBackend::X11(slot) => {
                let Ok(mut guard) = slot.lock() else {
                    return Duration::ZERO;
                };
                if guard.is_none() {
                    *guard = connect_x11_screensaver();
                }
                let Some(state) = guard.as_ref() else {
                    return Duration::ZERO;
                };
                if let Some(ms) = query_x11_idle_ms(&state.conn, state.root) {
                    IdleMath::from_x11_millis(ms)
                } else {
                    *guard = None;
                    Duration::ZERO
                }
            }
            IdleBackend::Mutter(conn) => query_mutter_idle_ms(conn)
                .map(IdleMath::from_mutter_millis)
                .unwrap_or(Duration::ZERO),
            IdleBackend::ExtIdleNotify(state) => {
                let Ok(mapping) = state.mapping.lock() else {
                    return Duration::ZERO;
                };
                mapping.idle_time(std::time::Instant::now())
            }
            IdleBackend::Unsupported => Duration::ZERO,
        }
    }
}

fn connect_x11_screensaver() -> Option<X11IdleState> {
    let (conn, screen_num) = RustConnection::connect(None).ok()?;
    let root = conn.setup().roots.get(screen_num)?.root;
    screensaver::query_version(&conn, 1, 0).ok()?.reply().ok()?;
    query_x11_idle_ms(&conn, root)?;
    Some(X11IdleState { conn, root })
}

fn query_x11_idle_ms(conn: &RustConnection, root: u32) -> Option<u32> {
    let reply = screensaver::query_info(conn, root).ok()?.reply().ok()?;
    Some(reply.ms_since_user_input)
}

fn connect_mutter_idle() -> Option<DbusConnection> {
    let conn = DbusConnection::session().ok()?;
    let _ = query_mutter_idle_ms(&conn)?;
    Some(conn)
}

fn query_mutter_idle_ms(conn: &DbusConnection) -> Option<u64> {
    let reply = conn
        .call_method(
            Some(MUTTER_DEST),
            MUTTER_PATH,
            Some(MUTTER_IFACE),
            "GetIdletime",
            &(),
        )
        .ok()?;
    reply.body().deserialize::<u64>().ok()
}

impl Drop for ExtIdleState {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl Dispatch<WlRegistry, ()> for IdleNotifyData {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        connection: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };

        if interface == "ext_idle_notifier_v1" {
            let notifier = registry.bind::<ExtIdleNotifierV1, _, _>(name, version.min(1), qh, ());
            state.notifier = Some(notifier);
        } else if interface == "wl_seat" {
            state.seat = Some(registry.bind::<WlSeat, _, _>(name, version.min(1), qh, ()));
        }

        if let (Some(notifier), Some(seat)) = (&state.notifier, &state.seat)
            && state.notification.is_none()
        {
            state.notification = Some(notifier.get_idle_notification(1_000, seat, qh, ()));
        }

        let _ = connection;
    }
}

impl Dispatch<WlSeat, ()> for IdleNotifyData {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: SeatEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for IdleNotifyData {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: <ExtIdleNotifierV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, ()> for IdleNotifyData {
    fn event(
        state: &mut Self,
        _: &ExtIdleNotificationV1,
        event: IdleNotificationEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Ok(mut mapping) = state.mapping.lock() else {
            return;
        };
        match event {
            IdleNotificationEvent::Idled => {
                mapping.on_idled(std::time::Instant::now(), Duration::from_secs(1));
            }
            IdleNotificationEvent::Resumed => mapping.on_resumed(),
            _ => {}
        }
    }
}

fn connect_ext_idle_notify() -> Option<Arc<ExtIdleState>> {
    let connection = Connection::connect_to_env().ok()?;
    let mut event_queue = connection.new_event_queue();
    let qh = event_queue.handle();
    let mapping = Arc::new(Mutex::new(ExtIdleMapping::new()));
    let mut data = IdleNotifyData {
        mapping: Arc::clone(&mapping),
        notifier: None,
        seat: None,
        notification: None,
    };

    connection.display().get_registry(&qh, ());
    event_queue.roundtrip(&mut data).ok()?;
    data.notification.as_ref()?;

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let spawn_result = thread::Builder::new()
        .name(String::from("damt-wayland-idle"))
        .spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                if event_queue.blocking_dispatch(&mut data).is_err() {
                    break;
                }
            }
        });
    spawn_result.ok()?;

    Some(Arc::new(ExtIdleState { mapping, stop }))
}
