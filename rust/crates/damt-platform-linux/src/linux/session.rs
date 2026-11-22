use std::fmt;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;

use damt_platform::{SessionEvent, SessionEventSource};
use futures_lite::{StreamExt, future, stream};
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, MatchRule, MessageStream};

use crate::pure::{map_dbus_signal, parse_raw_dbus_signal, resolve_logind_session_id};

const LOGIND_DEST: &str = "org.freedesktop.login1";
const LOGIND_MANAGER_PATH: &str = "/org/freedesktop/login1";
const LOGIND_MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
const LOGIND_SESSION_IFACE: &str = "org.freedesktop.login1.Session";
const SCREENSAVER_IFACE: &str = "org.freedesktop.ScreenSaver";

type Sink = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Linux [`SessionEventSource`] backed by `systemd-logind` over the system D-Bus
/// (`PrepareForSleep`, `PrepareForShutdown`, session `Lock`/`Unlock`), with an
/// `org.freedesktop.ScreenSaver` `ActiveChanged` fallback on the session bus.
pub struct LinuxSessionEvents {
    supported: bool,
    started: bool,
    stop_tx: Option<async_channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for LinuxSessionEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinuxSessionEvents")
            .field("supported", &self.supported)
            .field("started", &self.started)
            .finish_non_exhaustive()
    }
}

impl Default for LinuxSessionEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSessionEvents {
    /// Probes whether systemd-logind or `org.freedesktop.ScreenSaver` is reachable.
    #[must_use]
    pub fn new() -> Self {
        let supported = probe_session_backend();
        Self {
            supported,
            started: false,
            stop_tx: None,
            thread: None,
        }
    }
}

impl SessionEventSource for LinuxSessionEvents {
    fn is_supported(&self) -> bool {
        self.supported
    }

    fn start(&mut self, sink: Box<dyn Fn(SessionEvent) + Send + Sync>) {
        if self.started || !self.supported {
            return;
        }

        self.started = true;
        let sink: Sink = Arc::from(sink);
        let (stop_tx, stop_rx) = async_channel::bounded::<()>(1);
        let (ready_tx, ready_rx) = mpsc::channel::<bool>();

        let spawned = std::thread::Builder::new()
            .name("damt-linux-session".to_owned())
            .spawn(move || run_dbus_listener(sink, stop_rx, &ready_tx));

        match spawned {
            Ok(thread) => {
                self.stop_tx = Some(stop_tx);
                self.thread = Some(thread);
                if !ready_rx.recv().unwrap_or(false) {
                    self.supported = false;
                }
            }
            Err(_) => {
                self.started = false;
            }
        }
    }
}

impl Drop for LinuxSessionEvents {
    fn drop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.try_send(());
            stop_tx.close();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn probe_session_backend() -> bool {
    let has_logind = zbus::blocking::Connection::system()
        .ok()
        .is_some_and(|conn| dbus_name_has_owner(&conn, LOGIND_DEST));
    if has_logind {
        return true;
    }

    zbus::blocking::Connection::session()
        .ok()
        .is_some_and(|conn| dbus_name_has_owner(&conn, SCREENSAVER_IFACE))
}

fn dbus_name_has_owner(conn: &zbus::blocking::Connection, name: &str) -> bool {
    let Ok(reply) = conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "NameHasOwner",
        &(name,),
    ) else {
        return false;
    };
    reply.body().deserialize::<bool>().unwrap_or(false)
}

fn run_dbus_listener(
    sink: Sink,
    stop_rx: async_channel::Receiver<()>,
    ready_tx: &mpsc::Sender<bool>,
) {
    async_io::block_on(async move {
        let mut streams: Vec<MessageStream> = Vec::new();
        let mut expected_session_path: Option<String> = None;
        let mut has_logind_lock = false;

        if let Ok(sys_conn) = Connection::system().await
            && async_name_has_owner(&sys_conn, LOGIND_DEST).await
        {
            if let Ok(rule) = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(LOGIND_DEST)
                .and_then(|b| b.interface(LOGIND_MANAGER_IFACE))
                .and_then(|b| b.member("PrepareForSleep"))
                .map(zbus::match_rule::Builder::build)
                && let Ok(s) = MessageStream::for_match_rule(rule, &sys_conn, Some(16)).await
            {
                streams.push(s);
            }

            if let Ok(rule) = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(LOGIND_DEST)
                .and_then(|b| b.interface(LOGIND_MANAGER_IFACE))
                .and_then(|b| b.member("PrepareForShutdown"))
                .map(zbus::match_rule::Builder::build)
                && let Ok(s) = MessageStream::for_match_rule(rule, &sys_conn, Some(16)).await
            {
                streams.push(s);
            }

            expected_session_path = resolve_session_object_path(&sys_conn).await;
            if let Some(session_path) = expected_session_path.as_deref() {
                for member in ["Lock", "Unlock"] {
                    if let Ok(rule) = MatchRule::builder()
                        .msg_type(zbus::message::Type::Signal)
                        .sender(LOGIND_DEST)
                        .and_then(|b| b.path(session_path))
                        .and_then(|b| b.interface(LOGIND_SESSION_IFACE))
                        .and_then(|b| b.member(member))
                        .map(zbus::match_rule::Builder::build)
                        && let Ok(s) =
                            MessageStream::for_match_rule(rule, &sys_conn, Some(16)).await
                    {
                        streams.push(s);
                        has_logind_lock = true;
                    }
                }
            }
        }

        if !has_logind_lock
            && let Ok(sess_conn) = Connection::session().await
            && async_name_has_owner(&sess_conn, SCREENSAVER_IFACE).await
            && let Ok(rule) = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(SCREENSAVER_IFACE)
                .and_then(|b| b.member("ActiveChanged"))
                .map(zbus::match_rule::Builder::build)
            && let Ok(s) = MessageStream::for_match_rule(rule, &sess_conn, Some(16)).await
        {
            streams.push(s);
        }

        if streams.is_empty() {
            let _ = ready_tx.send(false);
            return;
        }
        let _ = ready_tx.send(true);

        let mut merged: Box<dyn futures_lite::Stream<Item = zbus::Result<zbus::Message>> + Unpin> =
            Box::new(streams.remove(0));
        for next_stream in streams {
            merged = Box::new(stream::or(merged, next_stream));
        }

        loop {
            let next_msg = future::or(
                async {
                    let _ = stop_rx.recv().await;
                    None
                },
                async { Some(merged.next().await) },
            )
            .await;

            let Some(item) = next_msg else {
                break;
            };
            let Some(msg_res) = item else {
                break;
            };
            let Ok(msg) = msg_res else {
                continue;
            };
            if let Some(event) = decode_message(&msg, expected_session_path.as_deref()) {
                sink(event);
            }
        }
    });
}

async fn async_name_has_owner(conn: &Connection, name: &str) -> bool {
    let Ok(reply) = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &(name,),
        )
        .await
    else {
        return false;
    };
    reply.body().deserialize::<bool>().unwrap_or(false)
}

async fn resolve_session_object_path(conn: &Connection) -> Option<String> {
    if let Some(path) = call_get_session(conn, "auto").await {
        return Some(path);
    }

    let xdg_id = std::env::var("XDG_SESSION_ID").ok();
    if let Some(id) = resolve_logind_session_id(xdg_id.as_deref())
        && let Some(path) = call_get_session(conn, id).await
    {
        return Some(path);
    }

    let reply = conn
        .call_method(
            Some(LOGIND_DEST),
            LOGIND_MANAGER_PATH,
            Some(LOGIND_MANAGER_IFACE),
            "GetSessionByPID",
            &(std::process::id(),),
        )
        .await
        .ok()?;
    reply
        .body()
        .deserialize::<OwnedObjectPath>()
        .ok()
        .map(|p| p.to_string())
}

async fn call_get_session(conn: &Connection, session_id: &str) -> Option<String> {
    let reply = conn
        .call_method(
            Some(LOGIND_DEST),
            LOGIND_MANAGER_PATH,
            Some(LOGIND_MANAGER_IFACE),
            "GetSession",
            &(session_id,),
        )
        .await
        .ok()?;
    reply
        .body()
        .deserialize::<OwnedObjectPath>()
        .ok()
        .map(|p| p.to_string())
}

fn decode_message(
    msg: &zbus::Message,
    expected_session_path: Option<&str>,
) -> Option<SessionEvent> {
    let header = msg.header();
    let interface = header.interface().map(zbus::names::InterfaceName::as_str)?;
    let member = header.member().map(zbus::names::MemberName::as_str)?;
    let path = header
        .path()
        .map(zbus::zvariant::ObjectPath::as_str)
        .unwrap_or("");
    let first_bool = msg.body().deserialize::<bool>().ok();
    let parsed = parse_raw_dbus_signal(interface, member, path, first_bool)?;
    map_dbus_signal(&parsed, expected_session_path)
}
