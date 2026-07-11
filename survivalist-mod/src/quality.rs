//! The quality system (docs/status.md "Quality system (higher
//! quality, lower chance)"; research + design in faction-war.md
//! "The quality system").
//!
//! Quality tiers are distinct item TYPES: the engine has no
//! per-item quality field and stacks merge by type, so a tiered
//! item is a separate, better, rarer prototype, and everything
//! downstream (price, damage, saves, names) rides vanilla. The
//! full variant set is GENERATED, never hand-authored:
//! scripts/generate_quality.ps1 reads every vanilla weapon and
//! armor definition and writes <Base>_<Tier><Sibling>.xml into
//! story/Equipment with per-tier stat/price/recoil MULTIPLIERS
//! (the knobs live in that script). Factorio naming: Uncommon,
//! Rare, Epic, Legendary above the vanilla Normal; named uniques
//! (unique.rs) sit above the whole ladder.
//!
//! Each tier ships several statistical SIBLINGS with jittered
//! stats sharing one display name, so two Rare rifles are
//! usually not exactly the same (real per-item stat ranges are
//! impossible: stats live on the type).
//!
//! THE EDGE ROLLS QUALITY: every weapon and armor piece in an
//! edge-spawned band's hands rolls a tier independently, with
//! odds set by the sender (military remnants roll best, raiders
//! lower). The swap is net zero items (Take + Delete the common
//! piece, Equipment.Spawn the tiered one into the same hand) and
//! runs ONLY on edge-spawned bands, the sanctioned faucet of the
//! no-cheating boundary. Rust keeps no item lists: the variant
//! type name is derived by convention and the swap happens only
//! if that type exists.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoType};

use crate::common::{handle_of, on_main_thread, own, with};

/// Tier names, best first (Factorio naming). Must match the
/// generator's $Tiers.
const TIER_NAMES: [&str; 4] = ["Legendary", "Epic", "Rare", "Uncommon"];

/// Per-sender tier odds in PER MILLE, best tier first, evaluated
/// cumulatively from the top. Military remnants carry the best;
/// raiders roll lower. Higher quality, lower chance.
const MILITARY_ODDS: [u64; 4] = [10, 40, 100, 200]; // 1%, 4%, 10%, 20%
const RAIDER_ODDS: [u64; 4] = [3, 15, 50, 120]; // 0.3%, 1.5%, 5%, 12%

/// Statistical siblings per tier. Must match the generator's
/// $Siblings.
const SIBLINGS: u64 = 3;

/// A generated prototype that always exists when the variant set
/// is loaded; its absence means the story has not loaded our
/// Equipment XML yet (loads at story restart).
const CANARY: &str = "AssaultRifle_Uncommon1";

/// Swaps per tier (indexed like TIER_NAMES), for quality_status.
static SWAPS: [AtomicU32; 4] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];
static LAST_SWAP: Mutex<Option<String>> = Mutex::new(None);
/// The data-not-loaded line logs once per generation.
static MISSING_LOGGED: AtomicU32 = AtomicU32::new(0);

/// The edge roll: every weapon or armor piece carried by the band
/// rolls a tier by the sender's odds. Called by incursion.rs
/// right after an edge band spawns; best-effort (a failure leaves
/// the band exactly as the game spawned it).
pub fn upgrade_band_gear(band_h: i32, now: f32, military: bool) {
    match find_prototype(CANARY) {
        Ok(Some(h)) => drop(own(h)),
        Ok(None) => {
            if MISSING_LOGGED.swap(1, Ordering::Relaxed) == 0 {
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: quality: variant data not loaded; the edge rolls nothing (restart the story to load the generated Equipment XML)",
                );
            }
            return;
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: quality: canary lookup failed: {e}"),
            );
            return;
        }
    }
    let odds = if military { &MILITARY_ODDS } else { &RAIDER_ODDS };
    if let Err(e) = roll_band(band_h, odds, now) {
        mono::log(
            LogLevel::Warn,
            &format!("survivalist-mod: quality: edge roll failed: {e}"),
        );
    }
}

/// Pick a tier index from cumulative per-mille odds, or None for
/// the common item the game already spawned.
fn roll_tier(odds: &[u64; 4], now: f32, salt: u64) -> Option<usize> {
    let r = rng(now, salt, 1000);
    let mut cum = 0u64;
    for (i, &o) in odds.iter().enumerate() {
        cum += o;
        if r < cum {
            return Some(i);
        }
    }
    None
}

fn roll_band(band_h: i32, odds: &[u64; 4], now: f32) -> Result<(), String> {
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
        // Walk top-down: a swap mutates the container.
        for i in (0..n).rev() {
            let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([i]))?) else {
                continue;
            };
            let item = own(item_h);
            let Some(base_name) = handle_of(&item.invoke("GetPrototype", &json!([]))?)
                .and_then(|ph| {
                    with(ph, |p| {
                        let name = p
                            .read_field("Name")
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string));
                        drop(own(ph));
                        name
                    })
                })
            else {
                continue;
            };
            // Already tiered (or any modded underscore name): never
            // re-roll.
            if base_name.contains('_') {
                continue;
            }
            let salt = (mi as u64) * 131 + i as u64 + 7;
            let Some(tier_ix) = roll_tier(odds, now, salt) else {
                continue;
            };
            let sibling = rng(now, salt.wrapping_mul(97), SIBLINGS) + 1;
            let candidate = format!("{base_name}_{}{sibling}", TIER_NAMES[tier_ix]);
            // Only weapons and armor have variants; anything else
            // misses the lookup and stays as spawned.
            let Ok(Some(proto_h)) = find_prototype(&candidate) else {
                continue;
            };
            // The swap, net zero items: the common piece leaves
            // the world, the tiered one lands in the same hand.
            let taken = inv.invoke(
                "Take",
                &json!([{ "handle": mh }, { "handle": item_h }, 1]),
            )?;
            let Some(taken_h) = handle_of(&taken) else {
                drop(own(proto_h));
                continue;
            };
            if let Err(e) = with(taken_h, |t| t.invoke("Delete", &json!([]))) {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: quality: delete of the common piece failed: {e}"),
                );
            }
            drop(own(taken_h));
            let fine = mono::invoke_static(
                "Equipment",
                "Spawn",
                &json!([{ "handle": proto_h }, 1]),
            );
            drop(own(proto_h));
            let fine_h = match fine {
                Ok(v) => match handle_of(&v) {
                    Some(h) => h,
                    None => continue,
                },
                Err(e) => {
                    mono::log(
                        LogLevel::Warn,
                        &format!("survivalist-mod: quality: variant spawn failed: {e}"),
                    );
                    continue;
                }
            };
            let _ = member.invoke("Add", &json!([{ "handle": mh }, { "handle": fine_h }]));
            drop(own(fine_h));
            let who = member
                .invoke("GetDisplayNameString", &json!([]))
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "<unnamed>".into());
            SWAPS[tier_ix].fetch_add(1, Ordering::Relaxed);
            *LAST_SWAP.lock() =
                Some(format!("{} {base_name} in {who}'s hands", TIER_NAMES[tier_ix]));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: quality: a {} {base_name} crossed the edge in {who}'s hands",
                    TIER_NAMES[tier_ix],
                ),
            );
        }
    }
    Ok(())
}

/// Walk GameImpl.Instance.CurrentStories and ask each loaded
/// story for the prototype (Story.FindEquipmentPrototypeByName);
/// the one that loaded our XML answers. Shared with unique.rs.
pub(crate) fn find_prototype(name: &str) -> Result<Option<i32>, String> {
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
/// hash of the fire time and a salt). Shared with unique.rs.
pub(crate) fn rng(now: f32, salt: u64, n: u64) -> u64 {
    let mut h = (now.to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h % n.max(1)
}

// ---- ops ---------------------------------------------------------------------

pub fn register_ops() {
    OP_REGISTRY.register(OpDef::new(
        "quality_status",
        "The quality system's live state: is the variant data loaded, edge swaps per tier, and the last swap.",
        "{}",
        quality_status,
    ));
}

fn quality_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let loaded = match find_prototype(CANARY) {
            Ok(Some(h)) => {
                drop(own(h));
                true
            }
            _ => false,
        };
        let mut swaps = serde_json::Map::new();
        for (i, name) in TIER_NAMES.iter().enumerate() {
            swaps.insert(name.to_lowercase(), json!(SWAPS[i].load(Ordering::Relaxed)));
        }
        Ok(json!({
            "variants_loaded": loaded,
            "swaps": swaps,
            "last_swap": LAST_SWAP.lock().clone(),
        }))
    })
}
