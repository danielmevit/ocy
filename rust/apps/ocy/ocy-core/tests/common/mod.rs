#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_platform::{
    ControllerIdleProvider, FocusState, FocusStateProvider, ForegroundObservation, ForegroundProbe,
    IdleProvider, PlatformServices,
};

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

/// Scripted controller-activity provider: reports a fixed idle time.
#[derive(Debug)]
pub struct FakeControllerIdle {
    supported: AtomicBool,
    idle_nanos: AtomicU64,
}

impl FakeControllerIdle {
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

impl ControllerIdleProvider for FakeControllerIdle {
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

/// Scripted foreground probe. Unsupported until a test stores an observation.
#[derive(Debug)]
pub struct FakeForeground {
    supported: AtomicBool,
    observation: Mutex<ForegroundObservation>,
    paths: Mutex<Vec<String>>,
    dead_pids: Mutex<Vec<u32>>,
}

impl FakeForeground {
    pub fn new() -> Self {
        Self {
            supported: AtomicBool::new(false),
            observation: Mutex::new(ForegroundObservation::unknown()),
            paths: Mutex::new(Vec::new()),
            dead_pids: Mutex::new(Vec::new()),
        }
    }

    pub fn set_observation(&self, observation: ForegroundObservation) {
        self.supported.store(true, Ordering::Relaxed);
        *self.observation.lock().unwrap_or_else(|e| e.into_inner()) = observation;
    }

    pub fn set_game_bar_paths(&self, paths: Vec<String>) {
        self.supported.store(true, Ordering::Relaxed);
        *self.paths.lock().unwrap_or_else(|e| e.into_inner()) = paths;
    }

    pub fn mark_dead(&self, pid: u32) {
        self.dead_pids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(pid);
    }
}

impl ForegroundProbe for FakeForeground {
    fn is_supported(&self) -> bool {
        self.supported.load(Ordering::Relaxed)
    }

    fn observe(&self) -> ForegroundObservation {
        if !self.is_supported() {
            return ForegroundObservation::unknown();
        }
        self.observation
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn game_bar_exe_paths(&self) -> Vec<String> {
        if !self.is_supported() {
            return Vec::new();
        }
        self.paths.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn process_alive(&self, process_id: u32) -> bool {
        if !self.is_supported() || process_id == 0 {
            return false;
        }
        !self
            .dead_pids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&process_id)
    }
}

#[derive(Debug)]
pub struct TestPlatformServices {
    pub idle: FakeIdleProvider,
    pub focus: FakeFocusStateProvider,
    pub foreground: FakeForeground,
    pub controller: FakeControllerIdle,
}

impl TestPlatformServices {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            idle: FakeIdleProvider::new(),
            focus: FakeFocusStateProvider::new(),
            foreground: FakeForeground::new(),
            controller: FakeControllerIdle::new(),
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

    fn foreground(&self) -> &dyn ForegroundProbe {
        &self.foreground
    }

    fn controller(&self) -> &dyn ControllerIdleProvider {
        &self.controller
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
