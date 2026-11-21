use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_art::Theme;
use damt_platform::{AppThemeProvider, Clock, PlatformServices, ScreenRect};
use ocy_app::art::{
    OcyArt, SHORT_BREAK_ART_LOGICAL_PX, SUGGESTION_ART_LOGICAL_PX, SuggestionSchedule, art_theme,
    frame_and_next_delay, scaled_art_px,
};
use ocy_app::break_logic::{
    AppThemeTracker, CliArgs, DemoBreak, PREWARM_LEAD_TIME, PREWARM_TTL, PrewarmKind,
    PrewarmPlanner, app_theme_is_dark, build_demo_launch_plan, long_break_can_end,
    long_break_countdown_text, long_break_progress, parse_cli_args, short_break_progress,
    warning_buttons, warning_heading, warning_postpone_label,
};
use ocy_app::layout::{DipSize, OverlayLayout, PixelPoint, ScreenInfo};
use ocy_app::strings::Strings;
use ocy_app::win32_window::{
    WindowRole, WindowRoleGuard, apply_window_attributes_for_role, break_window_ex_style,
    current_window_role, should_allow_close, target_ex_style,
};
use ocy_core::engine::{BreakEngine, BreakKind, EngineEvent, EnginePhase, EngineSnapshot};
use ocy_core::exercises::ExerciseCatalogue;
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

#[derive(Debug)]
struct FakeAppThemeProvider {
    value: Mutex<Option<bool>>,
    calls: AtomicUsize,
}

impl FakeAppThemeProvider {
    fn new(value: Option<bool>) -> Self {
        Self {
            value: Mutex::new(value),
            calls: AtomicUsize::new(0),
        }
    }

    fn set_value(&self, value: Option<bool>) {
        *self.value.lock().unwrap() = value;
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl AppThemeProvider for FakeAppThemeProvider {
    fn is_supported(&self) -> bool {
        true
    }

    fn is_light_app_theme(&self) -> Option<bool> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.value.lock().unwrap()
    }
}

fn snapshot_working(until_short: Option<Duration>, until_long: Option<Duration>) -> EngineSnapshot {
    EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: until_short,
        time_until_long_break: until_long,
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    }
}

#[test]
fn placement_rects_corner_and_centered_across_dpi_and_work_area() {
    // 1. Standard 1080p at 1.0x with 40px bottom taskbar
    let primary = ScreenInfo::new(
        ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        1.0,
        true,
        Some(ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        }),
    );
    let corner_1x =
        OverlayLayout::compute_corner_position(&primary, DipSize::new(392.0, 140.0), 0.0);
    assert_eq!(corner_1x, PixelPoint::new(1920 - 392, 1040 - 140));

    let centered_1x =
        OverlayLayout::compute_centered_position(&primary, DipSize::new(392.0, 140.0));
    assert_eq!(
        centered_1x,
        PixelPoint::new((1920 - 392) / 2, (1080 - 140) / 2)
    );

    // 2. Secondary left monitor with negative X, 1.5x DPI, and top/bottom work area offset
    let secondary = ScreenInfo::new(
        ScreenRect {
            x: -3840,
            y: 0,
            width: 3840,
            height: 2160,
        },
        1.5,
        false,
        Some(ScreenRect {
            x: -3840,
            y: 48,
            width: 3840,
            height: 2052,
        }),
    );
    let corner_15x =
        OverlayLayout::compute_corner_position(&secondary, DipSize::new(392.0, 140.0), 0.0);
    // 392 * 1.5 = 588, 140 * 1.5 = 210
    assert_eq!(
        corner_15x,
        PixelPoint::new(-3840 + 3840 - 588, 48 + 2052 - 210)
    );

    let lb_dip = OverlayLayout::compute_long_break_window_size(340.0, &secondary);
    assert_eq!(lb_dip, DipSize::new(544.0, 420.0));
    let (lb_w, lb_h) = OverlayLayout::physical_size(lb_dip, secondary.scaling);
    assert_eq!((lb_w, lb_h), (816, 630));
    let lb_pos = OverlayLayout::compute_centered_position(&secondary, lb_dip);
    assert_eq!(
        lb_pos,
        PixelPoint::new(-3840 + (3840 - 816) / 2, (2160 - 630) / 2)
    );
}

#[test]
fn prewarm_decision_triggers_within_3s_and_respects_suppression_and_ttl() {
    let settings = OcySettings::default(); // short 10m, long 60m, warning 60s

    // Outside 3s window -> None
    let snap = snapshot_working(Some(Duration::from_secs(4)), Some(Duration::from_secs(300)));
    assert_eq!(PrewarmPlanner::decide(&snap, &settings, None), None);

    // Exactly 3s before short break -> ShortBreak
    let snap = snapshot_working(Some(PREWARM_LEAD_TIME), Some(Duration::from_secs(300)));
    assert_eq!(
        PrewarmPlanner::decide(&snap, &settings, None),
        Some(PrewarmKind::ShortBreak)
    );

    // Short break due in 2s, but long break due within 120s -> suppressed, no ShortBreak prewarm
    let snap = snapshot_working(Some(Duration::from_secs(2)), Some(Duration::from_secs(120)));
    assert_eq!(PrewarmPlanner::decide(&snap, &settings, None), None);

    // Long break warning due in 3s (long due in 63s with 60s warning) -> Warning
    let snap = snapshot_working(
        Some(Duration::from_secs(200)),
        Some(Duration::from_secs(63)),
    );
    assert_eq!(
        PrewarmPlanner::decide(&snap, &settings, None),
        Some(PrewarmKind::Warning)
    );

    // Long break with warning_seconds = 0 due in 3s -> LongBreak directly
    let mut no_warn_settings = settings.clone();
    no_warn_settings.long_breaks.warning_seconds = 0;
    let snap = snapshot_working(Some(Duration::from_secs(200)), Some(Duration::from_secs(3)));
    assert_eq!(
        PrewarmPlanner::decide(&snap, &no_warn_settings, None),
        Some(PrewarmKind::LongBreak)
    );

    // During LongBreakWarning with <=3s remaining -> LongBreak
    let mut warn_snap = snapshot_working(None, Some(Duration::from_secs(60)));
    warn_snap.phase = EnginePhase::LongBreakWarning;
    assert_eq!(
        PrewarmPlanner::decide(&warn_snap, &settings, Some(Duration::from_secs(4))),
        None
    );
    assert_eq!(
        PrewarmPlanner::decide(&warn_snap, &settings, Some(Duration::from_secs(3))),
        Some(PrewarmKind::LongBreak)
    );

    // Paused/Away/Deferred never pre-warm
    for phase in [
        EnginePhase::Paused,
        EnginePhase::Away,
        EnginePhase::Deferred,
        EnginePhase::ShortBreak,
        EnginePhase::LongBreak,
    ] {
        let mut s = snapshot_working(Some(Duration::from_secs(1)), Some(Duration::from_secs(1)));
        s.phase = phase;
        assert_eq!(PrewarmPlanner::decide(&s, &settings, None), None);
    }

    // Drop after 10s or on Paused/Away
    assert!(!PrewarmPlanner::should_drop(
        EnginePhase::Working,
        Duration::from_secs(9)
    ));
    assert!(PrewarmPlanner::should_drop(
        EnginePhase::Working,
        PREWARM_TTL
    ));
    assert!(PrewarmPlanner::should_drop(
        EnginePhase::Paused,
        Duration::from_secs(1)
    ));
    assert!(PrewarmPlanner::should_drop(
        EnginePhase::Away,
        Duration::from_secs(1)
    ));
}

#[test]
fn strict_mode_controls_warning_and_long_break_buttons() {
    let normal_with_postpones = warning_buttons(false, 2);
    assert!(normal_with_postpones.show_start_now);
    assert!(normal_with_postpones.show_postpone);
    assert!(normal_with_postpones.show_skip);
    assert!(!normal_with_postpones.show_strict_line);

    let normal_no_postpones = warning_buttons(false, 0);
    assert!(normal_no_postpones.show_start_now);
    assert!(!normal_no_postpones.show_postpone);
    assert!(normal_no_postpones.show_skip);
    assert!(!normal_no_postpones.show_strict_line);

    let strict = warning_buttons(true, 3);
    assert!(strict.show_start_now);
    assert!(!strict.show_postpone);
    assert!(!strict.show_skip);
    assert!(strict.show_strict_line);

    assert!(long_break_can_end(false));
    assert!(!long_break_can_end(true));
}

#[test]
fn countdown_text_and_progress_match_csharp() {
    let strings = Strings::from_embedded().unwrap();
    assert_eq!(
        warning_heading(&strings, Duration::from_secs(42)),
        "Long break in 0:42"
    );
    assert_eq!(warning_postpone_label(&strings, 3), "Postpone 3 min");
    assert_eq!(long_break_countdown_text(Duration::from_secs(252)), "4:12");

    // Short break drains 1.0 -> 0.0
    assert!(
        (short_break_progress(Duration::from_secs(15), Duration::from_secs(15)) - 1.0).abs() < 1e-6
    );
    assert!(
        (short_break_progress(Duration::from_secs(5), Duration::from_secs(20)) - 0.25).abs() < 1e-6
    );
    assert!((short_break_progress(Duration::ZERO, Duration::from_secs(15)) - 0.0).abs() < 1e-6);

    // Long break fills 0.0 -> 1.0
    assert!(
        (long_break_progress(Duration::from_secs(300), Duration::from_secs(300)) - 0.0).abs()
            < 1e-6
    );
    assert!(
        (long_break_progress(Duration::from_secs(252), Duration::from_secs(300)) - 0.16).abs()
            < 1e-6
    );
    assert!((long_break_progress(Duration::ZERO, Duration::from_secs(300)) - 1.0).abs() < 1e-6);
}

#[test]
fn reduced_motion_returns_frozen_frame_and_no_timer() {
    let art = OcyArt::from_embedded().unwrap();

    // palming has frozen = 4 in manifest.json
    let (seq_reduced, frozen) = art.sequencer_for("palming", true).unwrap();
    assert_eq!(frozen, 4);
    let (frame, next) = frame_and_next_delay(&seq_reduced, Duration::from_secs(10), true, frozen);
    assert_eq!(frame, 4);
    assert_eq!(next, None);

    // Normal motion starts at sequence[0] and schedules next_change_after
    let (seq_normal, frozen_normal) = art.sequencer_for("palming", false).unwrap();
    let (frame0, next0) = frame_and_next_delay(&seq_normal, Duration::ZERO, false, frozen_normal);
    assert_eq!(frame0, 1);
    assert_eq!(next0, Some(Duration::from_millis(240)));
}

#[test]
fn embedded_art_covers_all_exercises_themes_and_clears_cache() {
    let art = OcyArt::from_embedded().unwrap();
    assert_eq!(scaled_art_px(SHORT_BREAK_ART_LOGICAL_PX, 1.5), 84);
    assert_eq!(scaled_art_px(SUGGESTION_ART_LOGICAL_PX, 2.0), 80);

    for ex in ExerciseCatalogue::ALL {
        let info = art
            .exercise_info(ex.id)
            .unwrap_or_else(|| panic!("missing manifest entry for {}", ex.id));
        for theme in [Theme::Light, Theme::Dark] {
            for frame in 1..=info.frames {
                let img = art
                    .render_frame(ex.id, theme, frame, 56)
                    .unwrap_or_else(|| panic!("missing svg for {} {:?} f{frame}", ex.id, theme))
                    .unwrap_or_else(|e| {
                        panic!("failed render {} {:?} f{frame}: {e}", ex.id, theme)
                    });
                assert_eq!(img.size().width, 56);
                assert_eq!(img.size().height, 56);
            }
        }
        let frozen = art
            .render_frozen(ex.id, art_theme(true), SUGGESTION_ART_LOGICAL_PX, 1.5)
            .unwrap()
            .unwrap();
        assert_eq!(frozen.size().width, 60);
    }

    assert!(art.cache_bytes_used() > 0);
    art.clear_cache();
    assert_eq!(art.cache_bytes_used(), 0);
}

#[test]
fn win32_ex_style_and_close_guard_match_rules() {
    const WS_EX_TOPMOST: usize = 0x0000_0008;
    const WS_EX_WINDOWEDGE: usize = 0x0000_0100;
    const WS_EX_ACCEPTFILES: usize = 0x0000_0010;
    const WS_EX_TOOLWINDOW: usize = 0x0000_0080;
    const WS_EX_APPWINDOW: usize = 0x0004_0000;
    const WS_EX_NOACTIVATE: usize = 0x0800_0000;

    let style =
        target_ex_style(WS_EX_APPWINDOW | WS_EX_TOPMOST | WS_EX_WINDOWEDGE | WS_EX_ACCEPTFILES);
    assert_eq!(style & WS_EX_APPWINDOW, 0);
    assert_ne!(style & WS_EX_TOOLWINDOW, 0);
    assert_ne!(style & WS_EX_NOACTIVATE, 0);
    assert_ne!(style & WS_EX_TOPMOST, 0);
    assert_ne!(style & WS_EX_WINDOWEDGE, 0);
    assert_ne!(style & WS_EX_ACCEPTFILES, 0);
    assert_eq!(
        break_window_ex_style(WS_EX_APPWINDOW),
        style & !(WS_EX_TOPMOST | WS_EX_WINDOWEDGE | WS_EX_ACCEPTFILES)
    );

    assert!(!should_allow_close(true));
    assert!(should_allow_close(false));
}

#[test]
fn window_role_guard_sets_and_resets_across_early_returns_and_nesting() {
    assert_eq!(current_window_role(), WindowRole::Normal);

    fn early_return_helper(fail: bool) -> Result<(), &'static str> {
        let _guard = WindowRoleGuard::new(WindowRole::BreakCard);
        assert_eq!(current_window_role(), WindowRole::BreakCard);
        if fail {
            return Err("early");
        }
        Ok(())
    }

    assert!(early_return_helper(true).is_err());
    assert_eq!(current_window_role(), WindowRole::Normal);

    assert!(early_return_helper(false).is_ok());
    assert_eq!(current_window_role(), WindowRole::Normal);

    {
        let _outer = WindowRoleGuard::new(WindowRole::BreakCard);
        assert_eq!(current_window_role(), WindowRole::BreakCard);
        {
            let _inner = WindowRoleGuard::new(WindowRole::Normal);
            assert_eq!(current_window_role(), WindowRole::Normal);
        }
        assert_eq!(current_window_role(), WindowRole::BreakCard);
    }
    assert_eq!(current_window_role(), WindowRole::Normal);
}

#[test]
fn window_attributes_hook_applies_break_card_defaults_and_leaves_normal_untouched() {
    use slint::winit_030::winit::window::{WindowAttributes, WindowLevel};

    let normal_attrs = apply_window_attributes_for_role(WindowAttributes::default(), Some(1234));
    assert!(normal_attrs.active);
    assert_eq!(normal_attrs.window_level, WindowLevel::Normal);

    let _guard = WindowRoleGuard::new(WindowRole::BreakCard);
    let break_attrs = apply_window_attributes_for_role(WindowAttributes::default(), Some(1234));
    assert!(!break_attrs.active);
    assert_eq!(break_attrs.window_level, WindowLevel::AlwaysOnTop);
}

#[test]
fn app_theme_tracker_falls_back_to_light_and_rechecks_at_most_every_60s() {
    assert!(!app_theme_is_dark(None));
    assert!(!app_theme_is_dark(Some(true)));
    assert!(app_theme_is_dark(Some(false)));

    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let provider = FakeAppThemeProvider::new(None);
    let mut tracker = AppThemeTracker::new(&provider, clock_dyn);
    assert!(!tracker.is_dark());
    assert_eq!(provider.call_count(), 1);

    // 30s later: provider changed to dark, but within 60s window so not re-read
    provider.set_value(Some(false));
    clock.advance(Duration::from_secs(30));
    assert!(!tracker.refresh(&provider, false));
    assert_eq!(provider.call_count(), 1);
    assert!(!tracker.is_dark());

    // At 60s: re-read happens and dark flips to true
    clock.advance(Duration::from_secs(30));
    assert!(tracker.refresh(&provider, false));
    assert_eq!(provider.call_count(), 2);
    assert!(tracker.is_dark());
}

#[test]
fn overlay_break_windows_respects_short_fullscreen_and_60s_theme_refresh() {
    #[derive(Debug)]
    struct TestPlatform {
        theme: FakeAppThemeProvider,
    }
    impl PlatformServices for TestPlatform {
        fn app_theme(&self) -> &dyn AppThemeProvider {
            &self.theme
        }
    }

    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).unwrap());
    let platform = Arc::new(TestPlatform {
        theme: FakeAppThemeProvider::new(Some(true)),
    });
    let mut overlay = ocy_app::coordinator::OverlayBreakWindows::with_clock(
        ocy_app::coordinator::NullBreakWindows,
        platform.clone(),
        clock.clone(),
    );

    assert_eq!(platform.theme.call_count(), 1);
    let mut settings = OcySettings::default();
    settings.short_breaks.fullscreen = true;
    let snap = snapshot_working(None, None);
    use ocy_app::coordinator::BreakWindows;
    overlay.sync_state(&snap, &settings);

    clock.advance(Duration::from_secs(30));
    overlay.on_tick(&snap);
    assert_eq!(platform.theme.call_count(), 1);

    clock.advance(Duration::from_secs(30));
    overlay.on_tick(&snap);
    assert_eq!(platform.theme.call_count(), 2);
}

#[test]
fn cli_flag_parsing_handles_report_memory_demo_break_demo_in_and_demo_settings() {
    assert_eq!(parse_cli_args(Vec::<String>::new()), CliArgs::default());
    assert_eq!(
        parse_cli_args(["--report-memory", "--demo-break", "short"].map(str::to_owned)),
        CliArgs {
            report_memory: true,
            demo_break: Some(DemoBreak::Short),
            demo_in_secs: None,
            demo_settings: false,
        }
    );
    assert_eq!(
        parse_cli_args(["--demo-break=WARNING", "--demo-in", "5"].map(str::to_owned)),
        CliArgs {
            report_memory: false,
            demo_break: Some(DemoBreak::Warning),
            demo_in_secs: Some(5),
            demo_settings: false,
        }
    );
    assert_eq!(
        parse_cli_args(["--demo-break", "long", "--demo-in=12"].map(str::to_owned)),
        CliArgs {
            report_memory: false,
            demo_break: Some(DemoBreak::Long),
            demo_in_secs: Some(12),
            demo_settings: false,
        }
    );
    assert_eq!(
        parse_cli_args(["--demo-settings"].map(str::to_owned)),
        CliArgs {
            report_memory: false,
            demo_break: None,
            demo_in_secs: None,
            demo_settings: true,
        }
    );
    assert_eq!(
        parse_cli_args(["--demo-break", "unknown", "--demo-in", "nope"].map(str::to_owned)),
        CliArgs {
            report_memory: false,
            demo_break: None,
            demo_in_secs: None,
            demo_settings: false,
        }
    );
    assert_eq!(
        parse_cli_args(["--demo-in=-3"].map(str::to_owned)),
        CliArgs {
            report_memory: false,
            demo_break: None,
            demo_in_secs: None,
            demo_settings: false,
        }
    );
}

#[test]
fn demo_in_launch_plan_triggers_prewarm_and_break_through_real_engine_ticks() {
    #[derive(Debug)]
    struct DummyPlatform;
    impl PlatformServices for DummyPlatform {}

    for (demo, expected_prewarm) in [
        (DemoBreak::Short, PrewarmKind::ShortBreak),
        (DemoBreak::Warning, PrewarmKind::Warning),
        (DemoBreak::Long, PrewarmKind::LongBreak),
    ] {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 12, 0, 0).unwrap());
        let mut settings = OcySettings::default();
        let plan = build_demo_launch_plan(demo, Some(5), &mut settings);
        assert!(!plan.trigger_immediately);
        assert_eq!(plan.initial_clock_offset_secs, 0);

        let mut engine = BreakEngine::new(
            settings.clone(),
            Arc::new(DummyPlatform),
            clock.clone(),
            None,
            None,
            plan.initial_state,
            None,
        );

        let mut saw_prewarm = false;
        let mut saw_due = false;
        for sec in 1..=5 {
            clock.advance(Duration::from_secs(1));
            let events = engine.tick();
            let snap = engine.snapshot();
            if PrewarmPlanner::decide(&snap, &settings, None) == Some(expected_prewarm) {
                saw_prewarm = true;
            }
            for ev in events {
                match (demo, ev) {
                    (DemoBreak::Short, EngineEvent::BreakStarted(info))
                        if info.kind == BreakKind::Short =>
                    {
                        saw_due = true;
                        assert_eq!(sec, 5);
                    }
                    (DemoBreak::Warning, EngineEvent::LongBreakWarningStarted(_)) => {
                        saw_due = true;
                        assert_eq!(sec, 5);
                    }
                    (DemoBreak::Long, EngineEvent::BreakStarted(info))
                        if info.kind == BreakKind::Long =>
                    {
                        saw_due = true;
                        assert_eq!(sec, 5);
                    }
                    _ => {}
                }
            }
        }
        assert!(saw_prewarm, "expected prewarm for {demo:?}");
        assert!(saw_due, "expected due event at 5s for {demo:?}");
    }
}

#[test]
fn prewarm_art_populates_cache_and_clears_cleanly() {
    let art = OcyArt::from_embedded().unwrap();
    assert_eq!(art.cache_bytes_used(), 0);

    art.prewarm_short_break(Theme::Light, 56, false, true);
    let short_bytes = art.cache_bytes_used();
    assert!(short_bytes > 0);
    art.clear_cache();
    assert_eq!(art.cache_bytes_used(), 0);

    art.prewarm_long_break(Theme::Dark, SUGGESTION_ART_LOGICAL_PX, 1.5);
    assert!(art.cache_bytes_used() > 0);
    art.clear_cache();
    assert_eq!(art.cache_bytes_used(), 0);

    art.prewarm_long_break_frames(Theme::Light, SUGGESTION_ART_LOGICAL_PX, 1.0, false);
    assert!(art.cache_bytes_used() > 0);
    art.clear_cache();
    assert_eq!(art.cache_bytes_used(), 0);
}

#[test]
fn suggestion_schedule_animates_both_slots_by_soonest_change_and_respects_reduced_motion() {
    let art = OcyArt::from_embedded().unwrap();

    // stretch-arms: step 380ms, sequence [1,1,2,3,3,3,3,2], frozen 3
    // palming: step 240ms, sequence [1,2,3,2,...], frozen 4
    let (seq1, frozen1) = art.sequencer_for("stretch-arms", false).unwrap();
    let (seq2, frozen2) = art.sequencer_for("palming", false).unwrap();
    let mut sched = SuggestionSchedule::new(
        Some(("stretch-arms".to_owned(), seq1, frozen1)),
        Some(("palming".to_owned(), seq2, frozen2)),
        false,
    );

    let (init, next0) = sched.initial();
    assert_eq!(init, [Some(1), Some(1)]);
    assert_eq!(next0, Some(Duration::from_millis(240)));

    // At 240ms: palming advances 1 -> 2; stretch-arms is still in first 380ms step (no change),
    // soonest next change is stretch-arms at 380ms (140ms away).
    let (u1, next1) = sched.advance_at(Duration::from_millis(240));
    assert_eq!(u1, [None, Some(2)]);
    assert_eq!(next1, Some(Duration::from_millis(140)));

    // At 380ms: stretch-arms enters step 1 (still frame 1, so no redraw needed); palming next at 480ms (100ms away).
    let (u2, next2) = sched.advance_at(Duration::from_millis(380));
    assert_eq!(u2, [None, None]);
    assert_eq!(next2, Some(Duration::from_millis(100)));

    // At 480ms: palming advances 2 -> 3; stretch-arms next at 760ms (280ms away), palming next at 720ms (240ms away).
    let (u3, next3) = sched.advance_at(Duration::from_millis(480));
    assert_eq!(u3, [None, Some(3)]);
    assert_eq!(next3, Some(Duration::from_millis(240)));

    // At 760ms: stretch-arms reaches step 2 (frame 2); palming at 760ms is in step 3 (frame 2).
    let (u4, next4) = sched.advance_at(Duration::from_millis(760));
    assert_eq!(u4, [Some(2), Some(2)]);
    assert_eq!(next4, Some(Duration::from_millis(200))); // palming next boundary at 960ms

    // Reduced motion returns frozen frames immediately and schedules no timer.
    let (seq1_rm, f1) = art.sequencer_for("stretch-arms", true).unwrap();
    let (seq2_rm, f2) = art.sequencer_for("palming", true).unwrap();
    let mut sched_rm = SuggestionSchedule::new(
        Some(("stretch-arms".to_owned(), seq1_rm, f1)),
        Some(("palming".to_owned(), seq2_rm, f2)),
        true,
    );
    let (init_rm, next_rm) = sched_rm.initial();
    assert_eq!(init_rm, [Some(3), Some(4)]);
    assert_eq!(next_rm, None);

    // Single-slot schedule works cleanly too.
    let (seq_single, f_single) = art.sequencer_for("palming", false).unwrap();
    let mut single = SuggestionSchedule::new(
        Some(("palming".to_owned(), seq_single, f_single)),
        None,
        false,
    );
    assert_eq!(
        single.initial(),
        ([Some(1), None], Some(Duration::from_millis(240)))
    );
    assert_eq!(
        single.advance_at(Duration::from_millis(240)),
        ([Some(2), None], Some(Duration::from_millis(240)))
    );
}
