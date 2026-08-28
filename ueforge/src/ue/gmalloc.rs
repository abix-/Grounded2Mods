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
/// MEASURED 2026-08-26 on the MISERY Steam build:
/// **slot 5**, `imm8 0x28`. Three independent call sites agree,
/// and `measure_malloc_slot` re-derives it from the running image
/// at any time. `misery-mod/tests/research_gmalloc.rs` is the
/// test, and it FAILS rather than answering if the sites
/// disagree, which is what caught the first wrong scan.
///
/// If a game patch moves it, that test says so.
const MALLOC_SLOT: Option<usize> = Some(5);

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
static GMALLOC_GLOBAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Set once the first allocation has been announced in the log.
static ANNOUNCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

// ---- Measuring Malloc's vtable slot ----

/// Find `FMalloc::Malloc`'s vtable slot by reading it out of the
/// running image.
///
/// The slot is NOT guessable. Slot 2 was inferred from
/// patternsleuth's own pattern bytes and tried live on
/// 2026-08-26: the call returned null and the process died in the
/// same second. So it gets measured.
///
/// What we look for is the call inside `FMemory::Malloc`, which
/// forwards to `GMalloc->Malloc(count, alignment)`:
///
/// ```text
/// 48 8B 0D | ?? ?? ?? ??   mov rcx, [GMalloc]   <- xref to the global
/// ...                      argument shuffling, length varies
/// 48 8B 01                 mov rax, [rcx]       <- load the vtable
/// FF 50 ??                 call [rax+imm8]      <- the byte we want
/// ```
///
/// The anchor is the xref: `GMALLOC_GLOBAL` is already resolved
/// at init, so we scan for instructions that reference THAT
/// address rather than for a byte pattern that might match
/// anywhere. Then read forward a little for the vtable load and
/// the indirect call.
///
/// Returns `(slot, imm8, code address)` for each site found. More
/// than one is normal: `FMemory::Malloc` and its tail-call twin
/// both do this, and they should agree.
///
/// Read-only. It scans the image and reads bytes; it calls
/// nothing.
pub fn measure_malloc_slot() -> Result<Vec<(usize, u8, usize)>, String> {
    let global = gmalloc_global().ok_or("GMalloc global not resolved yet")?;

    // Anchoring on the xref alone is NOT enough, and a first
    // attempt that did so found SEVEN different slots: every
    // FMalloc virtual is reached the same way, so `Free`,
    // `Realloc` and `GetAllocationSize` all match
    // `mov rax,[rcx]; call [rax+imm8]` just as well as `Malloc`.
    // The research test caught that and refused to hand back an
    // answer (`misery-mod/tests/research_gmalloc.rs`).
    //
    // What separates Malloc is its ARGUMENTS. It takes
    // `(SIZE_T Count, uint32 Alignment)`, so the third argument
    // register `r8d` and the second `rdx` are both loaded before
    // the call. That is the shape patternsleuth's own GMalloc
    // pattern documents:
    //
    //   48 8B 0D ?? ?? ?? ??   mov rcx, [GMalloc]
    //   44 8B ??               mov r8d, <alignment>
    //   48 8B ??               mov rdx, <size>
    //   48 8B 01               mov rax, [rcx]
    //   FF 50 ??               call [rax+imm8]
    //
    // Which register each argument comes from varies by build, so
    // those bytes are wildcards; the ORDER of the two loads
    // varies too, so both are tried.
    // Read off the running image, 2026-08-26. The vtable load
    // comes FIRST, then the arguments, and it TAIL-JUMPS rather
    // than calling:
    //
    //   48 8B 0D ?? ?? ?? ??   mov rcx, [GMalloc]
    //   48 8B 01               mov rax, [rcx]      vtable
    //   44 8B C3               mov r8d, ebx        alignment
    //   48 8B D7               mov rdx, rdi        size
    //   ...                    epilogue
    //   48 FF 60 28            jmp [rax+0x28]      slot 5
    //
    // A first attempt put the argument loads BEFORE the vtable
    // load and matched nothing. The two argument registers are
    // what separate Malloc(Count, Alignment) from Free(void*),
    // which is otherwise the same shape and sits at a different
    // slot.
    let sig = format!("48 8B 0D X0x{global:X} 48 8B 01 44 8B ?? 48 8B ??");
    let sites = modforge::patterns::sleuth::scan_all_matches(&sig)
        .map_err(|e| format!("scan failed: {e}"))?;

    let mut found = Vec::new();
    for site in sites {
        // The epilogue between the arguments and the jump varies
        // in length, so read forward for whichever comes first.
        for offset in 16..48 {
            // SAFETY: our own loaded image, inside a region
            // patternsleuth just matched in.
            let b = unsafe { std::slice::from_raw_parts((site + offset) as *const u8, 4) };
            // FF 50 imm8 = call [rax+imm8]
            if b[0] == 0xFF && b[1] == 0x50 {
                found.push((b[2] as usize / 8, b[2], site));
                break;
            }
            // 48 FF 60 imm8 = jmp [rax+imm8], the tail-call form
            if b[0] == 0x48 && b[1] == 0xFF && b[2] == 0x60 {
                found.push((b[3] as usize / 8, b[3], site));
                break;
            }
        }
    }
    if found.is_empty() {
        return Err(format!(
            "no FMemory::Malloc-shaped call to GMalloc at {global:#x}"
        ));
    }
    Ok(found)
}

/// Every instruction that references the `GMalloc` global, with
/// the bytes that follow it.
///
/// For when the shapes in [`measure_malloc_slot`] do not match:
/// rather than guessing another encoding, read what the binary
/// actually does. Returns `(address, hex bytes)`.
///
/// Read-only.
pub fn gmalloc_call_sites(bytes_each: usize) -> Result<Vec<(usize, String)>, String> {
    let global = gmalloc_global().ok_or("GMalloc global not resolved yet")?;
    let sig = format!("48 8B 0D X0x{global:X}");
    let sites = modforge::patterns::sleuth::scan_all_matches(&sig)
        .map_err(|e| format!("xref scan failed: {e}"))?;
    let mut out = Vec::new();
    for site in sites {
        // SAFETY: reading our own loaded image at a position
        // patternsleuth just matched inside.
        let raw = unsafe { std::slice::from_raw_parts(site as *const u8, bytes_each) };
        out.push((
            site,
            raw.iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    Ok(out)
}

/// What the slot currently is, if it has been set.
pub fn configured_slot() -> Option<usize> {
    MALLOC_SLOT
}
