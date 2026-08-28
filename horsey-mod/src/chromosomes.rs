//! Chromosome map decoded from CRISPR's gene-offset table (resolved
//! via [`crate::targets::resolve::chromosome_table`]).
//!
//! The table is `i32[chromosome_id][0..17]`, stride `0x44`, up to 20
//! chromosomes. Each entry is a flat gene index in `[0, 240)`; the
//! first `-1` in a row terminates that chromosome's slot list.
//!
//! This module reads the table once on first access and exposes:
//! - [`chromosome_map`]. The parsed list, one entry per chromosome
//!   that has at least one gene.
//! - [`flat_to_chromosome`]. Inverse lookup so any flat gene index
//!   0..=239 maps back to `(chromosome_id, slot_pos)`.

use std::sync::OnceLock;

const STRIDE: usize = 0x44;
const SLOTS_PER_CHROMO: usize = 17;
const MAX_CHROMOS: usize = 20;
const TABLE_BYTES: usize = STRIDE * MAX_CHROMOS;
const VANILLA_GENE_COUNT: usize = 240;

/// One chromosome's gene layout, as encoded in the in-game CRISPR
/// table. `id` is the engine's chromosome_id (0..=19). `slots[k]` is
/// the flat gene index 0..=239 of slot `k` on this chromosome.
#[derive(Debug, Clone)]
pub struct Chromosome {
    pub id: u8,
    pub slots: Vec<u8>,
}

/// Reverse map: flat_gene_idx (0..=239) -> Some((chromosome_id, slot_pos)).
type ReverseMap = [Option<(u8, u8)>; VANILLA_GENE_COUNT];

struct CachedMap {
    chromosomes: Vec<Chromosome>,
    reverse: ReverseMap,
}

static CACHE: OnceLock<Option<CachedMap>> = OnceLock::new();

fn build() -> Option<CachedMap> {
    let addr = crate::targets_registry::resolve::chromosome_table()?;
    if !modforge::winproc::is_addr_readable(addr + TABLE_BYTES) {
        return None;
    }
    let mut chromosomes = Vec::with_capacity(MAX_CHROMOS);
    let mut reverse: ReverseMap = [None; VANILLA_GENE_COUNT];
    let mut total = 0usize;
    for cid in 0..MAX_CHROMOS {
        let mut slots = Vec::with_capacity(SLOTS_PER_CHROMO);
        for s in 0..SLOTS_PER_CHROMO {
            let off = cid * STRIDE + s * 4;
            // SAFETY: range readability checked above.
            let v = unsafe { *((addr + off) as *const i32) };
            if v == -1 {
                break;
            }
            if !(0..VANILLA_GENE_COUNT as i32).contains(&v) {
                modforge::log!(
                    "chromosomes::build: chromo {cid} slot {s} out of range: {v}; aborting"
                );
                return None;
            }
            let v = v as u8;
            if reverse[v as usize].is_some() {
                modforge::log!("chromosomes::build: duplicate flat idx {v}; aborting");
                return None;
            }
            reverse[v as usize] = Some((cid as u8, slots.len() as u8));
            slots.push(v);
            total += 1;
        }
        if !slots.is_empty() {
            chromosomes.push(Chromosome {
                id: cid as u8,
                slots,
            });
        }
    }
    if total != VANILLA_GENE_COUNT {
        modforge::log!("chromosomes::build: got {total} slots, expected {VANILLA_GENE_COUNT}");
        return None;
    }
    Some(CachedMap {
        chromosomes,
        reverse,
    })
}

/// Parsed chromosome map (only chromosomes with >=1 gene are
/// included). Returns `&[]` if resolver missed.
pub fn chromosome_map() -> &'static [Chromosome] {
    match CACHE.get_or_init(build) {
        Some(c) => &c.chromosomes,
        None => &[],
    }
}

/// Flat gene index 0..=239 -> Some((chromosome_id, slot_pos)) or None
/// if `flat` is out of range or the table couldn't be resolved.
pub fn flat_to_chromosome(flat: u8) -> Option<(u8, u8)> {
    let cached = CACHE.get_or_init(build).as_ref()?;
    cached.reverse[flat as usize]
}
