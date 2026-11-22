use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use damt_platform::{
    AutostartService, Clock, PlatformError, PlatformServices, TaskbarThemeProvider, Unsupported,
};
use ocy_app::coordinator::{BreakUiCoordinator, BreakWindows, SettingsWindow};
use ocy_app::host::AppHost;
use ocy_app::settings_logic::{
    ChangeAction, DemoPlatformServices, ManagedSettingsWindow, SETTINGS_APPLY_DELAY,
    SETTINGS_REFRESH_INTERVAL, SettingsBackend, SettingsCallbacks, SettingsCreateParams,
    SettingsDebounceState, SettingsLiveView, SettingsTexts, SettingsUiHandlers, SettingsValues,
    app_version, default_app_settings, format_next_break_text, format_slider_duration,
    format_slider_every_minutes, format_slider_minutes, format_slider_seconds, format_slider_times,
    load_app_settings,
};
use ocy_app::strings::Strings;
use ocy_app::tray::{
    AppTheme, TrayController, TrayIconState, TrayPresentation, TrayPresenter, TraySink,
};
use ocy_core::engine::{BreakEngine, BreakInfo, EngineSnapshot};
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;
use ocy_core::time::FakeClock;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ocy-settings-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[derive(Debug)]
struct FakeAutostart {
    supported: bool,
    enabled: Mutex<bool>,
    fail_on_set: Mutex<bool>,
    set_calls: Mutex<Vec<bool>>,
}

impl FakeAutostart {
    fn new(supported: bool, enabled: bool) -> Self {
        Self {
            supported,
            enabled: Mutex::new(enabled),
            fail_on_set: Mutex::new(false),
            set_calls: Mutex::new(Vec::new()),
        }
    }

    fn set_fail(&self, fail: bool) {
        *self.fail_on_set.lock().unwrap() = fail;
    }

    fn calls(&self) -> Vec<bool> {
        self.set_calls.lock().unwrap().clone()
    }
}

impl AutostartService for FakeAutostart {
    fn is_supported(&self) -> bool {
        self.supported
    }

    fn is_enabled(&self) -> bool {
        self.supported && *self.enabled.lock().unwrap()
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        self.set_calls.lock().unwrap().push(enabled);
        if *self.fail_on_set.lock().unwrap() {
            return Err(PlatformError("simulated registry error".to_owned()));
        }
        *self.enabled.lock().unwrap() = enabled;
        Ok(())
    }
}

#[derive(Debug)]
struct FakePlatform {
    autostart: FakeAutostart,
}

impl FakePlatform {
    fn new(autostart: FakeAutostart) -> Self {
        Self { autostart }
    }
}

impl PlatformServices for FakePlatform {
    fn autostart(&self) -> &dyn AutostartService {
        &self.autostart
    }
}

#[derive(Debug, Clone)]
struct FakeInstanceInner {
    values: RefCell<SettingsValues>,
    handlers: RefCell<SettingsUiHandlers>,
    preset: Cell<i32>,
    language_index: Cell<i32>,
    live_view: RefCell<SettingsLiveView>,
    save_error: RefCell<String>,
    autostart_error: RefCell<String>,
    show_start_with_system: Cell<bool>,
    dark: Cell<bool>,
    light_icon_set: Cell<bool>,
}

#[derive(Clone)]
struct FakeInstance(Rc<FakeInstanceInner>);

#[derive(Default)]
struct FakeBackendState {
    create_count: usize,
    activate_count: usize,
    set_texts_count: usize,
    destroy_on_close_count: usize,
    hide_and_destroy_count: usize,
    last_texts: Option<SettingsTexts>,
    current: Option<FakeInstance>,
    scheduled_timers: VecDeque<(Duration, Box<dyn FnOnce()>)>,
    refresh_timer: Option<(Duration, Box<dyn Fn()>)>,
    retrigger_changed_on_revert: bool,
}

#[derive(Clone, Default)]
struct FakeSettingsBackend {
    state: Rc<RefCell<FakeBackendState>>,
}

impl FakeSettingsBackend {
    fn current_instance(&self) -> Option<FakeInstance> {
        self.state.borrow().current.clone()
    }

    fn set_values(&self, f: impl FnOnce(&mut SettingsValues)) {
        if let Some(inst) = self.current_instance() {
            f(&mut inst.0.values.borrow_mut());
        }
    }

    fn trigger_changed(&self) {
        let cb = self
            .current_instance()
            .map(|inst| Rc::clone(&inst.0.handlers.borrow().on_changed));
        if let Some(cb) = cb {
            cb();
        }
    }

    fn trigger_preset_selected(&self, index: i32) {
        let cb = self
            .current_instance()
            .map(|inst| Rc::clone(&inst.0.handlers.borrow().on_preset_selected));
        if let Some(cb) = cb {
            cb(index);
        }
    }

    fn trigger_language_selected(&self, index: i32) {
        let cb = self
            .current_instance()
            .map(|inst| Rc::clone(&inst.0.handlers.borrow().on_language_selected));
        if let Some(cb) = cb {
            cb(index);
        }
    }

    fn trigger_refresh_tick(&self) {
        let borrow = self.state.borrow();
        if let Some((_, ref cb)) = borrow.refresh_timer {
            cb();
        }
    }

    fn trigger_try_short(&self) {
        let cb = self
            .current_instance()
            .map(|inst| Rc::clone(&inst.0.handlers.borrow().on_try_short_break));
        if let Some(cb) = cb {
            cb();
        }
    }

    fn trigger_try_long(&self) {
        let cb = self
            .current_instance()
            .map(|inst| Rc::clone(&inst.0.handlers.borrow().on_try_long_break));
        if let Some(cb) = cb {
            cb();
        }
    }

    fn trigger_close(&self) {
        let cb = self
            .current_instance()
            .map(|inst| Rc::clone(&inst.0.handlers.borrow().on_close_requested));
        if let Some(cb) = cb {
            cb();
        }
    }

    fn fire_oldest_timer(&self) -> bool {
        let timer = self.state.borrow_mut().scheduled_timers.pop_front();
        if let Some((_delay, cb)) = timer {
            cb();
            true
        } else {
            false
        }
    }

    fn fire_newest_timer(&self) -> bool {
        let timer = self.state.borrow_mut().scheduled_timers.pop_back();
        if let Some((_delay, cb)) = timer {
            cb();
            true
        } else {
            false
        }
    }
}

impl SettingsBackend for FakeSettingsBackend {
    type Instance = FakeInstance;

    fn create(
        &self,
        params: SettingsCreateParams<'_>,
        handlers: SettingsUiHandlers,
    ) -> Result<Self::Instance, String> {
        let inst = FakeInstance(Rc::new(FakeInstanceInner {
            values: RefCell::new(params.values.clone()),
            handlers: RefCell::new(handlers),
            preset: Cell::new(params.preset),
            language_index: Cell::new(params.language_index),
            live_view: RefCell::new(params.live_view.clone()),
            save_error: RefCell::new(String::new()),
            autostart_error: RefCell::new(String::new()),
            show_start_with_system: Cell::new(params.show_start_with_system),
            dark: Cell::new(params.dark),
            light_icon_set: Cell::new(false),
        }));
        let mut st = self.state.borrow_mut();
        st.create_count += 1;
        st.last_texts = Some(params.texts.clone());
        st.current = Some(inst.clone());
        Ok(inst)
    }

    fn activate(&self, _instance: &Self::Instance) {
        self.state.borrow_mut().activate_count += 1;
    }

    fn set_texts(
        &self,
        instance: &Self::Instance,
        texts: &SettingsTexts,
        language_index: i32,
        _strings: &Arc<Strings>,
    ) {
        instance.0.language_index.set(language_index);
        let mut st = self.state.borrow_mut();
        st.set_texts_count += 1;
        st.last_texts = Some(texts.clone());
    }

    fn set_dark(&self, instance: &Self::Instance, dark: bool) {
        instance.0.dark.set(dark);
    }

    fn set_light_icon_set(&self, instance: &Self::Instance, use_light_set: bool) {
        instance.0.light_icon_set.set(use_light_set);
    }

    fn read_values(&self, instance: &Self::Instance) -> SettingsValues {
        instance.0.values.borrow().clone()
    }

    fn set_preset(&self, instance: &Self::Instance, preset: i32) {
        instance.0.preset.set(preset);
    }

    fn apply_preset_values(&self, instance: &Self::Instance, preset: i32, values: &SettingsValues) {
        instance.0.preset.set(preset);
        let mut cur = instance.0.values.borrow_mut();
        cur.short_interval = values.short_interval;
        cur.short_duration = values.short_duration;
        cur.long_interval = values.long_interval;
        cur.long_duration = values.long_duration;
    }

    fn set_language_index(&self, instance: &Self::Instance, index: i32) {
        instance.0.language_index.set(index);
    }

    fn set_live_view(&self, instance: &Self::Instance, live_view: &SettingsLiveView) {
        *instance.0.live_view.borrow_mut() = live_view.clone();
    }

    fn set_start_with_system(&self, instance: &Self::Instance, enabled: bool) {
        instance.0.values.borrow_mut().start_with_system = enabled;
        if self.state.borrow().retrigger_changed_on_revert {
            let cb = Rc::clone(&instance.0.handlers.borrow().on_changed);
            cb();
        }
    }

    fn set_start_with_system_error(&self, instance: &Self::Instance, error: &str) {
        *instance.0.autostart_error.borrow_mut() = error.to_owned();
    }

    fn set_save_error(&self, instance: &Self::Instance, error: &str) {
        *instance.0.save_error.borrow_mut() = error.to_owned();
    }

    fn schedule_apply_timer(&self, delay: Duration, on_fire: Box<dyn FnOnce()>) {
        self.state
            .borrow_mut()
            .scheduled_timers
            .push_back((delay, on_fire));
    }

    fn start_refresh_timer(&self, interval: Duration, on_tick: Box<dyn Fn()>) {
        self.state.borrow_mut().refresh_timer = Some((interval, on_tick));
    }

    fn stop_refresh_timer(&self) {
        self.state.borrow_mut().refresh_timer = None;
    }

    fn destroy_on_close(&self, _instance: Self::Instance) {
        let mut st = self.state.borrow_mut();
        st.destroy_on_close_count += 1;
        st.current = None;
    }

    fn hide_and_destroy(&self, _instance: Self::Instance) {
        let mut st = self.state.borrow_mut();
        st.hide_and_destroy_count += 1;
        st.current = None;
    }
}

#[test]
fn settings_values_round_trip_and_preserve_non_ui_fields() {
    use ocy_core::settings::{LongBreakSettings, ShortBreakSettings};

    let original = OcySettings {
        schema_version: 1,
        sounds: true,
        status_notifications: false,
        language: "mk".to_owned(),
        start_with_system: false,
        short_breaks: ShortBreakSettings {
            enabled: false,
            interval_minutes: 15,
            duration_seconds: 25,
            fullscreen: true,
        },
        long_breaks: LongBreakSettings {
            enabled: false,
            interval_minutes: 90,
            duration_minutes: 8,
            warning_seconds: 30,
            postpone_minutes: 5,
            max_postpones: 6,
        },
        strict_mode: true,
        inactivity_tracking: false,
        away_threshold_seconds: 240,
        pause_during_fullscreen: false,
        has_window_nearby: false,
        natural_moments: true,
        games_mode: ocy_core::settings::GamesMode::Hold,
        games_silent: vec!["Demo.exe".to_owned()],
        water_nudges: false,
    };

    // When autostart is supported, start_with_system is loaded from platform autostart state.
    let loaded = SettingsValues::from_settings(&original, true, true);
    assert!(loaded.start_with_system);
    assert!(!loaded.short_enabled);
    assert_eq!(loaded.short_interval, 15);
    assert_eq!(loaded.short_duration, 25);
    assert!(loaded.short_fullscreen);
    assert!(!loaded.long_enabled);
    assert_eq!(loaded.long_interval, 90);
    assert_eq!(loaded.long_duration, 8);
    assert_eq!(loaded.long_warning, 30);
    assert_eq!(loaded.long_postpone, 5);
    assert_eq!(loaded.long_max_postpones, 6);
    assert!(loaded.strict_mode);
    assert!(!loaded.inactivity_tracking);
    assert_eq!(loaded.away_threshold, 240);
    assert!(!loaded.pause_fullscreen);
    assert!(!loaded.window_nearby);
    assert!(!loaded.water_nudges);

    let mut edited = loaded;
    edited.short_enabled = true;
    edited.short_interval = 20;
    edited.short_duration = 40;
    edited.short_fullscreen = false;
    edited.long_enabled = true;
    edited.long_interval = 75;
    edited.long_duration = 10;
    edited.long_warning = 45;
    edited.long_postpone = 7;
    edited.long_max_postpones = 2;
    edited.strict_mode = false;
    edited.inactivity_tracking = true;
    edited.away_threshold = 300;
    edited.pause_fullscreen = true;
    edited.window_nearby = true;
    edited.water_nudges = true;
    edited.start_with_system = true;

    let applied = edited.apply_to(&original, true);
    assert_eq!(applied.schema_version, 1);
    assert!(applied.sounds);
    assert!(!applied.status_notifications);
    assert_eq!(applied.language, "mk");
    assert!(applied.start_with_system);
    assert!(applied.short_breaks.enabled);
    assert_eq!(applied.short_breaks.interval_minutes, 20);
    assert_eq!(applied.short_breaks.duration_seconds, 40);
    assert!(!applied.short_breaks.fullscreen);
    assert!(applied.long_breaks.enabled);
    assert_eq!(applied.long_breaks.interval_minutes, 75);
    assert_eq!(applied.long_breaks.duration_minutes, 10);
    assert_eq!(applied.long_breaks.warning_seconds, 45);
    assert_eq!(applied.long_breaks.postpone_minutes, 7);
    assert_eq!(applied.long_breaks.max_postpones, 2);
    assert!(!applied.strict_mode);
    assert!(applied.inactivity_tracking);
    assert_eq!(applied.away_threshold_seconds, 300);
    assert!(applied.pause_during_fullscreen);
    assert!(applied.has_window_nearby);
    assert_eq!(applied.games_mode, ocy_core::settings::GamesMode::Hold);
    assert_eq!(applied.games_silent, ["Demo.exe"]);
    assert!(applied.water_nudges);

    // When autostart is unsupported, original.start_with_system is preserved on save.
    let applied_unsupported = edited.apply_to(&original, false);
    assert!(!applied_unsupported.start_with_system);
}

#[test]
fn settings_texts_resolve_every_en_json_key_without_missing_keys() {
    let strings = Strings::from_embedded().unwrap();
    let texts = SettingsTexts::from_strings(&strings, app_version());

    assert_eq!(texts.window_title, "Ocy \u{2014} Settings");
    assert_eq!(texts.today_title, "Today");
    assert_eq!(texts.today_taken_label, "taken");
    assert_eq!(texts.today_skipped_label, "skipped");
    assert_eq!(texts.timeline_title, "The next 2 hours");
    assert_eq!(texts.timeline_short_label, "Short breaks");
    assert_eq!(texts.timeline_long_label, "Long breaks");
    assert_eq!(texts.now_label, "Now");
    assert_eq!(texts.preset_gentle_label, "Gentle");
    assert_eq!(texts.preset_balanced_label, "Balanced");
    assert_eq!(texts.preset_focused_label, "Focused");
    assert_eq!(texts.preset_custom_label, "Custom");
    assert_eq!(
        texts.preset_gentle_summary,
        "15 s every 40 min\n5 min every 90 min"
    );
    assert_eq!(
        texts.preset_balanced_summary,
        "15 s every 25 min\n5 min every 60 min"
    );
    assert_eq!(
        texts.preset_focused_summary,
        "20 s every 50 min\n10 min every 90 min"
    );
    assert_eq!(texts.customize_title, "Customize");
    assert_eq!(texts.general_title, "General");
    assert_eq!(texts.tab_breaks_label, "Breaks");
    assert_eq!(texts.tab_strictness_label, "Strictness");
    assert_eq!(texts.tab_quiet_label, "Quiet times");
    assert_eq!(texts.try_short_label, "Try a short break");
    assert_eq!(texts.try_long_label, "Try a long break");
    assert!(texts.about_name_version.starts_with("Ocy "));
    assert_eq!(texts.language_model[0], "Automatic (Windows language)");

    let all_fields = [
        &texts.window_title,
        &texts.today_title,
        &texts.today_taken_label,
        &texts.today_skipped_label,
        &texts.timeline_title,
        &texts.timeline_short_label,
        &texts.timeline_long_label,
        &texts.now_label,
        &texts.preset_gentle_label,
        &texts.preset_balanced_label,
        &texts.preset_focused_label,
        &texts.preset_custom_label,
        &texts.preset_gentle_summary,
        &texts.preset_balanced_summary,
        &texts.preset_focused_summary,
        &texts.customize_title,
        &texts.general_title,
        &texts.tab_breaks_label,
        &texts.tab_strictness_label,
        &texts.tab_quiet_label,
        &texts.start_with_system_label,
        &texts.start_with_system_help,
        &texts.short_enabled_label,
        &texts.short_interval_label,
        &texts.short_duration_label,
        &texts.short_fullscreen_label,
        &texts.short_fullscreen_help,
        &texts.try_short_label,
        &texts.long_enabled_label,
        &texts.long_interval_label,
        &texts.long_duration_label,
        &texts.long_warning_label,
        &texts.long_postpone_label,
        &texts.long_max_postpones_label,
        &texts.try_long_label,
        &texts.water_nudges_label,
        &texts.water_nudges_help,
        &texts.strict_label,
        &texts.strict_help,
        &texts.inactivity_label,
        &texts.inactivity_help,
        &texts.away_label,
        &texts.pause_fullscreen_label,
        &texts.pause_fullscreen_help,
        &texts.window_nearby_label,
        &texts.window_nearby_help,
        &texts.language_label,
        &texts.sounds_label,
        &texts.sounds_help,
        &texts.notifications_label,
        &texts.notifications_help,
        &texts.about_name_version,
        &texts.about_tagline,
    ];
    for value in all_fields {
        assert!(!value.is_empty());
        assert!(!value.contains('{'));
        assert!(!value.contains('}'));
        assert!(!value.starts_with("settings."));
    }
}

#[test]
fn debounce_state_machine_schedule_restart_flush_and_loading_guard() {
    let autostart = FakeAutostart::new(true, false);
    let mut state = SettingsDebounceState::new();

    // Events during load must be ignored.
    state.begin_load();
    assert_eq!(
        state.on_changed(true, true, &autostart),
        ChangeAction::Ignored
    );
    assert!(!state.has_pending_changes());
    assert!(autostart.calls().is_empty());
    state.end_load(false);

    // First edit schedules a timer.
    let g1 = match state.on_changed(true, false, &autostart) {
        ChangeAction::ScheduleTimer {
            generation,
            clear_autostart_error,
        } => {
            assert!(!clear_autostart_error);
            generation
        }
        other => panic!("unexpected action: {other:?}"),
    };
    assert!(state.has_pending_changes());

    // Second edit restarts the timer with a fresh generation; stale g1 is ignored.
    let g2 = match state.on_changed(true, false, &autostart) {
        ChangeAction::ScheduleTimer { generation, .. } => generation,
        other => panic!("unexpected action: {other:?}"),
    };
    assert_ne!(g1, g2);
    assert!(!state.on_timer_fired(g1));
    assert!(state.has_pending_changes());
    assert!(state.on_timer_fired(g2));
    assert!(!state.has_pending_changes());

    // Flushing when nothing is pending is a no-op.
    assert!(!state.flush_if_pending());

    // Edit then flush (on close or Try) applies immediately and invalidates the old timer.
    let g3 = match state.on_changed(true, false, &autostart) {
        ChangeAction::ScheduleTimer { generation, .. } => generation,
        other => panic!("unexpected action: {other:?}"),
    };
    assert!(state.flush_if_pending());
    assert!(!state.has_pending_changes());
    assert!(!state.on_timer_fired(g3));
}

#[test]
fn autostart_error_reverts_toggle_without_save_loop() {
    let dir = temp_dir("autostart-err");
    let platform = Arc::new(FakePlatform::new(FakeAutostart::new(true, false)));
    platform.autostart.set_fail(true);

    let saved: Rc<RefCell<Vec<OcySettings>>> = Rc::new(RefCell::new(Vec::new()));
    let saved_cb = Rc::clone(&saved);
    let backend = FakeSettingsBackend::default();
    backend.state.borrow_mut().retrigger_changed_on_revert = true;

    let mut win = ManagedSettingsWindow::new(
        backend.clone(),
        platform.clone(),
        Arc::new(Strings::from_embedded().unwrap()),
        damt_app::ErrorLog::new(&dir, "ocy"),
        SettingsCallbacks {
            on_save: Rc::new(move |s| {
                saved_cb.borrow_mut().push(s);
                true
            }),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        OcySettings::default(),
    );

    win.show();

    // User flips autostart to true -> set_enabled fails -> error shown, toggle reverted, no save scheduled.
    backend.set_values(|v| v.start_with_system = true);
    backend.trigger_changed();

    assert_eq!(platform.autostart.calls(), vec![true]);
    let inst = backend.current_instance().unwrap();
    assert!(!inst.0.values.borrow().start_with_system);
    assert_eq!(
        *inst.0.autostart_error.borrow(),
        "Could not change the start-up setting."
    );
    assert!(!win.has_pending_changes());
    assert!(backend.state.borrow().scheduled_timers.is_empty());
    assert!(saved.borrow().is_empty());

    // Now autostart succeeds -> error clears, change is scheduled and saved.
    platform.autostart.set_fail(false);
    backend.set_values(|v| v.start_with_system = true);
    backend.trigger_changed();
    assert_eq!(platform.autostart.calls(), vec![true, true]);
    assert_eq!(*inst.0.autostart_error.borrow(), "");
    assert!(win.has_pending_changes());
    assert!(backend.fire_newest_timer());
    assert_eq!(saved.borrow().len(), 1);
    assert!(saved.borrow()[0].start_with_system);

    drop(win);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn managed_settings_window_single_instance_show_again_and_close_flush() {
    let dir = temp_dir("single-instance");
    let platform = Arc::new(FakePlatform::new(FakeAutostart::new(true, false)));
    let saved: Rc<RefCell<Vec<OcySettings>>> = Rc::new(RefCell::new(Vec::new()));
    let saved_cb = Rc::clone(&saved);
    let backend = FakeSettingsBackend::default();

    let initial = OcySettings {
        sounds: true, // non-UI field must survive edits
        ..Default::default()
    };

    let mut win = ManagedSettingsWindow::new(
        backend.clone(),
        platform,
        Arc::new(Strings::from_embedded().unwrap()),
        damt_app::ErrorLog::new(&dir, "ocy"),
        SettingsCallbacks {
            on_save: Rc::new(move |s| {
                saved_cb.borrow_mut().push(s);
                true
            }),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        initial,
    );

    // 1. First show() creates one instance.
    win.show();
    assert!(win.is_open());
    assert_eq!(backend.state.borrow().create_count, 1);
    assert_eq!(backend.state.borrow().activate_count, 0);

    // 2. Second show() activates the existing window without creating a second one.
    win.show();
    assert_eq!(backend.state.borrow().create_count, 1);
    assert_eq!(backend.state.borrow().activate_count, 1);

    // 3. Edit twice within 400 ms -> restarts timer, stale first timer does not save.
    backend.set_values(|v| v.short_interval = 25);
    backend.trigger_changed();
    assert_eq!(
        backend.state.borrow().scheduled_timers.front().unwrap().0,
        SETTINGS_APPLY_DELAY
    );

    backend.set_values(|v| v.long_duration = 12);
    backend.trigger_changed();
    assert_eq!(backend.state.borrow().scheduled_timers.len(), 2);

    assert!(backend.fire_oldest_timer());
    assert!(saved.borrow().is_empty());
    assert!(win.has_pending_changes());

    assert!(backend.fire_oldest_timer());
    assert_eq!(saved.borrow().len(), 1);
    assert_eq!(saved.borrow()[0].short_breaks.interval_minutes, 25);
    assert_eq!(saved.borrow()[0].long_breaks.duration_minutes, 12);
    assert!(saved.borrow()[0].sounds);

    // 4. Edit and close immediately -> flushes pending change and destroys instance.
    backend.set_values(|v| v.strict_mode = true);
    backend.trigger_changed();
    assert!(win.has_pending_changes());
    backend.trigger_close();

    assert!(!win.is_open());
    assert!(!win.has_pending_changes());
    assert_eq!(backend.state.borrow().destroy_on_close_count, 1);
    assert_eq!(saved.borrow().len(), 2);
    assert!(saved.borrow()[1].strict_mode);

    // 5. Reopening builds a fresh window with the updated settings.
    win.show();
    assert!(win.is_open());
    assert_eq!(backend.state.borrow().create_count, 2);
    assert!(
        backend
            .current_instance()
            .unwrap()
            .0
            .values
            .borrow()
            .strict_mode
    );

    drop(win);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn managed_settings_window_try_buttons_flush_before_starting_break() {
    let dir = temp_dir("try-flush");
    let platform = Arc::new(FakePlatform::new(FakeAutostart::new(false, false)));
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events_save = Rc::clone(&events);
    let events_short = Rc::clone(&events);
    let events_long = Rc::clone(&events);
    let backend = FakeSettingsBackend::default();

    let mut win = ManagedSettingsWindow::new(
        backend.clone(),
        platform,
        Arc::new(Strings::from_embedded().unwrap()),
        damt_app::ErrorLog::new(&dir, "ocy"),
        SettingsCallbacks {
            on_save: Rc::new(move |_| {
                events_save.borrow_mut().push("save");
                true
            }),
            on_try_short_break: Rc::new(move || {
                events_short.borrow_mut().push("try_short");
            }),
            on_try_long_break: Rc::new(move || {
                events_long.borrow_mut().push("try_long");
            }),
        },
        OcySettings::default(),
    );

    win.show();
    // Autostart unsupported -> General card hidden.
    assert!(
        !backend
            .current_instance()
            .unwrap()
            .0
            .show_start_with_system
            .get()
    );

    // Pending change + Try short break -> save runs before try_short.
    backend.set_values(|v| v.short_duration = 35);
    backend.trigger_changed();
    backend.trigger_try_short();
    assert_eq!(*events.borrow(), vec!["save", "try_short"]);
    assert!(!win.has_pending_changes());

    // Pending change + Try long break -> save runs before try_long.
    events.borrow_mut().clear();
    backend.set_values(|v| v.long_duration = 9);
    backend.trigger_changed();
    backend.trigger_try_long();
    assert_eq!(*events.borrow(), vec!["save", "try_long"]);

    // Try with no pending change does not perform a redundant save.
    events.borrow_mut().clear();
    backend.trigger_try_short();
    assert_eq!(*events.borrow(), vec!["try_short"]);

    drop(win);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn managed_settings_window_save_failure_sets_and_clears_save_error() {
    let dir = temp_dir("save-err");
    let platform = Arc::new(FakePlatform::new(FakeAutostart::new(true, false)));
    let should_succeed = Rc::new(Cell::new(false));
    let should_succeed_cb = Rc::clone(&should_succeed);
    let backend = FakeSettingsBackend::default();

    let mut win = ManagedSettingsWindow::new(
        backend.clone(),
        platform,
        Arc::new(Strings::from_embedded().unwrap()),
        damt_app::ErrorLog::new(&dir, "ocy"),
        SettingsCallbacks {
            on_save: Rc::new(move |_| should_succeed_cb.get()),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        OcySettings::default(),
    );

    win.show();
    backend.set_values(|v| v.short_interval = 18);
    backend.trigger_changed();
    backend.fire_newest_timer();

    let inst = backend.current_instance().unwrap();
    assert_eq!(
        *inst.0.save_error.borrow(),
        "Could not save your settings. Please try again."
    );

    should_succeed.set(true);
    backend.set_values(|v| v.short_interval = 19);
    backend.trigger_changed();
    backend.fire_newest_timer();
    assert_eq!(*inst.0.save_error.borrow(), "");

    drop(win);
    let _ = fs::remove_dir_all(&dir);
}

#[derive(Debug, Default)]
struct DummyBreakWindows;
impl BreakWindows for DummyBreakWindows {
    fn show_short_break(&mut self, _info: &BreakInfo) {}
    fn show_warning(&mut self, _countdown: Duration) {}
    fn show_long_break(&mut self, _info: &BreakInfo) {}
    fn update_countdown(&mut self, _snapshot: &EngineSnapshot) {}
    fn close_all(&mut self) {}
}

#[derive(Debug, Default)]
struct DummySettingsWindow {
    synced_settings: Vec<OcySettings>,
    show_count: usize,
}
impl SettingsWindow for DummySettingsWindow {
    fn sync_settings(&mut self, settings: &OcySettings) {
        self.synced_settings.push(settings.clone());
    }
    fn show(&mut self) {
        self.show_count += 1;
    }
}

#[derive(Debug, Default)]
struct DummyTraySink;
impl TraySink for DummyTraySink {
    fn set_tooltip(&mut self, _tooltip: &str) {}
    fn set_icon(&mut self, _state: TrayIconState, _use_light_set: bool) {}
    fn set_menu(&mut self, _presentation: &TrayPresentation) {}
}

#[derive(Debug)]
struct StaticTaskbarTheme;
impl TaskbarThemeProvider for StaticTaskbarTheme {
    fn is_supported(&self) -> bool {
        true
    }
    fn is_light_taskbar(&self) -> Option<bool> {
        Some(true)
    }
}

#[test]
fn app_host_save_settings_updates_engine_persists_and_handles_failure() {
    let dir = temp_dir("host-save");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 19, 10, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let engine_store = AppDataStore::new(&dir, Arc::clone(&clock_dyn));
    let settings_store = AppDataStore::new(&dir, Arc::clone(&clock_dyn));
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let engine = BreakEngine::new(
        OcySettings::default(),
        platform,
        Arc::clone(&clock_dyn),
        Some(engine_store),
        None,
        None,
        None,
    );
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_embedded().unwrap());
    let presenter = TrayPresenter::new(strings, Arc::clone(&clock_dyn));
    let theme: Rc<dyn TaskbarThemeProvider> = Rc::new(StaticTaskbarTheme);
    let tray = TrayController::new(
        presenter,
        DummyTraySink,
        theme,
        clock_dyn,
        Some(AppTheme::Light),
        &snapshot,
    );
    let coordinator = BreakUiCoordinator::new(DummyBreakWindows, DummySettingsWindow::default());
    let error_log = damt_app::ErrorLog::new(&dir, "ocy");

    let mut host = AppHost::with_store(engine, coordinator, tray, error_log, Some(settings_store));

    let mut updated = OcySettings::default();
    updated.short_breaks.interval_minutes = 22;
    updated.strict_mode = true;
    assert!(host.save_settings(updated));
    assert_eq!(host.engine().settings().short_breaks.interval_minutes, 22);
    assert!(host.engine().settings().strict_mode);

    let on_disk = AppDataStore::with_system_clock(&dir).load_settings();
    assert_eq!(on_disk.short_breaks.interval_minutes, 22);
    assert!(on_disk.strict_mode);

    // show_settings syncs latest engine settings before calling show().
    host.show_settings();
    assert_eq!(host.coordinator().settings_window().show_count, 1);
    assert_eq!(
        host.coordinator()
            .settings_window()
            .synced_settings
            .last()
            .unwrap()
            .short_breaks
            .interval_minutes,
        22
    );

    // Failure case: point settings_store at a regular file path so creating settings.json fails.
    let blocker = dir.join("not-a-directory");
    fs::write(&blocker, b"file").unwrap();
    let bad_store = AppDataStore::with_system_clock(&blocker);
    let engine2 =
        BreakEngine::with_defaults(OcySettings::default(), Arc::new(Unsupported), clock.clone());
    let snap2 = engine2.snapshot();
    let tray2 = TrayController::new(
        TrayPresenter::new(Arc::new(Strings::from_embedded().unwrap()), clock.clone()),
        DummyTraySink,
        Rc::new(StaticTaskbarTheme),
        clock,
        Some(AppTheme::Light),
        &snap2,
    );
    let mut failing_host = AppHost::with_store(
        engine2,
        BreakUiCoordinator::new(DummyBreakWindows, DummySettingsWindow::default()),
        tray2,
        damt_app::ErrorLog::new(&dir, "ocy"),
        Some(bad_store),
    );
    assert!(!failing_host.save_settings(OcySettings::default()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn demo_platform_services_uses_safe_noop_autostart() {
    let inner = FakePlatform::new(FakeAutostart::new(true, true));
    let demo = DemoPlatformServices::new(inner);

    assert!(demo.autostart().is_supported());
    assert!(!demo.autostart().is_enabled());
    assert!(demo.autostart().set_enabled(true).is_ok());
    assert!(demo.autostart().is_enabled());
    assert!(demo.inner().autostart.calls().is_empty());
}

#[test]
fn managed_settings_window_forwards_light_icon_set_to_open_instance() {
    let dir = temp_dir("icon-set");
    let platform = Arc::new(FakePlatform::new(FakeAutostart::new(true, false)));
    let backend = FakeSettingsBackend::default();
    let mut win = ManagedSettingsWindow::new(
        backend.clone(),
        platform,
        Arc::new(Strings::from_embedded().unwrap()),
        damt_app::ErrorLog::new(&dir, "ocy"),
        SettingsCallbacks {
            on_save: Rc::new(|_| true),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        OcySettings::default(),
    );

    win.show();
    let inst = backend.current_instance().unwrap();
    assert!(!inst.0.light_icon_set.get());

    win.set_light_icon_set(true);
    assert!(inst.0.light_icon_set.get());

    win.set_light_icon_set(false);
    assert!(!inst.0.light_icon_set.get());

    drop(win);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fresh_install_defaults_language_to_auto_while_existing_files_keep_their_value() {
    let dir = temp_dir("fresh-vs-existing-lang");
    let store = AppDataStore::with_system_clock(&dir);

    assert_eq!(default_app_settings().language, "auto");
    // Missing settings.json -> fresh install gets "auto"
    assert_eq!(load_app_settings(&store).language, "auto");

    // Existing file with "en" keeps "en"
    let existing = OcySettings::default();
    store.save_settings(&existing).unwrap();
    assert_eq!(load_app_settings(&store).language, "en");

    // Existing file with "mk" keeps "mk"
    let existing_mk = OcySettings {
        language: "mk".to_owned(),
        ..OcySettings::default()
    };
    store.save_settings(&existing_mk).unwrap();
    assert_eq!(load_app_settings(&store).language, "mk");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn live_language_switch_updates_tray_and_open_settings_window_immediately() {
    let dir = temp_dir("live-lang-switch");
    let clock = Arc::new(FakeClock::from_utc(2026, 11, 20, 8, 0, 0).unwrap());
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let store = AppDataStore::new(&dir, Arc::clone(&clock_dyn));
    let platform = Arc::new(FakePlatform::new(FakeAutostart::new(true, false)));
    let error_log = damt_app::ErrorLog::new(&dir, "ocy");

    let initial = OcySettings {
        language: "en".to_owned(),
        ..OcySettings::default()
    };
    let engine = BreakEngine::new(
        initial.clone(),
        platform.clone(),
        Arc::clone(&clock_dyn),
        None,
        None,
        None,
        None,
    );
    let snapshot = engine.snapshot();
    let strings = Arc::new(Strings::from_language(&initial.language));
    let presenter = TrayPresenter::new(strings.clone(), Arc::clone(&clock_dyn));

    #[derive(Debug, Default)]
    struct CapturingTraySink {
        last_tooltip: String,
        last_presentation: Option<TrayPresentation>,
    }
    impl TraySink for CapturingTraySink {
        fn set_tooltip(&mut self, tooltip: &str) {
            self.last_tooltip = tooltip.to_owned();
        }
        fn set_icon(&mut self, _state: TrayIconState, _use_light_set: bool) {}
        fn set_menu(&mut self, presentation: &TrayPresentation) {
            self.last_presentation = Some(presentation.clone());
        }
    }

    let tray = TrayController::new(
        presenter,
        CapturingTraySink::default(),
        Rc::new(StaticTaskbarTheme),
        Arc::clone(&clock_dyn),
        Some(AppTheme::Light),
        &snapshot,
    );

    let backend = FakeSettingsBackend::default();
    let settings_win = ManagedSettingsWindow::new(
        backend.clone(),
        platform,
        strings,
        error_log.clone(),
        SettingsCallbacks {
            on_save: Rc::new(|_| true),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        initial,
    );
    let coordinator = BreakUiCoordinator::new(DummyBreakWindows, settings_win);
    let mut host = AppHost::with_store(engine, coordinator, tray, error_log, Some(store));

    host.show_settings();
    assert_eq!(
        backend
            .state
            .borrow()
            .last_texts
            .as_ref()
            .unwrap()
            .font_family,
        "Segoe UI"
    );
    assert_eq!(
        backend
            .state
            .borrow()
            .last_texts
            .as_ref()
            .unwrap()
            .window_title,
        "Ocy \u{2014} Settings"
    );
    assert_eq!(
        host.tray()
            .sink()
            .last_presentation
            .as_ref()
            .unwrap()
            .settings
            .text,
        "Settings"
    );
    assert_eq!(host.tray().sink().last_tooltip, "Next break in 25 min");

    // Live switch to German -> both Settings texts and tray menu/tooltip update immediately
    let mut updated_de = host.engine().settings().clone();
    updated_de.language = "de".to_owned();
    assert!(host.save_settings(updated_de));

    assert_eq!(backend.state.borrow().set_texts_count, 1);
    let de_texts = backend.state.borrow().last_texts.clone().unwrap();
    assert_eq!(de_texts.window_title, "Ocy \u{2014} Einstellungen");
    assert_eq!(de_texts.general_title, "Allgemein");
    assert_eq!(
        host.tray()
            .sink()
            .last_presentation
            .as_ref()
            .unwrap()
            .settings
            .text,
        "Einstellungen"
    );
    assert_eq!(
        host.tray()
            .sink()
            .last_presentation
            .as_ref()
            .unwrap()
            .quit
            .text,
        "Ocy beenden"
    );
    assert_eq!(host.tray().sink().last_tooltip, "Nächste Pause in 25 Min.");

    // Live switch to Macedonian -> updates again immediately
    let mut updated_mk = host.engine().settings().clone();
    updated_mk.language = "mk".to_owned();
    assert!(host.save_settings(updated_mk));

    assert_eq!(backend.state.borrow().set_texts_count, 2);
    let mk_texts = backend.state.borrow().last_texts.clone().unwrap();
    assert_eq!(mk_texts.font_family, "Segoe UI");
    assert_eq!(mk_texts.window_title, "Ocy \u{2014} Поставки");
    assert_eq!(mk_texts.general_title, "Општо");
    assert_eq!(
        host.tray()
            .sink()
            .last_presentation
            .as_ref()
            .unwrap()
            .settings
            .text,
        "Поставки"
    );
    assert_eq!(host.tray().sink().last_tooltip, "Следна пауза за 25 мин");

    // Live switch to Japanese -> updates font_family to Yu Gothic UI as well
    let mut updated_ja = host.engine().settings().clone();
    updated_ja.language = "ja".to_owned();
    assert!(host.save_settings(updated_ja));
    assert_eq!(backend.state.borrow().set_texts_count, 3);
    let ja_texts = backend.state.borrow().last_texts.clone().unwrap();
    assert_eq!(ja_texts.font_family, "Yu Gothic UI");
    assert_eq!(ja_texts.window_title, "Ocy \u{2014} 設定");
    assert_eq!(ja_texts.general_title, "一般");
    assert_eq!(
        host.tray()
            .sink()
            .last_presentation
            .as_ref()
            .unwrap()
            .settings
            .text,
        "設定"
    );
    assert_eq!(host.tray().sink().last_tooltip, "次の休憩まで 25 分");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn managed_settings_v2_refreshes_timeline_today_and_header_on_edits_and_30s_ticks() {
    use ocy_core::engine::EnginePhase;
    use ocy_core::statistics::StatisticsData;

    let dir = temp_dir("v2-refresh");
    let clock =
        Arc::new(FakeClock::from_fixed_offset(2026, 11, 20, 14, 0, 0, 1).expect("valid clock"));
    let clock_dyn: Arc<dyn Clock> = clock.clone();
    let platform = Arc::new(FakePlatform::new(FakeAutostart::new(true, false)));
    let saved: Rc<RefCell<Vec<OcySettings>>> = Rc::new(RefCell::new(Vec::new()));
    let saved_cb = Rc::clone(&saved);
    let backend = FakeSettingsBackend::default();

    let mut win = ManagedSettingsWindow::with_clock(
        backend.clone(),
        platform,
        Arc::clone(&clock_dyn),
        Arc::new(Strings::from_embedded().unwrap()),
        damt_app::ErrorLog::new(&dir, "ocy"),
        SettingsCallbacks {
            on_save: Rc::new(move |s| {
                saved_cb.borrow_mut().push(s);
                true
            }),
            on_try_short_break: Rc::new(|| {}),
            on_try_long_break: Rc::new(|| {}),
        },
        OcySettings::default(),
    );

    let mut stats = StatisticsData::default();
    let day = stats.get_or_create(clock.now().date());
    day.short_completed = 4;
    day.long_completed = 1;
    day.short_skipped = 2;

    let snap = EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: Some(Duration::from_secs(7 * 60)),
        time_until_long_break: Some(Duration::from_secs(47 * 60)),
        time_until_long_break_warning: Some(Duration::from_secs(46 * 60)),
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        natural_hold: None,
        poll_external: false,
        game_in_front: false,
    };
    win.sync_live_status(&snap, &stats);
    win.show();

    let inst = backend.current_instance().unwrap();
    assert_eq!(inst.0.preset.get(), 1); // Balanced
    assert_eq!(
        backend.state.borrow().refresh_timer.as_ref().map(|t| t.0),
        Some(SETTINGS_REFRESH_INTERVAL)
    );
    {
        let lv = inst.0.live_view.borrow();
        assert_eq!(lv.next_break_text, "Next short break in 7 min");
        assert_eq!(lv.today.taken, 5);
        assert_eq!(lv.today.skipped, 2);
        assert_eq!(lv.timeline_labels, ["14:30", "15:00", "15:30", "16:00"]);
        assert!(!lv.timeline_marks.is_empty());
    }

    // Disabling short breaks switches header to next long break at {clock}.
    let long_first_snap = EngineSnapshot {
        time_until_short_break: None,
        time_until_long_break: Some(Duration::from_secs(25 * 60)),
        ..snap.clone()
    };
    win.sync_live_status(&long_first_snap, &stats);
    assert_eq!(
        inst.0.live_view.borrow().next_break_text,
        "Next long break at 14:25"
    );

    // Selecting Focused preset updates sliders, preset index, timeline preview immediately, and debounces save.
    backend.trigger_preset_selected(2);
    assert_eq!(inst.0.preset.get(), 2);
    assert_eq!(inst.0.values.borrow().short_interval, 50);
    assert_eq!(inst.0.values.borrow().long_interval, 90);
    assert!(win.has_pending_changes());
    assert!(backend.fire_newest_timer());
    assert_eq!(saved.borrow().len(), 1);
    assert_eq!(saved.borrow()[0].short_breaks.interval_minutes, 50);

    // Hand-edit short_interval -> preset becomes Custom (3) and timeline preview refreshes immediately.
    let marks_before = inst.0.live_view.borrow().timeline_marks.clone();
    backend.set_values(|v| v.short_interval = 12);
    backend.trigger_changed();
    assert_eq!(inst.0.preset.get(), 3);
    assert_ne!(inst.0.live_view.borrow().timeline_marks, marks_before);

    // 30 s timer tick refreshes labels and today counts while window is open.
    clock.advance(Duration::from_secs(60));
    backend.trigger_refresh_tick();
    assert_eq!(
        inst.0.live_view.borrow().timeline_labels,
        ["14:31", "15:01", "15:31", "16:01"]
    );

    // Language switch through dropdown saves immediately and updates texts + font.
    let ja_idx = ocy_app::language_choice::index_for_i32("ja", None);
    backend.trigger_language_selected(ja_idx);
    assert_eq!(saved.borrow().last().unwrap().language, "ja");
    assert_eq!(
        backend
            .state
            .borrow()
            .last_texts
            .as_ref()
            .unwrap()
            .font_family,
        "Yu Gothic UI"
    );

    // Closing stops the 30 s refresh timer.
    backend.trigger_close();
    assert!(backend.state.borrow().refresh_timer.is_none());

    drop(win);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn v2_formatters_and_next_break_header_cover_plurals_durations_and_phases() {
    use ocy_core::engine::EnginePhase;
    use ocy_core::time::Paused;

    let en = Strings::from_language("en");
    assert_eq!(format_slider_every_minutes(&en, 20), "every 20 min");
    assert_eq!(format_slider_minutes(&en, 5), "5 min");
    assert_eq!(format_slider_seconds(&en, 15), "15 s");
    assert_eq!(format_slider_times(&en, 1), "1 time");
    assert_eq!(format_slider_times(&en, 3), "3 times");
    assert_eq!(format_slider_duration(&en, 30), "30 s");
    assert_eq!(format_slider_duration(&en, 120), "2 min");
    assert_eq!(format_slider_duration(&en, 90), "90 s");

    let clock = FakeClock::from_fixed_offset(2026, 11, 20, 10, 0, 0, 0).expect("clock");
    let working_short = EngineSnapshot {
        phase: EnginePhase::Working,
        time_until_short_break: Some(Duration::from_secs(600)),
        time_until_long_break: Some(Duration::from_secs(3600)),
        time_until_long_break_warning: None,
        current_break: None,
        paused_until_utc: None,
        postpones_left: 3,
        strict_mode: false,
        natural_hold: None,
        poll_external: false,
        game_in_front: false,
    };
    assert_eq!(
        format_next_break_text(&working_short, &en, &clock),
        "Next short break in 10 min"
    );

    // When long break is due within 2 minutes after short break, short break is suppressed -> show nextLong.
    let suppressed_short = EngineSnapshot {
        time_until_short_break: Some(Duration::from_secs(60)),
        time_until_long_break: Some(Duration::from_secs(150)),
        ..working_short.clone()
    };
    assert_eq!(
        format_next_break_text(&suppressed_short, &en, &clock),
        "Next long break at 10:02"
    );

    let paused_indef = EngineSnapshot {
        phase: EnginePhase::Paused,
        paused_until_utc: Some(Paused::UntilResumed),
        ..working_short
    };
    assert_eq!(
        format_next_break_text(&paused_indef, &en, &clock),
        "Paused until you resume"
    );
}

#[test]
fn games_mode_and_silent_list_round_trip_through_settings_values() {
    let original = OcySettings {
        games_silent: vec!["eldenring.exe".to_owned(), "cs2.exe".to_owned()],
        ..OcySettings::default()
    };
    let mut values = SettingsValues::from_settings(&original, true, true);
    assert!(!values.games_hold);
    assert_eq!(values.games_silent, original.games_silent);

    values.games_hold = true;
    values.games_silent.remove(0);
    let applied = values.apply_to(&original, true);
    assert_eq!(applied.games_mode, ocy_core::settings::GamesMode::Hold);
    assert_eq!(applied.games_silent, vec!["cs2.exe".to_owned()]);
}
