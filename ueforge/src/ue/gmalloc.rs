//! The engine's own allocator.
//!
//! Anything the engine will later free or grow MUST come from
//! here. Handing the engine a buffer from Rust's heap works right
//! up until the engine reallocs it, at which point
//! `FMallocBinned2` looks for its canary in the bytes before the
//! block, finds Rust's data instead, and kills the process:
//!
//! ```text
//! FMallocBinned2 Attempt to realloc an unrecognized block
//! 000001F6FA450000  canary == 0x65 != 0xe3
//! ```
//!
//! That crash showed up on disconnect, long after the write that
//! caused it, because disconnect is when the engine tears down
//! the arrays a mod grew during play.
//!
//! `GMalloc` is a global pointer to an `FMalloc` object, and
//! `Malloc` is a virtual method on it, so no function needs
//! resolving: read the global, follow it, call through the
//! vtable.
//!
//! **Slot 2.** patternsleuth's own GMalloc patterns match inside
//! `FMemory::Malloc`, which forwards to `GMalloc->Malloc(...)`,
//! and the call is in the pattern bytes:
//!
//! ```text
//! 48 8B 0D | ?? ?? ?? ??   mov rcx, [GMalloc]
//! 44 8B C3                 mov r8d, ebx      alignment
//! 48 8B D7                 mov rdx, rdi      size
//! 48 8B 01                 mov rax, [rcx]    vtable
//! FF 50 10                 call [rax+0x10]   <- byte 0x10 = slot 2
//! ```
//!
//! A second pattern ends in the tail-jump form of the same call,
//! `48 FF 60 10`. Byte offset 0x10 is slot 2, taking two
//! arguments: `Malloc(SIZE_T Count, uint32 Alignment)`.

use std::ffi::c_void;

/// `FMalloc::Malloc`'s vtable slot, once MEASURED from this
/// binary. `None` means "not known yet, do not call anything".
///
/// It starts as `None` on purpose. Slot 2 was inferred from
/// patternsleuth's pattern bytes (`call [rax+0x10]` inside
/// `FMemory::Malloc`) and tried live on 2026-08-26: the call
/// returned null and the process died the same second. An
/// unverified vtable slot is a call to an arbitrary engine
/// function with arbitrary arguments, so this stays `None` until
/// the displacement is read out of the running image.
///
/// Measure it by finding `mov rax,[rcx]; call [rax+imm8]` inside
/// `FMemory::Malloc`, the function patternsleuth already anchors
/// in to locate the GMalloc global. The `imm8` IS the byte
/// offset; the slot is that over 8.
const MALLOC_SLOT: Option<usize> = None;

/// UE allocates most things 16-byte aligned; a `TArray` buffer of
/// structs never needs more.
pub const DEFAULT_ALIGNMENT: u32 = 16;

type MallocFn = unsafe extern "system" fn(*mut c_void, usize, u32) -> *mut c_void;

/// Address of the `GMalloc` global.
///
/// Set once by `platform::resolve_and_init`, from the same
/// patternsleuth scan that resolves everything else. It is NOT
/// resolved on demand: a lazy resolve fires a fresh scan at
/// whatever moment a mod first grows an array, which puts a
/// rayon-backed scan on the game thread mid-frame.
static GMALLOC_GLOBAL: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Set once the first allocation has been announced in the log.
static ANNOUNCED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Record the resolved `GMalloc` global. Called at init.
pub fn set_global(addr: usize) {
    GMALLOC_GLOBAL.store(addr, std::sync::atomic::Ordering::Release);
}

fn gmalloc_global() -> Option<usize> {
    match GMALLOC_GLOBAL.load(std::sync::atomic::Ordering::Acquire) {
        0 => None,
        a => Some(a),
    }
}

/// The live `FMalloc` object, or `None` before the runtime is up
/// or if the global has not been written yet (very early startup).
fn fmalloc() -> Option<*mut c_void> {
    super::try_runtime()?;
    let global = gmalloc_global()?;
    // SAFETY: the resolved address is the GMalloc global, which
    // holds an FMalloc pointer once the engine has started.
    let obj = unsafe { *(global as *const *mut c_void) };
    if obj.is_null() { None } else { Some(obj) }
}

/// Allocate `size` bytes from the engine's allocator, zeroed.
///
/// The engine may freely realloc or free the result, which is the
/// whole point: memory handed to an engine structure has to be
/// engine memory.
///
/// Returns `None` rather than falling back to Rust's heap: a
/// silent fallback is what caused the crash this module exists to
/// prevent.
pub fn alloc_zeroed(size: usize, alignment: u32) -> Option<*mut u8> {
    if size == 0 {
        return None;
    }
    let obj = fmalloc()?;
    let Some(slot) = MALLOC_SLOT else {
        if !ANNOUNCED.swap(true, std::sync::atomic::Ordering::AcqRel) {
            crate::log::log(format_args!(
                "gmalloc: FMalloc {:#x} reachable, but Malloc's vtable slot has not \
                 been measured for this build; refusing to allocate",
                obj as usize
            ));
        }
        return None;
    };
    // SAFETY: obj is the live FMalloc, and `slot` is a MEASURED
    // Malloc taking (this, count, alignment).
    let ptr = unsafe {
        let vtable = *(obj as *const *const usize);
        let target = *vtable.add(slot);
        // Log BEFORE the first call, not after: if the slot is
        // wrong the process dies inside it and an after-the-fact
        // log never gets written.
        if !ANNOUNCED.swap(true, std::sync::atomic::Ordering::AcqRel) {
            crate::log::log(format_args!(
                "gmalloc: FMalloc {:#x} vtable {:#x} slot {slot} -> {target:#x}, \
                 first alloc {size} bytes",
                obj as usize, vtable as usize
            ));
        }
        let f: MallocFn = std::mem::transmute(target);
        f(obj, size, alignment)
    };
    if ptr.is_null() {
        return None;
    }
    // FMalloc does not promise zeroed memory. Callers writing a
    // partly-filled array would otherwise leave the tail holding
    // whatever the allocator last had there, which for an array
    // of pointers means the engine dereferencing garbage.
    // SAFETY: ptr is a live allocation of at least `size` bytes.
    unsafe {
        std::ptr::write_bytes(ptr as *mut u8, 0, size);
    }
    Some(ptr as *mut u8)
}

/// True when the engine allocator can be reached, for callers
/// that want to fail early with a clear message.
pub fn is_available() -> bool {
    fmalloc().is_some()
}
