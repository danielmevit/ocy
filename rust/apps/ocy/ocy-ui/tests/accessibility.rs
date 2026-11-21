//! Screen-reader checks through Slint's testing backend: every interactive control exposes a role,
//! a label and a working default action; values and checked states follow the component state.
use std::cell::Cell;
use std::rc::Rc;

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};
use ocy_ui::{LongBreakCard, SettingsWindow, ShortBreakCard, WarningCard};
use slint::ComponentHandle;

fn init() {
    thread_local! { static DONE: Cell<bool> = const { Cell::new(false) }; }
    DONE.with(|done| {
        if !done.replace(true) {
            i_slint_backend_testing::init_no_event_loop();
        }
    });
}

const INTERACTIVE: [AccessibleRole; 4] = [
    AccessibleRole::Button,
    AccessibleRole::Switch,
    AccessibleRole::Checkbox,
    AccessibleRole::Spinbox,
];

/// One handle per accessible item: the query yields a handle for each element merged into an item
/// (component root + base), so keep the first per role, label and geometry.
fn unique(found: Vec<ElementHandle>) -> Vec<ElementHandle> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for e in found {
        let pos = e.absolute_position();
        let size = e.size();
        let key = (
            format!("{:?}", e.accessible_role()),
            e.accessible_label().unwrap_or_default().to_string(),
            [pos.x, pos.y, size.width, size.height].map(f32::to_bits),
        );
        if !seen.contains(&key) {
            seen.push(key);
            out.push(e);
        }
    }
    out
}

fn interactive(root: &impl i_slint_backend_testing::ElementRoot) -> Vec<ElementHandle> {
    unique(
        ElementQuery::from_root(root)
            .match_descendants()
            .match_predicate(|e| {
                e.accessible_role()
                    .is_some_and(|r| INTERACTIVE.contains(&r))
            })
            .find_all(),
    )
}

/// The testing backend skips items clipped away, so give each window room for all its content.
fn tall<C: ComponentHandle>(c: &C) {
    c.window().set_size(slint::LogicalSize::new(600.0, 3000.0));
}

fn by_label(
    root: &impl i_slint_backend_testing::ElementRoot,
    label: &str,
    role: AccessibleRole,
) -> ElementHandle {
    let found = unique(
        ElementQuery::from_root(root)
            .match_descendants()
            .match_accessible_label(label)
            .match_accessible_role(role)
            .find_all(),
    );
    assert_eq!(
        found.len(),
        1,
        "expected exactly one {role:?} labelled {label:?}"
    );
    found.into_iter().next().unwrap_or_else(|| unreachable!())
}

fn assert_all_labelled(root: &impl i_slint_backend_testing::ElementRoot, expected: usize) {
    let all = interactive(root);
    assert_eq!(all.len(), expected, "interactive control count");
    for e in all {
        let label = e.accessible_label().unwrap_or_default();
        assert!(
            !label.trim().is_empty(),
            "{:?} {:?} has no accessible label",
            e.accessible_role(),
            e.type_name()
        );
    }
}

fn settings() -> SettingsWindow {
    let ui = SettingsWindow::new().unwrap_or_else(|e| panic!("{e}"));
    ui.set_window_title("Ocy Settings".into());
    ui.set_general_title("General".into());
    ui.set_short_breaks_title("Short breaks".into());
    ui.set_long_breaks_title("Long breaks".into());
    ui.set_behaviour_title("Behaviour".into());
    ui.set_about_title("About".into());
    ui.set_start_with_system_label("Start with Windows".into());
    ui.set_short_enabled_label("Enable short breaks".into());
    ui.set_short_interval_label("Every".into());
    ui.set_short_duration_label("Short break length".into());
    ui.set_short_fullscreen_label("Full screen".into());
    ui.set_try_short_label("Try a short break".into());
    ui.set_long_enabled_label("Enable long breaks".into());
    ui.set_long_interval_label("Long break every".into());
    ui.set_long_duration_label("Long break length".into());
    ui.set_long_warning_label("Warn before".into());
    ui.set_long_postpone_label("Postpone by".into());
    ui.set_long_max_postpones_label("Max postpones".into());
    ui.set_try_long_label("Try a long break".into());
    ui.set_strict_label("Strict mode".into());
    ui.set_inactivity_label("Pause when away".into());
    ui.set_away_label("Away after".into());
    ui.set_pause_fullscreen_label("Pause for full-screen apps".into());
    ui.set_window_nearby_label("Show near the active window".into());
    ui.set_unit_minutes("min".into());
    ui.set_unit_seconds("sec".into());
    tall(&ui);
    ui
}

#[test]
fn settings_controls_have_roles_and_labels() {
    init();
    let ui = settings();
    // 8 toggles (start with system, short, short full screen, long, strict, inactivity, pause,
    // nearby), 8 number fields (short 2, long 5, away), 2 links.
    assert_all_labelled(&ui, 8 + 8 + 2);
    by_label(&ui, "Try a short break", AccessibleRole::Button);
    by_label(&ui, "Away after", AccessibleRole::Spinbox);
    by_label(&ui, "Strict mode", AccessibleRole::Switch);
    for title in [
        "General",
        "Short breaks",
        "Long breaks",
        "Behaviour",
        "About",
    ] {
        by_label(&ui, title, AccessibleRole::Groupbox);
    }
}

#[test]
fn toggle_checked_state_follows_value_and_default_action_toggles() {
    init();
    let ui = settings();
    let changed = Rc::new(Cell::new(0));
    let c = Rc::clone(&changed);
    ui.on_changed(move || c.set(c.get() + 1));

    let strict = by_label(&ui, "Strict mode", AccessibleRole::Switch);
    assert_eq!(strict.accessible_checkable(), Some(true));
    assert_eq!(strict.accessible_checked(), Some(false));
    ui.set_strict_mode(true);
    assert_eq!(strict.accessible_checked(), Some(true));

    strict.invoke_accessible_default_action();
    assert!(!ui.get_strict_mode());
    assert_eq!(strict.accessible_checked(), Some(false));
    assert_eq!(changed.get(), 1);
}

#[test]
fn number_field_value_includes_unit_and_steps() {
    init();
    let ui = settings();
    let away = by_label(&ui, "Away after", AccessibleRole::Spinbox);
    assert_eq!(away.accessible_value().as_deref(), Some("120 sec"));
    assert_eq!(away.accessible_value_minimum(), Some(30.0));
    assert_eq!(away.accessible_value_maximum(), Some(1800.0));

    away.invoke_accessible_increment_action();
    assert_eq!(ui.get_away_threshold(), 121);
    assert_eq!(away.accessible_value().as_deref(), Some("121 sec"));
    away.invoke_accessible_decrement_action();
    away.invoke_accessible_decrement_action();
    assert_eq!(ui.get_away_threshold(), 119);

    // A field without a unit announces the bare number.
    let max = by_label(&ui, "Max postpones", AccessibleRole::Spinbox);
    assert_eq!(max.accessible_value().as_deref(), Some("3"));
}

#[test]
fn close_button_default_action_fires_close_callback() {
    init();
    let card = ShortBreakCard::new().unwrap_or_else(|e| panic!("{e}"));
    tall(&card);
    card.set_heading("Look away".into());
    card.set_close_label("Close".into());
    card.set_progress(0.25);
    let closed = Rc::new(Cell::new(0));
    let c = Rc::clone(&closed);
    card.on_close_clicked(move || c.set(c.get() + 1));

    assert_all_labelled(&card, 1);
    by_label(&card, "Close", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!(closed.get(), 1);

    let progress = by_label(&card, "Look away", AccessibleRole::ProgressIndicator);
    assert_eq!(progress.accessible_value().as_deref(), Some("25"));
    // The illustration is decorative and stays out of the tree.
    let images = ElementQuery::from_root(&card)
        .match_descendants()
        .match_accessible_role(AccessibleRole::Image)
        .find_all();
    assert!(images.is_empty());
}

#[test]
fn warning_card_buttons_are_labelled_and_activate() {
    init();
    let card = WarningCard::new().unwrap_or_else(|e| panic!("{e}"));
    tall(&card);
    card.set_start_now_label("Start now".into());
    card.set_postpone_label("Postpone".into());
    card.set_skip_label("Skip".into());
    let hits = Rc::new(Cell::new(0));
    let h = Rc::clone(&hits);
    card.on_postpone(move || h.set(h.get() + 1));
    assert_all_labelled(&card, 3);
    by_label(&card, "Postpone", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!(hits.get(), 1);

    card.set_strict(true);
    assert_all_labelled(&card, 1);
}

#[test]
fn long_break_countdown_is_announced_as_progress() {
    init();
    let card = LongBreakCard::new().unwrap_or_else(|e| panic!("{e}"));
    tall(&card);
    card.set_countdown_label("Time remaining".into());
    card.set_countdown("4:12".into());
    card.set_end_label("End break".into());
    card.set_progress(0.3);
    let ended = Rc::new(Cell::new(0));
    let e = Rc::clone(&ended);
    card.on_end_break(move || e.set(e.get() + 1));

    let progress = by_label(&card, "Time remaining", AccessibleRole::ProgressIndicator);
    assert_eq!(progress.accessible_value().as_deref(), Some("4:12"));
    assert_eq!(progress.accessible_value_minimum(), Some(0.0));
    assert_eq!(progress.accessible_value_maximum(), Some(100.0));

    assert_all_labelled(&card, 1);
    by_label(&card, "End break", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!(ended.get(), 1);

    card.set_can_end(false);
    assert_all_labelled(&card, 0);
}

fn press_escape<C: ComponentHandle>(c: &C) {
    let text: slint::SharedString = slint::platform::Key::Escape.into();
    c.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
    c.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
}

#[test]
fn escape_closes_settings_and_ends_non_strict_long_break() {
    init();
    let ui = settings();
    let esc = Rc::new(Cell::new(0));
    let e = Rc::clone(&esc);
    ui.on_escape_pressed(move || e.set(e.get() + 1));
    press_escape(&ui);
    assert_eq!(esc.get(), 1);

    let card = LongBreakCard::new().unwrap_or_else(|e| panic!("{e}"));
    tall(&card);
    let ended = Rc::new(Cell::new(0));
    let e = Rc::clone(&ended);
    card.on_end_break(move || e.set(e.get() + 1));
    press_escape(&card);
    assert_eq!(ended.get(), 1);
    // Strict mode: no End break button and Esc does nothing.
    card.set_can_end(false);
    press_escape(&card);
    assert_eq!(ended.get(), 1);
}
