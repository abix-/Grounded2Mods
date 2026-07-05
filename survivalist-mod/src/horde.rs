//! The horde: the counterweight to the alpha settlement
//! (operator-locked, docs/faction-war.md "NO ALLIANCES; the horde
//! is the counterweight").
//!
//! Success buys attention from the dead. Every scan, the largest
//! settlement above the pressure threshold (AI or player alike)
//! draws a zombie pack: MORE zombies the bigger the camp, and
//! STRONGER strains at each size tier. The pack is created by the
//! game's own pack spawner (`ZombieSpawnPoint.SpawnAmbientZombies`,
//! the same call its spawn points use), placed on a ring well
//! outside the camp, and pointed at the walls with the zombies'
//! own walk order (`MoveToTile`): they shamble in, sense the
//! living, and the game's threat/defense machinery does the rest.
//!
//! The strain tiers are the game's own infection types in enum
//! order: Green, Blue, Red, White. A camp that grows carries its
//! size; first place is a detriment.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde_json::json;

use unityforge::mono::{self, LogLevel};

use crate::common::{base_centre, ctype, display_name, for_each_community, handle_of, own, with};

/// Seconds between horde scans.
const HORDE_SCAN_PERIOD_SECS: f32 = 300.0;

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
struct Pack {
    community_h: i32,
}

static PACKS: Mutex<Vec<Pack>> = Mutex::new(Vec::new());
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

/// Logged once per generation when the running shim predates the
/// static-invoke bridge (the horde arms after a game restart).
static SHIM_TOO_OLD_LOGGED: OnceLock<()> = OnceLock::new();

pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last < HORDE_SCAN_PERIOD_SECS {
        return;
    }
    LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
    if let Err(e) = horde_scan(now) {
        if e.contains("pre-v6") {
            SHIM_TOO_OLD_LOGGED.get_or_init(|| {
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: horde -- held back: the running shim predates static invoke; restart the game to arm the horde",
                );
            });
        } else if !e.contains("not found") {
            mono::log(LogLevel::Warn, &format!("survivalist-mod: horde scan failed: {e}"));
        }
    }
}

fn horde_scan(now: f32) -> Result<(), String> {
    // Prune packs that have been put down.
    {
        let mut packs = PACKS.lock();
        packs.retain(|p| {
            let standing = with(p.community_h, any_member_alive);
            if !standing {
                drop(own(p.community_h));
            }
            standing
        });
        if packs.len() >= MAX_PACKS {
            return Ok(());
        }
    }

    // The alpha: the largest settlement above the threshold,
    // player camp included (first place is first place).
    let mut alpha: Option<(i32, String, i64, (i64, i64))> = None;
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
        if alpha.as_ref().map(|a| members > a.2).unwrap_or(true) {
            if let Some(old) = alpha.replace((com.handle().0, display_name(&com), members, centre)) {
                drop(own(old.0));
            }
            std::mem::forget(com);
        }
        Ok(true)
    })?;
    let Some((alpha_h, alpha_name, members, (cx, cy))) = alpha else {
        return Ok(());
    };

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
    let pack = mono::invoke_static(
        "ZombieSpawnPoint",
        "SpawnAmbientZombies",
        &json!([{"x": sx, "y": sy}, min, max, strain, null]),
    )?;
    let Some(pack_h) = handle_of(&pack) else {
        drop(own(alpha_h));
        return Ok(());
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

    PACKS.lock().push(Pack { community_h: pack_h });
    drop(own(alpha_h));

    crate::chronicle::post(&format!("the dead are massing near {alpha_name}"));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: horde -- {pointed} {strain} zombie(s) converge on {alpha_name} ({members} strong): first place is a detriment",
        ),
    );
    Ok(())
}

fn any_member_alive(com: &unityforge::mono::MonoObject) -> bool {
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
