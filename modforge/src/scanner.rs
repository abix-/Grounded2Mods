//! Cheat-Engine-style memory scanner. In-process, so no
//! `ReadProcessMemory` dance. We walk our own committed
//! VirtualAlloc regions and match value patterns directly.
//!
//! Surface (all exposed as ops on the embedding mod's `/debug`):
//!
//! - `scan_memory { type, value, regions? }` → first scan; opens
//!   a session and returns the survivor address list.
//! - `scan_rescan { session_id, mode, value? }` → narrows the
//!   session by re-reading current values and applying one of
//!   `exact`, `changed`, `unchanged`, `decreased`, `increased`,
//!   `decreased_by` (with `delta`), `increased_by` (with `delta`).
//! - `scan_session { session_id, max?, offset? }` → paginate
//!   over the current survivor set.
//! - `scan_close { session_id }` → drop the session state.
//! - `freeze { addr, type, value, hz? }` → spawn a writer thread
//!   that rewrites `value` at `addr` every `1/hz` seconds.
//! - `unfreeze { addr }` / `freeze_list`.
//!
//! Value types: `u8 i8 u16 i16 u32 i32 u64 i64 f32 f64`. Step
//! size = type's natural alignment. For exotic types (FName,
//! UTF-16 strings, byte sequences) you'd extend `Value` later;
//! the primitive set handles ~95% of game-state finds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use serde_json::{Value as Json, json};

use crate::args::{arg_str, arg_u64};

// ---- Value type + parsing -------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
}

impl Ty {
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "u8" => Ty::U8,
            "i8" => Ty::I8,
            "u16" => Ty::U16,
            "i16" => Ty::I16,
            "u32" => Ty::U32,
            "i32" => Ty::I32,
            "u64" => Ty::U64,
            "i64" => Ty::I64,
            "f32" => Ty::F32,
            "f64" => Ty::F64,
            other => return Err(format!("unknown value type '{other}'")),
        })
    }
    fn size(self) -> usize {
        match self {
            Ty::U8 | Ty::I8 => 1,
            Ty::U16 | Ty::I16 => 2,
            Ty::U32 | Ty::I32 | Ty::F32 => 4,
            Ty::U64 | Ty::I64 | Ty::F64 => 8,
        }
    }
}

/// In-memory value. f32/f64 stored as bits so equality works
/// the way scan callers expect (NaN matches NaN; -0 != +0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Val {
    U64Bits(u64, Ty),
}

impl Val {
    fn from_json(v: &Json, ty: Ty) -> Result<Self, String> {
        let bits = match ty {
            Ty::U8 => v
                .as_u64()
                .ok_or("expected u8")?
                .try_into()
                .map(|x: u8| x as u64)
                .map_err(|_| "u8 out of range")?,
            Ty::I8 => v
                .as_i64()
                .ok_or("expected i8")?
                .try_into()
                .map(|x: i8| (x as u8) as u64)
                .map_err(|_| "i8 out of range")?,
            Ty::U16 => v
                .as_u64()
                .ok_or("expected u16")?
                .try_into()
                .map(|x: u16| x as u64)
                .map_err(|_| "u16 out of range")?,
            Ty::I16 => v
                .as_i64()
                .ok_or("expected i16")?
                .try_into()
                .map(|x: i16| (x as u16) as u64)
                .map_err(|_| "i16 out of range")?,
            Ty::U32 => v
                .as_u64()
                .ok_or("expected u32")?
                .try_into()
                .map(|x: u32| x as u64)
                .map_err(|_| "u32 out of range")?,
            Ty::I32 => v
                .as_i64()
                .ok_or("expected i32")?
                .try_into()
                .map(|x: i32| (x as u32) as u64)
                .map_err(|_| "i32 out of range")?,
            Ty::U64 => v.as_u64().ok_or("expected u64")?,
            Ty::I64 => v.as_i64().ok_or("expected i64")? as u64,
            Ty::F32 => (v.as_f64().ok_or("expected f32")? as f32).to_bits() as u64,
            Ty::F64 => v.as_f64().ok_or("expected f64")?.to_bits(),
        };
        Ok(Val::U64Bits(bits, ty))
    }
    fn bytes(self) -> Vec<u8> {
        let Val::U64Bits(bits, ty) = self;
        match ty {
            Ty::U8 | Ty::I8 => vec![bits as u8],
            Ty::U16 | Ty::I16 => (bits as u16).to_le_bytes().to_vec(),
            Ty::U32 | Ty::I32 | Ty::F32 => (bits as u32).to_le_bytes().to_vec(),
            Ty::U64 | Ty::I64 | Ty::F64 => bits.to_le_bytes().to_vec(),
        }
    }
}

/// Read a `Ty`-sized value at `ptr` as a u64 of bits.
///
/// SAFETY: The caller must guarantee `[ptr, ptr + ty.size())` is
/// readable. Used on a buffer freshly populated by
/// [`safe_read_chunk`] (which validates the source range), so the
/// read is always against process-owned heap.
unsafe fn read_bits_buf(ptr: *const u8, ty: Ty) -> u64 {
    // SAFETY: caller's `unsafe fn` contract guarantees [ptr,
    // ptr+ty.size()) is readable (populated by safe_read_chunk).
    // Each match arm reads the matching T via read_unaligned.
    unsafe {
        match ty {
            Ty::U8 | Ty::I8 => (ptr.read_unaligned()) as u64,
            Ty::U16 | Ty::I16 => (ptr as *const u16).read_unaligned() as u64,
            Ty::U32 | Ty::I32 | Ty::F32 => (ptr as *const u32).read_unaligned() as u64,
            Ty::U64 | Ty::I64 | Ty::F64 => (ptr as *const u64).read_unaligned(),
        }
    }
}

/// Fault-safe read of a `Ty`-sized value from an address that may
/// have been freed by the host process. Uses `ReadProcessMemory`
/// against the current process so the kernel performs the access
/// check without raising an SEH exception in our trampoline.
/// Returns `None` if the source range is no longer readable.
fn safe_read_bits(addr: usize, ty: Ty) -> Option<u64> {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let size = ty.size();
    let mut buf = [0u8; 8];
    let mut read: usize = 0;
    // SAFETY: ReadProcessMemory against the current process is
    // safe even if `addr` is freed/unmapped (kernel returns 0
    // bytes read). buf is stack-allocated 8 bytes; size <= 8.
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            addr as *const _,
            buf.as_mut_ptr() as *mut _,
            size,
            &mut read,
        )
    };
    if ok == 0 || read != size {
        return None;
    }
    Some(match ty {
        Ty::U8 | Ty::I8 => buf[0] as u64,
        Ty::U16 | Ty::I16 => u16::from_le_bytes([buf[0], buf[1]]) as u64,
        Ty::U32 | Ty::I32 | Ty::F32 => u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64,
        Ty::U64 | Ty::I64 | Ty::F64 => u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]),
    })
}

/// Fault-safe bulk read of `[addr, addr+len)` into `dst`. Returns
/// the number of bytes the kernel actually copied (may be less
/// than `len` if a page in the range was freed mid-call). The
/// scanner walks regions in chunks via this helper so a torn page
/// only invalidates that chunk. The rest of the region is still
/// scanned. Without this, `read_unaligned` over a freed page raises
/// an access violation that crashes the host process.
fn safe_read_chunk(addr: usize, dst: &mut [u8]) -> usize {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut read: usize = 0;
    // SAFETY: same as safe_read_bits; kernel-side validation lets
    // us scan torn pages without an access violation.
    let ok = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            addr as *const _,
            dst.as_mut_ptr() as *mut _,
            dst.len(),
            &mut read,
        )
    };
    if ok == 0 { 0 } else { read }
}

/// Sign-extending or float-aware comparator helpers used by the
/// `decreased / increased` rescan modes.
fn cmp(ty: Ty, a: u64, b: u64) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match ty {
        Ty::U8 => (a as u8).cmp(&(b as u8)),
        Ty::I8 => (a as i8).cmp(&(b as i8)),
        Ty::U16 => (a as u16).cmp(&(b as u16)),
        Ty::I16 => (a as i16).cmp(&(b as i16)),
        Ty::U32 => (a as u32).cmp(&(b as u32)),
        Ty::I32 => (a as i32).cmp(&(b as i32)),
        Ty::U64 => a.cmp(&b),
        Ty::I64 => (a as i64).cmp(&(b as i64)),
        Ty::F32 => f32::from_bits(a as u32)
            .partial_cmp(&f32::from_bits(b as u32))
            .unwrap_or(Equal),
        Ty::F64 => f64::from_bits(a)
            .partial_cmp(&f64::from_bits(b))
            .unwrap_or(Equal),
    }
}

fn signed_diff(ty: Ty, a: u64, b: u64) -> i64 {
    // Returns a - b for the type, sign-aware.
    match ty {
        Ty::U8 => (a as u8 as i64) - (b as u8 as i64),
        Ty::I8 => (a as i8 as i64) - (b as i8 as i64),
        Ty::U16 => (a as u16 as i64) - (b as u16 as i64),
        Ty::I16 => (a as i16 as i64) - (b as i16 as i64),
        Ty::U32 => (a as u32 as i64) - (b as u32 as i64),
        Ty::I32 => (a as i32 as i64) - (b as i32 as i64),
        Ty::U64 => (a as i64).wrapping_sub(b as i64),
        Ty::I64 => (a as i64).wrapping_sub(b as i64),
        Ty::F32 => (f32::from_bits(a as u32) - f32::from_bits(b as u32)) as i64,
        Ty::F64 => (f64::from_bits(a) - f64::from_bits(b)) as i64,
    }
}

// ---- Region walker (private RW pages) ------------------------------------

#[derive(Clone, Copy)]
struct Region {
    base: usize,
    size: usize,
}

/// Walk every committed private RW region in this process. Skip
/// image / mapped / read-only / executable. We're explicitly
/// chasing dynamic state (UObjects on the heap, allocator
/// buffers, etc.).
fn iter_private_rw_regions() -> Vec<Region> {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_PRIVATE, MEMORY_BASIC_INFORMATION, PAGE_READWRITE, PAGE_WRITECOPY,
        VirtualQuery,
    };
    let mut out = Vec::new();
    let mut addr: usize = 0;
    // SAFETY: zero-init of a #[repr(C)] POD struct is well-defined
    // (all fields are integer/pointer types that accept any byte
    // pattern; VirtualQuery overwrites every field).
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let info_size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();
    loop {
        // SAFETY: VirtualQuery on the current process is safe for
        // any address; returns 0 when out of address space, which
        // we use as the loop-exit condition.
        let written = unsafe { VirtualQuery(addr as *const _, &mut info, info_size) };
        if written == 0 {
            break;
        }
        let next = info.BaseAddress as usize + info.RegionSize;
        if next <= addr {
            break;
        }
        if info.State == MEM_COMMIT
            && info.Type == MEM_PRIVATE
            && (info.Protect & 0xFF == PAGE_READWRITE || info.Protect & 0xFF == PAGE_WRITECOPY)
        {
            out.push(Region {
                base: info.BaseAddress as usize,
                size: info.RegionSize,
            });
        }
        addr = next;
    }
    out
}

// ---- Session state --------------------------------------------------------

struct Session {
    ty: Ty,
    addresses: Vec<usize>,
}

static SESSIONS: OnceLock<Mutex<HashMap<u64, Session>>> = OnceLock::new();
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

fn sessions() -> &'static Mutex<HashMap<u64, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---- Freezes --------------------------------------------------------------

/// Where a freeze's target address came from. Selector-backed
/// freezes can re-resolve when the cached address goes stale (UE
/// recycles allocations, the source object moves, etc.). Raw-addr
/// freezes have no recovery path. Once the page is gone, the
/// freeze just stops.
#[derive(Clone)]
enum FreezeSource {
    /// `addr:0x...` / `class:Foo` / `singleton:Bar` + byte offset.
    /// Re-resolved on validation miss.
    Selector { selector: String, offset: usize },
    /// User passed a raw `addr:0x...` directly. Stops on staleness.
    RawAddr,
}

struct FreezeJob {
    /// Most recent resolved address for the write. Re-resolved
    /// from `source` when validation misses.
    addr: usize,
    /// Original address at registration time. Used in log lines
    /// + as the stable map key.
    initial_addr: usize,
    ty: Ty,
    bytes: Vec<u8>,
    hz: u32,
    interval: Duration,
    /// Earliest moment the sweeper should write this job again.
    next_due: Instant,
    source: FreezeSource,
    /// Consecutive failed validations. The sweeper drops the job
    /// after `MAX_CONSECUTIVE_FAILURES` so a permanently-stale
    /// freeze can't keep re-resolving forever.
    failures: u32,
}

static FREEZES: OnceLock<Mutex<HashMap<usize, FreezeJob>>> = OnceLock::new();

fn freezes() -> &'static Mutex<HashMap<usize, FreezeJob>> {
    FREEZES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cap on simultaneous freezes. Each freeze burns ~24 bytes of
/// state; the cap bounds the sweeper's per-tick walk and stops
/// a misbehaving test client from stuffing the map. 64 is ~10x
/// the largest realistic concurrent set we've ever used.
pub const MAX_FREEZES: usize = 64;

const MAX_CONSECUTIVE_FAILURES: u32 = 30;
const SWEEPER_DEFAULT_TICK: Duration = Duration::from_millis(10);
const SWEEPER_MIN_TICK: Duration = Duration::from_millis(1);

/// Single sweeper thread that services every active freeze.
/// Replaces the per-freeze thread (which leaked one OS thread per
/// freeze and had no global stop knob). Lazy-spawned on the first
/// `freeze()` call; runs until `shutdown_sweeper_if_running` flips
/// its stop flag (called from `ueforge_mod_shutdown`).
struct Sweeper {
    stop: Arc<AtomicBool>,
    join: Mutex<Option<thread::JoinHandle<()>>>,
}

static SWEEPER: OnceLock<Sweeper> = OnceLock::new();

fn ensure_sweeper() {
    SWEEPER.get_or_init(|| {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let join = thread::Builder::new()
            .name("ueforge-freeze-sweeper".into())
            .spawn(move || sweeper_loop(stop_clone))
            .expect("spawn freeze sweeper");
        Sweeper {
            stop,
            join: Mutex::new(Some(join)),
        }
    });
}

/// Stop the freeze sweeper if it's running, and clear the
/// freeze map. Idempotent + safe to call when no freeze ever
/// ran. Wired into `ueforge_mod_shutdown`.
pub fn shutdown_sweeper_if_running() {
    if let Some(s) = SWEEPER.get() {
        s.stop.store(true, Ordering::Release);
        if let Some(j) = s.join.lock().take() {
            let _ = j.join();
        }
    }
    if let Some(map) = FREEZES.get() {
        map.lock().clear();
    }
}

fn sweeper_loop(stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let mut next_wake: Option<Instant> = None;
        let mut to_drop: Vec<usize> = Vec::new();
        {
            let mut map = freezes().lock();
            for (key, job) in map.iter_mut() {
                if job.next_due <= now {
                    if !is_writable(job.addr, job.bytes.len()) {
                        // Try to re-resolve from a selector.
                        // Raw-addr freezes have no recovery path.
                        let recovered = match &job.source {
                            FreezeSource::Selector { selector, offset } => {
                                resolve_selector_addr(selector, *offset)
                                    .ok()
                                    .filter(|a| is_writable(*a, job.bytes.len()))
                            }
                            FreezeSource::RawAddr => None,
                        };
                        match recovered {
                            Some(new_addr) => {
                                crate::log::log(format_args!(
                                    "ueforge freeze: re-resolved 0x{:X} -> 0x{:X}",
                                    job.addr, new_addr
                                ));
                                job.addr = new_addr;
                                job.failures = 0;
                            }
                            None => {
                                job.failures = job.failures.saturating_add(1);
                                if job.failures >= MAX_CONSECUTIVE_FAILURES {
                                    crate::log::log(format_args!(
                                        "ueforge freeze: 0x{:X} stale for {} ticks; dropping",
                                        job.initial_addr, job.failures
                                    ));
                                    to_drop.push(*key);
                                    continue;
                                }
                                job.next_due = now + job.interval;
                                next_wake = Some(match next_wake {
                                    Some(t) => t.min(job.next_due),
                                    None => job.next_due,
                                });
                                continue;
                            }
                        }
                    }
                    // SAFETY: is_writable confirmed the destination
                    // range is committed RW within the queried region.
                    // Bytes are owned and outlive the copy.
                    unsafe {
                        let dst = job.addr as *mut u8;
                        std::ptr::copy_nonoverlapping(job.bytes.as_ptr(), dst, job.bytes.len());
                    }
                    job.failures = 0;
                    job.next_due = now + job.interval;
                }
                next_wake = Some(match next_wake {
                    Some(t) => t.min(job.next_due),
                    None => job.next_due,
                });
            }
            for k in to_drop {
                map.remove(&k);
            }
        }
        // Sleep until the soonest next_due, clamped to a minimum
        // of 1 ms (so a hz=1000 freeze still gets serviced) and a
        // maximum of 10 ms (so a registry change is picked up
        // promptly when the map was empty).
        let sleep = match next_wake {
            Some(t) => t
                .saturating_duration_since(Instant::now())
                .max(SWEEPER_MIN_TICK),
            None => SWEEPER_DEFAULT_TICK,
        };
        thread::sleep(sleep);
    }
}

/// True if the address range [addr, addr+len) lies entirely within
/// a single committed, writable region. Cheap (one `region::query`);
/// call before every freeze write to avoid faulting on a guard
/// page when the source allocation is freed.
fn is_writable(addr: usize, len: usize) -> bool {
    if addr == 0 {
        return false;
    }
    let region = match region::query(addr as *const std::ffi::c_void) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if !region.is_committed() {
        return false;
    }
    if !region.protection().contains(region::Protection::WRITE) {
        return false;
    }
    // Reject writes that span the end of the queried region.
    let region_end = region.as_range().end as usize;
    addr.saturating_add(len) <= region_end
}

// ---- Ops ------------------------------------------------------------------

pub fn scan_memory(args: &Json) -> Result<Json, String> {
    let ty = Ty::from_str(arg_str(args, "type")?)?;
    let v = args.get("value").ok_or("missing arg 'value'")?;
    let target = Val::from_json(v, ty)?;
    let Val::U64Bits(target_bits, _) = target;

    // Clear any prior cancellation flag so this scan starts fresh.
    SCAN_CANCEL.store(false, Ordering::Release);

    let regions = iter_private_rw_regions();
    let step = ty.size();
    let mut survivors = Vec::new();
    let mut cancelled = false;
    // 64 KiB scratch. Amortizes the ReadProcessMemory syscall
    // (one per chunk, not one per value) while keeping the failure
    // domain of a freed page to that chunk only.
    const CHUNK: usize = 64 * 1024;
    let mut scratch = vec![0u8; CHUNK];
    'outer: for r in &regions {
        if r.size < step {
            continue;
        }
        let mut chunk_off = 0usize;
        while chunk_off < r.size {
            // Cancellation check at the chunk boundary. Cheap (one
            // atomic load per ~64 KiB of memory walked). Bounds the
            // worst-case latency between op call and abort to ~one
            // ReadProcessMemory call (typically sub-ms).
            if SCAN_CANCEL.load(Ordering::Acquire) {
                cancelled = true;
                break 'outer;
            }
            let want = (r.size - chunk_off).min(CHUNK);
            let got = safe_read_chunk(r.base + chunk_off, &mut scratch[..want]);
            if got < step {
                // Page freed or otherwise unreadable. Skip the
                // chunk; pages beyond may still be valid.
                chunk_off += want;
                continue;
            }
            let mut off = 0usize;
            while off + step <= got {
                // SAFETY: off + step <= got was checked by the
                // while condition; scratch has at least `got`
                // valid bytes from safe_read_chunk above.
                let bits = unsafe { read_bits_buf(scratch.as_ptr().add(off), ty) };
                if bits == target_bits {
                    survivors.push(r.base + chunk_off + off);
                }
                off += step;
            }
            chunk_off += got;
        }
    }
    // Reset the flag so a stale set doesn't bleed into the next scan.
    SCAN_CANCEL.store(false, Ordering::Release);

    // Build the JSON sample BEFORE taking the sessions lock so
    // long scans don't block other ops (freeze_list, scan_close,
    // or a concurrent rescan on a different session) for the
    // multi-second duration of the walk above.
    let sample: Vec<String> = survivors
        .iter()
        .take(20)
        .map(|a| format!("0x{a:X}"))
        .collect();
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    let matches = survivors.len();
    sessions().lock().insert(
        id,
        Session {
            ty,
            addresses: survivors,
        },
    );
    Ok(json!({
        "session_id": id,
        "type": format!("{ty:?}").to_lowercase(),
        "matches": matches,
        "regions_scanned": regions.len(),
        "sample": sample,
        "cancelled": cancelled,
    }))
}

/// Cancellation flag for [`scan_memory`] / [`scan_rescan`]. Set
/// by [`scan_cancel`]; the scan loops check it at every
/// 64 KiB chunk boundary and bail with whatever survivors they
/// have so far.
static SCAN_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Abort the in-flight `scan_memory` / `scan_rescan` (if any).
/// Returns the snapshot of the flag prior to setting. The scan
/// loop checks the flag at every 64 KiB chunk boundary; worst-
/// case latency is one ReadProcessMemory call.
pub fn scan_cancel(_args: &Json) -> Result<Json, String> {
    let was = SCAN_CANCEL.swap(true, Ordering::Release);
    Ok(json!({ "previous": was }))
}

pub fn scan_rescan(args: &Json) -> Result<Json, String> {
    let id = arg_u64(args, "session_id", None)?;
    let mode = arg_str(args, "mode")?;
    let mut s = sessions().lock();
    let sess = s
        .get_mut(&id)
        .ok_or_else(|| format!("session {id} not found"))?;
    let ty = sess.ty;

    // Read current value at every survivor. `safe_read_bits` uses
    // ReadProcessMemory so a freed page returns None instead of
    // crashing the process; survivors that no longer read are
    // dropped from the session.
    let cur: Vec<Option<u64>> = sess
        .addresses
        .iter()
        .map(|&a| safe_read_bits(a, ty))
        .collect();

    let new: Vec<usize> = match mode {
        "exact" => {
            let v = args.get("value").ok_or("'exact' needs 'value'")?;
            let Val::U64Bits(want, _) = Val::from_json(v, ty)?;
            sess.addresses
                .iter()
                .zip(&cur)
                .filter_map(|(a, c)| match c {
                    Some(b) if *b == want => Some(*a),
                    _ => None,
                })
                .collect()
        }
        "unchanged" | "changed" | "decreased" | "increased" => {
            // Need previous value for these. We don't store
            // per-survivor previous; instead, accept this rescan
            // contract: caller passes "value" = the previous
            // reference (e.g. the value that was true before the
            // in-game action). Cheat-Engine has true diff-mode
            // that stashes per-addr prev internally; future work.
            let v = args.get("value").ok_or(format!(
                "'{mode}' needs 'value' = the previous reference value"
            ))?;
            let Val::U64Bits(prev, _) = Val::from_json(v, ty)?;
            use std::cmp::Ordering as O;
            sess.addresses
                .iter()
                .zip(&cur)
                .filter_map(|(a, c)| {
                    let b = (*c)?;
                    let keep = match mode {
                        "unchanged" => b == prev,
                        "changed" => b != prev,
                        "decreased" => cmp(ty, b, prev) == O::Less,
                        "increased" => cmp(ty, b, prev) == O::Greater,
                        _ => unreachable!(),
                    };
                    keep.then_some(*a)
                })
                .collect()
        }
        "decreased_by" | "increased_by" => {
            let v = args.get("value").ok_or(format!(
                "'{mode}' needs 'value' = the previous reference value"
            ))?;
            let Val::U64Bits(prev, _) = Val::from_json(v, ty)?;
            let delta_v = args.get("delta").ok_or(format!("'{mode}' needs 'delta'"))?;
            let Val::U64Bits(delta_bits, _) = Val::from_json(delta_v, ty)?;
            let want_diff = match mode {
                "decreased_by" => -(signed_diff(ty, delta_bits, 0).abs()),
                "increased_by" => signed_diff(ty, delta_bits, 0).abs(),
                _ => unreachable!(),
            };
            sess.addresses
                .iter()
                .zip(&cur)
                .filter_map(|(a, c)| {
                    let b = (*c)?;
                    (signed_diff(ty, b, prev) == want_diff).then_some(*a)
                })
                .collect()
        }
        other => return Err(format!("unknown rescan mode '{other}'")),
    };

    sess.addresses = new;
    Ok(json!({
        "session_id": id,
        "matches": sess.addresses.len(),
        "sample": sess.addresses.iter().take(20).map(|a| format!("0x{a:X}")).collect::<Vec<_>>(),
    }))
}

pub fn scan_session(args: &Json) -> Result<Json, String> {
    let id = arg_u64(args, "session_id", None)?;
    let max = arg_u64(args, "max", Some(200))? as usize;
    let offset = arg_u64(args, "offset", Some(0))? as usize;
    let s = sessions().lock();
    let sess = s
        .get(&id)
        .ok_or_else(|| format!("session {id} not found"))?;
    let total = sess.addresses.len();
    let end = (offset + max).min(total);
    let slice: Vec<String> = sess.addresses[offset.min(total)..end]
        .iter()
        .map(|a| format!("0x{a:X}"))
        .collect();
    Ok(json!({
        "session_id": id,
        "type": format!("{:?}", sess.ty).to_lowercase(),
        "total": total,
        "offset": offset,
        "returned": slice.len(),
        "addresses": slice,
    }))
}

pub fn scan_close(args: &Json) -> Result<Json, String> {
    let id = arg_u64(args, "session_id", None)?;
    let removed = sessions().lock().remove(&id).is_some();
    Ok(json!({"session_id": id, "removed": removed}))
}

// ---- Freeze ---------------------------------------------------------------

pub fn freeze(args: &Json) -> Result<Json, String> {
    let ty = Ty::from_str(arg_str(args, "type")?)?;
    let v = args.get("value").ok_or("missing arg 'value'")?;
    let bytes = Val::from_json(v, ty)?.bytes();
    let hz = arg_u64(args, "hz", Some(30))? as u32;
    let hz = hz.clamp(1, 1000);

    // Two arg shapes:
    //   { selector, offset?, type, value } . Preferred. The
    //     sweeper re-resolves on staleness (address-recycling safe).
    //   { addr, type, value }              . Legacy. Address is
    //     opaque; the sweeper drops the freeze when the page is
    //     unmapped or loses RW protection.
    let (addr, source) = match args.get("selector").and_then(Json::as_str) {
        Some(sel) => {
            let offset = arg_u64(args, "offset", Some(0))? as usize;
            let addr = resolve_selector_addr(sel, offset)?;
            (
                addr,
                FreezeSource::Selector {
                    selector: sel.to_string(),
                    offset,
                },
            )
        }
        None => {
            let addr_str = arg_str(args, "addr")?;
            (parse_addr(addr_str)?, FreezeSource::RawAddr)
        }
    };

    if !is_writable(addr, bytes.len()) {
        return Err(format!(
            "freeze: address 0x{addr:X} is not committed RW for {} bytes",
            bytes.len()
        ));
    }

    // Cap concurrent freezes (replacing an existing freeze on the
    // same addr is fine and doesn't count toward the cap).
    {
        let map = freezes().lock();
        if !map.contains_key(&addr) && map.len() >= MAX_FREEZES {
            return Err(format!(
                "freeze: cap reached ({MAX_FREEZES}); unfreeze something first"
            ));
        }
    }

    let interval = Duration::from_millis((1000 / hz.max(1)) as u64);
    let job = FreezeJob {
        addr,
        initial_addr: addr,
        ty,
        bytes,
        hz,
        interval,
        // Service immediately on the next sweeper tick.
        next_due: Instant::now(),
        source,
        failures: 0,
    };
    freezes().lock().insert(addr, job);

    // Spawn the sweeper on first use. Subsequent freezes share it.
    ensure_sweeper();

    Ok(json!({"addr": format!("0x{addr:X}"), "hz": hz, "frozen": true}))
}

/// Selector-aware freeze resolver. Each framework installs its
/// own via `set_selector_resolver` at init. ueforge resolves
/// against UObject + selector grammar; unityforge resolves
/// against Mono types + selectors. Modforge does not know about
/// either.
pub type SelectorResolver = fn(&str, usize) -> Result<usize, String>;

static SELECTOR_RESOLVER: std::sync::OnceLock<SelectorResolver> = std::sync::OnceLock::new();

/// Install the per-framework selector-to-address resolver. Idempotent;
/// only the first call wins, subsequent calls are silent no-ops.
pub fn set_selector_resolver(resolver: SelectorResolver) {
    let _ = SELECTOR_RESOLVER.set(resolver);
}

fn resolve_selector_addr(selector: &str, offset: usize) -> Result<usize, String> {
    let resolver = SELECTOR_RESOLVER
        .get()
        .ok_or("freeze: selector resolver not installed (framework setup error)")?;
    resolver(selector, offset)
}

pub fn unfreeze(args: &Json) -> Result<Json, String> {
    let addr_str = arg_str(args, "addr")?;
    let addr = parse_addr(addr_str)?;
    let removed = freezes().lock().remove(&addr).is_some();
    Ok(json!({"addr": format!("0x{addr:X}"), "removed": removed}))
}

pub fn freeze_list(_args: &Json) -> Result<Json, String> {
    let f = freezes().lock();
    let entries: Vec<Json> = f
        .iter()
        .map(|(addr, job)| {
            let mut row = json!({
                "addr": format!("0x{addr:X}"),
                "type": format!("{:?}", job.ty).to_lowercase(),
                "bytes_hex": hex::encode(&job.bytes),
                "hz": job.hz,
            });
            match &job.source {
                FreezeSource::Selector { selector, offset } => {
                    row["selector"] = json!(selector);
                    row["offset"] = json!(format!("0x{offset:X}"));
                }
                FreezeSource::RawAddr => {
                    row["selector"] = json!(null);
                }
            }
            row
        })
        .collect();
    Ok(json!({"count": entries.len(), "freezes": entries}))
}

fn parse_addr(s: &str) -> Result<usize, String> {
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    usize::from_str_radix(hex, 16).map_err(|e| format!("bad address '{s}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Property: every numeric Ty round-trips through
    // `Val::from_json` -> `Val::bytes` so the scanner's
    // "compare against user-supplied value" path can't
    // truncate or sign-extend the input incorrectly. The
    // byte length must match `Ty::size`; the decoded bytes
    // must match `value.to_le_bytes()` for the corresponding
    // primitive.
    proptest::proptest! {
        #[test]
        fn val_u8_round_trips(v in proptest::prelude::any::<u8>()) {
            let bits = Val::from_json(&json!(v), Ty::U8).unwrap().bytes();
            assert_eq!(bits, vec![v]);
        }
        #[test]
        fn val_i8_round_trips(v in proptest::prelude::any::<i8>()) {
            let bits = Val::from_json(&json!(v), Ty::I8).unwrap().bytes();
            assert_eq!(bits, vec![v as u8]);
        }
        #[test]
        fn val_u32_round_trips(v in proptest::prelude::any::<u32>()) {
            let bits = Val::from_json(&json!(v), Ty::U32).unwrap().bytes();
            assert_eq!(bits, v.to_le_bytes().to_vec());
        }
        #[test]
        fn val_i32_round_trips(v in proptest::prelude::any::<i32>()) {
            let bits = Val::from_json(&json!(v), Ty::I32).unwrap().bytes();
            assert_eq!(bits, (v as u32).to_le_bytes().to_vec());
        }
        #[test]
        fn val_u64_round_trips(v in proptest::prelude::any::<u64>()) {
            let bits = Val::from_json(&json!(v), Ty::U64).unwrap().bytes();
            assert_eq!(bits, v.to_le_bytes().to_vec());
        }
        #[test]
        fn val_i64_round_trips(v in proptest::prelude::any::<i64>()) {
            let bits = Val::from_json(&json!(v), Ty::I64).unwrap().bytes();
            assert_eq!(bits, (v as u64).to_le_bytes().to_vec());
        }
        #[test]
        fn val_f32_round_trips(v in proptest::prelude::any::<f32>()) {
            // NaN payloads survive: bits comparison, not value
            // equality, lets us assert across NaN.
            let bits = Val::from_json(&json!(v as f64), Ty::F32).unwrap().bytes();
            assert_eq!(bits, v.to_bits().to_le_bytes().to_vec());
        }
        #[test]
        fn val_f64_round_trips(v in proptest::prelude::any::<f64>()) {
            let bits = Val::from_json(&json!(v), Ty::F64).unwrap().bytes();
            assert_eq!(bits, v.to_bits().to_le_bytes().to_vec());
        }
    }

    #[test]
    fn val_rejects_out_of_range_u8() {
        let err = Val::from_json(&json!(256u32), Ty::U8).unwrap_err();
        assert!(err.contains("u8"));
    }

    #[test]
    fn val_rejects_negative_for_unsigned() {
        let err = Val::from_json(&json!(-1i32), Ty::U32).unwrap_err();
        assert!(err.contains("u32"));
    }

    #[test]
    fn parse_addr_accepts_hex_with_prefix() {
        assert_eq!(parse_addr("0xDEAD").unwrap(), 0xDEAD);
        assert_eq!(parse_addr("0XBEEF").unwrap(), 0xBEEF);
        assert_eq!(parse_addr("CAFE").unwrap(), 0xCAFE);
    }

    #[test]
    fn parse_addr_rejects_garbage() {
        assert!(parse_addr("zzz").is_err());
    }
}
