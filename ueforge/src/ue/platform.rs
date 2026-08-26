//! Host process detection helpers.
//!
//! Two pieces:
//! - `host_image_base`. `GetModuleHandleW(null)` cast as
//!   `usize`. The image base of the main exe; offsets in
//!   [`PlatformOffsets`] are relative to this.
//! - `host_exe_name`. Basename of the running exe, e.g.
//!   `"Grounded2-WinGRTS-Shipping.exe"` or
//!   `"OutworldStation-Win64-Shipping.exe"`.
//! - `detect_platform`. Match the exe basename against a
//!   game-supplied table of `(exe_substring, PlatformOffsets)`
//!   pairs. Substring match (not equality) so suffixed exes
//!   (e.g. `Foo-Win64-Shipping-debug.exe`) still match.
//!
//! Game crates declare their offset tables and pass them in at
//! init time:
//!
//! ```ignore
//! use ueforge::ue::{PlatformOffsets, platform};
//!
//! const STEAM: PlatformOffsets = PlatformOffsets { /* ... */ };
//! const XBOX:  PlatformOffsets = PlatformOffsets { /* ... */ };
//!
//! const PLATFORMS: &[(&str, &PlatformOffsets)] = &[
//!     ("WinGRTS-Shipping.exe", &STEAM),
//!     ("WinGDK-Shipping.exe",  &XBOX),
//! ];
//!
//! let offsets = platform::detect(PLATFORMS).unwrap_or(&STEAM);
//! ```

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr;

use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};

use crate::ue::offsets::PlatformOffsets;

/// `GetModuleHandleW(null)` cast to `usize`. Used as the base for
/// every offset in [`PlatformOffsets`].
pub fn host_image_base() -> usize {
    let h = unsafe { GetModuleHandleW(ptr::null()) };
    h as usize
}

/// Basename of the host process exe, e.g.
/// `"OutworldStation-Win64-Shipping.exe"`.
pub fn host_exe_name() -> Option<String> {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) };
    if n == 0 {
        return None;
    }
    let path = OsString::from_wide(&buf[..n as usize])
        .to_string_lossy()
        .into_owned();
    let last = path.rsplit(['/', '\\']).next()?.to_string();
    Some(last)
}

/// Match the host exe's basename against a list of
/// `(substring, offsets)` pairs. Returns the first matching
/// `&PlatformOffsets`. Caller falls back to a default on `None`.
pub fn detect<'a>(
    table: &[(&str, &'a PlatformOffsets)],
) -> Option<&'a PlatformOffsets> {
    let exe = host_exe_name()?;
    table
        .iter()
        .find(|(needle, _)| exe.contains(needle))
        .map(|(_, off)| *off)
}

/// Detect the host exe, choose the right `PlatformOffsets`, init
/// the runtime, and log a one-line header. Falls back to the
/// first row in `table` with a WARN if the exe doesn't match.
///
/// The single boilerplate every UE4SS Rust mod's worker runs at
/// startup. Replaces ~15 lines of per-mod setup:
///
/// ```ignore
/// const PLATFORMS: &[(&str, &ueforge::ue::PlatformOffsets)] = &[
///     ("MyGame-WinShipping.exe", &MY_OFFSETS),
/// ];
/// let _rt = ueforge::ue::platform::detect_and_init(PLATFORMS);
/// ```
pub fn detect_and_init(
    table: &[(&str, &'static PlatformOffsets)],
) -> &'static crate::ue::Runtime {
    let image_base = host_image_base();
    let offsets = detect(table).unwrap_or_else(|| {
        let exe = host_exe_name().unwrap_or_default();
        crate::log!(
            "WARN: unknown host exe '{}'; defaulting to first platform row",
            exe
        );
        // Safe: callers always pass a non-empty table; an empty
        // table here is a programming error, not a runtime
        // condition. Panic with a clear message instead of UB.
        table
            .first()
            .map(|(_, off)| *off)
            .expect("detect_and_init: platforms table must be non-empty")
    });
    crate::log!(
        "image_base = 0x{:x}, GObjects = 0x{:x}",
        image_base,
        image_base + offsets.g_objects
    );
    unsafe { crate::ue::init_runtime(image_base, offsets) }
}

/// Resolve all address offsets via patternsleuth, combine with
/// the two structural constants the caller provides, init the
/// runtime, and run discovery.
///
/// Panics if patternsleuth fails. There are no hardcoded address
/// fallbacks; stale addresses break on every game patch, so we
/// require the pattern scan to succeed.
pub fn resolve_and_init(
    process_event_idx: usize,
    g_objects_layout: crate::ue::GObjectsLayout,
) -> &'static crate::ue::Runtime {
    let image_base = host_image_base();
    let exe = host_exe_name().unwrap_or_default();
    crate::log!("image_base = 0x{image_base:x}, host = {exe}");

    let resolved = crate::ue::resolvers::resolve_image_offsets()
        .expect("patternsleuth failed: cannot resolve UE offsets");
    crate::log!(
        "patternsleuth resolved: g_objects=0x{:x} g_names=0x{:x} append_string=0x{:x}",
        resolved.g_objects,
        resolved.g_names,
        resolved.append_string
    );
    // Stash the engine allocator now, while we are already in the
    // one scan and off the game thread. Resolving it later, on
    // demand, means firing a fresh patternsleuth scan at whatever
    // moment a mod happens to grow an array, and patternsleuth
    // scans in a rayon pool. See misery research.md 26.4 for what
    // that costs.
    crate::ue::gmalloc::set_global(image_base + resolved.gmalloc);

    let offsets: &'static PlatformOffsets = Box::leak(Box::new(PlatformOffsets {
        g_objects: resolved.g_objects,
        append_string: resolved.append_string,
        g_names: resolved.g_names,
        process_event_idx,
        g_objects_layout,
    }));

    let rt = unsafe { crate::ue::init_runtime(image_base, offsets) };
    crate::log!(
        "ue runtime ready, GObjects = 0x{:x}",
        image_base + offsets.g_objects
    );
    let _ = crate::discovery::run_at_load();
    rt
}
