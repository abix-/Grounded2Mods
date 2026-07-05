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
//! Storage is Rust-side, keyed by the stable community `Id`, and
//! PERSISTED: a sidecar file keyed by the save's world seed
//! remembers every genome, lesson, and conscript across hot
//! reloads and game restarts (see the persistence section below).
//! Seeding stays deterministic for anyone the file has never met.

use std::collections::{HashMap, HashSet};

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
    /// Grow / annex / prey appetite. Drives the ambition war
    /// (with aggression) and learns from its outcomes.
    Expansionism,
    /// Fortify / turtle / careful-dealings appetite. Drives the
    /// trade act (trade.rs) and learns from its outcomes.
    Defensiveness,
    /// Steal / extort / manipulate appetite. Drives the theft act
    /// (steal.rs) and learns from its outcomes.
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
    mark_dirty();
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
    mark_dirty();
}

/// A faction died (consumed / extinct): its faction-level genome
/// dies with it.
pub fn remove(id: i64) {
    let mut g = GENOMES.lock();
    if let Some(map) = g.as_mut() {
        map.remove(&id);
    }
    mark_dirty();
}

// ---- per-survivor genomes (the collective model) ---------------------------
//
// The true unit of Darwinian selection is the INDIVIDUAL. Each
// survivor carries their own genome, keyed by character Id, that
// varies at birth (Id jitter), learns from what THAT person
// lived, and dies with them. A settlement's decisions emerge from
// its voting members' individual genomes (survival.rs franchise
// vote). The faction-level genome above is the v1 aggregate; the
// per-survivor map here is the deeper model.

static INDIVIDUALS: Mutex<Option<HashMap<i64, Genome>>> = Mutex::new(None);

/// Members taken BY FORCE (looter press-gang, predation absorb).
/// In a Looter faction they are voiceless (franchise excludes
/// them); a Normal faction lets everyone vote, so the flag only
/// matters for Looters.
static CONSCRIPTS: Mutex<Option<HashSet<i64>>> = Mutex::new(None);

/// Genome for a survivor, seeding on first sight from faction type
/// + the character's own Id jitter (individual variation).
pub fn individual(char_id: i64, ctype: &str) -> Genome {
    let mut g = INDIVIDUALS.lock();
    let map = g.get_or_insert_with(HashMap::new);
    *map.entry(char_id).or_insert_with(|| seed(char_id, ctype))
}

/// Reinforce one survivor's trait by an outcome they lived.
pub fn reinforce_individual(char_id: i64, t: Trait, direction_up: bool, magnitude: f64) {
    let mut g = INDIVIDUALS.lock();
    let Some(map) = g.as_mut() else { return };
    let Some(genome) = map.get_mut(&char_id) else { return };
    let step = LEARN_RATE * magnitude.clamp(0.25, 2.0);
    genome.adjust(t, if direction_up { step } else { -step });
    mark_dirty();
}

/// A survivor died: their genome leaves the pool (individual
/// selection).
pub fn drop_individual(char_id: i64) {
    if let Some(map) = INDIVIDUALS.lock().as_mut() {
        map.remove(&char_id);
    }
    if let Some(set) = CONSCRIPTS.lock().as_mut() {
        set.remove(&char_id);
    }
    mark_dirty();
}

/// Mark a survivor as taken by force (non-core).
pub fn mark_conscript(char_id: i64) {
    CONSCRIPTS
        .lock()
        .get_or_insert_with(HashSet::new)
        .insert(char_id);
    mark_dirty();
}

/// True if this survivor was taken by force (a Looter conscript).
pub fn is_conscript(char_id: i64) -> bool {
    CONSCRIPTS
        .lock()
        .as_ref()
        .map(|s| s.contains(&char_id))
        .unwrap_or(false)
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

// ---- persistence (the genome memory) ----------------------------------------
//
// Learning only matters if it SURVIVES. Everything above is
// session memory: a hot reload or a game restart re-seeds the
// deterministic starting genomes and forgets every lesson and
// every conscript. The sidecar file below is the world's memory:
// keyed by the save's world seed (Session.RandomSeed), restored
// on the first tick with a live game, written at most every 30
// seconds while dirty, and flushed on every shutdown (hot reloads
// call shutdown, so a reload loses nothing).

use std::path::PathBuf;

const PERSIST_WRITE_COOLDOWN_SECS: f32 = 30.0;
const SCHEMA_VERSION: i64 = 1;

static PERSIST_SEED: Mutex<Option<i64>> = Mutex::new(None);
static PERSIST_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PERSIST_LAST_WRITE_BITS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

fn mark_dirty() {
    PERSIST_DIRTY.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn store_path(seed: i64) -> Option<PathBuf> {
    let profile = std::env::var("USERPROFILE").ok()?;
    Some(
        PathBuf::from(profile)
            .join("AppData/LocalLow/Ginormocorp Industries/Survivalist Invisible Strain")
            .join(format!("survivalist-mod.genomes.seed{seed}.json")),
    )
}

/// Full-precision genome json (to_json rounds for display; the
/// memory must not).
fn genome_to_store(g: &Genome) -> Json {
    json!({
        "aggression": g.aggression,
        "expansionism": g.expansionism,
        "defensiveness": g.defensiveness,
        "guile": g.guile,
    })
}

fn genome_from_store(v: &Json) -> Option<Genome> {
    let f = |k: &str| v.get(k).and_then(Json::as_f64);
    Some(Genome {
        aggression: f("aggression")?,
        expansionism: f("expansionism")?,
        defensiveness: f("defensiveness")?,
        guile: f("guile")?,
    })
}

/// Called every tick from lib.rs. Restores once per generation as
/// soon as a game is up; writes while dirty on a cooldown.
pub fn persistence_tick(now: f32) {
    let seed = {
        let mut s = PERSIST_SEED.lock();
        match *s {
            Some(seed) => seed,
            None => {
                let Ok(seed) = crate::common::session_seed() else {
                    return; // menu; try again next tick
                };
                *s = Some(seed);
                restore(seed);
                seed
            }
        }
    };
    if !PERSIST_DIRTY.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let last = f32::from_bits(PERSIST_LAST_WRITE_BITS.load(std::sync::atomic::Ordering::Relaxed));
    if last != 0.0 && now - last < PERSIST_WRITE_COOLDOWN_SECS {
        return;
    }
    PERSIST_LAST_WRITE_BITS.store(now.to_bits(), std::sync::atomic::Ordering::Relaxed);
    write_store(seed);
}

/// Final flush; wired into on_shutdown so hot reloads keep every
/// lesson.
pub fn persist_now() {
    let seed = { *PERSIST_SEED.lock() };
    if let Some(seed) = seed {
        if PERSIST_DIRTY.load(std::sync::atomic::Ordering::Relaxed) {
            write_store(seed);
        }
    }
}

fn restore(seed: i64) {
    let Some(path) = store_path(seed) else { return };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return; // fresh world; nothing to remember yet
    };
    let Ok(v) = serde_json::from_str::<Json>(&text) else {
        return;
    };
    let mut factions = 0usize;
    let mut individuals = 0usize;
    let mut conscripts = 0usize;
    if let Some(map) = v.get("factions").and_then(Json::as_object) {
        let mut g = GENOMES.lock();
        let store = g.get_or_insert_with(HashMap::new);
        for (k, gv) in map {
            if let (Ok(id), Some(genome)) = (k.parse::<i64>(), genome_from_store(gv)) {
                store.insert(id, genome);
                factions += 1;
            }
        }
    }
    if let Some(map) = v.get("individuals").and_then(Json::as_object) {
        let mut g = INDIVIDUALS.lock();
        let store = g.get_or_insert_with(HashMap::new);
        for (k, gv) in map {
            if let (Ok(id), Some(genome)) = (k.parse::<i64>(), genome_from_store(gv)) {
                store.insert(id, genome);
                individuals += 1;
            }
        }
    }
    if let Some(list) = v.get("conscripts").and_then(Json::as_array) {
        let mut c = CONSCRIPTS.lock();
        let store = c.get_or_insert_with(HashSet::new);
        for idv in list {
            if let Some(id) = idv.as_i64() {
                store.insert(id);
                conscripts += 1;
            }
        }
    }
    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        &format!(
            "survivalist-mod: genome memory restored ({factions} faction(s), {individuals} survivor(s), {conscripts} conscript(s)) for world seed {seed}"
        ),
    );
}

fn write_store(seed: i64) {
    let Some(path) = store_path(seed) else { return };
    let factions: serde_json::Map<String, Json> = GENOMES
        .lock()
        .as_ref()
        .map(|m| {
            m.iter()
                .map(|(id, g)| (id.to_string(), genome_to_store(g)))
                .collect()
        })
        .unwrap_or_default();
    let individuals: serde_json::Map<String, Json> = INDIVIDUALS
        .lock()
        .as_ref()
        .map(|m| {
            m.iter()
                .map(|(id, g)| (id.to_string(), genome_to_store(g)))
                .collect()
        })
        .unwrap_or_default();
    let conscripts: Vec<Json> = CONSCRIPTS
        .lock()
        .as_ref()
        .map(|s| {
            let mut ids: Vec<i64> = s.iter().copied().collect();
            ids.sort_unstable();
            ids.into_iter().map(Json::from).collect()
        })
        .unwrap_or_default();
    let doc = json!({
        "schema_version": SCHEMA_VERSION,
        "factions": factions,
        "individuals": individuals,
        "conscripts": conscripts,
    });
    // Atomic save: temp + rename, so a crash mid-write never
    // truncates the world's memory.
    let tmp = path.with_extension("json.tmp");
    let Ok(text) = serde_json::to_string(&doc) else { return };
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
        PERSIST_DIRTY.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
