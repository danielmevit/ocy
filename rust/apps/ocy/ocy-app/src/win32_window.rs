//! Win32 window role attributes, hidden owner window, and never-activate subclassing for break cards.

use std::cell::Cell;
use std::fmt;

use slint::winit_030::winit;

const WS_EX_TOOLWINDOW_BITS: usize = 0x0000_0080;
const WS_EX_APPWINDOW_BITS: usize = 0x0004_0000;
const WS_EX_NOACTIVATE_BITS: usize = 0x0800_0000;

/// Role of the next Slint/winit window constructed on the current thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowRole {
    /// Standard application window (e.g., Settings): leaves winit attributes untouched.
    #[default]
    Normal,
    /// Break or warning card: non-activating, always-on-top, owned by the hidden owner HWND.
    BreakCard,
}

thread_local! {
    static NEXT_WINDOW_ROLE: Cell<WindowRole> = const { Cell::new(WindowRole::Normal) };
}

/// Returns the current thread-local [`WindowRole`].
#[must_use]
pub fn current_window_role() -> WindowRole {
    NEXT_WINDOW_ROLE.with(Cell::get)
}

/// RAII guard that sets [`WindowRole`] for the current scope and restores the previous role on drop.
#[derive(Debug)]
pub struct WindowRoleGuard {
    previous: WindowRole,
}

impl WindowRoleGuard {
    /// Sets the thread-local window role to `role` until this guard is dropped.
    #[must_use]
    pub fn new(role: WindowRole) -> Self {
        let previous = NEXT_WINDOW_ROLE.with(|cell| cell.replace(role));
        Self { previous }
    }
}

impl Drop for WindowRoleGuard {
    fn drop(&mut self) {
        NEXT_WINDOW_ROLE.with(|cell| cell.set(self.previous));
    }
}

/// Applies winit [`WindowAttributes`](winit::window::WindowAttributes) appropriate for the current
/// thread-local [`WindowRole`].
#[must_use]
pub fn apply_window_attributes_for_role(
    mut attrs: winit::window::WindowAttributes,
    owner_hwnd: Option<isize>,
) -> winit::window::WindowAttributes {
    match current_window_role() {
        WindowRole::Normal => {
            let _ = owner_hwnd;
            attrs
        }
        WindowRole::BreakCard => {
            attrs = attrs
                .with_active(false)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
            #[cfg(windows)]
            if let Some(owner) = owner_hwnd {
                use winit::platform::windows::WindowAttributesExtWindows;
                attrs = attrs.with_owner_window(owner);
            }
            #[cfg(not(windows))]
            let _ = owner_hwnd;
            attrs
        }
    }
}

/// Computes the target `GWL_EXSTYLE` for break/warning windows:
/// keeps unrelated bits, adds `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, and strips `WS_EX_APPWINDOW`.
#[must_use]
pub const fn target_ex_style(current_ex_style: usize) -> usize {
    (current_ex_style & !WS_EX_APPWINDOW_BITS) | WS_EX_TOOLWINDOW_BITS | WS_EX_NOACTIVATE_BITS
}

/// Backwards-compatible alias for [`target_ex_style`].
#[must_use]
pub const fn break_window_ex_style(current_ex_style: usize) -> usize {
    target_ex_style(current_ex_style)
}

/// Close requests (such as Alt+F4) are refused while a break/warning is active.
#[must_use]
pub const fn should_allow_close(break_active: bool) -> bool {
    !break_active
}

/// Hidden top-level owner window (`WS_POPUP`, 0×0, never shown) used as the Win32 owner for break
/// card windows so winit omits `WS_EX_APPWINDOW` and Windows keeps them off the taskbar and Alt+Tab.
pub struct OwnerWindow {
    #[cfg(windows)]
    hwnd: windows::Win32::Foundation::HWND,
}

impl fmt::Debug for OwnerWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(windows)]
        {
            f.debug_struct("OwnerWindow")
                .field("hwnd", &(self.hwnd.0 as isize))
                .finish()
        }
        #[cfg(not(windows))]
        {
            f.debug_struct("OwnerWindow").finish()
        }
    }
}

impl OwnerWindow {
    /// Creates the hidden owner window once at startup (no-op stub on non-Windows).
    pub fn create() -> Result<Self, String> {
        #[cfg(windows)]
        {
            windows_impl::create_owner_window().map(|hwnd| Self { hwnd })
        }
        #[cfg(not(windows))]
        {
            Ok(Self {})
        }
    }

    /// Returns the raw Win32 `HWND` value for `WindowAttributesExtWindows::with_owner_window`.
    #[must_use]
    pub fn raw_hwnd(&self) -> Option<isize> {
        #[cfg(windows)]
        {
            Some(self.hwnd.0 as isize)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}

impl Drop for OwnerWindow {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            // SAFETY: `self.hwnd` was created by `OwnerWindow::create` on the main thread and is
            // destroyed exactly once on application exit.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd);
            }
        }
    }
}

/// Installs the never-activate / toolwindow subclass once and applies `GWL_EXSTYLE` + topmost
/// `SetWindowPos` once the underlying winit window and Win32 `HWND` exist.
pub fn install_break_window_subclass_and_style(window: &slint::Window) -> bool {
    #[cfg(windows)]
    {
        windows_impl::install_break_window_subclass_and_style(window)
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        false
    }
}

/// Announces `text` to screen readers (Narrator) for a break card that just appeared, without
/// focusing it: raises a UI Automation notification event on the card's window. The cards are
/// never-activate tool windows, so Narrator does not move to them on its own; the full card stays
/// readable through AccessKit (Narrator scan mode / UIA tree) once a client queries it. Returns
/// `true` when the event was raised (or no UIA client is listening); `false` before the `HWND`
/// exists, so the caller can retry on the next window event.
pub fn announce_to_screen_reader(window: &slint::Window, text: &str) -> bool {
    #[cfg(windows)]
    {
        windows_impl::announce_to_screen_reader(window, text)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, text);
        true
    }
}

const BRAND_SPROUT_LIGHT_16: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-light-16.png");
const BRAND_SPROUT_LIGHT_20: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-light-20.png");
const BRAND_SPROUT_LIGHT_24: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-light-24.png");
const BRAND_SPROUT_LIGHT_32: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-light-32.png");
const BRAND_SPROUT_LIGHT_48: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-light-48.png");
const BRAND_SPROUT_LIGHT_64: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-light-64.png");

const BRAND_SPROUT_DARK_16: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-dark-16.png");
const BRAND_SPROUT_DARK_20: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-dark-20.png");
const BRAND_SPROUT_DARK_24: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-dark-24.png");
const BRAND_SPROUT_DARK_32: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-dark-32.png");
const BRAND_SPROUT_DARK_48: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-dark-48.png");
const BRAND_SPROUT_DARK_64: &[u8] =
    include_bytes!("../../../../../assets/icons/brand/png/ocy-sprout-dark-64.png");

/// Icon source chosen for a single Win32 window icon slot (`ICON_SMALL` or `ICON_BIG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowIconSource {
    /// Build an owned `HICON` at runtime from the embedded brand sprout PNG of the matching theme.
    BrandSprout {
        /// Embedded PNG bytes (smallest available asset `>= target_size_px`, or `64`).
        png_bytes: &'static [u8],
        /// Nominal asset dimension in pixels (`16`, `20`, `24`, `32`, `48`, or `64`).
        asset_size_px: i32,
        /// Exact target pixel dimension (`SM_CXSMICON` / `SM_CXICON` for the window's DPI).
        target_size_px: i32,
        /// Whether the light-taskbar variant was selected.
        use_light_set: bool,
    },
    /// Load the shared face icon from executable resource `1` (`LR_SHARED`).
    ExeResource {
        /// Exact target pixel dimension (`>= 40`).
        target_size_px: i32,
    },
}

/// Returns the embedded brand sprout PNG bytes and nominal asset size for `target_size_px`
/// (smallest available size `>= target_size_px` from `16, 20, 24, 32, 48, 64`).
#[must_use]
pub const fn brand_sprout_png_for_size(
    target_size_px: i32,
    use_light_set: bool,
) -> (i32, &'static [u8]) {
    let bucket = if target_size_px <= 16 {
        16
    } else if target_size_px <= 20 {
        20
    } else if target_size_px <= 24 {
        24
    } else if target_size_px <= 32 {
        32
    } else if target_size_px <= 48 {
        48
    } else {
        64
    };
    let bytes = match (use_light_set, bucket) {
        (true, 16) => BRAND_SPROUT_LIGHT_16,
        (true, 20) => BRAND_SPROUT_LIGHT_20,
        (true, 24) => BRAND_SPROUT_LIGHT_24,
        (true, 32) => BRAND_SPROUT_LIGHT_32,
        (true, 48) => BRAND_SPROUT_LIGHT_48,
        (true, _) => BRAND_SPROUT_LIGHT_64,
        (false, 16) => BRAND_SPROUT_DARK_16,
        (false, 20) => BRAND_SPROUT_DARK_20,
        (false, 24) => BRAND_SPROUT_DARK_24,
        (false, 32) => BRAND_SPROUT_DARK_32,
        (false, 48) => BRAND_SPROUT_DARK_48,
        (false, _) => BRAND_SPROUT_DARK_64,
    };
    (bucket, bytes)
}

/// Chooses the window icon source for `target_size_px`:
/// sizes `< 40` px (all `<= 32` px standard sizes plus intermediate fractional DPIs) use the
/// theme-matched brand sprout PNG; sizes `>= 40` px keep executable resource `1` (the face).
#[must_use]
pub const fn select_window_icon_source(
    target_size_px: i32,
    use_light_set: bool,
) -> WindowIconSource {
    if target_size_px >= 40 {
        WindowIconSource::ExeResource { target_size_px }
    } else {
        let (asset_size_px, png_bytes) = brand_sprout_png_for_size(target_size_px, use_light_set);
        WindowIconSource::BrandSprout {
            png_bytes,
            asset_size_px,
            target_size_px,
            use_light_set,
        }
    }
}

/// Applies DPI-appropriate `ICON_SMALL` and `ICON_BIG` icons to `window`'s Win32 `HWND`,
/// choosing the brand sprout PNG variant from `use_light_set` for sizes `< 40` px (`<= 32` px)
/// and executable resource `1` for sizes `>= 40` px. Owned `HICON`s are destroyed on replacement
/// and on `WM_NCDESTROY`. Returns `true` once a real `HWND` was inspected.
pub fn apply_window_icon(window: &slint::Window, use_light_set: bool) -> bool {
    #[cfg(windows)]
    {
        windows_impl::apply_window_icon(window, use_light_set)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, use_light_set);
        true
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::winit_030::WinitWindowAccessor;
    use windows::Win32::Foundation::{
        ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIconFromResourceEx, CreateWindowExW, DefWindowProcW, DestroyIcon, GWL_EXSTYLE,
        GetSystemMetrics, GetWindowLongPtrW, GetWindowThreadProcessId, HICON, HWND_TOPMOST,
        ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LR_SHARED, LoadImageW, MA_NOACTIVATE,
        RegisterClassW, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, STYLESTRUCT,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW, SetWindowLongPtrW,
        SetWindowPos, WINDOW_EX_STYLE, WINDOWPOS, WM_DPICHANGED, WM_MOUSEACTIVATE, WM_NCDESTROY,
        WM_SETICON, WM_STYLECHANGING, WM_WINDOWPOSCHANGING, WNDCLASSW, WS_POPUP,
    };
    use windows::core::{PCWSTR, w};

    use super::{WindowIconSource, select_window_icon_source, target_ex_style};

    const OWNER_CLASS_NAME: PCWSTR = w!("OcyHiddenOwnerWindow");
    const BREAK_WINDOW_SUBCLASS_ID: usize = 0x4F43_5931;
    const NORMAL_WINDOW_ICON_SUBCLASS_ID: usize = 0x4F43_5932;
    const APP_ICON_RESOURCE_ID: PCWSTR = PCWSTR(std::ptr::without_provenance(1));
    const ICON_RESOURCE_FORMAT_VER: u32 = 0x0003_0000;

    #[derive(Default)]
    struct OwnedWindowIcons {
        small: Option<HICON>,
        big: Option<HICON>,
        last_applied: Option<(bool, i32, i32)>,
    }

    thread_local! {
        static OWNED_WINDOW_ICONS: RefCell<HashMap<isize, OwnedWindowIcons>> =
            RefCell::new(HashMap::new());
    }

    static OWNER_CLASS_REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();

    unsafe extern "system" fn owner_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: The hidden owner window has no custom message handling; `DefWindowProcW` is the
        // documented default window procedure.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn ensure_owner_class() -> Result<(), String> {
        OWNER_CLASS_REGISTERED
            .get_or_init(|| {
                // SAFETY: `GetModuleHandleW(None)` returns the current process module handle and
                // `WNDCLASSW` references a static class name and `owner_wnd_proc`.
                unsafe {
                    let hinstance = GetModuleHandleW(None)
                        .map_err(|err| format!("GetModuleHandleW failed: {err}"))?;
                    let class = WNDCLASSW {
                        lpfnWndProc: Some(owner_wnd_proc),
                        hInstance: hinstance.into(),
                        lpszClassName: OWNER_CLASS_NAME,
                        ..Default::default()
                    };
                    let atom = RegisterClassW(&class);
                    if atom != 0 || GetLastError() == ERROR_CLASS_ALREADY_EXISTS {
                        Ok(())
                    } else {
                        Err(format!("RegisterClassW failed: {:?}", GetLastError()))
                    }
                }
            })
            .clone()
    }

    pub(super) fn create_owner_window() -> Result<HWND, String> {
        ensure_owner_class()?;
        // SAFETY: `OWNER_CLASS_NAME` is registered above; `WS_POPUP` with no parent creates an
        // unshown 0x0 top-level owner window owned by the current process.
        unsafe {
            let hinstance =
                GetModuleHandleW(None).map_err(|err| format!("GetModuleHandleW failed: {err}"))?;
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                OWNER_CLASS_NAME,
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
            .map_err(|err| format!("CreateWindowExW owner failed: {err}"))
        }
    }

    unsafe extern "system" fn break_window_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        uid_subclass: usize,
        _ref_data: usize,
    ) -> LRESULT {
        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_STYLECHANGING => {
                if wparam.0 as i32 == GWL_EXSTYLE.0 && lparam.0 != 0 {
                    // SAFETY: For `WM_STYLECHANGING`, Win32 supplies a valid mutable `STYLESTRUCT`
                    // pointer in `lparam` for the duration of the message dispatch.
                    let style_struct = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
                    style_struct.styleNew = target_ex_style(style_struct.styleNew as usize) as u32;
                }
                // SAFETY: Forwards the message (with updated `styleNew` for `GWL_EXSTYLE`) to the
                // next window procedure in the subclass chain.
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_WINDOWPOSCHANGING => {
                if lparam.0 != 0 {
                    // SAFETY: For `WM_WINDOWPOSCHANGING`, Win32 supplies a valid mutable `WINDOWPOS`
                    // pointer in `lparam` for the duration of the message dispatch.
                    let window_pos = unsafe { &mut *(lparam.0 as *mut WINDOWPOS) };
                    window_pos.flags |= SWP_NOACTIVATE;
                }
                // SAFETY: Forwards the message with `SWP_NOACTIVATE` enforced to the next proc.
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_NCDESTROY => {
                // SAFETY: `WM_NCDESTROY` is the final message for `hwnd`; removing our subclass
                // before delegating ensures no further subclass calls occur.
                unsafe {
                    let _ =
                        RemoveWindowSubclass(hwnd, Some(break_window_subclass_proc), uid_subclass);
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
            }
            _ => {
                // SAFETY: Forwards unhandled messages unchanged to the next proc in the chain.
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
        }
    }

    pub(super) fn announce_to_screen_reader(window: &slint::Window, text: &str) -> bool {
        use windows::Win32::UI::Accessibility::{
            NotificationKind_Other, NotificationProcessing_ImportantMostRecent,
            UiaClientsAreListening, UiaHostProviderFromHwnd, UiaRaiseNotificationEvent,
        };
        use windows::core::BSTR;

        let Some(hwnd) = extract_win32_hwnd(window) else {
            return false;
        };
        if text.is_empty() {
            return true;
        }
        // SAFETY: `hwnd` is the live card window on this (its owning) thread; the provider is a
        // COM object owned by `provider` for the duration of the call, and the BSTRs outlive it.
        unsafe {
            if !UiaClientsAreListening().as_bool() {
                return true;
            }
            if let Ok(provider) = UiaHostProviderFromHwnd(hwnd) {
                let _ = UiaRaiseNotificationEvent(
                    &provider,
                    NotificationKind_Other,
                    NotificationProcessing_ImportantMostRecent,
                    &BSTR::from(text),
                    &BSTR::from("ocy-break-card"),
                );
            }
        }
        true
    }

    fn extract_win32_hwnd(window: &slint::Window) -> Option<HWND> {
        window
            .with_winit_window(|winit_win| {
                let raw = winit_win.window_handle().ok()?;
                match raw.as_raw() {
                    RawWindowHandle::Win32(win32) => Some(HWND(win32.hwnd.get() as *mut c_void)),
                    _ => None,
                }
            })
            .flatten()
    }

    pub(super) fn install_break_window_subclass_and_style(window: &slint::Window) -> bool {
        let Some(hwnd) = extract_win32_hwnd(window) else {
            return false;
        };

        // SAFETY: `GetWindowThreadProcessId` and `GetCurrentThreadId` have no preconditions;
        // `SetWindowSubclass` must run on the HWND's owning thread, which is verified first.
        // Applying `GWL_EXSTYLE` and `SetWindowPos(..., SWP_NOACTIVATE | ...)` updates the live
        // frame without activating the window.
        unsafe {
            if GetWindowThreadProcessId(hwnd, None) != GetCurrentThreadId() {
                return false;
            }
            if !SetWindowSubclass(
                hwnd,
                Some(break_window_subclass_proc),
                BREAK_WINDOW_SUBCLASS_ID,
                0,
            )
            .as_bool()
            {
                return false;
            }
            let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as usize;
            let desired = target_ex_style(current);
            if desired != current {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired as isize);
            }
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
            );
        }

        true
    }

    fn destroy_owned_icon(icon: Option<HICON>) {
        if let Some(hicon) = icon
            && !hicon.is_invalid()
        {
            // SAFETY: `hicon` is an owned non-shared `HICON` created by
            // `CreateIconFromResourceEx` and is no longer referenced by any window (either
            // already replaced via `WM_SETICON` or cleaned up on `WM_NCDESTROY`).
            unsafe {
                let _ = DestroyIcon(hicon);
            }
        }
    }

    unsafe extern "system" fn window_icon_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        uid_subclass: usize,
        _ref_data: usize,
    ) -> LRESULT {
        match msg {
            WM_DPICHANGED => {
                let new_dpi = ((wparam.0 >> 16) & 0xFFFF) as u32;
                let use_light_set = OWNED_WINDOW_ICONS.with(|map| {
                    map.borrow()
                        .get(&(hwnd.0 as isize))
                        .and_then(|entry| entry.last_applied.map(|(light, _, _)| light))
                });
                if let Some(light) = use_light_set {
                    apply_icons_to_hwnd(hwnd, light, Some(new_dpi));
                }
                // SAFETY: Forwards `WM_DPICHANGED` to the next window procedure in the chain.
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_NCDESTROY => {
                let removed =
                    OWNED_WINDOW_ICONS.with(|map| map.borrow_mut().remove(&(hwnd.0 as isize)));
                // SAFETY: `WM_NCDESTROY` is the final message for `hwnd`; removing our subclass and
                // forwarding `DefSubclassProc` finishes window teardown before we destroy any owned
                // `HICON`s that were assigned to `hwnd`.
                let res = unsafe {
                    let _ =
                        RemoveWindowSubclass(hwnd, Some(window_icon_subclass_proc), uid_subclass);
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                };
                if let Some(icons) = removed {
                    destroy_owned_icon(icons.small);
                    destroy_owned_icon(icons.big);
                }
                res
            }
            _ => {
                // SAFETY: Forwards unhandled messages unchanged to the next proc in the chain.
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
        }
    }

    fn apply_icon_slot(
        hwnd: HWND,
        hinstance: Option<HINSTANCE>,
        slot: u32,
        cx: i32,
        cy: i32,
        use_light_set: bool,
        owned_slot: &mut Option<HICON>,
    ) {
        let target_px = cx.max(cy);
        let source = select_window_icon_source(target_px, use_light_set);

        let mut new_owned: Option<HICON> = None;
        let mut handle_raw: isize = 0;

        if let WindowIconSource::BrandSprout { png_bytes, .. } = source {
            // SAFETY: `png_bytes` is a static valid PNG slice; `CreateIconFromResourceEx` with
            // `ficon = true` and `ICON_RESOURCE_FORMAT_VER` (`0x00030000`) creates an owned
            // `HICON` scaled to `(cx, cy)` that we track in `owned_slot` and destroy on
            // replacement or `WM_NCDESTROY`.
            if let Ok(hicon) = unsafe {
                CreateIconFromResourceEx(
                    png_bytes,
                    true,
                    ICON_RESOURCE_FORMAT_VER,
                    cx,
                    cy,
                    LR_DEFAULTCOLOR,
                )
            } && !hicon.is_invalid()
            {
                new_owned = Some(hicon);
                handle_raw = hicon.0 as isize;
            }
        }

        if handle_raw == 0
            && let Some(inst) = hinstance
        {
            // SAFETY: `LoadImageW` with `LR_SHARED` loads a process-shared icon handle from
            // executable resource `1` (which Windows destroys on module unload, so `DestroyIcon`
            // must not be called on it).
            if let Ok(shared) = unsafe {
                LoadImageW(
                    Some(inst),
                    APP_ICON_RESOURCE_ID,
                    IMAGE_ICON,
                    cx,
                    cy,
                    LR_SHARED,
                )
            } && !shared.0.is_null()
            {
                handle_raw = shared.0 as isize;
            }
        }

        if handle_raw == 0 {
            return;
        }

        // SAFETY: `hwnd` is a live Win32 window on the current thread and `handle_raw` is a valid
        // `HICON` for `slot` (`ICON_SMALL` or `ICON_BIG`). We swap the window's icon first before
        // destroying any previous owned `HICON` so the window never holds a destroyed handle.
        unsafe {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(slot as usize)),
                Some(LPARAM(handle_raw)),
            );
        }
        let previous = std::mem::replace(owned_slot, new_owned);
        destroy_owned_icon(previous);
    }

    fn apply_icons_to_hwnd(hwnd: HWND, use_light_set: bool, dpi_override: Option<u32>) {
        // SAFETY: `GetWindowThreadProcessId` and `GetCurrentThreadId` have no preconditions;
        // `SetWindowSubclass` runs only on the HWND's owning thread and is idempotent for a given
        // callback + `NORMAL_WINDOW_ICON_SUBCLASS_ID`. `GetDpiForWindow` and `GetSystemMetrics*`
        // are read-only metric queries on the live `hwnd`.
        let (small_cx, small_cy, big_cx, big_cy) = unsafe {
            if GetWindowThreadProcessId(hwnd, None) != GetCurrentThreadId() {
                return;
            }
            let _ = SetWindowSubclass(
                hwnd,
                Some(window_icon_subclass_proc),
                NORMAL_WINDOW_ICON_SUBCLASS_ID,
                0,
            );
            let dpi = dpi_override
                .filter(|&d| d != 0)
                .unwrap_or_else(|| GetDpiForWindow(hwnd));
            if dpi != 0 {
                (
                    GetSystemMetricsForDpi(SM_CXSMICON, dpi),
                    GetSystemMetricsForDpi(SM_CYSMICON, dpi),
                    GetSystemMetricsForDpi(SM_CXICON, dpi),
                    GetSystemMetricsForDpi(SM_CYICON, dpi),
                )
            } else {
                (
                    GetSystemMetrics(SM_CXSMICON),
                    GetSystemMetrics(SM_CYSMICON),
                    GetSystemMetrics(SM_CXICON),
                    GetSystemMetrics(SM_CYICON),
                )
            }
        };

        let small_cx = if small_cx > 0 { small_cx } else { 16 };
        let small_cy = if small_cy > 0 { small_cy } else { 16 };
        let big_cx = if big_cx > 0 { big_cx } else { 32 };
        let big_cy = if big_cy > 0 { big_cy } else { 32 };
        let key = (use_light_set, small_cx.max(small_cy), big_cx.max(big_cy));

        let already_applied = OWNED_WINDOW_ICONS.with(|map| {
            map.borrow()
                .get(&(hwnd.0 as isize))
                .is_some_and(|entry| entry.last_applied == Some(key))
        });
        if already_applied {
            return;
        }

        // SAFETY: `GetModuleHandleW(None)` returns the handle of the running executable.
        let hinstance = unsafe { GetModuleHandleW(None) }.ok().map(Into::into);

        // Take existing owned handles out while calling `SendMessageW(WM_SETICON, ...)` so we
        // never hold a `RefCell` borrow across a Win32 `SendMessageW` call.
        let mut entry = OWNED_WINDOW_ICONS
            .with(|map| map.borrow_mut().remove(&(hwnd.0 as isize)))
            .unwrap_or_default();

        apply_icon_slot(
            hwnd,
            hinstance,
            ICON_SMALL,
            small_cx,
            small_cy,
            use_light_set,
            &mut entry.small,
        );
        apply_icon_slot(
            hwnd,
            hinstance,
            ICON_BIG,
            big_cx,
            big_cy,
            use_light_set,
            &mut entry.big,
        );
        entry.last_applied = Some(key);

        OWNED_WINDOW_ICONS.with(|map| {
            map.borrow_mut().insert(hwnd.0 as isize, entry);
        });
    }

    pub(super) fn apply_window_icon(window: &slint::Window, use_light_set: bool) -> bool {
        let Some(hwnd) = extract_win32_hwnd(window) else {
            return false;
        };
        apply_icons_to_hwnd(hwnd, use_light_set, None);
        true
    }
}
