//! Win32 layered-window implementation. Compiled only on Windows.

use std::ffi::c_void;
use std::sync::OnceLock;

use damt_platform::ScreenRect;
use windows::Win32::Foundation::{
    COLORREF, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, TRUE,
};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, ReleaseDC,
    SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect,
    MONITORINFOF_PRIMARY, RegisterClassW, SW_SHOWNOACTIVATE, ShowWindow, ULW_ALPHA,
    UpdateLayeredWindow, WINDOW_EX_STYLE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, w};

use crate::pure::{fill_border, fill_solid, premultiplied_argb, scaled_length};
use crate::{MonitorId, MonitorInfo, Rgba, WindowInfo};

fn class_name() -> PCWSTR {
    w!("DamtOverlayWindow")
}

static CLASS_REGISTERED: OnceLock<bool> = OnceLock::new();

/// Enumerate monitors with `EnumDisplayMonitors` and effective DPI.
pub(crate) fn enumerate_monitors() -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();

    // SAFETY: the callback receives a pointer to `monitors`, which outlives the enumeration, and pushes
    // only owned values into it.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_callback),
            LPARAM(&mut monitors as *mut Vec<MonitorInfo> as isize),
        );
    }

    monitors
}

unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _hdc: HDC,
    _clip: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the pointer to the caller's Vec, valid for the duration of EnumDisplayMonitors.
    let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorInfo>) };

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    // SAFETY: `monitor` comes from EnumDisplayMonitors and `info.cbSize` is initialized.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return TRUE;
    }

    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    // SAFETY: `monitor` is valid and both out-pointers are aligned stack locals.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };

    monitors.push(MonitorInfo {
        id: MonitorId::new(monitor.0 as isize),
        rect: rect_from(info.rcMonitor),
        work_area: rect_from(info.rcWork),
        dpi: if dpi_x == 0 { 96 } else { dpi_x },
        primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
    });

    TRUE
}

fn rect_from(rect: RECT) -> ScreenRect {
    ScreenRect {
        x: rect.left,
        y: rect.top,
        width: rect.right.wrapping_sub(rect.left),
        height: rect.bottom.wrapping_sub(rect.top),
    }
}

/// A created popup window that destroys itself on drop.
pub(crate) struct NativeWindow {
    hwnd: HWND,
}

impl NativeWindow {
    fn show(&self) {
        // SAFETY: `hwnd` is a live window created by this module; this shows it without activating.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    fn info(&self) -> WindowInfo {
        let mut rect = RECT::default();
        // SAFETY: `hwnd` is live and `rect` is an aligned out-parameter.
        let rect = if unsafe { GetWindowRect(self.hwnd, &mut rect) }.is_ok() {
            rect_from(rect)
        } else {
            ScreenRect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            }
        };

        // SAFETY: `hwnd` is live and GWL_EXSTYLE is a read-only query.
        let ex_style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) } as u32;

        WindowInfo {
            handle: self.hwnd.0 as isize,
            rect,
            ex_style,
        }
    }
}

impl std::fmt::Debug for NativeWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeWindow")
            .field("handle", &(self.hwnd.0 as isize))
            .finish()
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        // SAFETY: this module owns the window and destroys it exactly once.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Minimal window procedure: every message goes to `DefWindowProcW`.
unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: this class has no custom message handling; DefWindowProcW is the documented default.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn ensure_class() -> bool {
    *CLASS_REGISTERED.get_or_init(|| {
        // SAFETY: the module handle and the fully initialized WNDCLASSW are valid for RegisterClassW.
        unsafe {
            let hinstance = match GetModuleHandleW(None) {
                Ok(handle) => handle,
                Err(_) => return false,
            };

            let class = WNDCLASSW {
                lpfnWndProc: Some(overlay_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name(),
                ..Default::default()
            };

            let atom = RegisterClassW(&class);
            atom != 0 || GetLastError() == windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS
        }
    })
}

fn create_window(rect: ScreenRect, ex_style: WINDOW_EX_STYLE) -> Option<NativeWindow> {
    if rect.width <= 0 || rect.height <= 0 || !ensure_class() {
        return None;
    }

    // SAFETY: the class is registered; the popup has no parent, and every argument is a plain value or a
    // module handle owned by the process.
    let hwnd = unsafe {
        let hinstance = GetModuleHandleW(None).ok()?;
        CreateWindowExW(
            ex_style,
            class_name(),
            PCWSTR::null(),
            WS_POPUP,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .ok()?
    };

    Some(NativeWindow { hwnd })
}

/// Copy `pixels` into a 32-bit premultiplied DIB and hand it to `UpdateLayeredWindow`.
fn blit(window: &NativeWindow, rect: ScreenRect, width: i32, height: i32, pixels: &[u32]) -> bool {
    let len = match pixel_len(width, height) {
        Some(len) if pixels.len() >= len => len,
        _ => return false,
    };

    // SAFETY: GetDC returns a DC for the whole screen; it is released before returning.
    let screen = unsafe { GetDC(None) };
    if screen.is_invalid() {
        return false;
    }

    // SAFETY: `screen` is a valid DC returned by GetDC.
    let memory = unsafe { CreateCompatibleDC(Some(screen)) };
    if memory.is_invalid() {
        // SAFETY: `screen` came from GetDC and is no longer used.
        unsafe {
            ReleaseDC(None, screen);
        }
        return false;
    }

    let header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: `memory` is valid, `header` is initialized, and `bits` is an aligned out-parameter.
    let bitmap = match unsafe {
        CreateDIBSection(Some(memory), &header, DIB_RGB_COLORS, &mut bits, None, 0)
    } {
        Ok(bitmap) => bitmap,
        Err(_) => {
            // SAFETY: both handles were created above and are no longer used.
            unsafe {
                let _ = DeleteDC(memory);
                ReleaseDC(None, screen);
            }
            return false;
        }
    };

    if bits.is_null() {
        // SAFETY: both handles were created above and are no longer used.
        unsafe {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
        }
        return false;
    }

    // SAFETY: CreateDIBSection allocated `len` 32-bit pixels and `pixels` has at least that many; all
    // handles stay valid until the end of the block, and the DIB is deselected before it is deleted.
    unsafe {
        let destination = std::slice::from_raw_parts_mut(bits.cast::<u32>(), len);
        destination.copy_from_slice(&pixels[..len]);

        let previous = SelectObject(memory, bitmap.into());
        let destination_point = POINT {
            x: rect.x,
            y: rect.y,
        };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let source_point = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        let result = UpdateLayeredWindow(
            window.hwnd,
            Some(screen),
            Some(&destination_point),
            Some(&size),
            Some(memory),
            Some(&source_point),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        result.is_ok()
    }
}

fn pixel_len(width: i32, height: i32) -> Option<usize> {
    if width <= 0 || height <= 0 {
        return None;
    }

    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)
}

/// One click-through border window.
pub(crate) struct FrameWindow {
    window: NativeWindow,
    rect: ScreenRect,
    thickness: i32,
}

impl std::fmt::Debug for FrameWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameWindow")
            .field("window", &self.window)
            .field("rect", &self.rect)
            .field("thickness", &self.thickness)
            .finish()
    }
}

impl FrameWindow {
    pub(crate) fn new(monitor: &MonitorInfo, color: Rgba, thickness_dip: u32) -> Option<Self> {
        let ex_style =
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
        let window = create_window(monitor.rect, ex_style)?;
        let frame = Self {
            window,
            rect: monitor.rect,
            thickness: scaled_length(thickness_dip, monitor.dpi),
        };

        if !frame.apply_color(color) {
            return None;
        }

        frame.window.show();
        Some(frame)
    }

    pub(crate) fn apply_color(&self, color: Rgba) -> bool {
        let Some(len) = pixel_len(self.rect.width, self.rect.height) else {
            return false;
        };

        let mut pixels = vec![0u32; len];
        fill_border(
            &mut pixels,
            self.rect.width,
            self.rect.height,
            self.thickness,
            premultiplied_argb(color),
        );
        blit(
            &self.window,
            self.rect,
            self.rect.width,
            self.rect.height,
            &pixels,
        )
    }

    pub(crate) fn info(&self) -> WindowInfo {
        self.window.info()
    }
}

/// One input-blocking dim window.
pub(crate) struct DimWindow {
    window: NativeWindow,
    rect: ScreenRect,
}

impl std::fmt::Debug for DimWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DimWindow")
            .field("window", &self.window)
            .field("rect", &self.rect)
            .finish()
    }
}

impl DimWindow {
    pub(crate) fn new(monitor: &MonitorInfo, color: Rgba) -> Option<Self> {
        let ex_style = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
        let window = create_window(monitor.rect, ex_style)?;
        let len = pixel_len(monitor.rect.width, monitor.rect.height)?;
        let mut pixels = vec![0u32; len];
        fill_solid(&mut pixels, premultiplied_argb(color));

        if !blit(
            &window,
            monitor.rect,
            monitor.rect.width,
            monitor.rect.height,
            &pixels,
        ) {
            return None;
        }

        window.show();
        Some(Self {
            window,
            rect: monitor.rect,
        })
    }

    pub(crate) fn info(&self) -> WindowInfo {
        self.window.info()
    }
}
