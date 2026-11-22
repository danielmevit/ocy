//! Embedded exercise illustration assets, caching, and stepped frame sequencing.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_art::{ArtCache, ArtError, EmbeddedSource, FrameSequencer, Manifest, Theme};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const MANIFEST_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/ocy-v1f/manifest.json"
));

macro_rules! svg_entry {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../assets/illustrations/ocy-v1f/svg/",
                $name
            )) as &[u8],
        )
    };
}

/// Placeholder art mapping: exercises listed here render with another exercise's illustration
/// until their own art arrives. `water` reuses the stand-walk illustration.
///
/// This mapping lives here instead of the illustration manifest because the shared
/// `damt-art` manifest test requires an on-disk `water-*.svg` per manifest entry.
const PLACEHOLDER_ART: &[(&str, &str)] = &[("water", "stand-walk")];

/// Resolves an exercise id to the id whose manifest entry and SVG frames provide its art.
#[must_use]
fn art_id(exercise_id: &str) -> &str {
    PLACEHOLDER_ART
        .iter()
        .find(|(id, _)| *id == exercise_id)
        .map_or(exercise_id, |(_, source)| *source)
}

const EMBEDDED_SVGS: &[(&str, &[u8])] = &[
    svg_entry!("blink-light-f1.svg"),
    svg_entry!("blink-dark-f1.svg"),
    svg_entry!("blink-light-f2.svg"),
    svg_entry!("blink-dark-f2.svg"),
    svg_entry!("blink-light-f3.svg"),
    svg_entry!("blink-dark-f3.svg"),
    svg_entry!("close-tight-light-f1.svg"),
    svg_entry!("close-tight-dark-f1.svg"),
    svg_entry!("close-tight-light-f2.svg"),
    svg_entry!("close-tight-dark-f2.svg"),
    svg_entry!("close-tight-light-f3.svg"),
    svg_entry!("close-tight-dark-f3.svg"),
    svg_entry!("close-tight-light-f4.svg"),
    svg_entry!("close-tight-dark-f4.svg"),
    svg_entry!("figure-eight-light-f1.svg"),
    svg_entry!("figure-eight-dark-f1.svg"),
    svg_entry!("figure-eight-light-f2.svg"),
    svg_entry!("figure-eight-dark-f2.svg"),
    svg_entry!("figure-eight-light-f3.svg"),
    svg_entry!("figure-eight-dark-f3.svg"),
    svg_entry!("figure-eight-light-f4.svg"),
    svg_entry!("figure-eight-dark-f4.svg"),
    svg_entry!("figure-eight-light-f5.svg"),
    svg_entry!("figure-eight-dark-f5.svg"),
    svg_entry!("figure-eight-light-f6.svg"),
    svg_entry!("figure-eight-dark-f6.svg"),
    svg_entry!("figure-eight-light-f7.svg"),
    svg_entry!("figure-eight-dark-f7.svg"),
    svg_entry!("look-left-right-light-f1.svg"),
    svg_entry!("look-left-right-dark-f1.svg"),
    svg_entry!("look-left-right-light-f2.svg"),
    svg_entry!("look-left-right-dark-f2.svg"),
    svg_entry!("look-left-right-light-f3.svg"),
    svg_entry!("look-left-right-dark-f3.svg"),
    svg_entry!("look-up-down-light-f1.svg"),
    svg_entry!("look-up-down-dark-f1.svg"),
    svg_entry!("look-up-down-light-f2.svg"),
    svg_entry!("look-up-down-dark-f2.svg"),
    svg_entry!("look-up-down-light-f3.svg"),
    svg_entry!("look-up-down-dark-f3.svg"),
    svg_entry!("near-far-light-f1.svg"),
    svg_entry!("near-far-dark-f1.svg"),
    svg_entry!("near-far-light-f2.svg"),
    svg_entry!("near-far-dark-f2.svg"),
    svg_entry!("neck-shoulders-light-f1.svg"),
    svg_entry!("neck-shoulders-dark-f1.svg"),
    svg_entry!("neck-shoulders-light-f2.svg"),
    svg_entry!("neck-shoulders-dark-f2.svg"),
    svg_entry!("neck-shoulders-light-f3.svg"),
    svg_entry!("neck-shoulders-dark-f3.svg"),
    svg_entry!("neck-shoulders-light-f4.svg"),
    svg_entry!("neck-shoulders-dark-f4.svg"),
    svg_entry!("neck-shoulders-light-f5.svg"),
    svg_entry!("neck-shoulders-dark-f5.svg"),
    svg_entry!("neck-shoulders-light-f6.svg"),
    svg_entry!("neck-shoulders-dark-f6.svg"),
    svg_entry!("neck-shoulders-light-f7.svg"),
    svg_entry!("neck-shoulders-dark-f7.svg"),
    svg_entry!("neck-shoulders-light-f8.svg"),
    svg_entry!("neck-shoulders-dark-f8.svg"),
    svg_entry!("palming-light-f1.svg"),
    svg_entry!("palming-dark-f1.svg"),
    svg_entry!("palming-light-f2.svg"),
    svg_entry!("palming-dark-f2.svg"),
    svg_entry!("palming-light-f3.svg"),
    svg_entry!("palming-dark-f3.svg"),
    svg_entry!("palming-light-f4.svg"),
    svg_entry!("palming-dark-f4.svg"),
    svg_entry!("roll-light-f1.svg"),
    svg_entry!("roll-dark-f1.svg"),
    svg_entry!("roll-light-f2.svg"),
    svg_entry!("roll-dark-f2.svg"),
    svg_entry!("roll-light-f3.svg"),
    svg_entry!("roll-dark-f3.svg"),
    svg_entry!("roll-light-f4.svg"),
    svg_entry!("roll-dark-f4.svg"),
    svg_entry!("roll-light-f5.svg"),
    svg_entry!("roll-dark-f5.svg"),
    svg_entry!("roll-light-f6.svg"),
    svg_entry!("roll-dark-f6.svg"),
    svg_entry!("stand-walk-light-f1.svg"),
    svg_entry!("stand-walk-dark-f1.svg"),
    svg_entry!("stand-walk-light-f2.svg"),
    svg_entry!("stand-walk-dark-f2.svg"),
    svg_entry!("stand-walk-light-f3.svg"),
    svg_entry!("stand-walk-dark-f3.svg"),
    svg_entry!("stand-walk-light-f4.svg"),
    svg_entry!("stand-walk-dark-f4.svg"),
    svg_entry!("stretch-arms-light-f1.svg"),
    svg_entry!("stretch-arms-dark-f1.svg"),
    svg_entry!("stretch-arms-light-f2.svg"),
    svg_entry!("stretch-arms-dark-f2.svg"),
    svg_entry!("stretch-arms-light-f3.svg"),
    svg_entry!("stretch-arms-dark-f3.svg"),
    svg_entry!("window-far-light-f1.svg"),
    svg_entry!("window-far-dark-f1.svg"),
    svg_entry!("window-far-light-f2.svg"),
    svg_entry!("window-far-dark-f2.svg"),
    svg_entry!("window-far-light-f3.svg"),
    svg_entry!("window-far-dark-f3.svg"),
];

/// Logical illustration size for the short-break card.
pub const SHORT_BREAK_ART_LOGICAL_PX: f64 = 56.0;
/// Logical illustration size for long-break suggestion slots.
pub const SUGGESTION_ART_LOGICAL_PX: f64 = 40.0;

/// Converts a logical art size and monitor DPI scale factor into a physical pixel size (`>= 1`).
#[must_use]
pub fn scaled_art_px(logical_px: f64, scale: f64) -> u32 {
    let safe_scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let px = (logical_px * safe_scale).round();
    if px >= 1.0 { px as u32 } else { 1 }
}

/// Maps `dark` mode flag to `damt_art::Theme`.
#[must_use]
pub const fn art_theme(dark: bool) -> Theme {
    if dark { Theme::Dark } else { Theme::Light }
}

/// Computes the frame to display at `elapsed` and the delay until the next step (or `None` when
/// `reduced_motion` is active or the sequencer has no next change).
#[must_use]
pub fn frame_and_next_delay(
    sequencer: &FrameSequencer,
    elapsed: Duration,
    reduced_motion: bool,
    frozen_frame: u32,
) -> (u32, Option<Duration>) {
    if reduced_motion {
        return (frozen_frame, None);
    }
    let frame = sequencer.get_frame(elapsed);
    let next = sequencer.next_change_after(elapsed);
    if next.is_zero() {
        (frame, None)
    } else {
        (frame, Some(next))
    }
}

/// Pure stepped-animation schedule for a single long-break suggestion slot.
#[derive(Debug, Clone)]
pub struct SuggestionSlot {
    pub exercise_id: String,
    pub sequencer: FrameSequencer,
    pub frozen_frame: u32,
    pub last_frame: u32,
}

/// Shared stepped-animation schedule driving up to two long-break suggestion illustrations via a
/// single timer scheduled by whichever slot changes soonest.
#[derive(Debug, Clone)]
pub struct SuggestionSchedule {
    slots: [Option<SuggestionSlot>; 2],
    reduced_motion: bool,
}

impl SuggestionSchedule {
    /// Builds a schedule from up to two optional `(exercise_id, sequencer, frozen_frame)` slots.
    #[must_use]
    pub fn new(
        slot1: Option<(String, FrameSequencer, u32)>,
        slot2: Option<(String, FrameSequencer, u32)>,
        reduced_motion: bool,
    ) -> Self {
        let make = |item: Option<(String, FrameSequencer, u32)>| {
            item.map(|(exercise_id, sequencer, frozen_frame)| SuggestionSlot {
                exercise_id,
                sequencer,
                frozen_frame,
                last_frame: 0,
            })
        };
        Self {
            slots: [make(slot1), make(slot2)],
            reduced_motion,
        }
    }

    /// Computes initial frames for both slots at `Duration::ZERO` and the delay until the soonest
    /// frame step (or `None` under reduced motion or when no slot animates).
    #[must_use]
    pub fn initial(&mut self) -> ([Option<u32>; 2], Option<Duration>) {
        self.step(Duration::ZERO, true)
    }

    /// Advances the schedule at `elapsed`, returning `Some(frame)` for any slot whose frame changed
    /// since the previous call, plus the delay until the soonest next step across active slots.
    #[must_use]
    pub fn advance_at(&mut self, elapsed: Duration) -> ([Option<u32>; 2], Option<Duration>) {
        self.step(elapsed, false)
    }

    /// Returns the exercise id for slot `index` (`0` or `1`), if present.
    #[must_use]
    pub fn exercise_id(&self, index: usize) -> Option<&str> {
        self.slots
            .get(index)
            .and_then(|s| s.as_ref().map(|slot| slot.exercise_id.as_str()))
    }

    fn step(
        &mut self,
        elapsed: Duration,
        force_emit: bool,
    ) -> ([Option<u32>; 2], Option<Duration>) {
        let mut updates = [None, None];
        let mut soonest: Option<Duration> = None;

        for (idx, slot_opt) in self.slots.iter_mut().enumerate() {
            let Some(slot) = slot_opt.as_mut() else {
                continue;
            };
            let (frame, next) = frame_and_next_delay(
                &slot.sequencer,
                elapsed,
                self.reduced_motion,
                slot.frozen_frame,
            );
            if force_emit || frame != slot.last_frame {
                slot.last_frame = frame;
                updates[idx] = Some(frame);
            }
            if let Some(delay) = next {
                soonest = Some(match soonest {
                    Some(prev) => prev.min(delay),
                    None => delay,
                });
            }
        }

        (updates, soonest)
    }
}

/// Embedded Ocy exercise art manifest, SVG frames, and raster cache.
pub struct OcyArt {
    manifest: Manifest,
    source: EmbeddedSource,
    cache: ArtCache,
    tracked_cache_keys: Mutex<VecDeque<(String, Theme, u32, u32)>>,
}

impl fmt::Debug for OcyArt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OcyArt")
            .field("exercise_count", &self.manifest.exercises.len())
            .field("cache_bytes", &self.cache.bytes_used())
            .field("cache_entries", &self.cache_entry_count())
            .finish_non_exhaustive()
    }
}

impl OcyArt {
    /// Parses the embedded manifest and binds all embedded exercise SVG frames.
    pub fn from_embedded() -> Result<Self, ArtError> {
        let manifest = Manifest::parse(MANIFEST_JSON)?;
        Ok(Self {
            manifest,
            source: EmbeddedSource::new(EMBEDDED_SVGS),
            cache: ArtCache::new(),
            tracked_cache_keys: Mutex::new(VecDeque::new()),
        })
    }

    /// Creates an empty [`OcyArt`] fallback with no manifest entries or frames.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            manifest: Manifest {
                version: String::new(),
                palette: String::new(),
                view_box: Vec::new(),
                exercises: BTreeMap::new(),
            },
            source: EmbeddedSource::new(&[]),
            cache: ArtCache::new(),
            tracked_cache_keys: Mutex::new(VecDeque::new()),
        }
    }

    /// Returns the manifest entry for `exercise_id`, if present (placeholder art resolves
    /// through [`art_id`]).
    #[must_use]
    pub fn exercise_info(&self, exercise_id: &str) -> Option<&damt_art::ExerciseInfo> {
        self.manifest.get(art_id(exercise_id))
    }

    /// Builds a [`FrameSequencer`] for `exercise_id`, freezing to `frozen` when `reduced_motion` is true.
    pub fn sequencer_for(
        &self,
        exercise_id: &str,
        reduced_motion: bool,
    ) -> Option<(FrameSequencer, u32)> {
        let info = self.manifest.get(art_id(exercise_id))?;
        let step = Duration::from_millis(u64::from(info.step_ms));
        let mut seq = FrameSequencer::new(&info.sequence, step).ok()?;
        if reduced_motion {
            seq = seq.with_reduced_motion(info.frozen);
        }
        Some((seq, info.frozen))
    }

    fn get_cached_raster(
        &self,
        exercise_id: &str,
        theme: Theme,
        frame: u32,
        size_px: u32,
    ) -> Option<Result<Arc<damt_art::Image>, ArtError>> {
        let res = self
            .cache
            .get(&self.source, exercise_id, theme, frame, size_px)?;
        if res.is_ok() {
            let capacity = ArtCache::default_capacity();
            if capacity > 0 {
                let mut keys = self
                    .tracked_cache_keys
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let key = (exercise_id.to_owned(), theme, frame, size_px);
                if let Some(pos) = keys.iter().position(|existing| existing == &key) {
                    keys.remove(pos);
                    keys.push_back(key);
                } else {
                    if keys.len() >= capacity {
                        keys.pop_front();
                    }
                    keys.push_back(key);
                }
            }
        }
        Some(res)
    }

    /// Rasterises `(exercise_id, theme, frame)` at `size_px` using the internal [`ArtCache`].
    pub fn render_frame(
        &self,
        exercise_id: &str,
        theme: Theme,
        frame: u32,
        size_px: u32,
    ) -> Option<Result<Image, ArtError>> {
        let raster = self.get_cached_raster(art_id(exercise_id), theme, frame, size_px)?;
        Some(raster.map(|img| {
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                &img.pixels,
                img.width,
                img.height,
            );
            Image::from_rgba8_premultiplied(buffer)
        }))
    }

    /// Rasterises the manifest's `frozen` frame for `exercise_id` at `logical_px * scale`.
    pub fn render_frozen(
        &self,
        exercise_id: &str,
        theme: Theme,
        logical_px: f64,
        scale: f64,
    ) -> Option<Result<Image, ArtError>> {
        let info = self.manifest.get(art_id(exercise_id))?;
        let size_px = scaled_art_px(logical_px, scale);
        self.render_frame(exercise_id, theme, info.frozen, size_px)
    }

    /// Clears all cached rasterized frames so idle memory stays minimal.
    pub fn clear_cache(&self) {
        self.cache.clear();
        self.tracked_cache_keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    /// Pre-rasterises eligible short-break eye-exercise frames into the internal [`ArtCache`].
    pub fn prewarm_short_break(
        &self,
        theme: Theme,
        size_px: u32,
        reduced_motion: bool,
        has_window_nearby: bool,
    ) {
        for exercise in ocy_core::exercises::ExerciseCatalogue::ALL {
            if exercise.kind != ocy_core::exercises::ExerciseKind::Eyes {
                continue;
            }
            if !has_window_nearby && exercise.requires_window {
                continue;
            }
            let Some(info) = self.manifest.get(art_id(exercise.id)) else {
                continue;
            };
            if reduced_motion {
                let _ = self.get_cached_raster(art_id(exercise.id), theme, info.frozen, size_px);
            } else {
                for frame in 1..=info.frames {
                    let _ = self.get_cached_raster(art_id(exercise.id), theme, frame, size_px);
                }
            }
        }
    }

    /// Pre-rasterises frozen suggestion frames for long breaks into the internal [`ArtCache`].
    pub fn prewarm_long_break(&self, theme: Theme, logical_px: f64, scale: f64) {
        self.prewarm_long_break_frames(theme, logical_px, scale, true);
    }

    /// Pre-rasterises suggestion frames (all frames when `!reduced_motion`, else frozen) for long breaks.
    pub fn prewarm_long_break_frames(
        &self,
        theme: Theme,
        logical_px: f64,
        scale: f64,
        reduced_motion: bool,
    ) {
        let size_px = scaled_art_px(logical_px, scale);
        for exercise in ocy_core::exercises::ExerciseCatalogue::ALL {
            if let Some(info) = self.manifest.get(art_id(exercise.id)) {
                if reduced_motion {
                    let _ =
                        self.get_cached_raster(art_id(exercise.id), theme, info.frozen, size_px);
                } else {
                    for frame in 1..=info.frames {
                        let _ = self.get_cached_raster(art_id(exercise.id), theme, frame, size_px);
                    }
                }
            }
        }
    }

    /// Returns the number of cached pixel bytes currently held.
    #[must_use]
    pub fn cache_bytes_used(&self) -> usize {
        self.cache.bytes_used()
    }

    /// Returns the number of cached raster entries currently held.
    #[must_use]
    pub fn cache_entry_count(&self) -> usize {
        self.tracked_cache_keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}
