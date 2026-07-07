//! Scavenge: the low-risk acquisition act of the multidimensional
//! repertoire (docs/faction-war.md "Multidimensional factions").
//!
//! A camp whose franchise votes expansionist sends a party out to
//! loot an abandoned building and carry the goods home. Nothing is
//! conjured: the target is a real building-prop that no living
//! community owns (picking up ownerless goods is NOT theft; the
//! game's own IsStealingToPickUp trips only on a living community's
//! property), the goods ride home in real members' inventories via
//! the proven Take/Add transfer, and the base's own organizer stows
//! them once the party is home. There is no vanilla "scavenge
//! mission" to borrow, so the squad is driven by hand through the
//! same Going/Returning stages the trade caravan (trade.rs) uses.
//!
//! The party SIZE is emergent, never fixed. The carriers are the
//! most eager yes-voters (the people who wanted the raid), capped
//! by what the camp can spare (a fraction of its members, so
//! defense is not stripped) and by how much loot the target holds
//! (no more carriers than there are stacks to haul). Every term is
//! a live variable already in the choice.
//!
//! Learning: the voting franchise and the faction learn
//! EXPANSIONISM from the outcome (goods home raise it; a party lost
//! on the road or come home empty lower it).

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

use crate::common::{
    base_centre, ctype, display_name, for_each_community, handle_of, list_len, own, parse_xy, with,
};
use crate::genome::{self, Trait};

/// Seconds between scavenge scans.
const SCAV_SCAN_PERIOD_SECS: f32 = 200.0;

/// A voter favors scavenging if their expansionism clears this.
const SCAV_FLOOR: f64 = 0.55;

/// The camp must be able to spare a party.
const SCAV_MIN_MEMBERS: i64 = 4;

/// One carrier hauls this many stacks; the loot found decides how
/// many carriers are worth sending.
const STACKS_PER_CARRIER: i64 = 3;

/// The whole party takes at most this much in one trip.
const MAX_HAUL_STACKS: i64 = 9;

/// Keep the camp defensible: the party is capped at members divided
/// by this, so only a fraction ever leaves at once.
const DEFENSE_DIVISOR: i64 = 3;

/// Arrival threshold at the target prop (squared tile distance);
/// generous so a large building's centre still counts as reached.
const PROP_ARRIVE_SQ: f64 = 100.0;

/// Arrival threshold coming home (squared tile distance to the
/// camp's nearest building), the same measure the trade act uses.
const HOME_ARRIVE_SQ: f64 = 25.0;

/// A party that has not come home by then is recalled.
const MISSION_TIMEOUT_SECS: f32 = 900.0;

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Going,
    Returning,
}

struct Mission {
    scav_h: i32,
    scav_id: i64,
    scav_name: String,
    carriers: Vec<i32>,
    prop_h: i32,
    prop_tile: (f64, f64),
    prop_name: String,
    home: (i64, i64),
    squad_id: i64,
    stage: Stage,
    hauled: i64,
    voter_ids: Vec<i64>,
    deadline: f32,
}

static MISSIONS: Mutex<Vec<Mission>> = Mutex::new(Vec::new());
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

/// The active scavenge a faction is running, for survival_status.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSIONS.lock().iter().find(|m| m.scav_id == faction_id).map(|m| {
        json!({ "prop": m.prop_name, "carriers": m.carriers.len() })
    })
}

pub fn tick(now: f32) {
    advance_missions(now);
    let last = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last >= SCAV_SCAN_PERIOD_SECS {
        LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(LogLevel::Warn, &format!("survivalist-mod: scavenge scan failed: {e}"));
            }
        }
    }
}

// ---- launching ---------------------------------------------------------------

struct Winner {
    handle: i32,
    id: i64,
    name: String,
    ctype: String,
    members: i64,
    votes: i64,
    eff: f64,
    voter_ids: Vec<i64>,
    /// Free yes-voters as (char_id, expansionism), the carrier pool.
    candidates: Vec<(i64, f64)>,
}

fn launch_scan(now: f32) -> Result<(), String> {
    // The scavenger: an expansionist camp whose franchise votes it.
    // One party per camp (the active list), one launch per scan.
    let active: Vec<i64> = MISSIONS.lock().iter().map(|m| m.scav_id).collect();
    let mut winner: Option<Winner> = None;
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        let at_war = handle_of(&com.read_field("InvasionTarget")?).is_some();
        if members < SCAV_MIN_MEMBERS
            || at_war
            || list_len(&com, "Threats") > 0
            || active.contains(&id)
        {
            return Ok(true);
        }

        let looter = t == "Looter";
        let leader_id = com
            .read_field("Leader")
            .ok()
            .as_ref()
            .and_then(handle_of)
            .map(|lh| own(lh).read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1));

        let mut votes = 0i64;
        let mut franchise = 0i64;
        let mut sum = 0.0f64;
        let mut voter_ids = Vec::new();
        let mut candidates: Vec<(i64, f64)> = Vec::new();
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
            for i in 0..count {
                let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
                    continue;
                };
                let member = own(h);
                let alive = member
                    .invoke("get_AliveAndNotZombie", &json!([]))
                    .map(|v| v == json!(true))
                    .unwrap_or(false);
                let human = member
                    .invoke("GetBaseObjectType", &json!([]))
                    .map(|v| v == json!("Human"))
                    .unwrap_or(false);
                if !alive || !human {
                    continue;
                }
                let char_id = member.read_field("Id")?.as_i64().unwrap_or(-1);
                if looter && genome::is_conscript(char_id) {
                    continue;
                }
                let g = genome::individual(char_id, &t);
                let s = g.get(Trait::Expansionism);
                franchise += 1;
                sum += s;
                voter_ids.push(char_id);
                if s >= SCAV_FLOOR {
                    votes += 1;
                    // A sparable member (free, awake, not the leader)
                    // is a carrier candidate: the vote decides who
                    // goes, not just whether.
                    let conscious = member
                        .invoke("get_IsConscious", &json!([]))
                        .map(|v| v == json!(true))
                        .unwrap_or(false);
                    let squadded =
                        handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null))
                            .is_some();
                    if conscious && !squadded && Some(char_id) != leader_id {
                        candidates.push((char_id, s));
                    }
                }
            }
        }
        if franchise > 0 && votes * 2 > franchise && !candidates.is_empty() {
            let eff = sum / franchise as f64;
            if winner.as_ref().map(|w| eff > w.eff).unwrap_or(true) {
                if let Some(old) = winner.replace(Winner {
                    handle: com.handle().0,
                    id,
                    name: display_name(&com),
                    ctype: t.clone(),
                    members,
                    votes,
                    eff,
                    voter_ids,
                    candidates,
                }) {
                    drop(own(old.handle));
                }
                std::mem::forget(com);
            }
        }
        Ok(true)
    })?;

    let Some(mut w) = winner else {
        return Ok(());
    };

    let Some(home) = with(w.handle, base_centre) else {
        drop(own(w.handle));
        return Ok(());
    };

    // The target: the nearest abandoned building with loot in it.
    let Some((prop_h, prop_tile, prop_name, items)) = find_target(home)? else {
        drop(own(w.handle));
        return Ok(());
    };

    // Dynamic party: most eager yes-voters, capped by what the camp
    // can spare AND by how much loot is out there.
    w.candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let capacity_cap = (w.members / DEFENSE_DIVISOR).max(1) as usize;
    let haul_budget = items.min(MAX_HAUL_STACKS);
    let loot_cap = ((haul_budget + STACKS_PER_CARRIER - 1) / STACKS_PER_CARRIER).max(1) as usize;
    let k = w.candidates.len().min(capacity_cap).min(loot_cap).max(1);
    let party_ids: Vec<i64> = w.candidates.iter().take(k).map(|(id, _)| *id).collect();

    let carriers = with(w.handle, |com| gather_members(com, &party_ids))?;
    if carriers.is_empty() {
        drop(own(w.handle));
        drop(own(prop_h));
        return Ok(());
    }

    let squad_id = with(w.handle, |com| -> Result<i64, String> {
        let squad_h =
            handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?).ok_or("AddSquad gave no squad")?;
        for &c in &carriers {
            com.invoke(
                "AddToSquad",
                &json!([{ "handle": c }, { "handle": squad_h }]),
            )?;
        }
        let dest = json!({"x": prop_tile.0 as i64, "y": prop_tile.1 as i64});
        let squad = own(squad_h);
        squad.write_field("GoalTile", &dest)?;
        let sid = squad.read_field("Id")?.as_i64().unwrap_or(-1);
        drop(squad);
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, dest, null, false]),
        )?;
        Ok(sid)
    })?;

    let franchise = w.voter_ids.len();
    let party_size = carriers.len();
    MISSIONS.lock().push(Mission {
        scav_h: w.handle,
        scav_id: w.id,
        scav_name: w.name.clone(),
        carriers,
        prop_h,
        prop_tile,
        prop_name: prop_name.clone(),
        home,
        squad_id,
        stage: Stage::Going,
        hauled: 0,
        voter_ids: w.voter_ids,
        deadline: now + MISSION_TIMEOUT_SECS,
    });

    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: scavenge -- {} ({}, {} of {} voters expansionist, {:.2}) sends {} scavenger(s) to loot {} ({} stack(s) there)",
            w.name, w.ctype, w.votes, franchise, w.eff, party_size, prop_name, items,
        ),
    );
    Ok(())
}

/// The nearest ownerless building holding loot: (handle, centre
/// tile, name, stack count). Walks the game's own public props list
/// (PropManager.AllProps), the same list its scavenging brain uses.
fn find_target(home: (i64, i64)) -> Result<Option<(i32, (f64, f64), String, i64)>, String> {
    let pm = prop_manager()?;
    let list_h = handle_of(&pm.read_field("AllProps")?).ok_or("AllProps is null")?;
    let list = own(list_h);
    let count = list.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    let mut best: Option<(i32, (f64, f64), String, i64, f64)> = None;
    for i in 0..count {
        let Some(ph) = handle_of(&list.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        let prop = own(ph);
        // Owned by the living? Then it is theft, not scavenging.
        let living_owner = match handle_of(&prop.read_field("Community").unwrap_or(Json::Null)) {
            None => false,
            Some(oh) => own(oh)
                .invoke("HasAnyActiveMembers", &json!([]))
                .map(|v| v == json!(true))
                .unwrap_or(false),
        };
        if living_owner {
            continue;
        }
        // Holds real goods?
        let Some(inv_h) = handle_of(&prop.read_field("Inventory").unwrap_or(Json::Null)) else {
            continue;
        };
        let items = own(inv_h)
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if items <= 0 {
            continue;
        }
        let Some(ptile) = prop
            .invoke("GetCentreTile", &json!([]))
            .ok()
            .and_then(|v| parse_xy(&v))
        else {
            continue;
        };
        let ptile = (ptile.0 as f64, ptile.1 as f64);
        let d2 = (ptile.0 - home.0 as f64).powi(2) + (ptile.1 - home.1 as f64).powi(2);
        if best.as_ref().map(|b| d2 < b.4).unwrap_or(true) {
            let name = prop
                .invoke("GetDisplayNameString", &json!([]))
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "a ruin".into());
            if let Some((old_h, ..)) = best.replace((ph, ptile, name, items, d2)) {
                drop(own(old_h));
            }
            std::mem::forget(prop);
        }
    }
    Ok(best.map(|(h, tile, name, items, _)| (h, tile, name, items)))
}

fn prop_manager() -> Result<MonoObject, String> {
    let session = MonoType::find("Session")
        .and_then(|t| t.singleton_instance())
        .ok_or("Session.Instance not found (no game loaded?)")?;
    let pm_h =
        handle_of(&session.read_field("PropManager")?).ok_or("Session.PropManager is null")?;
    Ok(own(pm_h))
}

/// Gather live handles for the chosen carrier ids, re-checking each
/// is still alive and free (state can move between the vote and the
/// staffing). Forgets the kept handles so the mission owns them.
fn gather_members(com: &MonoObject, ids: &[i64]) -> Result<Vec<i32>, String> {
    let mut out = Vec::new();
    let Some(m_h) = handle_of(&com.read_field("Members")?) else {
        return Ok(out);
    };
    let mlist = own(m_h);
    let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    for i in 0..count {
        let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        let member = own(h);
        let mid = member.read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1);
        let alive = member
            .invoke("get_AliveAndNotZombie", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false);
        let squadded =
            handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null)).is_some();
        if ids.contains(&mid) && alive && !squadded {
            out.push(h);
            std::mem::forget(member);
        }
    }
    Ok(out)
}

// ---- advancing ---------------------------------------------------------------

fn advance_missions(now: f32) {
    let mut missions = MISSIONS.lock();
    let mut i = 0;
    while i < missions.len() {
        let done = match advance(&mut missions[i], now) {
            Ok(d) => d,
            Err(e) => {
                mono::log(
                    LogLevel::Warn,
                    &format!(
                        "survivalist-mod: scavenge -- mission for {} ABORTED on error: {e}",
                        missions[i].scav_name
                    ),
                );
                true
            }
        };
        if done {
            let m = missions.remove(i);
            with(m.scav_h, |com| {
                if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
                    if let Some(sq_h) = handle_of(&sq) {
                        let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
                    }
                }
            });
            drop(own(m.scav_h));
            drop(own(m.prop_h));
            for c in m.carriers {
                drop(own(c));
            }
        } else {
            i += 1;
        }
    }
}

/// One mission step. Ok(true) = mission over, clean up.
fn advance(m: &mut Mission, now: f32) -> Result<bool, String> {
    // The party's position is the first still-living carrier.
    let lead = m.carriers.iter().copied().find(|&c| {
        with(c, |ch| {
            ch.invoke("get_AliveAndNotZombie", &json!([]))
                .map(|v| v == json!(true))
                .unwrap_or(false)
        })
    });
    let Some(lead) = lead else {
        // The whole party died: reaching out cost blood.
        reinforce_all(m, false, 2.0);
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: scavenge -- {}'s party was lost on the road; the camp sours on reaching out",
                m.scav_name
            ),
        );
        return Ok(true);
    };
    if now >= m.deadline {
        reinforce_all(m, false, 0.5);
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: scavenge -- {}'s scavengers never came home (timeout)",
                m.scav_name
            ),
        );
        return Ok(true);
    }

    let tile = with(lead, |c| c.invoke("get_Tile", &json!([])))?;
    let (lx, ly) = parse_xy(&tile).ok_or("carrier tile unreadable")?;
    match m.stage {
        Stage::Going => {
            let d2 = (lx as f64 - m.prop_tile.0).powi(2) + (ly as f64 - m.prop_tile.1).powi(2);
            if d2 > PROP_ARRIVE_SQ {
                return Ok(false);
            }
            m.hauled = take_loot(m)?;
            let home = json!({"x": m.home.0, "y": m.home.1});
            with(m.scav_h, |com| -> Result<(), String> {
                if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
                    if let Some(sq_h) = handle_of(&sq) {
                        let squad = own(sq_h);
                        squad.write_field("GoalTile", &home)?;
                        com.invoke(
                            "SetSquadAction",
                            &json!([{ "handle": sq_h }, "GoTo", 0, home, null, false]),
                        )?;
                    }
                }
                Ok(())
            })?;
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: scavenge -- {}'s scavengers reached {} and took {} stack(s); heading home",
                    m.scav_name, m.prop_name, m.hauled,
                ),
            );
            m.stage = Stage::Returning;
            Ok(false)
        }
        Stage::Returning => {
            let d = with(m.scav_h, |com| {
                com.invoke("GetDistSqToNearestBuilding", &json!([tile]))
            })?
            .as_f64()
            .unwrap_or(f64::MAX);
            if d > HOME_ARRIVE_SQ {
                return Ok(false);
            }
            if m.hauled > 0 {
                reinforce_all(m, true, 1.0);
                crate::chronicle::post(&format!(
                    "{}'s scavengers came home from {} with a haul",
                    m.scav_name, m.prop_name
                ));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: scavenge -- {} comes home with {} stack(s) from {}; the camp trusts reaching out more",
                        m.scav_name, m.hauled, m.prop_name,
                    ),
                );
            } else {
                reinforce_all(m, false, 0.5);
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: scavenge -- {}'s scavengers came home empty from {}",
                        m.scav_name, m.prop_name
                    ),
                );
            }
            Ok(true)
        }
    }
}

fn reinforce_all(m: &Mission, up: bool, magnitude: f64) {
    for &v in &m.voter_ids {
        genome::reinforce_individual(v, Trait::Expansionism, up, magnitude);
    }
    genome::reinforce(m.scav_id, Trait::Expansionism, up, magnitude);
}

/// Move up to the haul budget from the prop's inventory into the
/// carriers' own inventories, each hauling up to STACKS_PER_CARRIER.
/// The base's organizer stows the carried goods once they are home.
fn take_loot(m: &Mission) -> Result<i64, String> {
    let Some(inv_h) = with(m.prop_h, |p| {
        handle_of(&p.read_field("Inventory").unwrap_or(Json::Null))
    }) else {
        return Ok(0);
    };
    let inv = own(inv_h);
    let mut hauled = 0i64;
    for &c in &m.carriers {
        if hauled >= MAX_HAUL_STACKS {
            break;
        }
        let carrier = own(c);
        let Some(cinv_h) = handle_of(&carrier.read_field("Inventory").unwrap_or(Json::Null)) else {
            continue;
        };
        let cinv = own(cinv_h);
        let mut mine = 0i64;
        while mine < STACKS_PER_CARRIER && hauled < MAX_HAUL_STACKS {
            let count = inv.invoke("get_Count", &json!([])).ok().and_then(|v| v.as_i64()).unwrap_or(0);
            if count <= 0 {
                break;
            }
            let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([0])).unwrap_or(Json::Null))
            else {
                break;
            };
            let item = own(item_h);
            let amount = item
                .invoke("GetAmount", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            std::mem::forget(item);
            let taken = inv.invoke(
                "Take",
                &json!([{ "handle": c }, { "handle": item_h }, amount]),
            )?;
            let Some(taken_h) = handle_of(&taken) else {
                break;
            };
            cinv.invoke("Add", &json!([{ "handle": c }, { "handle": taken_h }]))?;
            hauled += 1;
            mine += 1;
        }
    }
    Ok(hauled)
}
