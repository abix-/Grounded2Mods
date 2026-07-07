//! The horde: the counterweight to the alpha settlement
//! (operator-locked, docs/faction-war.md "NO ALLIANCES; the horde
//! is the counterweight").
//!
//! Success buys attention from the dead. The horde is the
//! storyteller's first rule (src/storyteller.rs): whenever the
//! director fires it, the largest settlement above the pressure
//! threshold (AI or player alike) draws a zombie pack, MORE zombies
//! the bigger the camp and STRONGER strains at each size tier. The
//! pack is created by the game's own pack spawner
//! (`ZombieSpawnPoint.SpawnAmbientZombies`, the same call its spawn
//! points use), placed on a ring well outside the camp, and pointed
//! at the walls with the zombies' own walk order (`MoveToTile`):
//! they shamble in, sense the living, and the game's threat/defense
//! machinery does the rest.
//!
//! The strain tiers are the game's own infection types in enum
//! order: Green, Blue, Red, White. A camp that grows carries its
//! size; first place is a detriment.

use std::sync::OnceLock;

use parking_lot::Mutex;
use serde_json::json;

use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{base_centre, ctype, display_name, for_each_community, handle_of, own, with};
use crate::storyteller::{Outcome, Rule};

/// The horde as a storyteller rule; the director paces it.
pub const RULE: Rule = Rule {
    name: "horde",
    weight: 1,
    run,
};

/// Camps below this size draw no extra pressure (vanilla ambient
/// zombies only): growth is safe until it is conspicuous.
const HORDE_MIN_MEMBERS: i64 = 16;

/// At most this many of our packs roaming at once map-wide.
const MAX_PACKS: usize = 2;

/// The pack appears this many tiles out from the camp centre: off
/// the walls, a real shamble away.
const PACK_RING_TILES: f64 = 70.0;

/// Size tiers: at or above `members`, a pack of `min..=max`
/// zombies of `strain` (the game's own infection types, ascending
/// menace).
const TIERS: &[(i64, i64, i64, &str)] = &[
    (44, 10, 12, "White"),
    (34, 8, 10, "Red"),
    (24, 6, 8, "Blue"),
    (16, 4, 6, "Green"),
];

/// A pack we set loose: pruned once nobody in it is left standing.
/// `target_id` is the community Id it was sent at (stable across the
/// handle table, unlike a raw handle), so the survivable guard can
/// refuse a second pack at the same camp.
struct Pack {
    pack_h: i32,
    target_id: i64,
}

static PACKS: Mutex<Vec<Pack>> = Mutex::new(Vec::new());

/// Logged once per generation when the running shim predates the
/// static-invoke bridge (the horde arms after a game restart).
static SHIM_TOO_OLD_LOGGED: OnceLock<()> = OnceLock::new();

/// The largest eligible settlement, kept alive for one rule pass.
/// Dropping it releases the handle back to the shim table.
struct Alpha {
    com: MonoObject,
    id: i64,
    name: String,
    members: i64,
    centre: (i64, i64),
}

fn find_alpha() -> Result<Option<Alpha>, String> {
    let mut best: Option<(i32, String, i64, (i64, i64))> = None;
    for_each_community(|com| {
        let t = ctype(&com);
        let eligible = t == "Player"
            || ((t == "Normal" || t == "Looter")
                && com.invoke("IsAISettlement", &json!([]))? == json!(true));
        if !eligible {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members < HORDE_MIN_MEMBERS {
            return Ok(true);
        }
        let Some(centre) = base_centre(&com) else {
            return Ok(true);
        };
        if best.as_ref().map(|b| members > b.2).unwrap_or(true) {
            if let Some(old) = best.replace((com.handle().0, display_name(&com), members, centre)) {
                drop(own(old.0));
            }
            std::mem::forget(com);
        }
        Ok(true)
    })?;
    let Some((h, name, members, centre)) = best else {
        return Ok(None);
    };
    let com = own(h);
    let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
    Ok(Some(Alpha {
        com,
        id,
        name,
        members,
        centre,
    }))
}

fn run(now: f32) -> Result<Outcome, String> {
    // Prune packs that have been put down; cap concurrent packs.
    {
        let mut packs = PACKS.lock();
        packs.retain(|p| {
            let standing = with(p.pack_h, any_member_alive);
            if !standing {
                drop(own(p.pack_h));
            }
            standing
        });
        if packs.len() >= MAX_PACKS {
            return Ok(Outcome::Passed);
        }
    }

    let Some(alpha) = find_alpha()? else {
        return Ok(Outcome::Passed);
    };
    let alpha_h = alpha.com.handle().0;

    // The survivable line: never pile the dead onto a camp already
    // in a fight, and never send a second pack at the same camp.
    if !crate::storyteller::safe_to_pressure(alpha_h) {
        return Ok(Outcome::Passed);
    }
    if PACKS.lock().iter().any(|p| p.target_id == alpha.id) {
        return Ok(Outcome::Passed);
    }

    let (cx, cy) = alpha.centre;
    let members = alpha.members;
    let (_, min, max, strain) = *TIERS
        .iter()
        .find(|(at_least, ..)| members >= *at_least)
        .expect("TIERS covers every size at or above HORDE_MIN_MEMBERS");

    // A ring point out from the camp; the angle comes from a hash
    // of the scan time and camp handle so successive packs arrive
    // from different directions.
    let mut h = (now.to_bits() as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (alpha_h as u64) << 17;
    h ^= h >> 29;
    let angle = (h % 6283) as f64 / 1000.0;
    let sx = cx + (angle.cos() * PACK_RING_TILES) as i64;
    let sy = cy + (angle.sin() * PACK_RING_TILES) as i64;

    // The game's own pack spawner: a null spawn point makes them
    // roaming hunter zombies with no respawn ties.
    let pack = match mono::invoke_static(
        "ZombieSpawnPoint",
        "SpawnAmbientZombies",
        &json!([{"x": sx, "y": sy}, min, max, strain, null]),
    ) {
        Ok(p) => p,
        Err(e) if e.contains("pre-v6") => {
            SHIM_TOO_OLD_LOGGED.get_or_init(|| {
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: horde -- held back: the running shim predates static invoke; restart the game to arm the horde",
                );
            });
            return Ok(Outcome::Passed);
        }
        Err(e) => return Err(e),
    };
    let Some(pack_h) = handle_of(&pack) else {
        return Ok(Outcome::Passed);
    };

    // Point every zombie at the camp with their own walk order;
    // from there their senses and the camp's defenses take over.
    let dest = json!({"x": cx, "y": cy});
    let mut pointed = 0i64;
    with(pack_h, |com| -> Result<(), String> {
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
            for i in 0..count {
                let Some(zh) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
                    continue;
                };
                let zombie = own(zh);
                if zombie.invoke("MoveToTile", &json!(["Walk", dest.clone()])).is_ok() {
                    pointed += 1;
                }
            }
        }
        Ok(())
    })?;

    PACKS.lock().push(Pack {
        pack_h,
        target_id: alpha.id,
    });

    crate::chronicle::post(&format!("the dead are massing near {}", alpha.name));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: horde -- {pointed} {strain} zombie(s) converge on {} ({members} strong): first place is a detriment",
            alpha.name,
        ),
    );
    Ok(Outcome::Fired)
}

/// The current alpha's name and members, for the storyteller status
/// readout. None when no camp is over the pressure threshold.
pub fn alpha_view() -> Result<Option<(String, i64)>, String> {
    Ok(find_alpha()?.map(|a| (a.name.clone(), a.members)))
}

/// How many of our packs are currently tracked as roaming.
pub fn live_pack_count() -> usize {
    PACKS.lock().len()
}

/// Spawn a pack of `min..=max` zombies of `strain` at (sx, sy) and
/// point every one of them at `dest` with its own walk order.
/// Returns how many were pointed. The game's own spawner, reused so
/// the incursion loop's traveling mega-horde does not duplicate the
/// bridge plumbing. Errs with "pre-v6" when the running shim cannot
/// static-invoke (needs a game restart, same as the horde).
pub fn spawn_traveling_pack(
    sx: i64,
    sy: i64,
    dest: (i64, i64),
    min: i64,
    max: i64,
    strain: &str,
) -> Result<i64, String> {
    let pack = mono::invoke_static(
        "ZombieSpawnPoint",
        "SpawnAmbientZombies",
        &json!([{"x": sx, "y": sy}, min, max, strain, null]),
    )?;
    let Some(pack_h) = handle_of(&pack) else {
        return Ok(0);
    };
    let d = json!({"x": dest.0, "y": dest.1});
    let mut pointed = 0i64;
    with(pack_h, |com| -> Result<(), String> {
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
            for i in 0..count {
                let Some(zh) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
                    continue;
                };
                let zombie = own(zh);
                if zombie.invoke("MoveToTile", &json!(["Walk", d.clone()])).is_ok() {
                    pointed += 1;
                }
            }
        }
        Ok(())
    })?;
    Ok(pointed)
}

fn any_member_alive(com: &MonoObject) -> bool {
    let Some(m_h) = com.read_field("Members").ok().as_ref().and_then(handle_of) else {
        return false;
    };
    let mlist = own(m_h);
    let count = mlist
        .invoke("get_Count", &json!([]))
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    for i in 0..count {
        let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i])).unwrap_or(serde_json::Value::Null))
        else {
            continue;
        };
        // Zombies are dead already; "alive" for them is get_Alive.
        if own(h).invoke("get_Alive", &json!([])).ok() == Some(json!(true)) {
            return true;
        }
    }
    false
}
