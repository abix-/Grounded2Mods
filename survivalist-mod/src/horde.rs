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

use serde_json::json;

use modforge::storyteller::{
    PressureTarget, PressureTier, PressureTracker, pressure_ring_position, pressure_tier,
    strongest_pressure_target,
};
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
const TIERS: &[PressureTier<(i64, i64, &str)>] = &[
    PressureTier {
        at_least: 44,
        value: (10, 12, "White"),
    },
    PressureTier {
        at_least: 34,
        value: (8, 10, "Red"),
    },
    PressureTier {
        at_least: 24,
        value: (6, 8, "Blue"),
    },
    PressureTier {
        at_least: 16,
        value: (4, 6, "Green"),
    },
];

static PRESSURE: PressureTracker<i64, i32> = PressureTracker::new(MAX_PACKS);

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
    let mut targets = Vec::new();
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
        let Some(centre) = base_centre(&com) else {
            return Ok(true);
        };
        let name = display_name(&com);
        targets.push(PressureTarget {
            eligible,
            pressure: members,
            value: (com, name, centre),
        });
        Ok(true)
    })?;
    let Some(target) = strongest_pressure_target(targets, HORDE_MIN_MEMBERS) else {
        return Ok(None);
    };
    let (com, name, centre) = target.value;
    let members = target.pressure;
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
    PRESSURE.prune(
        |pack_h| with(pack_h, any_member_alive),
        |pack_h| drop(own(pack_h)),
    );
    if PRESSURE.is_full() {
        return Ok(Outcome::Passed);
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
    if PRESSURE.is_targeted(alpha.id) {
        return Ok(Outcome::Passed);
    }

    let (cx, cy) = alpha.centre;
    let members = alpha.members;
    let (min, max, strain) = pressure_tier(members, TIERS)
        .expect("TIERS covers every size at or above HORDE_MIN_MEMBERS")
        .value;

    // A ring point out from the camp; the angle comes from a hash
    // of the scan time and camp handle so successive packs arrive
    // from different directions.
    let (sx, sy) = pressure_ring_position(now, alpha_h as u64, alpha.centre, PACK_RING_TILES);

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

    PRESSURE.track(alpha.id, pack_h);

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
    PRESSURE.len()
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
