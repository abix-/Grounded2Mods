//! HK1 horse-transfer plumbing.
//!
//! Approach: don't write Horse state directly (the +0x1d0 field is a
//! downstream display value; writing it doesn't move the horse). Instead,
//! invoke the game's own drop-commit method from the Home Location's
//! vtable (`vtable[+0x78]` = `FUN_1400de2e0`, resolved at runtime). That
//! function reads `LOC[0x29]` (the currently-grabbed horse pointer) plus
//! `LOC[0x174]/+0x178` (cursor position) and commits a drop at the
//! cursor's hit-tested target.
//!
//! Flow per transfer:
//!   1. Set `LOC[0x29] = horse_ptr` (mark horse as grabbed).
//!   2. Set `LOC[0x174] = target_x`, `LOC[0x178] = target_y` (calibrated
//!      cursor position over the target container).
//!   3. Call `vtable[+0x78](LOC)`. Returns non-zero on successful drop.
//!
//! Targets are calibrated once by the operator: they move the in-game
//! cursor over each container area and click "Save TRUCK" / "Save
//! PASTURE". The captured (x, y) pair gets persisted to disk.
//!
//! Resolved at runtime, not hardcoded:
//!   - Home Location vtable RVA `0x30f3d0` (from
//!     `hk1.probe.scene_slot_vtables`).
//!   - Drop-commit slot offset `+0x78` (from the decomp at
//!     `FUN_1400d2ab0` line 1722).
//!
//! All side-effects (audio, animation, container update) come for free
//! because the game's own commit method runs.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Home Location class vtable RVA. Slot 0x00 of the scene table and
/// the race-lane slots 0x08..0x38 all share this vtable.
pub const HOME_LOC_VTABLE_RVA: usize = 0x30f3d0;

/// Vtable offset for the drop-commit method (param_1.vtable[+0x78]).
/// Confirmed via FUN_1400d2ab0:1722.
pub const VTABLE_DROP_COMMIT: usize = 0x78;

/// Location field offsets on the shared Location class.
pub const LOC_HORSE_VEC_BEGIN: usize = 0x130; // ptr to first Horse* (= LOC[0x26])
pub const LOC_HORSE_VEC_END: usize = 0x138; // one-past-last (= LOC[0x27])
pub const LOC_GRABBED_HORSE: usize = 0x29 * 8; // = 0x148 (LOC[0x29])
pub const LOC_DRAG_IDX: usize = 0x2d * 8; // = 0x168 (LOC[0x2d])
pub const LOC_CURSOR_X: usize = 0x174; // float
pub const LOC_CURSOR_Y: usize = 0x178; // float
pub const LOC_CLICK_STATE: usize = 0x37 * 8; // = 0x1b8 (LOC[0x37])

/// Click-state value the drop-commit branch keys on. Observed via the
/// snap2/snap3 drag diff (the game wrote 0 to +0x1b8 after the drop,
/// implying it was 1 in the trigger frame).
pub const CLICK_RELEASE: u32 = 1;

fn log_path() -> Option<PathBuf> {
    modforge::log::dll_dir().map(|d| d.join("hk1_overlay.log"))
}

fn calib_path() -> Option<PathBuf> {
    modforge::log::dll_dir().map(|d| d.join("hk1_targets.json"))
}

static LOG_LOCK: Mutex<()> = Mutex::new(());

/// Append one line to `<dll_dir>/hk1_overlay.log`.
pub fn log_line(s: &str) {
    let Some(path) = log_path() else { return };
    let _g = LOG_LOCK.lock().ok();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{ts}] {s}");
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Targets {
    pub truck: Option<(f32, f32)>,
    pub pasture: Option<(f32, f32)>,
}

static TARGETS: Mutex<Option<Targets>> = Mutex::new(None);

/// Load the saved (truck, pasture) cursor positions from
/// `<dll_dir>/hk1_targets.json`. Returns an empty Targets if the file
/// doesn't exist or fails to parse.
pub fn load_targets() -> Targets {
    if let Some(t) = *TARGETS.lock().unwrap() {
        return t;
    }
    let mut t = Targets::default();
    if let Some(path) = calib_path() {
        if let Ok(mut f) = File::open(&path) {
            let mut s = String::new();
            if f.read_to_string(&mut s).is_ok() {
                // hand-rolled parser; trivial schema, no serde dependency
                t.truck = parse_xy(&s, "truck");
                t.pasture = parse_xy(&s, "pasture");
            }
        }
    }
    *TARGETS.lock().unwrap() = Some(t);
    t
}

fn parse_xy(s: &str, key: &str) -> Option<(f32, f32)> {
    let needle = format!("\"{key}\"");
    let idx = s.find(&needle)?;
    let rest = &s[idx + needle.len()..];
    let lbr = rest.find('[')?;
    let rbr = rest.find(']')?;
    let inner = &rest[lbr + 1..rbr];
    let mut nums = inner.split(',').map(|p| p.trim().parse::<f32>());
    let x = nums.next()?.ok()?;
    let y = nums.next()?.ok()?;
    Some((x, y))
}

pub fn save_targets_pub(t: Targets) {
    save_targets(t);
}

fn save_targets(t: Targets) {
    let Some(path) = calib_path() else { return };
    let fmt_xy = |v: Option<(f32, f32)>| match v {
        Some((x, y)) => format!("[{x}, {y}]"),
        None => "null".into(),
    };
    let s = format!(
        "{{\"truck\": {}, \"pasture\": {}}}\n",
        fmt_xy(t.truck),
        fmt_xy(t.pasture)
    );
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
    {
        let _ = f.write_all(s.as_bytes());
    }
    *TARGETS.lock().unwrap() = Some(t);
    log_line(&format!(
        "targets saved: truck={:?} pasture={:?}",
        t.truck, t.pasture
    ));
}

/// Read N bytes from `addr` and return as space-separated hex.
fn dump_hex(addr: usize, n: usize) -> Option<String> {
    if n == 0 || !modforge::winproc::is_addr_readable(addr) {
        return None;
    }
    if !modforge::winproc::is_addr_readable(addr + n - 1) {
        return None;
    }
    // SAFETY: both endpoints checked.
    let slice = unsafe { std::slice::from_raw_parts(addr as *const u8, n) };
    Some(
        slice
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Snapshot the Home Location (0x240 bytes) and a horse (0x498 bytes)
/// to the overlay log with a labeled header. Use before AND after any
/// state mutation so the diff in the log file reveals what changed.
pub fn snapshot(label: &str, horse_ptr: usize) {
    let loc = home_loc_ptr();
    let loc_s = loc
        .and_then(|p| dump_hex(p, 0x240))
        .unwrap_or_else(|| "(no home loc)".into());
    let horse_s = dump_hex(horse_ptr, 0x498).unwrap_or_else(|| "(unreadable horse)".into());
    let cursor = read_cursor()
        .map(|(x, y)| format!("({x},{y})"))
        .unwrap_or("?".into());
    let loc_ptr_s = loc.map(|p| format!("0x{p:x}")).unwrap_or("(none)".into());
    log_line(&format!(
        "SNAPSHOT label='{label}' loc_ptr={loc_ptr_s} horse_ptr=0x{horse_ptr:x} cursor={cursor}"
    ));
    log_line(&format!("  LOC[0..0x240]: {loc_s}"));
    log_line(&format!("  HORSE[0..0x498]: {horse_s}"));
}

/// Walk GS+0x438 -> *(arr + 0) (the Home Location object).
pub fn home_loc_ptr() -> Option<usize> {
    let gs = crate::gamestate::ptr();
    if gs == 0 {
        return None;
    }
    if !modforge::winproc::is_addr_readable(gs + 0x438) {
        return None;
    }
    // SAFETY: GS+0x438 just checked.
    let arr_ptr = unsafe { *((gs + 0x438) as *const usize) };
    if !modforge::winproc::is_addr_readable(arr_ptr) {
        return None;
    }
    // SAFETY: arr_ptr just checked.
    let loc = unsafe { *(arr_ptr as *const usize) };
    if loc == 0 || !modforge::winproc::is_addr_readable(loc + 0x300) {
        return None;
    }
    Some(loc)
}

/// Read the cursor coords the click-handler watches (`LOC[0x174]/+0x178`).
pub fn read_cursor() -> Option<(f32, f32)> {
    let loc = home_loc_ptr()?;
    if !modforge::winproc::is_addr_readable(loc + LOC_CURSOR_Y + 3) {
        return None;
    }
    // SAFETY: range just checked.
    let x = unsafe { *((loc + LOC_CURSOR_X) as *const f32) };
    let y = unsafe { *((loc + LOC_CURSOR_Y) as *const f32) };
    Some((x, y))
}

/// Capture the current cursor coords as `which`'s calibration.
pub fn calibrate(which: &str) -> Option<(f32, f32)> {
    let pos = read_cursor()?;
    let mut t = load_targets();
    match which {
        "truck" => t.truck = Some(pos),
        "pasture" => t.pasture = Some(pos),
        _ => return None,
    }
    save_targets(t);
    log_line(&format!("calibrate {which} -> {pos:?}"));
    Some(pos)
}

/// Capture LOC + a horse's bytes at the moment the operator clicks
/// the "Snapshot here" overlay button. Used by the operator to mark
/// before/after states (e.g. before drag in pasture, after drag onto
/// truck) so we can diff them offline.
pub fn snapshot_here(label: &str, horse_ptr: usize) {
    snapshot(label, horse_ptr);
}

/// Read the drop-commit function pointer from the Home Location vtable.
fn resolve_drop_commit_fn() -> Option<usize> {
    let image_base = crate::targets::image_base();
    let slot = image_base + HOME_LOC_VTABLE_RVA + VTABLE_DROP_COMMIT;
    if !modforge::winproc::is_addr_readable(slot + 7) {
        return None;
    }
    // SAFETY: slot+7 just checked; image_base + RVA is in .rdata.
    let fn_addr = unsafe { *(slot as *const usize) };
    if fn_addr == 0 {
        return None;
    }
    Some(fn_addr)
}

/// Transfer a horse to a calibrated container via the game's own
/// drop-commit method. Returns `Some(result)` from `vtable[+0x78]`
/// (non-zero = drop accepted) or `None` if the call could not be made.
pub fn transfer_horse(horse_ptr: usize, dest: &str) -> Option<u8> {
    let t = load_targets();
    let target = match dest {
        "truck" => t.truck?,
        "pasture" => t.pasture?,
        _ => return None,
    };
    let loc = home_loc_ptr()?;
    let fn_addr = resolve_drop_commit_fn()?;

    // Stash all old values for restore on completion.
    if !modforge::winproc::is_addr_readable(loc + LOC_CLICK_STATE + 3) {
        return None;
    }
    // SAFETY: range checked.
    let old_grabbed = unsafe { *((loc + LOC_GRABBED_HORSE) as *const usize) };
    let old_drag_idx = unsafe { *((loc + LOC_DRAG_IDX) as *const i32) };
    let old_click = unsafe { *((loc + LOC_CLICK_STATE) as *const u32) };
    let old_cx = unsafe { *((loc + LOC_CURSOR_X) as *const f32) };
    let old_cy = unsafe { *((loc + LOC_CURSOR_Y) as *const f32) };

    // Find the horse's index in LOC[0x130/0x138] vector. The drop-commit
    // function probably dereferences LOC.horses[drag_idx] internally,
    // so the index must be valid.
    let vec_begin = unsafe { *((loc + LOC_HORSE_VEC_BEGIN) as *const usize) };
    let vec_end = unsafe { *((loc + LOC_HORSE_VEC_END) as *const usize) };
    let drag_idx = find_horse_index(horse_ptr, vec_begin, vec_end);
    let Some(drag_idx) = drag_idx else {
        log_line(&format!(
            "transfer ABORT horse=0x{horse_ptr:x} not in LOC[0x130/0x138] vec ({vec_begin:#x}..{vec_end:#x})"
        ));
        return None;
    };

    log_line(&format!(
        "transfer dest={dest} horse=0x{horse_ptr:x} idx={drag_idx} loc=0x{loc:x} fn=0x{fn_addr:x} \
         target=({},{}) old_grabbed=0x{old_grabbed:x} old_drag_idx={old_drag_idx} old_click={old_click} \
         old_cursor=({old_cx},{old_cy})",
        target.0, target.1
    ));

    // Stage the full LOC: grabbed horse + drag index + click-release
    // state + cursor over target.
    // SAFETY: writes covered by readability checks above.
    unsafe {
        *((loc + LOC_GRABBED_HORSE) as *mut usize) = horse_ptr;
        *((loc + LOC_DRAG_IDX) as *mut i32) = drag_idx as i32;
        *((loc + LOC_CLICK_STATE) as *mut u32) = CLICK_RELEASE;
        *((loc + LOC_CURSOR_X) as *mut f32) = target.0;
        *((loc + LOC_CURSOR_Y) as *mut f32) = target.1;
    }
    // Invoke vtable[+0x78](LOC, drag_idx, param3) inside an SEH guard.
    // The Ghidra decomp at FUN_1400d2ab0:1722 showed the call as
    // single-arg `(*param_1)`, but disassembling FUN_1400de2e0's
    // prologue reveals it actually takes three args:
    //   rcx = this LOC
    //   rdx = drag_idx (sign-extended, used as horses-vec index)
    //   r8d = param3 (stored to ebx; usage further in body)
    // Confirmed via fault@+0x31 = `mov r15, [rax + r12*8]` where r12
    // is sign-extended rdx. Without passing rdx the function reads
    // register garbage and AVs on the vector deref.
    type DropCommitFn = unsafe extern "system" fn(*mut u8, i32, i32) -> u8;
    // SAFETY: fn_addr was just read from a readable vtable slot in
    // .rdata of the loaded Horsey image.
    let f: DropCommitFn = unsafe { std::mem::transmute(fn_addr) };
    log_line(&format!(
        "staged LOC: calling vtable[+0x78](LOC=0x{loc:x}, drag_idx={drag_idx}, param3=1) under SEH guard"
    ));
    let outcome = modforge::seh::guard(|| {
        // SAFETY: f points to a real game function; we provide the
        // expected (this, drag_idx, param3) triple. SEH catches any
        // remaining bad-state crashes.
        unsafe { f(loc as *mut u8, drag_idx as i32, 1) }
    });
    match outcome {
        Ok(result) => {
            log_line(&format!("vtable returned result={result}"));
            Some(result)
        }
        Err(e) => {
            log_line(&format!(
                "vtable CRASHED but caught: {e}; reverting LOC stage"
            ));
            // SAFETY: same range as the original writes; restoring the
            // pre-call values so the game isn't left half-grabbed.
            unsafe {
                *((loc + LOC_GRABBED_HORSE) as *mut usize) = old_grabbed;
                *((loc + LOC_DRAG_IDX) as *mut i32) = old_drag_idx;
                *((loc + LOC_CLICK_STATE) as *mut u32) = old_click;
                *((loc + LOC_CURSOR_X) as *mut f32) = old_cx;
                *((loc + LOC_CURSOR_Y) as *mut f32) = old_cy;
            }
            None
        }
    }
}

/// Walk a `Horse**` vector in `[begin, end)`; return the index of
/// `horse_ptr` if present.
fn find_horse_index(horse_ptr: usize, begin: usize, end: usize) -> Option<usize> {
    if end < begin {
        return None;
    }
    let count = (end - begin) / 8;
    if count == 0 || count > 0x1000 {
        return None;
    }
    if !modforge::winproc::is_addr_readable(begin) {
        return None;
    }
    if !modforge::winproc::is_addr_readable(end - 1) {
        return None;
    }
    for i in 0..count {
        // SAFETY: range checked above; each elem is 8 bytes aligned.
        let p = unsafe { *((begin + i * 8) as *const usize) };
        if p == horse_ptr {
            return Some(i);
        }
    }
    None
}
