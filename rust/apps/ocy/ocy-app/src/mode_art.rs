//! Embedded mode illustrations for the Settings header and preset cards (placeholder set).
//!
//! The SVGs live in `assets/illustrations/modes/svg/` and the timing in its `manifest.json`; the
//! final art replaces those files only (see that folder's README). Frames follow the exercise
//! art's naming, `<id>-<theme>-f<N>.svg`, with ids `scene-<mode>` and `glyph-<mode>`.

use std::time::Duration;

use damt_art::{ArtError, EmbeddedSource, FrameSequencer, FrameSource, Manifest, Raster, Theme};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const MANIFEST_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../assets/illustrations/modes/manifest.json"
));

macro_rules! svg_entry {
    ($name:expr) => {
        (
            $name,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../assets/illustrations/modes/svg/",
                $name
            )) as &[u8],
        )
    };
}

macro_rules! mode_entries {
    ($($mode:literal),*) => {
        &[$(
            svg_entry!(concat!("scene-", $mode, "-light-f1.svg")),
            svg_entry!(concat!("scene-", $mode, "-light-f2.svg")),
            svg_entry!(concat!("scene-", $mode, "-light-f3.svg")),
            svg_entry!(concat!("scene-", $mode, "-light-f4.svg")),
            svg_entry!(concat!("scene-", $mode, "-dark-f1.svg")),
            svg_entry!(concat!("scene-", $mode, "-dark-f2.svg")),
            svg_entry!(concat!("scene-", $mode, "-dark-f3.svg")),
            svg_entry!(concat!("scene-", $mode, "-dark-f4.svg")),
            svg_entry!(concat!("glyph-", $mode, "-light-f1.svg")),
            svg_entry!(concat!("glyph-", $mode, "-dark-f1.svg")),
        )*]
    };
}

const EMBEDDED_SVGS: &[(&str, &[u8])] = mode_entries!("gentle", "balanced", "focused", "custom");

/// Mode ids in preset order (`0` Gentle, `1` Balanced, `2` Focused, `3` Custom / "Yours").
pub const MODES: [&str; 4] = ["gentle", "balanced", "focused", "custom"];
/// Scene frames per mode (the Slint `mode-scenes` array is mode-major, `MODES.len() * SCENE_FRAMES`).
pub const SCENE_FRAMES: u32 = 4;
/// Logical width of the header scene (160 x 110 view box).
pub const SCENE_LOGICAL_WIDTH: f64 = 160.0;
/// Logical size of a preset-card glyph.
pub const GLYPH_LOGICAL_PX: f64 = 20.0;

/// Embedded mode art: manifest plus SVG frames.
pub struct ModeArt {
    manifest: Manifest,
    source: EmbeddedSource,
}

impl std::fmt::Debug for ModeArt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModeArt")
            .field("entries", &self.manifest.exercises.len())
            .finish_non_exhaustive()
    }
}

fn to_slint(img: &damt_art::Image) -> Image {
    let buffer =
        SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&img.pixels, img.width, img.height);
    Image::from_rgba8_premultiplied(buffer)
}

fn physical(logical: f64, scale: f64) -> u32 {
    crate::art::scaled_art_px(logical, scale)
}

impl ModeArt {
    /// Parses the embedded manifest and binds the embedded SVGs.
    pub fn from_embedded() -> Result<Self, ArtError> {
        Ok(Self {
            manifest: Manifest::parse(MANIFEST_JSON)?,
            source: EmbeddedSource::new(EMBEDDED_SVGS),
        })
    }

    /// The shared embedded set (`None`, after logging once by the caller, if it fails to parse).
    #[must_use]
    pub fn shared() -> Option<&'static Self> {
        static ART: std::sync::OnceLock<Option<ModeArt>> = std::sync::OnceLock::new();
        ART.get_or_init(|| Self::from_embedded().ok()).as_ref()
    }

    /// Frame sequencer for `preset`'s header idle loop (`scene-<mode>` timing) and its frozen frame.
    #[must_use]
    pub fn scene_sequencer(&self, preset: i32) -> Option<(FrameSequencer, u32)> {
        let mode = MODES.get(usize::try_from(preset).ok()?)?;
        let info = self.manifest.get(&format!("scene-{mode}"))?;
        let seq = FrameSequencer::new(
            &info.sequence,
            Duration::from_millis(u64::from(info.step_ms)),
        )
        .ok()?;
        Some((seq, info.frozen))
    }

    /// Rasterises every scene frame (mode-major) at `scale`.
    pub fn scenes(&self, theme: Theme, scale: f64) -> Result<Vec<Image>, ArtError> {
        let width = physical(SCENE_LOGICAL_WIDTH, scale);
        let mut out = Vec::with_capacity(MODES.len() * SCENE_FRAMES as usize);
        for mode in MODES {
            for frame in 1..=SCENE_FRAMES {
                let id = format!("scene-{mode}");
                let svg = self
                    .source
                    .svg(&id, theme, frame)
                    .ok_or_else(|| ArtError::Manifest(format!("{id} frame {frame} missing")))?;
                out.push(to_slint(&Raster::render_width(&svg, width)?));
            }
        }
        Ok(out)
    }

    /// Rasterises the four card glyphs at `scale`.
    pub fn glyphs(&self, theme: Theme, scale: f64) -> Result<Vec<Image>, ArtError> {
        let size = physical(GLYPH_LOGICAL_PX, scale);
        MODES
            .iter()
            .map(|mode| {
                let id = format!("glyph-{mode}");
                let svg = self
                    .source
                    .svg(&id, theme, 1)
                    .ok_or_else(|| ArtError::Manifest(format!("{id} missing")))?;
                Ok(to_slint(&Raster::render(&svg, size)?))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_has_scene_frames_glyphs_and_timing() {
        let art = ModeArt::from_embedded().unwrap();
        for theme in [Theme::Light, Theme::Dark] {
            let scenes = art.scenes(theme, 1.0).unwrap();
            assert_eq!(scenes.len(), 16);
            assert_eq!(scenes[0].size().width, 160);
            assert_eq!(scenes[0].size().height, 110);
            assert_eq!(art.glyphs(theme, 1.5).unwrap()[0].size().width, 30);
        }
        for preset in 0..4 {
            let (_, frozen) = art.scene_sequencer(preset).unwrap();
            assert_eq!(frozen, 1);
        }
        assert!(art.scene_sequencer(4).is_none());
    }
}
