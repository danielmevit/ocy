use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use damt_app::ErrorLog;
use damt_platform::{Clock, PlatformServices, TaskbarThemeProvider, Unsupported};
use ocy_app::break_logic::{CliArgs, parse_cli_args};
use ocy_app::coordinator::{BreakUiCoordinator, OverlayBreakWindows, SettingsWindow};
use ocy_app::host::AppHost;
use ocy_app::instance::{is_probe_mode, resolve_instance_id, windows_mutex_name};
use ocy_app::slint_breaks::{BreakActionCallbacks, LatencyOptions, SlintBreakWindows};
use ocy_app::soak::{
    OffsetClock, SoakArgError, SoakCycleSample, SoakHooks, SoakSummary, SoakTimings,
    build_soak_settings, format_soak_cycle_line, format_soak_summary_line,
    least_squares_slope_after_cycle3, parse_soak_cycles, start_soak_probe, summarize_soak,
    validate_soak_cycles,
};
use ocy_app::strings::Strings;
use ocy_app::tray::{
    AppTheme, TrayController, TrayIconState, TrayPresentation, TrayPresenter, TraySink,
};
use ocy_core::engine::BreakEngine;
use ocy_core::settings::OcySettings;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{EventLoopProxy, Platform, PlatformError, WindowAdapter};

#[test]
fn soak_argument_parsing_accepts_bounds_and_rejects_invalid_values() {
    assert_eq!(parse_soak_cycles(Vec::<String>::new()), Ok(None));
    assert_eq!(parse_soak_cycles(["--demo-settings"]), Ok(None));
    assert_eq!(parse_soak_cycles(["--soak", "1"]), Ok(Some(1)));
    assert_eq!(parse_soak_cycles(["--soak", "30"]), Ok(Some(30)));
    assert_eq!(parse_soak_cycles(["--soak", "200"]), Ok(Some(200)));
    assert_eq!(parse_soak_cycles(["--soak=1"]), Ok(Some(1)));
    assert_eq!(parse_soak_cycles(["--soak=200"]), Ok(Some(200)));
    assert_eq!(
        parse_soak_cycles(["--instance-scope", "s1", "--soak", "42"]),
        Ok(Some(42))
    );

    assert_eq!(
        parse_soak_cycles(["--soak"]),
        Err(SoakArgError::MissingValue)
    );
    assert_eq!(
        parse_soak_cycles(["--soak", "0"]),
        Err(SoakArgError::OutOfRange(0))
    );
    assert_eq!(
        parse_soak_cycles(["--soak=0"]),
        Err(SoakArgError::OutOfRange(0))
    );
    assert_eq!(
        parse_soak_cycles(["--soak", "201"]),
        Err(SoakArgError::OutOfRange(201))
    );
    assert_eq!(
        parse_soak_cycles(["--soak=999"]),
        Err(SoakArgError::OutOfRange(999))
    );

    for bad in ["", " ", "-1", "+5", "abc", "12.5", "30s"] {
        assert_eq!(
            validate_soak_cycles(bad),
            Err(SoakArgError::InvalidValue(bad.to_owned())),
            "bad={bad:?}"
        );
        assert_eq!(
            parse_soak_cycles(["--soak", bad]),
            Err(SoakArgError::InvalidValue(bad.to_owned()))
        );
        let eq = format!("--soak={bad}");
        assert_eq!(
            parse_soak_cycles([eq]),
            Err(SoakArgError::InvalidValue(bad.to_owned()))
        );
    }

    let cli = parse_cli_args(["--soak", "30"].map(str::to_owned));
    assert_eq!(
        cli,
        CliArgs {
            soak_cycles: Some(30),
            ..CliArgs::default()
        }
    );
    assert!(is_probe_mode(&cli));
    assert_eq!(
        windows_mutex_name(&resolve_instance_id(&cli, None, 777)),
        r"Local\xyz.damt.ocy.probe-777"
    );
}

#[test]
fn soak_slope_maths_ignores_cycles_1_and_2_and_computes_least_squares() {
    // Fewer than 2 samples in cycles 3..=n yields 0.0 slope.
    assert_eq!(least_squares_slope_after_cycle3(&[]), 0.0);
    assert_eq!(least_squares_slope_after_cycle3(&[100]), 0.0);
    assert_eq!(least_squares_slope_after_cycle3(&[100, 500]), 0.0);
    assert_eq!(least_squares_slope_after_cycle3(&[100, 500, 900]), 0.0);

    // Plateau after warm-up: cycles 1 and 2 jump, cycles 3..=6 stay flat -> 0.0 bytes/cycle.
    let plateau = [
        3_700_000, 5_750_000, 5_750_000, 5_750_000, 5_750_000, 5_750_000,
    ];
    assert_eq!(least_squares_slope_after_cycle3(&plateau), 0.0);

    // Exact linear growth after cycle 3: cycles 3..=6 = [1000, 1500, 2000, 2500] -> +500.0/cycle.
    let linear_leak = [9_999_999, 123_456, 1000, 1500, 2000, 2500];
    let slope = least_squares_slope_after_cycle3(&linear_leak);
    assert!((slope - 500.0).abs() < 1e-9, "slope={slope}");

    // Negative slope: cycles 3..=5 = [6000, 4000, 2000] -> -2000.0/cycle.
    let shrinking = [100, 200, 6000, 4000, 2000];
    let neg_slope = least_squares_slope_after_cycle3(&shrinking);
    assert!((neg_slope + 2000.0).abs() < 1e-9, "neg_slope={neg_slope}");

    // Least-squares fit on noisy points: x = [0, 1, 2, 3], y = [100, 200, 200, 400].
    // mean_x = 1.5, mean_y = 225.0
    // dx = [-1.5, -0.5, 0.5, 1.5], dy = [-125, -25, -25, 175]
    // num = 187.5 + 12.5 - 12.5 + 262.5 = 450.0, den = 2.25 + 0.25 + 0.25 + 2.25 = 5.0 -> 90.0.
    let noisy = [1, 2, 100, 200, 200, 400];
    let noisy_slope = least_squares_slope_after_cycle3(&noisy);
    assert!(
        (noisy_slope - 90.0).abs() < 1e-9,
        "noisy_slope={noisy_slope}"
    );

    let summary = summarize_soak(&[3_700_000, 5_900_000, 5_750_000, 5_850_000, 5_950_000]);
    assert_eq!(
        summary,
        SoakSummary {
            cycles: 5,
            first_private: 3_700_000,
            last_private: 5_950_000,
            max_private: 5_950_000,
            growth_after_cycle3_per_cycle: 100_000.0,
        }
    );
    assert_eq!(
        format_soak_summary_line(&summary),
        "Ocy soak summary cycles=5 first_private=3700000 last_private=5950000 max_private=5950000 growth_after_cycle3_per_cycle=100000.00"
    );

    let line = format_soak_cycle_line(&SoakCycleSample {
        cycle: 2,
        private_bytes: 5_750_000,
        working_set_bytes: 12_000_000,
        live_break_windows: 0,
        art_cache_entries: 0,
        overlays: 0,
    });
    assert_eq!(
        line,
        "Ocy soak cycle=2 private_bytes=5750000 working_set_bytes=12000000 live_break_windows=0 art_cache_entries=0 overlays=0"
    );
}

struct ProxyState {
    queue: Mutex<VecDeque<Box<dyn FnOnce() + Send>>>,
    quit: AtomicBool,
    thread: Thread,
}

struct HeadlessEventLoopProxy(Arc<ProxyState>);

impl EventLoopProxy for HeadlessEventLoopProxy {
    fn quit_event_loop(&self) -> Result<(), slint::EventLoopError> {
        self.0.quit.store(true, Ordering::SeqCst);
        self.0.thread.unpark();
        Ok(())
    }

    fn invoke_from_event_loop(
        &self,
        event: Box<dyn FnOnce() + Send>,
    ) -> Result<(), slint::EventLoopError> {
        self.0
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(event);
        self.0.thread.unpark();
        Ok(())
    }
}

struct HeadlessPlatform {
    started_at: Instant,
    proxy: Arc<ProxyState>,
    windows: RefCell<Vec<Weak<MinimalSoftwareWindow>>>,
}

impl HeadlessPlatform {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            proxy: Arc::new(ProxyState {
                queue: Mutex::new(VecDeque::new()),
                quit: AtomicBool::new(false),
                thread: thread::current(),
            }),
            windows: RefCell::new(Vec::new()),
        }
    }

    fn render_pending_windows(&self, buf: &mut Vec<PremultipliedRgbaColor>) {
        let live: Vec<Rc<MinimalSoftwareWindow>> = {
            let mut list = self.windows.borrow_mut();
            list.retain(|w| w.strong_count() > 0);
            list.iter().filter_map(Weak::upgrade).collect()
        };
        for win in live {
            let size = win.size();
            let width = size.width as usize;
            let height = size.height as usize;
            if width == 0 || height == 0 {
                continue;
            }
            let needed = width.saturating_mul(height);
            if buf.len() < needed {
                buf.resize(needed, PremultipliedRgbaColor::default());
            }
            win.draw_if_needed(|renderer| {
                let _ = renderer.render(&mut buf[..needed], width);
            });
        }
    }
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let win = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        self.windows.borrow_mut().push(Rc::downgrade(&win));
        Ok(win)
    }

    fn duration_since_start(&self) -> Duration {
        self.started_at.elapsed()
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(HeadlessEventLoopProxy(Arc::clone(&self.proxy))))
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        let mut render_buf = Vec::new();
        self.proxy.quit.store(false, Ordering::SeqCst);
        loop {
            slint::platform::update_timers_and_animations();
            loop {
                let next = self
                    .proxy
                    .queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                match next {
                    Some(cb) => cb(),
                    None => break,
                }
            }
            self.render_pending_windows(&mut render_buf);
            if self.proxy.quit.load(Ordering::SeqCst) {
                break Ok(());
            }
            let wait = slint::platform::duration_until_next_timer_update()
                .unwrap_or(Duration::from_millis(10))
                .min(Duration::from_millis(10));
            if !wait.is_zero() {
                thread::park_timeout(wait);
            }
        }
    }
}

#[derive(Debug, Default)]
struct DummySettingsWindow;

impl SettingsWindow for DummySettingsWindow {
    fn show(&mut self) {}
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
fn headless_soak_3_cycles_drops_all_break_windows_and_art_cache_entries() {
    slint::platform::set_platform(Box::new(HeadlessPlatform::new()))
        .expect("headless testing platform should install cleanly");

    let temp_dir = std::env::temp_dir().join(format!("ocy-soak-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let error_log = ErrorLog::new(&temp_dir, "ocy");

    let mut settings = OcySettings::default();
    build_soak_settings(&mut settings);

    let offset_clock = Arc::new(OffsetClock::new(0));
    let clock: Arc<dyn Clock> = Arc::clone(&offset_clock) as Arc<dyn Clock>;
    let platform: Arc<dyn PlatformServices> = Arc::new(Unsupported);
    let strings = Arc::new(Strings::from_embedded().expect("embedded strings"));

    let engine = BreakEngine::new(
        settings.clone(),
        Arc::clone(&platform),
        Arc::clone(&clock),
        None,
        None,
        None,
        None,
    );
    let snapshot = engine.snapshot();
    let tray_presenter = TrayPresenter::new(Arc::clone(&strings), Arc::clone(&clock));
    let tray_controller = TrayController::new(
        tray_presenter,
        DummyTraySink,
        Rc::new(StaticTaskbarTheme),
        Arc::clone(&clock),
        Some(AppTheme::Light),
        &snapshot,
    );

    type TestHost =
        AppHost<OverlayBreakWindows<SlintBreakWindows>, DummySettingsWindow, DummyTraySink>;
    let host_slot: Rc<RefCell<Option<Weak<RefCell<TestHost>>>>> = Rc::new(RefCell::new(None));

    let make_cb = |slot: &Rc<RefCell<Option<Weak<RefCell<TestHost>>>>>,
                   f: fn(&mut TestHost)|
     -> Rc<dyn Fn()> {
        let slot = Rc::clone(slot);
        Rc::new(move || {
            if let Some(weak) = slot.borrow().as_ref()
                && let Some(rc) = weak.upgrade()
                && let Ok(mut host) = rc.try_borrow_mut()
            {
                f(&mut host);
            }
        })
    };

    let callbacks = BreakActionCallbacks {
        on_skip: make_cb(&host_slot, |h| {
            let _ = h.skip_current_break();
        }),
        on_start_long_break_now: make_cb(&host_slot, TestHost::start_long_break_now),
        on_postpone_long_break: make_cb(&host_slot, |h| {
            let _ = h.postpone_long_break();
        }),
    };

    let slint_break_windows = SlintBreakWindows::new(
        Arc::clone(&platform),
        Arc::clone(&clock),
        Arc::clone(&strings),
        error_log.clone(),
        callbacks,
        settings,
        LatencyOptions::default(),
    );
    let break_windows = OverlayBreakWindows::with_clock(slint_break_windows, platform, clock);
    let coordinator = BreakUiCoordinator::new(break_windows, DummySettingsWindow);
    let host = Rc::new(RefCell::new(AppHost::new(
        engine,
        coordinator,
        tray_controller,
        error_log,
    )));
    *host_slot.borrow_mut() = Some(Rc::downgrade(&host));

    let samples: Rc<RefCell<Vec<SoakCycleSample>>> = Rc::new(RefCell::new(Vec::new()));
    let summary_out: Rc<RefCell<Option<SoakSummary>>> = Rc::new(RefCell::new(None));

    let samples_cb = Rc::clone(&samples);
    let summary_cb = Rc::clone(&summary_out);
    let clock_cb = Arc::clone(&offset_clock);

    // Durations > 380 ms ensure damt-art stepped animation timers fire during short/long breaks
    // while keeping the headless test fast (~3 s total).
    let timings = SoakTimings {
        prewarm_lead: Duration::from_millis(20),
        short_animate: Duration::from_millis(450),
        warning_show: Duration::from_millis(120),
        long_show: Duration::from_millis(450),
        settle: Duration::from_millis(100),
    };

    start_soak_probe(
        Rc::downgrade(&host),
        timings,
        3,
        SoakHooks {
            advance_clock_secs: Rc::new(move |secs| clock_cb.advance_secs(secs)),
            on_schedule_changed: Rc::new(|| {}),
            sample_memory: Rc::new(|| (1024, 2048)),
            on_cycle: Rc::new(move |sample| {
                samples_cb.borrow_mut().push(sample);
            }),
            on_done: Rc::new(move |summary| {
                *summary_cb.borrow_mut() = Some(summary);
                let _ = slint::quit_event_loop();
            }),
        },
    );

    slint::run_event_loop_until_quit().expect("headless event loop");

    let recorded = samples.borrow();
    assert_eq!(recorded.len(), 3, "expected 3 completed soak cycles");
    for sample in recorded.iter() {
        assert_eq!(
            sample.live_break_windows, 0,
            "cycle {} leaked break windows",
            sample.cycle
        );
        assert_eq!(
            sample.art_cache_entries, 0,
            "cycle {} leaked art cache entries",
            sample.cycle
        );
        assert_eq!(sample.overlays, 0, "cycle {} leaked overlays", sample.cycle);
    }
    let summary = summary_out.borrow().expect("soak summary");
    assert_eq!(summary.cycles, 3);

    host.borrow_mut().shutdown();
    *host_slot.borrow_mut() = None;
    drop(host);
    let _ = std::fs::remove_dir_all(&temp_dir);
}
