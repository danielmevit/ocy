//! Integration tests for the Settings V2 pure logic modules:
//! `presets`, `timeline`, `today`, and `language_choice`.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use damt_app::ErrorLog;
use damt_platform::{Clock, FocusState, FocusStateProvider, IdleProvider, PlatformServices};
use ocy_app::language_choice;
use ocy_app::presets::{self, Preset};
use ocy_app::strings::available_languages;
use ocy_app::timeline;
use ocy_app::today::TodaySummary;
use ocy_core::engine::{BreakEngine, BreakKind, EngineEvent, EnginePhase, NATURAL_LULL};
use ocy_core::exercises::{DefaultExercisePicker, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;
use ocy_ui::TimelineMarkKind;

#[derive(Debug)]
struct ScriptedPlatform {
    idle_nanos: AtomicU64,
}

impl ScriptedPlatform {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            idle_nanos: AtomicU64::new(0),
        })
    }

    fn set_idle(&self, duration: Duration) {
        self.idle_nanos.store(
            u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
}

impl IdleProvider for ScriptedPlatform {
    fn is_supported(&self) -> bool {
        true
    }

    fn idle_time(&self) -> Duration {
        Duration::from_nanos(self.idle_nanos.load(Ordering::Relaxed))
    }
}

impl FocusStateProvider for ScriptedPlatform {
    fn is_supported(&self) -> bool {
        true
    }

    fn state(&self) -> FocusState {
        FocusState::Normal
    }
}

impl PlatformServices for ScriptedPlatform {
    fn idle(&self) -> &dyn IdleProvider {
        self
    }

    fn focus(&self) -> &dyn FocusStateProvider {
        self
    }
}

fn simulate_2h_against_engine(settings: OcySettings, periodic_natural_lull: bool) {
    let clock =
        Arc::new(FakeClock::from_fixed_offset(2026, 11, 20, 9, 0, 0, 1).expect("valid clock"));
    let platform = ScriptedPlatform::new();
    let picker = Box::new(DefaultExercisePicker::new(SeededRng::new(11)));
    let mut engine = BreakEngine::new(
        settings,
        platform.clone(),
        Arc::clone(&clock) as Arc<dyn Clock>,
        None,
        Some(picker),
        None,
        None,
    );

    let projection = timeline::project(&engine.snapshot(), engine.settings(), clock.as_ref());
    assert_eq!(projection.labels, ["09:30", "10:00", "10:30", "11:00"]);
    let ui_marks = projection.to_ui_marks();
    assert_eq!(ui_marks.len(), projection.marks.len());

    let mut actual_starts: Vec<(BreakKind, Duration)> = Vec::new();
    let span_secs = timeline::TIMELINE_SPAN.as_secs();
    for sec in 1..=span_secs {
        if periodic_natural_lull && sec % 15 == 0 {
            // Brief 2 s micro-pause while reading/thinking (never approaches away_threshold).
            platform.set_idle(NATURAL_LULL);
        } else {
            platform.set_idle(Duration::ZERO);
        }
        clock.advance(Duration::from_secs(1));
        for ev in engine.tick() {
            if let EngineEvent::BreakStarted(info) = ev {
                actual_starts.push((info.kind, Duration::from_secs(sec)));
            }
        }
    }

    assert_eq!(projection.marks.len(), actual_starts.len());
    for ((mark, ui_mark), (actual_kind, actual_at)) in projection
        .marks
        .iter()
        .zip(ui_marks.iter())
        .zip(actual_starts.iter())
    {
        assert_eq!(mark.kind, *actual_kind);
        let expected_ui_kind = match *actual_kind {
            BreakKind::Short => TimelineMarkKind::ShortBreak,
            BreakKind::Long => TimelineMarkKind::LongBreak,
        };
        assert_eq!(ui_mark.kind, expected_ui_kind);

        let mark_secs = f64::from(mark.position) * timeline::TIMELINE_SPAN.as_secs_f64();
        let actual_secs = actual_at.as_secs_f64();
        assert!(
            (mark_secs - actual_secs).abs() <= 60.0,
            "mark at {mark_secs}s vs actual {actual_kind:?} at {actual_secs}s"
        );
    }
}

#[test]
fn presets_round_trip_with_timeline_and_engine() {
    for &preset in presets::list() {
        let mut settings = presets::apply(preset, &OcySettings::default());
        assert_eq!(presets::detect(&settings), preset);
        settings.natural_moments = false;
        simulate_2h_against_engine(settings, false);
    }
}

#[test]
fn timeline_matches_real_engine_with_natural_moments_and_brief_lulls() {
    simulate_2h_against_engine(OcySettings::default(), true);
}

#[test]
fn today_counts_breaks_completed_and_skipped_during_engine_run() {
    let clock =
        Arc::new(FakeClock::from_fixed_offset(2026, 11, 20, 10, 0, 0, 2).expect("valid clock"));
    let platform = ScriptedPlatform::new();
    let settings = OcySettings {
        natural_moments: false,
        ..OcySettings::default()
    };
    let mut engine = BreakEngine::new(
        settings,
        platform,
        Arc::clone(&clock) as Arc<dyn Clock>,
        None,
        None,
        None,
        None,
    );

    assert_eq!(
        TodaySummary::for_today(&engine, clock.as_ref()),
        TodaySummary {
            taken: 0,
            skipped: 0
        }
    );

    // Trigger and complete one short break.
    assert!(engine.start_short_break_now().0);
    assert_eq!(engine.phase(), EnginePhase::ShortBreak);
    for _ in 0..15 {
        clock.advance(Duration::from_secs(1));
        let _ = engine.tick();
    }

    // Trigger and skip a second short break.
    assert!(engine.start_short_break_now().0);
    assert!(engine.skip_current_break().0);

    // Trigger and complete one long break.
    assert!(engine.start_long_break_now().0);
    for _ in 0..(5 * 60) {
        clock.advance(Duration::from_secs(1));
        let _ = engine.tick();
    }

    assert_eq!(
        TodaySummary::for_today(&engine, clock.as_ref()),
        TodaySummary {
            taken: 2,
            skipped: 1
        }
    );
}

#[test]
fn language_choice_model_and_unknown_logging() {
    let model = language_choice::dropdown_model("Automatic");
    assert_eq!(model[0], "Automatic");
    assert_eq!(model.len(), available_languages().len() + 1);

    let dir = std::env::temp_dir().join(format!("ocy-s2b1-lang-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let log = ErrorLog::new(&dir, "ocy");

    assert_eq!(language_choice::index_for("auto", Some(&log)), 0);
    assert_eq!(language_choice::index_for("", Some(&log)), 0);
    assert!(!log.path().exists(), "known tags must not write error log");

    assert_eq!(language_choice::index_for("zz-INVALID", Some(&log)), 0);
    assert!(log.path().exists(), "unknown tag must write to ErrorLog");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preset_from_index_matches_list_order() {
    for (idx, &preset) in presets::list().iter().enumerate() {
        assert_eq!(preset.index(), idx as i32);
        assert_eq!(Preset::from_index(idx as i32), Some(preset));
    }
}
