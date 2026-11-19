#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod cache;
pub mod manifest;
pub mod raster;
pub mod source;

pub use cache::ArtCache;
pub use manifest::{ExerciseInfo, IllustrationInfo, Manifest};
pub use raster::{Image, Raster};
pub use source::{DirSource, EmbeddedSource, FrameSource, Theme};

use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArtError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("invalid SVG: {0}")]
    Svg(String),
    #[error("invalid sequence: {0}")]
    Sequence(String),
    #[error("could not create an image of size {0}x{0}")]
    InvalidSize(u32),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct FrameSequencer {
    sequence: Vec<u32>,
    step: Duration,
    frozen: Option<u32>,
}

impl FrameSequencer {
    pub fn new(sequence: &[u32], step: Duration) -> Result<Self, ArtError> {
        if sequence.is_empty() {
            return Err(ArtError::Sequence("sequence cannot be empty".to_owned()));
        }
        Ok(Self {
            sequence: sequence.to_vec(),
            step: if step.is_zero() {
                Duration::from_millis(300)
            } else {
                step
            },
            frozen: None,
        })
    }

    pub fn with_reduced_motion(mut self, frozen: u32) -> Self {
        self.frozen = Some(frozen);
        self
    }

    pub fn with_frozen(mut self, frozen: u32) -> Self {
        self.frozen = Some(frozen);
        self
    }

    pub fn set_reduced_motion(&mut self, frozen: Option<u32>) {
        self.frozen = frozen;
    }

    pub fn sequence(&self) -> &[u32] {
        &self.sequence
    }

    pub fn step(&self) -> Duration {
        self.step
    }

    pub fn get_frame(&self, elapsed: Duration) -> u32 {
        if let Some(frozen) = self.frozen {
            return frozen;
        }
        Self::get_frame_static(&self.sequence, self.step, elapsed)
    }

    pub fn get_frame_static(sequence: &[u32], step: Duration, elapsed: Duration) -> u32 {
        if sequence.is_empty() {
            return 1;
        }
        if step.is_zero() || step.as_nanos() == 0 {
            return sequence[0];
        }
        let elapsed = elapsed.as_nanos();
        let step_index = elapsed / step.as_nanos();
        sequence[(step_index as usize) % sequence.len()]
    }

    pub fn next_change_after(&self, elapsed: Duration) -> Duration {
        if self.frozen.is_some() {
            return Duration::ZERO;
        }
        if self.step.is_zero() {
            return Duration::ZERO;
        }
        let elapsed = elapsed.max(Duration::ZERO);
        let elapsed_nanos = elapsed.as_nanos();
        let step_nanos = self.step.as_nanos();
        let elapsed_index = elapsed_nanos / step_nanos;
        let next_index = elapsed_index + 1;
        let next_elapsed = Duration::from_nanos(
            u64::try_from(next_index.saturating_mul(step_nanos)).unwrap_or(u64::MAX),
        );
        next_elapsed.saturating_sub(elapsed)
    }
}

pub fn get_frame(sequence: &[u32], step: Duration, elapsed: Duration) -> u32 {
    FrameSequencer::get_frame_static(sequence, step, elapsed)
}
