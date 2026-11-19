//! Real Slint [`SettingsWindow`] implementation (`ocy_ui::SettingsWindow`).

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use damt_app::ErrorLog;
use damt_overlay::enumerate_monitors;
use damt_platform::{Clock, PlatformServices};
use ocy_core::settings::OcySettings;
use ocy_ui::SettingsWindow as UiSettingsWindow;
use slint::winit_030::{
    EventResult, WinitWindowAccessor, winit::dpi::PhysicalPosition as WinitPhysicalPosition,
    winit::event::WindowEvent,
};
use slint::{CloseRequestResponse, ComponentHandle, PhysicalPosition, Timer};

use crate::break_logic::AppThemeTracker;
use crate::coordinator::SettingsWindow;
use crate::layout::{DipSize, OverlayLayout, ScreenInfo, select_active_screen};
use crate::settings_logic::{
    ManagedSettingsWindow, SettingsBackend, SettingsCallbacks, SettingsTexts, SettingsUiHandlers,
    SettingsValues,
};
use crate::strings::Strings;
use crate::tray::TaskbarIconTracker;
use crate::win32_window::{WindowRole, WindowRoleGuard, apply_window_icon};

/// Default logical dimensions of `ocy_ui::SettingsWindow` (matches `preferred-width`/`height` in `ocy.slint`).
const SETTINGS_WINDOW_DIP_SIZE: DipSize = DipSize::new(560.0, 640.0);

fn apply_texts(ui: &UiSettingsWindow, texts: &SettingsTexts) {
    ui.set_window_title(texts.window_title.as_str().into());
    ui.set_general_title(texts.general_title.as_str().into());
    ui.set_short_breaks_title(texts.short_breaks_title.as_str().into());
    ui.set_long_breaks_title(texts.long_breaks_title.as_str().into());
    ui.set_behaviour_title(texts.behaviour_title.as_str().into());
    ui.set_about_title(texts.about_title.as_str().into());
    ui.set_start_with_system_label(texts.start_with_system_label.as_str().into());
    ui.set_start_with_system_help(texts.start_with_system_help.as_str().into());
    ui.set_short_enabled_label(texts.short_enabled_label.as_str().into());
    ui.set_short_interval_label(texts.short_interval_label.as_str().into());
    ui.set_short_interval_help(texts.short_interval_help.as_str().into());
    ui.set_short_duration_label(texts.short_duration_label.as_str().into());
    ui.set_short_duration_help(texts.short_duration_help.as_str().into());
    ui.set_short_fullscreen_label(texts.short_fullscreen_label.as_str().into());
    ui.set_short_fullscreen_help(texts.short_fullscreen_help.as_str().into());
    ui.set_try_short_label(texts.try_short_label.as_str().into());
    ui.set_long_enabled_label(texts.long_enabled_label.as_str().into());
    ui.set_long_interval_label(texts.long_interval_label.as_str().into());
    ui.set_long_interval_help(texts.long_interval_help.as_str().into());
    ui.set_long_duration_label(texts.long_duration_label.as_str().into());
    ui.set_long_duration_help(texts.long_duration_help.as_str().into());
    ui.set_long_warning_label(texts.long_warning_label.as_str().into());
    ui.set_long_warning_help(texts.long_warning_help.as_str().into());
    ui.set_long_postpone_label(texts.long_postpone_label.as_str().into());
    ui.set_long_postpone_help(texts.long_postpone_help.as_str().into());
    ui.set_long_max_postpones_label(texts.long_max_postpones_label.as_str().into());
    ui.set_long_max_postpones_help(texts.long_max_postpones_help.as_str().into());
    ui.set_try_long_label(texts.try_long_label.as_str().into());
    ui.set_strict_label(texts.strict_label.as_str().into());
    ui.set_strict_help(texts.strict_help.as_str().into());
    ui.set_inactivity_label(texts.inactivity_label.as_str().into());
    ui.set_inactivity_help(texts.inactivity_help.as_str().into());
    ui.set_away_label(texts.away_label.as_str().into());
    ui.set_away_help(texts.away_help.as_str().into());
    ui.set_pause_fullscreen_label(texts.pause_fullscreen_label.as_str().into());
    ui.set_pause_fullscreen_help(texts.pause_fullscreen_help.as_str().into());
    ui.set_window_nearby_label(texts.window_nearby_label.as_str().into());
    ui.set_window_nearby_help(texts.window_nearby_help.as_str().into());
    ui.set_unit_minutes(texts.unit_minutes.as_str().into());
    ui.set_unit_seconds(texts.unit_seconds.as_str().into());
    ui.set_about_name_version(texts.about_name_version.as_str().into());
    ui.set_about_tagline(texts.about_tagline.as_str().into());
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
    ui.set_strict_mode(values.strict_mode);
    ui.set_inactivity_tracking(values.inactivity_tracking);
    ui.set_away_threshold(values.away_threshold);
    ui.set_pause_fullscreen(values.pause_fullscreen);
    ui.set_window_nearby(values.window_nearby);
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
        strict_mode: ui.get_strict_mode(),
        inactivity_tracking: ui.get_inactivity_tracking(),
        away_threshold: ui.get_away_threshold(),
        pause_fullscreen: ui.get_pause_fullscreen(),
        window_nearby: ui.get_window_nearby(),
    }
}

/// [`SettingsBackend`] backed by Slint's [`ocy_ui::SettingsWindow`].
#[derive(Clone)]
pub struct SlintSettingsBackend {
    platform: Arc<dyn PlatformServices>,
    error_log: ErrorLog,
    use_light_icon_set: Rc<Cell<bool>>,
}

impl fmt::Debug for SlintSettingsBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlintSettingsBackend")
            .field("error_log", &self.error_log)
            .field("use_light_icon_set", &self.use_light_icon_set.get())
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
        Self {
            platform,
            error_log,
            use_light_icon_set,
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
        texts: &SettingsTexts,
        values: &SettingsValues,
        show_start_with_system: bool,
        dark: bool,
        handlers: SettingsUiHandlers,
    ) -> Result<Self::Instance, String> {
        let _role = WindowRoleGuard::new(WindowRole::Normal);
        let ui = UiSettingsWindow::new().map_err(|err| err.to_string())?;
        ocy_ui::set_dark(&ui, dark);

        ui.set_show_start_with_system(show_start_with_system);
        ui.set_save_error("".into());
        ui.set_start_with_system_error("".into());
        apply_texts(&ui, texts);
        apply_values(&ui, values);

        let on_changed = Rc::clone(&handlers.on_changed);
        ui.on_changed(move || on_changed());

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

        let active_screen = self.active_monitor_info();
        if let Some(screen) = active_screen {
            let initial_pos = OverlayLayout::compute_centered_work_area_position(
                &screen,
                SETTINGS_WINDOW_DIP_SIZE,
            );
            ui.window()
                .set_position(PhysicalPosition::new(initial_pos.x, initial_pos.y));
        }

        let icons_applied = Rc::new(Cell::new(false));
        let icons_applied_ev = Rc::clone(&icons_applied);
        let light_icon_ev = Rc::clone(&self.use_light_icon_set);
        ui.window().on_winit_window_event(move |win, event| {
            if (!icons_applied_ev.get() || matches!(event, WindowEvent::ScaleFactorChanged { .. }))
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

    fn set_dark(&self, instance: &Self::Instance, dark: bool) {
        ocy_ui::set_dark(instance, dark);
    }

    fn set_light_icon_set(&self, instance: &Self::Instance, use_light_set: bool) {
        self.use_light_icon_set.set(use_light_set);
        let _ = apply_window_icon(instance.window(), use_light_set);
    }

    fn read_values(&self, instance: &Self::Instance) -> SettingsValues {
        read_ui_values(instance)
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

    fn destroy_on_close(&self, instance: Self::Instance) {
        // Defer dropping the component until after `on_close_requested` finishes hiding it.
        Timer::single_shot(Duration::ZERO, move || drop(instance));
    }

    fn hide_and_destroy(&self, instance: Self::Instance) {
        let _ = instance.hide();
        drop(instance);
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
        let icon_tracker =
            TaskbarIconTracker::new(platform.taskbar_theme(), platform.app_theme(), clock);
        let use_light_icon_set = Rc::new(Cell::new(icon_tracker.use_light_set()));
        let backend = SlintSettingsBackend::with_icon_theme(
            Arc::clone(&platform),
            error_log.clone(),
            Rc::clone(&use_light_icon_set),
        );
        let inner = ManagedSettingsWindow::new(
            backend,
            Arc::clone(&platform),
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
