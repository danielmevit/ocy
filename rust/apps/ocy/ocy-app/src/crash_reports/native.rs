//! Windows native-crash capture: an unhandled-exception filter (`SetUnhandledExceptionFilter`)
//! that writes a raw record with **no heap allocation**, and a module table populated at
//! startup so frames can be named `module+offset`.
//!
//! Rust's abort path uses `__fastfail`, which bypasss exception filters, so this covers
//! access violations and stack overflows (and similar `EXCEPTION_*` faults), not Rust
//! panics — those go through the panic hook in [`super`]. A stack overflow still reaches
//! the filter because the filter re-arms `SetThreadStackGuarantee` on the crashing thread
//! (and the main thread sets a guarantee at startup).
//!
//! The filter appends one bounded line per crash to `crashes/native-raw.log`
//! (`C:0x<code> A:0x<addr> F:mod+0xoff;...`). The next start converts those lines into
//! normal reports ([`super::convert_native_raw`]) and truncates the file. Everything the
//! handler touches — the file handle, the record buffer, the module table — is allocated
//! once at startup into a frozen, leaked block; the handler only reads and writes it.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use windows::Win32::Foundation::{HANDLE, HMODULE};
use windows::Win32::Storage::FileSystem::{
    CREATE_ALWAYS, CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, WriteFile,
};
use windows::Win32::System::Diagnostics::Debug::{
    EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, RtlCaptureStackBackTrace,
    SetUnhandledExceptionFilter,
};
use windows::Win32::System::ProcessStatus::{
    EnumProcessModules, GetModuleFileNameExW, GetModuleInformation, MODULEINFO,
};
use windows::Win32::System::Threading::{GetCurrentProcess, SetThreadStackGuarantee};
use windows::core::PCWSTR;

use super::{CrashStore, MAX_NATIVE_RECORDS_PER_RUN, NATIVE_RAW_FILE_NAME};

/// Stack guaranteed for the handler on the crashing thread, in bytes.
const HANDLER_STACK_GUARANTEE: u32 = 32 * 1024;

/// Stack guarantee requested for the main thread at startup.
const STARTUP_STACK_GUARANTEE: u32 = 64 * 1024;

/// At most this many frames are captured per native crash.
const MAX_FRAMES: usize = 32;

/// At most this many modules are named at startup (the table is a fixed block).
const MAX_MODULES: usize = 64;

/// Longest module file name stored, in bytes.
const MAX_MODULE_NAME: usize = 64;

/// Longest raw record line, in bytes.
const MAX_RECORD: usize = 4096;

/// One named module in the frozen table.
#[repr(C)]
#[derive(Clone, Copy)]
struct ModuleEntry {
    base: u64,
    size: u64,
    name: [u8; MAX_MODULE_NAME],
    name_len: usize,
}

impl ModuleEntry {
    fn name(&self) -> &str {
        // ASCII-only by construction (file names of loaded modules); fall back to a
        // lossy slice if anything odd slipped in.
        std::str::from_utf8(&self.name[..self.name_len]).unwrap_or("unknown")
    }
}

/// The frozen handler state, allocated once at startup and never mutated after.
#[repr(C)]
struct NativeState {
    file: HANDLE,
    modules: [ModuleEntry; MAX_MODULES],
    module_count: usize,
    written: AtomicU32,
    reentry: AtomicBool,
    buffer: [u8; MAX_RECORD],
}

impl NativeState {
    fn empty() -> Self {
        Self {
            file: HANDLE(null_mut()),
            modules: [ModuleEntry {
                base: 0,
                size: 0,
                name: [0; MAX_MODULE_NAME],
                name_len: 0,
            }; MAX_MODULES],
            module_count: 0,
            written: AtomicU32::new(0),
            reentry: AtomicBool::new(false),
            buffer: [0; MAX_RECORD],
        }
    }
}

static STATE: AtomicPtr<NativeState> = AtomicPtr::new(null_mut());

/// Installs the native capture: startup stack guarantee, module table, pre-opened raw
/// record file, and the unhandled-exception filter. Never blocks and never propagates
/// errors; a partial install simply degrades to panic-only capture.
pub fn install(store: CrashStore) {
    // SAFETY: kernel32 startup-stack queries; both take a plain pointer to a stack value.
    unsafe {
        let mut guarantee = STARTUP_STACK_GUARANTEE;
        let _ = SetThreadStackGuarantee(&mut guarantee);
    }

    let mut state = Box::new(NativeState::empty());
    populate_module_table(&mut state);
    if let Some(file) = open_raw_file(&store.crashes_dir()) {
        state.file = file;
    }

    let ptr = Box::into_raw(state);
    STATE.store(ptr, Ordering::Release);
    // SAFETY: the filter is a plain top-level exception filter callback.
    unsafe {
        SetUnhandledExceptionFilter(Some(exception_filter));
    }
}

/// Opens (creating/truncating) the raw record file. Called after the previous start's
/// records were converted, so the truncate never loses data.
fn open_raw_file(crashes_dir: &Path) -> Option<HANDLE> {
    let _ = std::fs::create_dir_all(crashes_dir);
    let path: Vec<u16> = crashes_dir
        .join(NATIVE_RAW_FILE_NAME)
        .as_os_str()
        .to_owned()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: path is a valid NUL-terminated wide string; the handle is owned by the
    // frozen state.
    unsafe {
        let handle = CreateFileW(
            PCWSTR::from_raw(path.as_ptr()),
            FILE_APPEND_DATA.0,
            FILE_SHARE_READ,
            None,
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()?;
        Some(handle)
    }
}

/// Fills the module table (heap is fine here: this is startup, not the handler).
fn populate_module_table(state: &mut NativeState) {
    // SAFETY: GetCurrentProcess returns the pseudo-handle of this process; the arrays
    // are stack buffers of the documented shapes.
    unsafe {
        let process = GetCurrentProcess();
        let mut handles = [HMODULE::default(); MAX_MODULES];
        let mut needed = 0u32;
        if EnumProcessModules(
            process,
            handles.as_mut_ptr(),
            size_of::<HMODULE>() as u32 * MAX_MODULES as u32,
            &mut needed,
        )
        .is_err()
        {
            return;
        }
        let count = (needed as usize / size_of::<HMODULE>()).min(MAX_MODULES);
        for module in &handles[..count] {
            let mut info = MODULEINFO::default();
            if GetModuleInformation(process, *module, &mut info, size_of::<MODULEINFO>() as u32)
                .is_err()
            {
                continue;
            }
            let mut name_buf = [0u16; 260];
            let copied = GetModuleFileNameExW(Some(process), Some(*module), &mut name_buf);
            if copied == 0 {
                continue;
            }
            let path = String::from_utf16_lossy(&name_buf[..copied as usize]);
            let Some(file_name) = path.rsplit(['\\', '/']).next() else {
                continue;
            };
            let entry = &mut state.modules[state.module_count];
            entry.base = info.lpBaseOfDll as usize as u64;
            entry.size = info.SizeOfImage as u64;
            let bytes = file_name.as_bytes();
            entry.name_len = bytes.len().min(MAX_MODULE_NAME);
            entry.name[..entry.name_len].copy_from_slice(&bytes[..entry.name_len]);
            state.module_count += 1;
            if state.module_count == MAX_MODULES {
                break;
            }
        }
    }
}

/// Module name and base for a code address, from the frozen table.
#[must_use]
pub fn module_for_address(address: u64) -> Option<(String, u64)> {
    let state = STATE.load(Ordering::Acquire);
    // SAFETY: the pointer is either null or a leaked, frozen, fully initialized state.
    let state = unsafe { state.as_ref()? };
    state
        .modules
        .iter()
        .take(state.module_count)
        .find(|entry| address >= entry.base && address < entry.base.saturating_add(entry.size))
        .map(|entry| (entry.name().to_owned(), entry.base))
}

/// The unhandled-exception filter: re-arms the stack guarantee, guards re-entry, checks
/// consent, captures a bounded backtrace, and appends one raw record with no allocation.
unsafe extern "system" fn exception_filter(info: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: whole body is the no-alloc crash path described in the module docs.
    unsafe {
        let state = STATE.load(Ordering::Acquire);
        if state.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let state = &*state;

        // Stack overflow room for this thread's exception handling.
        let mut guarantee = HANDLER_STACK_GUARANTEE;
        let _ = SetThreadStackGuarantee(&mut guarantee);

        if state.reentry.swap(true, Ordering::AcqRel) {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let result = handle_exception(state, info);
        state.reentry.store(false, Ordering::Release);
        result
    }
}

/// The guarded body of the filter (still no heap).
unsafe fn handle_exception(state: &NativeState, info: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: the EXCEPTION_POINTERS passed by the OS are valid for the filter call.
    unsafe {
        if super::consent_is_never() {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        if state.written.load(Ordering::Acquire) >= MAX_NATIVE_RECORDS_PER_RUN {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let record = (*info).ExceptionRecord;
        let code = (*record).ExceptionCode.0 as u32;
        let address = (*record).ExceptionAddress as usize as u64;

        // Bounded capture; the stack array is inside the guaranteed stack.
        let mut frames = [null_mut::<core::ffi::c_void>(); MAX_FRAMES];
        let captured = RtlCaptureStackBackTrace(1, &mut frames, None) as usize;

        let mut len = 0usize;
        write_record(state, &mut len, code, address, &frames[..captured]);
        if len == 0 {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        state.written.fetch_add(1, Ordering::AcqRel);

        // Synchronous pre-opened handle; WriteFile with append access needs no seek.
        if WriteFile(state.file, Some(&state.buffer[..len]), None, None).is_err() {
            // Nothing else to do; the filter never propagates errors.
        }
        EXCEPTION_CONTINUE_SEARCH
    }
}

/// Builds `C:0x<code> A:0x<addr> F:<module>+0x<off>;<...>\n` in the pre-allocated buffer.
/// Bounded by construction: every part is hex or a fixed-size module name.
unsafe fn write_record(
    state: &NativeState,
    len: &mut usize,
    code: u32,
    address: u64,
    frames: &[*mut core::ffi::c_void],
) {
    // SAFETY: writes stay in bounds through push_bytes/push_hex; the buffer is frozen
    // handler-owned storage.
    unsafe {
        let buf = &state.buffer;
        push_bytes(buf, len, b"C:0x");
        push_hex(buf, len, code as u64);
        push_bytes(buf, len, b" A:0x");
        push_hex(buf, len, address);
        push_bytes(buf, len, b" F:");
        for (i, frame) in frames.iter().enumerate() {
            let frame = *frame as usize as u64;
            if frame == 0 {
                continue;
            }
            if i > 0 {
                push_bytes(buf, len, b";");
            }
            match state
                .modules
                .iter()
                .take(state.module_count)
                .find(|entry| frame >= entry.base && frame < entry.base.saturating_add(entry.size))
            {
                Some(entry) => {
                    push_bytes(buf, len, &entry.name[..entry.name_len]);
                    push_bytes(buf, len, b"+0x");
                    push_hex(buf, len, frame - entry.base);
                }
                None => {
                    push_bytes(buf, len, b"unknown+0x");
                    push_hex(buf, len, frame);
                }
            }
        }
        push_bytes(buf, len, b"\n");
    }
}

/// Copies bytes into the record buffer, silently stopping at the bound.
unsafe fn push_bytes(buf: &[u8; MAX_RECORD], len: &mut usize, bytes: &[u8]) {
    // SAFETY: len is clamped to the buffer bound on every step.
    unsafe {
        for &byte in bytes {
            if *len >= buf.len() {
                return;
            }
            let ptr = buf.as_ptr().add(*len) as *mut u8;
            *ptr = byte;
            *len += 1;
        }
    }
}

/// Writes `value` as lowercase hex.
unsafe fn push_hex(buf: &[u8; MAX_RECORD], len: &mut usize, mut value: u64) {
    // SAFETY: bounded by push_bytes.
    unsafe {
        let mut digits = [0u8; 16];
        let mut used = 0usize;
        loop {
            digits[used] = b"0123456789abcdef"[(value & 0xf) as usize];
            used += 1;
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        let mut out = [0u8; 16];
        for (i, digit) in digits[..used].iter().rev().enumerate() {
            out[i] = *digit;
        }
        push_bytes(buf, len, &out[..used]);
    }
}
