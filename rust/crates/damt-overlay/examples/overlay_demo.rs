//! Windows smoke demos for the native overlays.
//!
//! Shows green edge frames on every non-primary monitor for 4 s, switches them to clay, then shows dims on
//! every monitor for 3 s. Prints each window rect against its monitor rect and the extended style bits.
//! The lead runs this on the owner's PC.

#[cfg(not(windows))]
fn main() {
    eprintln!("damt-overlay overlay_demo: this example runs on Windows only.");
}

#[cfg(windows)]
fn main() {
    use damt_overlay::{Dims, EdgeFrames, MonitorInfo, Rgba, enumerate_monitors};

    let monitors = enumerate_monitors();
    println!("monitors: {}", monitors.len());
    for monitor in &monitors {
        println!(
            "  id={} rect=[{},{} {}x{}] work=[{},{} {}x{}] dpi={} primary={}",
            monitor.id.raw(),
            monitor.rect.x,
            monitor.rect.y,
            monitor.rect.width,
            monitor.rect.height,
            monitor.work_area.x,
            monitor.work_area.y,
            monitor.work_area.width,
            monitor.work_area.height,
            monitor.dpi,
            monitor.primary,
        );
    }

    if monitors.is_empty() {
        println!("no monitors reported");
        return;
    }

    let green = Rgba::from_hex_rgb("#2F7D62", 140).unwrap_or(Rgba::rgb(0x2F, 0x7D, 0x62));
    let clay = Rgba::from_hex_rgb("#B04E2C", 140).unwrap_or(Rgba::rgb(0xB0, 0x4E, 0x2C));
    let dim = Rgba::new(0, 0, 0, 199);

    let secondary: Vec<MonitorInfo> = monitors
        .iter()
        .copied()
        .filter(|monitor| !monitor.primary)
        .collect();
    println!(
        "edge frames: green, {} non-primary monitor(s), 3 DIP",
        secondary.len()
    );

    let mut frames = EdgeFrames::show(&secondary, green, 3);
    print_windows("frames", &frames.window_info(), &monitors);
    wait(4_000);

    frames.set_color(clay);
    println!("edge frames: clay");
    wait(2_000);

    let dims = Dims::show(&monitors, dim);
    print_windows("dims", &dims.window_info(), &monitors);
    wait(3_000);

    drop(dims);
    drop(frames);
    println!("overlays destroyed");
}

#[cfg(windows)]
fn print_windows(
    label: &str,
    windows: &[damt_overlay::WindowInfo],
    monitors: &[damt_overlay::MonitorInfo],
) {
    if windows.is_empty() {
        println!("  {label}: no windows created");
        return;
    }

    for window in windows {
        match monitors.iter().find(|monitor| monitor.rect == window.rect) {
            Some(monitor) => println!(
                "  {label}: hwnd=0x{:X} rect=[{},{} {}x{}] monitor_rect=[{},{} {}x{}] equal={} exstyle=0x{:08X}",
                window.handle,
                window.rect.x,
                window.rect.y,
                window.rect.width,
                window.rect.height,
                monitor.rect.x,
                monitor.rect.y,
                monitor.rect.width,
                monitor.rect.height,
                window.rect == monitor.rect,
                window.ex_style,
            ),
            None => println!(
                "  {label}: hwnd=0x{:X} rect=[{},{} {}x{}] (no monitor rect matches) exstyle=0x{:08X}",
                window.handle,
                window.rect.x,
                window.rect.y,
                window.rect.width,
                window.rect.height,
                window.ex_style,
            ),
        }
    }
}

#[cfg(windows)]
fn wait(milliseconds: u64) {
    use std::time::{Duration, Instant};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
    };

    let deadline = Instant::now() + Duration::from_millis(milliseconds);
    let mut message = MSG::default();

    while Instant::now() < deadline {
        // SAFETY: `message` is an aligned out-parameter and the window class handles every message with
        // DefWindowProcW.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: PeekMessageW filled `message` with a valid message.
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}
