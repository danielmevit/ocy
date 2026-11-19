#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_platform::{FocusState, FocusStateProvider, IdleProvider, PlatformServices};

#[derive(Debug)]
pub struct FakeIdleProvider {
    supported: AtomicBool,
    idle_nanos: AtomicU64,
}

impl FakeIdleProvider {
    pub fn new() -> Self {
        Self {
            supported: AtomicBool::new(true),
            idle_nanos: AtomicU64::new(0),
        }
    }

    pub fn set_supported(&self, supported: bool) {
        self.supported.store(supported, Ordering::Relaxed);
    }

    pub fn set_idle_time(&self, duration: Duration) {
        let nanos = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
        self.idle_nanos.store(nanos, Ordering::Relaxed);
    }
}

impl IdleProvider for FakeIdleProvider {
    fn is_supported(&self) -> bool {
        self.supported.load(Ordering::Relaxed)
    }

    fn idle_time(&self) -> Duration {
        if !self.is_supported() {
            return Duration::ZERO;
        }
        Duration::from_nanos(self.idle_nanos.load(Ordering::Relaxed))
    }
}

#[derive(Debug)]
pub struct FakeFocusStateProvider {
    supported: AtomicBool,
    state: Mutex<FocusState>,
}

impl FakeFocusStateProvider {
    pub fn new() -> Self {
        Self {
            supported: AtomicBool::new(true),
            state: Mutex::new(FocusState::Normal),
        }
    }

    pub fn set_supported(&self, supported: bool) {
        self.supported.store(supported, Ordering::Relaxed);
    }

    pub fn set_state(&self, state: FocusState) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
    }
}

impl FocusStateProvider for FakeFocusStateProvider {
    fn is_supported(&self) -> bool {
        self.supported.load(Ordering::Relaxed)
    }

    fn state(&self) -> FocusState {
        if !self.is_supported() {
            return FocusState::Unknown;
        }
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Debug)]
pub struct TestPlatformServices {
    pub idle: FakeIdleProvider,
    pub focus: FakeFocusStateProvider,
}

impl TestPlatformServices {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            idle: FakeIdleProvider::new(),
            focus: FakeFocusStateProvider::new(),
        })
    }
}

impl PlatformServices for TestPlatformServices {
    fn idle(&self) -> &dyn IdleProvider {
        &self.idle
    }

    fn focus(&self) -> &dyn FocusStateProvider {
        &self.focus
    }
}

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("ocy-core-test-{prefix}-{pid}-{id}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
