//! A11Y-1: what Narrator hears when a break card appears (the cards never take focus, so the app
//! raises a UI Automation notification with this text).
use ocy_app::break_logic::{
    break_announcement, warning_announcement, warning_buttons, warning_heading,
    warning_postpone_label,
};
use ocy_app::strings::Strings;
use std::time::Duration;

#[test]
fn parts_join_as_sentences_without_doubling_punctuation() {
    assert_eq!(
        break_announcement(&["Look left and right", "Keep your head still."]),
        "Look left and right. Keep your head still."
    );
    assert_eq!(break_announcement(&["Done.", "Next"]), "Done. Next");
    assert_eq!(
        break_announcement(&["左右を見る。", "頭を動かさずに"]),
        "左右を見る。 頭を動かさずに"
    );
    assert_eq!(break_announcement(&["", "  only  ", ""]), "only");
    assert_eq!(break_announcement(&[]), "");
}

#[test]
fn warning_announces_only_the_buttons_it_shows() {
    let s = Strings::from_language("en");
    let heading = warning_heading(&s, Duration::from_secs(42));
    let postpone = warning_postpone_label(&s, 3);
    let (start, skip, strict) = (
        s.get("warning.startNow"),
        s.get("warning.skip"),
        s.get("warning.strictOn"),
    );

    let normal = warning_announcement(
        &heading,
        warning_buttons(false, 3),
        start,
        &postpone,
        skip,
        strict,
    );
    assert!(normal.starts_with(&heading), "{normal}");
    for part in [start, postpone.as_str(), skip] {
        assert!(normal.contains(part), "{normal} lacks {part}");
    }
    assert!(!normal.contains(strict), "{normal}");

    let no_postpones = warning_announcement(
        &heading,
        warning_buttons(false, 0),
        start,
        &postpone,
        skip,
        strict,
    );
    assert!(!no_postpones.contains(postpone.as_str()), "{no_postpones}");

    let strict_mode = warning_announcement(
        &heading,
        warning_buttons(true, 3),
        start,
        &postpone,
        skip,
        strict,
    );
    assert!(
        strict_mode.contains(strict) && strict_mode.contains(start),
        "{strict_mode}"
    );
    assert!(
        !strict_mode.contains(skip) && !strict_mode.contains(postpone.as_str()),
        "{strict_mode}"
    );
}
