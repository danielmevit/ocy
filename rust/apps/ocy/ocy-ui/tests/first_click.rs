//! Headless interaction tests verifying every interactive control responds on the first click
//! and keeps keyboard Tab + Space focus and activation working.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ocy_ui::{LongBreakCard, SettingsWindow, ShortBreakCard, Theme, WarningCard};
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
fn settings_toggle_section_toggle_stepper_and_try_links_fire_on_first_click() {
    init_platform();

    let build_settings = || {
        let ui = SettingsWindow::new().unwrap();
        let window = take_window();
        ui.global::<Theme>().set_dark(false);
        ui.set_show_start_with_system(true);
        ui.set_general_title("General".into());
        ui.set_short_breaks_title("Short breaks".into());
        ui.set_long_breaks_title("Long breaks".into());
        ui.set_behaviour_title("Behaviour".into());
        ui.set_about_title("About".into());
        ui.set_start_with_system_label("Start with system".into());
        ui.set_start_with_system_help("Open Ocy automatically.".into());
        ui.set_short_enabled_label("Short breaks".into());
        ui.set_short_interval_label("Short break interval".into());
        ui.set_short_duration_label("Short break length".into());
        ui.set_short_fullscreen_label("Full-screen short breaks".into());
        ui.set_try_short_label("Try a short break".into());
        ui.set_long_enabled_label("Long breaks".into());
        ui.set_long_interval_label("Long break interval".into());
        ui.set_long_duration_label("Long break length".into());
        ui.set_long_warning_label("Warning time".into());
        ui.set_long_postpone_label("Postpone by".into());
        ui.set_long_max_postpones_label("Maximum postpones".into());
        ui.set_try_long_label("Try a long break".into());
        ui.set_strict_label("Strict mode".into());
        ui.set_inactivity_label("Track inactivity".into());
        ui.set_away_label("Away after".into());
        ui.set_pause_fullscreen_label("Pause in fullscreen".into());
        ui.set_window_nearby_label("A window is nearby".into());
        ui.set_unit_minutes("min".into());
        ui.set_unit_seconds("s".into());
        ui.set_about_name_version("Ocy 0.2.0".into());
        ui.set_about_tagline("A quieter rhythm.".into());
        ui.set_start_with_system(true);
        ui.set_short_enabled(true);
        ui.set_short_interval(10);
        (ui, window)
    };

    // 1. SettingRow Toggle ("Start with system", checked = true -> solid accent pill on the right).
    {
        let (ui, window) = build_settings();
        let changed = Rc::new(Cell::new(0u32));
        let changed_cb = Rc::clone(&changed);
        ui.on_changed(move || changed_cb.set(changed_cb.get() + 1));

        let img = layout_and_render(&window, 560.0, || 1600.0);
        // Find the first accent toggle pill (40x20) on the right side (x > 450, y < 95).
        let toggle_box = bounding_box(&img, 450, 20, 540, 95, |px| near(px, LIGHT_ACCENT, 8))
            .expect("Start with system toggle pill");
        let (cx, cy) = center_of(toggle_box);
        click_once(&window, cx, cy);
        assert_eq!(
            changed.get(),
            1,
            "Settings Toggle must fire changed once on first click (box={toggle_box:?}, center=({cx}, {cy}))"
        );
        assert!(
            !ui.get_start_with_system(),
            "Settings Toggle must flip checked on first click"
        );
    }

    // 2. SectionHeader Toggle ("Short breaks", checked = true -> solid accent pill on the right).
    {
        let (ui, window) = build_settings();
        let changed = Rc::new(Cell::new(0u32));
        let changed_cb = Rc::clone(&changed);
        ui.on_changed(move || changed_cb.set(changed_cb.get() + 1));

        let img = layout_and_render(&window, 560.0, || 1600.0);
        // Second accent pill on the right side sits at the Short breaks SectionHeader (y in 95..170).
        let header_toggle = bounding_box(&img, 450, 95, 540, 170, |px| near(px, LIGHT_ACCENT, 8))
            .expect("Short breaks SectionHeader toggle");
        let (cx, cy) = center_of(header_toggle);
        click_once(&window, cx, cy);
        assert_eq!(
            changed.get(),
            1,
            "SectionHeader Toggle must fire changed once on first click"
        );
        assert!(
            !ui.get_short_enabled(),
            "SectionHeader Toggle must flip short_enabled on first click"
        );
    }

    // 3. NumberField stepper ("Short break interval", up and down chevrons).
    {
        let (ui, window) = build_settings();
        let changed = Rc::new(Cell::new(0u32));
        let changed_cb = Rc::clone(&changed);
        ui.on_changed(move || changed_cb.set(changed_cb.get() + 1));

        let img = layout_and_render(&window, 560.0, || 1600.0);
        // Control column starts at x = 560 - 16 - 16 - 132 = 396; NumberField box is 92x28 (x=396..488).
        // Scan right below the Short breaks SectionHeader toggle for the 92 px wide border.
        let header_toggle = bounding_box(&img, 450, 95, 540, 170, |px| near(px, LIGHT_ACCENT, 8))
            .expect("Short breaks SectionHeader toggle");
        let box_rect = bounding_box(
            &img,
            395,
            header_toggle.3 + 4,
            490,
            header_toggle.3 + 48,
            |px| !near(px, LIGHT_SURFACE, 12),
        )
        .expect("Short break interval NumberField box");
        let up_x = box_rect.2 as f32 - 9.0;
        let up_y = box_rect.1 as f32 + 7.0;
        click_once(&window, up_x, up_y);
        assert_eq!(
            changed.get(),
            1,
            "NumberField up stepper must fire changed once on first click"
        );
        assert_eq!(ui.get_short_interval(), 11);

        // Also verify down stepper on a fresh window!
        let (ui2, window2) = build_settings();
        let changed2 = Rc::new(Cell::new(0u32));
        let changed2_cb = Rc::clone(&changed2);
        ui2.on_changed(move || changed2_cb.set(changed2_cb.get() + 1));
        let _ = layout_and_render(&window2, 560.0, || 1600.0);
        let down_y = box_rect.3 as f32 - 7.0;
        click_once(&window2, up_x, down_y);
        assert_eq!(
            changed2.get(),
            1,
            "NumberField down stepper must fire changed once on first click"
        );
        assert_eq!(ui2.get_short_interval(), 9);
        assert_eq!(ui2.get_short_duration(), 15);
    }

    // 4. Both "Try..." LinkButtons ("Try a short break" and "Try a long break").
    {
        let (ui, window) = build_settings();
        let try_short = Rc::new(Cell::new(0u32));
        let try_short_cb = Rc::clone(&try_short);
        ui.on_try_short_break(move || try_short_cb.set(try_short_cb.get() + 1));

        let img = layout_and_render(&window, 560.0, || 1600.0);
        // Find accent pixels on the left side (x in 28..200) — the two LinkButtons are the only
        // accent-coloured text on the left side!
        let short_link = bounding_box(&img, 28, 250, 220, 480, |px| near(px, LIGHT_ACCENT, 25))
            .expect("Try a short break LinkButton");
        let (cx, cy) = center_of(short_link);
        click_once(&window, cx, cy);
        assert_eq!(
            try_short.get(),
            1,
            "Try a short break LinkButton must fire on first click"
        );

        let (ui2, window2) = build_settings();
        let try_long = Rc::new(Cell::new(0u32));
        let try_long_cb = Rc::clone(&try_long);
        ui2.on_try_long_break(move || try_long_cb.set(try_long_cb.get() + 1));

        let img2 = layout_and_render(&window2, 560.0, || 1600.0);
        let long_link = bounding_box(&img2, 28, 600, 220, 1000, |px| near(px, LIGHT_ACCENT, 25))
            .expect("Try a long break LinkButton");
        let (cx2, cy2) = center_of(long_link);
        click_once(&window2, cx2, cy2);
        assert_eq!(
            try_long.get(),
            1,
            "Try a long break LinkButton must fire on first click"
        );
    }

    // 5. Realistic scrollable 560x640 SettingsWindow (Flickable active): Toggle & Try short still fire on first click.
    {
        let (ui, window) = build_settings();
        let changed = Rc::new(Cell::new(0u32));
        let changed_cb = Rc::clone(&changed);
        ui.on_changed(move || changed_cb.set(changed_cb.get() + 1));

        let img = layout_and_render(&window, 560.0, || 640.0);
        let toggle_box = bounding_box(&img, 450, 20, 540, 95, |px| near(px, LIGHT_ACCENT, 8))
            .expect("Start with system toggle pill at 560x640");
        let (cx, cy) = center_of(toggle_box);
        click_once(&window, cx, cy);
        assert_eq!(
            changed.get(),
            1,
            "Scrollable 560x640 Settings Toggle must fire on first click"
        );
        assert!(!ui.get_start_with_system());
    }
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
