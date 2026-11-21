use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_platform::{Clock, PlatformServices, TaskbarThemeProvider, Unsupported};
use ocy_app::strings::Strings;
use ocy_app::tray::{
    AppTheme, PauseSource, TRAY_MENU_ORDER, TrayController, TrayIconState, TrayMenuEntry,
    TrayMenuSlot, TrayPresentation, TrayPresenter, TraySink, can_take_long_break_now,
    can_take_short_break_now,
};
use ocy_core::engine::{BreakEngine, BreakKind, CurrentBreakInfo, EnginePhase, EngineSnapshot};
use ocy_core::settings::OcySettings;
use ocy_core::time::{DateTimeOffset, FakeClock, Paused};

fn make_presenter() -> (TrayPresenter, Arc<FakeClock>) {
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 17, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    (TrayPresenter::new(strings, clock_dyn), clock)
}

fn make_presenter_with_offset(
    year: i16,
    month: i8,
    day: i8,
    hour: i8,
    minute: i8,
    second: i8,
    offset_hours: i8,
) -> (TrayPresenter, Arc<FakeClock>) {
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let clock = Arc::new(
        FakeClock::from_fixed_offset(year, month, day, hour, minute, second, offset_hours).unwrap(),
    );
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    (TrayPresenter::new(strings, clock_dyn), clock)
}

#[test]
fn working_phase_presents_correct_state() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: Some(Duration::from_secs(8 * 60)),
        time_until_long_break: Some(Duration::from_secs(45 * 60)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::Active);
    assert_eq!(presentation.tooltip, "Next break in 8 min");
    assert_eq!(
        presentation.take_short_break_now.text,
        "Take a short break now"
    );
    assert!(presentation.take_short_break_now.is_visible);
    assert!(presentation.take_short_break_now.is_enabled);
    assert!(presentation.take_long_break_now.is_enabled);
    assert!(!presentation.resume.is_visible);
    assert!(presentation.quit.is_enabled);
    assert!(presentation.settings.is_enabled);
}

#[test]
fn warning_phase_presents_correct_state() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::LongBreakWarning,
        time_until_short_break: Some(Duration::from_secs(5 * 60)),
        time_until_long_break: Some(Duration::from_secs(42)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::Attention);
    assert_eq!(presentation.tooltip, "Next break in 0:42");
    assert!(!presentation.take_short_break_now.is_enabled);
    assert!(presentation.take_long_break_now.is_enabled);
    assert!(!presentation.resume.is_visible);
    assert!(presentation.quit.is_enabled);
}

#[test]
fn short_break_phase_presents_correct_state() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::ShortBreak,
        time_until_short_break: Some(Duration::ZERO),
        time_until_long_break: Some(Duration::from_secs(30 * 60)),
        current_break: Some(CurrentBreakInfo {
            kind: BreakKind::Short,
            exercise_id: "blink".to_owned(),
            total_duration: Duration::from_secs(15),
            remaining_duration: Duration::from_secs(10),
            suggestion_exercise_ids: None,
        }),
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::OnBreak);
    assert_eq!(presentation.tooltip, "On a break");
    assert!(!presentation.take_short_break_now.is_enabled);
    assert!(!presentation.take_long_break_now.is_enabled);
    assert!(!presentation.resume.is_visible);
    assert!(presentation.pause_1_hour.is_enabled);
    assert!(presentation.quit.is_enabled);
}

#[test]
fn non_strict_long_break_presents_correct_state() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::LongBreak,
        time_until_short_break: None,
        time_until_long_break: Some(Duration::ZERO),
        current_break: Some(CurrentBreakInfo {
            kind: BreakKind::Long,
            exercise_id: "palming".to_owned(),
            total_duration: Duration::from_secs(5 * 60),
            remaining_duration: Duration::from_secs(4 * 60),
            suggestion_exercise_ids: None,
        }),
        paused_until_utc: None,
        postpones_left: 0,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::OnBreak);
    assert_eq!(presentation.tooltip, "On a break");
    assert!(!presentation.take_short_break_now.is_enabled);
    assert!(!presentation.take_long_break_now.is_enabled);
    assert!(!presentation.resume.is_visible);
    assert!(presentation.pause_1_hour.is_enabled);
    assert!(presentation.pause_3_hours.is_enabled);
    assert!(presentation.pause_until_resumed.is_enabled);
    assert!(presentation.settings.is_enabled);
    assert!(presentation.quit.is_enabled);
}

#[test]
fn strict_long_break_disables_all_menu_items_except_quit() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::LongBreak,
        time_until_short_break: None,
        time_until_long_break: Some(Duration::ZERO),
        current_break: Some(CurrentBreakInfo {
            kind: BreakKind::Long,
            exercise_id: "palming".to_owned(),
            total_duration: Duration::from_secs(5 * 60),
            remaining_duration: Duration::from_secs(4 * 60),
            suggestion_exercise_ids: None,
        }),
        paused_until_utc: None,
        postpones_left: 0,
        strict_mode: true,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::OnBreak);
    assert_eq!(presentation.tooltip, "On a break");
    assert!(!presentation.take_short_break_now.is_enabled);
    assert!(!presentation.take_long_break_now.is_enabled);
    assert!(!presentation.pause_1_hour.is_enabled);
    assert!(!presentation.pause_3_hours.is_enabled);
    assert!(!presentation.pause_until_resumed.is_enabled);
    assert!(!presentation.resume.is_enabled);
    assert!(!presentation.settings.is_enabled);
    assert!(presentation.quit.is_enabled);
}

#[test]
fn deferred_phase_presents_correct_state() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::Deferred,
        time_until_short_break: None,
        time_until_long_break: Some(Duration::ZERO),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::Active);
    assert!(presentation.take_short_break_now.is_enabled);
    assert!(presentation.take_long_break_now.is_enabled);
    assert!(!presentation.resume.is_visible);

    let snapshot_short_on = EngineSnapshot {
        time_until_short_break: Some(Duration::from_secs(60)),
        ..snapshot
    };
    let presentation_short_on = presenter.present(&snapshot_short_on, None);
    assert!(presentation_short_on.take_short_break_now.is_enabled);
}

#[test]
fn away_phase_presents_correct_state() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::Away,
        time_until_short_break: Some(Duration::from_secs(5 * 60)),
        time_until_long_break: Some(Duration::from_secs(35 * 60)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::Active);
    assert!(!presentation.take_short_break_now.is_enabled);
    assert!(!presentation.take_long_break_now.is_enabled);
    assert!(!presentation.resume.is_visible);
}

#[test]
fn paused_phase_shows_resume_menu_item() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::Paused,
        time_until_short_break: None,
        time_until_long_break: None,
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);

    assert_eq!(presentation.icon_state, TrayIconState::Paused);
    assert!(presentation.resume.is_visible);
    assert!(presentation.resume.is_enabled);
    assert!(!presentation.take_short_break_now.is_enabled);
    assert!(!presentation.take_long_break_now.is_enabled);
}

#[test]
fn paused_tooltip_indefinite_pause() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::Paused,
        time_until_short_break: None,
        time_until_long_break: None,
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);
    assert_eq!(presentation.tooltip, "Paused until you resume");

    let snapshot_max = EngineSnapshot {
        paused_until_utc: Some(Paused::UntilResumed),
        ..snapshot
    };
    let presentation_max = presenter.present(&snapshot_max, None);
    assert_eq!(presentation_max.tooltip, "Paused until you resume");
}

#[test]
fn paused_tooltip_timed_pause() {
    let (presenter, clock) = make_presenter();
    let now = DateTimeOffset::from_utc(clock.now().timestamp());
    let snapshot = EngineSnapshot {
        phase: EnginePhase::Paused,
        time_until_short_break: None,
        time_until_long_break: None,
        current_break: None,
        paused_until_utc: Some(Paused::Until(
            now.checked_add(Duration::from_secs(23 * 60)).unwrap(),
        )),
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    let presentation = presenter.present(&snapshot, None);
    assert_eq!(presentation.tooltip, "Paused for 23 min more");

    let snapshot_seconds = EngineSnapshot {
        paused_until_utc: Some(Paused::Until(
            now.checked_add(Duration::from_secs(42)).unwrap(),
        )),
        ..snapshot
    };
    let presentation_seconds = presenter.present(&snapshot_seconds, None);
    assert_eq!(presentation_seconds.tooltip, "Paused for 0:42 more");
}

#[test]
fn take_short_break_now_enable_rule_across_phases_and_parity_with_long_break() {
    let (presenter, clock) = make_presenter();
    let clock_dyn: Arc<dyn Clock> = clock;
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);

    // Every phase paired with whether short breaks are enabled (`time_until_short_break.is_some()`).
    let phases = [
        EnginePhase::Working,
        EnginePhase::LongBreakWarning,
        EnginePhase::ShortBreak,
        EnginePhase::LongBreak,
        EnginePhase::Deferred,
        EnginePhase::Paused,
        EnginePhase::Away,
    ];

    for phase in phases {
        for short_enabled in [true, false] {
            for strict_mode in [false, true] {
                let snapshot = EngineSnapshot {
                    phase,
                    time_until_short_break: short_enabled.then_some(Duration::from_secs(300)),
                    time_until_long_break: Some(Duration::from_secs(1200)),
                    current_break: None,
                    paused_until_utc: (phase == EnginePhase::Paused)
                        .then_some(Paused::UntilResumed),
                    postpones_left: 3,
                    strict_mode,
                    time_until_long_break_warning: None,
                    natural_hold: None,
                };
                let presentation = presenter.present(&snapshot, None);

                let expected_short = !(phase == EnginePhase::LongBreak && strict_mode)
                    && matches!(phase, EnginePhase::Working | EnginePhase::Deferred);
                let expected_short_helper = if phase == EnginePhase::LongBreak && strict_mode {
                    false
                } else {
                    can_take_short_break_now(&snapshot)
                };
                assert_eq!(
                    expected_short_helper, expected_short,
                    "phase={phase:?} short_enabled={short_enabled} strict={strict_mode}"
                );
                assert_eq!(
                    presentation.take_short_break_now.is_enabled, expected_short,
                    "phase={phase:?} short_enabled={short_enabled} strict={strict_mode}"
                );
                assert_eq!(
                    presentation.take_long_break_now.is_enabled,
                    !(phase == EnginePhase::LongBreak && strict_mode)
                        && can_take_long_break_now(&snapshot),
                    "long parity phase={phase:?} strict={strict_mode}"
                );
            }
        }
    }

    // Verify Working parity against a real BreakEngine instance.
    let mut engine_on = BreakEngine::with_defaults(
        OcySettings::default(),
        Arc::clone(&platform),
        Arc::clone(&clock_dyn),
    );
    let snap_on = engine_on.snapshot();
    assert!(
        presenter
            .present(&snap_on, None)
            .take_short_break_now
            .is_enabled
    );
    assert!(engine_on.start_short_break_now().0);
    // Once in ShortBreak, both tray items are disabled and start_short_break_now() returns false.
    let snap_in_short = engine_on.snapshot();
    let pres_in_short = presenter.present(&snap_in_short, None);
    assert!(!pres_in_short.take_short_break_now.is_enabled);
    assert!(!pres_in_short.take_long_break_now.is_enabled);
    assert!(!engine_on.start_short_break_now().0);

    // When short breaks are off in settings, tray short-break item stays enabled in Working
    // (matching "Take a long break now" and BreakEngine::start_short_break_now).
    let mut settings_off = OcySettings::default();
    settings_off.short_breaks.enabled = false;
    let mut engine_off = BreakEngine::with_defaults(settings_off, platform, clock_dyn);
    let pres_off = presenter.present(&engine_off.snapshot(), None);
    assert!(pres_off.take_short_break_now.is_enabled);
    assert!(pres_off.take_long_break_now.is_enabled);
    assert!(engine_off.start_short_break_now().0);
}

#[test]
fn tray_menu_item_order_places_pause_status_first_then_existing_order() {
    let (presenter, _) = make_presenter();
    let snapshot = EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: Some(Duration::from_secs(600)),
        time_until_long_break: Some(Duration::from_secs(3600)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };
    let presentation = presenter.present(&snapshot, None);

    assert_eq!(
        TRAY_MENU_ORDER,
        [
            TrayMenuSlot::PauseStatus,
            TrayMenuSlot::TakeShortBreakNow,
            TrayMenuSlot::TakeLongBreakNow,
            TrayMenuSlot::Separator1,
            TrayMenuSlot::Pause1Hour,
            TrayMenuSlot::Pause3Hours,
            TrayMenuSlot::PauseUntilResumed,
            TrayMenuSlot::Resume,
            TrayMenuSlot::Separator2,
            TrayMenuSlot::Settings,
            TrayMenuSlot::Quit,
        ]
    );

    let entries = presentation.ordered_menu_entries();
    assert_eq!(entries.len(), 11);
    assert!(matches!(
        entries[0],
        TrayMenuEntry::Item(TrayMenuSlot::PauseStatus, item)
            if !item.is_visible && !item.is_enabled && item.text.is_empty()
    ));
    assert!(matches!(
        entries[1],
        TrayMenuEntry::Item(TrayMenuSlot::TakeShortBreakNow, item)
            if item.text == "Take a short break now"
    ));
    assert!(matches!(
        entries[2],
        TrayMenuEntry::Item(TrayMenuSlot::TakeLongBreakNow, item)
            if item.text == "Take a long break now"
    ));
    assert_eq!(entries[3], TrayMenuEntry::Separator);
    assert!(matches!(
        entries[4],
        TrayMenuEntry::Item(TrayMenuSlot::Pause1Hour, item) if item.text == "Pause for 1 hour"
    ));
    assert!(matches!(
        entries[5],
        TrayMenuEntry::Item(TrayMenuSlot::Pause3Hours, item) if item.text == "Pause for 3 hours"
    ));
    assert!(matches!(
        entries[6],
        TrayMenuEntry::Item(TrayMenuSlot::PauseUntilResumed, item)
            if item.text == "Pause until resumed"
    ));
    assert!(matches!(
        entries[7],
        TrayMenuEntry::Item(TrayMenuSlot::Resume, item) if item.text == "Resume reminders"
    ));
    assert_eq!(entries[8], TrayMenuEntry::Separator);
    assert!(matches!(
        entries[9],
        TrayMenuEntry::Item(TrayMenuSlot::Settings, item) if item.text == "Settings"
    ));
    assert!(matches!(
        entries[10],
        TrayMenuEntry::Item(TrayMenuSlot::Quit, item) if item.text == "Quit Ocy"
    ));

    // Confirm the declarative Slint Menu in slint_tray.rs declares pause-status first, then
    // take-short-break right before take-long-break, and wires checkable/checked on pause items.
    let slint_src = include_str!("../src/slint_tray.rs");
    let pause_status_idx = slint_src
        .find("title: root.pause-status-text;")
        .expect("pause-status-text menu item");
    let short_idx = slint_src
        .find("title: root.take-short-break-text;")
        .expect("take-short-break-text menu item");
    let long_idx = slint_src
        .find("title: root.take-long-break-text;")
        .expect("take-long-break-text menu item");
    let first_sep_idx = slint_src.find("MenuSeparator {}").expect("first separator");
    assert!(pause_status_idx < short_idx && short_idx < long_idx && long_idx < first_sep_idx);
    assert!(slint_src.contains("checked <=> root.pause-1h-checked;"));
    assert!(slint_src.contains("checked <=> root.pause-3h-checked;"));
    assert!(slint_src.contains("checked <=> root.pause-until-resumed-checked;"));
    assert!(slint_src.contains("clicked => { root.tray-clicked(); }"));
}

#[test]
fn pause_status_line_formats_remainder_and_local_24h_clock_with_edge_cases() {
    // Local clock at 14:25:00+01:00 (UTC 13:25:00).
    let (presenter, clock) = make_presenter_with_offset(2026, 11, 19, 14, 25, 0, 1);

    let make_paused = |remaining_secs: u64| {
        let now_utc = DateTimeOffset::from_utc(clock.now().timestamp());
        EngineSnapshot {
            phase: EnginePhase::Paused,
            time_until_short_break: None,
            time_until_long_break: None,
            current_break: None,
            paused_until_utc: Some(Paused::Until(
                now_utc
                    .checked_add(Duration::from_secs(remaining_secs))
                    .unwrap(),
            )),
            postpones_left: 3,
            strict_mode: false,
            time_until_long_break_warning: None,
            natural_hold: None,
        }
    };

    // 1 h 15 min -> local 15:40 · 1 h 15 min left
    let p_75m = presenter.present(&make_paused(75 * 60), None);
    assert!(p_75m.pause_status.is_visible);
    assert!(!p_75m.pause_status.is_enabled);
    assert_eq!(
        p_75m.pause_status.text,
        "Paused until 15:40 \u{00b7} 1 h 15 min left"
    );

    // 45 min -> local 15:10 · 45 min left
    let p_45m = presenter.present(&make_paused(45 * 60), None);
    assert_eq!(
        p_45m.pause_status.text,
        "Paused until 15:10 \u{00b7} 45 min left"
    );

    // 59 s -> under 1 min
    let p_59s = presenter.present(&make_paused(59), None);
    assert_eq!(
        p_59s.pause_status.text,
        "Paused until 14:25 \u{00b7} under 1 min left"
    );

    // 0 s (boundary before expiry tick) -> under 1 min
    let p_0s = presenter.present(&make_paused(0), None);
    assert_eq!(
        p_0s.pause_status.text,
        "Paused until 14:25 \u{00b7} under 1 min left"
    );

    // Exactly 1 h (3600 s) -> 1 h 00 min
    let p_1h = presenter.present(&make_paused(3600), None);
    assert_eq!(
        p_1h.pause_status.text,
        "Paused until 15:25 \u{00b7} 1 h 00 min left"
    );

    // More than 3 h (e.g. 3 h 15 min) -> 3 h 15 min
    let p_gt3h = presenter.present(&make_paused(3 * 3600 + 15 * 60), None);
    assert_eq!(
        p_gt3h.pause_status.text,
        "Paused until 17:40 \u{00b7} 3 h 15 min left"
    );

    // Across midnight: local 23:30:00+01:00 + 1 h = 00:30 next day
    let (night_presenter, night_clock) = make_presenter_with_offset(2026, 11, 19, 23, 30, 0, 1);
    let night_until = DateTimeOffset::from_utc(night_clock.now().timestamp())
        .checked_add(Duration::from_secs(3600))
        .unwrap();
    let night_snapshot = EngineSnapshot {
        phase: EnginePhase::Paused,
        time_until_short_break: None,
        time_until_long_break: None,
        current_break: None,
        paused_until_utc: Some(Paused::Until(night_until)),
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };
    let p_midnight = night_presenter.present(&night_snapshot, None);
    assert_eq!(
        p_midnight.pause_status.text,
        "Paused until 00:30 \u{00b7} 1 h 00 min left"
    );

    // Indefinite pause -> "Paused until you resume"
    let indef_snapshot = EngineSnapshot {
        paused_until_utc: Some(Paused::UntilResumed),
        ..night_snapshot
    };
    let p_indef = night_presenter.present(&indef_snapshot, None);
    assert!(p_indef.pause_status.is_visible);
    assert!(!p_indef.pause_status.is_enabled);
    assert_eq!(p_indef.pause_status.text, "Paused until you resume");
}

#[test]
fn pause_status_line_is_hidden_in_every_non_paused_phase_and_visible_only_when_paused() {
    let (presenter, clock) = make_presenter();
    let now_utc = DateTimeOffset::from_utc(clock.now().timestamp());

    let non_paused_cases = [
        (EnginePhase::Working, false),
        (EnginePhase::LongBreakWarning, false),
        (EnginePhase::ShortBreak, false),
        (EnginePhase::LongBreak, false),
        (EnginePhase::LongBreak, true),
        (EnginePhase::Deferred, false),
        (EnginePhase::Away, false),
    ];

    for (phase, strict_mode) in non_paused_cases {
        let snapshot = EngineSnapshot {
            phase,
            time_until_short_break: Some(Duration::from_secs(300)),
            time_until_long_break: Some(Duration::from_secs(1200)),
            current_break: None,
            paused_until_utc: None,
            postpones_left: 3,
            strict_mode,
            time_until_long_break_warning: None,
            natural_hold: None,
        };
        let pres = presenter.present(&snapshot, None);
        assert!(
            !pres.pause_status.is_visible,
            "pause_status must be hidden in phase={phase:?} strict={strict_mode}"
        );
        assert!(!pres.pause_status.is_enabled);
        assert!(pres.pause_status.text.is_empty());
    }

    let paused_snapshot = EngineSnapshot {
        phase: EnginePhase::Paused,
        time_until_short_break: None,
        time_until_long_break: None,
        current_break: None,
        paused_until_utc: Some(Paused::Until(
            now_utc.checked_add(Duration::from_secs(1800)).unwrap(),
        )),
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };
    let pres_paused = presenter.present(&paused_snapshot, None);
    assert!(pres_paused.pause_status.is_visible);
    assert!(!pres_paused.pause_status.is_enabled);
    assert!(!pres_paused.pause_status.text.is_empty());
}

#[test]
fn active_pause_option_checkmark_tracks_source_clears_on_resume_or_expiry_and_handles_restart() {
    let (presenter, clock) = make_presenter();
    let now_utc = DateTimeOffset::from_utc(clock.now().timestamp());
    let timed_snapshot = |secs: u64| EngineSnapshot {
        phase: EnginePhase::Paused,
        time_until_short_break: None,
        time_until_long_break: None,
        current_break: None,
        paused_until_utc: Some(Paused::Until(
            now_utc.checked_add(Duration::from_secs(secs)).unwrap(),
        )),
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };
    let indef_snapshot = EngineSnapshot {
        phase: EnginePhase::Paused,
        time_until_short_break: None,
        time_until_long_break: None,
        current_break: None,
        paused_until_utc: Some(Paused::UntilResumed),
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };
    let working_snapshot = EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: Some(Duration::from_secs(600)),
        time_until_long_break: Some(Duration::from_secs(3600)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        time_until_long_break_warning: None,
        natural_hold: None,
    };

    // 1. Pause for 1 hour -> only pause_1_hour checked.
    presenter.record_pause(PauseSource::OneHour);
    let p1 = presenter.present(&timed_snapshot(3600), None);
    assert!(p1.pause_1_hour.is_checked);
    assert!(!p1.pause_3_hours.is_checked);
    assert!(!p1.pause_until_resumed.is_checked);

    // 2. Switch to Pause for 3 hours -> checkmark moves cleanly.
    presenter.record_pause(PauseSource::ThreeHours);
    let p3 = presenter.present(&timed_snapshot(3 * 3600), None);
    assert!(!p3.pause_1_hour.is_checked);
    assert!(p3.pause_3_hours.is_checked);
    assert!(!p3.pause_until_resumed.is_checked);

    // 3. Switch to Pause until resumed -> only pause_until_resumed checked.
    presenter.record_pause(PauseSource::UntilResumed);
    let pu = presenter.present(&indef_snapshot, None);
    assert!(!pu.pause_1_hour.is_checked);
    assert!(!pu.pause_3_hours.is_checked);
    assert!(pu.pause_until_resumed.is_checked);

    // 4. Resume or pause expiry -> no checkmarks AND remembered source cleared.
    presenter.record_pause(PauseSource::OneHour);
    let _ = presenter.present(&timed_snapshot(3600), None);
    let pw = presenter.present(&working_snapshot, None);
    assert!(!pw.pause_1_hour.is_checked);
    assert!(!pw.pause_3_hours.is_checked);
    assert!(!pw.pause_until_resumed.is_checked);
    assert_eq!(presenter.active_pause_source(), None);

    // Subsequent timed pause without a recorded source shows no checkmark.
    let p_after_expiry = presenter.present(&timed_snapshot(1800), None);
    assert!(!p_after_expiry.pause_1_hour.is_checked);
    assert!(!p_after_expiry.pause_3_hours.is_checked);
    assert!(!p_after_expiry.pause_until_resumed.is_checked);

    // 5. Simulated app restart: fresh TrayPresenter with persisted timed pause -> no checkmark.
    let (restarted_presenter, _) = make_presenter();
    let p_restarted_timed = restarted_presenter.present(&timed_snapshot(2400), None);
    assert!(!p_restarted_timed.pause_1_hour.is_checked);
    assert!(!p_restarted_timed.pause_3_hours.is_checked);
    assert!(!p_restarted_timed.pause_until_resumed.is_checked);

    // 6. Simulated app restart with persisted indefinite pause -> "Pause until resumed" checked.
    let (restarted_indef_presenter, _) = make_presenter();
    let p_restarted_indef = restarted_indef_presenter.present(&indef_snapshot, None);
    assert!(!p_restarted_indef.pause_1_hour.is_checked);
    assert!(!p_restarted_indef.pause_3_hours.is_checked);
    assert!(p_restarted_indef.pause_until_resumed.is_checked);
}

#[derive(Debug, Default)]
struct RecordingMenuSink {
    presentations: Vec<TrayPresentation>,
}

impl TraySink for RecordingMenuSink {
    fn set_tooltip(&mut self, _tooltip: &str) {}
    fn set_icon(&mut self, _state: TrayIconState, _use_light_set: bool) {}
    fn set_menu(&mut self, presentation: &TrayPresentation) {
        self.presentations.push(presentation.clone());
    }
}

#[derive(Debug)]
struct StaticTheme(bool);

impl TaskbarThemeProvider for StaticTheme {
    fn is_supported(&self) -> bool {
        true
    }
    fn is_light_taskbar(&self) -> Option<bool> {
        Some(self.0)
    }
}

#[test]
fn controller_refreshes_pause_status_on_ticks_and_on_reclicking_same_pause_option() {
    let (presenter, clock) = make_presenter_with_offset(2026, 11, 19, 14, 0, 0, 1);
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let theme: Rc<dyn TaskbarThemeProvider> = Rc::new(StaticTheme(false));
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let mut engine =
        BreakEngine::with_defaults(OcySettings::default(), platform, Arc::clone(&clock_dyn));
    let mut controller = TrayController::new(
        presenter,
        RecordingMenuSink::default(),
        theme,
        clock_dyn,
        Some(AppTheme::Dark),
        &engine.snapshot(),
    );
    assert_eq!(controller.sink().presentations.len(), 1);
    assert!(!controller.sink().presentations[0].pause_status.is_visible);

    // User clicks "Pause for 1 hour" at 14:00:00+01:00.
    controller.record_pause(PauseSource::OneHour);
    let _ = engine.pause_for(PauseSource::OneHour.duration());
    controller.update(&engine.snapshot());
    assert_eq!(controller.sink().presentations.len(), 2);
    let second = &controller.sink().presentations[1];
    assert!(second.pause_status.is_visible);
    assert_eq!(
        second.pause_status.text,
        "Paused until 15:00 \u{00b7} 1 h 00 min left"
    );
    assert!(second.pause_1_hour.is_checked);

    // Advance 10 s (scheduler tick while paused): remainder drops from 60m to 59m50s -> "59 min left".
    clock.advance(Duration::from_secs(10));
    let _ = engine.tick();
    controller.update(&engine.snapshot());
    assert_eq!(controller.sink().presentations.len(), 3);
    let third = &controller.sink().presentations[2];
    assert_eq!(
        third.pause_status.text,
        "Paused until 15:00 \u{00b7} 59 min left"
    );

    // Re-click "Pause for 1 hour" within the same minute -> set_menu still runs to restore check state.
    let count_before = controller.sink().presentations.len();
    controller.record_pause(PauseSource::OneHour);
    let _ = engine.pause_for(PauseSource::OneHour.duration());
    controller.update(&engine.snapshot());
    assert_eq!(controller.sink().presentations.len(), count_before + 1);
    assert!(
        controller
            .sink()
            .presentations
            .last()
            .unwrap()
            .pause_1_hour
            .is_checked
    );
}
