#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use std::cell::{Cell, RefCell};
use std::process::ExitCode;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use damt_app::{Acquire, AppDataPaths, ErrorLog, SingleInstance, install_panic_hook};
use damt_platform::{Clock, PlatformServices, SystemClock};
use jiff::{SignedDuration, ZonedDateTime};
use ocy_app::break_logic::{DemoBreak, build_demo_launch_plan, parse_cli_args};
use ocy_app::coordinator::{BreakUiCoordinator, OverlayBreakWindows};
use ocy_app::host::AppHost;
use ocy_app::instance::{is_probe_mode, parse_instance_scope, resolve_instance_id};
use ocy_app::scheduler;
use ocy_app::settings_logic::{DemoPlatformServices, SettingsCallbacks};
use ocy_app::slint_breaks::{
    BreakActionCallbacks, BreakLatencySample, LatencyOptions, SlintBreakWindows,
};
use ocy_app::slint_settings::SlintSettingsWindow;
use ocy_app::slint_tray::{OcyTray, SlintTraySink};
use ocy_app::strings::Strings;
use ocy_app::tray::{AppTheme, PauseSource, TrayController, TrayPresenter};
use ocy_app::win32_window::{OwnerWindow, apply_window_attributes_for_role};
use ocy_core::engine::BreakEngine;
use ocy_core::persistence::AppDataStore;
use ocy_core::settings::OcySettings;
use slint::{Timer, TimerMode};

const APP_DATA_DIR_NAME: &str = "Ocy";
const ERROR_LOG_PREFIX: &str = "ocy";

type LiveHost = AppHost<OverlayBreakWindows<SlintBreakWindows>, SlintSettingsWindow, SlintTraySink>;

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
            scheduler::next_wake_delay(&snapshot, clock.now().timestamp())
        })
        .unwrap_or(Duration::from_secs(1));
        timer.start(TimerMode::SingleShot, delay, || {
            WAKEUP_COUNT.with(|count| count.set(count.get() + 1));
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

#[derive(Debug)]
struct OffsetClock {
    offset_secs: AtomicI64,
}

impl OffsetClock {
    const fn new(offset_secs: i64) -> Self {
        Self {
            offset_secs: AtomicI64::new(offset_secs),
        }
    }

    fn set_offset_secs(&self, secs: i64) {
        self.offset_secs.store(secs, Ordering::Relaxed);
    }
}

impl Clock for OffsetClock {
    fn now(&self) -> ZonedDateTime {
        let now = SystemClock.now();
        let secs = self.offset_secs.load(Ordering::Relaxed);
        if secs == 0 {
            return now;
        }
        now.checked_add(SignedDuration::from_secs(secs))
            .unwrap_or(now)
    }
}

fn build_platform_services(
    error_log: &ErrorLog,
    safe_demo_autostart: bool,
) -> (Arc<dyn PlatformServices>, Option<PlatformHolder>) {
    #[cfg(windows)]
    {
        use damt_platform::SessionEventSource;
        use damt_platform_win::WinPlatform;

        let mut win_platform = WinPlatform::new(APP_DATA_DIR_NAME);
        let dispatch_log = error_log.clone();
        win_platform.session().start(Box::new(move |event| {
            if let Err(err) = slint::invoke_from_event_loop(move || {
                with_ui_host(|host| host.on_session_event(event));
                arm_next_tick();
            }) {
                dispatch_log.log_error("session_event_dispatch", &err);
            }
        }));

        let platform: Arc<dyn PlatformServices> = if safe_demo_autostart {
            Arc::new(DemoPlatformServices::new(win_platform))
        } else {
            Arc::new(win_platform)
        };
        (platform, None)
    }

    #[cfg(not(windows))]
    {
        let _ = error_log;
        let platform: Arc<dyn PlatformServices> = if safe_demo_autostart {
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

#[cfg(windows)]
fn process_memory_bytes() -> (u64, u64) {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut pmc = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    // SAFETY: `GetCurrentProcess` returns a valid pseudo-handle and `pmc` is an aligned
    // out-parameter with `cb` initialized to `size_of::<PROCESS_MEMORY_COUNTERS_EX>()`.
    let res = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut pmc).cast::<PROCESS_MEMORY_COUNTERS>(),
            pmc.cb,
        )
    };
    if res.is_ok() {
        (pmc.PrivateUsage as u64, pmc.WorkingSetSize as u64)
    } else {
        (0, 0)
    }
}

#[cfg(not(windows))]
fn process_memory_bytes() -> (u64, u64) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return (0, 0);
    };
    let mut rss_kb = 0u64;
    let mut anon_kb = 0u64;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            rss_kb = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("RssAnon:") {
            anon_kb = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
        }
    }
    (anon_kb * 1024, rss_kb * 1024)
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
    let cli = parse_cli_args(raw_args);

    #[cfg(windows)]
    if cli.report_memory || cli.demo_break.is_some() {
        attach_parent_console_if_needed();
    }

    let is_demo_or_probe = is_probe_mode(&cli);
    let is_isolated = is_demo_or_probe || instance_scope.is_some();
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

    if let Err(err) = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(move |attrs| {
            apply_window_attributes_for_role(attrs, owner_hwnd)
        })
        .select()
    {
        error_log.log_error("slint_backend_select", &err);
        return ExitCode::FAILURE;
    }

    let strings = Arc::new(Strings::from_embedded().unwrap_or_else(|err| {
        error_log.log_error("strings_load", &err);
        Strings::default()
    }));

    let mut settings = if is_isolated {
        OcySettings::default()
    } else {
        AppDataStore::new(&data_dir, Arc::new(SystemClock)).load_settings()
    };
    let demo_plan = cli
        .demo_break
        .map(|demo| build_demo_launch_plan(demo, cli.demo_in_secs, &mut settings));
    let demo_offset_clock = demo_plan
        .as_ref()
        .filter(|plan| plan.initial_clock_offset_secs != 0)
        .map(|plan| Arc::new(OffsetClock::new(plan.initial_clock_offset_secs)));
    let clock: Arc<dyn Clock> = match &demo_offset_clock {
        Some(c) => Arc::clone(c) as Arc<dyn Clock>,
        None => Arc::new(SystemClock),
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

    let tray_component = match OcyTray::new() {
        Ok(tray) => tray,
        Err(err) => {
            error_log.log_error("tray_create", &err);
            return ExitCode::FAILURE;
        }
    };

    let taskbar_theme: Rc<dyn damt_platform::TaskbarThemeProvider> = Rc::new(TaskbarThemeAdapter {
        platform: Arc::clone(&platform),
    });

    let initial_app_theme = match platform.app_theme().is_light_app_theme() {
        Some(false) => Some(AppTheme::Dark),
        _ => Some(AppTheme::Light),
    };

    let tray_sink = SlintTraySink::new(tray_component, error_log.clone());
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
    };

    let latency_options = LatencyOptions {
        log_to_file: cli.report_memory || std::env::var_os("OCY_DEBUG_LATENCY").is_some(),
        print_on_first_paint: cli.demo_break.is_some() && !cli.report_memory,
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
    let settings_window = SlintSettingsWindow::new(
        Arc::clone(&platform),
        Arc::clone(&clock),
        Arc::clone(&strings),
        error_log.clone(),
        settings_callbacks,
        initial_settings,
    );
    let coordinator = BreakUiCoordinator::new(break_windows, settings_window);
    let host = Rc::new(RefCell::new(AppHost::with_store(
        engine,
        coordinator,
        tray_controller,
        error_log.clone(),
        settings_store,
    )));

    UI_HOST.with(|slot| {
        *slot.borrow_mut() = Some(Rc::downgrade(&host));
    });

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

    if cli.report_memory {
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
