//! Exercise catalogue, definitions, and injectable random exercise picker.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Category of break exercise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExerciseKind {
    /// Eye-rest exercise used in short breaks and as the eye suggestion in long breaks.
    Eyes,
    /// Stretch or movement exercise used as the primary suggestion in long breaks.
    Body,
}

/// Static definition of a break exercise in [`ExerciseCatalogue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExerciseDefinition {
    /// Unique identifier (e.g. `"blink"`, `"window-far"`).
    pub id: &'static str,
    /// Exercise category.
    pub kind: ExerciseKind,
    /// Whether this exercise requires a nearby window.
    pub requires_window: bool,
    /// Suggested exercise duration.
    pub suggested_duration: Duration,
}

impl ExerciseDefinition {
    /// Localization key for the exercise title (`exercise.<id>.title`).
    #[must_use]
    pub fn title_key(&self) -> String {
        format!("exercise.{}.title", self.id)
    }

    /// Localization key for the exercise instructions (`exercise.<id>.body`).
    #[must_use]
    pub fn body_key(&self) -> String {
        format!("exercise.{}.body", self.id)
    }
}

/// Built-in catalogue of Ocy exercises.
#[derive(Debug)]
pub struct ExerciseCatalogue;

impl ExerciseCatalogue {
    /// All built-in exercises (9 eye exercises, 3 body exercises).
    pub const ALL: &'static [ExerciseDefinition] = &[
        // Eyes (9 exercises)
        ExerciseDefinition {
            id: "blink",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(15),
        },
        ExerciseDefinition {
            id: "close-tight",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(15),
        },
        ExerciseDefinition {
            id: "roll",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(15),
        },
        ExerciseDefinition {
            id: "look-up-down",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(15),
        },
        ExerciseDefinition {
            id: "look-left-right",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(15),
        },
        ExerciseDefinition {
            id: "near-far",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(15),
        },
        ExerciseDefinition {
            id: "window-far",
            kind: ExerciseKind::Eyes,
            requires_window: true,
            suggested_duration: Duration::from_secs(20),
        },
        ExerciseDefinition {
            id: "palming",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(20),
        },
        ExerciseDefinition {
            id: "figure-eight",
            kind: ExerciseKind::Eyes,
            requires_window: false,
            suggested_duration: Duration::from_secs(20),
        },
        // Body (3 exercises)
        ExerciseDefinition {
            id: "neck-shoulders",
            kind: ExerciseKind::Body,
            requires_window: false,
            suggested_duration: Duration::from_secs(60),
        },
        ExerciseDefinition {
            id: "stand-walk",
            kind: ExerciseKind::Body,
            requires_window: false,
            suggested_duration: Duration::from_secs(120),
        },
        ExerciseDefinition {
            id: "stretch-arms",
            kind: ExerciseKind::Body,
            requires_window: false,
            suggested_duration: Duration::from_secs(60),
        },
    ];

    /// Looks up an exercise by ID (case-insensitive).
    #[must_use]
    pub fn by_id(id: &str) -> Option<&'static ExerciseDefinition> {
        Self::ALL.iter().find(|e| e.id.eq_ignore_ascii_case(id))
    }
}

/// Injectable source of bounded pseudo-random indices for exercise selection.
pub trait RandomSource: fmt::Debug {
    /// Returns an integer in `0..upper_bound` (or `0` if `upper_bound == 0`).
    fn next_usize(&mut self, upper_bound: usize) -> usize;
}

/// Minimal deterministic SplitMix64 PRNG (zero external dependencies).
#[derive(Debug, Clone)]
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    /// Creates a deterministic PRNG from `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Creates a PRNG seeded from system time.
    #[must_use]
    pub fn from_system_time() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::new(nanos)
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Default for SeededRng {
    fn default() -> Self {
        Self::from_system_time()
    }
}

impl RandomSource for SeededRng {
    fn next_usize(&mut self, upper_bound: usize) -> usize {
        if upper_bound <= 1 {
            return 0;
        }
        (self.next_u64() % (upper_bound as u64)) as usize
    }
}

/// Selects exercises for short and long breaks.
pub trait ExercisePicker: fmt::Debug {
    /// Picks an eye exercise for a short break.
    fn pick_short_exercise(
        &mut self,
        has_window_nearby: bool,
        previous_exercise_id: Option<&str>,
    ) -> &'static ExerciseDefinition;

    /// Picks `(body, eyes)` exercises for a long break.
    fn pick_long_exercises(
        &mut self,
        has_window_nearby: bool,
        previous_eyes_id: Option<&str>,
        previous_body_id: Option<&str>,
    ) -> (&'static ExerciseDefinition, &'static ExerciseDefinition);
}

/// Default [`ExercisePicker`] backed by an injectable [`RandomSource`].
#[derive(Debug, Clone)]
pub struct DefaultExercisePicker<R: RandomSource = SeededRng> {
    rng: R,
}

impl Default for DefaultExercisePicker<SeededRng> {
    fn default() -> Self {
        Self::new(SeededRng::from_system_time())
    }
}

impl<R: RandomSource> DefaultExercisePicker<R> {
    /// Creates a picker with the given random source.
    #[must_use]
    pub const fn new(rng: R) -> Self {
        Self { rng }
    }

    fn pick_from_kind(
        &mut self,
        kind: ExerciseKind,
        has_window_nearby: bool,
        previous_id: Option<&str>,
    ) -> &'static ExerciseDefinition {
        let mut candidates: Vec<&'static ExerciseDefinition> = ExerciseCatalogue::ALL
            .iter()
            .filter(|e| e.kind == kind)
            .filter(|e| has_window_nearby || !e.requires_window)
            .collect();

        if candidates.len() > 1
            && let Some(prev) = previous_id
            && !prev.is_empty()
        {
            let without_prev: Vec<&'static ExerciseDefinition> = candidates
                .iter()
                .copied()
                .filter(|e| !e.id.eq_ignore_ascii_case(prev))
                .collect();
            if !without_prev.is_empty() {
                candidates = without_prev;
            }
        }

        let idx = self.rng.next_usize(candidates.len());
        candidates
            .get(idx)
            .copied()
            .unwrap_or(&ExerciseCatalogue::ALL[0])
    }
}

impl<R: RandomSource> ExercisePicker for DefaultExercisePicker<R> {
    fn pick_short_exercise(
        &mut self,
        has_window_nearby: bool,
        previous_exercise_id: Option<&str>,
    ) -> &'static ExerciseDefinition {
        self.pick_from_kind(ExerciseKind::Eyes, has_window_nearby, previous_exercise_id)
    }

    fn pick_long_exercises(
        &mut self,
        has_window_nearby: bool,
        previous_eyes_id: Option<&str>,
        previous_body_id: Option<&str>,
    ) -> (&'static ExerciseDefinition, &'static ExerciseDefinition) {
        let body = self.pick_from_kind(ExerciseKind::Body, has_window_nearby, previous_body_id);
        let eyes = self.pick_short_exercise(has_window_nearby, previous_eyes_id);
        (body, eyes)
    }
}
