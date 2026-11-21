//! Headless Slint integration tests for the real `SlintSettingsWindow` (`ocy_ui::SettingsWindowV2`).

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_app::ErrorLog;
use damt_platform::{
    AppThemeProvider, AutostartService, Clock, PlatformError, PlatformServices,
    TaskbarThemeProvider,
};
use ocy_app::STATUS_NOTIFICATIONS_SHIPPED;
use ocy_app::break_logic::parse_cli_args;
use ocy_app::coordinator::{BreakUiCoordinator, BreakWindows};
use ocy_app::host::AppHost;
use ocy_app::language_choice;
use ocy_app::presets::Preset;
use ocy_app::settings_logic::{SettingsCallbacks, default_app_settings};
use ocy_app::slint_settings::SlintSettingsWindow;
use ocy_app::strings::{Strings, parse_demo_language};
use ocy_app::tray::{
    AppTheme, TrayController, TrayIconState, TrayPresentation, TrayPresenter, TraySink,
};
use ocy_core::engine::{BreakEngine, BreakInfo, EngineSnapshot};
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;
use slint::Model;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError as SlintPlatformError, WindowAdapter};

struct MinimalHeadlessPlatform;

impl Platform for MinimalHeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, SlintPlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
    }

    fn duration_since_start(&self) -> Duration {
        Duration::ZERO
    }
}

fn init_headless() {
    let _ = slint::platform::set_platform(Box::new(MinimalHeadlessPlatform));
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-v2-window-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[derive(Debug)]
struct FakeAutostart(bool);

impl AutostartService for FakeAutostart {
    fn is_supported(&self) -> bool {
        self.0
    }

    fn is_enabled(&self) -> bool {
        false
    }

    fn set_enabled(&self, _enabled: bool) -> Result<(), PlatformError> {
        Ok(())
    }
}

#[derive(Debug)]
struct ToggleThemePlatform {
    autostart: FakeAutostart,
    light_app_theme: Mutex<Option<bool>>,
}

impl ToggleThemePlatform {
    fn new(autostart_supported: bool, light_theme: bool) -> Arc<Self> {
        Arc::new(Self {
            autostart: FakeAutostart(autostart_supported),
            light_app_theme: Mutex::new(Some(light_theme)),
        })
    }

    fn set_light(&self, light: bool) {
        *self.light_app_theme.lock().unwrap() = Some(light);
    }
}

impl AppThemeProvider for ToggleThemePlatform {
    fn is_supported(&self) -> bool {
        true
    }

    fn is_light_app_theme(&self) -> Option<bool> {
        *self.light_app_theme.lock().unwrap()
    }
}

impl PlatformServices for ToggleThemePlatform {
    fn autostart(&self) -> &dyn AutostartService {
        &self.autostart
    }

    fn app_theme(&self) -> &dyn AppThemeProvider {
        self
    }
}

#[derive(Debug, Default)]
struct DummyBreakWindows;
impl BreakWindows for DummyBreakWindows {
    fn show_short_break(&mut self, _info: &BreakInfo) {}
    fn show_warning(&mut self, _countdown: Duration) {}
    fn show_long_break(&mut self, _info: &BreakInfo) {}
    fn update_countdown(&mut self, _snapshot: &EngineSnapshot) {}
    fn close_all(&mut self) {}
}

#[derive(Debug, Default)]
struct DummyTraySink;
impl TraySink for DummyTraySink {
    fn set_tooltip(&mut self, _tooltip: &str) {}
    fn set_icon(&mut self, _state: TrayIconState, _use_light_set: bool) {}
    fn set_menu(&mut self, _presentation: &TrayPresentation) {}
}

#[derive(Debug)]
struct StaticTaskbarTheme;
impl TaskbarThemeProvider for StaticTaskbarTheme {
    fn is_supported(&self) -> bool {
        true
    }
    fn is_light_taskbar(&self) -> Option<bool> {
        Some(true)
    }
}

type TestHost = AppHost<DummyBreakWindows, SlintSettingsWindow, DummyTraySink>;

fn make_host(
    dir: &std::path::Path,
    settings: OcySettings,
    platform: Arc<ToggleThemePlatform>,
) -> Rc<RefCell<TestHost>> {
    let clock =
        Arc::new(FakeClock::from_fixed_offset(2026, 11, 20, 15, 0, 0, 0).expect("valid clock"));
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let error_log = ErrorLog::new(dir, "ocy");

    let engine = BreakEngine::new(
        settings.clone(),
        platform.clone(),
        Arc::clone(&clock_dyn),
        None,
        None,
        None,
        None,
    );
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_language(&settings.language));
    let tray = TrayController::new(
        TrayPresenter::new(Arc::clone(&strings), Arc::clone(&clock_dyn)),
        DummyTraySink,
        Rc::new(StaticTaskbarTheme),
        Arc::clone(&clock_dyn),
        Some(AppTheme::Light),
        &snapshot,
    );

    let host_slot: Rc<RefCell<Option<Rc<RefCell<TestHost>>>>> = Rc::new(RefCell::new(None));
    let slot_save = Rc::clone(&host_slot);
    let settings_window = SlintSettingsWindow::new(
        platform,
        clock_dyn,
        strings,
        error_log.clone(),
        SettingsCallbacks {
            on_save: Rc::new(move |updated| {
                slot_save
                    .borrow()
                    .as_ref()
                    .is_some_and(|h| h.borrow_mut().save_settings(updated))
            }),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        settings,
    );
    let coordinator = BreakUiCoordinator::new(DummyBreakWindows, settings_window);
    let store = AppDataStore::new(
        dir,
        Arc::new(FakeClock::from_utc(2026, 11, 20, 15, 0, 0).unwrap()),
    );
    let host = Rc::new(RefCell::new(AppHost::with_store(
        engine,
        coordinator,
        tray,
        error_log,
        Some(store),
    )));
    *host_slot.borrow_mut() = Some(Rc::clone(&host));
    host
}

#[test]
fn headless_v2_presets_round_trip_and_hand_edit_switches_to_custom() {
    init_headless();
    let dir = temp_dir("presets-custom");
    let platform = ToggleThemePlatform::new(true, true);
    let initial = OcySettings {
        language: "en".to_owned(),
        ..OcySettings::default()
    };
    let host = make_host(&dir, initial, platform);

    host.borrow_mut().show_settings();
    let ui = host
        .borrow()
        .coordinator()
        .settings_window()
        .active_window()
        .expect("V2 window open");

    // Defaults detect as Balanced (1), mascot is rendered, hidden rows obey gates, formatters work.
    assert_eq!(ui.get_preset(), Preset::Balanced.index());
    assert!(!ui.get_show_sounds());
    assert_eq!(ui.get_show_notifications(), STATUS_NOTIFICATIONS_SHIPPED);
    assert!(ui.get_show_start_with_system());
    assert!(ui.get_mascot().size().width > 0);
    assert_eq!(
        ui.get_next_break_text().as_str(),
        "Next short break in 25 min"
    );
    assert_eq!(
        ui.invoke_format_every_minutes(ui.get_short_interval())
            .as_str(),
        "every 25 min"
    );
    assert_eq!(
        ui.invoke_format_minutes(ui.get_long_duration()).as_str(),
        "5 min"
    );
    assert_eq!(
        ui.invoke_format_seconds(ui.get_short_duration()).as_str(),
        "15 s"
    );
    assert_eq!(
        ui.invoke_format_times(ui.get_long_max_postpones()).as_str(),
        "3 times"
    );
    assert_eq!(
        ui.invoke_format_duration(ui.get_away_threshold()).as_str(),
        "2 min"
    );
    assert_eq!(ui.get_timeline_labels().row_count(), 4);
    assert_eq!(
        ui.get_timeline_labels().row_data(0).as_deref(),
        Some("15:30")
    );

    // Selecting Gentle (0) updates sliders and timeline marks immediately.
    let initial_marks = ui.get_timeline().row_count();
    ui.invoke_preset_selected(Preset::Gentle.index());
    assert_eq!(ui.get_preset(), Preset::Gentle.index());
    assert_eq!(ui.get_short_interval(), 40);
    assert_eq!(ui.get_short_duration(), 15);
    assert_eq!(ui.get_long_interval(), 90);
    assert_eq!(ui.get_long_duration(), 5);
    assert!(ui.get_timeline().row_count() < initial_marks);

    // Selecting Focused (2) updates sliders and preset state.
    ui.invoke_preset_selected(Preset::Focused.index());
    assert_eq!(ui.get_preset(), Preset::Focused.index());
    assert_eq!(ui.get_short_interval(), 50);
    assert_eq!(ui.get_short_duration(), 20);
    assert_eq!(ui.get_long_interval(), 90);
    assert_eq!(ui.get_long_duration(), 10);

    // Hand-edit short_interval -> preset switches to Custom (3) and timeline updates immediately.
    let focused_marks = ui.get_timeline().row_count();
    ui.set_short_interval(12);
    ui.invoke_changed();
    assert_eq!(ui.get_preset(), Preset::Custom.index());
    assert_eq!(
        ui.invoke_format_every_minutes(ui.get_short_interval())
            .as_str(),
        "every 12 min"
    );
    assert!(ui.get_timeline().row_count() > focused_marks);

    // Hand-editing back to Balanced's four values detects Balanced (1) again.
    ui.set_short_interval(25);
    ui.set_short_duration(15);
    ui.set_long_interval(60);
    ui.set_long_duration(5);
    ui.invoke_changed();
    assert_eq!(ui.get_preset(), Preset::Balanced.index());

    // Closing via Esc flushes pending save to AppHost.
    ui.invoke_preset_selected(Preset::Gentle.index());
    ui.invoke_escape_pressed();
    assert_eq!(
        host.borrow()
            .engine()
            .settings()
            .short_breaks
            .interval_minutes,
        40
    );

    drop(ui);
    host.borrow_mut().shutdown();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn headless_v2_language_switch_updates_texts_slider_formatters_and_font_family() {
    init_headless();
    let dir = temp_dir("lang-switch");
    let platform = ToggleThemePlatform::new(false, true);
    let initial = OcySettings {
        language: "en".to_owned(),
        ..OcySettings::default()
    };
    let host = make_host(&dir, initial, Arc::clone(&platform));

    host.borrow_mut().show_settings();
    let ui = host
        .borrow()
        .coordinator()
        .settings_window()
        .active_window()
        .expect("V2 window open");

    // Autostart unsupported -> row hidden.
    assert!(!ui.get_show_start_with_system());
    assert_eq!(ui.get_font_family().as_str(), "Segoe UI");
    assert_eq!(ui.get_window_title().as_str(), "Ocy \u{2014} Settings");

    // Open Customize -> Breaks so sliders are active during live language switch.
    ui.set_customize_open(true);
    ui.set_customize_tab(0);
    assert_eq!(
        ui.invoke_format_every_minutes(ui.get_short_interval())
            .as_str(),
        "every 25 min"
    );

    // Switch to German via the V2 language-selected callback.
    let de_idx = language_choice::index_for_i32("de", None);
    ui.set_language_index(de_idx);
    ui.invoke_language_selected(de_idx);

    assert_eq!(host.borrow().engine().settings().language, "de");
    assert_eq!(ui.get_font_family().as_str(), "Segoe UI");
    assert_eq!(ui.get_window_title().as_str(), "Ocy \u{2014} Einstellungen");
    assert_eq!(ui.get_customize_title().as_str(), "Anpassen");
    assert_eq!(ui.get_general_title().as_str(), "Allgemein");
    assert_eq!(
        ui.get_next_break_text().as_str(),
        "Nächste kurze Pause in 25 Min."
    );
    assert_eq!(
        ui.get_language_model().row_data(0).as_deref(),
        Some("Automatisch (Windows-Sprache)")
    );
    assert_eq!(ui.get_language_index(), de_idx);
    assert_eq!(
        ui.invoke_format_every_minutes(ui.get_short_interval())
            .as_str(),
        "alle 25 Min."
    );

    // Switch to Japanese -> font-family changes to Yu Gothic UI and all texts switch.
    let ja_idx = language_choice::index_for_i32("ja", None);
    ui.set_language_index(ja_idx);
    ui.invoke_language_selected(ja_idx);

    assert_eq!(host.borrow().engine().settings().language, "ja");
    assert_eq!(ui.get_font_family().as_str(), "Yu Gothic UI");
    assert_eq!(ui.get_window_title().as_str(), "Ocy \u{2014} 設定");
    assert_eq!(ui.get_today_title().as_str(), "今日");
    assert_eq!(
        ui.invoke_format_times(ui.get_long_max_postpones()).as_str(),
        "3回"
    );

    // Flipping app theme while open re-renders mascot.
    platform.set_light(false);
    host.borrow_mut().on_tick();
    assert!(ui.get_mascot().size().width > 0);

    drop(ui);
    host.borrow_mut().shutdown();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn headless_v2_today_counts_and_demo_language_flag_work_end_to_end() {
    init_headless();
    let dir = temp_dir("today-demo");
    let raw_args = vec![
        "--demo-settings".to_owned(),
        "--demo-language".to_owned(),
        "cs".to_owned(),
    ];
    let cli = parse_cli_args(raw_args.clone());
    assert!(cli.demo_settings);
    let demo_lang = parse_demo_language(&raw_args).expect("demo language");

    let mut settings = default_app_settings();
    settings.language = demo_lang;

    let platform = ToggleThemePlatform::new(true, true);
    let host = make_host(&dir, settings, platform);

    // Skip 1 short break before opening Settings so Today card reflects live stats.
    assert!(host.borrow_mut().start_short_break_now());
    assert!(host.borrow_mut().skip_current_break());
    host.borrow_mut().show_settings();

    let ui = host
        .borrow()
        .coordinator()
        .settings_window()
        .active_window()
        .expect("V2 window open");

    assert_eq!(ui.get_today_skipped(), 1);
    assert_eq!(
        ui.get_language_index(),
        language_choice::index_for_i32("cs", None)
    );
    assert_eq!(ui.get_window_title().as_str(), "Ocy \u{2014} Nastavení");
    assert_eq!(ui.get_font_family().as_str(), "Segoe UI");

    drop(ui);
    host.borrow_mut().shutdown();
    let _ = fs::remove_dir_all(&dir);
}
