use crate::ArtError;
use serde_json::Value;
use std::collections::BTreeMap;

pub type IllustrationInfo = ExerciseInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExerciseInfo {
    pub frames: u32,
    pub sequence: Vec<u32>,
    pub step_ms: u32,
    pub frozen: u32,
}

impl ExerciseInfo {
    pub fn step_ms(&self) -> u32 {
        self.step_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    pub palette: String,
    pub view_box: Vec<i32>,
    pub exercises: BTreeMap<String, ExerciseInfo>,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, ArtError> {
        let root: Value = serde_json::from_str(json).map_err(ArtError::Json)?;
        let object = root
            .as_object()
            .ok_or_else(|| ArtError::Manifest("root must be an object".to_owned()))?;

        let version = object
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let palette = object
            .get("palette")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let view_box = object
            .get("viewBox")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_i64)
                    .map(|value| value as i32)
                    .collect()
            })
            .unwrap_or_default();

        let mut exercises = BTreeMap::new();
        let exercise_object = object
            .get("exercises")
            .and_then(Value::as_object)
            .ok_or_else(|| ArtError::Manifest("exercises must be an object".to_owned()))?;

        for (id, value) in exercise_object {
            let exercise = value
                .as_object()
                .ok_or_else(|| ArtError::Manifest(format!("{id}: exercise must be an object")))?;
            let frames = exercise
                .get("frames")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    ArtError::Manifest(format!("{id}: frames must be a positive integer"))
                })? as u32;
            if frames == 0 {
                return Err(ArtError::Manifest(format!("{id}: frames must be positive")));
            }

            let sequence = exercise
                .get("sequence")
                .and_then(Value::as_array)
                .ok_or_else(|| ArtError::Manifest(format!("{id}: sequence must be an array")))?
                .iter()
                .map(|item| {
                    item.as_u64().map(|value| value as u32).ok_or_else(|| {
                        ArtError::Manifest(format!("{id}: sequence values must be integers"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if sequence.is_empty() {
                return Err(ArtError::Manifest(format!(
                    "{id}: sequence cannot be empty"
                )));
            }
            if sequence.iter().any(|frame| *frame == 0 || *frame > frames) {
                return Err(ArtError::Manifest(format!(
                    "{id}: sequence values must be within 1..={frames}"
                )));
            }

            let frozen = exercise
                .get("frozen")
                .and_then(Value::as_u64)
                .ok_or_else(|| ArtError::Manifest(format!("{id}: frozen must be an integer")))?
                as u32;
            if frozen == 0 || frozen > frames {
                return Err(ArtError::Manifest(format!(
                    "{id}: frozen must be within 1..={frames}"
                )));
            }

            let step_ms = exercise.get("stepMs").and_then(Value::as_i64).unwrap_or(0);
            let step_ms = if step_ms > 0 { step_ms as u32 } else { 300 };

            exercises.insert(
                id.clone(),
                ExerciseInfo {
                    frames,
                    sequence,
                    step_ms,
                    frozen,
                },
            );
        }

        Ok(Self {
            version,
            palette,
            view_box,
            exercises,
        })
    }

    pub fn get(&self, id: &str) -> Option<&ExerciseInfo> {
        if id.is_empty() {
            return None;
        }
        self.exercises.get(id)
    }
}
