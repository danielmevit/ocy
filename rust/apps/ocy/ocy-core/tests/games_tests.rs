//! Pure-logic tests for game classification, gentle and hold behaviour, and the post-game prompt.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestPlatformServices;
use damt_platform::{
    FocusState, ForegroundObservation, QUNS_APP, QUNS_BUSY, QUNS_PRESENTATION_MODE,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};
use jiff::civil::Date;
use ocy_core::engine::{
    BreakEngine, BreakKind, BreakOutcome, BreakSurface, EngineEvent, EnginePhase,
};
use ocy_core::games::{self, FullscreenClass, GameDecision, PromptPull};
use ocy_core::settings::{GamesMode, OcySettings};
use ocy_core::time::FakeClock;

const DEMO: &str = r"D:\Games\Demo\Demo.exe";
const OTHER: &str = r"C:\Windows\notepad.exe";
const STEP: Duration = Duration::from_secs(10);

fn observation(quns: Option<i32>, path: Option<&str>, pid: Option<u32>) -> ForegroundObservation {
    let exe_path = path.map(str::to_owned);
    let exe_name = exe_path.as_deref().and_then(|value| {
        let name = value.rsplit(['\\', '/']).next().unwrap_or(value);
        if name.is_empty() {
            None
        } else {
            Some(name.to_owned())
        }
    });
    ForegroundObservation {
        notification_state: quns,
        exe_name,
        exe_path,
        process_id: pid,
    }
}

fn class_of(
    focus: FocusState,
    quns: Option<i32>,
    path: Option<&str>,
    paths: &[&str],
) -> FullscreenClass {
    let obs = observation(quns, path, Some(42));
    let stored: Vec<String> = paths.iter().map(|path| (*path).to_owned()).collect();
    games::classify(focus, &obs, &stored)
}

fn decision(
    class: &FullscreenClass,
    settings: &OcySettings,
    hold_until_alive: bool,
) -> GameDecision {
    games::decide(class, settings, hold_until_alive)
}

struct Harness {
    clock: Arc<FakeClock>,
    platform: Arc<TestPlatformServices>,
    engine: BreakEngine,
}

impl Harness {
    fn new(mut settings: OcySettings) -> Self {
        settings.natural_moments = false;
        settings.clamp();
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).expect("valid date"));
        let platform = TestPlatformServices::new();
        // The user is calm by default (quiet, but nowhere near the 120 s away threshold):
        // in a gentle game a due card now always waits for a calm moment, whatever
        // `natural_moments` says. Tests that need an active user set idle themselves.
        platform.idle.set_idle_time(Duration::from_secs(5));
        platform.controller.set_idle_time(Duration::from_secs(5));
        let engine = BreakEngine::with_defaults(settings, platform.clone(), clock.clone());
        Self {
            clock,
            platform,
            engine,
        }
    }

    fn show(&self, quns: i32, path: Option<&str>, pid: Option<u32>, game_bar: bool) {
        self.platform
            .foreground
            .set_observation(observation(Some(quns), path, pid));
        if game_bar {
            if let Some(path) = path {
                self.platform
                    .foreground
                    .set_game_bar_paths(vec![path.to_owned()]);
            }
        } else {
            self.platform.foreground.set_game_bar_paths(Vec::new());
        }
    }

    fn leave_to_desktop(&self) {
        self.platform.focus.set_state(FocusState::Normal);
        self.platform.foreground.set_observation(observation(
            Some(5),
            Some(r"C:\Windows\explorer.exe"),
            Some(7),
        ));
    }

    fn tick(&mut self, step: Duration) -> Vec<EngineEvent> {
        self.clock.advance(step);
        self.engine.tick()
    }

    fn tick_for(&mut self, total: Duration) {
        let mut left = total;
        while left > Duration::ZERO {
            let step = STEP.min(left);
            self.tick(step);
            left -= step;
        }
    }
}

fn today() -> Date {
    Date::constant(2026, 11, 17)
}

fn game_settings() -> OcySettings {
    let mut settings = OcySettings {
        natural_moments: false,
        ..OcySettings::default()
    };
    settings.short_breaks.interval_minutes = 25;
    settings.short_breaks.duration_seconds = 10;
    settings.long_breaks.interval_minutes = 60;
    settings.long_breaks.duration_minutes = 5;
    settings.long_breaks.warning_seconds = 60;
    settings
}

#[test]
fn classification_separates_games_from_presentations_and_other_fullscreen_apps() {
    let game = class_of(
        FocusState::FullscreenApp,
        Some(QUNS_BUSY),
        Some(DEMO),
        &[DEMO],
    );
    assert!(matches!(
        game,
        FullscreenClass::Game {
            undetectable_exclusive: false,
            ..
        }
    ));

    let same_name_other_dir = class_of(
        FocusState::FullscreenApp,
        Some(QUNS_BUSY),
        Some(r"C:\Temp\Demo.exe"),
        &[DEMO],
    );
    assert!(matches!(
        same_name_other_dir,
        FullscreenClass::PresentationOrOther
    ));

    let bare = class_of(
        FocusState::FullscreenApp,
        Some(QUNS_APP),
        Some(DEMO),
        &["Demo.exe"],
    );
    assert!(matches!(bare, FullscreenClass::Game { .. }));

    let slash = class_of(
        FocusState::Normal,
        Some(QUNS_BUSY),
        Some("D:/Games/Demo/Demo.exe"),
        &[DEMO],
    );
    assert!(matches!(slash, FullscreenClass::Game { .. }));

    let d3d = class_of(
        FocusState::Normal,
        Some(QUNS_RUNNING_D3D_FULL_SCREEN),
        Some(OTHER),
        &[],
    );
    assert!(matches!(
        d3d,
        FullscreenClass::Game {
            undetectable_exclusive: false,
            ..
        }
    ));

    let hidden = games::classify(
        FocusState::FullscreenApp,
        &ForegroundObservation {
            notification_state: Some(QUNS_RUNNING_D3D_FULL_SCREEN),
            exe_name: None,
            exe_path: None,
            process_id: Some(9),
        },
        &[],
    );
    assert!(matches!(
        hidden,
        FullscreenClass::Game {
            undetectable_exclusive: true,
            process_id: Some(9),
            ..
        }
    ));

    let presentation = class_of(
        FocusState::Normal,
        Some(QUNS_PRESENTATION_MODE),
        Some(DEMO),
        &[DEMO],
    );
    assert!(matches!(presentation, FullscreenClass::PresentationOrOther));

    let video = class_of(FocusState::FullscreenApp, Some(QUNS_BUSY), Some(OTHER), &[]);
    assert!(matches!(video, FullscreenClass::PresentationOrOther));

    let windowed = class_of(FocusState::FullscreenApp, Some(5), Some(DEMO), &[DEMO]);
    assert!(matches!(windowed, FullscreenClass::NotFullscreen));

    let unknown_fullscreen = class_of(FocusState::FullscreenApp, None, None, &[]);
    assert!(matches!(
        unknown_fullscreen,
        FullscreenClass::PresentationOrOther
    ));

    let unknown_normal = class_of(FocusState::Normal, None, None, &[]);
    assert!(matches!(unknown_normal, FullscreenClass::NotFullscreen));
}

#[test]
fn gentle_is_the_default_and_hold_silent_and_undetectable_games_hold() {
    let mut settings = OcySettings::default();
    let game = class_of(
        FocusState::FullscreenApp,
        Some(QUNS_BUSY),
        Some(DEMO),
        &[DEMO],
    );
    let video = class_of(FocusState::FullscreenApp, Some(QUNS_BUSY), Some(OTHER), &[]);
    let gentle = decision(&game, &settings, false);
    assert!(!gentle.break_held);
    assert!(gentle.gentle_game);
    assert!(!gentle.suppressed_play);

    settings.games_mode = GamesMode::Hold;
    let held = decision(&game, &settings, false);
    assert!(held.break_held && held.suppressed_play && !held.gentle_game);
    let presentation = decision(&video, &settings, false);
    assert!(presentation.break_held);
    assert!(!presentation.suppressed_play);

    settings.games_mode = GamesMode::Gentle;
    settings.pause_during_fullscreen = false;
    assert!(!decision(&video, &settings, false).break_held);
    settings.games_silent.push("demo.exe".to_owned());
    let silent = decision(&game, &settings, false);
    assert!(silent.break_held && silent.suppressed_play);

    let hidden = games::classify(
        FocusState::FullscreenApp,
        &ForegroundObservation {
            notification_state: Some(QUNS_RUNNING_D3D_FULL_SCREEN),
            exe_name: None,
            exe_path: None,
            process_id: Some(9),
        },
        &[],
    );
    settings.games_silent.clear();
    settings.pause_during_fullscreen = true;
    assert!(decision(&hidden, &settings, false).suppressed_play);

    assert!(decision(&FullscreenClass::NotFullscreen, &settings, true).break_held);
}

#[test]
fn prompt_pulls_the_sooner_break_and_prefers_the_long_break_inside_two_minutes() {
    let settings = game_settings();
    assert_eq!(
        games::prompt_pull(
            &settings,
            Duration::from_secs(20 * 60),
            Duration::from_secs(45 * 60)
        ),
        PromptPull::Short
    );
    assert_eq!(
        games::prompt_pull(
            &settings,
            Duration::from_secs(5 * 60),
            Duration::from_secs(59 * 60)
        ),
        PromptPull::Long
    );
}

#[test]
fn a_gentle_game_shows_the_small_card_for_short_and_long_breaks() {
    let mut settings = game_settings();
    settings.short_breaks.interval_minutes = 10;
    settings.long_breaks.interval_minutes = 15;
    let mut h = Harness::new(settings);
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);

    h.tick_for(Duration::from_secs(10 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    let current = h.engine.current_break().expect("short card");
    assert_eq!(current.kind, BreakKind::Short);
    assert_eq!(current.surface, BreakSurface::GentleCard);
    assert_eq!(current.total_duration, Duration::from_secs(10));

    h.tick(Duration::from_secs(10));
    assert_eq!(h.engine.phase(), EnginePhase::Working);

    h.tick_for(Duration::from_secs(4 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
    h.tick_for(Duration::from_secs(60));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    let current = h.engine.current_break().expect("gentle long card");
    assert_eq!(current.surface, BreakSurface::GentleCard);
    assert_eq!(current.total_duration, Duration::from_secs(10));

    let (started, events) = h.engine.start_long_break_now();
    assert!(started);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::BreakEnded(_, BreakOutcome::Interrupted)))
    );
    let current = h.engine.current_break().expect("real long break");
    assert_eq!(current.surface, BreakSurface::Standard);
    assert_eq!(current.total_duration, Duration::from_secs(5 * 60));

    h.engine.skip_current_break();
    let mut settings = h.engine.settings().clone();
    settings.short_breaks.enabled = false;
    h.engine.update_settings(settings);
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    h.tick_for(Duration::from_secs(14 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
    h.tick_for(Duration::from_secs(60));
    let events = h.tick(Duration::from_secs(10));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::BreakEnded(_, BreakOutcome::Passed)))
    );
    let stats = h.engine.statistics().get(today());
    assert_eq!(stats.map(|day| day.long_completed), Some(0));
    assert_eq!(h.engine.since_long_break(), Duration::ZERO);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
}

#[test]
fn strict_mode_refuses_to_skip_a_gentle_long_card_until_it_passes() {
    let mut settings = game_settings();
    settings.short_breaks.enabled = false;
    settings.long_breaks.interval_minutes = 15;
    settings.strict_mode = true;
    let mut h = Harness::new(settings);
    h.show(QUNS_RUNNING_D3D_FULL_SCREEN, Some(DEMO), Some(42), false);

    h.tick_for(Duration::from_secs(14 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreakWarning);
    h.tick_for(Duration::from_secs(60));
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
    assert!(!h.engine.skip_current_break().0);
    assert_eq!(h.engine.phase(), EnginePhase::LongBreak);
}

#[test]
fn hold_mode_and_the_silent_list_are_case_insensitive_and_keep_breaks_held() {
    let mut settings = game_settings();
    settings.games_mode = GamesMode::Hold;
    let mut h = Harness::new(settings);
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    h.tick_for(Duration::from_secs(25 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);

    let mut settings = game_settings();
    settings.games_silent.push(r"C:\Games\DEMO.EXE".to_owned());
    let mut h = Harness::new(settings);
    h.show(QUNS_APP, Some(DEMO), Some(42), true);
    h.tick_for(Duration::from_secs(25 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.settings().is_silent_game("demo.exe"));
}

#[test]
fn hold_until_close_survives_alt_tab_and_ends_when_the_process_exits() {
    let mut h = Harness::new(game_settings());
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    assert!(h.engine.hold_until_foreground_game_closes());

    h.platform.focus.set_state(FocusState::Normal);
    h.platform.foreground.set_observation(observation(
        Some(5),
        Some(r"C:\Windows\explorer.exe"),
        Some(7),
    ));
    h.tick_for(Duration::from_secs(25 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_ne!(h.engine.phase(), EnginePhase::ShortBreak);

    h.platform.foreground.mark_dead(42);
    h.tick(STEP);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    // The death tick already counted 10 s, so the remaining short interval starts the card
    // without a further tick that would also finish it.
    h.tick_for(Duration::from_secs(25 * 60 - 10));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(
        h.engine.current_break().map(|info| info.surface),
        Some(BreakSurface::Standard)
    );
}

#[test]
fn leaving_a_held_game_after_forty_five_minutes_prompts_the_next_break() {
    let mut h = Harness::new(game_settings());
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    h.engine.silence_foreground_game();
    h.tick_for(Duration::from_secs(45 * 60));
    h.leave_to_desktop();
    h.tick(STEP);
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert_eq!(
        h.engine.current_break().map(|info| info.surface),
        Some(BreakSurface::Standard)
    );
}

#[test]
fn leaving_before_forty_five_minutes_or_after_a_video_does_not_prompt() {
    let mut h = Harness::new(game_settings());
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    h.engine.silence_foreground_game();
    h.tick_for(Duration::from_secs(44 * 60));
    h.leave_to_desktop();
    h.tick(STEP);
    assert_eq!(h.engine.phase(), EnginePhase::Working);

    let mut h = Harness::new(game_settings());
    h.show(QUNS_BUSY, Some(OTHER), Some(8), false);
    h.tick_for(Duration::from_secs(45 * 60));
    h.leave_to_desktop();
    h.tick(STEP);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.since_short_break() < h.engine.settings().short_breaks.interval());
}

#[test]
fn an_exclusive_game_without_a_readable_exe_holds_and_a_borderless_unknown_game_holds_like_video() {
    let mut h = Harness::new(game_settings());
    h.platform
        .foreground
        .set_observation(ForegroundObservation {
            notification_state: Some(QUNS_RUNNING_D3D_FULL_SCREEN),
            exe_name: None,
            exe_path: None,
            process_id: Some(9),
        });
    h.platform.foreground.set_game_bar_paths(Vec::new());
    assert!(!h.engine.silence_foreground_game());
    h.tick_for(Duration::from_secs(25 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::Working);

    let mut h = Harness::new(game_settings());
    h.show(QUNS_BUSY, Some(OTHER), Some(8), false);
    h.tick_for(Duration::from_secs(25 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert_eq!(h.engine.since_short_break(), Duration::ZERO);
}

#[test]
fn a_natural_moment_still_caps_the_post_game_prompt_at_one_minute() {
    let mut settings = game_settings();
    settings.natural_moments = true;
    let mut h = Harness::new(settings);
    h.engine.update_settings({
        let mut settings = h.engine.settings().clone();
        settings.natural_moments = true;
        settings
    });
    h.platform.idle.set_idle_time(Duration::ZERO);
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    h.engine.silence_foreground_game();
    h.tick_for(Duration::from_secs(45 * 60));
    h.leave_to_desktop();
    h.tick(STEP);
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.snapshot().natural_hold.is_some());

    h.tick_for(Duration::from_secs(50));
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    h.tick(STEP);
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
}

#[test]
fn direct_short_break_during_a_gentle_game_stays_a_small_card() {
    let mut h = Harness::new(game_settings());
    h.show(QUNS_RUNNING_D3D_FULL_SCREEN, Some(DEMO), Some(42), false);
    let (started, events) = h.engine.start_short_break_now();
    assert!(started);
    let info = events.iter().find_map(|event| match event {
        EngineEvent::BreakStarted(info) => Some(info),
        _ => None,
    });
    let info = info.expect("break started");
    assert_eq!(info.surface, BreakSurface::GentleCard);
    assert_eq!(info.kind, BreakKind::Short);
}

#[test]
fn suspended_reminders_produce_no_game_break_and_no_post_game_prompt() {
    let mut settings = game_settings();
    settings.short_breaks.interval_minutes = 10;
    settings.long_breaks.interval_minutes = 15;
    let mut h = Harness::new(settings);
    h.engine.set_reminders_suspended(true);

    // A gentle game would normally show small cards here.
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    let mut started = Vec::new();
    for _ in 0..(20 * 6) {
        started.extend(h.tick(STEP));
    }
    // A held game played for 45 minutes would normally prompt the next break on leaving.
    h.engine.silence_foreground_game();
    for _ in 0..(45 * 6) {
        started.extend(h.tick(STEP));
    }
    h.leave_to_desktop();
    started.extend(h.tick(STEP));
    assert!(
        !started
            .iter()
            .any(|event| matches!(event, EngineEvent::BreakStarted(_))),
        "a suspended engine must not start any break"
    );
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(h.engine.current_break().is_none());
    assert!(!h.engine.snapshot().poll_external);

    // Resuming must not fire a post-game prompt armed during the suspension.
    h.engine.set_reminders_suspended(false);
    let events = h.tick(STEP);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, EngineEvent::BreakStarted(_))),
        "resuming must not prompt a break for play seen while suspended"
    );
}

#[test]
fn a_gentle_long_break_in_a_game_respects_the_water_throttle() {
    use ocy_core::exercises::{DefaultExercisePicker, ExerciseCatalogue, SeededRng};

    let mut settings = game_settings();
    settings.natural_moments = false;
    settings.water_nudges = true;
    settings.short_breaks.enabled = false;
    settings.long_breaks.interval_minutes = 15;
    settings.long_breaks.warning_seconds = 0;
    settings.clamp();
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).expect("valid date"));
    let platform = TestPlatformServices::new();
    // A calm user: in a game the gentle long card waits for a calm moment.
    platform.idle.set_idle_time(Duration::from_secs(5));
    platform.controller.set_idle_time(Duration::from_secs(5));
    let picker = Box::new(DefaultExercisePicker::new(SeededRng::new(42)));
    let engine = BreakEngine::new(
        settings,
        platform.clone(),
        clock.clone(),
        None,
        Some(picker),
        None,
        None,
    );
    let mut h = Harness {
        clock,
        platform,
        engine,
    };
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);

    let mut body_ids = Vec::new();
    for _ in 0..(40 * 16 * 6) {
        for event in h.tick(STEP) {
            if let EngineEvent::BreakStarted(info) = event {
                assert_eq!(info.kind, BreakKind::Long);
                assert_eq!(
                    h.engine.current_break().map(|current| current.surface),
                    Some(BreakSurface::GentleCard)
                );
                body_ids.push(info.exercise_id);
            }
        }
    }
    assert!(body_ids.len() >= 30, "expected many gentle long cards");
    assert!(
        body_ids
            .iter()
            .any(|id| id.as_str() == ExerciseCatalogue::WATER_ID),
        "water may still be suggested in a gentle long break"
    );
    for pair in body_ids.windows(2) {
        assert!(
            !(pair[0] == ExerciseCatalogue::WATER_ID && pair[1] == ExerciseCatalogue::WATER_ID),
            "water must never be suggested in two consecutive gentle long breaks"
        );
    }
}

#[test]
fn not_during_this_game_closes_the_card_uncounted_and_keeps_the_game_quiet() {
    let mut settings = game_settings();
    settings.short_breaks.interval_minutes = 10;
    let mut h = Harness::new(settings);
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    h.tick_for(Duration::from_secs(10 * 60));
    assert_eq!(h.engine.phase(), EnginePhase::ShortBreak);
    assert!(h.engine.snapshot().game_in_front);

    assert!(h.engine.silence_foreground_game());
    let (closed, events) = h.engine.dismiss_gentle_card();
    assert!(closed);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::BreakEnded(_, BreakOutcome::Interrupted)))
    );
    assert_eq!(h.engine.phase(), EnginePhase::Working);
    assert!(!h.engine.snapshot().game_in_front);
    let stats = h.engine.statistics().get(today());
    assert_eq!(stats.map_or(0, |day| day.short_skipped), 0);
    assert_eq!(stats.map_or(0, |day| day.short_completed), 0);

    for _ in 0..(30 * 6) {
        let events = h.tick(STEP);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::BreakStarted(_))),
            "no break may land on a silent game"
        );
    }
    assert!(!h.engine.dismiss_gentle_card().0);
}

#[test]
fn game_in_front_is_false_for_desktop_and_after_hold_until_close() {
    let mut h = Harness::new(game_settings());
    h.tick(STEP);
    assert!(!h.engine.snapshot().game_in_front);
    h.show(QUNS_BUSY, Some(DEMO), Some(42), true);
    h.tick(STEP);
    assert!(h.engine.snapshot().game_in_front);
    assert!(h.engine.hold_until_foreground_game_closes());
    assert!(!h.engine.snapshot().game_in_front);
}
