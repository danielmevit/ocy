use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_app::ErrorLog;
use damt_platform::{Clock, PlatformServices, Unsupported};
use ocy_app::coordinator::BreakWindows;
use ocy_app::slint_breaks::{BreakActionCallbacks, LatencyOptions, SlintBreakWindows};
use ocy_app::strings::Strings;
use ocy_core::engine::{BreakEngine, BreakInfo, BreakKind, BreakSurface};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};

struct MinimalHeadlessPlatform;

impl Platform for MinimalHeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
    }

    fn duration_since_start(&self) -> Duration {
        Duration::ZERO
    }
}

#[test]
fn live_language_switch_updates_active_break_cards_and_rebuilds_prewarmed_cards() {
    let _ = slint::platform::set_platform(Box::new(MinimalHeadlessPlatform));

    let temp_dir = std::env::temp_dir().join(format!("ocy-i18n-live-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let error_log = ErrorLog::new(&temp_dir, "ocy");

    let clock = Arc::new(FakeClock::from_utc(2026, 11, 20, 8, 30, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock;
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let mut settings = OcySettings {
        language: "en".to_owned(),
        ..OcySettings::default()
    };
    let engine = BreakEngine::with_defaults(settings.clone(), platform.clone(), clock_dyn.clone());
    let snapshot = engine.snapshot();

    let callbacks = BreakActionCallbacks {
        on_skip: Rc::new(|| {}),
        on_start_long_break_now: Rc::new(|| {}),
        on_postpone_long_break: Rc::new(|| {}),
        on_hold_game: Rc::new(|| {}),
    };
    let mut break_windows = SlintBreakWindows::new(
        platform,
        clock_dyn,
        Arc::new(Strings::from_language(&settings.language)),
        error_log,
        callbacks,
        settings.clone(),
        LatencyOptions::default(),
    );

    // 1. Pre-warm a short break card in English, then switch language to German -> pre-warmed card is rebuilt
    break_windows.prewarm_short_break_now();
    assert_eq!(
        break_windows.prewarm_short_close_label().as_deref(),
        Some("Skip")
    );

    settings.language = "de".to_owned();
    break_windows.sync_state(&snapshot, &settings);
    assert_eq!(
        break_windows.prewarm_short_close_label().as_deref(),
        Some("Überspringen")
    );

    // 2. Show a short break card in German, then switch live to Macedonian and Japanese -> visible card & font-family update immediately
    break_windows.show_short_break(&BreakInfo {
        kind: BreakKind::Short,
        exercise_id: "blink".to_owned(),
        total_duration: Duration::from_secs(15),
        suggestion_exercise_ids: None,
        surface: BreakSurface::Standard,
    });
    let short_card = break_windows
        .active_short_card()
        .expect("short card active");
    assert_eq!(short_card.get_font_family().as_str(), "Segoe UI");
    assert_eq!(short_card.get_heading().as_str(), "Langsam blinzeln");
    assert_eq!(short_card.get_close_label().as_str(), "Überspringen");

    settings.language = "mk".to_owned();
    break_windows.sync_state(&snapshot, &settings);
    assert_eq!(short_card.get_font_family().as_str(), "Segoe UI");
    assert_eq!(short_card.get_heading().as_str(), "Трепни полека");
    assert_eq!(short_card.get_close_label().as_str(), "Прескокни");

    settings.language = "ja".to_owned();
    break_windows.sync_state(&snapshot, &settings);
    assert_eq!(short_card.get_font_family().as_str(), "Yu Gothic UI");
    assert_eq!(short_card.get_heading().as_str(), "ゆっくりまばたき");
    assert_eq!(short_card.get_close_label().as_str(), "スキップ");
    break_windows.close_all();

    // 3. Show a warning card in Japanese, then switch live to English
    break_windows.show_warning(Duration::from_secs(42));
    let warning_card = break_windows
        .active_warning_card()
        .expect("warning card active");
    assert_eq!(warning_card.get_font_family().as_str(), "Yu Gothic UI");
    assert_eq!(warning_card.get_heading().as_str(), "長い休憩まで 0:42");
    assert_eq!(warning_card.get_start_now_label().as_str(), "今すぐ開始");

    settings.language = "en".to_owned();
    break_windows.sync_state(&snapshot, &settings);
    assert_eq!(warning_card.get_font_family().as_str(), "Segoe UI");
    assert_eq!(warning_card.get_heading().as_str(), "Long break in 0:42");
    assert_eq!(warning_card.get_start_now_label().as_str(), "Start now");
    assert_eq!(warning_card.get_postpone_label().as_str(), "Postpone 3 min");
    break_windows.close_all();

    // 4. Show a long break card in English, then switch live to German and Japanese
    break_windows.show_long_break(&BreakInfo {
        kind: BreakKind::Long,
        exercise_id: "palming".to_owned(),
        total_duration: Duration::from_secs(300),
        suggestion_exercise_ids: Some(vec!["stretch-arms".to_owned(), "palming".to_owned()]),
        surface: BreakSurface::Standard,
    });
    let long_card = break_windows.active_long_card().expect("long card active");
    assert_eq!(long_card.get_font_family().as_str(), "Segoe UI");
    assert_eq!(long_card.get_end_label().as_str(), "End break");
    assert_eq!(
        long_card.get_suggestion1_title().as_str(),
        "Stretch your arms"
    );

    settings.language = "de".to_owned();
    break_windows.sync_state(&snapshot, &settings);
    assert_eq!(long_card.get_font_family().as_str(), "Segoe UI");
    assert_eq!(long_card.get_end_label().as_str(), "Pause beenden");
    assert_eq!(
        long_card.get_countdown_label().as_str(),
        "Zurück zum Bildschirm in"
    );
    assert_eq!(long_card.get_suggestion1_title().as_str(), "Arme strecken");
    assert_eq!(
        long_card.get_suggestion2_title().as_str(),
        "Augen in den Handflächen ausruhen"
    );

    settings.language = "ja".to_owned();
    break_windows.sync_state(&snapshot, &settings);
    assert_eq!(long_card.get_font_family().as_str(), "Yu Gothic UI");
    assert_eq!(long_card.get_end_label().as_str(), "休憩を終了");
    assert_eq!(long_card.get_countdown_label().as_str(), "画面に戻るまで");
    assert_eq!(long_card.get_suggestion1_title().as_str(), "腕を伸ばす");
    break_windows.close_all();

    // 5. SettingsWindowV2 exposes font-family bound to default-font-family
    let settings_ui = ocy_ui::SettingsWindowV2::new().expect("SettingsWindowV2 created");
    assert_eq!(settings_ui.get_font_family().as_str(), "Segoe UI");
    settings_ui.set_font_family("Yu Gothic UI".into());
    assert_eq!(settings_ui.get_font_family().as_str(), "Yu Gothic UI");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
