//! The quality system, first slice (docs/status.md "Quality
//! system (higher quality, lower chance)"; research + design in
//! faction-war.md "The quality system").
//!
//! Quality tiers are distinct item TYPES: the engine has no
//! per-item quality field and stacks merge by type, so a "fine"
//! item is a separate, better, rarer prototype shipped as story
//! XML (story/Equipment/FineAssaultRifle.xml), and everything
//! downstream (price, damage, saves, names) rides vanilla.
//!
//! This slice is THE EDGE ROLLS QUALITY: when a military remnant
//! band crosses the edge (incursion.rs), each common assault
//! rifle in a spawned hand has a low chance to be a Fine Assault
//! Rifle instead, capped at one per band. The swap is net zero
//! items (take the common rifle, delete it, spawn the fine type
//! into the same hand) and runs ONLY on edge-spawned bands, the
//! sanctioned faucet of the no-cheating boundary.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoType};

use crate::common::{handle_of, on_main_thread, own, with};

/// The tier variant shipped in story/Equipment/FineAssaultRifle.xml.
const FINE_ASSAULT_RIFLE: &str = "FineAssaultRifle";

/// The common type it upgrades (vanilla BaseStory prototype).
const BASE_ASSAULT_RIFLE: &str = "AssaultRifle";

/// Percent chance per carried common rifle; at most one swap per
/// band. Higher quality, lower chance.
const FINE_ROLL_PCT: u64 = 25;

static SWAPS_TOTAL: AtomicU32 = AtomicU32::new(0);
static LAST_SWAP: Mutex<Option<String>> = Mutex::new(None);
/// The quest-data-not-loaded line logs once per generation, not
/// once per band.
static MISSING_LOGGED: AtomicU32 = AtomicU32::new(0);

/// The edge roll: upgrade at most one common assault rifle in the
/// band's hands to the fine tier. Called by incursion.rs right
/// after a military remnant band spawns; best-effort (a failure
/// leaves the band exactly as the game spawned it).
pub fn upgrade_band_gear(band_h: i32, now: f32) {
    let proto_h = match find_prototype(FINE_ASSAULT_RIFLE) {
        Ok(Some(h)) => h,
        Ok(None) => {
            if MISSING_LOGGED.swap(1, Ordering::Relaxed) == 0 {
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: quality: {FINE_ASSAULT_RIFLE} prototype not loaded; the edge rolls nothing (restart the story to load Equipment/FineAssaultRifle.xml)"
                    ),
                );
            }
            return;
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: quality: prototype lookup failed: {e}"),
            );
            return;
        }
    };
    if let Err(e) = roll_band(band_h, proto_h, now) {
        mono::log(
            LogLevel::Warn,
            &format!("survivalist-mod: quality: edge roll failed: {e}"),
        );
    }
    drop(own(proto_h));
}

fn roll_band(band_h: i32, proto_h: i32, now: f32) -> Result<(), String> {
    let Some(m_h) = with(band_h, |b| b.read_field("Members").ok().as_ref().and_then(handle_of))
    else {
        return Ok(());
    };
    let mlist = own(m_h);
    let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    for mi in 0..count {
        let Some(mh) = handle_of(&mlist.invoke("get_Item", &json!([mi]))?) else {
            continue;
        };
        let member = own(mh);
        let alive = member
            .invoke("get_AliveAndNotZombie", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false);
        if !alive {
            continue;
        }
        let Some(inv_h) = handle_of(&member.read_field("Inventory")?) else {
            continue;
        };
        let inv = own(inv_h);
        let n = inv.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
        for i in 0..n {
            let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([i]))?) else {
                continue;
            };
            let item = own(item_h);
            let is_base = handle_of(&item.invoke("GetPrototype", &json!([]))?)
                .map(|ph| {
                    with(ph, |p| {
                        let name = p
                            .read_field("Name")
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string));
                        drop(own(ph));
                        name
                    })
                })
                .flatten()
                .map(|name| name == BASE_ASSAULT_RIFLE)
                .unwrap_or(false);
            if !is_base {
                continue;
            }
            // Higher quality, lower chance.
            if rng(now, (mi as u64) * 131 + i as u64 + 7, 100) >= FINE_ROLL_PCT {
                continue;
            }
            // The swap, net zero items: the common rifle leaves
            // the world, the fine one lands in the same hand.
            let taken = inv.invoke(
                "Take",
                &json!([{ "handle": mh }, { "handle": item_h }, 1]),
            )?;
            let Some(taken_h) = handle_of(&taken) else {
                continue;
            };
            if let Err(e) = with(taken_h, |t| t.invoke("Delete", &json!([]))) {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: quality: delete of the common rifle failed: {e}"),
                );
            }
            drop(own(taken_h));
            let fine = mono::invoke_static(
                "Equipment",
                "Spawn",
                &json!([{ "handle": proto_h }, 1]),
            )?;
            let Some(fine_h) = handle_of(&fine) else {
                return Err("Equipment.Spawn gave no item".into());
            };
            let _ = member.invoke("Add", &json!([{ "handle": mh }, { "handle": fine_h }]));
            drop(own(fine_h));
            let who = member
                .invoke("GetDisplayNameString", &json!([]))
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "<unnamed>".into());
            SWAPS_TOTAL.fetch_add(1, Ordering::Relaxed);
            *LAST_SWAP.lock() = Some(who.clone());
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: quality: a FINE assault rifle crossed the edge in {who}'s hands"
                ),
            );
            return Ok(()); // at most one per band
        }
    }
    Ok(())
}

/// Walk GameImpl.Instance.CurrentStories and ask each loaded
/// story for the prototype (Story.FindEquipmentPrototypeByName);
/// the one that loaded our XML answers.
fn find_prototype(name: &str) -> Result<Option<i32>, String> {
    let game = MonoType::find("GameImpl")
        .and_then(|t| t.singleton_instance())
        .ok_or("GameImpl.Instance not found")?;
    let Some(list_h) = handle_of(&game.read_field("CurrentStories")?) else {
        return Ok(None);
    };
    let list = own(list_h);
    let n = list.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    for i in 0..n {
        let Some(story_h) = handle_of(&list.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        let story = own(story_h);
        if let Ok(p) = story.invoke("FindEquipmentPrototypeByName", &json!([name])) {
            if let Some(ph) = handle_of(&p) {
                return Ok(Some(ph));
            }
        }
    }
    Ok(None)
}

/// A pseudo-random value in [0, n): the incursion rng shape (a
/// hash of the fire time and a salt).
fn rng(now: f32, salt: u64, n: u64) -> u64 {
    let mut h = (now.to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h % n.max(1)
}

// ---- ops ---------------------------------------------------------------------

pub fn register_ops() {
    OP_REGISTRY.register(OpDef::new(
        "quality_status",
        "The quality system's live state: is the fine prototype loaded, how many edge swaps happened, who carried the last one.",
        "{}",
        quality_status,
    ));
}

fn quality_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let loaded = match find_prototype(FINE_ASSAULT_RIFLE) {
            Ok(Some(h)) => {
                drop(own(h));
                true
            }
            _ => false,
        };
        Ok(json!({
            "fine_assault_rifle_loaded": loaded,
            "swaps_total": SWAPS_TOTAL.load(Ordering::Relaxed),
            "last_swap": LAST_SWAP.lock().clone(),
        }))
    })
}
