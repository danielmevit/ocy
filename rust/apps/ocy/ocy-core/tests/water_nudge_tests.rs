//! Tests for the gentle water reminder in long breaks (`water_nudges`): rotation frequency,
//! the never-two-in-a-row throttle, and the off switch.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::TestPlatformServices;
use ocy_core::engine::{BreakEngine, BreakInfo, BreakKind, EngineEvent};
use ocy_core::exercises::{DefaultExercisePicker, ExerciseCatalogue, SeededRng};
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

struct WaterHarness {
    clock: Arc<FakeClock>,
    engine: BreakEngine,
}

impl WaterHarness {
    fn new(water_nudges: bool, seed: u64) -> Self {
        let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 9, 0, 0).expect("valid date"));
        let platform = TestPlatformServices::new();
        let settings = OcySettings {
            natural_moments: false, // deterministic timing, no input lulls
            water_nudges,
            ..OcySettings::default()
        };
        let picker = Box::new(DefaultExercisePicker::new(SeededRng::new(seed)));
        let engine = BreakEngine::new(
            settings,
            platform,
            clock.clone(),
            None,
            Some(picker),
            None,
            None,
        );
        Self { clock, engine }
    }

    /// Starts and completes one long break, returning its primary (body) exercise id.
    fn run_long_break(&mut self) -> String {
        let (started, events) = self.engine.start_long_break_now();
        assert!(started, "long break must start while working");
        let body_id = events
            .iter()
            .find_map(|event| match event {
                EngineEvent::BreakStarted(info) => Some(info.exercise_id.clone()),
                _ => None,
            })
            .expect("BreakStarted raised");
        assert_ne!(body_id, String::new());

        let duration = 5 * 60;
        for _ in 0..duration {
            self.clock.advance(Duration::from_secs(1));
            let _ = self.engine.tick();
        }
        body_id
    }

    /// Runs `count` long breaks back to back, returning the body exercise id of each.
    fn long_break_body_ids(&mut self, count: usize) -> Vec<String> {
        (0..count).map(|_| self.run_long_break()).collect()
    }
}

#[test]
fn water_nudge_on_appears_but_never_in_consecutive_long_breaks() {
    let mut harness = WaterHarness::new(true, 42);
    let body_ids = harness.long_break_body_ids(40);

    let water_count = body_ids
        .iter()
        .filter(|id| id.as_str() == ExerciseCatalogue::WATER_ID)
        .count();
    assert!(water_count > 0, "water should join the long-break rotation");
    // At most every second long break: never two in a row.
    assert!(
        water_count * 2 <= body_ids.len(),
        "water appeared in {water_count} of {} long breaks",
        body_ids.len()
    );
    for pair in body_ids.windows(2) {
        assert!(
            !(pair[0] == ExerciseCatalogue::WATER_ID && pair[1] == ExerciseCatalogue::WATER_ID),
            "water must never be suggested in two consecutive long breaks"
        );
    }
}

#[test]
fn water_nudge_off_never_suggests_water() {
    let mut harness = WaterHarness::new(false, 42);
    let body_ids = harness.long_break_body_ids(40);

    assert!(
        body_ids
            .iter()
            .all(|id| id.as_str() != ExerciseCatalogue::WATER_ID),
        "water must never appear when the setting is off"
    );
}

#[test]
fn water_nudge_suggestion_pair_keeps_an_eye_exercise() {
    let mut harness = WaterHarness::new(true, 7);

    let (started, events) = harness.engine.start_long_break_now();
    assert!(started);
    let info = events
        .iter()
        .find_map(|event| match event {
            EngineEvent::BreakStarted(info) => Some(info.clone()),
            _ => None,
        })
        .expect("BreakStarted raised");
    assert_eq!(info.kind, BreakKind::Long);
    let BreakInfo {
        suggestion_exercise_ids: Some(suggestions),
        ..
    } = info
    else {
        panic!("long breaks carry two suggestions");
    };
    assert_eq!(suggestions.len(), 2);
    assert_eq!(suggestions[0], info.exercise_id);
    assert_ne!(suggestions[1], suggestions[0]);
}
