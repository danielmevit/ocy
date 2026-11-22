//! Single-instance acquisition with a signal channel.
//!
//! Windows uses a named mutex `Local\<app_id>` and an auto-reset event `Local\<app_id>.signal`, exactly the
//! names the C# 0.1 app uses. For Ocy, `app_id` is `xyz.damt.ocy`, so the C# and Rust builds exclude each
//! other during the migration.
//!
//! Non-Windows targets use a lock file as the source of truth and a Unix domain socket only as the
//! signal channel. The socket is placed in `$XDG_RUNTIME_DIR` when available, otherwise in the app's
//! data directory. When that path would not fit in `sockaddr_un::sun_path` (104 bytes on macOS, 108 on
//! Linux, NUL included), it falls back to a short per-user path; a socket problem never blocks start-up.
//! The per-user `/tmp` fallback directory is verified with `symlink_metadata` (a real directory we
//! own, mode 0700) before any socket is bound or connected through it.
//!
//! On Windows the acquiring thread owns the mutex, so drop the guard on the same thread that acquired it.

#[cfg(not(windows))]
use std::path::Path;

/// Attempt to become the single running instance.
#[derive(Debug)]
pub enum Acquire {
    /// This process is the first instance; keep the guard for as long as the app runs.
    First(SingleInstanceGuard),
    /// Another instance owns the mutex or lock file. On Windows the running instance was signalled.
    AlreadyRunning,
}

/// The single-instance API. All methods are associated functions; the type is never instantiated.
#[derive(Debug, Default, Clone, Copy)]
pub struct SingleInstance;

impl SingleInstance {
    /// Try to acquire `app_id`.
    ///
    /// On Windows, when another instance is running, its `Local\<app_id>.signal` event is set before this
    /// returns [`Acquire::AlreadyRunning`].
    pub fn acquire(app_id: &str) -> Acquire {
        match platform::try_acquire(app_id) {
            Some(inner) => Acquire::First(SingleInstanceGuard { inner }),
            None => Acquire::AlreadyRunning,
        }
    }

    /// Acquire an instance using an explicit lock-file directory.
    ///
    /// This is the Unix path used by tests and embedders; on Windows use [`acquire`](Self::acquire).
    #[cfg(not(windows))]
    pub fn acquire_in_dir(data_dir: impl AsRef<Path>, app_id: &str) -> Acquire {
        match nix::try_acquire_in_dir(data_dir.as_ref(), app_id) {
            Some(inner) => Acquire::First(SingleInstanceGuard { inner }),
            None => Acquire::AlreadyRunning,
        }
    }
}

/// The owned first-instance guard.
///
/// Dropping it stops the signal listener (if any) and releases the mutex or lock file.
#[derive(Debug)]
pub struct SingleInstanceGuard {
    inner: platform::Guard,
}

impl SingleInstanceGuard {
    /// Wait for signals from later launches on a background thread and call `callback` for each one.
    ///
    /// The callback runs on that background thread; the app is responsible for marshalling to its UI thread.
    /// Calling this more than once is a no-op. On non-Windows targets this is a no-op when the
    /// signal socket could not be bound (the single-instance exclusion still holds).
    pub fn listen<F>(&mut self, callback: F)
    where
        F: Fn() + Send + 'static,
    {
        self.inner.listen(Box::new(callback));
    }
}

#[cfg(windows)]
use win as platform;

#[cfg(not(windows))]
use nix as platform;

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0,
    };
    use windows::Win32::System::Threading::{
        CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, ReleaseMutex,
        SetEvent, WaitForMultipleObjects,
    };
    use windows::core::PCWSTR;

    /// The owned Windows first-instance state.
    #[derive(Debug)]
    pub(super) struct Guard {
        mutex: HANDLE,
        signal: Option<HANDLE>,
        listener: Option<Listener>,
    }

    /// A background thread waiting for the signal event, plus the event that stops it.
    #[derive(Debug)]
    struct Listener {
        thread: Option<std::thread::JoinHandle<()>>,
        stop: HANDLE,
    }

    pub(super) fn try_acquire(app_id: &str) -> Option<Guard> {
        let mutex_name = wide(&format!("Local\\{app_id}"));
        let event_name = wide(&format!("Local\\{app_id}.signal"));

        // SAFETY: both names are null-terminated, live for the duration of the calls and are only read.
        let signal = unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) }.ok();

        // SAFETY: the name is null-terminated and lives for the call. `binitialowner = true` makes this
        // thread the owner when the mutex is created.
        let mutex = match unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr())) } {
            Ok(mutex) => mutex,
            Err(_) => {
                close(signal);
                return None;
            }
        };

        // SAFETY: GetLastError is thread-local; read it immediately after CreateMutexW.
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

        if already_exists {
            signal_running_instance(signal, &event_name);
            // SAFETY: the mutex handle is valid but this thread does not own it.
            unsafe {
                let _ = CloseHandle(mutex);
            }
            close(signal);
            return None;
        }

        let signal = signal.or_else(|| {
            // SAFETY: the name is null-terminated and lives for the call.
            unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) }.ok()
        });

        Some(Guard {
            mutex,
            signal,
            listener: None,
        })
    }

    impl Guard {
        pub(super) fn listen(&mut self, callback: Box<dyn Fn() + Send>) {
            if self.listener.is_some() {
                return;
            }

            let Some(signal) = self.signal else {
                return;
            };

            // SAFETY: a null security attribute and null name create an anonymous manual-reset event.
            let stop = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
                Ok(stop) => stop,
                Err(_) => return,
            };

            let signal_raw = signal.0 as isize;
            let stop_raw = stop.0 as isize;

            let thread = std::thread::Builder::new()
                .name(String::from("damt-single-instance"))
                .spawn(move || {
                    let signal = HANDLE(signal_raw as *mut c_void);
                    let stop = HANDLE(stop_raw as *mut c_void);

                    loop {
                        // SAFETY: both events stay valid until Guard::drop sets `stop` and joins this
                        // thread.
                        let result =
                            unsafe { WaitForMultipleObjects(&[signal, stop], false, INFINITE) };

                        if result == WAIT_OBJECT_0 {
                            callback();
                        } else {
                            break;
                        }
                    }
                });

            match thread {
                Ok(thread) => {
                    self.listener = Some(Listener {
                        thread: Some(thread),
                        stop,
                    });
                }
                Err(_) => close(Some(stop)),
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(mut listener) = self.listener.take() {
                // SAFETY: `stop` is an event owned by this listener.
                unsafe {
                    let _ = SetEvent(listener.stop);
                }
                if let Some(thread) = listener.thread.take() {
                    let _ = thread.join();
                }
                // SAFETY: the listener thread has joined and no longer uses the event.
                unsafe {
                    let _ = CloseHandle(listener.stop);
                }
            }

            close(self.signal.take());

            // SAFETY: both handles are valid and owned by this guard. ReleaseMutex is best effort.
            unsafe {
                let _ = ReleaseMutex(self.mutex);
                let _ = CloseHandle(self.mutex);
            }
        }
    }

    /// Signal the running instance, either through the handle we already opened or by retrying `OpenEventW`
    /// for up to one second (the C# `TryOpenExisting` loop).
    fn signal_running_instance(event: Option<HANDLE>, name: &[u16]) {
        if let Some(event) = event {
            // SAFETY: `event` is a valid event handle opened or created above.
            unsafe {
                let _ = SetEvent(event);
            }
            return;
        }

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            // SAFETY: `name` is null-terminated and lives for the call.
            let opened = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) };

            if let Ok(event) = opened {
                // SAFETY: `event` is a valid event handle opened above.
                unsafe {
                    let _ = SetEvent(event);
                    let _ = CloseHandle(event);
                }
                return;
            }

            if Instant::now() >= deadline {
                return;
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn close(handle: Option<HANDLE>) {
        if let Some(handle) = handle {
            // SAFETY: the handle is owned by the caller and closed exactly once.
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(not(windows))]
mod nix {
    use std::fs::{File, OpenOptions};
    use std::io::{Read as _, Write as _};
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::{self, JoinHandle};

    const SIGNAL_BYTE: u8 = 1;
    const STOP_BYTE: u8 = 0;

    /// The `sockaddr_un::sun_path` capacity in bytes, NUL included: 104 on macOS, 108 on Linux
    /// and the other Unix systems.
    #[cfg(target_os = "macos")]
    const SUN_PATH_LIMIT: usize = 104;
    #[cfg(not(target_os = "macos"))]
    const SUN_PATH_LIMIT: usize = 108;

    /// The owned Unix first-instance state; dropping the file releases the advisory lock.
    ///
    /// `socket_path` is `None` when the signal socket could not be bound: the lock file alone
    /// decides first versus already running, so a missing channel never blocks start-up.
    #[derive(Debug)]
    pub(super) struct Guard {
        _lock_file: File,
        socket_path: Option<PathBuf>,
        listener: Option<Listener>,
    }

    #[derive(Debug)]
    struct Listener {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Guard {
        pub(super) fn listen(&mut self, callback: Box<dyn Fn() + Send>) {
            if self.listener.is_some() {
                return;
            }

            let Some(socket_path) = self.socket_path.clone() else {
                return;
            };
            let _ = std::fs::remove_file(&socket_path);
            let Ok(listener) = UnixListener::bind(&socket_path) else {
                return;
            };
            let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);

            let thread = thread::Builder::new()
                .name(String::from("damt-single-instance"))
                .spawn(move || {
                    loop {
                        if let Ok((mut stream, _)) = listener.accept() {
                            let mut byte = [0_u8; 1];
                            if stream.read(&mut byte).ok().is_some_and(|read| read == 1)
                                && byte[0] == SIGNAL_BYTE
                                && !thread_stop.load(Ordering::Acquire)
                            {
                                callback();
                            }
                        }

                        if thread_stop.load(Ordering::Acquire) {
                            break;
                        }
                    }
                });

            match thread {
                Ok(thread) => {
                    self.listener = Some(Listener {
                        stop,
                        thread: Some(thread),
                    });
                }
                Err(_) => {
                    let _ = std::fs::remove_file(&socket_path);
                }
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(mut listener) = self.listener.take() {
                listener.stop.store(true, Ordering::Release);
                if let Some(socket_path) = self.socket_path.as_ref()
                    && let Ok(mut stream) = UnixStream::connect(socket_path)
                {
                    let _ = stream.write_all(&[STOP_BYTE]);
                }
                if let Some(thread) = listener.thread.take() {
                    let _ = thread.join();
                }
            }
            if let Some(socket_path) = self.socket_path.as_ref() {
                let _ = std::fs::remove_file(socket_path);
            }
        }
    }

    pub(super) fn try_acquire(app_id: &str) -> Option<Guard> {
        let app_name = app_name_from_id(app_id);
        let data_dir = crate::AppDataPaths::new(app_name).data_dir();
        let socket_dir = xdg_runtime_dir().unwrap_or_else(|| data_dir.clone());
        try_acquire_in_dirs(&data_dir, &socket_dir, app_id)
    }

    pub(super) fn try_acquire_in_dir(data_dir: &Path, app_id: &str) -> Option<Guard> {
        try_acquire_in_dirs(data_dir, data_dir, app_id)
    }

    fn try_acquire_in_dirs(data_dir: &Path, socket_dir: &Path, app_id: &str) -> Option<Guard> {
        try_acquire_with_xdg(data_dir, socket_dir, xdg_runtime_dir(), app_id)
    }

    /// The acquisition flow with an explicit `$XDG_RUNTIME_DIR`, so tests can force the per-user
    /// `/tmp` fallback without touching the process environment.
    fn try_acquire_with_xdg(
        data_dir: &Path,
        socket_dir: &Path,
        xdg_dir: Option<PathBuf>,
        app_id: &str,
    ) -> Option<Guard> {
        std::fs::create_dir_all(data_dir).ok()?;
        // The socket directory is best effort: a missing or unwritable one only disables the
        // signal channel, it must not make the app refuse to start.
        let _ = std::fs::create_dir_all(socket_dir);

        let path = data_dir.join(lock_file_name(app_id));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .ok()?;

        // The lock file is the source of truth: it alone decides First versus AlreadyRunning.
        let uid = file.metadata().map_or(0, |metadata| metadata.uid());

        if file.try_lock().is_err() {
            signal_socket(socket_dir, xdg_dir.as_deref(), uid, app_id);
            return None;
        }

        // We hold the lock, so no other instance is running and any socket at our path is stale:
        // always remove it and rebind. A bind failure only disables the signal channel.
        let socket_path = socket_path_in(socket_dir, xdg_dir.as_deref(), uid, app_id);
        let socket_path =
            if ensure_socket_dir(&socket_path, uid, app_id) && bind_socket(&socket_path) {
                Some(socket_path)
            } else {
                None
            };

        Some(Guard {
            _lock_file: file,
            socket_path,
            listener: None,
        })
    }

    fn signal_socket(socket_dir: &Path, xdg_dir: Option<&Path>, uid: u32, app_id: &str) {
        let socket_path = socket_path_in(socket_dir, xdg_dir, uid, app_id);
        if !ensure_socket_dir(&socket_path, uid, app_id) {
            return;
        }
        if let Ok(mut stream) = UnixStream::connect(socket_path) {
            let _ = stream.write_all(&[SIGNAL_BYTE]);
        }
    }

    /// `$XDG_RUNTIME_DIR` when set and non-empty.
    fn xdg_runtime_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    /// The pure socket-path rule; every instance computes the same result from the same inputs.
    fn socket_path_in(
        socket_dir: &Path,
        xdg_dir: Option<&Path>,
        uid: u32,
        app_id: &str,
    ) -> PathBuf {
        let direct = socket_dir.join(socket_file_name(app_id));
        if path_fits_sun_path(&direct) {
            return direct;
        }
        if let Some(xdg_dir) = xdg_dir {
            let candidate = xdg_dir.join(socket_file_name(app_id));
            if path_fits_sun_path(&candidate) {
                return candidate;
            }
        }
        per_user_socket_dir(uid, app_id).join(format!("{}.sock", hash8(&direct)))
    }

    fn socket_file_name(app_id: &str) -> String {
        format!("{app_id}.sock")
    }

    /// A path fits `sun_path` when its bytes plus the terminating NUL stay within the limit.
    fn path_fits_sun_path(path: &Path) -> bool {
        path.as_os_str().as_bytes().len() < SUN_PATH_LIMIT
    }

    /// The per-user 0700 fallback directory `/tmp/<app_id-prefix>-<uid>/`.
    fn per_user_socket_dir(uid: u32, app_id: &str) -> PathBuf {
        PathBuf::from("/tmp").join(format!("{}-{uid}", app_name_from_id(app_id)))
    }

    /// 32-bit FNV-1a over the path bytes, as eight hex characters.
    fn hash8(path: &Path) -> String {
        let mut hash: u32 = 0x811c_9dc5;
        for byte in path.as_os_str().as_bytes() {
            hash ^= u32::from(*byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        format!("{hash:08x}")
    }

    /// Make sure the socket's parent directory exists and is safe to bind or connect in.
    ///
    /// Only the per-user fallback directory is checked: it must be a real directory we own (see
    /// [`socket_dir_is_safe`]). `false` means the caller must not use the path — the signal channel
    /// stays off and start-up is unaffected.
    fn ensure_socket_dir(socket_path: &Path, uid: u32, app_id: &str) -> bool {
        let Some(parent) = socket_path.parent() else {
            return true;
        };
        // A failure here leaves the decision to the safety check: an existing directory is fine,
        // anything else (missing, a symlink, a file) is rejected below.
        let _ = std::fs::create_dir_all(parent);
        if parent != per_user_socket_dir(uid, app_id).as_path() {
            return true;
        }
        socket_dir_is_safe(parent, uid)
    }

    /// The per-user fallback directory is safe when `symlink_metadata` shows a real directory —
    /// a symlink is never accepted, even when it points somewhere plausible — owned by `uid`,
    /// with the mode repaired to 0700 only while we own it.
    fn socket_dir_is_safe(dir: &Path, uid: u32) -> bool {
        let Ok(metadata) = std::fs::symlink_metadata(dir) else {
            return false;
        };
        if !metadata.is_dir() || metadata.uid() != uid {
            return false;
        }
        if metadata.mode() & 0o777 == 0o700 {
            return true;
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).is_ok()
    }

    /// Remove any socket at `path` and bind a fresh one. `false` only means the signal channel
    /// is disabled; start-up is never blocked.
    fn bind_socket(path: &Path) -> bool {
        let _ = std::fs::remove_file(path);
        match UnixListener::bind(path) {
            Ok(listener) => {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
                drop(listener);
                true
            }
            Err(_) => false,
        }
    }

    /// The last dot-separated app id component, matching the C# Ocy data directory.
    fn app_name_from_id(app_id: &str) -> &str {
        app_id
            .rsplit('.')
            .next()
            .filter(|component| !component.is_empty())
            .unwrap_or(app_id)
    }

    fn lock_file_name(app_id: &str) -> String {
        format!(".{}.lock", app_name_from_id(app_id).to_lowercase())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn short_socket_dir_is_used_directly() {
            let dir = PathBuf::from("/tmp/ocy-data");
            assert_eq!(
                socket_path_in(&dir, None, 1000, "xyz.damt.ocy"),
                dir.join("xyz.damt.ocy.sock")
            );
        }

        #[test]
        fn the_last_fitting_path_is_direct_and_one_more_byte_falls_back() {
            // "<dir>/xyz.damt.ocy.sock" fits while its byte length stays under `SUN_PATH_LIMIT`.
            let fitting = PathBuf::from("/tmp").join("d".repeat(SUN_PATH_LIMIT - 24));
            assert_eq!(
                socket_path_in(&fitting, None, 1000, "xyz.damt.ocy"),
                fitting.join("xyz.damt.ocy.sock")
            );

            let too_long = PathBuf::from("/tmp").join("d".repeat(SUN_PATH_LIMIT - 23));
            assert_ne!(
                socket_path_in(&too_long, None, 1000, "xyz.damt.ocy"),
                too_long.join("xyz.damt.ocy.sock")
            );
        }

        #[test]
        fn long_dir_falls_back_to_xdg_runtime_dir() {
            let dir = PathBuf::from("/tmp").join("d".repeat(200));
            let xdg = PathBuf::from("/run/user/1000");
            assert_eq!(
                socket_path_in(&dir, Some(&xdg), 1000, "xyz.damt.ocy"),
                xdg.join("xyz.damt.ocy.sock")
            );
        }

        #[test]
        fn long_dir_without_xdg_uses_a_per_user_hashed_socket() {
            let dir = PathBuf::from("/tmp").join("d".repeat(200));
            let path = socket_path_in(&dir, None, 1000, "xyz.damt.ocy");

            assert_eq!(
                path.parent(),
                Some(per_user_socket_dir(1000, "xyz.damt.ocy").as_path())
            );
            let name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default();
            assert_eq!(
                name,
                format!("{}.sock", hash8(&dir.join("xyz.damt.ocy.sock")))
            );
            assert!(name.ends_with(".sock"));
            assert!(
                name.as_bytes()[..8]
                    .iter()
                    .all(|byte| byte.is_ascii_hexdigit())
            );
        }

        #[test]
        fn different_long_dirs_hash_to_different_sockets() {
            let one = PathBuf::from("/tmp").join("d".repeat(200));
            let two = PathBuf::from("/tmp").join("d".repeat(201));
            assert_ne!(
                socket_path_in(&one, None, 1000, "xyz.damt.ocy"),
                socket_path_in(&two, None, 1000, "xyz.damt.ocy")
            );
        }

        #[test]
        fn a_long_xdg_dir_also_falls_back_to_the_per_user_socket() {
            let dir = PathBuf::from("/tmp").join("d".repeat(200));
            let xdg = PathBuf::from("/tmp").join("x".repeat(300));
            assert_eq!(
                socket_path_in(&dir, Some(&xdg), 1000, "xyz.damt.ocy"),
                socket_path_in(&dir, None, 1000, "xyz.damt.ocy")
            );
        }

        #[test]
        fn per_user_dir_is_named_after_the_app_and_user() {
            assert_eq!(
                per_user_socket_dir(1000, "xyz.damt.ocy"),
                PathBuf::from("/tmp/ocy-1000")
            );
        }

        /// The owner of a file we just created is our effective uid, the same trick the
        /// acquisition flow uses with the lock file.
        fn test_uid() -> u32 {
            let dir = std::env::temp_dir().join(format!("damt-uid-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            let file = dir.join("uid");
            let _ = std::fs::write(&file, b"");
            let uid = std::fs::metadata(&file).map_or(0, |metadata| metadata.uid());
            let _ = std::fs::remove_file(&file);
            uid
        }

        /// A data dir whose socket path cannot fit in `sun_path`, forcing the per-user
        /// `/tmp` fallback (the `None` XDG dir keeps the test independent of the environment).
        fn long_data_dir() -> PathBuf {
            PathBuf::from("/tmp").join("l".repeat(120))
        }

        #[test]
        fn a_symlink_at_the_fallback_dir_is_never_bound_and_first_still_wins() {
            let uid = test_uid();
            let app_id = "xyz.damt.l2dsym";
            let fallback = per_user_socket_dir(uid, app_id);
            let target =
                std::env::temp_dir().join(format!("damt-sym-target-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&target);
            let _ = std::fs::remove_file(&fallback);
            assert!(
                std::os::unix::fs::symlink(&target, &fallback).is_ok(),
                "the test symlink should be created"
            );

            let dir = long_data_dir();
            let guard = match try_acquire_with_xdg(&dir, &dir, None, app_id) {
                Some(guard) => guard,
                None => panic!("the lock file alone decides: First must win"),
            };
            assert!(
                guard.socket_path.is_none(),
                "no socket may be bound through a symlinked fallback directory"
            );
            assert!(
                try_acquire_with_xdg(&dir, &dir, None, app_id).is_none(),
                "the second instance must not connect through the symlink either"
            );
            assert!(
                std::fs::read_dir(&target).map_or(0, Iterator::count) == 0,
                "nothing may be created through the symlink"
            );

            drop(guard);
            let _ = std::fs::remove_file(&fallback);
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::remove_dir_all(&target);
        }

        #[test]
        fn an_owned_fallback_dir_with_mode_0755_is_repaired_to_0700() {
            let uid = test_uid();
            let app_id = "xyz.damt.l2dmode";
            let fallback = per_user_socket_dir(uid, app_id);
            let _ = std::fs::remove_dir_all(&fallback);
            assert!(
                std::fs::create_dir(&fallback).is_ok(),
                "create the fallback dir"
            );
            assert!(
                std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o755)).is_ok(),
                "open the mode"
            );

            let dir = long_data_dir();
            let guard = match try_acquire_with_xdg(&dir, &dir, None, app_id) {
                Some(guard) => guard,
                None => panic!("the lock file alone decides: First must win"),
            };
            assert!(
                guard.socket_path.is_some(),
                "the repaired fallback directory must be used"
            );
            let mode = std::fs::metadata(&fallback)
                .map_or(0, |metadata| metadata.permissions().mode() & 0o777);
            assert_eq!(mode, 0o700);

            drop(guard);
            let _ = std::fs::remove_dir_all(&fallback);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
