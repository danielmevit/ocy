use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_platform::{AppThemeProvider, Clock, PlatformServices, TaskbarThemeProvider, Unsupported};
use ocy_app::strings::Strings;
use ocy_app::tray::{
    AppTheme, TaskbarIconTracker, TrayController, TrayIconState, TrayPresentation, TrayPresenter,
    TraySetSelector, TraySink, app_theme_from_opt, decode_tray_icon_rgba,
};
use ocy_app::win32_window::{WindowIconSource, select_window_icon_source};
use ocy_core::engine::BreakEngine;
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

#[derive(Debug)]
struct FakeTaskbarThemeProvider {
    value: Cell<Option<bool>>,
    call_count: Cell<usize>,
}

impl FakeTaskbarThemeProvider {
    fn new(value: Option<bool>) -> Self {
        Self {
            value: Cell::new(value),
            call_count: Cell::new(0),
        }
    }

    fn set_value(&self, value: Option<bool>) {
        self.value.set(value);
    }

    fn call_count(&self) -> usize {
        self.call_count.get()
    }
}

impl TaskbarThemeProvider for FakeTaskbarThemeProvider {
    fn is_supported(&self) -> bool {
        true
    }

    fn is_light_taskbar(&self) -> Option<bool> {
        self.call_count.set(self.call_count.get() + 1);
        self.value.get()
    }
}

#[derive(Debug, Default)]
struct RecordingTraySink {
    icon_updates: Vec<(TrayIconState, bool)>,
    tooltip_updates: Vec<String>,
    menu_updates: usize,
}

impl TraySink for RecordingTraySink {
    fn set_tooltip(&mut self, tooltip: &str) {
        self.tooltip_updates.push(tooltip.to_owned());
    }

    fn set_icon(&mut self, state: TrayIconState, use_light_set: bool) {
        self.icon_updates.push((state, use_light_set));
    }

    fn set_menu(&mut self, _presentation: &TrayPresentation) {
        self.menu_updates += 1;
    }
}

#[test]
fn tray_set_selector_resolves_correct_set() {
    let cases = [
        (Some(true), AppTheme::Light, true),
        (Some(true), AppTheme::Dark, true),
        (Some(false), AppTheme::Light, false),
        (Some(false), AppTheme::Dark, false),
        (None, AppTheme::Light, true),
        (None, AppTheme::Dark, false),
    ];

    for (is_light_taskbar, app_theme, expected) in cases {
        assert_eq!(
            TraySetSelector::use_light_set(is_light_taskbar, Some(app_theme)),
            expected
        );
    }
}

#[test]
fn tray_set_selector_null_app_theme_falls_back_to_dark() {
    assert!(!TraySetSelector::use_light_set(None, None));
}

#[test]
fn fake_platform_services_with_dark_taskbar_theme_tray_uses_dark_set() {
    let fake_theme = Rc::new(FakeTaskbarThemeProvider::new(Some(false)));
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 18, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock;
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine =
        BreakEngine::with_defaults(OcySettings::default(), platform, Arc::clone(&clock_dyn));

    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme_dyn: Rc<dyn TaskbarThemeProvider> = fake_theme.clone();
    let controller = TrayController::new(
        presenter,
        RecordingTraySink::default(),
        theme_dyn,
        clock_dyn,
        Some(AppTheme::Light),
        &engine.snapshot(),
    );

    assert!(!controller.current_use_light_set());
    assert_eq!(
        controller.sink().icon_updates,
        vec![(TrayIconState::Active, false)]
    );
    assert_eq!(fake_theme.call_count(), 1);
}

#[test]
fn fake_platform_services_with_light_taskbar_theme_tray_uses_light_set() {
    let fake_theme = Rc::new(FakeTaskbarThemeProvider::new(Some(true)));
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 18, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock;
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine =
        BreakEngine::with_defaults(OcySettings::default(), platform, Arc::clone(&clock_dyn));

    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme_dyn: Rc<dyn TaskbarThemeProvider> = fake_theme.clone();
    let controller = TrayController::new(
        presenter,
        RecordingTraySink::default(),
        theme_dyn,
        clock_dyn,
        Some(AppTheme::Dark),
        &engine.snapshot(),
    );

    assert!(controller.current_use_light_set());
    assert_eq!(
        controller.sink().icon_updates,
        vec![(TrayIconState::Active, true)]
    );
    assert_eq!(fake_theme.call_count(), 1);
}

#[test]
fn tray_controller_reads_taskbar_theme_at_most_once_per_minute_on_tick() {
    let fake_theme = Rc::new(FakeTaskbarThemeProvider::new(Some(false)));
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 18, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine =
        BreakEngine::with_defaults(OcySettings::default(), platform, Arc::clone(&clock_dyn));

    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme_dyn: Rc<dyn TaskbarThemeProvider> = fake_theme.clone();
    let mut controller = TrayController::new(
        presenter,
        RecordingTraySink::default(),
        theme_dyn,
        clock_dyn,
        Some(AppTheme::Light),
        &engine.snapshot(),
    );

    // 1. Initially read at start
    assert_eq!(fake_theme.call_count(), 1);
    assert!(!controller.current_use_light_set());
    assert_eq!(controller.sink().icon_updates.len(), 1);

    // 2. Tick after 30 seconds -> within 60s, should NOT query provider again
    clock.advance(Duration::from_secs(30));
    controller.update(&engine.snapshot());
    assert_eq!(fake_theme.call_count(), 1);
    assert_eq!(controller.sink().icon_updates.len(), 1);

    // 3. User changed taskbar theme in OS to light
    fake_theme.set_value(Some(true));

    // Tick at 59 seconds -> still under 60s from start
    clock.advance(Duration::from_secs(29));
    controller.update(&engine.snapshot());
    assert_eq!(fake_theme.call_count(), 1);
    assert!(!controller.current_use_light_set());
    assert_eq!(controller.sink().icon_updates.len(), 1);

    // 4. Tick at 60 seconds -> exactly 60s elapsed, provider is queried and icon swapped
    clock.advance(Duration::from_secs(1));
    controller.update(&engine.snapshot());
    assert_eq!(fake_theme.call_count(), 2);
    assert!(controller.current_use_light_set());
    assert_eq!(
        controller.sink().icon_updates,
        vec![
            (TrayIconState::Active, false),
            (TrayIconState::Active, true)
        ]
    );

    // 5. Subsequent update within the next minute does not query or swap
    controller.update(&engine.snapshot());
    assert_eq!(fake_theme.call_count(), 2);
    assert_eq!(controller.sink().icon_updates.len(), 2);
}

#[test]
fn embedded_tray_icons_all_decode_as_32x32_rgba() {
    let states = [
        TrayIconState::Active,
        TrayIconState::Paused,
        TrayIconState::OnBreak,
        TrayIconState::Attention,
    ];
    for use_light in [false, true] {
        for state in states {
            let (w, h, pixels) = decode_tray_icon_rgba(state, use_light).unwrap();
            assert_eq!((w, h), (32, 32));
            assert_eq!(pixels.len(), 32 * 32 * 4);
        }
    }
}

#[derive(Debug)]
struct FakeAppThemeProvider {
    value: Cell<Option<bool>>,
}

impl FakeAppThemeProvider {
    fn new(value: Option<bool>) -> Self {
        Self {
            value: Cell::new(value),
        }
    }
}

impl AppThemeProvider for FakeAppThemeProvider {
    fn is_supported(&self) -> bool {
        true
    }

    fn is_light_app_theme(&self) -> Option<bool> {
        self.value.get()
    }
}

#[test]
fn window_icon_selector_uses_sprout_png_below_40_and_exe_resource_at_40_plus() {
    for (target_px, expected_asset_px) in
        [(16, 16), (20, 20), (24, 24), (28, 32), (32, 32), (36, 48)]
    {
        let dark = select_window_icon_source(target_px, false);
        let light = select_window_icon_source(target_px, true);
        let (dark_bytes, light_bytes) = match (dark, light) {
            (
                WindowIconSource::BrandSprout {
                    png_bytes: db,
                    asset_size_px: da,
                    target_size_px: dt,
                    use_light_set: false,
                },
                WindowIconSource::BrandSprout {
                    png_bytes: lb,
                    asset_size_px: la,
                    target_size_px: lt,
                    use_light_set: true,
                },
            ) => {
                assert_eq!(da, expected_asset_px);
                assert_eq!(la, expected_asset_px);
                assert_eq!(dt, target_px);
                assert_eq!(lt, target_px);
                (db, lb)
            }
            other => panic!("expected BrandSprout for {target_px}px, got {other:?}"),
        };
        assert_ne!(
            dark_bytes, light_bytes,
            "dark and light sprout PNGs must differ at {target_px}px"
        );
        let dark_pix = resvg::tiny_skia::Pixmap::decode_png(dark_bytes).unwrap();
        let light_pix = resvg::tiny_skia::Pixmap::decode_png(light_bytes).unwrap();
        assert_eq!(
            (dark_pix.width(), dark_pix.height()),
            (expected_asset_px as u32, expected_asset_px as u32)
        );
        assert_eq!(
            (light_pix.width(), light_pix.height()),
            (expected_asset_px as u32, expected_asset_px as u32)
        );
    }

    for big_px in [40, 48, 56, 64, 96] {
        for use_light in [false, true] {
            assert_eq!(
                select_window_icon_source(big_px, use_light),
                WindowIconSource::ExeResource {
                    target_size_px: big_px
                }
            );
        }
    }
}

#[test]
fn taskbar_icon_tracker_matches_tray_rule_and_rechecks_every_60s() {
    assert_eq!(app_theme_from_opt(Some(true)), Some(AppTheme::Light));
    assert_eq!(app_theme_from_opt(Some(false)), Some(AppTheme::Dark));
    assert_eq!(app_theme_from_opt(None), None);

    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();

    // Dark taskbar overrides light app theme -> dark icon set (use_light_set = false).
    let taskbar = FakeTaskbarThemeProvider::new(Some(false));
    let app_theme = FakeAppThemeProvider::new(Some(true));
    let mut tracker = TaskbarIconTracker::new(&taskbar, &app_theme, clock_dyn);
    assert!(!tracker.use_light_set());
    assert_eq!(taskbar.call_count(), 1);

    // Taskbar flips to light; before 60s, non-forced refresh does not re-query.
    taskbar.set_value(Some(true));
    clock.advance(Duration::from_secs(59));
    assert!(!tracker.refresh(&taskbar, &app_theme, false));
    assert!(!tracker.use_light_set());
    assert_eq!(taskbar.call_count(), 1);

    // At 60s, re-query runs and flips to light icon set.
    clock.advance(Duration::from_secs(1));
    assert!(tracker.refresh(&taskbar, &app_theme, false));
    assert!(tracker.use_light_set());
    assert_eq!(taskbar.call_count(), 2);

    // When taskbar theme is unknown (None), falls back to app theme (Dark -> false, Light -> true).
    taskbar.set_value(None);
    app_theme.value.set(Some(false));
    assert!(tracker.refresh(&taskbar, &app_theme, true));
    assert!(!tracker.use_light_set());
}
