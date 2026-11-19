use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_overlay::{MonitorId, MonitorInfo};
use damt_platform::{
    Clock, PlatformServices, ScreenRect, SessionEvent, TaskbarThemeProvider, Unsupported,
};
use ocy_app::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
use ocy_app::host::AppHost;
use ocy_app::layout::non_active_monitors;
use ocy_app::strings::Strings;
use ocy_app::tokens::{
    EDGE_FRAME_THICKNESS_DIP, attention_frame_color, edge_frame_color, overlay_dim_color,
};
use ocy_app::tray::{
    AppTheme, PauseSource, TrayController, TrayIconState, TrayPresentation, TrayPresenter, TraySink,
};
use ocy_core::engine::{BreakEngine, BreakInfo, EnginePhase, EngineSnapshot};
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

#[derive(Debug, Default)]
struct DummyBreakWindows {
    close_count: usize,
    short_count: usize,
    warning_count: usize,
    long_count: usize,
}

impl BreakWindows for DummyBreakWindows {
    fn show_short_break(&mut self, _info: &BreakInfo) {
        self.short_count += 1;
    }

    fn show_warning(&mut self, _countdown: Duration) {
        self.warning_count += 1;
    }

    fn show_long_break(&mut self, _info: &BreakInfo) {
        self.long_count += 1;
    }

    fn update_countdown(&mut self, _snapshot: &EngineSnapshot) {}

    fn close_all(&mut self) {
        self.close_count += 1;
    }
}

#[derive(Debug, Default)]
struct DummySettingsWindow {
    show_count: usize,
}

impl SettingsWindow for DummySettingsWindow {
    fn show(&mut self) {
        self.show_count += 1;
    }
}

#[derive(Debug, Default)]
struct DummyTraySink {
    last_state: Option<TrayIconState>,
    last_presentation: Option<TrayPresentation>,
}

impl TraySink for DummyTraySink {
    fn set_tooltip(&mut self, _tooltip: &str) {}
    fn set_icon(&mut self, state: TrayIconState, _use_light_set: bool) {
        self.last_state = Some(state);
    }
    fn set_menu(&mut self, presentation: &TrayPresentation) {
        self.last_presentation = Some(presentation.clone());
    }
}

#[derive(Debug)]
struct StaticTaskbarTheme(Option<bool>);

impl TaskbarThemeProvider for StaticTaskbarTheme {
    fn is_supported(&self) -> bool {
        true
    }
    fn is_light_taskbar(&self) -> Option<bool> {
        self.0
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-app-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn host_autosaves_every_60_ticks_and_on_shutdown() {
    let dir = temp_dir("autosave");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let store = AppDataStore::new(&dir, Arc::clone(&clock_dyn));
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine = BreakEngine::new(
        OcySettings::default(),
        platform,
        Arc::clone(&clock_dyn),
        Some(store),
        None,
        None,
        None,
    );
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme: Rc<dyn TaskbarThemeProvider> = Rc::new(StaticTaskbarTheme(Some(true)));
    let tray = TrayController::new(
        presenter,
        DummyTraySink::default(),
        theme,
        clock_dyn,
        Some(AppTheme::Light),
        &snapshot,
    );
    let coordinator =
        BreakUiCoordinator::new(DummyBreakWindows::default(), DummySettingsWindow::default());
    let error_log = damt_app::ErrorLog::new(&dir, "ocy");

    let mut host = AppHost::new(engine, coordinator, tray, error_log);

    // 59 ticks should not have written state.json yet
    for _ in 0..59 {
        clock.advance(Duration::from_secs(1));
        host.on_tick();
    }
    assert!(!dir.join("state.json").exists());

    // 60th tick triggers autosave
    clock.advance(Duration::from_secs(1));
    host.on_tick();
    assert!(dir.join("state.json").exists());

    let saved_at_60 = AppDataStore::with_system_clock(&dir).load_state();
    assert_eq!(saved_at_60.since_short_break, Duration::from_secs(60));

    // Advance 5 more seconds, then shutdown -> saves immediately
    for _ in 0..5 {
        clock.advance(Duration::from_secs(1));
        host.on_tick();
    }
    host.shutdown();

    let saved_on_quit = AppDataStore::with_system_clock(&dir).load_state();
    assert_eq!(saved_on_quit.since_short_break, Duration::from_secs(65));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn host_forwards_session_events_and_closes_overlays_on_lock() {
    let dir = temp_dir("session");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine =
        BreakEngine::with_defaults(OcySettings::default(), platform, Arc::clone(&clock_dyn));
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme: Rc<dyn TaskbarThemeProvider> = Rc::new(StaticTaskbarTheme(Some(false)));
    let tray = TrayController::new(
        presenter,
        DummyTraySink::default(),
        theme,
        clock_dyn,
        Some(AppTheme::Dark),
        &snapshot,
    );
    let coordinator =
        BreakUiCoordinator::new(DummyBreakWindows::default(), DummySettingsWindow::default());
    let error_log = damt_app::ErrorLog::new(&dir, "ocy");

    let mut host = AppHost::new(engine, coordinator, tray, error_log);
    let close_before = host.coordinator().break_windows().close_count;

    host.on_session_event(SessionEvent::Locked);
    assert_eq!(host.engine().phase(), EnginePhase::Away);
    assert!(host.coordinator().break_windows().close_count > close_before);

    host.on_session_event(SessionEvent::Unlocked);
    assert_eq!(host.engine().phase(), EnginePhase::Working);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_active_monitors_filters_selected_monitor() {
    let monitors = [
        MonitorInfo {
            id: MonitorId::new(1),
            rect: ScreenRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            work_area: ScreenRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
            dpi: 96,
            primary: true,
        },
        MonitorInfo {
            id: MonitorId::new(2),
            rect: ScreenRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1200,
            },
            work_area: ScreenRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1200,
            },
            dpi: 96,
            primary: false,
        },
    ];

    let active = ScreenRect {
        x: -1800,
        y: 100,
        width: 800,
        height: 600,
    };
    let others = non_active_monitors(&monitors, Some(active));
    assert_eq!(others.len(), 1);
    assert_eq!(others[0].id, MonitorId::new(1));
}

#[test]
fn overlay_tokens_match_design_system_values() {
    assert_eq!(EDGE_FRAME_THICKNESS_DIP, 3);
    assert_eq!(
        edge_frame_color(false),
        damt_overlay::Rgba::new(0x2F, 0x7D, 0x62, 0x8C)
    );
    assert_eq!(
        attention_frame_color(false),
        damt_overlay::Rgba::new(0xB0, 0x4E, 0x2C, 0x8C)
    );
    assert_eq!(
        overlay_dim_color(false),
        damt_overlay::Rgba::new(0x00, 0x00, 0x00, 0xC7)
    );
}

#[test]
fn host_tray_left_click_shows_and_reactivates_settings_and_pause_from_tray_tracks_state() {
    let dir = temp_dir("tray-click-pause");
    let clock = Arc::new(FakeClock::from_fixed_offset(2026, 11, 19, 14, 0, 0, 1).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine =
        BreakEngine::with_defaults(OcySettings::default(), platform, Arc::clone(&clock_dyn));
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme: Rc<dyn TaskbarThemeProvider> = Rc::new(StaticTaskbarTheme(Some(false)));
    let tray = TrayController::new(
        presenter,
        DummyTraySink::default(),
        theme,
        clock_dyn,
        Some(AppTheme::Dark),
        &snapshot,
    );
    let coordinator =
        BreakUiCoordinator::new(DummyBreakWindows::default(), DummySettingsWindow::default());
    let error_log = damt_app::ErrorLog::new(&dir, "ocy");

    let mut host = AppHost::new(engine, coordinator, tray, error_log);
    assert_eq!(host.coordinator().settings_window().show_count, 0);

    // Left-click opens Settings; a second left-click restores/focuses it via the same show path.
    host.on_tray_left_click();
    assert_eq!(host.coordinator().settings_window().show_count, 1);
    host.on_tray_left_click();
    assert_eq!(host.coordinator().settings_window().show_count, 2);

    // Pause for 1 hour via tray seam -> status visible, 1 h item checked.
    host.pause_from_tray(PauseSource::OneHour);
    let pres_1h = host.tray().sink().last_presentation.as_ref().unwrap();
    assert!(pres_1h.pause_status.is_visible);
    assert!(!pres_1h.pause_status.is_enabled);
    assert_eq!(
        pres_1h.pause_status.text,
        "Paused until 15:00 \u{00b7} 1 h 00 min left"
    );
    assert!(pres_1h.pause_1_hour.is_checked);
    assert!(!pres_1h.pause_3_hours.is_checked);
    assert!(!pres_1h.pause_until_resumed.is_checked);

    // Pause expires on tick -> status hidden, checkmarks cleared.
    clock.advance(Duration::from_secs(3600));
    host.on_tick();
    assert_eq!(host.engine().phase(), EnginePhase::Working);
    let pres_after = host.tray().sink().last_presentation.as_ref().unwrap();
    assert!(!pres_after.pause_status.is_visible);
    assert!(!pres_after.pause_1_hour.is_checked);
    assert!(!pres_after.pause_3_hours.is_checked);
    assert!(!pres_after.pause_until_resumed.is_checked);

    // Pause until resumed, then explicit resume -> clears checkmark & status.
    host.pause_from_tray(PauseSource::UntilResumed);
    let pres_indef = host.tray().sink().last_presentation.as_ref().unwrap();
    assert!(pres_indef.pause_status.is_visible);
    assert_eq!(pres_indef.pause_status.text, "Paused until you resume");
    assert!(pres_indef.pause_until_resumed.is_checked);

    host.resume();
    let pres_resumed = host.tray().sink().last_presentation.as_ref().unwrap();
    assert!(!pres_resumed.pause_status.is_visible);
    assert!(!pres_resumed.pause_until_resumed.is_checked);

    let _ = fs::remove_dir_all(&dir);
}
