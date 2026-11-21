//! Headless interaction tests verifying every interactive control responds on the first click
//! and keeps keyboard Tab + Space focus and activation working.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ocy_ui::{LongBreakCard, SettingsWindowV2, ShortBreakCard, Theme, WarningCard};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Key, Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, SharedString};

const LIGHT_ACCENT: [u8; 3] = [0x2F, 0x7D, 0x62];
const LIGHT_SURFACE: [u8; 3] = [0xFF, 0xFF, 0xFF];

thread_local! {
    static LAST_WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct HeadlessPlatform;

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        LAST_WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
        Ok(window)
    }
}

fn init_platform() {
    let _ = slint::platform::set_platform(Box::new(HeadlessPlatform));
}

fn take_window() -> Rc<MinimalSoftwareWindow> {
    LAST_WINDOW
        .with(|slot| slot.borrow_mut().take())
        .expect("no window adapter was created")
}

struct Rgba {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 4]>,
}

impl Rgba {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y * self.width + x) as usize]
    }
}

fn near(p: [u8; 4], c: [u8; 3], tol: i32) -> bool {
    p[3] == 255 && (0..3).all(|i| (p[i] as i32 - c[i] as i32).abs() <= tol)
}

fn layout_and_render(
    window: &MinimalSoftwareWindow,
    logical_w: f32,
    height_fn: impl Fn() -> f32,
) -> Rgba {
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    let width = logical_w.ceil() as u32;
    let mut size = (0, 0);
    let mut buffer = Vec::new();
    for _ in 0..3 {
        let h = height_fn().ceil() as u32;
        if (width, h) != size {
            size = (width, h);
            window.set_size(PhysicalSize::new(width, h));
            buffer = vec![PremultipliedRgbaColor::default(); (width * h) as usize];
        }
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            buffer.fill(PremultipliedRgbaColor::default());
            renderer.render(&mut buffer, width as usize);
        });
    }
    let pixels = buffer
        .iter()
        .map(|p| {
            if p.alpha == 0 {
                [0, 0, 0, 0]
            } else {
                let un =
                    |c: u8| ((c as u32 * 255 + p.alpha as u32 / 2) / p.alpha as u32).min(255) as u8;
                [un(p.red), un(p.green), un(p.blue), p.alpha]
            }
        })
        .collect();
    Rgba {
        width,
        height: size.1,
        pixels,
    }
}

fn redraw_only(window: &MinimalSoftwareWindow, width: u32, height: u32) -> Rgba {
    let mut buffer = vec![PremultipliedRgbaColor::default(); (width * height) as usize];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        buffer.fill(PremultipliedRgbaColor::default());
        renderer.render(&mut buffer, width as usize);
    });
    let pixels = buffer
        .iter()
        .map(|p| {
            if p.alpha == 0 {
                [0, 0, 0, 0]
            } else {
                let un =
                    |c: u8| ((c as u32 * 255 + p.alpha as u32 / 2) / p.alpha as u32).min(255) as u8;
                [un(p.red), un(p.green), un(p.blue), p.alpha]
            }
        })
        .collect();
    Rgba {
        width,
        height,
        pixels,
    }
}

fn click_once(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

fn press_key(window: &MinimalSoftwareWindow, text: SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

fn bounding_box(
    img: &Rgba,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    pred: impl Fn([u8; 4]) -> bool,
) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if pred(img.at(x, y)) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x <= max_x && min_y <= max_y {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

fn center_of((x0, y0, x1, y1): (u32, u32, u32, u32)) -> (f32, f32) {
    ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0)
}

#[test]
fn short_break_close_button_fires_on_first_click() {
    init_platform();
    let card = ShortBreakCard::new().unwrap();
    let window = take_window();
    card.global::<Theme>().set_dark(false);
    card.set_heading("Look left and right".into());
    card.set_body("Keep your head still and look left, then right.".into());
    card.set_close_label("Close".into());
    card.set_progress(0.8);

    let clicks = Rc::new(Cell::new(0u32));
    let clicks_cb = Rc::clone(&clicks);
    card.on_close_clicked(move || clicks_cb.set(clicks_cb.get() + 1));

    let img = layout_and_render(&window, 392.0, || card.get_content_height());
    // CloseButton is 28x28 at top-right inside 16 px window margin + 14x12 card padding.
    let close_left = 392 - 16 - 14 - 28;
    let close_top = 16 + 12;
    let cx = close_left as f32 + 14.0;
    let cy = close_top as f32 + 14.0;
    let ink = (close_top..close_top + 28)
        .flat_map(|y| (close_left..close_left + 28).map(move |x| (x, y)))
        .filter(|&(x, y)| !near(img.at(x, y), LIGHT_SURFACE, 12))
        .count();
    assert!(ink > 5, "expected close x glyph at ({cx}, {cy}), got {ink}");

    click_once(&window, cx, cy);
    assert_eq!(
        clicks.get(),
        1,
        "ShortBreakCard CloseButton must fire on first click"
    );
}

#[test]
fn warning_card_all_three_buttons_fire_on_first_click() {
    init_platform();

    // Test each button on a fresh WarningCard (no prior focus in the window).
    for target in ["start_now", "postpone", "skip"] {
        let card = WarningCard::new().unwrap();
        let window = take_window();
        card.global::<Theme>().set_dark(false);
        card.set_heading("Long break in 0:42".into());
        card.set_start_now_label("Start now".into());
        card.set_postpone_label("Postpone 3 min".into());
        card.set_skip_label("Skip".into());
        card.set_strict(false);
        card.set_can_postpone(true);

        let start_hits = Rc::new(Cell::new(0u32));
        let postpone_hits = Rc::new(Cell::new(0u32));
        let skip_hits = Rc::new(Cell::new(0u32));
        let s = Rc::clone(&start_hits);
        let p = Rc::clone(&postpone_hits);
        let k = Rc::clone(&skip_hits);
        card.on_start_now(move || s.set(s.get() + 1));
        card.on_postpone(move || p.set(p.get() + 1));
        card.on_skip(move || k.set(k.get() + 1));

        let img = layout_and_render(&window, 392.0, || card.get_content_height());
        let start_box = bounding_box(&img, 16, 16, 376, img.height - 16, |px| {
            near(px, LIGHT_ACCENT, 8)
        })
        .expect("Start now accent button bounding box");
        let btn_y_mid = (start_box.1 + start_box.3) / 2;
        let row_y0 = start_box.1;
        let row_y1 = start_box.3 + 1;
        // Find contiguous horizontal spans of non-surface pixels in the button row (inside card right padding).
        let right_bound = 360u32;
        let mut spans: Vec<(u32, u32)> = Vec::new();
        let mut cur_start: Option<u32> = None;
        for col in (start_box.2 + 3)..right_bound {
            let has_ink = (row_y0..row_y1).any(|y| !near(img.at(col, y), LIGHT_SURFACE, 12));
            match (cur_start, has_ink) {
                (None, true) => cur_start = Some(col),
                (Some(st), false) => {
                    // Merge text character gaps (<= 5 px), split on 8 px button spacing.
                    let gap_end = (col..(col + 6).min(right_bound)).find(|&nx| {
                        (row_y0..row_y1).any(|y| !near(img.at(nx, y), LIGHT_SURFACE, 12))
                    });
                    if gap_end.is_none() {
                        spans.push((st, col - 1));
                        cur_start = None;
                    }
                }
                _ => {}
            }
        }
        if let Some(st) = cur_start {
            spans.push((st, right_bound - 1));
        }
        assert_eq!(
            spans.len(),
            2,
            "expected Postpone and Skip spans, got {spans:?}"
        );

        let (cx, cy) = match target {
            "start_now" => center_of(start_box),
            "postpone" => ((spans[0].0 + spans[0].1) as f32 / 2.0, btn_y_mid as f32),
            "skip" => ((spans[1].0 + spans[1].1) as f32 / 2.0, btn_y_mid as f32),
            _ => unreachable!(),
        };

        click_once(&window, cx, cy);
        match target {
            "start_now" => assert_eq!(
                (start_hits.get(), postpone_hits.get(), skip_hits.get()),
                (1, 0, 0),
                "WarningCard Start now failed on first click at ({cx}, {cy})"
            ),
            "postpone" => assert_eq!(
                (start_hits.get(), postpone_hits.get(), skip_hits.get()),
                (0, 1, 0),
                "WarningCard Postpone failed on first click at ({cx}, {cy})"
            ),
            "skip" => assert_eq!(
                (start_hits.get(), postpone_hits.get(), skip_hits.get()),
                (0, 0, 1),
                "WarningCard Skip failed on first click at ({cx}, {cy})"
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn long_break_end_button_fires_on_first_click() {
    init_platform();
    let card = LongBreakCard::new().unwrap();
    let window = take_window();
    card.global::<Theme>().set_dark(false);
    card.set_dim_backdrop(false);
    card.set_countdown_label("Back to your screen in".into());
    card.set_countdown("4:12".into());
    card.set_heading("Time to step away".into());
    card.set_body("Step away from your screen.".into());
    card.set_show_suggestions(false);
    card.set_progress(0.2);
    card.set_end_label("End break".into());
    card.set_can_end(true);

    let hits = Rc::new(Cell::new(0u32));
    let hits_cb = Rc::clone(&hits);
    card.on_end_break(move || hits_cb.set(hits_cb.get() + 1));

    let img = layout_and_render(&window, 640.0, || 520.0);
    // Card is 480 px wide centred at x = 80..560. End break button is in bottom-right inside 24 px padding.
    let card_h = card.get_content_height().ceil() as u32;
    let card_bottom = (520 + card_h) / 2;
    let end_box = bounding_box(
        &img,
        420,
        card_bottom - 64,
        560 - 20,
        card_bottom - 18,
        |px| !near(px, LIGHT_SURFACE, 12),
    )
    .expect("End break button bounding box");

    let (cx, cy) = center_of(end_box);
    click_once(&window, cx, cy);
    assert_eq!(
        hits.get(),
        1,
        "LongBreakCard End break must fire on first click at ({cx}, {cy})"
    );
}

#[test]
fn keyboard_tab_focuses_controls_shows_focus_ring_and_space_activates() {
    init_platform();
    // 1. Button in WarningCard: Tab twice -> Postpone -> focus ring appears -> Space fires postpone.
    {
        let card = WarningCard::new().unwrap();
        let window = take_window();
        card.global::<Theme>().set_dark(false);
        card.set_heading("Long break in 0:42".into());
        card.set_start_now_label("Start now".into());
        card.set_postpone_label("Postpone 3 min".into());
        card.set_skip_label("Skip".into());
        card.set_strict(false);
        card.set_can_postpone(true);

        let start_hits = Rc::new(Cell::new(0u32));
        let postpone_hits = Rc::new(Cell::new(0u32));
        let s = Rc::clone(&start_hits);
        let p = Rc::clone(&postpone_hits);
        card.on_start_now(move || s.set(s.get() + 1));
        card.on_postpone(move || p.set(p.get() + 1));

        let img_before = layout_and_render(&window, 392.0, || card.get_content_height());
        let accent_before = img_before
            .pixels
            .iter()
            .filter(|px| near(**px, LIGHT_ACCENT, 8))
            .count();

        window.dispatch_event(WindowEvent::WindowActiveChanged(true));
        press_key(&window, Key::Tab.into());
        press_key(&window, Key::Tab.into());

        let img_focused = redraw_only(&window, img_before.width, img_before.height);
        let accent_after = img_focused
            .pixels
            .iter()
            .filter(|px| near(**px, LIGHT_ACCENT, 8))
            .count();
        assert!(
            accent_after > accent_before + 40,
            "expected keyboard focus ring on Postpone (accent pixels before={accent_before}, after={accent_after})"
        );

        press_key(&window, " ".into());
        assert_eq!(start_hits.get(), 0);
        assert_eq!(
            postpone_hits.get(),
            1,
            "Space on focused button must activate it"
        );
    }

    // 2. CloseButton in ShortBreakCard: Tab -> focus ring appears -> Space fires close_clicked.
    {
        let card = ShortBreakCard::new().unwrap();
        let window = take_window();
        card.global::<Theme>().set_dark(false);
        card.set_heading("Blink".into());
        card.set_body("Blink slowly.".into());
        card.set_close_label("Close".into());
        card.set_progress(0.5);

        let hits = Rc::new(Cell::new(0u32));
        let hits_cb = Rc::clone(&hits);
        card.on_close_clicked(move || hits_cb.set(hits_cb.get() + 1));

        let img_before = layout_and_render(&window, 392.0, || card.get_content_height());
        window.dispatch_event(WindowEvent::WindowActiveChanged(true));
        press_key(&window, Key::Tab.into());
        let img_focused = redraw_only(&window, img_before.width, img_before.height);
        let accent_after = img_focused
            .pixels
            .iter()
            .filter(|px| near(**px, LIGHT_ACCENT, 8))
            .count();
        assert!(accent_after > 20, "expected focus ring on CloseButton");

        press_key(&window, " ".into());
        assert_eq!(
            hits.get(),
            1,
            "Space on focused CloseButton must activate it"
        );
    }
}

// ---- Settings V2: every new control fires on the first click and works from the keyboard ----

#[path = "support/mod.rs"]
mod support;

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};

fn v2_window(tab: Option<i32>) -> (SettingsWindowV2, Rc<MinimalSoftwareWindow>) {
    init_platform();
    let ui = support::settings_v2("en", false).unwrap();
    let window = take_window();
    if let Some(tab) = tab {
        ui.set_customize_open(true);
        ui.set_customize_tab(tab);
    }
    // The host grows the window to the content (never below 640).
    let _ = layout_and_render(&window, 560.0, || ui.get_content_height().max(640.0));
    (ui, window)
}

fn element(ui: &SettingsWindowV2, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementQuery::from_root(ui)
        .match_descendants()
        .match_accessible_label(label)
        .match_accessible_role(role)
        .find_first()
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

/// Clicks at the fraction (`fx`, `fy`) of the element's box; returns the click point.
fn click_in(window: &MinimalSoftwareWindow, e: &ElementHandle, fx: f32, fy: f32) -> (f32, f32) {
    let p = e.absolute_position();
    let s = e.size();
    let (x, y) = (p.x + s.width * fx, p.y + s.height * fy);
    click_once(window, x, y);
    (x, y)
}

fn counter() -> (Rc<Cell<u32>>, Rc<Cell<u32>>) {
    let c = Rc::new(Cell::new(0u32));
    (Rc::clone(&c), c)
}

#[test]
fn settings_v2_presets_accordion_tabs_toggles_and_links_fire_on_first_click() {
    // Preset card.
    {
        let (ui, window) = v2_window(None);
        let picked = Rc::new(Cell::new(-1));
        let p = Rc::clone(&picked);
        ui.on_preset_selected(move |i| p.set(i));
        click_in(
            &window,
            &element(&ui, "Gentle", AccessibleRole::Button),
            0.5,
            0.5,
        );
        assert_eq!(picked.get(), 0, "Gentle preset must fire on first click");
        assert_eq!(ui.get_preset(), 0);
        click_in(
            &window,
            &element(&ui, "Focused", AccessibleRole::Button),
            0.5,
            0.5,
        );
        assert_eq!(picked.get(), 2, "Focused preset must fire on first click");
    }
    // Accordion header.
    {
        let (ui, window) = v2_window(None);
        click_in(
            &window,
            &element(&ui, "Customize", AccessibleRole::Button),
            0.5,
            0.5,
        );
        assert!(
            ui.get_customize_open(),
            "Customize header must open on first click"
        );
        // General is open by default: fold it, then open it again; Customize stays open throughout.
        let _ = layout_and_render(&window, 560.0, || ui.get_content_height().max(640.0));
        click_in(
            &window,
            &element(&ui, "General", AccessibleRole::Button),
            0.5,
            0.5,
        );
        assert!(
            !ui.get_general_open(),
            "General header must fold on first click"
        );
        assert!(
            ui.get_customize_open(),
            "folding General keeps Customize open"
        );
        let _ = layout_and_render(&window, 560.0, || ui.get_content_height().max(640.0));
        click_in(
            &window,
            &element(&ui, "General", AccessibleRole::Button),
            0.5,
            0.5,
        );
        assert!(
            ui.get_general_open() && ui.get_customize_open(),
            "both sections stay open"
        );
    }
    // Tab.
    {
        let (ui, window) = v2_window(Some(0));
        click_in(
            &window,
            &element(&ui, "Quiet times", AccessibleRole::Tab),
            0.5,
            0.5,
        );
        assert_eq!(
            ui.get_customize_tab(),
            2,
            "Quiet times tab must select on first click"
        );
    }
    // ToggleRow in General (Start with system) and Toggle + Try links on the Breaks tab.
    {
        let (ui, window) = v2_window(None);
        let (changed, c) = counter();
        ui.on_changed(move || c.set(c.get() + 1));
        click_in(
            &window,
            &element(&ui, "Start with system", AccessibleRole::Switch),
            0.5,
            0.5,
        );
        assert_eq!(
            changed.get(),
            1,
            "V2 ToggleRow must fire changed on first click"
        );
        assert!(!ui.get_start_with_system());

        let (ui, window) = v2_window(Some(0));
        let (changed, c) = counter();
        ui.on_changed(move || c.set(c.get() + 1));
        click_in(
            &window,
            &element(&ui, "Short breaks", AccessibleRole::Switch),
            0.5,
            0.5,
        );
        assert_eq!(
            changed.get(),
            1,
            "V2 Toggle must fire changed on first click"
        );
        assert!(!ui.get_short_enabled());

        let (ui, window) = v2_window(Some(0));
        let (tries, t) = counter();
        ui.on_try_short_break(move || t.set(t.get() + 1));
        click_in(
            &window,
            &element(&ui, "Try a short break", AccessibleRole::Button),
            0.5,
            0.5,
        );
        assert_eq!(tries.get(), 1, "Try a short break must fire on first click");

        let (ui, window) = v2_window(Some(0));
        let (tries, t) = counter();
        ui.on_try_long_break(move || t.set(t.get() + 1));
        click_in(
            &window,
            &element(&ui, "Try a long break", AccessibleRole::Button),
            0.5,
            0.5,
        );
        assert_eq!(tries.get(), 1, "Try a long break must fire on first click");
    }
}

#[test]
fn settings_v2_slider_sets_value_on_first_press_and_drags() {
    let (ui, window) = v2_window(Some(0));
    let (changed, c) = counter();
    ui.on_changed(move || c.set(c.get() + 1));
    let slider = element(&ui, "Short break interval", AccessibleRole::Slider);
    let p = slider.absolute_position();
    let s = slider.size();
    // The track is the slider's bottom 16 px; 7 px knob insets at both ends.
    let y = p.y + s.height - 8.0;
    let at = |t: f32| p.x + 7.0 + (s.width - 14.0) * t;
    click_once(&window, at(1.0), y);
    assert_eq!(changed.get(), 1, "Slider must change on the first click");
    assert_eq!(ui.get_short_interval(), 60);
    click_once(&window, at(0.0), y);
    assert_eq!(ui.get_short_interval(), 2);

    // Drag from the start to the middle.
    window.dispatch_event(WindowEvent::PointerPressed {
        position: LogicalPosition::new(at(0.0), y),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(at(0.5), y),
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: LogicalPosition::new(at(0.5), y),
        button: PointerEventButton::Left,
    });
    assert_eq!(ui.get_short_interval(), 31);
}

#[test]
fn settings_v2_dropdown_opens_on_first_click_and_picks() {
    let (ui, window) = v2_window(None);
    let picked = Rc::new(Cell::new(-1));
    let p = Rc::clone(&picked);
    ui.on_language_selected(move |i| p.set(i));
    let combo = element(&ui, "Language", AccessibleRole::Combobox);
    click_in(&window, &combo, 0.5, 0.5);
    let _ = redraw_only(&window, 560, 640);
    let deutsch = ElementQuery::from_root(&ui)
        .match_descendants()
        .match_accessible_label("Deutsch")
        .find_first()
        .expect("the language list must open on the first click");
    // Popup items report positions relative to the popup, which opens 2 px below the box.
    let (cp, cs) = (combo.absolute_position(), combo.size());
    let (dp, ds) = (deutsch.absolute_position(), deutsch.size());
    click_once(
        &window,
        cp.x + dp.x + ds.width / 2.0,
        cp.y + cs.height + 2.0 + dp.y + ds.height / 2.0,
    );
    assert_eq!(picked.get(), 2);
    assert_eq!(ui.get_language_index(), 2);
}

#[test]
fn settings_v2_keyboard_tab_reaches_new_controls_and_keys_act() {
    let (ui, window) = v2_window(Some(2));
    let tab: SharedString = Key::Tab.into();
    let right: SharedString = Key::RightArrow.into();
    let space: SharedString = " ".into();
    // Tab order: Gentle, Balanced, Focused, Customize, tab list, inactivity, away slider.
    for _ in 0..7 {
        press_key(&window, tab.clone());
    }
    press_key(&window, right.clone());
    assert_eq!(
        ui.get_away_threshold(),
        150,
        "Right arrow steps the focused slider by 30 s"
    );

    let (ui, window) = v2_window(Some(0));
    for _ in 0..5 {
        press_key(&window, tab.clone());
    }
    press_key(&window, right);
    assert_eq!(
        ui.get_customize_tab(),
        1,
        "Right arrow moves the focused tab list"
    );

    let (ui, window) = v2_window(None);
    let picked = Rc::new(Cell::new(-1));
    let p = Rc::clone(&picked);
    ui.on_preset_selected(move |i| p.set(i));
    press_key(&window, tab.clone());
    press_key(&window, space.clone());
    assert_eq!(picked.get(), 0, "Space picks the focused preset");
    for _ in 0..3 {
        press_key(&window, tab.clone());
    }
    press_key(&window, space);
    assert!(
        ui.get_customize_open(),
        "Space opens the focused accordion header"
    );
}
