//! Named uniques: the more-to-discover pillar's first slice
//! (docs/status.md "More to discover (named uniques)").
//!
//! One-of-a-kind items that enter ONLY with the incursion that
//! fits them, announced by the chronicle, and never twice in one
//! save. The first unique is The Colonel's Rifle: each military
//! remnant band that crosses the edge has a low chance to carry
//! it in, until one does; after that it exists in the world and
//! never enters again. The band ARRIVES carrying it (the edge is
//! the sanctioned faucet for new loot, docs/status.md
//! "No cheating"), so nothing is conjured inside the map.
//!
//! The entered-flag persists in a seed-keyed sidecar (the genome
//! memory's pattern, genome.rs), written at the moment of entry,
//! so neither hot reloads nor save reloads double-spawn it.

use std::path::PathBuf;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel};

use crate::common::{handle_of, on_main_thread, own, session_seed, with};
use crate::quality::{find_prototype, rng};

/// The prototype shipped in story/Equipment/ColonelsRifle.xml.
const COLONELS_RIFLE: &str = "ColonelsRifle";

/// Percent chance per military band until it enters; then never.
const ENTER_ROLL_PCT: u64 = 20;

const SCHEMA_VERSION: i64 = 1;

/// Uniques that have entered THIS save (lazy from the sidecar).
static ENTERED: Mutex<Option<Vec<String>>> = Mutex::new(None);
/// Who carried the last unique in, for unique_status.
static LAST_CARRIER: Mutex<Option<String>> = Mutex::new(None);
/// The data-not-loaded line logs once per generation.
static MISSING_LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn store_path(seed: i64) -> Option<PathBuf> {
    let profile = std::env::var("USERPROFILE").ok()?;
    Some(
        PathBuf::from(profile)
            .join("AppData/LocalLow/Ginormocorp Industries/Survivalist Invisible Strain")
            .join(format!("survivalist-mod.uniques.seed{seed}.json")),
    )
}

/// The entered list for this save, loaded once per generation.
fn entered(seed: i64) -> Vec<String> {
    let mut slot = ENTERED.lock();
    if let Some(list) = slot.as_ref() {
        return list.clone();
    }
    let list: Vec<String> = store_path(seed)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<Json>(&text).ok())
        .and_then(|v| {
            v.get("entered").and_then(Json::as_array).map(|a| {
                a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()
            })
        })
        .unwrap_or_default();
    *slot = Some(list.clone());
    list
}

/// Record an entry and write the sidecar NOW (entries are rare,
/// one-time, and must survive any reload; same atomic
/// tmp-then-rename shape as the genome store).
fn mark_entered(seed: i64, name: &str) {
    let mut slot = ENTERED.lock();
    let list = slot.get_or_insert_with(Vec::new);
    if !list.iter().any(|x| x == name) {
        list.push(name.to_string());
    }
    let Some(path) = store_path(seed) else { return };
    let text = json!({
        "schema_version": SCHEMA_VERSION,
        "entered": list.clone(),
    })
    .to_string();
    let tmp = path.with_extension("json.tmp");
    if !(std::fs::write(&tmp, &text).is_ok() && std::fs::rename(&tmp, &path).is_ok()) {
        mono::log(
            LogLevel::Warn,
            "survivalist-mod: unique: sidecar write failed; a reload could re-enter a unique",
        );
    }
}

/// The military remnants' unique: roll The Colonel's Rifle into
/// the band's hands, once per save. Called by incursion.rs right
/// after a military band spawns; best-effort.
pub fn maybe_enter_with_military(band_h: i32, now: f32) {
    let Ok(seed) = session_seed() else { return };
    if entered(seed).iter().any(|x| x == COLONELS_RIFLE) {
        return;
    }
    if rng(now, 7793, 100) >= ENTER_ROLL_PCT {
        return;
    }
    let proto_h = match find_prototype(COLONELS_RIFLE) {
        Ok(Some(h)) => h,
        Ok(None) => {
            if MISSING_LOGGED.swap(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: unique: {COLONELS_RIFLE} prototype not loaded; nothing storied crosses (restart the story to load Equipment/ColonelsRifle.xml)"
                    ),
                );
            }
            return;
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: unique: prototype lookup failed: {e}"),
            );
            return;
        }
    };
    let result = give_to_band(band_h, proto_h);
    drop(own(proto_h));
    match result {
        Ok(Some(who)) => {
            mark_entered(seed, COLONELS_RIFLE);
            *LAST_CARRIER.lock() = Some(who.clone());
            crate::chronicle::post(
                "the soldiers that crossed carry something storied: The Colonel's Rifle",
            );
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: unique: The Colonel's Rifle crossed the edge in {who}'s hands; it will not come again"
                ),
            );
        }
        Ok(None) => {} // nobody living to carry it; the roll passes by
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: unique: entry failed: {e}"),
            );
        }
    }
}

/// Spawn the unique into the hands of the band's leader (or its
/// first living member). Returns the carrier's name.
fn give_to_band(band_h: i32, proto_h: i32) -> Result<Option<String>, String> {
    // Prefer the leader: the storied gun belongs to whoever leads.
    let mut carrier_h = with(band_h, |b| {
        b.read_field("Leader").ok().as_ref().and_then(handle_of)
    });
    if let Some(h) = carrier_h {
        let alive = with(h, |c| {
            c.invoke("get_AliveAndNotZombie", &json!([]))
                .map(|v| v == json!(true))
                .unwrap_or(false)
        });
        if !alive {
            drop(own(h));
            carrier_h = None;
        }
    }
    let carrier_h = match carrier_h {
        Some(h) => Some(h),
        None => first_living_member(band_h)?,
    };
    let Some(carrier_h) = carrier_h else {
        return Ok(None);
    };
    let item = mono::invoke_static("Equipment", "Spawn", &json!([{ "handle": proto_h }, 1]))?;
    let Some(item_h) = handle_of(&item) else {
        drop(own(carrier_h));
        return Err("Equipment.Spawn gave no item".into());
    };
    let who = with(carrier_h, |c| {
        let _ = c.invoke("Add", &json!([{ "handle": carrier_h }, { "handle": item_h }]));
        c.invoke("GetDisplayNameString", &json!([]))
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "<unnamed>".into())
    });
    drop(own(item_h));
    drop(own(carrier_h));
    Ok(Some(who))
}

fn first_living_member(band_h: i32) -> Result<Option<i32>, String> {
    let Some(m_h) = with(band_h, |b| b.read_field("Members").ok().as_ref().and_then(handle_of))
    else {
        return Ok(None);
    };
    let mlist = own(m_h);
    let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    for i in 0..count {
        let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        let alive = with(h, |c| {
            c.invoke("get_AliveAndNotZombie", &json!([]))
                .map(|v| v == json!(true))
                .unwrap_or(false)
        });
        if alive {
            return Ok(Some(h));
        }
        drop(own(h));
    }
    Ok(None)
}

// ---- ops ---------------------------------------------------------------------

pub fn register_ops() {
    OP_REGISTRY.register(OpDef::new(
        "unique_status",
        "Named uniques: which have entered this save, whether the data is loaded, and who carried the last one in.",
        "{}",
        unique_status,
    ));
}

fn unique_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let loaded = match find_prototype(COLONELS_RIFLE) {
            Ok(Some(h)) => {
                drop(own(h));
                true
            }
            _ => false,
        };
        let entered_list = match session_seed() {
            Ok(seed) => entered(seed),
            Err(_) => Vec::new(),
        };
        Ok(json!({
            "colonels_rifle_loaded": loaded,
            "entered": entered_list,
            "last_entry_carrier": LAST_CARRIER.lock().clone(),
        }))
    })
}
