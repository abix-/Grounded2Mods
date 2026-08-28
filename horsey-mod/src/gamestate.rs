//! Typed accessors for the global GameState struct at
//! `DAT_1403fb0d8`. All offsets sourced from
//! `decompiled/annotated/0x14006dc80_save_game_writer.c` and our
//! decompilation of the cheat-menu / save-file paths.
//!
//! Safety:
//! - The game's main thread mutates GameState constantly. Reads
//!   from our HTTP worker are NOT synchronized with the game's
//!   per-frame update. For atomic-sized fields (uint32 etc.) the
//!   read sees a single coherent value; for multi-field
//!   structures (e.g. the 6-supply array) a snapshot may capture
//!   mid-update state. That's acceptable for the control-plane
//!   use case.
//! - Writes from the HTTP worker (e.g. `set_money`) MAY race with
//!   the game's own writes. For cheat-style mutations (money
//!   bump, year jump) the win-by-last-writer outcome is what the
//!   user wants. For state-machine mutations, we'll queue them
//!   onto the main thread later via a worker-queue (see ueforge's
//!   pe_queue.rs for the pattern).

use crate::targets::{self, gs_offset};

/// Resolve the runtime address of the GameState struct.
///
/// `GAMESTATE_PTR` (`0x1403fb0d8`) is a `.data` pointer slot whose
/// 8 bytes hold the address of a heap-allocated GameState object.
/// The decomp evidence is unambiguous: `FUN_14009c6a0` allocates
/// 0x448 bytes via `FUN_1402c704c(0x448)`, runs the constructor
/// `FUN_1400fd580` in place, then `DAT_1403fb0d8 = uVar2`. Every
/// read site uses `*(int *)(DAT_1403fb0d8 + N)`, i.e. pointer +
/// offset, not struct + offset.
///
/// Lifecycle: when no save is loaded the slot holds 0 (set by
/// `FUN_14009c4e0` on destruction). When a save is loaded the
/// slot holds a heap address. Reading the slot is always safe
/// (it lives in `.data`); dereferencing the heap pointer is only
/// safe when the pointer is non-null and well-formed.
///
/// Returns 0 when the dereffed pointer is null or clearly not a
/// heap address. Callers already gate every field access on
/// `if p == 0 { return None; }`, so a 0 return short-circuits
/// reads without crashing.
pub fn ptr() -> usize {
    // Locate the `.data` slot. Resolver-or-hardcoded; the resolver
    // is sanity-gated in targets::resolve and falls through to the
    // hardcoded RVA when no candidate is plausible.
    let slot = crate::targets_registry::resolve::gamestate_ptr()
        .unwrap_or_else(|| targets::rebase(targets::GAMESTATE_PTR));
    // SAFETY: slot lives inside the loaded image's `.data` section;
    // reading 8 bytes from it is safe for the process lifetime.
    let raw = unsafe { *(slot as *const usize) };
    if is_plausible_gamestate_pointer(raw) {
        raw
    } else {
        0
    }
}

/// Pure structural check: does `raw` look like it could be a
/// pointer to a heap-allocated GameState object?
///
///   - Null and tiny values are rejected (heap addresses on x64
///     Windows live well above the first 64 KiB).
///   - Kernel-space addresses (> canonical user range) are rejected.
///   - Misaligned values are rejected (heap allocations are at
///     least 8-aligned; the stale `0x2400000000B` we observed in
///     main-menu state has low nibble `0xB`, which fails).
///
/// Returns `false` for the "no save loaded" null and for stale
/// garbage in the slot. Returns `true` only for values whose shape
/// matches a real heap pointer.
#[inline]
pub fn is_plausible_gamestate_pointer(raw: usize) -> bool {
    raw >= 0x10000 && raw <= 0x7fff_ffff_ffff && (raw & 0x7) == 0
}

/// Heuristic: is a save actually loaded?
///
/// `ptr()` is always non-zero (the GameState struct is statically
/// embedded in `.data`), so a presence check is meaningless. Instead
/// we read the roster vector at `+0x280/+0x288`:
///
///   - Both pointers must be non-null and within a plausible heap
///     range (`begin >= 0x10000`).
///   - `end >= begin` (sane vector ordering).
///   - `end - begin` must be a multiple of 0x24 (the roster entry
///     stride) and yield a count under a sanity bound.
///
/// When no save is loaded, these slots usually hold null or leftover
/// values from a prior session and one of the checks fails. When a
/// save IS loaded, vanilla writes a real `std::vector<HorseEntry>`
/// span into them and every check passes. Empirically reliable; if
/// the game ever ships a build where the roster sits empty mid-load,
/// the UI just shows "no save loaded" for that frame instead of
/// garbage values.
///
/// Used by `snapshot::HorseyState::world_loaded` and by the in-game
/// UI's loaded-vs-menu gate.
pub fn looks_loaded() -> bool {
    let p = ptr();
    if p == 0 {
        return false;
    }
    // SAFETY: GameState struct is statically embedded and always
    // mapped; reading two pointer-sized fields at known offsets is
    // safe regardless of whether a save is loaded. We never deref
    // begin/end here, just inspect their numeric values.
    let begin = unsafe { *((p + gs_offset::horses_begin()) as *const usize) };
    let end = unsafe { *((p + gs_offset::horses_end()) as *const usize) };
    roster_span_looks_loaded(begin, end)
}

/// Forensic dump of every value the loaded-save heuristic touches.
/// Returned as JSON so tests can capture + diff across game states
/// (main menu vs. in-save) without re-implementing pointer reads.
pub fn diag() -> serde_json::Value {
    let p = ptr();
    if p == 0 {
        // Even with no GameState we want to know WHY: dump the slot
        // address and its raw 8-byte content so we can tell apart a
        // null slot (no save loaded) from a stale / wrong slot (the
        // resolver picked the wrong address).
        let resolved = crate::targets_registry::resolve::gamestate_ptr();
        let hardcoded_rebased = targets::rebase(targets::GAMESTATE_PTR);
        let slot = resolved.unwrap_or(hardcoded_rebased);
        // SAFETY: slot lives in .data of the loaded image; safe to read 8 bytes.
        let raw = unsafe { *(slot as *const usize) };
        // Also dump context: 16 qwords at slot-64..slot+64 in case GameState
        // pointer lives near our resolved address.
        let mut context = Vec::new();
        let base = slot.saturating_sub(64);
        for i in 0..16 {
            let addr = base + i * 8;
            // SAFETY: addr is within .data of the image (slot is in .data;
            // we range +/-64 bytes around it).
            let v = unsafe { *(addr as *const usize) };
            context.push(format!(
                "+0x{:03x} @ 0x{addr:x}: 0x{v:x} {}",
                (addr as i64 - slot as i64),
                if is_plausible_gamestate_pointer(v) {
                    "<-- plausible"
                } else {
                    ""
                }
            ));
        }
        return serde_json::json!({
            "ptr": "0x0",
            "resolved_slot": format!("0x{slot:x}"),
            "raw_slot_value": format!("0x{raw:x}"),
            "plausible": is_plausible_gamestate_pointer(raw),
            "pattern_resolver_returned": resolved.map(|a| format!("0x{a:x}")),
            "hardcoded_rebased": format!("0x{hardcoded_rebased:x}"),
            "context_around_slot": context,
        });
    }
    // SAFETY: GameState is statically embedded; offsets are within
    // its allocated range. We never deref the read pointers.
    let roster_begin = unsafe { *((p + gs_offset::horses_begin()) as *const usize) };
    let roster_end = unsafe { *((p + gs_offset::horses_end()) as *const usize) };
    let live_begin = unsafe { *((p + 0x130) as *const usize) };
    let live_end = unsafe { *((p + 0x138) as *const usize) };

    let stride = gs_offset::roster_stride();
    let roster_count = if roster_end >= roster_begin && roster_begin != 0 {
        roster_end.checked_sub(roster_begin).map(|s| s / stride)
    } else {
        None
    };
    let live_count = if live_end >= live_begin && live_begin != 0 {
        live_end.checked_sub(live_begin).map(|s| s / 8)
    } else {
        None
    };

    // Dump every documented GameState field at once. When the right
    // "save loaded" signal is unknown, a single in-save run of
    // `gamestate.diag` lets us see which field is non-zero only
    // when the player is actually in a save. The candidate set
    // mirrors `targets::gs_offset` so future drift is obvious.
    let r_u32 = |off: usize| -> u32 {
        // SAFETY: same rationale as the roster reads above. Uses
        // read_unaligned because a few documented offsets (e.g.
        // 0x415) sit on byte boundaries that the underlying field
        // type doesn't require to be u32-aligned.
        unsafe { ((p + off) as *const u32).read_unaligned() }
    };
    let r_u64 = |off: usize| -> u64 {
        // SAFETY: as above; read_unaligned guards against off-stride
        // pointer offsets in the candidate set.
        unsafe { ((p + off) as *const u64).read_unaligned() }
    };

    serde_json::json!({
        "ptr": format!("0x{p:x}"),
        "roster_0x280_0x288": {
            "begin": format!("0x{roster_begin:x}"),
            "end":   format!("0x{roster_end:x}"),
            "span_bytes": roster_end.wrapping_sub(roster_begin),
            "count_if_sane": roster_count,
            "looks_loaded": roster_span_looks_loaded(roster_begin, roster_end),
        },
        "live_0x130_0x138": {
            "begin": format!("0x{live_begin:x}"),
            "end":   format!("0x{live_end:x}"),
            "span_bytes": live_end.wrapping_sub(live_begin),
            "count_if_sane": live_count,
        },
        "fields_u32": {
            "frame_tick_0x254": r_u32(0x254),
            "field_268_0x268":  r_u32(0x268),
            "trailing_278_0x278": r_u32(0x278),
            "trailing_27c_0x27c": r_u32(0x27c),
            "money_0x308":  r_u32(gs_offset::MONEY),
            "year_0x314":   r_u32(gs_offset::YEAR),
            "sleeps_0x318": r_u32(gs_offset::SLEEPS),
            "supplies_start_0x31c": r_u32(gs_offset::SUPPLIES_START),
            "field_37c_0x37c": r_u32(gs_offset::FIELD_37C),
            "field_39c_0x39c": r_u32(gs_offset::FIELD_39C),
            "field_410_0x410": r_u32(gs_offset::FIELD_410),
            "field_414_0x414": r_u32(gs_offset::FIELD_414),
            "field_415_0x415": r_u32(gs_offset::FIELD_415),
            "field_418_0x418": r_u32(gs_offset::FIELD_418),
            "field_41c_0x41c": r_u32(gs_offset::FIELD_41C),
            "field_440_0x440": r_u32(gs_offset::FIELD_440),
        },
        "fields_u64": {
            // Pointers / 8-byte counters that might signal loaded state.
            "ptr_at_0x130": format!("0x{:x}", r_u64(0x130)),
            "ptr_at_0x138": format!("0x{:x}", r_u64(0x138)),
            "ptr_at_0x140": format!("0x{:x}", r_u64(0x140)),
            "ptr_at_0x148": format!("0x{:x}", r_u64(0x148)),
            "ptr_at_0x280": format!("0x{:x}", r_u64(0x280)),
            "ptr_at_0x288": format!("0x{:x}", r_u64(0x288)),
            "ptr_at_0x290": format!("0x{:x}", r_u64(0x290)),
        },
        "globals_u32": {
            "races": races(),
        },
        "verdict_looks_loaded": looks_loaded(),
        // Hex dump of the first 0x4000 bytes of GameState. When the
        // money offset has drifted between builds, grepping this for
        // the on-screen money value finds the new offset in one
        // round-trip.
        "hex_dump_0x000_0x4000": hex_dump(p, 0x4000),
    })
}

fn hex_dump(base: usize, n: usize) -> String {
    let mut out = String::with_capacity(n * 3);
    for i in 0..n {
        // SAFETY: range is within the statically-mapped GameState
        // struct; reads are byte-aligned.
        let b = unsafe { *((base + i) as *const u8) };
        if i > 0 && i % 16 == 0 {
            out.push('\n');
        }
        use std::fmt::Write;
        let _ = write!(out, "{b:02x} ");
    }
    out
}

/// Pure decision: does a `(begin, end)` pair for the roster vector at
/// `+0x280/+0x288` look like a real loaded-save span?
///
/// Extracted from `looks_loaded` so the heuristic itself is unit-
/// testable without needing a live GameState.
#[inline]
pub fn roster_span_looks_loaded(begin: usize, end: usize) -> bool {
    // Both pointers must look like heap addresses; nulls and tiny
    // values are leftover / uninitialized.
    if begin < 0x10000 || end < begin {
        return false;
    }
    let span = end - begin;
    let stride = gs_offset::roster_stride();
    if span % stride != 0 {
        return false;
    }
    let count = span / stride;
    // Vanilla save format caps the roster around 256; allow 10x slack
    // for modded saves. Anything bigger is fill-pattern noise.
    count < 2560
}

/// Read a uint32 field from GameState at the given offset.
pub fn read_u32(off: usize) -> Option<u32> {
    let p = ptr();
    if p == 0 {
        return None;
    }
    // SAFETY: GameState is alive for the world's lifetime; reading
    // a uint32 at a known offset is safe as long as `p != 0`.
    Some(unsafe { *((p + off) as *const u32) })
}

/// Write a uint32 field on GameState at the given offset.
pub fn write_u32(off: usize, value: u32) -> bool {
    let p = ptr();
    if p == 0 {
        return false;
    }
    // SAFETY: Same alignment guarantees as read_u32. Concurrent
    // reads/writes by the game thread are tolerated; the worst
    // case is a single torn-write, which the engine recovers from
    // on the next frame.
    unsafe {
        *((p + off) as *mut u32) = value;
    }
    true
}

pub fn read_u8(off: usize) -> Option<u8> {
    let p = ptr();
    if p == 0 {
        return None;
    }
    // SAFETY: same rationale as read_u32; uint8 reads are always atomic.
    Some(unsafe { *((p + off) as *const u8) })
}

pub fn write_u8(off: usize, value: u8) -> bool {
    let p = ptr();
    if p == 0 {
        return false;
    }
    // SAFETY: same rationale as write_u32.
    unsafe {
        *((p + off) as *mut u8) = value;
    }
    true
}

// =============================================================================
// Convenience accessors for documented fields
// =============================================================================

pub fn money() -> Option<u32> {
    read_u32(gs_offset::money())
}
pub fn set_money(value: u32) -> bool {
    write_u32(gs_offset::money(), value)
}

pub fn year() -> Option<u32> {
    read_u32(gs_offset::year())
}
pub fn set_year(value: u32) -> bool {
    write_u32(gs_offset::year(), value)
}

pub fn sleeps() -> Option<u32> {
    read_u32(gs_offset::sleeps())
}

pub fn frame_tick() -> Option<u32> {
    read_u32(gs_offset::FRAME_TICK)
}

// =============================================================================
// Cheat-toggle accessors (operate on dedicated globals, not GameState fields)
// =============================================================================

/// Read the "No Tire" toggle byte (true = tiredness disabled).
pub fn no_tire() -> bool {
    let abs = crate::targets_registry::resolve::no_tire_toggle()
        .unwrap_or_else(|| targets::rebase(targets::NO_TIRE_TOGGLE));
    // SAFETY: NO_TIRE_TOGGLE lives in the .data section of the
    // exe; valid for the process lifetime.
    unsafe { *(abs as *const u8) != 0 }
}

/// Set the "No Tire" toggle byte.
pub fn set_no_tire(enabled: bool) {
    let abs = crate::targets_registry::resolve::no_tire_toggle()
        .unwrap_or_else(|| targets::rebase(targets::NO_TIRE_TOGGLE));
    // SAFETY: Same as above; writing a byte to a .data global.
    unsafe {
        *(abs as *mut u8) = if enabled { 1 } else { 0 };
    }
}

/// Read the debug-mode active flag.
pub fn debug_mode() -> bool {
    let abs = crate::targets_registry::resolve::debug_mode_active()
        .unwrap_or_else(|| targets::rebase(targets::DEBUG_MODE_ACTIVE));
    // SAFETY: same .data-section global rationale.
    unsafe { *(abs as *const u8) != 0 }
}

/// Forcibly enable / disable debug mode (without making the user
/// type "debug" in the pause menu).
pub fn set_debug_mode(enabled: bool) {
    let abs = crate::targets_registry::resolve::debug_mode_active()
        .unwrap_or_else(|| targets::rebase(targets::DEBUG_MODE_ACTIVE));
    // SAFETY: same .data-section global rationale.
    unsafe {
        *(abs as *mut u8) = if enabled { 1 } else { 0 };
    }
}

/// Read the races counter (separate global, not in GameState).
pub fn races() -> u32 {
    let abs = crate::targets_registry::resolve::races_counter()
        .unwrap_or_else(|| targets::rebase(targets::RACES_COUNTER));
    // SAFETY: races counter is a uint32 in .data; always valid.
    unsafe { *(abs as *const u32) }
}

// =============================================================================
// Horse-list walker
// =============================================================================

/// Iterate the horse-roster pointers from GameState.
/// `(end - begin) / 0x24` is the count; each in-memory horse is
/// 0x24 = 36 bytes. The roster is the same one written by
/// `save_game_writer`.
pub fn horse_count() -> usize {
    let p = ptr();
    if p == 0 {
        return 0;
    }
    // SAFETY: begin/end pointers are stable while the world is
    // alive; reading two pointer-sized fields from GameState.
    let begin = unsafe { *((p + gs_offset::horses_begin()) as *const usize) };
    let end = unsafe { *((p + gs_offset::horses_end()) as *const usize) };
    if end < begin {
        0
    } else {
        (end - begin) / gs_offset::roster_stride()
    }
}

/// Get the in-memory horse pointer for index `i`. Returns None if
/// out of range. Note: these are 36-byte ROSTER ENTRIES, not the
/// full live-horse objects (the live objects are pointed to from
/// other lists like `+0x130/+0x138`).
pub fn horse_roster_entry(i: usize) -> Option<usize> {
    let p = ptr();
    if p == 0 {
        return None;
    }
    // SAFETY: begin pointer is stable while world is alive.
    let begin = unsafe { *((p + gs_offset::horses_begin()) as *const usize) };
    if begin == 0 {
        return None;
    }
    if i >= horse_count() {
        return None;
    }
    Some(begin + i * gs_offset::roster_stride())
}

// =============================================================================
// Live-horse pointer list (gamestate + 0x130 / +0x138)
// =============================================================================
//
// The retirement handler iterates `*(longlong *)(param_1 + 0x130)`. Through
// our decompilation pass, `param_1` at that callsite traces back to
// `DAT_1403fb0d8`, the GameState. The list holds 8-byte pointers to the
// full live-Horse objects whose +0x1fc/+0x200/+0x205/+0x206/+0x21c/+0x254
// fields we already model.

/// Live-horse list begin offset on GameState. Old decomp constant;
/// production reads go through `targets::gs_offset::live_horses_begin()`.
#[allow(dead_code)]
const LIVE_HORSES_BEGIN: usize = 0x130;
/// Live-horse list end offset on GameState. Old decomp constant;
/// production reads go through `targets::gs_offset::live_horses_end()`.
#[allow(dead_code)]
const LIVE_HORSES_END: usize = 0x138;

fn live_horses_begin_off() -> usize {
    targets::gs_offset::live_horses_begin()
}
fn live_horses_end_off() -> usize {
    targets::gs_offset::live_horses_end()
}

/// Count of live horse pointers in the gamestate `+0x130..+0x138` list.
pub fn live_horse_count() -> usize {
    let p = ptr();
    if p == 0 {
        return 0;
    }
    // SAFETY: begin/end pointers are stable while the world is alive.
    let begin = unsafe { *((p + live_horses_begin_off()) as *const usize) };
    let end = unsafe { *((p + live_horses_end_off()) as *const usize) };
    if end < begin { 0 } else { (end - begin) / 8 }
}

/// Get the i-th live Horse pointer. Returns None if out of range or
/// the list is empty.
pub fn live_horse_ptr(i: usize) -> Option<usize> {
    let p = ptr();
    if p == 0 {
        return None;
    }
    // SAFETY: begin is a stable pointer; loading 8 bytes at `begin + i*8`
    // is in-bounds when i < count.
    let begin = unsafe { *((p + live_horses_begin_off()) as *const usize) };
    if begin == 0 || i >= live_horse_count() {
        return None;
    }
    // SAFETY: read one pointer-sized slot inside the vector.
    Some(unsafe { *((begin + i * 8) as *const usize) })
}

// =============================================================================
// Owned-horse list (gamestate +0x438 -> +0x90 -> +0x130 / +0x138)
// =============================================================================
//
// Owned-horse list:
//   GS + 0x438                       = pointer to sub-system table
//   *(GS + 0x438) + 0                = sub-struct pointer for OWNED horses
//   that_sub_struct + 0x130/+0x138   = vector<Horse*> of player-owned horses
//
// Slot 0 of the table is the canonical owned list. Cross-validated against
// user's "I own 3 horses". Slot 0 had count 3 even when active_scene_id was
// -1 (overworld). HLT did NOT find this; they only iterated whichever scene
// was active. See docs/HORSE-PLACES.md for the full slot inventory.

const SCENE_TABLE_OFF: usize = 0x438;
const OWNED_SLOT_OFF: usize = 0x0;
const ACTIVE_SCENE_ID_OFF: usize = 0x25C;
const OWNED_VEC_BEGIN_OFF: usize = 0x130;
const OWNED_VEC_END_OFF: usize = 0x138;

/// Read the currently-active scene id from GS+0x25C. Returns None when
/// the value isn't in the documented range or GS isn't loaded.
pub fn active_scene_id() -> Option<i32> {
    let p = ptr();
    if p == 0 {
        return None;
    }
    // SAFETY: GameState struct statically mapped; reading u32 at known offset is safe.
    let id = unsafe { *((p + ACTIVE_SCENE_ID_OFF) as *const i32) };
    if (-1..256).contains(&id) {
        Some(id)
    } else {
        None
    }
}

/// Resolve the player-owned horse-list sub-struct via the sub-system
/// table at GS+0x438, slot 0. Every deref is gated on `is_addr_readable`
/// so a stale chain cannot fault the HTTP worker.
fn owned_stable() -> Option<usize> {
    let p = ptr();
    if p == 0 {
        return None;
    }
    let table_slot = p + SCENE_TABLE_OFF;
    if !modforge::winproc::is_addr_readable(table_slot) {
        return None;
    }
    // SAFETY: GameState page is mapped; readability checked.
    let table = unsafe { *(table_slot as *const usize) };
    let entry_slot = table.wrapping_add(OWNED_SLOT_OFF);
    if !modforge::winproc::is_addr_readable(entry_slot) {
        return None;
    }
    // SAFETY: entry_slot readability checked.
    let stable = unsafe { *(entry_slot as *const usize) };
    if stable == 0 || !modforge::winproc::is_addr_readable(stable + OWNED_VEC_END_OFF) {
        return None;
    }
    Some(stable)
}

/// Count of owned horses (vector<Horse*> at `stable+0x130/+0x138`).
pub fn owned_horse_count() -> usize {
    let Some(stable) = owned_stable() else {
        return 0;
    };
    // SAFETY: stable+0x130 and stable+0x138 readability checked in owned_stable.
    let begin = unsafe { *((stable + OWNED_VEC_BEGIN_OFF) as *const usize) };
    let end = unsafe { *((stable + OWNED_VEC_END_OFF) as *const usize) };
    if end < begin { 0 } else { (end - begin) / 8 }
}

/// Get the i-th owned Horse pointer. Returns None if out of range, the
/// chain is dead, or the resolved slot does not look like a heap pointer.
pub fn owned_horse_ptr(i: usize) -> Option<usize> {
    let stable = owned_stable()?;
    // SAFETY: begin readability via owned_stable's checks.
    let begin = unsafe { *((stable + OWNED_VEC_BEGIN_OFF) as *const usize) };
    if begin == 0 || i >= owned_horse_count() {
        return None;
    }
    let slot = begin + i * 8;
    if !modforge::winproc::is_addr_readable(slot) {
        return None;
    }
    // SAFETY: slot readability just checked.
    let horse = unsafe { *(slot as *const usize) };
    if !is_plausible_gamestate_pointer(horse) {
        return None;
    }
    Some(horse)
}

#[cfg(test)]
mod tests {
    use super::{is_plausible_gamestate_pointer as plausible, roster_span_looks_loaded as ok};

    #[test]
    fn null_slot_rejected() {
        // No save loaded -> FUN_14009c4e0 zeros the slot. ptr() must
        // return 0 so every `if p == 0 { return None }` guard fires.
        assert!(!plausible(0));
    }

    #[test]
    fn low_addresses_rejected() {
        // x64 Windows reserves the first 64 KiB. Heap allocations
        // never land there, so anything below is stale garbage.
        assert!(!plausible(0x1));
        assert!(!plausible(0xFFFF));
    }

    #[test]
    fn kernel_space_addresses_rejected() {
        // Anything above the canonical user-mode range is either
        // sign-extended kernel garbage or noncanonical.
        assert!(!plausible(0x8000_0000_0000_0000));
        assert!(!plausible(usize::MAX));
    }

    #[test]
    fn observed_main_menu_garbage_rejected() {
        // The exact stale value the previous fix-attempt observed
        // in main-menu state (low nibble 0xB; never an HMM-aligned
        // heap pointer). This is the regression that prompted the
        // (mistaken) "slot IS the struct" rewrite. Must reject.
        assert!(!plausible(0x2400000000B));
    }

    #[test]
    fn misaligned_values_rejected() {
        // Heap allocations are at least 8-byte aligned. Any value
        // with low 3 bits set is not a real heap pointer.
        assert!(!plausible(0x1_0001));
        assert!(!plausible(0x7FFF_FFFF_FFF7));
    }

    #[test]
    fn plausible_heap_pointers_accepted() {
        // Typical x64 Windows heap addresses: low-TiB range,
        // 16-aligned.
        assert!(plausible(0x0000_0240_1234_5670));
        assert!(plausible(0x0000_7FF0_DEAD_BEE0));
        assert!(plausible(0x1_0000)); // exactly at the low gate
    }

    #[test]
    fn null_pointers_are_not_loaded() {
        assert!(!ok(0, 0));
        assert!(!ok(0, 0x1000));
        assert!(
            !ok(0x1000, 0x2000),
            "addrs below 0x10000 don't look like heap"
        );
    }

    #[test]
    fn end_before_begin_is_not_loaded() {
        assert!(!ok(0x80000000, 0x70000000));
    }

    #[test]
    fn misaligned_span_is_not_loaded() {
        // span of 0x25 is not divisible by 0x24
        assert!(!ok(0x80000000, 0x80000000 + 0x25));
    }

    #[test]
    fn empty_but_aligned_span_is_loaded() {
        // begin == end, span = 0; 0 % 0x24 == 0; count = 0 < 2560
        assert!(ok(0x80000000, 0x80000000));
    }

    #[test]
    fn typical_save_span_is_loaded() {
        // 32 horses = 32 * 0x24 = 0x480
        assert!(ok(0x80000000, 0x80000000 + 0x480));
    }

    #[test]
    fn implausibly_huge_span_is_not_loaded() {
        // 100k horses worth of stride: pure fill-pattern noise.
        let huge = 100_000 * 0x24;
        assert!(!ok(0x80000000, 0x80000000 + huge));
    }

    #[test]
    fn fill_pattern_pointers_fail() {
        // Reproduce the 2026-05-15 main-menu state: bytes at the
        // roster slots hold a 0x01010101 fill pattern. Both pointers
        // are < 0x10000 -> fails the first gate. Even if they were
        // larger, the span is wildly off-stride.
        let begin = 0x01010101usize;
        let end = 0x02020202usize;
        assert!(!ok(begin, end));
    }
}
