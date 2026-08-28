//! Typed accessors for the Horse struct.
//!
//! "Horse pointer" in different parts of the engine means
//! different things:
//! - The roster entry in GameState.+0x280..+0x288 is a 36-byte
//!   record with name + parents + children (we walked this in
//!   `save_edit.py`).
//! - The full live-horse object (with the +0x1fc age, +0x200
//!   max_age, +0x205/+0x206 tiredness flags, etc.) is what
//!   appears in the track-manager's `+0x130/+0x138` list and is
//!   passed around as the typical "horse" argument.
//!
//! This module deals with the LIVE-HORSE pointer. The roster
//! walker lives in `gamestate.rs`.

use crate::targets::horse_offset;

/// Read horse.age (int32).
pub fn age(horse: usize) -> Option<i32> {
    if horse == 0 {
        return None;
    }
    // SAFETY: Caller passes a valid Horse* obtained from one of
    // the engine's horse lists (e.g. via a hook). The horse is
    // alive for the duration of the read.
    Some(unsafe { *((horse + horse_offset::AGE) as *const i32) })
}

/// Write horse.age. Use with care; if the game's per-frame aging
/// tick is running, this is racy.
pub fn set_age(horse: usize, value: i32) -> bool {
    if horse == 0 {
        return false;
    }
    // SAFETY: same alignment & lifetime guarantees as age().
    unsafe {
        *((horse + horse_offset::AGE) as *mut i32) = value;
    }
    true
}

pub fn max_age(horse: usize) -> Option<i32> {
    if horse == 0 {
        return None;
    }
    // SAFETY: same as age().
    Some(unsafe { *((horse + horse_offset::MAX_AGE) as *const i32) })
}

pub fn set_max_age(horse: usize, value: i32) -> bool {
    if horse == 0 {
        return false;
    }
    // SAFETY: same as set_age().
    unsafe {
        *((horse + horse_offset::MAX_AGE) as *mut i32) = value;
    }
    true
}

pub fn skill(horse: usize) -> Option<i32> {
    if horse == 0 {
        return None;
    }
    // SAFETY: same as age().
    Some(unsafe { *((horse + horse_offset::SKILL) as *const i32) })
}

pub fn set_skill(horse: usize, value: i32) -> bool {
    if horse == 0 {
        return false;
    }
    // SAFETY: same as set_age().
    unsafe {
        *((horse + horse_offset::SKILL) as *mut i32) = value;
    }
    true
}

pub fn tired_a(horse: usize) -> Option<u8> {
    if horse == 0 {
        return None;
    }
    // SAFETY: byte read at known offset; horse is alive.
    Some(unsafe { *((horse + horse_offset::TIRED_FLAG_A) as *const u8) })
}

pub fn tired_b(horse: usize) -> Option<u8> {
    if horse == 0 {
        return None;
    }
    // SAFETY: same as tired_a.
    Some(unsafe { *((horse + horse_offset::TIRED_FLAG_B) as *const u8) })
}

/// Force-clear tiredness on this horse. Note: if Yes Tire mode is
/// on, the engine clears these every frame anyway.
pub fn clear_tiredness(horse: usize) -> bool {
    if horse == 0 {
        return false;
    }
    // SAFETY: same as set_age(); writes two bytes on a known struct.
    unsafe {
        *((horse + horse_offset::TIRED_FLAG_A) as *mut u8) = 0;
        *((horse + horse_offset::TIRED_FLAG_B) as *mut u8) = 0;
    }
    true
}

pub fn species(horse: usize) -> Option<i32> {
    if horse == 0 {
        return None;
    }
    // SAFETY: int32 read at +0x1c.
    Some(unsafe { *((horse + horse_offset::type_or_species()) as *const i32) })
}

/// Read the per-horse container-kind cache at +0x1d0 (u32).
/// 7 = trailer, 9 = pasture, 0 or 2 right after a save/reload (see
/// `targets::horse_offset::CONTAINER_KIND`). This is the READ
/// hypothesis for trailer-vs-pasture; its reliability is verified by
/// the live manual-drag watch, not assumed here.
pub fn container_kind(horse: usize) -> Option<u32> {
    if horse == 0 {
        return None;
    }
    // SAFETY: u32 read at a known offset; horse is alive.
    Some(unsafe { *((horse + horse_offset::CONTAINER_KIND) as *const u32) })
}

/// Read the per-horse scene placement position (x at +0x1d4, y at
/// +0x1d8). Live-confirmed trailer-determining field: trailer horses
/// carry their trailer position (~13, 9), pasture horses read (0, 0) on
/// the overworld. See `targets::horse_offset::SCENE_POS_X`.
pub fn scene_pos(horse: usize) -> Option<(f32, f32)> {
    if horse == 0 {
        return None;
    }
    // SAFETY: two f32 reads at known offsets; horse is alive.
    let x = unsafe { *((horse + horse_offset::SCENE_POS_X) as *const f32) };
    let y = unsafe { *((horse + horse_offset::SCENE_POS_Y) as *const f32) };
    Some((x, y))
}

pub fn name_id(horse: usize) -> Option<u32> {
    if horse == 0 {
        return None;
    }
    // SAFETY: uint32 read at +0x1f8.
    Some(unsafe { *((horse + horse_offset::NAME_ID) as *const u32) })
}

/// Resolve a horse's `name_id` to a UTF-8 name via the global name
/// table. Entry layout confirmed via live-memory inspection
/// 2026-05-16 (`tests/dump_name_table_heap.rs`): each entry is a
/// MSVC `std::string` (`_String_val<_Simple_types<char>>`) at 0x88
/// stride. SSO buffer at +0x00..+0x10, `_Mysize` at +0x10, `_Myres`
/// (capacity) at +0x18. The OLD decomp incorrectly labeled +0x18
/// as size; that's capacity.
///
/// Lookup:
///   table = resolve::name_table_pointer_slot() -> heap ptr
///   entry = table + name_id * 0x88
///   size  = *(u64)(entry+0x10)
///   cap   = *(u64)(entry+0x18)
///   if cap > 15: string at *(u64)(entry+0x00) (heap)
///   else:        string inline at entry+0x00 (size bytes)
///
/// Every deref is gated with `is_addr_readable`.
pub fn name(horse_ptr: usize) -> Option<String> {
    let nid = name_id(horse_ptr)?;
    name_by_id(nid)
}

pub fn name_by_id(name_id: u32) -> Option<String> {
    if name_id == u32::MAX {
        return None;
    }
    let slot = crate::targets_registry::resolve::name_table()?;
    if !modforge::winproc::is_addr_readable(slot + 8) {
        return None;
    }
    // SAFETY: slot+8 checked. The resolver returns the static `.data` slot;
    // deref it here (at lookup time) to the live heap table base, which is
    // only allocated after the save loads.
    let table = unsafe { *(slot as *const usize) };
    if table < 0x1_0000 {
        return None;
    }
    let entry = table + (name_id as usize) * 0x88;
    if !modforge::winproc::is_addr_readable(entry + 0x88) {
        return None;
    }
    // SAFETY: entry+0x88 readability checked.
    let size = unsafe { *((entry + 0x10) as *const usize) };
    let cap = unsafe { *((entry + 0x18) as *const usize) };
    if size > 255 {
        return None;
    }
    if size == 0 {
        return Some(String::new());
    }
    let str_ptr = if cap > 0xF {
        // SAFETY: large-string. Read the heap ptr at entry+0x00.
        let p = unsafe { *(entry as *const usize) };
        if !modforge::winproc::is_addr_readable(p + size) {
            return None;
        }
        p
    } else {
        entry
    };
    // SAFETY: str_ptr range readability checked.
    let bytes = unsafe { std::slice::from_raw_parts(str_ptr as *const u8, size) };
    std::str::from_utf8(bytes).ok().map(String::from)
}

pub fn litter_stat(horse: usize) -> Option<i32> {
    if horse == 0 {
        return None;
    }
    // SAFETY: int32 read at +0x254.
    Some(unsafe { *((horse + horse_offset::LITTER_SIZE_STAT) as *const i32) })
}

pub fn set_litter_stat(horse: usize, value: i32) -> bool {
    if horse == 0 {
        return false;
    }
    // SAFETY: int32 write at +0x254.
    unsafe {
        *((horse + horse_offset::LITTER_SIZE_STAT) as *mut i32) = value;
    }
    true
}

/// Number of allele slots PER BANK on a Horse. The vanilla genome is
/// diploid: two banks of 240 bytes each starting at `horse +
/// ctx_offset()` (primary / maternal) and `horse + ctx_offset() +
/// PAIRED_BANK_DELTA` (paired / paternal).
///
/// Confirmed 2026-05-16 by reading the in-game CRISPR apply function
/// (`FUN_1400b39b0`), which writes each gene to BOTH banks. Writing
/// only the primary leaves the rendered phenotype unchanged because
/// the render path samples the paired bank (or applies dominance
/// across both). `tests/probe_paired_genome.rs` verified the paired
/// bank holds real allele data and survives writes to the primary.
pub const VANILLA_GENOME_LEN: usize = 240;

/// Byte distance from the primary bank to the paired bank.
/// CRISPR uses `+ 0xF0` (= 240) hard-coded.
pub const PAIRED_BANK_DELTA: usize = 0xF0;

fn primary_ptr(horse: usize) -> usize {
    horse + horse_offset::ctx_offset()
}

fn paired_ptr(horse: usize) -> usize {
    horse + horse_offset::ctx_offset() + PAIRED_BANK_DELTA
}

/// Read all 240 vanilla allele bytes from the PRIMARY bank only.
pub fn vanilla_alleles(horse: usize) -> Option<[u8; VANILLA_GENOME_LEN]> {
    if horse == 0 {
        return None;
    }
    let mut out = [0u8; VANILLA_GENOME_LEN];
    // SAFETY: horse is a live Horse*; primary bank is 240 bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(
            primary_ptr(horse) as *const u8,
            out.as_mut_ptr(),
            VANILLA_GENOME_LEN,
        );
    }
    Some(out)
}

/// Read both banks as `(primary, paired)`.
pub fn vanilla_alleles_both(
    horse: usize,
) -> Option<([u8; VANILLA_GENOME_LEN], [u8; VANILLA_GENOME_LEN])> {
    if horse == 0 {
        return None;
    }
    let mut p = [0u8; VANILLA_GENOME_LEN];
    let mut q = [0u8; VANILLA_GENOME_LEN];
    // SAFETY: live horse; both banks 240 bytes each.
    unsafe {
        std::ptr::copy_nonoverlapping(
            primary_ptr(horse) as *const u8,
            p.as_mut_ptr(),
            VANILLA_GENOME_LEN,
        );
        std::ptr::copy_nonoverlapping(
            paired_ptr(horse) as *const u8,
            q.as_mut_ptr(),
            VANILLA_GENOME_LEN,
        );
    }
    Some((p, q))
}

/// Read a single vanilla allele byte from the PRIMARY bank.
pub fn vanilla_allele(horse: usize, idx: usize) -> Option<u8> {
    if horse == 0 || idx >= VANILLA_GENOME_LEN {
        return None;
    }
    // SAFETY: bounds checked; live horse.
    Some(unsafe { *((primary_ptr(horse) + idx) as *const u8) })
}

/// Write one vanilla allele to BOTH banks (matches CRISPR).
pub fn set_vanilla_allele(horse: usize, idx: usize, value: u8) -> bool {
    if horse == 0 || idx >= VANILLA_GENOME_LEN {
        return false;
    }
    // SAFETY: bounds checked; live horse; both banks part of Horse.
    unsafe {
        *((primary_ptr(horse) + idx) as *mut u8) = value;
        *((paired_ptr(horse) + idx) as *mut u8) = value;
    }
    true
}

/// Overwrite all 240 vanilla allele bytes in BOTH banks.
pub fn set_vanilla_alleles(horse: usize, alleles: &[u8; VANILLA_GENOME_LEN]) -> bool {
    if horse == 0 {
        return false;
    }
    // SAFETY: live horse; 240 bytes per bank, both part of Horse.
    unsafe {
        std::ptr::copy_nonoverlapping(
            alleles.as_ptr(),
            primary_ptr(horse) as *mut u8,
            VANILLA_GENOME_LEN,
        );
        std::ptr::copy_nonoverlapping(
            alleles.as_ptr(),
            paired_ptr(horse) as *mut u8,
            VANILLA_GENOME_LEN,
        );
    }
    true
}
