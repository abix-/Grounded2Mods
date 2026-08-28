//! D3.1 / D3.2 horse-struct lifecycle anchors.
//!
//! Every horse creation path in the binary routes through a single
//! constructor `FUN_1400aac60` (after `FUN_1402c704c(0x498)` allocs
//! the 1176-byte buffer). The mirror destructor `FUN_1400bf1f0`
//! decrements the live-count global and frees the buffer.
//!
//! We hook both:
//! - **D3.1 (post-hook constructor)**: when a new horse is born,
//!   ensure `EXT_HORSE_GENOMES[ret_ptr]` exists with default-zero
//!   alleles. The breeding combinator (D3.4) and save loader (D4.2b)
//!   then overwrite it with real data.
//! - **D3.2 (pre-hook destructor)**: when a horse is freed, drop
//!   its entry so the map doesn't accumulate stale pointer keys.
//!
//! Handler discipline: atomics-only, off-stack slow path. See
//! `docs/DEBUGGING.md` §4b.

use crate::genes;
use crate::targets::{self, fn_addr};
use retour::GenericDetour;
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Signatures
// ---------------------------------------------------------------------------

/// `FUN_1400aac60`: horse constructor.
/// Decomp: `undefined8 * FUN_1400aac60(undefined8 *param_1)`.
/// Receives the freshly-alloc'd 0x498 buffer, returns the same ptr.
type HorseCtorFn = unsafe extern "system" fn(*mut c_void) -> *mut c_void;

/// `FUN_1400bf1f0`: horse destructor.
/// Decomp: `undefined8 * FUN_1400bf1f0(undefined8 *param_1, ulonglong param_2)`.
/// param_2 & 1 controls whether to free the buffer (C++ delete-flag).
type HorseDtorFn = unsafe extern "system" fn(*mut c_void, usize) -> *mut c_void;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

static CTOR_DETOUR: AtomicPtr<GenericDetour<HorseCtorFn>> = AtomicPtr::new(std::ptr::null_mut());
static DTOR_DETOUR: AtomicPtr<GenericDetour<HorseDtorFn>> = AtomicPtr::new(std::ptr::null_mut());

static CTOR_CALLS: AtomicU64 = AtomicU64::new(0);
static DTOR_CALLS: AtomicU64 = AtomicU64::new(0);
static ENTRIES_CREATED: AtomicU64 = AtomicU64::new(0);
static ENTRIES_DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn is_armed_ctor() -> bool {
    !CTOR_DETOUR.load(Ordering::Acquire).is_null()
}
pub fn is_armed_dtor() -> bool {
    !DTOR_DETOUR.load(Ordering::Acquire).is_null()
}
pub fn is_armed() -> bool {
    is_armed_ctor() && is_armed_dtor()
}

pub struct Stats {
    pub ctor_calls: u64,
    pub dtor_calls: u64,
    pub entries_created: u64,
    pub entries_dropped: u64,
}

pub fn stats() -> Stats {
    Stats {
        ctor_calls: CTOR_CALLS.load(Ordering::Relaxed),
        dtor_calls: DTOR_CALLS.load(Ordering::Relaxed),
        entries_created: ENTRIES_CREATED.load(Ordering::Relaxed),
        entries_dropped: ENTRIES_DROPPED.load(Ordering::Relaxed),
    }
}

fn reset_stats() {
    CTOR_CALLS.store(0, Ordering::Relaxed);
    DTOR_CALLS.store(0, Ordering::Relaxed);
    ENTRIES_CREATED.store(0, Ordering::Relaxed);
    ENTRIES_DROPPED.store(0, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

unsafe extern "system" fn ctor_handler(buf: *mut c_void) -> *mut c_void {
    CTOR_CALLS.fetch_add(1, Ordering::Relaxed);
    let p = CTOR_DETOUR.load(Ordering::Acquire);
    if p.is_null() {
        return buf;
    }
    // SAFETY: pointer published by arm() via Box::into_raw + Release.
    let result = unsafe { (*p).call(buf) };
    if !result.is_null() {
        ctor_slowpath(result as usize as u64);
    }
    result
}

#[inline(never)]
fn ctor_slowpath(horse_id: u64) {
    if genes::ensure_horse_ext_genome(horse_id) {
        ENTRIES_CREATED.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe extern "system" fn dtor_handler(buf: *mut c_void, flags: usize) -> *mut c_void {
    DTOR_CALLS.fetch_add(1, Ordering::Relaxed);
    if !buf.is_null() {
        dtor_slowpath(buf as usize as u64);
    }
    let p = DTOR_DETOUR.load(Ordering::Acquire);
    if p.is_null() {
        return buf;
    }
    // SAFETY: pointer published by arm() via Box::into_raw + Release.
    unsafe { (*p).call(buf, flags) }
}

#[inline(never)]
fn dtor_slowpath(horse_id: u64) {
    if genes::drop_horse_ext_genome(horse_id) {
        ENTRIES_DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Dryrun
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TargetReport {
    pub name: &'static str,
    pub rva: usize,
    pub runtime_addr: usize,
    pub prologue_bytes: [u8; 16],
}

pub fn dryrun_ctor() -> TargetReport {
    dryrun_at("HORSE_CONSTRUCTOR", fn_addr::HORSE_CONSTRUCTOR)
}

pub fn dryrun_dtor() -> TargetReport {
    dryrun_at("HORSE_DESTRUCTOR", fn_addr::HORSE_DESTRUCTOR)
}

fn dryrun_at(name: &'static str, rva: usize) -> TargetReport {
    let runtime_addr = targets::rebase(rva);
    let mut prologue = [0u8; 16];
    // SAFETY: function entry inside our running image; read-only.
    let view = unsafe { std::slice::from_raw_parts(runtime_addr as *const u8, 16) };
    prologue.copy_from_slice(view);
    TargetReport {
        name,
        rva,
        runtime_addr,
        prologue_bytes: prologue,
    }
}

// ---------------------------------------------------------------------------
// Arm / revert
// ---------------------------------------------------------------------------

pub fn arm() -> anyhow::Result<()> {
    arm_ctor()?;
    if let Err(e) = arm_dtor() {
        // If dtor fails to arm, revert ctor so we don't have a
        // half-armed state.
        revert_ctor();
        return Err(e);
    }
    reset_stats();
    Ok(())
}

fn arm_ctor() -> anyhow::Result<()> {
    if is_armed_ctor() {
        anyhow::bail!("lifecycle ctor already armed");
    }
    let runtime_addr = crate::targets_registry::resolve::horse_constructor()
        .unwrap_or_else(|| targets::rebase(fn_addr::HORSE_CONSTRUCTOR));
    log_prologue("HORSE_CONSTRUCTOR", runtime_addr);

    // SAFETY: address is the true entry of FUN_1400aac60.
    let target: HorseCtorFn = unsafe { std::mem::transmute(runtime_addr) };
    // SAFETY: signatures match decomp prototype.
    let detour = unsafe { GenericDetour::new(target, ctor_handler) }
        .map_err(|e| anyhow::anyhow!("lifecycle ctor: GenericDetour::new failed: {e}"))?;
    // SAFETY: enable installs JMP + trampoline.
    unsafe { detour.enable() }
        .map_err(|e| anyhow::anyhow!("lifecycle ctor: enable failed: {e}"))?;
    let leaked: *mut GenericDetour<HorseCtorFn> = Box::into_raw(Box::new(detour));
    CTOR_DETOUR.store(leaked, Ordering::Release);
    modforge::log!("lifecycle: armed HORSE_CONSTRUCTOR at 0x{runtime_addr:x}");
    Ok(())
}

fn arm_dtor() -> anyhow::Result<()> {
    if is_armed_dtor() {
        anyhow::bail!("lifecycle dtor already armed");
    }
    let runtime_addr = crate::targets_registry::resolve::horse_destructor()
        .unwrap_or_else(|| targets::rebase(fn_addr::HORSE_DESTRUCTOR));
    log_prologue("HORSE_DESTRUCTOR", runtime_addr);

    // SAFETY: address is the true entry of FUN_1400bf1f0.
    let target: HorseDtorFn = unsafe { std::mem::transmute(runtime_addr) };
    // SAFETY: signatures match.
    let detour = unsafe { GenericDetour::new(target, dtor_handler) }
        .map_err(|e| anyhow::anyhow!("lifecycle dtor: GenericDetour::new failed: {e}"))?;
    // SAFETY: enable installs JMP + trampoline.
    unsafe { detour.enable() }
        .map_err(|e| anyhow::anyhow!("lifecycle dtor: enable failed: {e}"))?;
    let leaked: *mut GenericDetour<HorseDtorFn> = Box::into_raw(Box::new(detour));
    DTOR_DETOUR.store(leaked, Ordering::Release);
    modforge::log!("lifecycle: armed HORSE_DESTRUCTOR at 0x{runtime_addr:x}");
    Ok(())
}

fn log_prologue(name: &str, addr: usize) {
    // SAFETY: target inside our loaded image; 8 bytes readable.
    let prologue = unsafe { std::slice::from_raw_parts(addr as *const u8, 8) };
    let prologue_hex: String = prologue
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    modforge::log!("lifecycle: {name} prologue=[{prologue_hex}]");
}

pub fn revert() {
    // Dtor first (so a free in-flight is unhooked before the ctor
    // detour is yanked). Order doesn't strictly matter on x64 with
    // atomics + detour revert, but mirror LIFO of arm.
    revert_dtor();
    revert_ctor();
}

fn revert_ctor() {
    let p = CTOR_DETOUR.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if p.is_null() {
        return;
    }
    // SAFETY: published by arm_ctor(); sole owner now.
    let det = unsafe { Box::from_raw(p) };
    // SAFETY: disable restores the prologue.
    let res = unsafe { det.disable() };
    match res {
        Ok(()) => modforge::log!("lifecycle: reverted HORSE_CONSTRUCTOR"),
        Err(e) => modforge::log!("lifecycle: revert ctor FAILED: {e}"),
    }
}

fn revert_dtor() {
    let p = DTOR_DETOUR.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if p.is_null() {
        return;
    }
    // SAFETY: published by arm_dtor(); sole owner now.
    let det = unsafe { Box::from_raw(p) };
    // SAFETY: disable restores the prologue.
    let res = unsafe { det.disable() };
    match res {
        Ok(()) => modforge::log!("lifecycle: reverted HORSE_DESTRUCTOR"),
        Err(e) => modforge::log!("lifecycle: revert dtor FAILED: {e}"),
    }
}
