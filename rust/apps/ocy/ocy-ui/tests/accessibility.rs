//! Screen-reader checks through Slint's testing backend: every interactive control exposes a role,
//! a label and a working default action; values and checked states follow the component state.
use std::cell::Cell;
use std::rc::Rc;

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};
use ocy_ui::{LongBreakCard, SettingsWindowV2, ShortBreakCard, WarningCard};
use slint::ComponentHandle;

fn init() {
    thread_local! { static DONE: Cell<bool> = const { Cell::new(false) }; }
    DONE.with(|done| {
        if !done.replace(true) {
            i_slint_backend_testing::init_no_event_loop();
        }
    });
}

const INTERACTIVE: [AccessibleRole; 7] = [
    AccessibleRole::Slider,
    AccessibleRole::Tab,
    AccessibleRole::Combobox,
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
fn escape_ends_non_strict_long_break() {
    init();
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

// ---- Settings V2 ----

#[path = "support/mod.rs"]
mod support;

/// Client height a 1080p work area (1040 px above a 40 px taskbar) leaves the window, minus the frame:
/// the host's `settings_window_height` never grows past this.
const WORK_AREA_CLIENT: f32 = 1000.0;

fn v2(lang: &str, tab: Option<i32>) -> SettingsWindowV2 {
    let ui = support::settings_v2(lang, false).unwrap_or_else(|e| panic!("{e}"));
    if let Some(tab) = tab {
        ui.set_customize_open(true);
        ui.set_customize_tab(tab);
    }
    // The real default size; the host grows it to the content (both sections may be open).
    ui.window().set_size(slint::LogicalSize::new(560.0, 640.0));
    // Walking the tree instantiates the `if` sections so their height counts.
    let _ = interactive(&ui);
    let need = ui.get_content_height().ceil();
    if need > 640.0 {
        ui.window()
            .set_size(slint::LogicalSize::new(560.0, need.min(WORK_AREA_CLIENT)));
    }
    ui
}

/// Every interactive control is labelled, the expected number is exposed, and each one lies wholly
/// inside the window the host sizes to the content (640 px default, grown up to the work area), so
/// nothing needs scrolling and UI Automation sees all of it, with Customize and General both open.
#[test]
fn settings_v2_every_state_fits_the_grown_window_with_labelled_controls_in_en_and_de() {
    init();
    // presets 3 + section headers 2 + General (language, start with system, window nearby) 3,
    // then per Customize tab.
    let states = [
        (None, 3 + 2 + 3),
        (Some(0), 3 + 2 + 3 + 3 + 2 + 4 + 1 + 2), // tabs, 2 toggles, 4 sliders, full screen, 2 links
        (Some(1), 3 + 2 + 3 + 3 + 1 + 3),         // tabs, strict, 3 sliders
        (Some(2), 3 + 2 + 3 + 3 + 2 + 1),         // tabs, inactivity + pause, away slider
    ];
    for lang in ["en", "de"] {
        for (tab, expected) in states {
            let ui = v2(lang, tab);
            assert!(ui.get_general_open(), "General is open by default");
            assert_eq!(ui.get_customize_open(), tab.is_some());
            let need = ui.get_content_height();
            assert!(
                need <= WORK_AREA_CLIENT,
                "{lang} {tab:?}: content needs {need} px, more than the work area"
            );
            let height = need.ceil().max(640.0);
            let all = interactive(&ui);
            assert_eq!(
                all.len(),
                expected,
                "{lang} {tab:?}: interactive control count"
            );
            for e in all {
                let label = e.accessible_label().unwrap_or_default();
                assert!(
                    !label.trim().is_empty(),
                    "{lang} {tab:?}: {:?} unlabelled",
                    e.accessible_role()
                );
                let p = e.absolute_position();
                let s = e.size();
                assert!(
                    p.x >= 0.0 && p.y >= 0.0 && p.x + s.width <= 560.0 && p.y + s.height <= height,
                    "{lang} {tab:?}: {label:?} at {p:?} {s:?} is outside 560x{height}"
                );
            }
        }
    }
}

/// With General folded, Customize alone still fits the 640 px default (no growing needed).
#[test]
fn settings_v2_customize_alone_fits_640() {
    init();
    for lang in ["en", "de"] {
        for tab in 0..3 {
            let ui = v2(lang, Some(tab));
            ui.set_general_open(false);
            let _ = interactive(&ui);
            let need = ui.get_content_height();
            assert!(need <= 640.0, "{lang} tab {tab}: needs {need} px");
        }
    }
}

#[test]
fn settings_v2_compact_height_folds_timeline_then_today() {
    init();
    let ui = v2("en", None);
    assert!(ui.get_timeline_shown() && ui.get_today_shown());
    ui.window().set_size(slint::LogicalSize::new(480.0, 520.0));
    assert!(!ui.get_timeline_shown() && ui.get_today_shown());
    ui.window().set_size(slint::LogicalSize::new(480.0, 400.0));
    assert!(!ui.get_timeline_shown() && !ui.get_today_shown());
}

#[test]
fn settings_v2_slider_tabs_presets_accordion_and_dropdown_semantics() {
    init();
    let ui = v2("en", Some(0));
    let changed = Rc::new(Cell::new(0));
    let c = Rc::clone(&changed);
    ui.on_changed(move || c.set(c.get() + 1));

    // Slider: value is the whole phrase with its unit; increment/decrement step and save.
    let interval = by_label(&ui, "Short break interval", AccessibleRole::Slider);
    assert_eq!(interval.accessible_value().as_deref(), Some("every 20 min"));
    assert_eq!(interval.accessible_value_minimum(), Some(2.0));
    assert_eq!(interval.accessible_value_maximum(), Some(60.0));
    interval.invoke_accessible_increment_action();
    assert_eq!(ui.get_short_interval(), 21);
    assert_eq!(interval.accessible_value().as_deref(), Some("every 21 min"));
    assert_eq!(changed.get(), 1);
    let long_interval = by_label(&ui, "Long break interval", AccessibleRole::Slider);
    assert_eq!(long_interval.accessible_value_step(), Some(5.0));
    long_interval.invoke_accessible_decrement_action();
    assert_eq!(ui.get_long_interval(), 55);

    // Tabs: a tab list with selectable tabs; the default action switches the page.
    let strictness = by_label(&ui, "Strictness", AccessibleRole::Tab);
    assert_eq!(strictness.accessible_item_selected(), Some(false));
    assert_eq!(
        by_label(&ui, "Breaks", AccessibleRole::Tab).accessible_item_selected(),
        Some(true)
    );
    strictness.invoke_accessible_default_action();
    assert_eq!(ui.get_customize_tab(), 1);
    by_label(&ui, "Customize", AccessibleRole::TabList);
    let max = by_label(&ui, "Maximum postpones", AccessibleRole::Slider);
    assert_eq!(max.accessible_value().as_deref(), Some("3 times"));
    max.set_accessible_value("1");
    assert_eq!(ui.get_long_max_postpones(), 1);
    assert_eq!(max.accessible_value().as_deref(), Some("1 time"));

    // Presets: checkable buttons; the default action reports the pick.
    let picked = Rc::new(Cell::new(-1));
    let p = Rc::clone(&picked);
    ui.on_preset_selected(move |i| p.set(i));
    let gentle = by_label(&ui, "Gentle", AccessibleRole::Button);
    assert_eq!(gentle.accessible_checked(), Some(false));
    assert_eq!(
        by_label(&ui, "Balanced", AccessibleRole::Button).accessible_checked(),
        Some(true)
    );
    gentle.invoke_accessible_default_action();
    assert_eq!(picked.get(), 0);
    assert_eq!(gentle.accessible_checked(), Some(true));

    // Section headers: expandable buttons that open and close independently.
    let customize = by_label(&ui, "Customize", AccessibleRole::Button);
    assert_eq!(customize.accessible_expandable(), Some(true));
    assert_eq!(customize.accessible_expanded(), Some(true));
    let general = by_label(&ui, "General", AccessibleRole::Button);
    assert_eq!(general.accessible_expanded(), Some(true));
    general.invoke_accessible_default_action();
    assert!(!ui.get_general_open());
    assert!(
        ui.get_customize_open(),
        "folding General leaves Customize open"
    );
    general.invoke_accessible_default_action();
    assert!(ui.get_general_open() && ui.get_customize_open());
    customize.invoke_accessible_default_action();
    assert!(!ui.get_customize_open());
    assert!(
        ui.get_general_open(),
        "folding Customize leaves General open"
    );

    // Language: a labelled combo box whose value is the selected entry.
    let language = by_label(&ui, "Language", AccessibleRole::Combobox);
    assert_eq!(
        language.accessible_value().as_deref(),
        Some("Automatic (Windows language)")
    );
    ui.set_language_index(2);
    assert_eq!(language.accessible_value().as_deref(), Some("Deutsch"));
}

#[test]
fn settings_v2_hidden_rows_stay_hidden_until_enabled_and_escape_closes() {
    init();
    let ui = v2("en", None);
    assert!(
        ElementQuery::from_root(&ui)
            .match_descendants()
            .match_accessible_label("Sounds")
            .find_all()
            .is_empty()
    );
    let before = interactive(&ui).len();
    ui.set_show_sounds(true);
    ui.set_show_notifications(true);
    assert_eq!(interactive(&ui).len(), before + 2);

    let esc = Rc::new(Cell::new(0));
    let e = Rc::clone(&esc);
    ui.on_escape_pressed(move || e.set(e.get() + 1));
    press_escape(&ui);
    assert_eq!(esc.get(), 1);
}

#[test]
fn settings_v2_toggle_checked_state_and_today_groupbox_semantics() {
    init();
    let ui = v2("en", Some(1));
    by_label(&ui, "Today", AccessibleRole::Groupbox);

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
