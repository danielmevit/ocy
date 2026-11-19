# C# to Rust parity

| CsFile.Method | rust_fn |
| --- | --- |
| `FrameSequencerTests.SequenceOrder_AdvancesThroughSequence` | `tests::sequence_order_advances` |
| `FrameSequencerTests.Looping_LoopsBackToStartOfSequence` | `tests::looping_loops_back` |
| `FrameSequencerTests.Holds_HoldsFramesWithNoInterpolation` | `tests::holds_frames` |
| `FrameSequencerTests.EdgeCases_HandleNegativeElapsedAndZeroStep` | `tests::negative_elapsed_and_zero_step` |
| `FrameSequencerTests.InvalidArguments_ThrowExpectedExceptions` | `tests::invalid_arguments` |
| `IllustrationManifestTests.EmbeddedManifest_LoadsExpectedExercises` | `tests::real_manifest_loads_all_exercises_and_files` |
| `IllustrationManifestTests.TryGet_MissingOrEmptyId_ReturnsFalse` | `tests::get_missing_empty_and_no_art_ids` |
| `IllustrationManifestTests.Parse_CustomJson_ParsesCorrectly` | `tests::parse_custom_json` |
