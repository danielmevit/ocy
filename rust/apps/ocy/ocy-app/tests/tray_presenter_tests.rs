use std::sync::Arc;
use std::time::Duration;

use damt_platform::{Clock, PlatformServices, Unsupported};
use ocy_app::strings::Strings;
use ocy_app::tray::{
    TRAY_MENU_ORDER, TrayIconState, TrayMenuEntry, TrayMenuSlot, TrayPresenter,
    can_take_long_break_now, can_take_short_break_now,
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
fn tray_menu_item_order_places_take_short_break_directly_above_take_long_break() {
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
    assert_eq!(entries.len(), 10);
    assert!(matches!(
        entries[0],
        TrayMenuEntry::Item(TrayMenuSlot::TakeShortBreakNow, item)
            if item.text == "Take a short break now"
    ));
    assert!(matches!(
        entries[1],
        TrayMenuEntry::Item(TrayMenuSlot::TakeLongBreakNow, item)
            if item.text == "Take a long break now"
    ));
    assert_eq!(entries[2], TrayMenuEntry::Separator);
    assert_eq!(entries[7], TrayMenuEntry::Separator);
    assert!(matches!(
        entries[8],
        TrayMenuEntry::Item(TrayMenuSlot::Settings, item) if item.text == "Settings"
    ));
    assert!(matches!(
        entries[9],
        TrayMenuEntry::Item(TrayMenuSlot::Quit, item) if item.text == "Quit Ocy"
    ));

    // Confirm the declarative Slint Menu in slint_tray.rs declares take-short-break right before take-long-break.
    let slint_src = include_str!("../src/slint_tray.rs");
    let short_idx = slint_src
        .find("title: root.take-short-break-text;")
        .expect("take-short-break-text menu item");
    let long_idx = slint_src
        .find("title: root.take-long-break-text;")
        .expect("take-long-break-text menu item");
    let first_sep_idx = slint_src.find("MenuSeparator {}").expect("first separator");
    assert!(short_idx < long_idx && long_idx < first_sep_idx);
}
