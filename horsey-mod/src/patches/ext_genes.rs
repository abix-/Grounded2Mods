//! DI-A detours for the extended-gene integration.
//!
//! Strategy: per-function trampolines via `retour::GenericDetour`.
//! Each detour wraps a vanilla function that touches the gene table
//! at a literal index. Our handler dispatches on the index: vanilla
//! path for `idx < 240`, sidecar path for `idx >= 240`. See
//! `horsey-mod/docs/HOOKING-STRATEGY.md` §6.2 for the rationale.
//!
//! v1 covers 3 of the 5 DI-A targets:
//!   - `EVAL_DIPLOID_BLEND_A` (FUN_1400a5d20)
//!   - `EVAL_DIPLOID_BLEND_B` (FUN_1400a5e00)
//!   - `GENE_ALLELE_SWAP`    (FUN_1400c03a0)
//!
//! Deferred (need separate analysis):
//!   - `GENE_DEATH_DRIFT` (FUN_1400c0660): the gene index isn't a
//!     function parameter; it's computed mid-function. Needs a
//!     mid-function patch, not an entry-point detour.
//!   - `CRISPR_LAB` UI dispatch (FUN_1400c1cf0): state-machine
//!     function with internal gene-idx dispatch. Needs structural
//!     analysis before we know where to detour.
//!
//! Safety posture: detours stay disarmed at attach by default. The
//! operator runs `genes.ext.dryrun` to confirm addresses look sane,
//! then `genes.ext.arm` to install. `revert()` runs from
//! `patches::revert_all` on DLL detach so hot reload is safe.

use crate::genes;
use crate::targets::{self, fn_addr};
use retour::GenericDetour;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

// HANDLER STACK BUDGET DISCIPLINE
//
// The DI-A handlers run on the GAME's threads, often nested deep
// inside vanilla call chains where the remaining stack budget is
// small. The first bring-up attempt with a `format!()` + 256-byte
// `dbg_trace` buffer in the handler entry allocated 0x148 bytes
// of stack and crashed at the first `MOVAPS [RSP+disp], XMM6`
// because Win64 doesn't auto-commit guard pages past the SUB-RSP.
//
// Rule: handlers do atomic ops + integer math + at most one CALL
// to a separately-compiled inner function. NO `format!`, NO
// `modforge::log!`, NO stack-buffered `OutputDebugStringA`, NO
// parking_lot Mutex lock taken at handler entry. Everything that
// needs >32 bytes of stack lives behind an `#[inline(never)]`
// helper that the handler tail-calls only when an extended index
// is detected (rare; arrives via D5 trampoline, not vanilla
// recursion).

// ---------------------------------------------------------------------------
// Function signatures (must match the vanilla decomp prototypes exactly)
// ---------------------------------------------------------------------------

/// `FUN_1400a5d20`: allele diploid blend evaluator (formula A).
///
/// Decomp signature: `float FUN_1400a5d20(longlong param_1, int param_2)`
type EvalDiploidBlendFn = unsafe extern "system" fn(*const u8, i32) -> f32;

/// `FUN_1400a5e00`: allele diploid blend evaluator (formula B).
///
/// Decomp signature: `int FUN_1400a5e00(longlong param_1, int param_2)`.
/// Returns an `int` per the decomp but the bit pattern IS an f32
/// (vanilla's caller bit-casts). We model as f32 to keep the math
/// path consistent; the win64 ABI returns both via XMM0 / RAX in
/// the same low 32 bits for our purposes.
type EvalDiploidBlendBFn = unsafe extern "system" fn(*const u8, i32) -> u32;

/// `FUN_1400c03a0`: gene allele renumber sync.
///
/// Decomp signature: `void FUN_1400c03a0(undefined8 param_1, int param_2, int param_3, int param_4)`
/// - `param_1`: opaque pointer (game state ctx). Passed through.
/// - `param_2`: gene index. Same dispatch criterion as the others.
/// - `param_3`, `param_4`: allele positions being swapped.
type GeneAlleleSwapFn = unsafe extern "system" fn(usize, i32, i32, i32);

// ---------------------------------------------------------------------------
// Per-detour state
// ---------------------------------------------------------------------------

// Lock-free detour storage. Each slot holds a `*mut GenericDetour<T>`
// owned by a leaked Box. Set once during `arm()`; loaded relaxed-
// atomically by every handler call. No parking_lot, no
// thread-local state, no chance of deadlock or TLS faults on
// foreign (game) threads.
//
// The trade-off: revert() must drop the Box manually. Done with
// Acquire/Release ordering paired with the AtomicPtr swap so the
// owning thread sees a fully-constructed detour and revert sees
// the same instance every handler is reading.
static EVAL_A_DETOUR: AtomicPtr<GenericDetour<EvalDiploidBlendFn>> =
    AtomicPtr::new(std::ptr::null_mut());
static EVAL_B_DETOUR: AtomicPtr<GenericDetour<EvalDiploidBlendBFn>> =
    AtomicPtr::new(std::ptr::null_mut());
static ALLELE_SWAP_DETOUR: AtomicPtr<GenericDetour<GeneAlleleSwapFn>> =
    AtomicPtr::new(std::ptr::null_mut());

/// Total handler invocations across all detours since `arm()`.
static CALL_COUNT: AtomicU64 = AtomicU64::new(0);

/// Handler invocations that took the extended path (`idx >= 240`).
static EXT_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

/// Max gene index any handler has seen. Sanity: stays < 240 until
/// something passes an extended index (e.g. the D5 trampoline
/// evaluating extended slots).
static MAX_IDX_SEEN: AtomicU64 = AtomicU64::new(0);

/// Per-handler invocation counts. (a, b, swap).
static CALL_COUNT_A: AtomicU64 = AtomicU64::new(0);
static CALL_COUNT_B: AtomicU64 = AtomicU64::new(0);
static CALL_COUNT_SWAP: AtomicU64 = AtomicU64::new(0);

/// Per-handler "EXTENDED path taken" counts. (a, b, swap).
/// Increments only when gene_idx >= 240. I.e. when the handler
/// dispatched to our sidecar instead of the trampoline.
static EXT_COUNT_A: AtomicU64 = AtomicU64::new(0);
static EXT_COUNT_B: AtomicU64 = AtomicU64::new(0);
static EXT_COUNT_SWAP: AtomicU64 = AtomicU64::new(0);

/// Last gene index passed to each handler. Useful diagnostic when
/// stats show non-zero counts but no extended-path hits.
static LAST_IDX_A: AtomicU64 = AtomicU64::new(0);
static LAST_IDX_B: AtomicU64 = AtomicU64::new(0);
static LAST_IDX_SWAP: AtomicU64 = AtomicU64::new(0);

pub fn is_armed() -> bool {
    !EVAL_A_DETOUR.load(Ordering::Acquire).is_null()
        || !EVAL_B_DETOUR.load(Ordering::Acquire).is_null()
        || !ALLELE_SWAP_DETOUR.load(Ordering::Acquire).is_null()
}

pub fn stats() -> Stats {
    Stats {
        call_count: CALL_COUNT_A.load(Ordering::Relaxed)
            + CALL_COUNT_B.load(Ordering::Relaxed)
            + CALL_COUNT_SWAP.load(Ordering::Relaxed),
        ext_call_count: EXT_CALL_COUNT.load(Ordering::Relaxed),
        max_idx_seen: MAX_IDX_SEEN.load(Ordering::Relaxed),
        call_count_a: CALL_COUNT_A.load(Ordering::Relaxed),
        call_count_b: CALL_COUNT_B.load(Ordering::Relaxed),
        call_count_swap: CALL_COUNT_SWAP.load(Ordering::Relaxed),
        ext_count_a: EXT_COUNT_A.load(Ordering::Relaxed),
        ext_count_b: EXT_COUNT_B.load(Ordering::Relaxed),
        ext_count_swap: EXT_COUNT_SWAP.load(Ordering::Relaxed),
        last_idx_a: LAST_IDX_A.load(Ordering::Relaxed),
        last_idx_b: LAST_IDX_B.load(Ordering::Relaxed),
        last_idx_swap: LAST_IDX_SWAP.load(Ordering::Relaxed),
    }
}

pub struct Stats {
    pub call_count: u64,
    pub ext_call_count: u64,
    pub max_idx_seen: u64,
    pub call_count_a: u64,
    pub call_count_b: u64,
    pub call_count_swap: u64,
    pub ext_count_a: u64,
    pub ext_count_b: u64,
    pub ext_count_swap: u64,
    pub last_idx_a: u64,
    pub last_idx_b: u64,
    pub last_idx_swap: u64,
}

fn reset_stats() {
    CALL_COUNT.store(0, Ordering::Relaxed);
    EXT_CALL_COUNT.store(0, Ordering::Relaxed);
    MAX_IDX_SEEN.store(0, Ordering::Relaxed);
    CALL_COUNT_A.store(0, Ordering::Relaxed);
    CALL_COUNT_B.store(0, Ordering::Relaxed);
    CALL_COUNT_SWAP.store(0, Ordering::Relaxed);
    EXT_COUNT_A.store(0, Ordering::Relaxed);
    EXT_COUNT_B.store(0, Ordering::Relaxed);
    EXT_COUNT_SWAP.store(0, Ordering::Relaxed);
    LAST_IDX_A.store(0, Ordering::Relaxed);
    LAST_IDX_B.store(0, Ordering::Relaxed);
    LAST_IDX_SWAP.store(0, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// EVAL_A handler. `idx<240` falls through to vanilla; `idx>=240`
/// computes the diploid blend against `EXT_GENE_TABLE` +
/// `EXT_HORSE_GENOMES`. See `genes::evaluate_ext_gene`.
///
/// SAFETY: signature matches the vanilla function. extern "system"
/// on x64 Windows is the win64 ABI; matches `(longlong, int) -> float`.
unsafe extern "system" fn eval_a_handler(genome: *const u8, idx: i32) -> f32 {
    CALL_COUNT_A.fetch_add(1, Ordering::Relaxed);
    let idx_u = idx.max(0) as u64;
    LAST_IDX_A.store(idx_u, Ordering::Relaxed);
    MAX_IDX_SEEN.fetch_max(idx_u, Ordering::Relaxed);
    if idx >= 240 {
        EXT_COUNT_A.fetch_add(1, Ordering::Relaxed);
        EXT_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        let horse_id = genome as u64;
        let ext_idx = (idx - 240) as usize;
        return genes::evaluate_ext_gene(horse_id, ext_idx);
    }
    let p = EVAL_A_DETOUR.load(Ordering::Acquire);
    if p.is_null() {
        return 0.0;
    }
    // SAFETY: pointer was published by arm() via Box::leak and
    // an AtomicPtr::store(Release); we Acquire-load it. The detour
    // outlives every handler call until revert() runs (which
    // unwires the JMP before dropping the Box).
    unsafe { (*p).call(genome, idx) }
}

/// EVAL_B handler. Same dispatch as EVAL_A but the vanilla return
/// type is `int` (bit-cast f32). For the extended path we evaluate
/// the same blend and bit-cast to u32 so the caller's bit-pattern
/// expectation is preserved.
///
/// SAFETY: extern "system" matches `(longlong, int) -> int`.
unsafe extern "system" fn eval_b_handler(genome: *const u8, idx: i32) -> u32 {
    CALL_COUNT_B.fetch_add(1, Ordering::Relaxed);
    let idx_u = idx.max(0) as u64;
    LAST_IDX_B.store(idx_u, Ordering::Relaxed);
    MAX_IDX_SEEN.fetch_max(idx_u, Ordering::Relaxed);
    if idx >= 240 {
        EXT_COUNT_B.fetch_add(1, Ordering::Relaxed);
        EXT_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        let horse_id = genome as u64;
        let ext_idx = (idx - 240) as usize;
        return genes::evaluate_ext_gene(horse_id, ext_idx).to_bits();
    }
    let p = EVAL_B_DETOUR.load(Ordering::Acquire);
    if p.is_null() {
        return 0;
    }
    // SAFETY: see eval_a_handler's note.
    unsafe { (*p).call(genome, idx) }
}

/// GENE_ALLELE_SWAP handler. For `idx<240` vanilla syncs the swap
/// across every pop record. For `idx>=240` we mirror the swap in
/// our `EXT_GENE_TABLE` and `EXT_POP_WEIGHTS` and SKIP vanilla
/// (vanilla would walk DAT_1403ee4b0 with an out-of-range index
/// and crash).
///
/// SAFETY: extern "system" matches `(undefined8, int, int, int) -> void`.
unsafe extern "system" fn allele_swap_handler(ctx: usize, gene_idx: i32, a: i32, b: i32) {
    CALL_COUNT_SWAP.fetch_add(1, Ordering::Relaxed);
    let idx_u = gene_idx.max(0) as u64;
    LAST_IDX_SWAP.store(idx_u, Ordering::Relaxed);
    MAX_IDX_SEEN.fetch_max(idx_u, Ordering::Relaxed);
    if gene_idx >= 240 {
        EXT_COUNT_SWAP.fetch_add(1, Ordering::Relaxed);
        EXT_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        let ext_idx = (gene_idx - 240) as usize;
        genes::swap_ext_alleles(ext_idx, a as usize, b as usize);
        return;
    }
    let p = ALLELE_SWAP_DETOUR.load(Ordering::Acquire);
    if p.is_null() {
        return;
    }
    // SAFETY: see eval_a_handler's note.
    unsafe { (*p).call(ctx, gene_idx, a, b) };
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
    /// True if `arm()` wires up a real detour for this target.
    pub wired: bool,
}

/// Per-target metadata. The full DI-A target set is enumerated for
/// dryrun visibility even when some are not yet wired.
const TARGETS: &[(&str, usize, bool)] = &[
    ("EVAL_DIPLOID_BLEND_A", fn_addr::EVAL_DIPLOID_BLEND_A, true),
    ("EVAL_DIPLOID_BLEND_B", fn_addr::EVAL_DIPLOID_BLEND_B, true),
    ("GENE_ALLELE_SWAP", fn_addr::GENE_ALLELE_SWAP, true),
    ("GENE_DEATH_DRIFT", fn_addr::GENE_DEATH_DRIFT, false),
];

pub fn dryrun() -> Vec<TargetReport> {
    let mut out = Vec::with_capacity(TARGETS.len());
    for (name, rva, wired) in TARGETS {
        let runtime_addr = targets::rebase(*rva);
        let mut prologue = [0u8; 16];
        // SAFETY: `runtime_addr` is the entry of a known function
        // in the running Horsey.exe image. Read-only.
        let view = unsafe { std::slice::from_raw_parts(runtime_addr as *const u8, 16) };
        prologue.copy_from_slice(view);
        out.push(TargetReport {
            name,
            rva: *rva,
            runtime_addr,
            prologue_bytes: prologue,
            wired: *wired,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Arm / revert
// ---------------------------------------------------------------------------

fn install_eval_a() -> anyhow::Result<()> {
    let runtime_addr = crate::targets_registry::resolve::eval_diploid_blend_a()
        .unwrap_or_else(|| targets::rebase(fn_addr::EVAL_DIPLOID_BLEND_A));
    let handler_addr = eval_a_handler as *const () as usize;
    modforge::log!(
        "ext_genes: install EVAL_DIPLOID_BLEND_A target=0x{runtime_addr:x} \
         handler=0x{handler_addr:x}"
    );
    // SAFETY: address is the entry of FUN_1400a5d20; signature matches.
    let target: EvalDiploidBlendFn = unsafe { std::mem::transmute(runtime_addr) };
    // SAFETY: target/replacement have identical signatures.
    let detour = unsafe { GenericDetour::new(target, eval_a_handler) }
        .map_err(|e| anyhow::anyhow!("eval_a: GenericDetour::new failed: {e}"))?;
    // SAFETY: enable writes the JMP + installs the trampoline.
    unsafe { detour.enable() }.map_err(|e| anyhow::anyhow!("eval_a: enable failed: {e}"))?;
    // Publish via leak + atomic-release so handlers can read lock-free.
    let leaked: *mut GenericDetour<EvalDiploidBlendFn> = Box::into_raw(Box::new(detour));
    EVAL_A_DETOUR.store(leaked, Ordering::Release);
    modforge::log!("ext_genes: armed EVAL_DIPLOID_BLEND_A at 0x{runtime_addr:x}");
    Ok(())
}

fn install_eval_b() -> anyhow::Result<()> {
    let runtime_addr = crate::targets_registry::resolve::eval_diploid_blend_b()
        .unwrap_or_else(|| targets::rebase(fn_addr::EVAL_DIPLOID_BLEND_B));
    // SAFETY: address is the entry of FUN_1400a5e00.
    let target: EvalDiploidBlendBFn = unsafe { std::mem::transmute(runtime_addr) };
    // SAFETY: signatures match.
    let detour = unsafe { GenericDetour::new(target, eval_b_handler) }
        .map_err(|e| anyhow::anyhow!("eval_b: GenericDetour::new failed: {e}"))?;
    // SAFETY: enable installs the detour.
    unsafe { detour.enable() }.map_err(|e| anyhow::anyhow!("eval_b: enable failed: {e}"))?;
    let leaked: *mut GenericDetour<EvalDiploidBlendBFn> = Box::into_raw(Box::new(detour));
    EVAL_B_DETOUR.store(leaked, Ordering::Release);
    modforge::log!("ext_genes: armed EVAL_DIPLOID_BLEND_B at 0x{runtime_addr:x}");
    Ok(())
}

fn install_allele_swap() -> anyhow::Result<()> {
    let runtime_addr = crate::targets_registry::resolve::gene_allele_swap()
        .unwrap_or_else(|| targets::rebase(fn_addr::GENE_ALLELE_SWAP));
    // SAFETY: address is the entry of FUN_1400c03a0.
    let target: GeneAlleleSwapFn = unsafe { std::mem::transmute(runtime_addr) };
    // SAFETY: signatures match.
    let detour = unsafe { GenericDetour::new(target, allele_swap_handler) }
        .map_err(|e| anyhow::anyhow!("allele_swap: GenericDetour::new failed: {e}"))?;
    // SAFETY: enable installs the detour.
    unsafe { detour.enable() }.map_err(|e| anyhow::anyhow!("allele_swap: enable failed: {e}"))?;
    let leaked: *mut GenericDetour<GeneAlleleSwapFn> = Box::into_raw(Box::new(detour));
    ALLELE_SWAP_DETOUR.store(leaked, Ordering::Release);
    modforge::log!("ext_genes: armed GENE_ALLELE_SWAP at 0x{runtime_addr:x}");
    Ok(())
}

/// Install the DI-A v1 detour set. Lock-free: each install
/// publishes its leaked Box pointer via AtomicPtr::store(Release).
pub fn arm() -> anyhow::Result<()> {
    if is_armed() {
        anyhow::bail!("ext_genes already armed");
    }
    install_eval_a()?;
    // EVAL_B added 2026-05-14 after EVAL_A was proven end-to-end
    // (2222 in-game invocations, game stayed alive). Same handler
    // discipline; same lock-free AtomicPtr storage.
    if let Err(e) = install_eval_b() {
        // Roll back EVAL_A so we never leave the game half-armed.
        revert();
        return Err(e);
    }
    // ALLELE_SWAP re-enabled 2026-05-14 after the
    // Ghidra-off-by-16 finding: real entry at 0x1400c0390
    // (NOT 0x1400c03a0). See targets.rs and DEBUGGING.md.
    if let Err(e) = install_allele_swap() {
        // Rollback A + B if SWAP fails.
        revert();
        return Err(e);
    }
    reset_stats();
    Ok(())
}

/// Revert every installed detour. Atomic-swap each slot to null
/// so any in-flight handler call sees the change; then disable
/// the detour and drop its Box.
pub fn revert() {
    let p_a = EVAL_A_DETOUR.swap(std::ptr::null_mut(), Ordering::AcqRel);
    let p_b = EVAL_B_DETOUR.swap(std::ptr::null_mut(), Ordering::AcqRel);
    let p_swap = ALLELE_SWAP_DETOUR.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if !p_a.is_null() {
        // SAFETY: p_a was published by install_eval_a via Box::into_raw.
        // We are the sole owner now (we just swapped it out atomically).
        let det = unsafe { Box::from_raw(p_a) };
        // SAFETY: disable restores the prologue retour stashed at enable.
        let res = unsafe { det.disable() };
        match res {
            Ok(()) => modforge::log!("ext_genes: reverted EVAL_DIPLOID_BLEND_A"),
            Err(e) => modforge::log!("ext_genes: revert EVAL_A FAILED: {e}"),
        }
    }
    if !p_b.is_null() {
        // SAFETY: same as above for EVAL_B.
        let det = unsafe { Box::from_raw(p_b) };
        // SAFETY: disable restores the prologue.
        let res = unsafe { det.disable() };
        match res {
            Ok(()) => modforge::log!("ext_genes: reverted EVAL_DIPLOID_BLEND_B"),
            Err(e) => modforge::log!("ext_genes: revert EVAL_B FAILED: {e}"),
        }
    }
    if !p_swap.is_null() {
        // SAFETY: same as above for ALLELE_SWAP.
        let det = unsafe { Box::from_raw(p_swap) };
        // SAFETY: disable restores the prologue.
        let res = unsafe { det.disable() };
        match res {
            Ok(()) => modforge::log!("ext_genes: reverted GENE_ALLELE_SWAP"),
            Err(e) => modforge::log!("ext_genes: revert ALLELE_SWAP FAILED: {e}"),
        }
    }
}
