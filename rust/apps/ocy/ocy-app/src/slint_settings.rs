//! Real Slint [`SettingsWindow`] implementation (`ocy_ui::SettingsWindowV2`).

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_app::ErrorLog;
use damt_overlay::enumerate_monitors;
use damt_platform::{Clock, PlatformServices};
use ocy_core::engine::EngineSnapshot;
use ocy_core::settings::OcySettings;
use ocy_core::statistics::StatisticsData;
use ocy_ui::{SettingsWindowV2 as UiSettingsWindow, SilentGame, TimelineMark as UiTimelineMark};
use slint::winit_030::{
    EventResult, WinitWindowAccessor, winit::dpi::PhysicalPosition as WinitPhysicalPosition,
    winit::event::WindowEvent,
};
use slint::{
    CloseRequestResponse, ComponentHandle, LogicalSize, Model, ModelRc, PhysicalPosition,
    SharedString, Timer, TimerMode, VecModel,
};

use crate::art::{OcyArt, art_theme, frame_and_next_delay};
use crate::break_logic::AppThemeTracker;
use crate::coordinator::SettingsWindow;
use crate::layout::{
    DipSize, OverlayLayout, ScreenInfo, select_active_screen, settings_window_height,
    settings_window_top, settings_window_width,
};
#[cfg(feature = "store")]
use crate::license::{LicenseState, StoreLicenseTexts};
use crate::mode_art::ModeArt;
use crate::settings_logic::{
    ManagedSettingsWindow, SettingsBackend, SettingsCallbacks, SettingsCreateParams,
    SettingsLiveView, SettingsTexts, SettingsUiHandlers, SettingsValues, format_slider_duration,
    format_slider_every_minutes, format_slider_minutes, format_slider_seconds, format_slider_times,
};
use crate::strings::Strings;
use crate::tray::TaskbarIconTracker;
use crate::win32_window::{WindowRole, WindowRoleGuard, apply_window_icon, raw_win32_hwnd};

/// Default logical dimensions of `ocy_ui::SettingsWindowV2` (matches `preferred-width`/`height` in `settings_v2.slint`).
const SETTINGS_WINDOW_DIP_SIZE: DipSize = DipSize::new(560.0, 640.0);

/// Exercise id and logical size used for the static sprout mascot in the Settings V2 header.
const MASCOT_EXERCISE_ID: &str = "blink";
const MASCOT_LOGICAL_PX: f64 = 48.0;

fn apply_texts(ui: &UiSettingsWindow, texts: &SettingsTexts) {
    ui.set_font_family(texts.font_family.as_str().into());
    ui.set_window_title(texts.window_title.as_str().into());
    ui.set_today_title(texts.today_title.as_str().into());
    ui.set_today_taken_label(texts.today_taken_label.as_str().into());
    ui.set_today_skipped_label(texts.today_skipped_label.as_str().into());
    ui.set_timeline_title(texts.timeline_title.as_str().into());
    ui.set_timeline_short_label(texts.timeline_short_label.as_str().into());
    ui.set_timeline_long_label(texts.timeline_long_label.as_str().into());
    ui.set_now_label(texts.now_label.as_str().into());
    ui.set_preset_gentle_label(texts.preset_gentle_label.as_str().into());
    ui.set_preset_balanced_label(texts.preset_balanced_label.as_str().into());
    ui.set_preset_focused_label(texts.preset_focused_label.as_str().into());
    ui.set_preset_custom_label(texts.preset_custom_label.as_str().into());
    ui.set_preset_gentle_summary(texts.preset_gentle_summary.as_str().into());
    ui.set_preset_balanced_summary(texts.preset_balanced_summary.as_str().into());
    ui.set_preset_focused_summary(texts.preset_focused_summary.as_str().into());
    ui.set_customize_title(texts.customize_title.as_str().into());
    ui.set_general_title(texts.general_title.as_str().into());
    ui.set_tab_breaks_label(texts.tab_breaks_label.as_str().into());
    ui.set_tab_strictness_label(texts.tab_strictness_label.as_str().into());
    ui.set_tab_quiet_label(texts.tab_quiet_label.as_str().into());
    ui.set_start_with_system_label(texts.start_with_system_label.as_str().into());
    ui.set_start_with_system_help(texts.start_with_system_help.as_str().into());
    ui.set_short_enabled_label(texts.short_enabled_label.as_str().into());
    ui.set_short_interval_label(texts.short_interval_label.as_str().into());
    ui.set_short_duration_label(texts.short_duration_label.as_str().into());
    ui.set_short_fullscreen_label(texts.short_fullscreen_label.as_str().into());
    ui.set_short_fullscreen_help(texts.short_fullscreen_help.as_str().into());
    ui.set_try_short_label(texts.try_short_label.as_str().into());
    ui.set_long_enabled_label(texts.long_enabled_label.as_str().into());
    ui.set_long_interval_label(texts.long_interval_label.as_str().into());
    ui.set_long_duration_label(texts.long_duration_label.as_str().into());
    ui.set_long_warning_label(texts.long_warning_label.as_str().into());
    ui.set_long_postpone_label(texts.long_postpone_label.as_str().into());
    ui.set_long_max_postpones_label(texts.long_max_postpones_label.as_str().into());
    ui.set_try_long_label(texts.try_long_label.as_str().into());
    ui.set_water_nudges_label(texts.water_nudges_label.as_str().into());
    ui.set_water_nudges_help(texts.water_nudges_help.as_str().into());
    ui.set_strict_label(texts.strict_label.as_str().into());
    ui.set_strict_help(texts.strict_help.as_str().into());
    ui.set_inactivity_label(texts.inactivity_label.as_str().into());
    ui.set_inactivity_help(texts.inactivity_help.as_str().into());
    ui.set_away_label(texts.away_label.as_str().into());
    ui.set_pause_fullscreen_label(texts.pause_fullscreen_label.as_str().into());
    ui.set_pause_fullscreen_help(texts.pause_fullscreen_help.as_str().into());
    ui.set_games_title(texts.games_title.as_str().into());
    let games_modes: Vec<SharedString> = texts
        .games_mode_model
        .iter()
        .map(|label| SharedString::from(label.as_str()))
        .collect();
    ui.set_games_mode_model(ModelRc::new(VecModel::from(games_modes)));
    ui.set_games_gentle_help(texts.games_gentle_help.as_str().into());
    ui.set_games_hold_help(texts.games_hold_help.as_str().into());
    ui.set_games_silent_title(texts.games_silent_title.as_str().into());
    ui.set_games_silent_empty(texts.games_silent_empty.as_str().into());
    ui.set_window_nearby_label(texts.window_nearby_label.as_str().into());
    ui.set_window_nearby_help(texts.window_nearby_help.as_str().into());
    ui.set_language_label(texts.language_label.as_str().into());
    ui.set_sounds_label(texts.sounds_label.as_str().into());
    ui.set_sounds_help(texts.sounds_help.as_str().into());
    ui.set_notifications_label(texts.notifications_label.as_str().into());
    ui.set_notifications_help(texts.notifications_help.as_str().into());
    ui.set_about_name_version(texts.about_name_version.as_str().into());
    ui.set_about_tagline(texts.about_tagline.as_str().into());

    let model: Vec<slint::SharedString> = texts
        .language_model
        .iter()
        .map(|s| s.as_str().into())
        .collect();
    ui.set_language_model(slint::ModelRc::new(VecModel::from(model)));
}

fn apply_values(ui: &UiSettingsWindow, values: &SettingsValues) {
    ui.set_start_with_system(values.start_with_system);
    ui.set_short_enabled(values.short_enabled);
    ui.set_short_interval(values.short_interval);
    ui.set_short_duration(values.short_duration);
    ui.set_short_fullscreen(values.short_fullscreen);
    ui.set_long_enabled(values.long_enabled);
    ui.set_long_interval(values.long_interval);
    ui.set_long_duration(values.long_duration);
    ui.set_long_warning(values.long_warning);
    ui.set_long_postpone(values.long_postpone);
    ui.set_long_max_postpones(values.long_max_postpones);
    ui.set_water_nudges(values.water_nudges);
    ui.set_strict_mode(values.strict_mode);
    ui.set_inactivity_tracking(values.inactivity_tracking);
    ui.set_away_threshold(values.away_threshold);
    ui.set_pause_fullscreen(values.pause_fullscreen);
    ui.set_games_mode_index(i32::from(values.games_hold));
    ui.set_window_nearby(values.window_nearby);
    ui.set_status_notifications(values.status_notifications);
}

fn apply_preset_sliders(ui: &UiSettingsWindow, preset: i32, values: &SettingsValues) {
    ui.set_preset(preset);
    ui.set_short_interval(values.short_interval);
    ui.set_short_duration(values.short_duration);
    ui.set_long_interval(values.long_interval);
    ui.set_long_duration(values.long_duration);
}

fn read_ui_values(ui: &UiSettingsWindow) -> SettingsValues {
    SettingsValues {
        start_with_system: ui.get_start_with_system(),
        short_enabled: ui.get_short_enabled(),
        short_interval: ui.get_short_interval(),
        short_duration: ui.get_short_duration(),
        short_fullscreen: ui.get_short_fullscreen(),
        long_enabled: ui.get_long_enabled(),
        long_interval: ui.get_long_interval(),
        long_duration: ui.get_long_duration(),
        long_warning: ui.get_long_warning(),
        long_postpone: ui.get_long_postpone(),
        long_max_postpones: ui.get_long_max_postpones(),
        water_nudges: ui.get_water_nudges(),
        strict_mode: ui.get_strict_mode(),
        inactivity_tracking: ui.get_inactivity_tracking(),
        away_threshold: ui.get_away_threshold(),
        pause_fullscreen: ui.get_pause_fullscreen(),
        window_nearby: ui.get_window_nearby(),
        status_notifications: ui.get_status_notifications(),
        games_hold: ui.get_games_mode_index() == 1,
        games_silent: ui
            .get_games_silent()
            .iter()
            .map(|game| game.name.to_string())
            .collect(),
    }
}

/// Fills the silent-games list, each row with its "Remove {name}" accessible name.
fn apply_games_silent(ui: &UiSettingsWindow, strings: &Strings, names: &[String]) {
    let rows: Vec<SilentGame> = names
        .iter()
        .map(|name| SilentGame {
            name: name.as_str().into(),
            remove_label: strings
                .format("settings.games.remove", &[("name", name)])
                .into(),
        })
        .collect();
    ui.set_games_silent(ModelRc::new(VecModel::from(rows)));
}

fn apply_live_view(ui: &UiSettingsWindow, live_view: &SettingsLiveView) {
    ui.set_next_break_text(live_view.next_break_text.as_str().into());
    let marks: Vec<UiTimelineMark> = live_view
        .timeline_marks
        .iter()
        .copied()
        .map(Into::into)
        .collect();
    ui.set_timeline(slint::ModelRc::new(VecModel::from(marks)));
    let labels: Vec<slint::SharedString> = live_view
        .timeline_labels
        .iter()
        .map(|s| s.as_str().into())
        .collect();
    ui.set_timeline_labels(slint::ModelRc::new(VecModel::from(labels)));
    ui.set_today_taken(live_view.today.taken_i32());
    ui.set_today_skipped(live_view.today.skipped_i32());
}

fn bind_formatters(ui: &UiSettingsWindow, strings: &Rc<RefCell<Arc<Strings>>>) {
    let s = Rc::clone(strings);
    ui.on_format_every_minutes(move |v| format_slider_every_minutes(&s.borrow(), v).into());

    let s = Rc::clone(strings);
    ui.on_format_minutes(move |v| format_slider_minutes(&s.borrow(), v).into());

    let s = Rc::clone(strings);
    ui.on_format_seconds(move |v| format_slider_seconds(&s.borrow(), v).into());

    let s = Rc::clone(strings);
    ui.on_format_times(move |v| format_slider_times(&s.borrow(), v).into());

    let s = Rc::clone(strings);
    ui.on_format_duration(move |v| format_slider_duration(&s.borrow(), v).into());
}

/// Forces Slint `Slider.value-text` bindings (which only track their integer `value` property) to
/// re-evaluate after a live language switch without triggering `edited` or `changed()`.
fn refresh_slider_value_texts(ui: &UiSettingsWindow) {
    fn nudge(get: impl FnOnce() -> i32, set: impl Fn(i32), min: i32) {
        let cur = get();
        let tmp = if cur > min { cur - 1 } else { cur + 1 };
        set(tmp);
        set(cur);
    }
    nudge(|| ui.get_short_interval(), |v| ui.set_short_interval(v), 2);
    nudge(|| ui.get_short_duration(), |v| ui.set_short_duration(v), 5);
    nudge(|| ui.get_long_interval(), |v| ui.set_long_interval(v), 15);
    nudge(|| ui.get_long_duration(), |v| ui.set_long_duration(v), 1);
    nudge(|| ui.get_long_warning(), |v| ui.set_long_warning(v), 0);
    nudge(|| ui.get_long_postpone(), |v| ui.set_long_postpone(v), 1);
    nudge(
        || ui.get_long_max_postpones(),
        |v| ui.set_long_max_postpones(v),
        0,
    );
    nudge(|| ui.get_away_threshold(), |v| ui.set_away_threshold(v), 30);
}

/// Mode descriptions under the header's mode name (string keys `settings.v2.mode.<mode>.description`).
fn apply_mode_texts(ui: &UiSettingsWindow, strings: &Strings) {
    let d = |mode: &str| -> slint::SharedString {
        strings
            .get(&format!("settings.v2.mode.{mode}.description"))
            .into()
    };
    ui.set_mode_gentle_description(d("gentle"));
    ui.set_mode_balanced_description(d("balanced"));
    ui.set_mode_focused_description(d("focused"));
    ui.set_mode_custom_description(d("custom"));
}

/// Rasterises the placeholder mode scenes (header) and glyphs (preset cards) for `dark` at `scale`.
fn apply_mode_art(ui: &UiSettingsWindow, dark: bool, scale: f64, error_log: &ErrorLog) {
    let Some(mode_art) = ModeArt::shared() else {
        error_log.log_error(
            "settings_mode_art_load",
            &"embedded mode art failed to parse",
        );
        return;
    };
    let theme = art_theme(dark);
    match mode_art.scenes(theme, scale) {
        Ok(scenes) => ui.set_mode_scenes(ModelRc::new(VecModel::from(scenes))),
        Err(err) => error_log.log_error("settings_mode_scene_render", &err),
    }
    match mode_art.glyphs(theme, scale) {
        Ok(glyphs) => ui.set_mode_glyphs(ModelRc::new(VecModel::from(glyphs))),
        Err(err) => error_log.log_error("settings_mode_glyph_render", &err),
    }
}

/// Steps the header scene's `mode-frame` like the exercise flip-books; reduced motion holds the
/// manifest's frozen frame and starts no timer.
fn start_mode_animation(ui: &UiSettingsWindow, reduced_motion: bool) -> Option<Timer> {
    let (sequencer, frozen) = ModeArt::shared()?.scene_sequencer(ui.get_preset().clamp(0, 3))?;
    let frame_i32 = |f: u32| i32::try_from(f).unwrap_or(1);
    let (frame, next) = frame_and_next_delay(&sequencer, Duration::ZERO, reduced_motion, frozen);
    ui.set_mode_frame(frame_i32(frame));
    let step = next?;
    let started = std::time::Instant::now();
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, step, move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_mode_frame(frame_i32(sequencer.get_frame(started.elapsed())));
        }
    });
    Some(timer)
}

fn apply_mascot(ui: &UiSettingsWindow, art: &OcyArt, dark: bool, scale: f64, error_log: &ErrorLog) {
    apply_mode_art(ui, dark, scale, error_log);
    if let Some(res) = art.render_frozen(
        MASCOT_EXERCISE_ID,
        art_theme(dark),
        MASCOT_LOGICAL_PX,
        scale,
    ) {
        match res {
            Ok(img) => ui.set_mascot(img),
            Err(err) => error_log.log_error("settings_mascot_render", &err),
        }
    }
}

#[cfg(feature = "store")]
fn apply_license(
    ui: &UiSettingsWindow,
    license_state: &'static str,
    trial_days_left: i32,
    texts: &StoreLicenseTexts,
) {
    ui.set_license_state(license_state.into());
    ui.set_trial_days_left(trial_days_left);
    ui.set_trial_days_left_text(texts.trial_days_left_text.as_str().into());
    ui.set_trial_ended_title(texts.trial_ended_title.as_str().into());
    ui.set_trial_ended_body(texts.trial_ended_body.as_str().into());
    ui.set_buy_label(texts.buy_label.as_str().into());
}

/// Resizes the window to the height its content needs (see [`settings_window_height`]) and keeps it
/// on screen: centred on first show, afterwards growing and shrinking downwards.
fn fit_to_content(
    ui: &UiSettingsWindow,
    screen: Option<ScreenInfo>,
    content_dip: f64,
    expected: &Cell<f64>,
    expected_width: &Cell<f64>,
    user_resized: bool,
    centre: bool,
) {
    let Some(screen) = screen else { return };
    let scale = if screen.scaling > 0.0 {
        screen.scaling
    } else {
        1.0
    };
    let work_area = screen.effective_working_area();
    let window = ui.window();
    let current = window.size().to_logical(window.scale_factor());
    let target = settings_window_height(
        content_dip,
        f64::from(work_area.height) / scale,
        f64::from(current.height),
        user_resized,
    );
    let width = settings_window_width(
        f64::from(work_area.width) / scale,
        f64::from(current.width),
        user_resized,
    );
    if (target - f64::from(current.height)).abs() < 0.5
        && (width - f64::from(current.width)).abs() < 0.5
    {
        return;
    }
    expected.set(target);
    expected_width.set(width);
    #[allow(clippy::cast_possible_truncation)]
    window.set_size(LogicalSize::new(width as f32, target as f32));
    window.with_winit_window(|winit_win| {
        let outer = winit_win.outer_size();
        let inner = winit_win.inner_size();
        let frame = outer.height.saturating_sub(inner.height);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let new_outer = (target * scale).round() as u32 + frame;
        let centred =
            OverlayLayout::compute_centered_rect_position(work_area, outer.width, new_outer);
        let (x, y) = match winit_win.outer_position() {
            // Later fits open downwards: keep x and the top, shift up only past the work-area bottom.
            Ok(p) if !centre => (p.x, settings_window_top(p.y, new_outer, work_area)),
            Ok(p) => (p.x, centred.y),
            Err(_) => (centred.x, centred.y),
        };
        winit_win.set_outer_position(WinitPhysicalPosition::new(x, y));
    });
}

/// [`SettingsBackend`] backed by Slint's [`ocy_ui::SettingsWindowV2`].
#[derive(Clone)]
pub struct SlintSettingsBackend {
    platform: Arc<dyn PlatformServices>,
    error_log: ErrorLog,
    art: Rc<OcyArt>,
    strings: Rc<RefCell<Arc<Strings>>>,
    use_light_icon_set: Rc<Cell<bool>>,
    dark_theme: Rc<Cell<bool>>,
    scale: Rc<Cell<f64>>,
    refresh_timer: Rc<RefCell<Option<Timer>>>,
    mode_timer: Rc<RefCell<Option<Timer>>>,
}

impl fmt::Debug for SlintSettingsBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlintSettingsBackend")
            .field("error_log", &self.error_log)
            .field("use_light_icon_set", &self.use_light_icon_set.get())
            .field("dark_theme", &self.dark_theme.get())
            .finish_non_exhaustive()
    }
}

impl SlintSettingsBackend {
    /// Creates a new Slint backend using `platform` for monitor selection and `error_log` for diagnostics.
    #[must_use]
    pub fn new(platform: Arc<dyn PlatformServices>, error_log: ErrorLog) -> Self {
        Self::with_icon_theme(platform, error_log, Rc::new(Cell::new(false)))
    }

    fn with_icon_theme(
        platform: Arc<dyn PlatformServices>,
        error_log: ErrorLog,
        use_light_icon_set: Rc<Cell<bool>>,
    ) -> Self {
        let art = Rc::new(OcyArt::from_embedded().unwrap_or_else(|err| {
            error_log.log_error("art_load", &err);
            OcyArt::empty()
        }));
        Self {
            platform,
            error_log,
            art,
            strings: Rc::new(RefCell::new(Arc::new(Strings::default()))),
            use_light_icon_set,
            dark_theme: Rc::new(Cell::new(false)),
            scale: Rc::new(Cell::new(1.0)),
            refresh_timer: Rc::new(RefCell::new(None)),
            mode_timer: Rc::new(RefCell::new(None)),
        }
    }

    fn active_monitor_info(&self) -> Option<ScreenInfo> {
        let monitors = enumerate_monitors();
        let active_bounds = self.platform.active_screen().active_screen_bounds();
        select_active_screen(&monitors, active_bounds)
    }
}

impl SettingsBackend for SlintSettingsBackend {
    type Instance = UiSettingsWindow;

    fn create(
        &self,
        params: SettingsCreateParams<'_>,
        handlers: SettingsUiHandlers,
    ) -> Result<Self::Instance, String> {
        let _role = WindowRoleGuard::new(WindowRole::Normal);
        let ui = UiSettingsWindow::new().map_err(|err| err.to_string())?;
        self.dark_theme.set(params.dark);
        ocy_ui::set_dark(&ui, params.dark);

        *self.strings.borrow_mut() = Arc::clone(params.strings);
        bind_formatters(&ui, &self.strings);

        ui.set_show_start_with_system(params.show_start_with_system);
        ui.set_show_sounds(false);
        ui.set_show_notifications(crate::STATUS_NOTIFICATIONS_SHIPPED);
        ui.set_save_error("".into());
        ui.set_start_with_system_error("".into());
        #[cfg(feature = "store")]
        {
            // NOTE: `ocy-ui/ui/settings_v2.slint` is outside S1b's allowed scope; once the design
            // worker adds `in property <string> license-state`, `in property <int> trial-days-left`,
            // trial/buy strings, and `callback buy()` to `SettingsWindowV2`, bind them here:
            //   ui.set_license_state(params.license_state.into());
            //   ui.set_trial_days_left(params.trial_days_left);
            //   let on_buy = Rc::clone(&handlers.on_buy);
            //   ui.on_buy(move || on_buy());
            ui.set_store_build(true);
            apply_license(
                &ui,
                params.license_state,
                params.trial_days_left,
                params.store_texts,
            );
            ui.set_made_with_slint_label(params.strings.get("store.madeWithSlint").into());
            let on_buy = Rc::clone(&handlers.on_buy);
            ui.on_buy(move || on_buy());
        }

        apply_texts(&ui, params.texts);
        apply_values(&ui, params.values);
        apply_games_silent(&ui, params.strings, &params.values.games_silent);
        ui.set_preset(params.preset);
        ui.set_language_index(params.language_index);
        apply_live_view(&ui, params.live_view);

        let active_screen = self.active_monitor_info();
        let initial_scale = active_screen.map_or(1.0, |s| s.scaling);
        self.scale.set(initial_scale);
        apply_mascot(&ui, &self.art, params.dark, initial_scale, &self.error_log);
        apply_mode_texts(&ui, params.strings);
        *self.mode_timer.borrow_mut() =
            start_mode_animation(&ui, self.platform.motion().prefers_reduced_motion());

        let on_changed = Rc::clone(&handlers.on_changed);
        ui.on_changed(move || on_changed());

        let on_preset = Rc::clone(&handlers.on_preset_selected);
        ui.on_preset_selected(move |i| on_preset(i));

        let on_language = Rc::clone(&handlers.on_language_selected);
        ui.on_language_selected(move |i| on_language(i));

        // Removing a silent game drops its row, then saves through the usual debounced path.
        let on_changed_remove = Rc::clone(&handlers.on_changed);
        let strings_remove = Rc::clone(&self.strings);
        let weak_remove = ui.as_weak();
        ui.on_remove_silent_game(move |index| {
            let Some(ui) = weak_remove.upgrade() else {
                return;
            };
            let mut names = read_ui_values(&ui).games_silent;
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            if index < names.len() {
                names.remove(index);
                apply_games_silent(&ui, &strings_remove.borrow(), &names);
                on_changed_remove();
            }
        });

        let on_try_short = Rc::clone(&handlers.on_try_short_break);
        ui.on_try_short_break(move || on_try_short());

        let on_try_long = Rc::clone(&handlers.on_try_long_break);
        ui.on_try_long_break(move || on_try_long());

        // Esc closes the window like its close button: hide first, then let the manager drop it.
        let on_escape_close = Rc::clone(&handlers.on_close_requested);
        let weak_ui = ui.as_weak();
        ui.on_escape_pressed(move || {
            if let Some(ui) = weak_ui.upgrade() {
                let _ = ui.hide();
            }
            on_escape_close();
        });

        let on_close = Rc::clone(&handlers.on_close_requested);
        ui.window().on_close_requested(move || {
            on_close();
            CloseRequestResponse::HideWindow
        });

        if let Some(screen) = active_screen {
            let initial_pos = OverlayLayout::compute_centered_work_area_position(
                &screen,
                SETTINGS_WINDOW_DIP_SIZE,
            );
            ui.window()
                .set_position(PhysicalPosition::new(initial_pos.x, initial_pos.y));
        }

        // Grow / shrink to the content (no scrolling, so UI Automation sees every control) until the
        // user resizes the window by hand.
        let expected_height = Rc::new(Cell::new(SETTINGS_WINDOW_DIP_SIZE.height));
        let expected_width = Rc::new(Cell::new(SETTINGS_WINDOW_DIP_SIZE.width));
        let user_resized = Rc::new(Cell::new(false));
        {
            let expected = Rc::clone(&expected_height);
            let expected_w = Rc::clone(&expected_width);
            let resized = Rc::clone(&user_resized);
            let weak = ui.as_weak();
            ui.on_content_height_changed(move |content| {
                if let Some(ui) = weak.upgrade() {
                    fit_to_content(
                        &ui,
                        active_screen,
                        f64::from(content),
                        &expected,
                        &expected_w,
                        resized.get(),
                        false,
                    );
                }
            });
        }

        let icons_applied = Rc::new(Cell::new(false));
        let icons_applied_ev = Rc::clone(&icons_applied);
        let light_icon_ev = Rc::clone(&self.use_light_icon_set);
        let art_ev = Rc::clone(&self.art);
        let dark_ev = Rc::clone(&self.dark_theme);
        let scale_ev = Rc::clone(&self.scale);
        let err_ev = self.error_log.clone();
        let weak_ev = ui.as_weak();
        let expected_ev = Rc::clone(&expected_height);
        let expected_w_ev = Rc::clone(&expected_width);
        let resized_ev = Rc::clone(&user_resized);
        ui.window().on_winit_window_event(move |win, event| {
            if let WindowEvent::Resized(size) = event {
                let scale = scale_ev.get().max(0.1);
                let logical_h = f64::from(size.height) / scale;
                let logical_w = f64::from(size.width) / scale;
                if size.height > 0
                    && ((logical_h - expected_ev.get()).abs() > 2.0
                        || (logical_w - expected_w_ev.get()).abs() > 2.0)
                {
                    resized_ev.set(true);
                }
            }
            let scale_changed = matches!(event, WindowEvent::ScaleFactorChanged { .. });
            if let WindowEvent::ScaleFactorChanged { scale_factor, .. } = event {
                scale_ev.set(*scale_factor);
                if let Some(ui) = weak_ev.upgrade() {
                    apply_mascot(&ui, &art_ev, dark_ev.get(), *scale_factor, &err_ev);
                }
            }
            if (!icons_applied_ev.get() || scale_changed)
                && apply_window_icon(win, light_icon_ev.get())
            {
                icons_applied_ev.set(true);
            }
            EventResult::Propagate
        });

        ui.show().map_err(|err| err.to_string())?;
        if !icons_applied.get() && apply_window_icon(ui.window(), self.use_light_icon_set.get()) {
            icons_applied.set(true);
        }
        if let Some(screen) = active_screen {
            let work_area = screen.effective_working_area();
            ui.window().with_winit_window(|winit_win| {
                let outer = winit_win.outer_size();
                if outer.width > 0 && outer.height > 0 {
                    let pos = OverlayLayout::compute_centered_rect_position(
                        work_area,
                        outer.width,
                        outer.height,
                    );
                    winit_win.set_outer_position(WinitPhysicalPosition::new(pos.x, pos.y));
                }
            });
        }
        fit_to_content(
            &ui,
            active_screen,
            f64::from(ui.get_content_height()),
            &expected_height,
            &expected_width,
            user_resized.get(),
            true,
        );
        ui.window().with_winit_window(|winit_win| {
            winit_win.set_minimized(false);
            winit_win.focus_window();
        });

        Ok(ui)
    }

    fn activate(&self, instance: &Self::Instance) {
        if let Err(err) = instance.show() {
            self.error_log.log_error("settings_window_activate", &err);
        }
        instance.window().with_winit_window(|winit_win| {
            winit_win.set_minimized(false);
            winit_win.focus_window();
        });
    }

    fn set_texts(
        &self,
        instance: &Self::Instance,
        texts: &SettingsTexts,
        language_index: i32,
        strings: &Arc<Strings>,
    ) {
        *self.strings.borrow_mut() = Arc::clone(strings);
        apply_texts(instance, texts);
        apply_mode_texts(instance, strings);
        #[cfg(feature = "store")]
        instance.set_made_with_slint_label(strings.get("store.madeWithSlint").into());
        instance.set_language_index(language_index);
        let silent = read_ui_values(instance).games_silent;
        apply_games_silent(instance, strings, &silent);
        bind_formatters(instance, &self.strings);
        refresh_slider_value_texts(instance);
    }

    fn set_dark(&self, instance: &Self::Instance, dark: bool) {
        self.dark_theme.set(dark);
        ocy_ui::set_dark(instance, dark);
        apply_mascot(instance, &self.art, dark, self.scale.get(), &self.error_log);
    }

    fn set_light_icon_set(&self, instance: &Self::Instance, use_light_set: bool) {
        self.use_light_icon_set.set(use_light_set);
        let _ = apply_window_icon(instance.window(), use_light_set);
    }

    fn read_values(&self, instance: &Self::Instance) -> SettingsValues {
        read_ui_values(instance)
    }

    fn set_preset(&self, instance: &Self::Instance, preset: i32) {
        instance.set_preset(preset);
    }

    fn apply_preset_values(&self, instance: &Self::Instance, preset: i32, values: &SettingsValues) {
        apply_preset_sliders(instance, preset, values);
    }

    fn set_games_silent(&self, instance: &Self::Instance, names: &[String]) {
        apply_games_silent(instance, &self.strings.borrow(), names);
    }

    fn set_language_index(&self, instance: &Self::Instance, index: i32) {
        instance.set_language_index(index);
    }

    fn set_live_view(&self, instance: &Self::Instance, live_view: &SettingsLiveView) {
        apply_live_view(instance, live_view);
    }

    fn set_start_with_system(&self, instance: &Self::Instance, enabled: bool) {
        instance.set_start_with_system(enabled);
    }

    fn set_start_with_system_error(&self, instance: &Self::Instance, error: &str) {
        instance.set_start_with_system_error(error.into());
    }

    fn set_save_error(&self, instance: &Self::Instance, error: &str) {
        instance.set_save_error(error.into());
    }

    fn schedule_apply_timer(&self, delay: Duration, on_fire: Box<dyn FnOnce()>) {
        Timer::single_shot(delay, on_fire);
    }

    fn start_refresh_timer(&self, interval: Duration, on_tick: Box<dyn Fn()>) {
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, interval, on_tick);
        *self.refresh_timer.borrow_mut() = Some(timer);
    }

    fn stop_refresh_timer(&self) {
        if let Some(timer) = self.refresh_timer.borrow_mut().take() {
            timer.stop();
        }
    }

    fn destroy_on_close(&self, instance: Self::Instance) {
        if let Some(timer) = self.mode_timer.borrow_mut().take() {
            timer.stop();
        }
        // Defer dropping the component until after `on_close_requested` finishes hiding it.
        Timer::single_shot(Duration::ZERO, move || drop(instance));
    }

    fn hide_and_destroy(&self, instance: Self::Instance) {
        if let Some(timer) = self.mode_timer.borrow_mut().take() {
            timer.stop();
        }
        let _ = instance.hide();
        drop(instance);
    }

    #[cfg(feature = "store")]
    fn set_license_status(
        &self,
        instance: &Self::Instance,
        license_state: &'static str,
        trial_days_left: i32,
        store_texts: &StoreLicenseTexts,
    ) {
        apply_license(instance, license_state, trial_days_left, store_texts);
    }
}

/// Real Slint Settings window implementing [`SettingsWindow`].
pub struct SlintSettingsWindow {
    inner: ManagedSettingsWindow<SlintSettingsBackend>,
    platform: Arc<dyn PlatformServices>,
    theme_tracker: AppThemeTracker,
    icon_tracker: TaskbarIconTracker,
    use_light_icon_set: Rc<Cell<bool>>,
}

impl fmt::Debug for SlintSettingsWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlintSettingsWindow")
            .field("inner", &self.inner)
            .field("dark_theme", &self.theme_tracker.is_dark())
            .field("use_light_icon_set", &self.icon_tracker.use_light_set())
            .finish_non_exhaustive()
    }
}

impl SlintSettingsWindow {
    /// Creates a new [`SlintSettingsWindow`] controller.
    #[must_use]
    pub fn new(
        platform: Arc<dyn PlatformServices>,
        clock: Arc<dyn Clock>,
        strings: Arc<Strings>,
        error_log: ErrorLog,
        callbacks: SettingsCallbacks,
        initial_settings: OcySettings,
    ) -> Self {
        let theme_tracker = AppThemeTracker::new(platform.app_theme(), Arc::clone(&clock));
        let icon_tracker = TaskbarIconTracker::new(
            platform.taskbar_theme(),
            platform.app_theme(),
            Arc::clone(&clock),
        );
        let use_light_icon_set = Rc::new(Cell::new(icon_tracker.use_light_set()));
        let backend = SlintSettingsBackend::with_icon_theme(
            Arc::clone(&platform),
            error_log.clone(),
            Rc::clone(&use_light_icon_set),
        );
        let inner = ManagedSettingsWindow::with_clock(
            backend,
            Arc::clone(&platform),
            clock,
            strings,
            error_log,
            callbacks,
            initial_settings,
        );
        Self {
            inner,
            platform,
            theme_tracker,
            icon_tracker,
            use_light_icon_set,
        }
    }

    /// Returns a strong handle to the open [`ocy_ui::SettingsWindowV2`], if open.
    #[must_use]
    pub fn active_window(&self) -> Option<UiSettingsWindow> {
        self.inner
            .with_active_instance(ComponentHandle::clone_strong)
    }

    /// Returns the raw Win32 `HWND` of the open Settings window for modal Store dialogs
    /// (`IInitializeWithWindow`), or `None` if closed or on non-Windows targets.
    #[must_use]
    pub fn active_hwnd(&self) -> Option<isize> {
        self.inner
            .with_active_instance(|ui| raw_win32_hwnd(ui.window()))
            .flatten()
    }

    /// Registers the Store purchase callback invoked by `buy()`.
    #[cfg(feature = "store")]
    pub fn set_on_buy(&mut self, on_buy: Rc<dyn Fn()>) {
        self.inner.set_on_buy(on_buy);
    }

    /// Current Store license state exposed to the Settings window.
    #[cfg(feature = "store")]
    #[must_use]
    pub fn license_state(&self) -> LicenseState {
        self.inner.license_state()
    }

    /// Remaining trial days (`-1` when not in a trial with a known end date, `0` when expired).
    #[cfg(feature = "store")]
    #[must_use]
    pub fn trial_days_left(&self) -> i32 {
        self.inner.trial_days_left()
    }

    /// Invokes the registered Store purchase callback (`buy()`).
    #[cfg(feature = "store")]
    pub fn buy(&self) {
        self.inner.buy();
    }

    /// Returns `true` if changes are pending debounce/save.
    #[must_use]
    pub fn has_pending_changes(&self) -> bool {
        self.inner.has_pending_changes()
    }
}

impl SettingsWindow for SlintSettingsWindow {
    fn sync_settings(&mut self, settings: &OcySettings) {
        self.inner.sync_settings(settings);
        if self.inner.is_open() {
            if self.theme_tracker.refresh(self.platform.app_theme(), false) {
                self.inner.set_dark(self.theme_tracker.is_dark());
            }
            if self.icon_tracker.refresh(
                self.platform.taskbar_theme(),
                self.platform.app_theme(),
                false,
            ) {
                let use_light = self.icon_tracker.use_light_set();
                self.use_light_icon_set.set(use_light);
                self.inner.set_light_icon_set(use_light);
            }
        }
    }

    fn sync_live_status(&mut self, snapshot: &EngineSnapshot, stats: &StatisticsData) {
        self.inner.sync_live_status(snapshot, stats);
    }

    #[cfg(feature = "store")]
    fn sync_license(&mut self, state: LicenseState, trial_days_left: i32) {
        self.inner.sync_license(state, trial_days_left);
    }

    fn show(&mut self) {
        self.theme_tracker.refresh(self.platform.app_theme(), true);
        self.icon_tracker.refresh(
            self.platform.taskbar_theme(),
            self.platform.app_theme(),
            true,
        );
        let use_light = self.icon_tracker.use_light_set();
        self.use_light_icon_set.set(use_light);
        self.inner.set_light_icon_set(use_light);
        self.inner.show();
    }
}
