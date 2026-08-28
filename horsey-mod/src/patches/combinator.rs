//! D3.4 breeding-combinator post-hook.
//!
//! Vanilla `FUN_1400a2d80(p_a_ctx, p_b_ctx, child_ctx)` performs
//! Mendelian recombination of two parents' 0x1e0-byte inline genomes
//! at `horse + 0x2b8` into the child's inline genome. See
//! [`horsey-mod/docs/GENE-CATALOG.md`](../../docs/GENE-CATALOG.md)
//! Part 1 Step 1 and `todo.md` D3.4 for the algorithm.
//!
//! Our handler runs AFTER the trampoline. We recover horse base
//! pointers from `ctx - 0x2b8` (the gene-state offset) and run our
//! own Mendelian combinator on the extended range via
//! `genes::combine_for_breeding`.
//!
//! Handler discipline: lock-free atomic-pointer pattern. Vanilla call
//! goes through `modforge::call_original!` which wraps the trampoline
//! invocation in `modforge::seh::guard`, so a bad pointer inside
//! vanilla logs a caught `SehError` instead of taking the game down.

use crate::genes;
use crate::targets::{self, fn_addr};
use modforge::hook::Hook;
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

/// Vanilla offset from `horse` to the inline genome ctx the
/// combinator takes.
fn horse_ctx_offset() -> usize {
    crate::targets::horse_offset::ctx_offset()
}

/// `FUN_1400a2d80` signature: three `longlong` pointers, void return.
type CombinatorFn = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void);

static HOOK_PTR: AtomicPtr<Hook<CombinatorFn>> = AtomicPtr::new(std::ptr::null_mut());

static CALL_COUNT: AtomicU64 = AtomicU64::new(0);
static COMBINES_DONE: AtomicU64 = AtomicU64::new(0);
static VANILLA_CRASHES: AtomicU64 = AtomicU64::new(0);

pub fn is_armed() -> bool {
    !HOOK_PTR.load(Ordering::Acquire).is_null()
}

pub struct Stats {
    pub call_count: u64,
    pub combines_done: u64,
    pub vanilla_crashes: u64,
}

pub fn stats() -> Stats {
    Stats {
        call_count: CALL_COUNT.load(Ordering::Relaxed),
        combines_done: COMBINES_DONE.load(Ordering::Relaxed),
        vanilla_crashes: VANILLA_CRASHES.load(Ordering::Relaxed),
    }
}

fn reset_stats() {
    CALL_COUNT.store(0, Ordering::Relaxed);
    COMBINES_DONE.store(0, Ordering::Relaxed);
    VANILLA_CRASHES.store(0, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Handler (post-hook). Atomics + indirect call + SEH-guarded vanilla.
// ---------------------------------------------------------------------------

unsafe extern "system" fn combinator_handler(
    p_a_ctx: *mut c_void,
    p_b_ctx: *mut c_void,
    child_ctx: *mut c_void,
) {
    CALL_COUNT.fetch_add(1, Ordering::Relaxed);

    let p = HOOK_PTR.load(Ordering::Acquire);
    if p.is_null() {
        return;
    }
    // SAFETY: pointer was published by `arm()` via Box::into_raw +
    // AtomicPtr::store(Release); valid until `revert()` swaps it out.
    let hook = unsafe { &*p };

    // Call vanilla under SEH guard. A crash inside FUN_1400a2d80
    // logs a SehError and we just skip our post-processing.
    let outcome = modforge::call_original!(hook, p_a_ctx, p_b_ctx, child_ctx);
    if outcome.is_err() {
        VANILLA_CRASHES.fetch_add(1, Ordering::Relaxed);
        return;
    }

    if p_a_ctx.is_null() || p_b_ctx.is_null() || child_ctx.is_null() {
        return;
    }

    // Recover horse base pointers. Vanilla call site:
    // FUN_1400a2d80(parent_a + 0x2b8, parent_b + 0x2b8, child + 0x2b8)
    let pa_id = (p_a_ctx as usize).wrapping_sub(horse_ctx_offset()) as u64;
    let pb_id = (p_b_ctx as usize).wrapping_sub(horse_ctx_offset()) as u64;
    let child_id = (child_ctx as usize).wrapping_sub(horse_ctx_offset()) as u64;

    combinator_slowpath(pa_id, pb_id, child_id);
    COMBINES_DONE.fetch_add(1, Ordering::Relaxed);
}

#[inline(never)]
fn combinator_slowpath(parent_a_id: u64, parent_b_id: u64, child_id: u64) {
    genes::combine_for_breeding(parent_a_id, parent_b_id, child_id);
}

// ---------------------------------------------------------------------------
// Dryrun (read-only diagnostic)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TargetReport {
    pub name: &'static str,
    pub rva: usize,
    pub runtime_addr: usize,
    pub prologue_bytes: [u8; 16],
}

pub fn dryrun() -> TargetReport {
    let rva = fn_addr::GENE_COMBINATOR;
    let runtime_addr = targets::rebase(rva);
    let mut prologue = [0u8; 16];
    // SAFETY: function entry inside our running image; read-only.
    let view = unsafe { std::slice::from_raw_parts(runtime_addr as *const u8, 16) };
    prologue.copy_from_slice(view);
    TargetReport {
        name: "GENE_COMBINATOR",
        rva,
        runtime_addr,
        prologue_bytes: prologue,
    }
}

// ---------------------------------------------------------------------------
// Arm / revert
// ---------------------------------------------------------------------------

pub fn arm() -> anyhow::Result<()> {
    if is_armed() {
        anyhow::bail!("combinator already armed");
    }
    let runtime_addr = crate::targets_registry::resolve::gene_combinator()
        .unwrap_or_else(|| targets::rebase(fn_addr::GENE_COMBINATOR));

    modforge::log!("combinator: installing GENE_COMBINATOR target=0x{runtime_addr:x}");
    // SAFETY: runtime_addr is the true entry of FUN_1400a2d80;
    // CombinatorFn matches the decomp prototype.
    let hook = unsafe {
        Hook::<CombinatorFn>::install("GENE_COMBINATOR", runtime_addr, combinator_handler)
    }?;
    let leaked = Box::into_raw(Box::new(hook));
    HOOK_PTR.store(leaked, Ordering::Release);
    reset_stats();
    modforge::log!("combinator: armed");
    Ok(())
}

pub fn revert() {
    let p = HOOK_PTR.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if p.is_null() {
        return;
    }
    // SAFETY: p was published by arm() and we just took ownership atomically.
    // Hook's Drop disables the detour.
    let _ = unsafe { Box::from_raw(p) };
    modforge::log!("combinator: reverted");
}
