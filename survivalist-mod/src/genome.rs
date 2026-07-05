//! Faction trait genome + learning (docs/faction-war.md "THE
//! VISION: a Darwinian world" + the learning layer).
//!
//! Each faction carries a small genome of strategic traits in
//! [0.05, 0.95]. The genome is:
//! - VARIATION: seeded by community type + per-faction jitter
//!   from the community Id (deterministic, so no RNG), so no two
//!   factions play identically.
//! - PLASTICITY: reinforced by the OUTCOMES of the choices it
//!   drives. A raid that paid off raises aggression; one that
//!   cost people lowers it. The faction's personality becomes a
//!   record of its own life.
//! - HEREDITY (hook): on conquest/absorption (a later phase) the
//!   victor's genome blends into the survivors it takes; when a
//!   faction dies its genome dies with it. `blend_into` is the
//!   seam.
//!
//! Storage is Rust-side, keyed by the stable community `Id`. v1
//! limitation (documented): the genome lives for the SESSION, not
//! across a save/load reload (that would need writing into the
//! game's save). Seeding is deterministic, so a reload re-seeds
//! the same starting genomes; only mid-session learning is lost
//! on reload.

use std::collections::HashMap;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

/// Learning step per reinforced outcome.
const LEARN_RATE: f64 = 0.06;
const TRAIT_MIN: f64 = 0.05;
const TRAIT_MAX: f64 = 0.95;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Trait {
    /// Raid / attack appetite.
    Aggression,
    // The three below are SEEDED + shown + propagate on conquest,
    // but their learning loops wait for the behaviors they drive
    // (annexing, fortify/flee, extort/ally) to be survival-wired.
    /// Grow / annex appetite.
    #[allow(dead_code)]
    Expansionism,
    /// Fortify / turtle / flee appetite.
    #[allow(dead_code)]
    Defensiveness,
    /// Extort / ally / manipulate appetite.
    #[allow(dead_code)]
    Guile,
}

impl Trait {
    #[allow(dead_code)]
    pub fn name(self) -> &'static str {
        match self {
            Trait::Aggression => "aggression",
            Trait::Expansionism => "expansionism",
            Trait::Defensiveness => "defensiveness",
            Trait::Guile => "guile",
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct Genome {
    pub aggression: f64,
    pub expansionism: f64,
    pub defensiveness: f64,
    pub guile: f64,
}

impl Genome {
    pub fn get(&self, t: Trait) -> f64 {
        match t {
            Trait::Aggression => self.aggression,
            Trait::Expansionism => self.expansionism,
            Trait::Defensiveness => self.defensiveness,
            Trait::Guile => self.guile,
        }
    }

    fn adjust(&mut self, t: Trait, delta: f64) {
        let slot = match t {
            Trait::Aggression => &mut self.aggression,
            Trait::Expansionism => &mut self.expansionism,
            Trait::Defensiveness => &mut self.defensiveness,
            Trait::Guile => &mut self.guile,
        };
        *slot = (*slot + delta).clamp(TRAIT_MIN, TRAIT_MAX);
    }

    pub fn to_json(self) -> Json {
        let r = |v: f64| (v * 100.0).round() / 100.0;
        json!({
            "aggression": r(self.aggression),
            "expansionism": r(self.expansionism),
            "defensiveness": r(self.defensiveness),
            "guile": r(self.guile),
        })
    }
}

/// Deterministic jitter in [-span, span] from the community Id.
fn jitter(id: i64, salt: i64, span: f64) -> f64 {
    // A tiny integer hash; deterministic, no RNG (which would
    // break determinism and is unavailable in this crate anyway).
    let mut h = (id.wrapping_mul(2654435761).wrapping_add(salt.wrapping_mul(40503))) as u64;
    h ^= h >> 13;
    h = h.wrapping_mul(0x9E3779B97F4A7C15);
    h ^= h >> 7;
    let unit = (h % 10_000) as f64 / 10_000.0; // [0,1)
    (unit * 2.0 - 1.0) * span
}

/// Seed a genome from the faction type + Id jitter. Looters are
/// born aggressive and guileful; Normal camps defensive.
fn seed(id: i64, ctype: &str) -> Genome {
    let looter = ctype == "Looter";
    let base = |normal: f64, loot: f64| if looter { loot } else { normal };
    Genome {
        aggression: (base(0.35, 0.65) + jitter(id, 1, 0.15)).clamp(TRAIT_MIN, TRAIT_MAX),
        expansionism: (base(0.5, 0.5) + jitter(id, 2, 0.15)).clamp(TRAIT_MIN, TRAIT_MAX),
        defensiveness: (base(0.6, 0.35) + jitter(id, 3, 0.15)).clamp(TRAIT_MIN, TRAIT_MAX),
        guile: (base(0.4, 0.6) + jitter(id, 4, 0.15)).clamp(TRAIT_MIN, TRAIT_MAX),
    }
}

static GENOMES: Mutex<Option<HashMap<i64, Genome>>> = Mutex::new(None);

/// Genome for a faction, seeding it on first sight.
pub fn get_or_seed(id: i64, ctype: &str) -> Genome {
    let mut g = GENOMES.lock();
    let map = g.get_or_insert_with(HashMap::new);
    *map.entry(id).or_insert_with(|| seed(id, ctype))
}

/// Reinforce a trait by a positive or negative outcome. `success`
/// scales the step (a big win moves more than a marginal one).
pub fn reinforce(id: i64, t: Trait, direction_up: bool, magnitude: f64) {
    let mut g = GENOMES.lock();
    let Some(map) = g.as_mut() else { return };
    let Some(genome) = map.get_mut(&id) else { return };
    let step = LEARN_RATE * magnitude.clamp(0.25, 2.0);
    genome.adjust(t, if direction_up { step } else { -step });
}

/// HEREDITY seam (for the conquest phase): blend a victor's
/// genome fraction into a survivor faction the victor absorbs.
#[allow(dead_code)]
pub fn blend_into(survivor_id: i64, victor: Genome, victor_weight: f64) {
    let mut g = GENOMES.lock();
    let Some(map) = g.as_mut() else { return };
    let Some(s) = map.get_mut(&survivor_id) else { return };
    let w = victor_weight.clamp(0.0, 1.0);
    let mix = |a: f64, b: f64| (a * (1.0 - w) + b * w).clamp(TRAIT_MIN, TRAIT_MAX);
    s.aggression = mix(s.aggression, victor.aggression);
    s.expansionism = mix(s.expansionism, victor.expansionism);
    s.defensiveness = mix(s.defensiveness, victor.defensiveness);
    s.guile = mix(s.guile, victor.guile);
}

/// A faction died (consumed / extinct): its genome dies with it.
/// This is the SELECTION half of evolution: unfit trait sets are
/// removed from the map's gene pool.
pub fn remove(id: i64) {
    let mut g = GENOMES.lock();
    if let Some(map) = g.as_mut() {
        map.remove(&id);
    }
}

/// Snapshot every seeded genome (for the status op).
pub fn snapshot() -> Vec<(i64, Genome)> {
    let g = GENOMES.lock();
    match g.as_ref() {
        Some(map) => {
            let mut v: Vec<(i64, Genome)> = map.iter().map(|(k, g)| (*k, *g)).collect();
            v.sort_by_key(|(id, _)| *id);
            v
        }
        None => Vec::new(),
    }
}
