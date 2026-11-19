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
    SettingsBackend, SettingsCallbacks, SettingsDebounceState, SettingsTexts, SettingsUiHandlers,
    SettingsValues, app_version,
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
    destroy_on_close_count: usize,
    hide_and_destroy_count: usize,
    last_texts: Option<SettingsTexts>,
    current: Option<FakeInstance>,
    scheduled_timers: VecDeque<(Duration, Box<dyn FnOnce()>)>,
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
        texts: &SettingsTexts,
        values: &SettingsValues,
        show_start_with_system: bool,
        dark: bool,
        handlers: SettingsUiHandlers,
    ) -> Result<Self::Instance, String> {
        let inst = FakeInstance(Rc::new(FakeInstanceInner {
            values: RefCell::new(*values),
            handlers: RefCell::new(handlers),
            save_error: RefCell::new(String::new()),
            autostart_error: RefCell::new(String::new()),
            show_start_with_system: Cell::new(show_start_with_system),
            dark: Cell::new(dark),
            light_icon_set: Cell::new(false),
        }));
        let mut st = self.state.borrow_mut();
        st.create_count += 1;
        st.last_texts = Some(texts.clone());
        st.current = Some(inst.clone());
        Ok(inst)
    }

    fn activate(&self, _instance: &Self::Instance) {
        self.state.borrow_mut().activate_count += 1;
    }

    fn set_dark(&self, instance: &Self::Instance, dark: bool) {
        instance.0.dark.set(dark);
    }

    fn set_light_icon_set(&self, instance: &Self::Instance, use_light_set: bool) {
        instance.0.light_icon_set.set(use_light_set);
    }

    fn read_values(&self, instance: &Self::Instance) -> SettingsValues {
        *instance.0.values.borrow()
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

    // When autostart is unsupported, original.start_with_system is preserved on save.
    let applied_unsupported = edited.apply_to(&original, false);
    assert!(!applied_unsupported.start_with_system);
}

#[test]
fn settings_texts_resolve_every_en_json_key_without_missing_keys() {
    let strings = Strings::from_embedded().unwrap();
    let texts = SettingsTexts::from_strings(&strings, app_version());

    assert_eq!(texts.window_title, "Ocy \u{2014} Settings");
    assert_eq!(texts.general_title, "General");
    assert_eq!(texts.short_breaks_title, "Short breaks");
    assert_eq!(texts.long_breaks_title, "Long breaks");
    assert_eq!(texts.behaviour_title, "Behaviour");
    assert_eq!(texts.about_title, "About");
    assert_eq!(texts.try_short_label, "Try a short break");
    assert_eq!(texts.try_long_label, "Try a long break");
    assert_eq!(texts.unit_minutes, "min");
    assert_eq!(texts.unit_seconds, "s");
    assert!(texts.about_name_version.starts_with("Ocy "));

    let all_fields = [
        &texts.window_title,
        &texts.general_title,
        &texts.short_breaks_title,
        &texts.long_breaks_title,
        &texts.behaviour_title,
        &texts.about_title,
        &texts.start_with_system_label,
        &texts.start_with_system_help,
        &texts.short_enabled_label,
        &texts.short_interval_label,
        &texts.short_interval_help,
        &texts.short_duration_label,
        &texts.short_duration_help,
        &texts.short_fullscreen_label,
        &texts.short_fullscreen_help,
        &texts.try_short_label,
        &texts.long_enabled_label,
        &texts.long_interval_label,
        &texts.long_interval_help,
        &texts.long_duration_label,
        &texts.long_duration_help,
        &texts.long_warning_label,
        &texts.long_warning_help,
        &texts.long_postpone_label,
        &texts.long_postpone_help,
        &texts.long_max_postpones_label,
        &texts.long_max_postpones_help,
        &texts.try_long_label,
        &texts.strict_label,
        &texts.strict_help,
        &texts.inactivity_label,
        &texts.inactivity_help,
        &texts.away_label,
        &texts.away_help,
        &texts.pause_fullscreen_label,
        &texts.pause_fullscreen_help,
        &texts.window_nearby_label,
        &texts.window_nearby_help,
        &texts.unit_minutes,
        &texts.unit_seconds,
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
