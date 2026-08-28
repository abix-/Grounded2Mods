//! File + console logger for Windows DLL mods.
//!
//! The DLL captures its own `HMODULE` in `DllMain`, calls
//! `set_dll_module(hmod)`, then later (from a worker thread)
//! calls `init(Config { .. })`. After init, every line goes to
//! `<DLL_dir>/<filename>` and (if `console: true`) a popped-up
//! console window titled `console_title`.
//!
//! Game-specific perf counters live in the embedding crate. Wrap
//! `log()` with a `time_scope` if you want timing. The generic
//! logger doesn't take that dep.

use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use parking_lot::Mutex;

use windows_sys::Win32::Foundation::{HANDLE, HMODULE};
use windows_sys::Win32::System::Console::{
    AllocConsole, GetStdHandle, STD_OUTPUT_HANDLE, SetConsoleTitleW, WriteConsoleA,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

/// Logger configuration. Only the file is mandatory; the console
/// is feature-gated at the embedding crate's level (e.g. ship
/// builds skip it).
#[derive(Clone, Copy)]
pub struct Config {
    /// File name (no path) under the DLL's directory.
    pub file_name: &'static str,
    /// Window title for the popped-up console. Used only when
    /// `console = true`. UTF-8 input is converted to UTF-16 for
    /// `SetConsoleTitleW`.
    pub console_title: &'static str,
    /// If `true`, call `AllocConsole` during `init` and mirror
    /// every line to stdout. Off in shipping.
    pub console: bool,
}

struct Sink {
    file: Mutex<Option<File>>,
    console: HANDLE,
}

// SAFETY: Sink holds a Mutex<Option<File>> (Send + Sync) + a
// raw HANDLE that we never write through outside of synchronized
// WriteFile calls; the OS handle itself is process-global +
// thread-safe for write traffic.
unsafe impl Send for Sink {}
// SAFETY: see Send impl. The file mutex serializes writes; the
// console HANDLE is set once at init and read-only thereafter.
unsafe impl Sync for Sink {}

static SINK: OnceLock<Sink> = OnceLock::new();

/// HMODULE of the embedding DLL, captured in `DllMain` and stored
/// here so `dll_dir()` can resolve `GetModuleFileNameW` after init.
/// Stored as `usize` so the `static` is `Sync` and zero-initialized.
static DLL_HMODULE: AtomicUsize = AtomicUsize::new(0);

/// Call from `DllMain` (or anywhere the DLL's HMODULE is known).
/// Required before `init`; without it, `dll_dir()` falls back to
/// `.\` and the log file lands wherever the host launched from.
pub fn set_dll_module(hmod: HMODULE) {
    DLL_HMODULE.store(hmod as usize, Ordering::Release);
}

/// Initialize the logger. Idempotent. Second call is a no-op so
/// it's safe to wire multiple init paths.
pub fn init(cfg: Config) {
    if SINK.get().is_some() {
        return;
    }
    let console = if cfg.console {
        // SAFETY: AllocConsole / SetConsoleTitleW /
        // GetStdHandle are FFI calls to the current process's
        // console subsystem. AllocConsole creates the console
        // if not present; SetConsoleTitleW receives a NUL-
        // terminated UTF-16 string we built from cfg; GetStdHandle
        // returns the stdout HANDLE (or INVALID_HANDLE on failure
        //. Callers tolerate both via is_null check elsewhere).
        unsafe {
            AllocConsole();
            let title: Vec<u16> = cfg
                .console_title
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            SetConsoleTitleW(title.as_ptr());
            GetStdHandle(STD_OUTPUT_HANDLE)
        }
    } else {
        std::ptr::null_mut()
    };
    let path = log_path(cfg.file_name);
    let file = File::create(&path).ok();
    let _ = SINK.set(Sink {
        file: Mutex::new(file),
        console,
    });
    log(format_args!("ueforge log file: {}", path.display()));
}

/// Resolve the directory containing the DLL captured via
/// `set_dll_module`. Returns `None` if the HMODULE is null or
/// `GetModuleFileNameW` fails.
///
/// Workers that may race `DllMain` should call
/// [`dll_dir_wait`] instead; this function is the non-blocking
/// shape.
pub fn dll_dir() -> Option<PathBuf> {
    let hmod = DLL_HMODULE.load(Ordering::Acquire) as HMODULE;
    if hmod.is_null() {
        return None;
    }
    let mut buf = [0u16; 1024];
    // SAFETY: hmod was captured by set_dll_module at DllMain
    // (non-null per the check above); buf is a stack-allocated
    // u16 array we own; len fits in u32.
    let n = unsafe { GetModuleFileNameW(hmod, buf.as_mut_ptr(), buf.len() as u32) };
    if n == 0 || (n as usize) >= buf.len() {
        return None;
    }
    let path = OsString::from_wide(&buf[..n as usize]);
    let mut p = PathBuf::from(path);
    p.pop();
    Some(p)
}

/// Like [`dll_dir`] but spins up to `timeout` waiting for
/// `set_dll_module` to land. Used by workers that may start
/// before `DllMain` finishes wiring the HMODULE. Returns `None`
/// after `timeout` elapses with no HMODULE captured. Poll
/// interval is 1ms; cheap on the worker that's about to do real
/// work anyway.
pub fn dll_dir_wait(timeout: std::time::Duration) -> Option<PathBuf> {
    if let Some(p) = dll_dir() {
        return Some(p);
    }
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
        if let Some(p) = dll_dir() {
            return Some(p);
        }
    }
    None
}

fn log_path(file_name: &str) -> PathBuf {
    dll_dir()
        .unwrap_or_else(|| PathBuf::from(".\\"))
        .join(file_name)
}

/// Write one line. Prepends a `[HH:MM:SS]` wall-clock prefix and
/// appends `\r\n`. If `init` hasn't run yet, the call is a silent
/// no-op (don't crash on early diagnostic prints).
pub fn log(args: std::fmt::Arguments<'_>) {
    let Some(sink) = SINK.get() else { return };
    let line = format_line(args);
    if !sink.console.is_null() {
        let mut written: u32 = 0;
        // SAFETY: sink.console was set at init; non-null per the
        // check above. WriteConsoleA reads `len` bytes from
        // `line.as_ptr()` (a valid Vec<u8> we own for the
        // duration of this call) and writes the byte count to
        // `&mut written`.
        unsafe {
            WriteConsoleA(
                sink.console,
                line.as_ptr().cast(),
                line.len() as u32,
                &mut written,
                std::ptr::null(),
            );
        }
    }
    let mut guard = sink.file.lock();
    if let Some(f) = guard.as_mut() {
        let _ = f.write_all(&line);
        let _ = f.flush();
    }
}

fn format_line(args: std::fmt::Arguments<'_>) -> Vec<u8> {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() % 86_400)
        .unwrap_or(0);
    let h = secs / 3600;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    let mut out = Vec::with_capacity(96);
    let _ = write!(&mut out, "[{:02}:{:02}:{:02}] ", h, m, s);
    let _ = out.write_fmt(args);
    out.extend_from_slice(b"\r\n");
    out
}

/// `ueforge::log!("foo {}", x)`. Convenience over
/// `ueforge::log::log(format_args!(...))`. Mirrors `println!`.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::log(format_args!($($arg)*))
    };
}
