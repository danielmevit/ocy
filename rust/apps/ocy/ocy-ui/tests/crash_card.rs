//! CR3: the crash-report card renders in every review state and language, light and dark.
#![deny(clippy::unwrap_used, clippy::expect_used)]

mod support;

use support::{CrashCardState, crash_card_shot, crash_settings_shot, init_platform};

#[test]
fn crash_card_renders_every_state() -> Result<(), String> {
    init_platform();
    for dark in [false, true] {
        for lang in ["en", "de", "ja"] {
            let card = crash_card_shot(lang, dark, CrashCardState::One)?;
            let several = crash_card_shot(lang, dark, CrashCardState::SeveralTicked)?;
            let preview = crash_card_shot(lang, dark, CrashCardState::Preview)?;
            // 392 px window at 2x; the count line makes the card taller, the preview taller still.
            assert_eq!(card.width, 784);
            assert!(several.height > card.height, "{lang}: count line missing");
            assert!(preview.height > card.height, "{lang}: preview not open");
            let row = crash_settings_shot(lang, dark, 560.0)?;
            assert!(row.height >= 1280);
        }
        let long = crash_card_shot("en", dark, CrashCardState::LongLabels)?;
        assert_eq!(long.width, 784);
    }
    Ok(())
}
