//! K1: the kit Dropdown with a long model (40 languages) opens a capped, scrollable list that
//! shows the current item, follows the keyboard (arrows, type-ahead, Enter, Esc), and a short model
//! keeps the old, uncapped list. Set `OCY_DROPDOWN_RENDER=<dir>` to also write the open list as PNGs.
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Key, Platform, WindowAdapter, WindowEvent};
use slint::{PhysicalSize, SharedString};
use slint_interpreter::{ComponentInstance, Value};

thread_local! {
    static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct Headless;

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let w = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        WINDOW.with(|s| *s.borrow_mut() = Some(w.clone()));
        Ok(w)
    }
}

const PROBE: &str = r#"
import { Dropdown } from "@damt-ui/kit.slint";
import { Theme } from "@app-theme/theme.slint";
export component Probe inherits Window {
    width: 320px;
    height: 420px;
    in property <[string]> items;
    in-out property <int> index;
    in property <bool> dark;
    out property <length> list-height: dd.list-height;
    out property <bool> list-scrolls: dd.list-scrolls;
    out property <length> list-offset: dd.list-offset;
    out property <int> highlighted: dd.highlighted;
    public function open() { dd.open(); }
    changed dark => { Theme.dark = self.dark; }
    background: Theme.bg;
    dd := Dropdown {
        x: 24px;
        y: 96px;
        width: 220px;
        model: root.items;
        current-index <=> root.index;
        accessible-name: "Language";
        window-height: root.height;
    }
}
"#;

/// 40 endonym-like names with varied first letters (sorted like the real list).
fn languages() -> Vec<String> {
    [
        "Automatic",
        "Bahasa Indonesia",
        "Bosanski",
        "Čeština",
        "Dansk",
        "Deutsch",
        "Eesti",
        "English",
        "Español",
        "Français",
        "Hrvatski",
        "Italiano",
        "Latviešu",
        "Lietuvių",
        "Magyar",
        "Nederlands",
        "Norsk bokmål",
        "Polski",
        "Português (Brasil)",
        "Português (Portugal)",
        "Română",
        "Shqip",
        "Slovenčina",
        "Slovenščina",
        "Srpski",
        "Suomi",
        "Svenska",
        "Tiếng Việt",
        "Türkçe",
        "Ελληνικά",
        "Български",
        "Македонски",
        "Русский",
        "Српски",
        "Українська",
        "हिन्दी",
        "ไทย",
        "日本語",
        "简体中文",
        "한국어",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

fn probe(items: &[String], index: i32) -> (ComponentInstance, Rc<MinimalSoftwareWindow>) {
    let _ = slint::platform::set_platform(Box::new(Headless));
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut paths = HashMap::new();
    paths.insert(damt_ui::LIBRARY_NAME.to_owned(), damt_ui::ui_dir());
    paths.insert(
        damt_ui::THEME_LIBRARY_NAME.to_owned(),
        manifest.join("../theme/generated"),
    );
    let mut compiler = slint_interpreter::Compiler::default();
    compiler.set_library_paths(paths);
    compiler.set_style("fluent".into());
    let result = spin_on::spin_on(
        compiler.build_from_source(PROBE.to_owned(), manifest.join("ui/probe.slint")),
    );
    let errors: Vec<String> = result
        .diagnostics()
        .filter(|d| d.level() == slint_interpreter::DiagnosticLevel::Error)
        .map(|d| d.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let ui = result.component("Probe").unwrap().create().unwrap();
    let window = WINDOW.with(|s| s.borrow_mut().take()).unwrap();
    let model: Vec<Value> = items
        .iter()
        .map(|s| Value::String(s.as_str().into()))
        .collect();
    ui.set_property(
        "items",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(model))),
    )
    .unwrap();
    ui.set_property("index", Value::Number(f64::from(index)))
        .unwrap();
    window.set_size(PhysicalSize::new(320, 420));
    (ui, window)
}

fn num(ui: &ComponentInstance, name: &str) -> f64 {
    match ui.get_property(name) {
        Ok(Value::Number(n)) => n,
        other => panic!("{name}: {other:?}"),
    }
}

fn flag(ui: &ComponentInstance, name: &str) -> bool {
    matches!(ui.get_property(name), Ok(Value::Bool(true)))
}

fn paint(window: &MinimalSoftwareWindow) -> Vec<PremultipliedRgbaColor> {
    let mut buffer = vec![PremultipliedRgbaColor::default(); 320 * 420];
    window.request_redraw();
    window.draw_if_needed(|r| {
        r.render(&mut buffer, 320);
    });
    buffer
}

fn key(window: &MinimalSoftwareWindow, text: SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
    paint(window);
}

/// Row `i` (28 px rows after 4 px padding) lies fully inside the visible part of the list.
fn row_visible(ui: &ComponentInstance, i: i32) -> bool {
    let top = 4.0 + 28.0 * f64::from(i);
    let off = num(ui, "list-offset");
    let h = num(ui, "list-height");
    top >= off && top + 28.0 <= off + h
}

#[test]
fn long_list_is_capped_scrolls_and_follows_the_keyboard() {
    let langs = languages();
    let (ui, window) = probe(&langs, 30);
    paint(&window);
    ui.invoke("open", &[]).unwrap();
    paint(&window);

    let h = num(&ui, "list-height");
    let content = 40.0 * 28.0 + 8.0;
    assert!(h < content && h <= 10.0 * 28.0 + 8.0, "list height {h}");
    // Fits in the 420 px window below or above the box (box at y 96..124).
    assert!(
        h <= 420.0 - 124.0 - 10.0 || h <= 96.0 - 10.0,
        "list height {h} overflows"
    );
    assert!(flag(&ui, "list-scrolls"));
    assert_eq!(num(&ui, "highlighted"), 30.0);
    assert!(row_visible(&ui, 30), "current row not in view on open");

    // Down three times: highlight moves, stays in view, selection unchanged until Enter.
    for _ in 0..3 {
        key(&window, Key::DownArrow.into());
    }
    assert_eq!(num(&ui, "highlighted"), 33.0);
    assert!(row_visible(&ui, 33));
    assert_eq!(num(&ui, "index"), 30.0);

    // Home, then type-ahead "s" twice: Shqip (21), then Slovenčina (22).
    key(&window, Key::Home.into());
    assert_eq!(num(&ui, "highlighted"), 0.0);
    assert!(row_visible(&ui, 0));
    key(&window, "s".into());
    assert_eq!(num(&ui, "highlighted"), 21.0);
    key(&window, "S".into());
    assert_eq!(num(&ui, "highlighted"), 22.0);
    assert!(row_visible(&ui, 22));
    key(&window, Key::End.into());
    assert_eq!(num(&ui, "highlighted"), 39.0);
    assert!(row_visible(&ui, 39));

    if let Some(dir) = std::env::var_os("OCY_DROPDOWN_RENDER") {
        for dark in [false, true] {
            ui.set_property("dark", Value::Bool(dark)).unwrap();
            key(&window, "d".into()); // Dansk (4)
            let px = paint(&window);
            let name = if dark {
                "dropdown-open-dark.png"
            } else {
                "dropdown-open-light.png"
            };
            write_png(&PathBuf::from(&dir).join(name), &px);
        }
        key(&window, Key::End.into());
    }

    // Enter picks the highlight and closes the list.
    key(&window, "\n".into());
    assert_eq!(num(&ui, "index"), 39.0);

    // Re-open, Esc closes without changing the selection.
    ui.invoke("open", &[]).unwrap();
    paint(&window);
    assert!(row_visible(&ui, 39));
    key(&window, Key::UpArrow.into());
    key(&window, Key::Escape.into());
    assert_eq!(num(&ui, "index"), 39.0);
}

#[test]
fn short_list_is_unchanged() {
    let items: Vec<String> = ["Automatic", "English", "Deutsch"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let (ui, window) = probe(&items, 1);
    paint(&window);
    ui.invoke("open", &[]).unwrap();
    paint(&window);
    assert_eq!(num(&ui, "list-height"), 3.0 * 28.0 + 8.0);
    assert!(!flag(&ui, "list-scrolls"));
    assert_eq!(num(&ui, "list-offset"), 0.0);
    assert_eq!(num(&ui, "highlighted"), 1.0);
}

fn write_png(path: &std::path::Path, px: &[PremultipliedRgbaColor]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 320, 420);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let flat: Vec<u8> = px
        .iter()
        .flat_map(|p| {
            let un = |c: u8| {
                if p.alpha == 0 {
                    0
                } else {
                    ((u32::from(c) * 255 + u32::from(p.alpha) / 2) / u32::from(p.alpha)).min(255)
                        as u8
                }
            };
            [un(p.red), un(p.green), un(p.blue), p.alpha]
        })
        .collect();
    enc.write_header().unwrap().write_image_data(&flat).unwrap();
}
