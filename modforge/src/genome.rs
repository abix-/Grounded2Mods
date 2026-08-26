//! Genome: trait pools with reinforcement learning and persistence.
//!
//! Each entity (faction, NPC, survivor) carries a small vector of
//! named f64 trait values clamped to a configurable range. Traits
//! are seeded deterministically from an id and a game-supplied
//! function, reinforced by the outcomes of the choices they drive,
//! blended on heredity events, and persisted to a sidecar JSON
//! file keyed by world seed.
//!
//! Engine-agnostic. Games supply their own trait names, seed
//! function, and store path.
//!
//! ```ignore
//! use modforge::genome::{Pool, PoolConfig};
//!
//! static FACTIONS: Pool = Pool::new(PoolConfig {
//!     traits: &["aggression", "expansionism", "defensiveness", "guile"],
//!     learn_rate: 0.06,
//!     min: 0.05,
//!     max: 0.95,
//! });
//!
//! // seed on first sight (game supplies the seed function)
//! let g = FACTIONS.get_or_seed(faction_id, || vec![0.5, 0.5, 0.6, 0.4]);
//!
//! // reinforce from an outcome
//! FACTIONS.reinforce(faction_id, 0, true, 1.0); // trait 0 up
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

pub struct PoolConfig {
    pub traits: &'static [&'static str],
    pub learn_rate: f64,
    pub min: f64,
    pub max: f64,
}

/// An engine-independent franchise vote over scored individuals.
/// Consumers decide who may vote and how each voter is scored;
/// this type owns the tally, majority rule, mean score, and voter
/// identity collection used for later reinforcement.
pub struct Ballot {
    floor: f64,
    score_sum: f64,
    pub franchise: i64,
    pub votes_for: i64,
    pub voter_ids: Vec<i64>,
}

impl Ballot {
    pub const fn new(floor: f64) -> Self {
        Self {
            floor,
            score_sum: 0.0,
            franchise: 0,
            votes_for: 0,
            voter_ids: Vec::new(),
        }
    }

    /// Add one enfranchised voter. Returns whether that voter
    /// voted yes, so consumers can select willing participants.
    pub fn cast(&mut self, voter_id: i64, score: f64) -> bool {
        let voted_for = score >= self.floor;
        self.franchise += 1;
        self.score_sum += score;
        self.votes_for += if voted_for { 1 } else { 0 };
        self.voter_ids.push(voter_id);
        voted_for
    }

    pub fn has_majority(&self) -> bool {
        majority(self.votes_for, self.franchise)
    }

    pub fn mean_score(&self) -> f64 {
        if self.franchise > 0 {
            self.score_sum / self.franchise as f64
        } else {
            0.0
        }
    }
}

/// Whether the yes votes form a strict majority of a non-empty
/// franchise.
pub fn majority(votes_for: i64, franchise: i64) -> bool {
    franchise > 0 && votes_for * 2 > franchise
}

pub struct Pool {
    config: PoolConfig,
    entries: Mutex<Option<HashMap<i64, Vec<f64>>>>,
    dirty: AtomicBool,
}

impl Pool {
    pub const fn new(config: PoolConfig) -> Self {
        Self {
            config,
            entries: Mutex::new(None),
            dirty: AtomicBool::new(false),
        }
    }

    pub fn trait_count(&self) -> usize {
        self.config.traits.len()
    }

    pub fn trait_name(&self, index: usize) -> &'static str {
        self.config.traits[index]
    }

    pub fn get_or_seed(&self, id: i64, seed_fn: impl FnOnce() -> Vec<f64>) -> Vec<f64> {
        let mut g = self.entries.lock();
        let map = g.get_or_insert_with(HashMap::new);
        map.entry(id)
            .or_insert_with(|| {
                let mut v = seed_fn();
                for val in &mut v {
                    *val = val.clamp(self.config.min, self.config.max);
                }
                v
            })
            .clone()
    }

    pub fn get(&self, id: i64) -> Option<Vec<f64>> {
        self.entries.lock().as_ref()?.get(&id).cloned()
    }

    pub fn reinforce(&self, id: i64, trait_index: usize, direction_up: bool, magnitude: f64) {
        let mut g = self.entries.lock();
        let Some(map) = g.as_mut() else { return };
        let Some(traits) = map.get_mut(&id) else {
            return;
        };
        if trait_index >= traits.len() {
            return;
        }
        let step = self.config.learn_rate * magnitude.clamp(0.25, 2.0);
        let delta = if direction_up { step } else { -step };
        traits[trait_index] = (traits[trait_index] + delta).clamp(self.config.min, self.config.max);
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn blend_into(&self, survivor_id: i64, victor: &[f64], victor_weight: f64) {
        let mut g = self.entries.lock();
        let Some(map) = g.as_mut() else { return };
        let Some(s) = map.get_mut(&survivor_id) else {
            return;
        };
        let w = victor_weight.clamp(0.0, 1.0);
        for (i, val) in s.iter_mut().enumerate() {
            if let Some(&v) = victor.get(i) {
                *val = (*val * (1.0 - w) + v * w).clamp(self.config.min, self.config.max);
            }
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn remove(&self, id: i64) {
        let mut g = self.entries.lock();
        if let Some(map) = g.as_mut() {
            map.remove(&id);
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Vec<(i64, Vec<f64>)> {
        let g = self.entries.lock();
        match g.as_ref() {
            Some(map) => {
                let mut v: Vec<(i64, Vec<f64>)> =
                    map.iter().map(|(k, g)| (*k, g.clone())).collect();
                v.sort_by_key(|(id, _)| *id);
                v
            }
            None => Vec::new(),
        }
    }

    pub fn to_json(&self, traits: &[f64]) -> Json {
        let mut obj = serde_json::Map::new();
        for (i, &name) in self.config.traits.iter().enumerate() {
            let v = traits.get(i).copied().unwrap_or(0.0);
            obj.insert(name.to_string(), json!((v * 100.0).round() / 100.0));
        }
        Json::Object(obj)
    }

    pub fn to_json_full(&self, traits: &[f64]) -> Json {
        let mut obj = serde_json::Map::new();
        for (i, &name) in self.config.traits.iter().enumerate() {
            let v = traits.get(i).copied().unwrap_or(0.0);
            obj.insert(name.to_string(), json!(v));
        }
        Json::Object(obj)
    }

    pub fn from_json(&self, v: &Json) -> Option<Vec<f64>> {
        let mut traits = Vec::with_capacity(self.config.traits.len());
        for &name in self.config.traits {
            traits.push(v.get(name).and_then(Json::as_f64)?);
        }
        Some(traits)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    pub fn restore_from_json(&self, section: &Json) {
        if let Some(map) = section.as_object() {
            let mut g = self.entries.lock();
            let store = g.get_or_insert_with(HashMap::new);
            for (k, gv) in map {
                if let (Ok(id), Some(traits)) = (k.parse::<i64>(), self.from_json(gv)) {
                    store.insert(id, traits);
                }
            }
        }
    }

    pub fn store_to_json(&self) -> Json {
        let g = self.entries.lock();
        let map: serde_json::Map<String, Json> = g
            .as_ref()
            .map(|m| {
                m.iter()
                    .map(|(id, traits)| (id.to_string(), self.to_json_full(traits)))
                    .collect()
            })
            .unwrap_or_default();
        Json::Object(map)
    }
}

/// Reinforce several traits for one entity in a pool.
pub fn reinforce_traits(
    pool: &Pool,
    id: i64,
    trait_indices: &[usize],
    direction_up: bool,
    magnitude: f64,
) {
    for &trait_index in trait_indices {
        pool.reinforce(id, trait_index, direction_up, magnitude);
    }
}

/// Reinforce the same traits for every individual that participated
/// in a decision.
pub fn reinforce_voters(
    individuals: &Pool,
    voter_ids: &[i64],
    trait_indices: &[usize],
    direction_up: bool,
    magnitude: f64,
) {
    for &trait_index in trait_indices {
        for &voter_id in voter_ids {
            individuals.reinforce(voter_id, trait_index, direction_up, magnitude);
        }
    }
}

/// Reinforce participating individuals and their faction aggregate
/// from the same outcome.
pub fn reinforce_collective(
    factions: &Pool,
    individuals: &Pool,
    faction_id: i64,
    voter_ids: &[i64],
    trait_indices: &[usize],
    direction_up: bool,
    magnitude: f64,
) {
    reinforce_voters(
        individuals,
        voter_ids,
        trait_indices,
        direction_up,
        magnitude,
    );
    reinforce_traits(factions, faction_id, trait_indices, direction_up, magnitude);
}

// ---- deterministic jitter ---------------------------------------------------

pub fn jitter(id: i64, salt: i64, span: f64) -> f64 {
    let mut h = (id
        .wrapping_mul(2654435761)
        .wrapping_add(salt.wrapping_mul(40503))) as u64;
    h ^= h >> 13;
    h = h.wrapping_mul(0x9E3779B97F4A7C15);
    h ^= h >> 7;
    let unit = (h % 10_000) as f64 / 10_000.0;
    (unit * 2.0 - 1.0) * span
}

// ---- persistence helper -----------------------------------------------------

pub struct GenomeStore {
    seed: Mutex<Option<i64>>,
    last_write_bits: AtomicU32,
    write_cooldown_secs: f32,
}

impl GenomeStore {
    pub const fn new(write_cooldown_secs: f32) -> Self {
        Self {
            seed: Mutex::new(None),
            last_write_bits: AtomicU32::new(0),
            write_cooldown_secs,
        }
    }

    pub fn seed(&self) -> Option<i64> {
        *self.seed.lock()
    }

    pub fn persistence_tick(
        &self,
        now: f32,
        seed_fn: impl FnOnce() -> Result<i64, String>,
        pools: &[&Pool],
        store_path: impl Fn(i64) -> Option<PathBuf>,
        build_doc: impl FnOnce(i64) -> Json,
        on_restore: impl FnOnce(i64, &Json),
    ) {
        let seed = {
            let mut s = self.seed.lock();
            match *s {
                Some(seed) => seed,
                None => {
                    let Ok(seed) = seed_fn() else {
                        return;
                    };
                    *s = Some(seed);
                    if let Some(path) = store_path(seed) {
                        if let Ok(text) = std::fs::read_to_string(&path) {
                            if let Ok(v) = serde_json::from_str::<Json>(&text) {
                                on_restore(seed, &v);
                            }
                        }
                    }
                    seed
                }
            }
        };

        let any_dirty = pools.iter().any(|p| p.is_dirty());
        if !any_dirty {
            return;
        }
        let last = f32::from_bits(self.last_write_bits.load(Ordering::Relaxed));
        if last != 0.0 && now - last < self.write_cooldown_secs {
            return;
        }
        self.last_write_bits.store(now.to_bits(), Ordering::Relaxed);
        self.write_now(seed, pools, store_path, build_doc);
    }

    pub fn flush(
        &self,
        pools: &[&Pool],
        store_path: impl Fn(i64) -> Option<PathBuf>,
        build_doc: impl FnOnce(i64) -> Json,
    ) {
        let seed = { *self.seed.lock() };
        if let Some(seed) = seed {
            if pools.iter().any(|p| p.is_dirty()) {
                self.write_now(seed, pools, store_path, build_doc);
            }
        }
    }

    fn write_now(
        &self,
        seed: i64,
        pools: &[&Pool],
        store_path: impl Fn(i64) -> Option<PathBuf>,
        build_doc: impl FnOnce(i64) -> Json,
    ) {
        let Some(path) = store_path(seed) else { return };
        let doc = build_doc(seed);
        let tmp = path.with_extension("json.tmp");
        let Ok(text) = serde_json::to_string(&doc) else {
            return;
        };
        if std::fs::write(&tmp, &text).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            for p in pools {
                p.clear_dirty();
            }
        }
    }
}
