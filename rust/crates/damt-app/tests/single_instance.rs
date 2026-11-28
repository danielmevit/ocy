//! Unix single-instance tests: the lock-file rule, the socket signal channel and the
//! short-path fallback.

#![cfg(not(windows))]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use damt_app::{Acquire, SingleInstance};

/// A fresh directory explicitly under `/tmp`: the socket path must stay short on every machine,
/// and the system temp dir is longer than `sun_path` on macOS.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = Path::new("/tmp").join(format!("damt-app-{name}-{}-{nanos}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn second_acquire_is_already_running_and_drop_releases() {
    let dir = temp_dir("single-instance");
    let app_id = "xyz.damt.ocy";

    let first = SingleInstance::acquire_in_dir(&dir, app_id);
    let guard = match first {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => {
            let _ = fs::remove_dir_all(&dir);
            panic!("the first acquire should win");
        }
    };

    assert!(
        dir.join(".ocy.lock").exists(),
        "the lock file should use the C# Ocy name"
    );
    assert!(matches!(
        SingleInstance::acquire_in_dir(&dir, app_id),
        Acquire::AlreadyRunning
    ));

    drop(guard);

    assert!(matches!(
        SingleInstance::acquire_in_dir(&dir, app_id),
        Acquire::First(_)
    ));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn different_app_ids_do_not_exclude_each_other() {
    let dir = temp_dir("single-instance-apps");

    let first = SingleInstance::acquire_in_dir(&dir, "xyz.damt.ocy.one");
    let second = SingleInstance::acquire_in_dir(&dir, "xyz.damt.ocy.two");

    assert!(matches!(first, Acquire::First(_)));
    assert!(matches!(second, Acquire::First(_)));

    drop(first);
    drop(second);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn first_and_second_instances_use_socket_signal() {
    let dir = temp_dir("socket-signal");
    let app_id = "xyz.damt.ocy.socket";

    let mut first = match SingleInstance::acquire_in_dir(&dir, app_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => panic!("the first acquire should win"),
    };
    let (tx, rx) = mpsc::channel();
    first.listen(move || {
        let _ = tx.send(());
    });

    assert!(matches!(
        SingleInstance::acquire_in_dir(&dir, app_id),
        Acquire::AlreadyRunning
    ));
    rx.recv_timeout(Duration::from_secs(2))
        .expect("the first instance should receive the signal");
    assert!(
        rx.recv_timeout(Duration::from_millis(100)).is_err(),
        "a real second-instance signal should call the callback exactly once"
    );
    let mode = fs::metadata(dir.join(format!("{app_id}.sock")))
        .expect("socket should exist")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);

    drop(first);
    assert!(
        rx.recv_timeout(Duration::from_millis(100)).is_err(),
        "the drop wake-up connection must not call the callback"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dropping_the_guard_ends_the_listener_promptly() {
    let dir = temp_dir("socket-drop");
    let app_id = "xyz.damt.ocy.drop";
    let mut first = match SingleInstance::acquire_in_dir(&dir, app_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => panic!("the first acquire should win"),
    };
    first.listen(|| {});

    let (tx, rx) = mpsc::channel();
    let drop_thread = thread::spawn(move || {
        drop(first);
        let _ = tx.send(());
    });

    rx.recv_timeout(Duration::from_secs(1))
        .expect("dropping the guard should end and join the listener promptly");
    drop_thread.join().expect("drop thread should exit");
    assert!(!dir.join(format!("{app_id}.sock")).exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stale_socket_is_removed_before_rebind() {
    let dir = temp_dir("stale-socket");
    let app_id = "xyz.damt.ocy.stale";
    let socket_path = dir.join(format!("{app_id}.sock"));
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(&socket_path, b"stale").expect("create stale socket file");

    let mut first = match SingleInstance::acquire_in_dir(&dir, app_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => panic!("stale socket should be removed"),
    };
    first.listen(|| {});

    assert!(socket_path.exists());
    drop(first);
    assert!(!socket_path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn responsive_stale_socket_is_replaced_while_the_lock_is_held() {
    let dir = temp_dir("stale-live-socket");
    let app_id = "xyz.damt.ocy.live";
    let socket_path = dir.join(format!("{app_id}.sock"));
    fs::create_dir_all(&dir).expect("create temp dir");
    let stale = UnixListener::bind(&socket_path).expect("bind the stale socket");

    let mut first = match SingleInstance::acquire_in_dir(&dir, app_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => {
            panic!("holding the lock must replace a stale responsive socket")
        }
    };
    let (tx, rx) = mpsc::channel();
    first.listen(move || {
        let _ = tx.send(());
    });

    assert!(matches!(
        SingleInstance::acquire_in_dir(&dir, app_id),
        Acquire::AlreadyRunning
    ));
    rx.recv_timeout(Duration::from_secs(2))
        .expect("the replaced socket should deliver the second-instance signal");

    drop(first);
    drop(stale);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fallback_lock_dir_accepts_socket_when_runtime_dir_is_missing() {
    let dir = temp_dir("missing-runtime");
    let app_id = "xyz.damt.ocy.fallback";
    let mut first = match SingleInstance::acquire_in_dir(&dir, app_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => panic!("fallback acquire should win"),
    };
    first.listen(|| {});

    let socket_path = dir.join(format!("{app_id}.sock"));
    assert!(UnixStream::connect(&socket_path).is_ok());
    drop(first);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn long_data_dir_uses_the_short_fallback_and_still_signals() {
    // A data dir longer than the largest `sun_path` (108 on Linux, 104 on macOS), built under
    // `/tmp` so the test does not depend on the machine's temp dir length.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = Path::new("/tmp").join(format!("damt-app-long-{}-{nanos}", std::process::id()));
    let dir = root.join("l".repeat(120));
    let app_id = "xyz.damt.ocy.long";

    let mut first = match SingleInstance::acquire_in_dir(&dir, app_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => {
            panic!("the first acquire should win with a too-long socket path")
        }
    };
    let (tx, rx) = mpsc::channel();
    first.listen(move || {
        let _ = tx.send(());
    });

    assert!(matches!(
        SingleInstance::acquire_in_dir(&dir, app_id),
        Acquire::AlreadyRunning
    ));
    rx.recv_timeout(Duration::from_secs(2))
        .expect("the second instance should signal through the short fallback path");
    assert!(
        !dir.join(format!("{app_id}.sock")).exists(),
        "no socket may be created under the too-long data dir"
    );

    drop(first);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn socket_bind_failure_disables_the_channel_but_not_start_up() {
    let dir = temp_dir("socket-bind-failure");
    let app_id = "xyz.damt.ocy.bindfail";
    fs::create_dir_all(&dir).expect("create temp dir");
    // A directory where the socket should be makes every bind fail.
    fs::create_dir_all(dir.join(format!("{app_id}.sock"))).expect("create the blocking directory");

    let mut first = match SingleInstance::acquire_in_dir(&dir, app_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => panic!("a socket problem must not block start-up"),
    };
    first.listen(|| {});

    assert!(
        matches!(
            SingleInstance::acquire_in_dir(&dir, app_id),
            Acquire::AlreadyRunning
        ),
        "the lock file alone decides, even without a signal channel"
    );

    drop(first);
    let _ = fs::remove_dir_all(&dir);
}
