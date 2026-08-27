//! Shared helpers for Rust functions called by C# shims.

use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

/// Run a foreign-call body without allowing a panic to cross the boundary.
pub fn catch_or<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Run a fallible foreign-call body and use one fallback for errors or panics.
pub fn catch_result_or<T, E>(fallback: T, f: impl FnOnce() -> Result<T, E>) -> T {
    catch_unwind(AssertUnwindSafe(f))
        .ok()
        .and_then(Result::ok)
        .unwrap_or(fallback)
}

/// Borrow a checked UTF-8 C string for one synchronous foreign call.
///
/// # Safety
///
/// A non-null `ptr` must reference a NUL-terminated string that remains valid
/// for the duration of `f`.
pub unsafe fn with_utf8<T>(ptr: *const c_char, f: impl FnOnce(&str) -> T) -> Option<T> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a live NUL-terminated string.
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(f)
}

/// Decode a checked UTF-16 path borrowed for one synchronous foreign call.
///
/// # Safety
///
/// A non-null `ptr` must reference at least `len` UTF-16 code units that remain
/// valid for the duration of this call.
pub unsafe fn utf16_path(ptr: *const u16, len: i32) -> Option<PathBuf> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: the caller guarantees that `ptr` covers `len` live code units.
    let units = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf16(units).ok().map(PathBuf::from)
}

/// Allocate a string for return to a C# shim, or return null for interior NUL.
pub fn string_into_raw(text: String) -> *mut c_char {
    CString::new(text)
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

/// Release a string previously returned by [`string_into_raw`].
///
/// # Safety
///
/// A non-null `ptr` must have come from [`string_into_raw`] and must be freed
/// exactly once.
pub unsafe fn string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: the caller guarantees this is a live allocation from
        // `string_into_raw` with no earlier free.
        drop(unsafe { CString::from_raw(ptr) });
    }
}
