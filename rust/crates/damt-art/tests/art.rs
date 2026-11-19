use damt_art::{
    ArtCache, DirSource, EmbeddedSource, FrameSequencer, FrameSource, Manifest, Raster, Theme,
};
use proptest::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_manifest_dir() -> PathBuf {
    crate_dir()
        .join("../../..")
        .join("assets/illustrations/ocy-v1f")
}

fn svg_dir() -> PathBuf {
    repo_manifest_dir().join("svg")
}

#[test]
fn sequence_order_advances() {
    let sequence = [1, 2, 3];
    let step = Duration::from_millis(100);
    let sequencer = FrameSequencer::new(&sequence, step).expect("valid sequencer");
    assert_eq!(sequencer.get_frame(Duration::ZERO), 1);
    assert_eq!(sequencer.get_frame(Duration::from_millis(50)), 1);
    assert_eq!(sequencer.get_frame(Duration::from_millis(100)), 2);
    assert_eq!(sequencer.get_frame(Duration::from_millis(150)), 2);
    assert_eq!(sequencer.get_frame(Duration::from_millis(200)), 3);
    assert_eq!(sequencer.get_frame(Duration::from_millis(299)), 3);
}

#[test]
fn looping_loops_back() {
    let sequence = [1, 2, 3];
    let step = Duration::from_millis(100);
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(300)),
        1
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(400)),
        2
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(500)),
        3
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(600)),
        1
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(1000)),
        2
    );
}

#[test]
fn holds_frames() {
    let sequence = [1, 1, 1, 1, 1, 2, 3, 3, 2];
    let step = Duration::from_millis(280);
    for ms in (0..1400).step_by(70) {
        assert_eq!(
            FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(ms)),
            1
        );
    }
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(1400)),
        2
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(1600)),
        2
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(1680)),
        3
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(2000)),
        3
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(2240)),
        2
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::from_millis(2520)),
        1
    );
}

#[test]
fn negative_elapsed_and_zero_step() {
    let sequence = [1, 2, 3];
    let step = Duration::from_millis(100);
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, step, Duration::ZERO),
        1
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, Duration::ZERO, Duration::from_secs(5)),
        1
    );
    assert_eq!(
        FrameSequencer::get_frame_static(&sequence, Duration::from_millis(100), Duration::ZERO),
        1
    );
}

#[test]
fn invalid_arguments() {
    assert!(FrameSequencer::new(&[], Duration::from_secs(1)).is_err());
}

#[test]
fn real_manifest_loads_all_exercises_and_files() {
    let manifest_path = repo_manifest_dir().join("manifest.json");
    let json = fs::read_to_string(manifest_path).expect("real manifest exists");
    let manifest = Manifest::parse(&json).expect("manifest is valid");
    assert_eq!(manifest.version, "v1f");
    assert_eq!(manifest.palette, "green");
    assert_eq!(manifest.view_box, vec![0, 0, 64, 64]);
    assert_eq!(manifest.exercises.len(), 12);

    for (id, info) in &manifest.exercises {
        for theme in [Theme::Light, Theme::Dark] {
            for frame in 1..=info.frames {
                let path = svg_dir().join(format!("{id}-{}-f{frame}.svg", theme.suffix()));
                assert!(path.is_file(), "{} missing", path.display());
            }
        }
    }
}

#[test]
fn get_missing_empty_and_no_art_ids() {
    let manifest = Manifest::parse(
        r#"{"exercises":{"test":{"frames":1,"sequence":[1],"stepMs":300,"frozen":1}}}"#,
    )
    .expect("valid manifest");
    assert!(manifest.get("nonexistent").is_none());
    assert!(manifest.get("").is_none());
    assert!(manifest.get("test-no-art").is_none());
}

#[test]
fn parse_custom_json() {
    let json = r#"{"version":"test","palette":"blue","viewBox":[0,0,100,100],"exercises":{"test-ex":{"frames":2,"sequence":[1,2],"stepMs":150,"frozen":2}}}"#;
    let manifest = Manifest::parse(json).expect("valid manifest");
    assert_eq!(manifest.version, "test");
    assert_eq!(manifest.palette, "blue");
    assert_eq!(manifest.view_box, vec![0, 0, 100, 100]);
    let info = manifest.get("test-ex").expect("exercise exists");
    assert_eq!(info.frames, 2);
    assert_eq!(info.step_ms, 150);
    assert_eq!(info.frozen, 2);
    assert_eq!(info.sequence, vec![1, 2]);
}

#[test]
fn invalid_manifest_sequence_frozen_and_step() {
    assert!(
        Manifest::parse(
            r#"{"exercises":{"x":{"frames":2,"sequence":[0,1],"stepMs":300,"frozen":1}}}"#
        )
        .is_err()
    );
    assert!(
        Manifest::parse(
            r#"{"exercises":{"x":{"frames":2,"sequence":[1,3],"stepMs":300,"frozen":1}}}"#
        )
        .is_err()
    );
    assert!(
        Manifest::parse(
            r#"{"exercises":{"x":{"frames":2,"sequence":[1,2],"stepMs":300,"frozen":3}}}"#
        )
        .is_err()
    );

    let manifest = Manifest::parse(
        r#"{"exercises":{"x":{"frames":2,"sequence":[1,2],"stepMs":0,"frozen":1}}}"#,
    )
    .expect("zero step defaults");
    assert_eq!(manifest.get("x").expect("exercise").step_ms, 300);
}

proptest! {
    #[test]
    fn next_change_property(sequence in prop::collection::vec(1u32..=8, 1..=16), step_ms in 1u64..5000, elapsed_ms in 0u64..100_000) {
        let step = Duration::from_millis(step_ms);
        let elapsed = Duration::from_millis(elapsed_ms);
        let sequencer = FrameSequencer::new(&sequence, step).expect("valid sequence");
        let change = sequencer.next_change_after(elapsed);
        prop_assert!(!change.is_zero());
        let before = sequencer.get_frame(elapsed);
        let after_elapsed = elapsed.saturating_add(change);
        let after = sequencer.get_frame(after_elapsed);
        prop_assert!(!sequence.is_empty());
        prop_assert!(change <= step);
        prop_assert!(sequence.contains(&before));
        prop_assert!(sequence.contains(&after));
    }

    #[test]
    fn reduced_motion_never_changes(frame in 1u32..=8, elapsed_ms in 0u64..100_000) {
        let sequencer = FrameSequencer::new(&[frame], Duration::from_millis(300))
            .expect("valid")
            .with_reduced_motion(frame);
        assert_eq!(sequencer.get_frame(Duration::from_millis(elapsed_ms)), frame);
    }
}

#[test]
fn real_raster_frame_checks() {
    let path = svg_dir().join("look-left-right-light-f2.svg");
    let svg = fs::read(path).expect("real frame");
    let image = Raster::render(&svg, 112).expect("raster render");
    assert_eq!((image.width, image.height), (112, 112));

    let opaque = image
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .count();
    assert!(opaque as f32 / (112 * 112) as f32 > 0.15);

    let mut found_outline = false;
    let mut found_mint = false;
    for pixel in image.pixels.chunks_exact(4) {
        if pixel[3] > 0
            && pixel[0].abs_diff(0x1B) <= 4
            && pixel[1].abs_diff(0x1E) <= 4
            && pixel[2].abs_diff(0x1F) <= 4
        {
            found_outline = true;
        }
        if pixel[3] > 0
            && pixel[0].abs_diff(0xDC) <= 4
            && pixel[1].abs_diff(0xE9) <= 4
            && pixel[2].abs_diff(0xE4) <= 4
        {
            found_mint = true;
        }
    }
    assert!(found_outline);
    assert!(found_mint);

    let dark = fs::read(svg_dir().join("look-left-right-dark-f2.svg")).expect("dark frame");
    let dark_image = Raster::render(&dark, 112).expect("dark render");
    assert_ne!(image.pixels, dark_image.pixels);
}

#[test]
fn synthetic_artboard_position_is_not_derived_from_content() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
        <circle cx="8" cy="8" r="2" fill="#000000"/>
    </svg>"##
        .as_bytes();

    for (size, expected) in [(64, 8.0_f64), (128, 16.0_f64)] {
        let image = Raster::render(svg, size).expect("synthetic raster render");
        let mut weighted_x = 0.0_f64;
        let mut weighted_y = 0.0_f64;
        let mut weight = 0.0_f64;
        for y in 0..size {
            for x in 0..size {
                let index = ((y * size + x) * 4) as usize;
                let alpha = image.pixels[index + 3] as f64;
                weighted_x += (x as f64 + 0.5) * alpha;
                weighted_y += (y as f64 + 0.5) * alpha;
                weight += alpha;
            }
        }
        assert!(weight > 0.0, "circle was not rendered at {size}px");
        let tolerance = if size == 64 { 0.5 } else { 1.0 };
        assert!(
            (weighted_x / weight - expected).abs() <= tolerance,
            "x centroid at {size}px: {}, expected {expected}",
            weighted_x / weight
        );
        assert!(
            (weighted_y / weight - expected).abs() <= tolerance,
            "y centroid at {size}px: {}, expected {expected}",
            weighted_y / weight
        );
    }
}

#[test]
fn look_left_right_head_outline_is_consistent_across_frames() {
    let mut boxes = Vec::new();
    for frame in 1..=3 {
        let path = svg_dir().join(format!("look-left-right-light-f{frame}.svg"));
        let svg = fs::read(path).expect("real frame");
        let image = Raster::render(&svg, 112).expect("raster render");
        let mut min_x = 112_u32;
        let mut min_y = 112_u32;
        let mut max_x = 0_u32;
        let mut max_y = 0_u32;
        let mut found = false;
        for y in 30..112 {
            for x in 0..112 {
                let index = ((y * 112 + x) * 4) as usize;
                let pixel = &image.pixels[index..index + 4];
                if pixel[3] > 0
                    && pixel[0].abs_diff(0x1B) <= 4
                    && pixel[1].abs_diff(0x1E) <= 4
                    && pixel[2].abs_diff(0x1F) <= 4
                {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        assert!(found, "frame {frame} has no head outline");
        boxes.push((min_x, min_y, max_x, max_y));
    }

    let baseline = boxes.first().expect("baseline box");
    for current_box in &boxes[1..] {
        for (actual, baseline_value) in [
            (current_box.0, baseline.0),
            (current_box.1, baseline.1),
            (current_box.2, baseline.2),
            (current_box.3, baseline.3),
        ] {
            assert!(
                actual.abs_diff(baseline_value) <= 2,
                "frame head outline differs: boxes {boxes:?}"
            );
        }
    }
}

#[test]
fn golden_image_matches() {
    let svg = fs::read(svg_dir().join("look-left-right-light-f2.svg")).expect("real frame");
    let image = Raster::render(&svg, 112).expect("raster render");
    let golden_path = crate_dir().join("tests/golden/look-left-right-light-f2-112.png");

    let pixmap = resvg::tiny_skia::Pixmap::from_vec(
        image.pixels.clone(),
        resvg::tiny_skia::IntSize::from_wh(112, 112).expect("valid size"),
    )
    .expect("valid pixmap");
    fs::create_dir_all(golden_path.parent().expect("golden parent")).expect("golden dir");
    if !golden_path.is_file() {
        pixmap.save_png(&golden_path).expect("write golden PNG");
    }
    let golden_pixmap = resvg::tiny_skia::Pixmap::load_png(&golden_path).expect("load golden PNG");
    let golden_pixels = golden_pixmap.data();
    assert_eq!(golden_pixels.len(), image.pixels.len());
    let mut mismatches = 0;
    let mut max_delta = 0_u8;
    for (actual, expected) in image.pixels.iter().zip(golden_pixels) {
        let delta = actual.abs_diff(*expected);
        max_delta = max_delta.max(delta);
        if delta > 2 {
            mismatches += 1;
        }
    }
    assert_eq!(
        mismatches, 0,
        "golden per-channel tolerance exceeded: max delta {max_delta}"
    );
}

#[test]
fn cache_lru_clear_and_bytes() {
    let source = DirSource::new(svg_dir());
    let cache = ArtCache::with_capacity(2);
    assert_eq!(cache.bytes_used(), 0);
    let _ = cache.get(&source, "look-left-right", Theme::Light, 1, 32);
    let _ = cache.get(&source, "look-left-right", Theme::Light, 2, 32);
    assert!(cache.bytes_used() > 0);

    let first = cache
        .get(&source, "look-left-right", Theme::Light, 1, 32)
        .expect("first cached")
        .expect("rendered");
    let _ = cache.get(&source, "look-left-right", Theme::Light, 3, 32);
    let misses = cache
        .get(&source, "look-left-right", Theme::Light, 2, 32)
        .expect("second frame")
        .expect("rendered");
    assert_ne!(first.pixels, misses.pixels);

    cache.clear();
    assert_eq!(cache.bytes_used(), 0);
}

#[test]
fn embedded_source_reads_include_bytes_style_entries() {
    let source = EmbeddedSource::new(&[("test-light-f1.svg", b"<svg/>")]);
    assert!(source.svg("test", Theme::Light, 1).is_some());
    assert!(source.svg("test", Theme::Dark, 1).is_none());
}

/// Art that legitimately reaches the 64-unit artboard edge must be rendered, never trimmed.
/// (Lead fix: R3a2 had added a post-render loop that erased a 2 px border; 56 of 100 real frames have art there.)
#[test]
fn art_at_the_artboard_edge_is_not_clipped() {
    let svg = fs::read(svg_dir().join("figure-eight-light-f1.svg")).expect("figure-eight frame");
    let image = Raster::render(&svg, 112).expect("render");
    let mut edge_opaque = 0;
    for y in 0..112u32 {
        for x in 0..112u32 {
            if (x < 2 || y < 2 || x >= 110 || y >= 110)
                && image.pixels[((y * 112 + x) * 4 + 3) as usize] > 0
            {
                edge_opaque += 1;
            }
        }
    }
    assert!(
        edge_opaque > 0,
        "the figure-eight art at the artboard edge was clipped"
    );
}
