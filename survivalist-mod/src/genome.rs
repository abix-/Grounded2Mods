//! Faction trait genome + learning (docs/faction-war.md "THE
//! VISION: a Darwinian world" + the learning layer).
//!
//! Delegates to [`modforge::genome`] for the pool, reinforcement,
//! blending, and persistence machinery. This module wires the
//! game-specific trait names, seed function, conscript set, and
//! store path.

use std::collections::HashSet;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::genome::{self, GenomeStore, Pool, PoolConfig};

const JITTER_SPAN: f64 = 0.15;

pub const AGGRESSION: usize = 0;
pub const EXPANSIONISM: usize = 1;
pub const DEFENSIVENESS: usize = 2;
pub const GUILE: usize = 3;

static POOL_CONFIG: PoolConfig = PoolConfig {
    traits: &["aggression", "expansionism", "defensiveness", "guile"],
    learn_rate: 0.06,
    min: 0.05,
    max: 0.95,
};

static FACTIONS: Pool = Pool::new(PoolConfig {
    traits: POOL_CONFIG.traits,
    learn_rate: POOL_CONFIG.learn_rate,
    min: POOL_CONFIG.min,
    max: POOL_CONFIG.max,
});

static INDIVIDUALS: Pool = Pool::new(PoolConfig {
    traits: POOL_CONFIG.traits,
    learn_rate: POOL_CONFIG.learn_rate,
    min: POOL_CONFIG.min,
    max: POOL_CONFIG.max,
});

static STORE: GenomeStore = GenomeStore::new(30.0);

static CONSCRIPTS: Mutex<Option<HashSet<i64>>> = Mutex::new(None);

fn seed(id: i64, ctype: &str) -> Vec<f64> {
    let looter = ctype == "Looter";
    let base = |normal: f64, loot: f64| if looter { loot } else { normal };
    vec![
        base(0.35, 0.65) + genome::jitter(id, 1, JITTER_SPAN),
        base(0.5, 0.5) + genome::jitter(id, 2, JITTER_SPAN),
        base(0.6, 0.35) + genome::jitter(id, 3, JITTER_SPAN),
        base(0.4, 0.6) + genome::jitter(id, 4, JITTER_SPAN),
    ]
}

pub fn get_or_seed(id: i64, ctype: &str) -> Vec<f64> {
    FACTIONS.get_or_seed(id, || seed(id, ctype))
}

pub fn reinforce(id: i64, trait_index: usize, direction_up: bool, magnitude: f64) {
    FACTIONS.reinforce(id, trait_index, direction_up, magnitude);
}

pub fn reinforce_traits(id: i64, trait_indices: &[usize], direction_up: bool, magnitude: f64) {
    genome::reinforce_traits(&FACTIONS, id, trait_indices, direction_up, magnitude);
}

#[allow(dead_code)]
pub fn blend_into(survivor_id: i64, victor: &[f64], victor_weight: f64) {
    FACTIONS.blend_into(survivor_id, victor, victor_weight);
}

pub fn remove(id: i64) {
    FACTIONS.remove(id);
}

pub fn individual(char_id: i64, ctype: &str) -> Vec<f64> {
    INDIVIDUALS.get_or_seed(char_id, || seed(char_id, ctype))
}

pub fn reinforce_individual(char_id: i64, trait_index: usize, direction_up: bool, magnitude: f64) {
    INDIVIDUALS.reinforce(char_id, trait_index, direction_up, magnitude);
}

pub fn reinforce_voters(
    voter_ids: &[i64],
    trait_indices: &[usize],
    direction_up: bool,
    magnitude: f64,
) {
    genome::reinforce_voters(
        &INDIVIDUALS,
        voter_ids,
        trait_indices,
        direction_up,
        magnitude,
    );
}

pub fn reinforce_collective(
    faction_id: i64,
    voter_ids: &[i64],
    trait_indices: &[usize],
    direction_up: bool,
    magnitude: f64,
) {
    genome::reinforce_collective(
        &FACTIONS,
        &INDIVIDUALS,
        faction_id,
        voter_ids,
        trait_indices,
        direction_up,
        magnitude,
    );
}

pub fn drop_individual(char_id: i64) {
    INDIVIDUALS.remove(char_id);
    if let Some(set) = CONSCRIPTS.lock().as_mut() {
        set.remove(&char_id);
    }
    FACTIONS.mark_dirty();
}

pub fn mark_conscript(char_id: i64) {
    CONSCRIPTS
        .lock()
        .get_or_insert_with(HashSet::new)
        .insert(char_id);
    FACTIONS.mark_dirty();
}

pub fn is_conscript(char_id: i64) -> bool {
    CONSCRIPTS
        .lock()
        .as_ref()
        .map(|s| s.contains(&char_id))
        .unwrap_or(false)
}

pub fn snapshot() -> Vec<(i64, Vec<f64>)> {
    FACTIONS.snapshot()
}

pub fn to_json(traits: &[f64]) -> Json {
    FACTIONS.to_json(traits)
}

fn store_path(seed: i64) -> Option<PathBuf> {
    let profile = std::env::var("USERPROFILE").ok()?;
    Some(
        PathBuf::from(profile)
            .join("AppData/LocalLow/Ginormocorp Industries/Survivalist Invisible Strain")
            .join(format!("survivalist-mod.genomes.seed{seed}.json")),
    )
}

fn build_doc(_seed: i64) -> Json {
    let conscripts: Vec<Json> = CONSCRIPTS
        .lock()
        .as_ref()
        .map(|s| {
            let mut ids: Vec<i64> = s.iter().copied().collect();
            ids.sort_unstable();
            ids.into_iter().map(Json::from).collect()
        })
        .unwrap_or_default();
    json!({
        "schema_version": 1,
        "factions": FACTIONS.store_to_json(),
        "individuals": INDIVIDUALS.store_to_json(),
        "conscripts": conscripts,
    })
}

fn on_restore(seed: i64, v: &Json) {
    FACTIONS.restore_from_json(&v["factions"]);
    INDIVIDUALS.restore_from_json(&v["individuals"]);
    let mut conscripts = 0usize;
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
    let factions = FACTIONS.snapshot().len();
    let individuals = INDIVIDUALS.snapshot().len();
    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        &format!(
            "survivalist-mod: genome memory restored ({factions} faction(s), {individuals} survivor(s), {conscripts} conscript(s)) for world seed {seed}"
        ),
    );
}

pub fn persistence_tick(now: f32) {
    STORE.persistence_tick(
        now,
        || crate::common::session_seed(),
        &[&FACTIONS, &INDIVIDUALS],
        store_path,
        build_doc,
        on_restore,
    );
}

pub fn persist_now() {
    STORE.flush(&[&FACTIONS, &INDIVIDUALS], store_path, build_doc);
}
