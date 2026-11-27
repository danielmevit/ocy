#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use std::cell::{Cell, RefCell};
use std::process::ExitCode;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use damt_app::{Acquire, AppDataPaths, ErrorLog, SingleInstance, install_panic_hook};
use damt_platform::{Clock, PlatformServices, SystemClock};
use ocy_app::break_logic::{
    DemoBreak, build_demo_launch_plan, demo_license_is_honoured, parse_cli_args,
};
use ocy_app::coordinator::{BreakUiCoordinator, OverlayBreakWindows};
use ocy_app::host::AppHost;
use ocy_app::instance::{is_probe_mode, parse_instance_scope, resolve_instance_id};
use ocy_app::notifications::{RecordingNotifier, STARTUP_NOTIFICATION_DELAY};
use ocy_app::scheduler;
use ocy_app::settings_logic::{
    DemoPlatformServices, SettingsCallbacks, default_app_settings, load_app_settings,
};
use ocy_app::slint_breaks::{
    BreakActionCallbacks, BreakLatencySample, LatencyOptions, SlintBreakWindows,
};
use ocy_app::slint_settings::SlintSettingsWindow;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use ocy_app::slint_tray::{OcyTray, SlintTraySink};
use ocy_app::soak::{
    OffsetClock, SoakHooks, SoakTimings, build_soak_settings, format_soak_cycle_line,
    format_soak_summary_line, parse_soak_cycles, process_memory_bytes, start_soak_probe,
};
use ocy_app::strings::{Strings, parse_demo_language};
use ocy_app::tray::{AppTheme, PauseSource, TrayController, TrayPresenter};
use ocy_app::win32_window::{OwnerWindow, apply_window_attributes_for_role};
use ocy_core::engine::BreakEngine;
use ocy_core::persistence::AppDataStore;
use slint::{Timer, TimerMode};

const APP_DATA_DIR_NAME: &str = "Ocy";
const ERROR_LOG_PREFIX: &str = "ocy";

#[cfg(target_os = "linux")]
type LiveTraySink = LinuxTraySink;
#[cfg(target_os = "macos")]
type LiveTraySink = MacTraySink;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
type LiveTraySink = SlintTraySink;

type LiveHost = AppHost<OverlayBreakWindows<SlintBreakWindows>, SlintSettingsWindow, LiveTraySink>;

thread_local! {
    static UI_HOST: RefCell<Option<Weak<RefCell<LiveHost>>>> = const { RefCell::new(None) };
}

fn with_ui_host_ret<R>(f: impl FnOnce(&mut LiveHost) -> R) -> Option<R> {
    let weak = UI_HOST.with(|slot| slot.borrow().clone());
    if let Some(weak) = weak
        && let Some(rc) = weak.upgrade()
        && let Ok(mut host) = rc.try_borrow_mut()
    {
        Some(f(&mut host))
    } else {
        None
    }
}

fn with_ui_host(f: impl FnOnce(&mut LiveHost)) {
    let _ = with_ui_host_ret(f);
}

thread_local! {
    /// The single-shot engine tick timer and the clock used to compute its delay.
    static SCHEDULER: RefCell<Option<(Timer, Arc<dyn Clock>)>> = const { RefCell::new(None) };
    /// Number of scheduler timer fires since startup (for `--report-memory`).
    static WAKEUP_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// (Re-)arms the single-shot engine tick timer with the adaptive delay for the current
/// engine state. Called after every tick and after anything that changes the schedule
/// (tray actions, settings saves, session events, "Try" buttons).
fn arm_next_tick() {
    SCHEDULER.with(|slot| {
        let borrow = slot.borrow();
        let Some((timer, clock)) = borrow.as_ref() else {
            return;
        };
        let clock = Arc::clone(clock);
        let delay = with_ui_host_ret(|host| {
            let snapshot = host.engine().snapshot();
            #[cfg(feature = "store")]
            {
                scheduler::next_wake_delay_for_license(
                    &snapshot,
                    clock.now().timestamp(),
                    host.license_state(),
                )
            }
            #[cfg(not(feature = "store"))]
            {
                scheduler::next_wake_delay(&snapshot, clock.now().timestamp())
            }
        })
        .unwrap_or(Duration::from_secs(1));
        timer.start(TimerMode::SingleShot, delay, || {
            WAKEUP_COUNT.with(|count| count.set(count.get() + 1));
            #[cfg(feature = "store")]
            store_license::maybe_periodic_recheck();
            with_ui_host(LiveHost::on_tick);
            arm_next_tick();
        });
    });
}

/// Wake-ups per minute over a measuring window of `window_secs` (for `--report-memory`).
fn wakeups_per_min(window_secs: u64) -> u64 {
    let count = WAKEUP_COUNT.with(Cell::get);
    count.saturating_mul(60) / window_secs.max(1)
}

fn build_platform_services(
    error_log: &ErrorLog,
    is_isolated: bool,
) -> (Arc<dyn PlatformServices>, Option<PlatformHolder>) {
    #[cfg(windows)]
    {
        use damt_platform::SessionEventSource;
        use damt_platform_win::{WinNotifier, WinPlatform};
        use ocy_app::notifications::{APP_DISPLAY_NAME, APP_NOTIFY_ICON_PNG, APP_USER_MODEL_ID};

        let mut win_platform = WinPlatform::new(APP_DATA_DIR_NAME)
            .with_startup_task_id(ocy_app::instance::STARTUP_TASK_ID);
        if !is_isolated {
            win_platform = win_platform.with_notifier(WinNotifier::new(
                APP_USER_MODEL_ID,
                APP_DISPLAY_NAME,
                APP_DATA_DIR_NAME,
                APP_NOTIFY_ICON_PNG,
            ));
        }
        let dispatch_log = error_log.clone();
        win_platform.session().start(Box::new(move |event| {
            if let Err(err) = slint::invoke_from_event_loop(move || {
                with_ui_host(|host| host.on_session_event(event));
                #[cfg(feature = "store")]
                if !is_isolated && event == damt_platform::SessionEvent::Resumed {
                    store_license::refresh();
                }
                arm_next_tick();
            }) {
                dispatch_log.log_error("session_event_dispatch", &err);
            }
        }));

        let platform: Arc<dyn PlatformServices> = if is_isolated {
            Arc::new(DemoPlatformServices::new(win_platform))
        } else {
            Arc::new(win_platform)
        };
        (platform, None)
    }

    #[cfg(target_os = "linux")]
    {
        use damt_platform::SessionEventSource;
        use damt_platform_linux::{LinuxNotifier, LinuxPlatform};
        use ocy_app::notifications::APP_DISPLAY_NAME;

        let mut linux_platform =
            LinuxPlatform::new(APP_DATA_DIR_NAME).with_desktop_id(ocy_app::instance::APP_ID);
        if !is_isolated {
            linux_platform = linux_platform.with_notifier(LinuxNotifier::new(
                APP_DISPLAY_NAME,
                ocy_app::instance::APP_ID,
            ));
        }
        let dispatch_log = error_log.clone();
        linux_platform.session().start(Box::new(move |event| {
            if let Err(err) = slint::invoke_from_event_loop(move || {
                with_ui_host(|host| host.on_session_event(event));
                arm_next_tick();
            }) {
                dispatch_log.log_error("session_event_dispatch", &err);
            }
        }));

        let platform: Arc<dyn PlatformServices> = if is_isolated {
            Arc::new(DemoPlatformServices::new(linux_platform))
        } else {
            Arc::new(linux_platform)
        };
        (platform, None)
    }

    #[cfg(target_os = "macos")]
    {
        use damt_platform::SessionEventSource;
        use damt_platform_mac::MacPlatform;

        let mut mac_platform = MacPlatform::new();
        let dispatch_log = error_log.clone();
        mac_platform.session().start(Box::new(move |event| {
            if let Err(err) = slint::invoke_from_event_loop(move || {
                with_ui_host(|host| host.on_session_event(event));
                arm_next_tick();
            }) {
                dispatch_log.log_error("session_event_dispatch", &err);
            }
        }));

        let platform: Arc<dyn PlatformServices> = if is_isolated {
            Arc::new(DemoPlatformServices::new(mac_platform))
        } else {
            Arc::new(mac_platform)
        };
        (platform, None)
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = error_log;
        let platform: Arc<dyn PlatformServices> = if is_isolated {
            Arc::new(DemoPlatformServices::new(damt_platform::Unsupported))
        } else {
            Arc::new(damt_platform::Unsupported)
        };
        (platform, None)
    }
}

struct PlatformHolder;

#[cfg(windows)]
fn attach_parent_console_if_needed() {
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
    };

    // SAFETY: `GetStdHandle` and `AttachConsole` are safe Win32 console queries/attachments with
    // no Rust-managed memory aliasing.
    unsafe {
        let missing_stdout = match GetStdHandle(STD_OUTPUT_HANDLE) {
            Ok(handle) => handle.is_invalid() || handle.0.is_null(),
            Err(_) => true,
        };
        if missing_stdout {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

fn format_memory_line(label: &str) -> String {
    let (priv_bytes, ws_bytes) = process_memory_bytes();
    format!(
        "{label}: private_mb={:.2} private_bytes={} working_set_mb={:.2} working_set_bytes={}",
        priv_bytes as f64 / 1_048_576.0,
        priv_bytes,
        ws_bytes as f64 / 1_048_576.0,
        ws_bytes,
    )
}

fn print_latency_line(sample: Option<BreakLatencySample>) {
    match sample {
        Some(s) => {
            let fp = s
                .due_to_first_paint_ms
                .map_or_else(|| "pending".to_owned(), |v| format!("{v:.2}"));
            println!(
                "Ocy break latency: kind={} prewarmed={} due_to_show_ms={:.2} due_to_first_paint_ms={}",
                s.kind, s.prewarmed, s.due_to_show_ms, fp
            );
        }
        None => {
            println!("Ocy break latency: none");
        }
    }
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let instance_scope = match parse_instance_scope(&raw_args) {
        Ok(scope) => scope,
        Err(err) => {
            #[cfg(windows)]
            attach_parent_console_if_needed();
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = parse_soak_cycles(&raw_args) {
        #[cfg(windows)]
        attach_parent_console_if_needed();
        eprintln!("{err}");
        return ExitCode::FAILURE;
    }
    let demo_language = parse_demo_language(&raw_args);
    // Test-only crash trigger (cargo feature `crash-test`): parsed before the args are
    // consumed; honoured only in isolated runs (checked right after `is_isolated`).
    #[cfg(feature = "crash-test")]
    let crash_test = ocy_app::crash_reports::parse_crash_test(&raw_args);
    let cli = parse_cli_args(raw_args);

    #[cfg(windows)]
    if cli.report_memory || cli.demo_break.is_some() || cli.soak_cycles.is_some() {
        attach_parent_console_if_needed();
    }

    let is_demo_or_probe = is_probe_mode(&cli);
    let is_isolated = is_demo_or_probe || instance_scope.is_some();
    // The deliberate-crash trigger may never run against the user's real data folder.
    #[cfg(feature = "crash-test")]
    if crash_test.is_some() && !is_isolated {
        #[cfg(windows)]
        attach_parent_console_if_needed();
        eprintln!("--crash-test needs an isolated data folder (--instance-scope or a demo run)");
        return ExitCode::FAILURE;
    }
    // `--demo-license` is only honoured in isolated runs of a Store build; otherwise it is
    // ignored with one line on stderr (the flag alone never makes a run isolated).
    if cli.demo_license.is_some() && !demo_license_is_honoured(is_isolated) {
        #[cfg(windows)]
        attach_parent_console_if_needed();
        eprintln!("--demo-license needs an isolated demo run of the Store build");
    }
    let temp_demo_dir = if is_isolated {
        Some(std::env::temp_dir().join(format!("ocy-demo-{}", std::process::id())))
    } else {
        None
    };
    let data_dir = temp_demo_dir
        .clone()
        .unwrap_or_else(|| AppDataPaths::new(APP_DATA_DIR_NAME).data_dir());
    let error_log = ErrorLog::new(&data_dir, ERROR_LOG_PREFIX);
    install_panic_hook(error_log.clone());
    // Crash-report capture (CR1): local reports only, nothing is ever sent here. The
    // panic hook chains with the damt-app hook installed above (both run). Isolated
    // demo runs skip the capture entirely unless --crash-test asked for a crash.
    #[cfg(feature = "crash-test")]
    let crash_capture = !is_isolated || crash_test.is_some();
    #[cfg(not(feature = "crash-test"))]
    let crash_capture = !is_isolated;
    if crash_capture {
        ocy_app::crash_reports::install(data_dir.clone());
    }

    let instance_id = resolve_instance_id(&cli, instance_scope.as_deref(), std::process::id());
    let mut single_instance_guard = match SingleInstance::acquire(&instance_id) {
        Acquire::First(guard) => guard,
        Acquire::AlreadyRunning => {
            if let Some(dir) = temp_demo_dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            return ExitCode::SUCCESS;
        }
    };

    let owner_window = match OwnerWindow::create() {
        Ok(owner) => Some(owner),
        Err(err) => {
            error_log.log(Some("owner_window_create"), &err);
            None
        }
    };
    let owner_hwnd = owner_window.as_ref().and_then(OwnerWindow::raw_hwnd);

    let selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into());
    #[cfg(target_os = "macos")]
    let selector = {
        let mut builder = slint::winit_030::winit::event_loop::EventLoop::<
            slint::winit_030::SlintEvent,
        >::with_user_event();
        ocy_app::macos_window::configure_event_loop_builder(&mut builder);
        selector.with_winit_event_loop_builder(builder)
    };
    if let Err(err) = selector
        .with_winit_window_attributes_hook(move |attrs| {
            let attrs = apply_window_attributes_for_role(attrs, owner_hwnd);
            #[cfg(target_os = "linux")]
            let attrs = ocy_app::linux_window::apply_window_attributes_for_role(attrs);
            #[cfg(target_os = "macos")]
            let attrs = ocy_app::macos_window::apply_window_attributes_for_role(attrs);
            attrs
        })
        .select()
    {
        error_log.log_error("slint_backend_select", &err);
        return ExitCode::FAILURE;
    }

    #[cfg(target_os = "linux")]
    {
        ocy_app::linux_window::init_winit_backend();
        if let Err(err) = slint::set_xdg_app_id(ocy_app::instance::APP_ID) {
            error_log.log_error("set_xdg_app_id", &err);
        }
    }

    #[cfg(target_os = "macos")]
    let _ = ocy_app::macos_window::ensure_accessory_activation_policy();

    let mut settings = if is_isolated {
        default_app_settings()
    } else {
        load_app_settings(&AppDataStore::new(&data_dir, Arc::new(SystemClock)))
    };
    if is_isolated && let Some(lang) = demo_language {
        settings.language = lang;
    }

    let strings = Arc::new(Strings::from_language(&settings.language));
    if let Some(rtl) = strings.rtl_fallback_from() {
        error_log.log(
            Some("i18n"),
            &format!("RTL language {rtl:?} is not supported yet; falling back to English"),
        );
    }
    let soak_offset_clock = if cli.soak_cycles.is_some() {
        build_soak_settings(&mut settings);
        Some(Arc::new(OffsetClock::new(0)))
    } else {
        None
    };
    let demo_plan = if cli.soak_cycles.is_some() {
        None
    } else {
        cli.demo_break
            .map(|demo| build_demo_launch_plan(demo, cli.demo_in_secs, &mut settings))
    };
    let demo_offset_clock = demo_plan
        .as_ref()
        .filter(|plan| plan.initial_clock_offset_secs != 0)
        .map(|plan| Arc::new(OffsetClock::new(plan.initial_clock_offset_secs)));
    let clock: Arc<dyn Clock> = match (&soak_offset_clock, &demo_offset_clock) {
        (Some(c), _) => Arc::clone(c) as Arc<dyn Clock>,
        (None, Some(c)) => Arc::clone(c) as Arc<dyn Clock>,
        (None, None) => Arc::new(SystemClock),
    };
    let initial_state = demo_plan
        .as_ref()
        .and_then(|plan| plan.initial_state.clone());
    let engine_store = if is_demo_or_probe {
        None
    } else {
        Some(AppDataStore::new(&data_dir, Arc::clone(&clock)))
    };
    let settings_store = if !is_demo_or_probe || cli.demo_settings {
        Some(AppDataStore::new(&data_dir, Arc::clone(&clock)))
    } else {
        None
    };

    let (platform, _holder) = build_platform_services(&error_log, is_isolated);
    let initial_settings = settings.clone();
    let engine = BreakEngine::new(
        settings,
        Arc::clone(&platform),
        Arc::clone(&clock),
        engine_store,
        None,
        initial_state,
        None,
    );
    let initial_snapshot = engine.snapshot();

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let tray_sink = {
        let tray_component = match OcyTray::new() {
            Ok(tray) => tray,
            Err(err) => {
                error_log.log_error("tray_create", &err);
                return ExitCode::FAILURE;
            }
        };
        SlintTraySink::new(tray_component, error_log.clone())
    };
    #[cfg(target_os = "linux")]
    let tray_sink = LinuxTraySink::spawn(error_log.clone());
    #[cfg(target_os = "macos")]
    let tray_sink = MacTraySink::spawn(error_log.clone());

    let taskbar_theme: Rc<dyn damt_platform::TaskbarThemeProvider> = Rc::new(TaskbarThemeAdapter {
        platform: Arc::clone(&platform),
    });

    let initial_app_theme = match platform.app_theme().is_light_app_theme() {
        Some(false) => Some(AppTheme::Dark),
        _ => Some(AppTheme::Light),
    };

    let tray_presenter = TrayPresenter::new(Arc::clone(&strings), Arc::clone(&clock));
    let tray_controller = TrayController::new(
        tray_presenter,
        tray_sink,
        taskbar_theme,
        Arc::clone(&clock),
        initial_app_theme,
        &initial_snapshot,
    );

    let callbacks = BreakActionCallbacks {
        on_skip: Rc::new(|| {
            with_ui_host(|h| {
                let _ = h.skip_current_break();
            });
            arm_next_tick();
        }),
        on_start_long_break_now: Rc::new(|| {
            with_ui_host(LiveHost::start_long_break_now);
            arm_next_tick();
        }),
        on_postpone_long_break: Rc::new(|| {
            with_ui_host(|h| {
                let _ = h.postpone_long_break();
            });
            arm_next_tick();
        }),
        on_hold_game: Rc::new(|| {
            with_ui_host(|h| {
                let _ = h.hold_current_game_and_close_card();
            });
            arm_next_tick();
        }),
    };

    let latency_options = LatencyOptions {
        log_to_file: cli.report_memory || std::env::var_os("OCY_DEBUG_LATENCY").is_some(),
        print_on_first_paint: cli.demo_break.is_some()
            && !cli.report_memory
            && cli.soak_cycles.is_none(),
    };
    let slint_break_windows = SlintBreakWindows::new(
        Arc::clone(&platform),
        Arc::clone(&clock),
        Arc::clone(&strings),
        error_log.clone(),
        callbacks,
        initial_settings.clone(),
        latency_options,
    );
    let latency_handle = slint_break_windows.latency_handle();
    let break_windows = OverlayBreakWindows::with_clock(
        slint_break_windows,
        Arc::clone(&platform),
        Arc::clone(&clock),
    );
    let settings_callbacks = SettingsCallbacks {
        on_save: Rc::new(|updated| {
            let saved = with_ui_host_ret(|h| h.save_settings(updated)).unwrap_or(false);
            arm_next_tick();
            saved
        }),
        on_try_short_break: Rc::new(|| {
            with_ui_host(|h| {
                let _ = h.start_short_break_now();
            });
            arm_next_tick();
        }),
        on_try_long_break: Rc::new(|| {
            with_ui_host(LiveHost::start_long_break_now);
            arm_next_tick();
        }),
    };
    #[cfg(feature = "store")]
    let mut settings_window = SlintSettingsWindow::new(
        Arc::clone(&platform),
        Arc::clone(&clock),
        Arc::clone(&strings),
        error_log.clone(),
        settings_callbacks,
        initial_settings,
    );
    #[cfg(not(feature = "store"))]
    let settings_window = SlintSettingsWindow::new(
        Arc::clone(&platform),
        Arc::clone(&clock),
        Arc::clone(&strings),
        error_log.clone(),
        settings_callbacks,
        initial_settings,
    );
    #[cfg(feature = "store")]
    if !is_isolated {
        settings_window.set_on_buy(Rc::new(store_license::trigger_purchase));
    } else if cli.demo_license.is_some() {
        // Honoured `--demo-license` run: Buy must be a logged no-op that never reaches
        // `request_purchase_async`.
        let demo_buy_log = error_log.clone();
        settings_window.set_on_buy(Rc::new(move || {
            demo_buy_log.log(Some("demo_purchase"), "demo: purchase skipped");
        }));
    }
    let notifier: Rc<dyn damt_platform::Notifier> = if is_isolated {
        Rc::new(RecordingNotifier::new())
    } else {
        Rc::new(NotifierAdapter {
            platform: Arc::clone(&platform),
        })
    };

    let coordinator = BreakUiCoordinator::new(break_windows, settings_window);
    let host = Rc::new(RefCell::new(
        AppHost::with_store(
            engine,
            coordinator,
            tray_controller,
            error_log.clone(),
            settings_store,
        )
        .with_notifier(notifier),
    ));

    UI_HOST.with(|slot| {
        *slot.borrow_mut() = Some(Rc::downgrade(&host));
    });

    // Anonymous usage stats (opt-in, off by default): a no-key build installs nothing at
    // all. The sender thread sleeps ~1 minute before its first attempt, so startup is never
    // blocked and never sends before the user has been asked (AN2 asks; nothing is due until
    // consent becomes Granted).
    if !is_isolated {
        // The manager and handle are installed process-wide (usage_stats::global), so AN2's
        // Settings toggle can reach them; the local binding only proves the build has a key.
        let _usage_stats = ocy_app::usage_stats::start_from_build(
            data_dir.clone(),
            Arc::clone(&clock),
            error_log.clone(),
        );
        // Settings load: apply the loaded consent once, so the mirror and the install
        // record reflect the persisted state from the very first sender attempt. Every
        // later consent change arrives through `AppHost::save_settings`.
        ocy_app::usage_stats::apply_consent_from_settings(host.borrow().engine().settings());
    }

    // Crash reports (CR1): apply the loaded crash-report consent once (the host's
    // in-memory settings are the single source of truth; every later change arrives
    // through `AppHost::save_settings`), then convert any native raw record a previous
    // crashed run left behind. Both are no-ops without an installed crash store.
    ocy_app::crash_reports::apply_crash_consent_from_settings(host.borrow().engine().settings());
    let _ = ocy_app::crash_reports::convert_native_pending();

    // With the feature on, the deliberate crash works in release builds too (the real
    // abort path, stripped exe); the hooks above have already written the report by the
    // time the process dies.
    #[cfg(feature = "crash-test")]
    if let Some(kind) = crash_test {
        ocy_app::crash_reports::run_crash_test(kind);
    }

    #[cfg(feature = "store")]
    if !is_isolated {
        let cache_path = data_dir.join(ocy_app::license::LICENSE_CACHE_FILE_NAME);
        let initial_license =
            store_license::init(cache_path, Arc::clone(&clock), error_log.clone());
        host.borrow_mut()
            .set_license_state(initial_license, clock.now().timestamp());
        store_license::refresh();
    } else if let Some(demo) = cli.demo_license {
        // Honoured `--demo-license` run: set the state directly, before any window can paint.
        // Isolated runs never call `store_license::init`/`refresh` and the periodic recheck
        // no-ops without a runtime, so no Microsoft Store query ever runs and `license.json`
        // is never written; Buy is the logged no-op registered above.
        let now = clock.now().timestamp();
        let state = ocy_app::break_logic::demo_license_state(demo, now);
        host.borrow_mut().set_license_state(state, now);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let host_ref = host.borrow();
        let tray = host_ref.tray().sink().component();
        tray.on_tray_clicked(|| with_ui_host(LiveHost::on_tray_left_click));
        tray.on_take_short_break_clicked(|| {
            with_ui_host(|h| {
                let _ = h.start_short_break_now();
            });
            arm_next_tick();
        });
        tray.on_take_long_break_clicked(|| {
            with_ui_host(LiveHost::start_long_break_now);
            arm_next_tick();
        });
        tray.on_hold_game_clicked(|| {
            with_ui_host(|h| {
                let _ = h.hold_until_game_closes();
            });
            arm_next_tick();
        });
        tray.on_pause_1h_clicked(|| {
            with_ui_host(|h| h.pause_from_tray(PauseSource::OneHour));
            arm_next_tick();
        });
        tray.on_pause_3h_clicked(|| {
            with_ui_host(|h| h.pause_from_tray(PauseSource::ThreeHours));
            arm_next_tick();
        });
        tray.on_pause_until_resumed_clicked(|| {
            with_ui_host(|h| h.pause_from_tray(PauseSource::UntilResumed));
            arm_next_tick();
        });
        tray.on_resume_clicked(|| {
            with_ui_host(LiveHost::resume);
            arm_next_tick();
        });
        tray.on_settings_clicked(|| with_ui_host(LiveHost::show_settings));
        tray.on_quit_clicked(|| {
            with_ui_host(LiveHost::shutdown);
            let _ = slint::quit_event_loop();
        });
        if let Err(err) = tray.show() {
            error_log.log_error("tray_show", &err);
            return ExitCode::FAILURE;
        }
    }

    if !is_isolated {
        Timer::single_shot(STARTUP_NOTIFICATION_DELAY, || {
            with_ui_host(LiveHost::on_startup_notification_timer);
        });
    }

    let listen_log = error_log.clone();
    single_instance_guard.listen(move || {
        if let Err(err) = slint::invoke_from_event_loop(|| {
            with_ui_host(LiveHost::show_settings);
        }) {
            listen_log.log_error("single_instance_dispatch", &err);
        }
    });

    if cli.demo_settings {
        host.borrow_mut().show_settings();
    }

    if let (Some(demo), Some(plan)) = (cli.demo_break, &demo_plan)
        && plan.trigger_immediately
    {
        match demo {
            DemoBreak::Short => {
                let _ = host.borrow_mut().start_short_break_now();
            }
            DemoBreak::Warning => {
                if let Some(c) = &demo_offset_clock {
                    c.set_offset_secs(0);
                }
                host.borrow_mut().on_tick();
            }
            DemoBreak::Long => {
                host.borrow_mut().start_long_break_now();
            }
        }
    }

    // Adaptive scheduler: a single-shot timer re-armed after every tick and after
    // anything that changes the schedule (see arm_next_tick).
    SCHEDULER.with(|slot| *slot.borrow_mut() = Some((Timer::default(), Arc::clone(&clock))));
    arm_next_tick();

    if let Some(soak_cycles) = cli.soak_cycles {
        let soak_clock = soak_offset_clock
            .as_ref()
            .map(Arc::clone)
            .unwrap_or_else(|| Arc::new(OffsetClock::new(0)));
        start_soak_probe(
            Rc::downgrade(&host),
            SoakTimings::LIVE,
            soak_cycles,
            SoakHooks {
                advance_clock_secs: Rc::new(move |secs| soak_clock.advance_secs(secs)),
                on_schedule_changed: Rc::new(arm_next_tick),
                sample_memory: Rc::new(process_memory_bytes),
                on_cycle: Rc::new(|sample| {
                    println!("{}", format_soak_cycle_line(&sample));
                }),
                on_done: Rc::new(|summary| {
                    println!("{}", format_soak_summary_line(&summary));
                    with_ui_host(LiveHost::shutdown);
                    let _ = slint::quit_event_loop();
                }),
            },
        );
    } else if cli.report_memory {
        if cli.demo_break.is_some() {
            let lat_ref = Rc::clone(&latency_handle);
            let wait_secs = cli.demo_in_secs.unwrap_or(0).saturating_add(10);
            Timer::single_shot(Duration::from_secs(wait_secs), move || {
                print_latency_line(*lat_ref.borrow());
                println!("{}", format_memory_line("Ocy break memory (10s)"));
                println!(
                    "Ocy scheduler: wakeups_per_min={}",
                    wakeups_per_min(wait_secs)
                );
                with_ui_host(LiveHost::shutdown);
                let _ = slint::quit_event_loop();
            });
        } else {
            let lat_ref = Rc::clone(&latency_handle);
            Timer::single_shot(Duration::from_secs(10), move || {
                println!("{}", format_memory_line("Ocy idle memory (10s)"));
                println!("Ocy scheduler: wakeups_per_min={}", wakeups_per_min(10));
                with_ui_host(|h| {
                    h.coordinator_mut()
                        .break_windows_mut()
                        .inner_mut()
                        .prewarm_short_break_now();
                });

                let lat_ref2 = Rc::clone(&lat_ref);
                Timer::single_shot(Duration::from_millis(250), move || {
                    with_ui_host(|h| {
                        let _ = h.start_short_break_now();
                    });

                    let lat_ref3 = Rc::clone(&lat_ref2);
                    Timer::single_shot(Duration::from_millis(250), move || {
                        print_latency_line(*lat_ref3.borrow());
                        println!("{}", format_memory_line("Ocy break memory (prewarmed)"));
                        with_ui_host(|h| {
                            let _ = h.skip_current_break();
                        });

                        Timer::single_shot(Duration::from_millis(250), || {
                            println!("{}", format_memory_line("Ocy post-break memory"));
                            with_ui_host(LiveHost::shutdown);
                            let _ = slint::quit_event_loop();
                        });
                    });
                });
            });
        }
    }

    let run_result = slint::run_event_loop_until_quit();
    if let Err(err) = &run_result {
        error_log.log_error("event_loop", err);
    }

    SCHEDULER.with(|slot| {
        let borrow = slot.borrow();
        if let Some((timer, _)) = borrow.as_ref() {
            timer.stop();
        }
    });
    host.borrow_mut().shutdown();
    #[cfg(feature = "store")]
    store_license::clear();
    UI_HOST.with(|slot| {
        *slot.borrow_mut() = None;
    });
    drop(host);
    drop(owner_window);
    drop(single_instance_guard);
    if let Some(dir) = temp_demo_dir {
        let _ = std::fs::remove_dir_all(dir);
    }

    if run_result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

struct TaskbarThemeAdapter {
    platform: Arc<dyn PlatformServices>,
}

impl damt_platform::TaskbarThemeProvider for TaskbarThemeAdapter {
    fn is_supported(&self) -> bool {
        self.platform.taskbar_theme().is_supported()
    }

    fn is_light_taskbar(&self) -> Option<bool> {
        self.platform.taskbar_theme().is_light_taskbar()
    }
}

struct NotifierAdapter {
    platform: Arc<dyn PlatformServices>,
}

impl damt_platform::Notifier for NotifierAdapter {
    fn is_supported(&self) -> bool {
        self.platform.notifier().is_supported()
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), damt_platform::PlatformError> {
        self.platform.notifier().notify(title, body)
    }
}

#[cfg(target_os = "linux")]
struct LinuxTraySink {
    handle: Option<damt_platform_linux::LinuxTrayHandle>,
    error_log: ErrorLog,
}

#[cfg(target_os = "linux")]
impl LinuxTraySink {
    fn spawn(error_log: ErrorLog) -> Self {
        use damt_platform_linux::{
            LinuxTrayConfig, LinuxTrayHandle, LinuxTrayIconState, LinuxTrayPresentation,
        };

        let dispatch_log = error_log.clone();
        let config = LinuxTrayConfig {
            id: ocy_app::instance::APP_ID.to_owned(),
            title: ocy_app::notifications::APP_DISPLAY_NAME.to_owned(),
        };
        let handle = match LinuxTrayHandle::spawn(
            config,
            LinuxTrayPresentation::default(),
            LinuxTrayIconState::Active,
            false,
            move |action| {
                if let Err(err) = slint::invoke_from_event_loop(move || {
                    dispatch_linux_tray_action(action);
                }) {
                    dispatch_log.log_error("linux_tray_dispatch", &err);
                }
            },
        ) {
            Ok(h) => Some(h),
            Err(err) => {
                error_log.log_error("tray_create", &err);
                None
            }
        };
        Self { handle, error_log }
    }
}

#[cfg(target_os = "linux")]
fn dispatch_linux_tray_action(action: damt_platform_linux::LinuxTrayAction) {
    use damt_platform_linux::LinuxTrayAction;

    match action {
        LinuxTrayAction::TrayClicked => with_ui_host(LiveHost::on_tray_left_click),
        LinuxTrayAction::TakeShortBreakNow => {
            with_ui_host(|h| {
                let _ = h.start_short_break_now();
            });
            arm_next_tick();
        }
        LinuxTrayAction::TakeLongBreakNow => {
            with_ui_host(LiveHost::start_long_break_now);
            arm_next_tick();
        }
        LinuxTrayAction::HoldUntilGameCloses => {
            with_ui_host(|h| {
                let _ = h.hold_until_game_closes();
            });
            arm_next_tick();
        }
        LinuxTrayAction::Pause1Hour => {
            with_ui_host(|h| h.pause_from_tray(PauseSource::OneHour));
            arm_next_tick();
        }
        LinuxTrayAction::Pause3Hours => {
            with_ui_host(|h| h.pause_from_tray(PauseSource::ThreeHours));
            arm_next_tick();
        }
        LinuxTrayAction::PauseUntilResumed => {
            with_ui_host(|h| h.pause_from_tray(PauseSource::UntilResumed));
            arm_next_tick();
        }
        LinuxTrayAction::Resume => {
            with_ui_host(LiveHost::resume);
            arm_next_tick();
        }
        LinuxTrayAction::Settings => with_ui_host(LiveHost::show_settings),
        LinuxTrayAction::Quit => {
            with_ui_host(LiveHost::shutdown);
            let _ = slint::quit_event_loop();
        }
    }
}

#[cfg(target_os = "linux")]
impl ocy_app::tray::TraySink for LinuxTraySink {
    fn set_tooltip(&mut self, tooltip: &str) {
        if let Some(handle) = &self.handle {
            handle.set_tooltip(tooltip);
        }
    }

    fn set_icon(&mut self, state: ocy_app::tray::TrayIconState, use_light_set: bool) {
        use damt_platform_linux::LinuxTrayIconState;
        use ocy_app::tray::TrayIconState;

        let mapped = match state {
            TrayIconState::Active => LinuxTrayIconState::Active,
            TrayIconState::Paused => LinuxTrayIconState::Paused,
            TrayIconState::OnBreak => LinuxTrayIconState::OnBreak,
            TrayIconState::Attention => LinuxTrayIconState::Attention,
        };
        if let Some(handle) = &self.handle
            && let Err(err) = handle.set_icon(mapped, use_light_set)
        {
            self.error_log.log_error("tray_icon", &err);
        }
    }

    fn set_menu(&mut self, presentation: &ocy_app::tray::TrayPresentation) {
        use damt_platform_linux::{LinuxTrayMenuItem, LinuxTrayPresentation};
        use ocy_app::tray::TrayMenuItemState;

        fn map_item(item: &TrayMenuItemState) -> LinuxTrayMenuItem {
            LinuxTrayMenuItem {
                text: item.text.clone(),
                is_visible: item.is_visible,
                is_enabled: item.is_enabled,
                is_checked: item.is_checked,
            }
        }

        if let Some(handle) = &self.handle {
            handle.set_menu(LinuxTrayPresentation {
                tooltip: presentation.tooltip.clone(),
                pause_status: map_item(&presentation.pause_status),
                take_short_break_now: map_item(&presentation.take_short_break_now),
                take_long_break_now: map_item(&presentation.take_long_break_now),
                hold_until_game_closes: map_item(&presentation.hold_until_game_closes),
                pause_1_hour: map_item(&presentation.pause_1_hour),
                pause_3_hours: map_item(&presentation.pause_3_hours),
                pause_until_resumed: map_item(&presentation.pause_until_resumed),
                resume: map_item(&presentation.resume),
                settings: map_item(&presentation.settings),
                quit: map_item(&presentation.quit),
            });
        }
    }
}

#[cfg(target_os = "macos")]
struct MacTraySink {
    handle: Option<damt_platform_mac::MacTrayHandle>,
    error_log: ErrorLog,
}

#[cfg(target_os = "macos")]
impl MacTraySink {
    fn spawn(error_log: ErrorLog) -> Self {
        use damt_platform_mac::{
            MacTrayConfig, MacTrayHandle, MacTrayIconState, MacTrayPresentation,
        };

        let dispatch_log = error_log.clone();
        let config = MacTrayConfig {
            title: ocy_app::notifications::APP_DISPLAY_NAME.to_owned(),
        };
        let handle = match MacTrayHandle::spawn(
            config,
            MacTrayPresentation::default(),
            MacTrayIconState::Active,
            move |action| {
                if let Err(err) = slint::invoke_from_event_loop(move || {
                    dispatch_mac_tray_action(action);
                }) {
                    dispatch_log.log_error("mac_tray_dispatch", &err);
                }
            },
        ) {
            Ok(h) => Some(h),
            Err(err) => {
                error_log.log_error("tray_create", &err);
                None
            }
        };
        Self { handle, error_log }
    }
}

#[cfg(target_os = "macos")]
fn dispatch_mac_tray_action(action: damt_platform_mac::MacTrayAction) {
    use damt_platform_mac::MacTrayAction;

    match action {
        MacTrayAction::TakeShortBreakNow => {
            with_ui_host(|h| {
                let _ = h.start_short_break_now();
            });
            arm_next_tick();
        }
        MacTrayAction::TakeLongBreakNow => {
            with_ui_host(LiveHost::start_long_break_now);
            arm_next_tick();
        }
        MacTrayAction::HoldUntilGameCloses => {
            with_ui_host(|h| {
                let _ = h.hold_until_game_closes();
            });
            arm_next_tick();
        }
        MacTrayAction::Pause1Hour => {
            with_ui_host(|h| h.pause_from_tray(PauseSource::OneHour));
            arm_next_tick();
        }
        MacTrayAction::Pause3Hours => {
            with_ui_host(|h| h.pause_from_tray(PauseSource::ThreeHours));
            arm_next_tick();
        }
        MacTrayAction::PauseUntilResumed => {
            with_ui_host(|h| h.pause_from_tray(PauseSource::UntilResumed));
            arm_next_tick();
        }
        MacTrayAction::Resume => {
            with_ui_host(LiveHost::resume);
            arm_next_tick();
        }
        MacTrayAction::Settings => with_ui_host(LiveHost::show_settings),
        MacTrayAction::Quit => {
            with_ui_host(LiveHost::shutdown);
            let _ = slint::quit_event_loop();
        }
    }
}

#[cfg(target_os = "macos")]
impl ocy_app::tray::TraySink for MacTraySink {
    fn set_tooltip(&mut self, tooltip: &str) {
        if let Some(handle) = &self.handle {
            handle.set_tooltip(tooltip);
        }
    }

    fn set_icon(&mut self, state: ocy_app::tray::TrayIconState, use_light_set: bool) {
        use damt_platform_mac::MacTrayIconState;
        use ocy_app::tray::TrayIconState;

        let mapped = match state {
            TrayIconState::Active => MacTrayIconState::Active,
            TrayIconState::Paused => MacTrayIconState::Paused,
            TrayIconState::OnBreak => MacTrayIconState::OnBreak,
            TrayIconState::Attention => MacTrayIconState::Attention,
        };
        if let Some(handle) = &mut self.handle
            && let Err(err) = handle.set_icon(mapped, use_light_set)
        {
            self.error_log.log_error("tray_icon", &err);
        }
    }

    fn set_menu(&mut self, presentation: &ocy_app::tray::TrayPresentation) {
        use damt_platform_mac::{MacTrayMenuItem, MacTrayPresentation};
        use ocy_app::tray::TrayMenuItemState;

        fn map_item(item: &TrayMenuItemState) -> MacTrayMenuItem {
            MacTrayMenuItem {
                text: item.text.clone(),
                is_visible: item.is_visible,
                is_enabled: item.is_enabled,
                is_checked: item.is_checked,
            }
        }

        if let Some(handle) = &self.handle {
            handle.set_menu(&MacTrayPresentation {
                tooltip: presentation.tooltip.clone(),
                pause_status: map_item(&presentation.pause_status),
                take_short_break_now: map_item(&presentation.take_short_break_now),
                take_long_break_now: map_item(&presentation.take_long_break_now),
                hold_until_game_closes: map_item(&presentation.hold_until_game_closes),
                pause_1_hour: map_item(&presentation.pause_1_hour),
                pause_3_hours: map_item(&presentation.pause_3_hours),
                pause_until_resumed: map_item(&presentation.pause_until_resumed),
                resume: map_item(&presentation.resume),
                settings: map_item(&presentation.settings),
                quit: map_item(&presentation.quit),
            });
        }
    }
}

#[cfg(feature = "store")]
mod store_license {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::Arc;

    use damt_app::ErrorLog;
    use damt_platform::Clock;
    use damt_platform_win::{RawStoreLicense, StorePurchaseOutcome};
    use ocy_app::license::{LicenseManager, LicenseState};

    use super::{UI_HOST, arm_next_tick, with_ui_host, with_ui_host_ret};

    struct StoreLicenseRuntime {
        manager: LicenseManager,
        in_flight: bool,
        error_logged: bool,
        error_log: ErrorLog,
        clock: Arc<dyn Clock>,
    }

    thread_local! {
        static RUNTIME: RefCell<Option<StoreLicenseRuntime>> = const { RefCell::new(None) };
    }

    pub fn init(cache_path: PathBuf, clock: Arc<dyn Clock>, error_log: ErrorLog) -> LicenseState {
        let manager = LicenseManager::new(Some(cache_path), Arc::clone(&clock));
        let initial = manager.state();
        RUNTIME.with(|slot| {
            *slot.borrow_mut() = Some(StoreLicenseRuntime {
                manager,
                in_flight: false,
                error_logged: false,
                error_log,
                clock,
            });
        });
        initial
    }

    pub fn clear() {
        RUNTIME.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }

    pub fn maybe_periodic_recheck() {
        let should_refresh = RUNTIME.with(|slot| {
            let borrow = slot.borrow();
            borrow
                .as_ref()
                .is_some_and(|rt| !rt.in_flight && rt.manager.is_recheck_due())
        });
        if should_refresh {
            refresh();
        }
    }

    pub fn refresh() {
        let can_start = RUNTIME.with(|slot| {
            let mut borrow = slot.borrow_mut();
            let Some(rt) = borrow.as_mut() else {
                return false;
            };
            if rt.in_flight {
                return false;
            }
            rt.in_flight = true;
            rt.manager.mark_check_started();
            true
        });
        if !can_start {
            return;
        }

        #[cfg(windows)]
        {
            damt_platform_win::query_app_license_async(|res| {
                let mapped = res.map_err(|err| err.to_string());
                let _ = slint::invoke_from_event_loop(move || {
                    on_query_completed(mapped);
                });
            });
        }
        #[cfg(not(windows))]
        {
            on_query_completed(Err(
                "Microsoft Store licensing is only available on Windows".to_owned(),
            ));
        }
    }

    fn on_query_completed(res: Result<RawStoreLicense, String>) {
        let update = RUNTIME.with(|slot| {
            let mut borrow = slot.borrow_mut();
            let rt = borrow.as_mut()?;
            rt.in_flight = false;
            let state = rt.manager.on_query_result(res);
            if let Some(err) = rt.manager.last_error() {
                if !rt.error_logged {
                    rt.error_logged = true;
                    rt.error_log.log(Some("store_license"), err);
                }
            } else {
                rt.error_logged = false;
            }
            Some((state, rt.clock.now().timestamp()))
        });

        if let Some((state, now)) = update
            && UI_HOST.with(|slot| slot.borrow().is_some())
        {
            with_ui_host(|host| host.set_license_state(state, now));
            arm_next_tick();
        }
    }

    pub fn trigger_purchase() {
        let owner_hwnd =
            with_ui_host_ret(|host| host.coordinator().settings_window().active_hwnd()).flatten();

        let Some(hwnd) = owner_hwnd else {
            RUNTIME.with(|slot| {
                if let Some(rt) = slot.borrow().as_ref() {
                    rt.error_log.log(
                        Some("store_purchase"),
                        "Settings window HWND unavailable for Store purchase",
                    );
                }
            });
            return;
        };

        #[cfg(windows)]
        {
            if let Err(err) =
                damt_platform_win::request_purchase_async(ocy_app::license::STORE_ID, hwnd, |res| {
                    let mapped = res.map_err(|err| err.to_string());
                    let _ = slint::invoke_from_event_loop(move || {
                        on_purchase_completed(mapped);
                    });
                })
            {
                RUNTIME.with(|slot| {
                    if let Some(rt) = slot.borrow().as_ref() {
                        rt.error_log.log_error("store_purchase_start", &err);
                    }
                });
            }
        }
        #[cfg(not(windows))]
        {
            let _ = hwnd;
            on_purchase_completed(Err(
                "Microsoft Store purchase is only available on Windows".to_owned()
            ));
        }
    }

    fn on_purchase_completed(res: Result<StorePurchaseOutcome, String>) {
        match res {
            Ok(outcome) if outcome.is_owned() => {
                let update = RUNTIME.with(|slot| {
                    let mut borrow = slot.borrow_mut();
                    let rt = borrow.as_mut()?;
                    let state = rt.manager.on_purchase_succeeded();
                    Some((state, rt.clock.now().timestamp()))
                });
                if let Some((state, now)) = update {
                    with_ui_host(|host| host.set_license_state(state, now));
                    arm_next_tick();
                }
                refresh();
            }
            Ok(StorePurchaseOutcome::NotPurchased) => {}
            Ok(other) => {
                RUNTIME.with(|slot| {
                    if let Some(rt) = slot.borrow().as_ref() {
                        rt.error_log.log(
                            Some("store_purchase"),
                            &format!("Store purchase did not complete: {other:?}"),
                        );
                    }
                });
            }
            Err(err) => {
                RUNTIME.with(|slot| {
                    if let Some(rt) = slot.borrow().as_ref() {
                        rt.error_log.log(Some("store_purchase"), &err);
                    }
                });
            }
        }
    }
}
