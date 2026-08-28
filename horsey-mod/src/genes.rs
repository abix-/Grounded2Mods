//! Extended-gene state for the bestiary expansion mod.
//!
//! Per the D0..D9 plan in `horsey-mod/docs/todo.md`, extending the
//! engine's gene capacity from 240 to 480 means storing the extended
//! state in our own buffers (the design principle: layer on top of
//! vanilla, never modify vanilla data). This module owns all three
//! sidecar buffers:
//!
//! - `EXT_GENE_TABLE`: definitions for genes 240..(240 + EXT_GENE_COUNT)
//! - `EXT_POP_WEIGHTS`: per-pop allele weights for those extended genes
//! - `EXT_HORSE_GENOMES`: per-horse diploid alleles for those extended genes
//!
//! The vanilla side remains at `DAT_1403ee4a4` (gene table), per-pop
//! `DAT_1403f2fc0 + pop_id * 0x1018` (pop records), and `horse[0x78]`
//! (per-horse 480-byte genome). We never write into those addresses
//! through this module.
//!
//! See `horsey-mod/docs/ALLELE-MODEL.md` for the engine model these
//! buffers mirror.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Extended gene count. Vanilla has 240 slots (`DAT_1403ee4a4` at
/// offsets [0..240) with 32-byte stride). We add this many MORE
/// slots in our own table, indexed [240..240+EXT_GENE_COUNT).
///
/// Default 240 = exactly double. The patch surface does not change
/// if we pick a higher number (e.g. 512, 1024); the cost is just
/// more heap. We keep 240 for v1 to make the math obvious.
pub const EXT_GENE_COUNT: usize = 240;

/// Total gene count (vanilla + extended).
pub const TOTAL_GENE_COUNT: usize = 240 + EXT_GENE_COUNT;

/// Default mutation rate for an unauthored gene. Matches the vanilla
/// loader's default at `FUN_1400a3eb0:44`.
pub const DEFAULT_MUTATION_RATE: u32 = 100;

/// Default scale. Matches `FUN_1400a3eb0:45`.
pub const DEFAULT_SCALE: u64 = 1;

/// Mirrors the vanilla 32-byte gene table record. We keep the layout
/// identical to the vanilla one so any future shared serializer code
/// can treat both sides interchangeably.
///
/// Vanilla layout (per `ALLELE-MODEL.md` and `FUN_1400a5d20:24`'s
/// self-check):
/// - +0x00 uint32 index    (must equal slot index)
/// - +0x04 uint32 m        (mutation rate, default 100)
/// - +0x08 uint64 s        (scale, default 1)
/// - +0x10 4 x uint32      (alleles g0..g3)
/// - +0x20 (next record)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtGene {
    /// User-facing name as it would appear in `genes-extended.xml`,
    /// e.g. `BX_WING_SIZE`. Vanilla genes are indexed by name in the
    /// chromomap; we mirror that for the extended set.
    pub name: String,

    /// Mutation rate (vanilla `m` field). Range typically 0..255.
    pub mutation_rate: u32,

    /// Scale (vanilla `s` field). Vanilla default is 1; non-default
    /// values change how the diploid blend formula scales.
    pub scale: u64,

    /// The four allele payload values. Per `FUN_1400a5d20`'s blend
    /// formula, allele index 0..3 selects which of these is used as
    /// the dominant or recessive contribution.
    pub alleles: [u32; 4],

    /// Render-output mapping. Tells the D5 trampoline which
    /// `param_1[N]` slot to write to and how. Optional: unset means
    /// the gene's value is computed and stored but produces no
    /// visible effect (useful for breeding-only traits).
    #[serde(default)]
    pub render: Option<RenderMapping>,
}

/// How an extended gene's evaluated value should be applied to the
/// per-horse render parameter array. Authored alongside the gene in
/// `genes-extended.xml`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderMapping {
    /// `param_1[N]` slot index (0..352 from Q-render-2).
    pub slot: u32,

    /// How to apply the value to the slot.
    pub mode: RenderMode,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    /// Add the gene's evaluated value to whatever vanilla wrote.
    Add,
    /// Multiply the slot by the gene's value.
    Mul,
    /// Override the slot regardless of vanilla.
    Set,
}

impl ExtGene {
    /// Default-initialize an extended gene. Used when a slot is
    /// touched without an explicit `genes-extended.xml` definition,
    /// or when a save loads with more authored genes than the
    /// running mod knows about.
    pub fn default_named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mutation_rate: DEFAULT_MUTATION_RATE,
            scale: DEFAULT_SCALE,
            alleles: [0, 0, 0, 0],
            render: None,
        }
    }
}

/// Per-pop extended weights. One entry per gene in the extended
/// range, with 4 weights (one per allele).
///
/// Mirrors the vanilla per-pop weight layout at offset `+0x28`
/// of the pop record, just for the extended range.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtPopWeights {
    /// `[gene_idx_within_ext][allele_idx]` = inverse weight.
    /// Inverse means lower number = more likely (matches vanilla).
    pub weights: Vec<[u32; 4]>,
}

impl ExtPopWeights {
    pub fn default_for_count(n: usize) -> Self {
        Self {
            weights: vec![[1, 1, 1, 1]; n],
        }
    }
}

/// Per-horse extended diploid genome. Two allele indices per gene,
/// stored in the same i / i+EXT_GENE_COUNT layout that vanilla uses
/// for its 480-byte block at `horse[0x78]`. Each allele index is
/// 0..3 (which of the 4 allele payloads on this gene to use).
///
/// Stored as a flat `Vec<u8>` of length `2 * EXT_GENE_COUNT`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtHorseGenome {
    pub alleles: Vec<u8>,
}

impl ExtHorseGenome {
    pub fn empty() -> Self {
        Self {
            alleles: vec![0; 2 * EXT_GENE_COUNT],
        }
    }
}

// =============================================================================
// State (the three sidecar buffers)
// =============================================================================

/// Extended gene table. `Vec` indexed [0..EXT_GENE_COUNT). To get
/// the vanilla-equivalent gene index, add 240.
fn gene_table() -> &'static RwLock<Vec<ExtGene>> {
    static T: OnceLock<RwLock<Vec<ExtGene>>> = OnceLock::new();
    T.get_or_init(|| {
        let mut v = Vec::with_capacity(EXT_GENE_COUNT);
        for i in 0..EXT_GENE_COUNT {
            v.push(ExtGene::default_named(format!("BX_RESERVED_{i:03}")));
        }
        RwLock::new(v)
    })
}

/// Per-pop extended weights. Keyed by vanilla pop_id (the same
/// `pop_id` field stored at offset 0 in vanilla pop records).
fn pop_weights() -> &'static RwLock<HashMap<u32, ExtPopWeights>> {
    static T: OnceLock<RwLock<HashMap<u32, ExtPopWeights>>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Per-horse extended genomes. Keyed by horse_id. The stable horse
/// identifier is open (D4.4 in todo); for now we accept whatever the
/// caller provides as a u64 key.
fn horse_genomes() -> &'static RwLock<HashMap<u64, ExtHorseGenome>> {
    static T: OnceLock<RwLock<HashMap<u64, ExtHorseGenome>>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Default extended-genome FALLBACK. Used when `apply_render_to_buf`
/// is called for a horse that has no entry in `horse_genomes()`.
/// Models the "every horse gets the same starter extended alleles
/// until D3 wires per-horse spawn-time generation."
///
/// Stored as the same flat `Vec<u8>` layout (`alleles[ext_idx] = m`,
/// `alleles[ext_idx + EXT_GENE_COUNT] = p`). None means "no fallback;
/// horses without entries contribute nothing to the render."
fn default_horse_genome() -> &'static RwLock<Option<ExtHorseGenome>> {
    static T: OnceLock<RwLock<Option<ExtHorseGenome>>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(None))
}

/// Set the default extended alleles for ONE extended gene. Applies to
/// every rendered horse that doesn't already have a specific entry.
/// Auto-creates the default genome if not yet initialized.
pub fn set_default_ext_alleles(
    ext_gene_idx: usize,
    maternal: u8,
    paternal: u8,
) -> Result<(), String> {
    if ext_gene_idx >= EXT_GENE_COUNT {
        return Err(format!(
            "ext_gene_idx {ext_gene_idx} out of range (max {})",
            EXT_GENE_COUNT - 1
        ));
    }
    if maternal > 3 || paternal > 3 {
        return Err("alleles must be 0..3".into());
    }
    let mut g = default_horse_genome().write();
    let genome = g.get_or_insert_with(ExtHorseGenome::empty);
    genome.alleles[ext_gene_idx] = maternal & 0x3;
    genome.alleles[ext_gene_idx + EXT_GENE_COUNT] = paternal & 0x3;
    Ok(())
}

/// Read the current default extended genome, if any.
pub fn get_default_ext_genome() -> Option<ExtHorseGenome> {
    default_horse_genome().read().clone()
}

/// Clear the default extended genome entirely.
pub fn clear_default_ext_genome() {
    *default_horse_genome().write() = None;
}

// =============================================================================
// Gene table accessors
// =============================================================================

/// Read an extended gene by extended-range index (0..EXT_GENE_COUNT).
pub fn get_ext_gene(ext_idx: usize) -> Option<ExtGene> {
    gene_table().read().get(ext_idx).cloned()
}

/// Read an extended gene by full-range index (240..TOTAL_GENE_COUNT).
/// Returns None for vanilla indices and out-of-range.
pub fn get_gene_by_total_idx(total_idx: usize) -> Option<ExtGene> {
    if total_idx < 240 || total_idx >= TOTAL_GENE_COUNT {
        return None;
    }
    get_ext_gene(total_idx - 240)
}

/// Set an extended gene's full definition. Returns the previous
/// value if the slot was already populated.
pub fn set_ext_gene(ext_idx: usize, gene: ExtGene) -> Option<ExtGene> {
    let mut t = gene_table().write();
    if ext_idx >= t.len() {
        return None;
    }
    let old = t[ext_idx].clone();
    t[ext_idx] = gene;
    Some(old)
}

/// List all extended genes (snapshot copy).
pub fn list_ext_genes() -> Vec<ExtGene> {
    gene_table().read().clone()
}

/// Look up an extended gene by name. Returns the extended-range
/// index AND the gene record. None if no extended gene has this
/// name.
pub fn find_ext_gene_by_name(name: &str) -> Option<(usize, ExtGene)> {
    gene_table()
        .read()
        .iter()
        .enumerate()
        .find(|(_, g)| g.name == name)
        .map(|(i, g)| (i, g.clone()))
}

// =============================================================================
// Per-pop weight accessors
// =============================================================================

/// Get a pop's extended weights. Auto-initializes to defaults if
/// not yet seen.
pub fn get_or_init_pop_weights(pop_id: u32) -> ExtPopWeights {
    let mut w = pop_weights().write();
    w.entry(pop_id)
        .or_insert_with(|| ExtPopWeights::default_for_count(EXT_GENE_COUNT))
        .clone()
}

/// Set a single allele weight for a pop's extended gene.
/// `ext_gene_idx` is in [0..EXT_GENE_COUNT). `allele_idx` is in [0..4).
pub fn set_pop_ext_weight(
    pop_id: u32,
    ext_gene_idx: usize,
    allele_idx: usize,
    weight: u32,
) -> Result<(), String> {
    if ext_gene_idx >= EXT_GENE_COUNT {
        return Err(format!(
            "ext_gene_idx {ext_gene_idx} out of range (max {})",
            EXT_GENE_COUNT - 1
        ));
    }
    if allele_idx >= 4 {
        return Err(format!("allele_idx {allele_idx} out of range (max 3)"));
    }
    let mut w = pop_weights().write();
    let entry = w
        .entry(pop_id)
        .or_insert_with(|| ExtPopWeights::default_for_count(EXT_GENE_COUNT));
    entry.weights[ext_gene_idx][allele_idx] = weight;
    Ok(())
}

/// List every pop_id with extended weights and their full weight
/// matrices. Sorted by pop_id ascending.
pub fn list_pop_ext_weights() -> Vec<(u32, ExtPopWeights)> {
    let mut v: Vec<_> = pop_weights()
        .read()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    v.sort_by_key(|(k, _)| *k);
    v
}

// =============================================================================
// Per-horse genome accessors
// =============================================================================

/// Read a horse's full extended genome. None if no entry exists.
pub fn get_horse_ext_genome(horse_id: u64) -> Option<ExtHorseGenome> {
    horse_genomes().read().get(&horse_id).cloned()
}

/// Set a horse's full extended genome.
pub fn set_horse_ext_genome(horse_id: u64, genome: ExtHorseGenome) {
    horse_genomes().write().insert(horse_id, genome);
}

/// Drop a horse's extended genome (call from D3.2 detour when the
/// vanilla 480-byte block is freed).
pub fn drop_horse_ext_genome(horse_id: u64) -> bool {
    horse_genomes().write().remove(&horse_id).is_some()
}

/// Ensure a horse has an entry in `EXT_HORSE_GENOMES`. Returns true
/// if a NEW entry was created, false if one already existed. Called
/// from the D3.1 lifecycle anchor (constructor post-hook). The
/// default value is from `get_default_ext_genome()` if set, else
/// all-zeros.
pub fn ensure_horse_ext_genome(horse_id: u64) -> bool {
    let mut g = horse_genomes().write();
    if g.contains_key(&horse_id) {
        return false;
    }
    let default = default_horse_genome()
        .read()
        .clone()
        .unwrap_or_else(ExtHorseGenome::empty);
    g.insert(horse_id, default);
    true
}

/// Read both diploid alleles for a specific extended gene of a
/// specific horse. Returns (maternal, paternal) allele indices each
/// in [0..3], or None if the horse has no extended genome.
pub fn get_horse_ext_alleles(horse_id: u64, ext_gene_idx: usize) -> Option<(u8, u8)> {
    if ext_gene_idx >= EXT_GENE_COUNT {
        return None;
    }
    let g = horse_genomes().read();
    let genome = g.get(&horse_id)?;
    let m = genome.alleles[ext_gene_idx] & 0x3;
    let p = genome.alleles[ext_gene_idx + EXT_GENE_COUNT] & 0x3;
    Some((m, p))
}

/// Set the diploid allele pair for one extended gene of one horse.
/// Auto-initializes the horse's extended genome if not present.
pub fn set_horse_ext_alleles(
    horse_id: u64,
    ext_gene_idx: usize,
    maternal: u8,
    paternal: u8,
) -> Result<(), String> {
    if ext_gene_idx >= EXT_GENE_COUNT {
        return Err(format!(
            "ext_gene_idx {ext_gene_idx} out of range (max {})",
            EXT_GENE_COUNT - 1
        ));
    }
    if maternal > 3 || paternal > 3 {
        return Err("alleles must be 0..3".into());
    }
    let mut g = horse_genomes().write();
    let genome = g.entry(horse_id).or_insert_with(ExtHorseGenome::empty);
    genome.alleles[ext_gene_idx] = maternal & 0x3;
    genome.alleles[ext_gene_idx + EXT_GENE_COUNT] = paternal & 0x3;
    Ok(())
}

/// List every horse_id with an extended genome.
pub fn list_horse_genome_ids() -> Vec<u64> {
    let mut v: Vec<_> = horse_genomes().read().keys().copied().collect();
    v.sort();
    v
}

// =============================================================================
// D3.4 breeding combinator (mirrors FUN_1400a2d80 for the ext range)
// =============================================================================

/// Mendelian recombination of two parents' extended genomes into a
/// child's extended genome. Mirrors vanilla `FUN_1400a2d80` semantics
/// over the ext range:
///   - child strand 0 (maternal) draws from parent A; each gene's
///     allele picks one of parent A's two strands at random.
///   - child strand 1 (paternal) draws from parent B; same.
///
/// User-locked design (2026-05-15): default is **independent
/// per-gene inheritance**. Vanilla forces same-parent inheritance
/// across 3 cluster ranges (Neck, Head, palette base) but for v1
/// the ext range does not include such linkage. A future XML
/// extension (`<linked-cluster>` in `genes-extended.xml`) can opt
/// in per-gene; not implemented here.
///
/// Lazy semantics: if either parent has no entry, the child gets
/// default-zero alleles from the missing parent's side. If both
/// parents are missing, the child entry is still created (all
/// zeros) so downstream lookups don't fail open.
pub fn combine_for_breeding(parent_a_id: u64, parent_b_id: u64, child_id: u64) {
    let (pa, pb) = {
        let g = horse_genomes().read();
        (g.get(&parent_a_id).cloned(), g.get(&parent_b_id).cloned())
    };
    let pa = pa.unwrap_or_else(ExtHorseGenome::empty);
    let pb = pb.unwrap_or_else(ExtHorseGenome::empty);

    let mut child = ExtHorseGenome::empty();
    // Use a fast, non-cryptographic RNG seeded per-call from horse
    // pointers + a process-local nonce. We don't need crypto
    // randomness; vanilla uses FUN_1400c6580 (a basic PRNG) too.
    let seed = parent_a_id
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(parent_b_id.wrapping_mul(0xBF58476D1CE4E5B9))
        .wrapping_add(child_id.wrapping_mul(0x94D049BB133111EB))
        .wrapping_add(combine_nonce());
    let mut rng = seed;

    // strand 0 of child <- random pick from parent A
    for g in 0..EXT_GENE_COUNT {
        rng = next_lcg(rng);
        let coin = ((rng >> 33) & 1) as usize;
        child.alleles[g] = pa.alleles[coin * EXT_GENE_COUNT + g] & 0x3;
    }
    // strand 1 of child <- random pick from parent B
    for g in 0..EXT_GENE_COUNT {
        rng = next_lcg(rng);
        let coin = ((rng >> 33) & 1) as usize;
        child.alleles[EXT_GENE_COUNT + g] = pb.alleles[coin * EXT_GENE_COUNT + g] & 0x3;
    }

    horse_genomes().write().insert(child_id, child);
}

/// Per-process monotonic combiner nonce so identical (a,b,c) triples
/// across separate breedings get different RNG streams.
fn combine_nonce() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0xa5a5_a5a5_a5a5_a5a5);
    N.fetch_add(0x100000001b3, Ordering::Relaxed)
}

/// SplitMix64-style step. Cheap, non-crypto.
#[inline]
fn next_lcg(x: u64) -> u64 {
    let z = x.wrapping_add(0x9E3779B97F4A7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

// =============================================================================
// Diploid evaluator (mirrors FUN_1400a5d20 for the extended range)
// =============================================================================

/// Evaluate an extended gene's effect for a specific horse. Mirrors
/// the math of `FUN_1400a5d20` but reads from our sidecar buffers.
///
/// Returns 0.0 if the horse has no extended genome OR the gene is
/// out of range. The D5 trampoline calls this for every extended
/// gene per horse per call to FUN_14009f680.
pub fn evaluate_ext_gene(horse_id: u64, ext_gene_idx: usize) -> f32 {
    let Some(gene) = get_ext_gene(ext_gene_idx) else {
        return 0.0;
    };
    let Some((m_allele, p_allele)) = get_horse_ext_alleles(horse_id, ext_gene_idx) else {
        return 0.0;
    };
    let dom_idx = m_allele.max(p_allele) as usize;
    let rec_idx = m_allele.min(p_allele) as usize;
    let dom_value = gene.alleles[dom_idx] as f32;
    let rec_value = gene.alleles[rec_idx] as f32;
    let m = gene.mutation_rate as f32;
    let s = gene.scale as f32;
    // Vanilla blend constants DAT_14039ca44 and DAT_14039ca5c are
    // not yet documented (open question). For now we use the
    // simplest interpretation: m/s in [0,1] is the recessive
    // contribution weight, (1 - m/s) is the dominant. With vanilla
    // m=100, s=1, this gives 100/1 = 100 -> clamped to 1.0.
    let recessive_weight = (m / s.max(1e-6)).clamp(0.0, 1.0);
    let dominant_weight = 1.0 - recessive_weight;
    dominant_weight * dom_value + recessive_weight * rec_value
}

/// Swap two allele payload positions for one extended gene. Used by
/// the DI-A detour on `FUN_1400c03a0` (gene_allele_renumber_sync):
/// when vanilla swaps alleles `a` and `b` of an extended gene, we
/// mirror the swap in `EXT_GENE_TABLE[ext_idx].alleles` AND every
/// pop's extended weights for that gene.
///
/// No-op if `ext_idx` or either allele index is out of range.
pub fn swap_ext_alleles(ext_idx: usize, a: usize, b: usize) {
    if ext_idx >= EXT_GENE_COUNT || a >= 4 || b >= 4 || a == b {
        return;
    }
    {
        let mut t = gene_table().write();
        t[ext_idx].alleles.swap(a, b);
    }
    let mut w = pop_weights().write();
    for entry in w.values_mut() {
        if let Some(weights) = entry.weights.get_mut(ext_idx) {
            weights.swap(a, b);
        }
    }
}

/// Evaluate every extended gene that has a render mapping for one
/// horse and apply the result to `buf` per its `RenderMode`. Called
/// by the D5 trampoline AFTER vanilla `FUN_14009f680` has populated
/// `buf` with the 258 vanilla slot writes.
///
/// `buf_len` is the float-count of the buffer (vanilla allocates
/// 353 floats). We bounds-check every slot write so a misauthored
/// `genes-extended.xml` with an out-of-range slot can't corrupt
/// stack neighbours.
///
/// SAFETY: caller guarantees `buf` is a valid `*mut f32` of length
/// `buf_len`, live for the duration of this call.
pub unsafe fn apply_render_to_buf(buf: *mut f32, buf_len: usize, horse_id: u64) -> usize {
    let mut applied = 0usize;
    let table = gene_table().read();
    let horses = horse_genomes().read();
    // Use the horse-specific genome if present; otherwise fall back
    // to the default-allele genome (if any). Caller gets zero work
    // done only if BOTH are absent.
    let default_holder = default_horse_genome().read();
    let genome: &ExtHorseGenome = if let Some(g) = horses.get(&horse_id) {
        g
    } else if let Some(g) = default_holder.as_ref() {
        g
    } else {
        return 0;
    };
    for (ext_idx, gene) in table.iter().enumerate() {
        let Some(render) = gene.render.as_ref() else {
            continue;
        };
        let slot = render.slot as usize;
        if slot >= buf_len {
            continue;
        }

        // Inline the diploid blend math against the snapshot we
        // already hold (avoid re-locking horse_genomes).
        let m_allele = (genome.alleles[ext_idx] & 0x3) as usize;
        let p_allele = (genome.alleles[ext_idx + EXT_GENE_COUNT] & 0x3) as usize;
        let dom = m_allele.max(p_allele);
        let rec = m_allele.min(p_allele);
        let dom_value = gene.alleles[dom] as f32;
        let rec_value = gene.alleles[rec] as f32;
        let m = gene.mutation_rate as f32;
        let s = gene.scale as f32;
        let recessive_weight = (m / s.max(1e-6)).clamp(0.0, 1.0);
        let dominant_weight = 1.0 - recessive_weight;
        let value = dominant_weight * dom_value + recessive_weight * rec_value;

        // SAFETY: bounds-checked above.
        unsafe {
            let p = buf.add(slot);
            match render.mode {
                RenderMode::Add => *p += value,
                RenderMode::Mul => *p *= value,
                RenderMode::Set => *p = value,
            }
        }
        applied += 1;
    }
    applied
}

// =============================================================================
// JSON snapshot (for HTTP dump op)
// =============================================================================

/// Full snapshot of every extended-gene buffer. Used by the
/// `genes.ext.dump` HTTP op.
#[derive(Serialize)]
pub struct ExtSnapshot {
    pub ext_gene_count: usize,
    pub total_gene_count: usize,
    pub genes: Vec<ExtGene>,
    pub pop_weights: HashMap<String, ExtPopWeights>,
    pub horse_genome_count: usize,
    pub horse_ids: Vec<u64>,
}

pub fn snapshot() -> ExtSnapshot {
    let pops: HashMap<String, ExtPopWeights> = pop_weights()
        .read()
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    let horse_ids: Vec<u64> = list_horse_genome_ids();
    ExtSnapshot {
        ext_gene_count: EXT_GENE_COUNT,
        total_gene_count: TOTAL_GENE_COUNT,
        genes: list_ext_genes(),
        pop_weights: pops,
        horse_genome_count: horse_ids.len(),
        horse_ids,
    }
}

/// Test-only: reset every sidecar buffer so tests that mutate
/// global state can run independently. Production code never calls
/// this. Keeps the table at vanilla defaults.
#[cfg(test)]
pub fn reset_all_for_tests() {
    let mut t = gene_table().write();
    t.clear();
    for i in 0..EXT_GENE_COUNT {
        t.push(ExtGene::default_named(format!("BX_RESERVED_{i:03}")));
    }
    drop(t);
    pop_weights().write().clear();
    horse_genomes().write().clear();
}

/// Test-only mutex serializing every test that touches global gene
/// state. Hold for the entire test body. Without it, parallel tests
/// pollute each other and assertions about `set_ext_gene` /
/// `set_horse_ext_alleles` outcomes are unreliable.
#[cfg(test)]
pub static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_table_has_ext_gene_count_entries() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        assert_eq!(list_ext_genes().len(), EXT_GENE_COUNT);
    }

    #[test]
    fn set_then_get_ext_gene() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let g = ExtGene {
            name: "TEST_GENE".into(),
            mutation_rate: 50,
            scale: 1,
            alleles: [10, 20, 30, 40],
            render: Some(RenderMapping {
                slot: 99,
                mode: RenderMode::Add,
            }),
        };
        set_ext_gene(0, g.clone());
        let back = get_ext_gene(0).unwrap();
        assert_eq!(back.name, "TEST_GENE");
        assert_eq!(back.alleles, [10, 20, 30, 40]);
    }

    #[test]
    fn pop_weights_default_to_ones() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let w = get_or_init_pop_weights(42);
        assert_eq!(w.weights.len(), EXT_GENE_COUNT);
        assert_eq!(w.weights[0], [1, 1, 1, 1]);
    }

    #[test]
    fn horse_genome_diploid_layout() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        set_horse_ext_alleles(7, 5, 2, 3).unwrap();
        let (m, p) = get_horse_ext_alleles(7, 5).unwrap();
        assert_eq!((m, p), (2, 3));
    }

    #[test]
    fn evaluate_homozygous() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let g = ExtGene {
            name: "T".into(),
            mutation_rate: 100,
            scale: 1,
            alleles: [10, 20, 30, 80],
            render: None,
        };
        set_ext_gene(0, g);
        set_horse_ext_alleles(1, 0, 3, 3).unwrap();
        // Homozygous 3,3: dom = rec = 80. Blend math doesn't matter.
        assert_eq!(evaluate_ext_gene(1, 0), 80.0);
    }

    #[test]
    fn evaluate_no_horse_genome_returns_zero() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        assert_eq!(evaluate_ext_gene(99999, 0), 0.0);
    }

    #[test]
    fn swap_ext_alleles_swaps_payload_and_weights() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let g = ExtGene {
            name: "T".into(),
            mutation_rate: 100,
            scale: 1,
            alleles: [10, 20, 30, 40],
            render: None,
        };
        set_ext_gene(2, g);
        set_pop_ext_weight(7, 2, 0, 1).unwrap();
        set_pop_ext_weight(7, 2, 3, 9).unwrap();

        swap_ext_alleles(2, 0, 3);

        let g_back = get_ext_gene(2).unwrap();
        assert_eq!(g_back.alleles, [40, 20, 30, 10]);
        let w = get_or_init_pop_weights(7);
        assert_eq!(w.weights[2][0], 9);
        assert_eq!(w.weights[2][3], 1);
    }

    #[test]
    fn swap_ext_alleles_ignores_oor() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        // Should not panic.
        swap_ext_alleles(EXT_GENE_COUNT + 1, 0, 1);
        swap_ext_alleles(0, 0, 9);
        swap_ext_alleles(0, 2, 2); // no-op self-swap
    }

    #[test]
    fn apply_render_to_buf_writes_mapped_slot_in_add_mode() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let g = ExtGene {
            name: "BX_WING".into(),
            mutation_rate: 100,
            scale: 1,
            alleles: [0, 0, 0, 80],
            render: Some(RenderMapping {
                slot: 42,
                mode: RenderMode::Add,
            }),
        };
        set_ext_gene(0, g);
        set_horse_ext_alleles(0x1234, 0, 3, 3).unwrap(); // homozygous 3 -> 80

        let mut buf = vec![1.0f32; 353];
        // SAFETY: buf is a live Vec we own; 353 floats.
        let applied = unsafe { apply_render_to_buf(buf.as_mut_ptr(), buf.len(), 0x1234) };
        assert_eq!(applied, 1);
        assert!(
            (buf[42] - 81.0).abs() < 1e-6,
            "expected 81.0 got {}",
            buf[42]
        );
        // Adjacent slots untouched.
        assert!((buf[41] - 1.0).abs() < 1e-6);
        assert!((buf[43] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn apply_render_to_buf_skips_oor_slot() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let g = ExtGene {
            name: "BX_BAD".into(),
            mutation_rate: 100,
            scale: 1,
            alleles: [0, 0, 0, 99],
            render: Some(RenderMapping {
                slot: 9999, // out of range
                mode: RenderMode::Set,
            }),
        };
        set_ext_gene(0, g);
        set_horse_ext_alleles(0x5678, 0, 3, 3).unwrap();

        let mut buf = vec![1.0f32; 353];
        // SAFETY: buf is a live Vec we own.
        let applied = unsafe { apply_render_to_buf(buf.as_mut_ptr(), buf.len(), 0x5678) };
        assert_eq!(applied, 0, "out-of-range slot must be skipped");
    }

    #[test]
    fn apply_render_to_buf_no_genome_returns_zero() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let g = ExtGene {
            name: "BX_X".into(),
            mutation_rate: 100,
            scale: 1,
            alleles: [0, 0, 0, 50],
            render: Some(RenderMapping {
                slot: 10,
                mode: RenderMode::Set,
            }),
        };
        set_ext_gene(0, g);

        let mut buf = vec![1.0f32; 353];
        // SAFETY: buf is a live Vec we own.
        let applied = unsafe { apply_render_to_buf(buf.as_mut_ptr(), buf.len(), 0xDEAD) };
        assert_eq!(applied, 0);
        assert!((buf[10] - 1.0).abs() < 1e-6, "buf must not be touched");
    }

    #[test]
    fn apply_render_to_buf_mul_and_set_modes() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let g_mul = ExtGene {
            name: "BX_MUL".into(),
            mutation_rate: 100,
            scale: 1,
            alleles: [0, 0, 0, 3],
            render: Some(RenderMapping {
                slot: 5,
                mode: RenderMode::Mul,
            }),
        };
        let g_set = ExtGene {
            name: "BX_SET".into(),
            mutation_rate: 100,
            scale: 1,
            alleles: [0, 0, 0, 77],
            render: Some(RenderMapping {
                slot: 6,
                mode: RenderMode::Set,
            }),
        };
        set_ext_gene(0, g_mul);
        set_ext_gene(1, g_set);
        set_horse_ext_alleles(1, 0, 3, 3).unwrap();
        set_horse_ext_alleles(1, 1, 3, 3).unwrap();

        let mut buf = vec![2.0f32; 353];
        // SAFETY: buf is a live Vec we own.
        let applied = unsafe { apply_render_to_buf(buf.as_mut_ptr(), buf.len(), 1) };
        assert_eq!(applied, 2);
        assert!(
            (buf[5] - 6.0).abs() < 1e-6,
            "mul: 2 * 3 = 6, got {}",
            buf[5]
        );
        assert!((buf[6] - 77.0).abs() < 1e-6, "set: 77, got {}", buf[6]);
    }

    // -----------------------------------------------------------------
    // D3.4 combinator Mendelian unit tests
    // -----------------------------------------------------------------

    /// Build a parent genome where strand A's alleles are all `a` and
    /// strand B's alleles are all `b`. Lets us assert that the child's
    /// alleles must come from {a, b} only.
    fn parent_with_uniform_strands(a: u8, b: u8) -> ExtHorseGenome {
        let mut g = ExtHorseGenome::empty();
        for i in 0..EXT_GENE_COUNT {
            g.alleles[i] = a & 0x3;
            g.alleles[i + EXT_GENE_COUNT] = b & 0x3;
        }
        g
    }

    #[test]
    fn combinator_child_strand_0_comes_from_parent_a_only() {
        // Parent A's strands hold values (1, 2).
        // Parent B's strands hold values (0, 3).
        // Child's strand 0 must contain only 1 or 2 (one of parent A's
        // strands per gene); child's strand 1 must contain only 0 or 3.
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();

        set_horse_ext_genome(100, parent_with_uniform_strands(1, 2));
        set_horse_ext_genome(200, parent_with_uniform_strands(0, 3));

        combine_for_breeding(100, 200, 300);
        let child = get_horse_ext_genome(300).expect("child genome should exist after combinator");

        for i in 0..EXT_GENE_COUNT {
            let mat = child.alleles[i];
            let pat = child.alleles[i + EXT_GENE_COUNT];
            assert!(
                mat == 1 || mat == 2,
                "child strand 0 gene {i} = {mat}, must be from parent A {{1, 2}}"
            );
            assert!(
                pat == 0 || pat == 3,
                "child strand 1 gene {i} = {pat}, must be from parent B {{0, 3}}"
            );
        }
    }

    #[test]
    fn combinator_picks_both_strands_of_each_parent_over_many_genes() {
        // With 240 genes and a fair coin, each parent's strand A and
        // strand B should both appear in the child at least once.
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();

        set_horse_ext_genome(100, parent_with_uniform_strands(1, 2));
        set_horse_ext_genome(200, parent_with_uniform_strands(0, 3));

        combine_for_breeding(100, 200, 300);
        let child = get_horse_ext_genome(300).unwrap();

        let strand0: Vec<u8> = (0..EXT_GENE_COUNT).map(|i| child.alleles[i]).collect();
        let strand1: Vec<u8> = (0..EXT_GENE_COUNT)
            .map(|i| child.alleles[i + EXT_GENE_COUNT])
            .collect();

        assert!(
            strand0.iter().any(|&a| a == 1),
            "no 1 in strand 0 (parent A strand A never selected)"
        );
        assert!(
            strand0.iter().any(|&a| a == 2),
            "no 2 in strand 0 (parent A strand B never selected)"
        );
        assert!(
            strand1.iter().any(|&a| a == 0),
            "no 0 in strand 1 (parent B strand A never selected)"
        );
        assert!(
            strand1.iter().any(|&a| a == 3),
            "no 3 in strand 1 (parent B strand B never selected)"
        );
    }

    #[test]
    fn combinator_no_parents_produces_zero_genome() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();

        // Neither parent has an ext genome entry. Child should still
        // get an entry (so downstream lookups don't fail open) but
        // every allele is 0.
        combine_for_breeding(100, 200, 300);
        let child =
            get_horse_ext_genome(300).expect("child genome should be created even without parents");
        assert!(
            child.alleles.iter().all(|&a| a == 0),
            "missing-parent path should produce all-zero alleles"
        );
    }

    #[test]
    fn combinator_one_missing_parent_falls_back_to_default() {
        // Parent A has a known genome; parent B is missing. Child's
        // strand 0 must come from parent A; strand 1 from the default
        // (all zeros).
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();

        set_horse_ext_genome(100, parent_with_uniform_strands(2, 2));
        combine_for_breeding(100, 200 /* missing */, 300);

        let child = get_horse_ext_genome(300).unwrap();
        for i in 0..EXT_GENE_COUNT {
            assert_eq!(child.alleles[i], 2, "strand 0 gene {i} != 2");
            assert_eq!(
                child.alleles[i + EXT_GENE_COUNT],
                0,
                "strand 1 gene {i} should be 0"
            );
        }
    }

    #[test]
    fn combinator_masks_alleles_to_0_3() {
        // Even if a parent has a bad byte (e.g. 0xff), the child's
        // alleles must be masked to 0..3 (& 0x3 guard).
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();

        let mut bad = ExtHorseGenome::empty();
        for i in 0..EXT_GENE_COUNT {
            bad.alleles[i] = 0xff;
            bad.alleles[i + EXT_GENE_COUNT] = 0xfe;
        }
        set_horse_ext_genome(100, bad.clone());
        set_horse_ext_genome(200, bad);

        combine_for_breeding(100, 200, 300);
        let child = get_horse_ext_genome(300).unwrap();
        for &a in &child.alleles {
            assert!(a < 4, "allele {a} not masked to 0..3");
        }
    }

    #[test]
    fn combinator_distinct_breedings_diverge() {
        // Two breedings of the same parents into the same child id
        // should not produce IDENTICAL outputs (the nonce ensures
        // different RNG streams). Otherwise consecutive offspring of
        // the same pair would always be carbon copies.
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();

        set_horse_ext_genome(100, parent_with_uniform_strands(1, 2));
        set_horse_ext_genome(200, parent_with_uniform_strands(0, 3));

        combine_for_breeding(100, 200, 300);
        let first = get_horse_ext_genome(300).unwrap();

        combine_for_breeding(100, 200, 301);
        let second = get_horse_ext_genome(301).unwrap();

        // With 240 genes and two independent random streams, the
        // probability of identical alleles is 2^-240. If they ARE
        // equal, the nonce / RNG seeding is broken.
        assert_ne!(
            first.alleles, second.alleles,
            "two breedings of same parents produced identical children; RNG nonce broken"
        );
    }
}
