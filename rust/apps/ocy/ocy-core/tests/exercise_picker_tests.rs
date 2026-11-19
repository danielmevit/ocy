use ocy_core::exercises::{
    DefaultExercisePicker, ExerciseCatalogue, ExerciseKind, ExercisePicker, SeededRng,
};

#[test]
fn catalogue_contains_expected_initial_exercises() {
    assert_eq!(ExerciseCatalogue::ALL.len(), 12);
    assert_eq!(
        ExerciseCatalogue::ALL
            .iter()
            .filter(|e| e.kind == ExerciseKind::Eyes)
            .count(),
        9
    );
    assert_eq!(
        ExerciseCatalogue::ALL
            .iter()
            .filter(|e| e.kind == ExerciseKind::Body)
            .count(),
        3
    );

    let window_far = ExerciseCatalogue::by_id("window-far").expect("window-far exists");
    assert!(window_far.requires_window);
    assert_eq!(window_far.title_key(), "exercise.window-far.title");
    assert_eq!(window_far.body_key(), "exercise.window-far.body");
}

#[test]
fn pick_short_exercise_always_eyes_only() {
    let mut picker = DefaultExercisePicker::new(SeededRng::new(12345));

    for _ in 0..50 {
        let exercise = picker.pick_short_exercise(true, None);
        assert_eq!(exercise.kind, ExerciseKind::Eyes);
    }
}

#[test]
fn window_filter_excludes_window_exercises_when_no_window_nearby() {
    let mut picker = DefaultExercisePicker::new(SeededRng::new(42));

    for _ in 0..100 {
        let exercise = picker.pick_short_exercise(false, None);
        assert!(!exercise.requires_window);
        assert_ne!(exercise.id, "window-far");
    }
}

#[test]
fn no_immediate_repeat_avoids_consecutive_identical_ids() {
    let mut picker = DefaultExercisePicker::new(SeededRng::new(999));
    let mut previous_id: Option<&str> = None;

    for _ in 0..100 {
        let exercise = picker.pick_short_exercise(true, previous_id);
        assert_ne!(Some(exercise.id), previous_id);
        previous_id = Some(exercise.id);
    }
}

#[test]
fn deterministic_with_seeded_random() {
    let mut picker1 = DefaultExercisePicker::new(SeededRng::new(777));
    let mut picker2 = DefaultExercisePicker::new(SeededRng::new(777));

    for _ in 0..20 {
        let ex1 = picker1.pick_short_exercise(true, None);
        let ex2 = picker2.pick_short_exercise(true, None);
        assert_eq!(ex1.id, ex2.id);
    }
}

#[test]
fn pick_long_exercises_picks_one_body_and_one_eyes() {
    let mut picker = DefaultExercisePicker::new(SeededRng::new(555));

    for _ in 0..30 {
        let (body, eyes) = picker.pick_long_exercises(true, None, None);
        assert_eq!(body.kind, ExerciseKind::Body);
        assert_eq!(eyes.kind, ExerciseKind::Eyes);
    }
}
