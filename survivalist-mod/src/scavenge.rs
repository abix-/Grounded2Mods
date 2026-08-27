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

use std::sync::atomic::AtomicU32;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::genome::Ballot;
use modforge::mission::{self, Stage, Step};
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

use crate::common::{
    base_centre, ctype, display_name, dist_sq_to_building, for_each_community, handle_of,
    is_npc_alive, own, parse_xy, remove_squad_and_drop, send_squad_home, with,
};
use crate::genome;

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
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSIONS
        .lock()
        .iter()
        .find(|m| m.scav_id == faction_id)
        .map(|m| json!({ "prop": m.prop_name, "carriers": m.carriers.len() }))
}

/// Advance this system when its scheduled game update is due.
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
pub fn tick(now: f32) {
    mission::advance_all(&MISSIONS, now, |m, e| {
        mono::log(
            LogLevel::Warn,
            &format!(
                "survivalist-mod: scavenge: mission for {} aborted: {e}",
                m.scav_name
            ),
        );
    });
    if mission::should_tick(now, SCAV_SCAN_PERIOD_SECS, &LAST_SCAN_BITS) {
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: scavenge scan failed: {e}"),
                );
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

/// Find one faction ready to start scavenging missions.
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
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
            || com.field_list_len("Threats") > 0
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
            .map(|lh| {
                own(lh)
                    .read_field("Id")
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(-1)
            });

        let mut ballot = Ballot::new(SCAV_FLOOR);
        let mut candidates: Vec<(i64, f64)> = Vec::new();
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let count = mlist.list_len_or_zero()?;
            for i in 0..count {
                let Some(h) = mlist.list_handle(i)? else {
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
                let s = g[genome::EXPANSIONISM];
                if ballot.cast(char_id, s) {
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
        if ballot.has_majority() && !candidates.is_empty() {
            let eff = ballot.mean_score();
            if winner.as_ref().map(|w| eff > w.eff).unwrap_or(true) {
                if let Some(old) = winner.replace(Winner {
                    handle: com.handle().0,
                    id,
                    name: display_name(&com),
                    ctype: t.clone(),
                    members,
                    votes: ballot.votes_for,
                    eff,
                    voter_ids: ballot.voter_ids,
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
    w.candidates
        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
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
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
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
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
fn find_target(home: (i64, i64)) -> Result<Option<(i32, (f64, f64), String, i64)>, String> {
    let pm = prop_manager()?;
    let list_h = handle_of(&pm.read_field("AllProps")?).ok_or("AllProps is null")?;
    let list = own(list_h);
    // AllProps is large and VOLATILE: the game creates and destroys
    // props constantly, so the list can shrink under this walk and a
    // A now-stale list index throws. Read the count softly and
    // stop the walk on the first faulting index instead of letting it
    // abort (and log-spam) the whole scan.
    let count = list.list_len().unwrap_or(0);
    let mut best: Option<(i32, (f64, f64), String, i64, f64)> = None;
    for i in 0..count {
        let ph = match list.list_handle(i) {
            Ok(Some(handle)) => handle,
            Ok(None) => continue,
            Err(_) => break, // the live props list moved under us
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
        let items = own(inv_h).list_len().unwrap_or(0);
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

/// Open the game's complete prop list to find abandoned loot.
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
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
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
fn gather_members(com: &MonoObject, ids: &[i64]) -> Result<Vec<i32>, String> {
    let mut out = Vec::new();
    let Some(m_h) = handle_of(&com.read_field("Members")?) else {
        return Ok(out);
    };
    let mlist = own(m_h);
    let count = mlist.list_len_or_zero()?;
    for i in 0..count {
        let Some(h) = mlist.list_handle(i)? else {
            continue;
        };
        let member = own(h);
        let mid = member
            .read_field("Id")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
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

// ---- Mission trait ---------------------------------------------------------

impl mission::Mission for Mission {
    modforge::mission_accessors!();

    /// Check whether the mission agent can continue.
    /// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
    fn is_agent_alive(&self) -> Result<bool, String> {
        let any_alive = self
            .carriers
            .iter()
            .any(|&c| is_npc_alive(c).unwrap_or(false));
        if !any_alive {
            reinforce_all(self, false, 2.0);
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: scavenge: {}'s party was lost on the road; the camp sours on reaching out",
                    self.scav_name
                ),
            );
        }
        Ok(any_alive)
    }

    /// Resolve what happens when the mission reaches its destination.
    /// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
    fn on_going(&mut self, _now: f32) -> Result<Step, String> {
        let lead = self
            .carriers
            .iter()
            .copied()
            .find(|&c| is_npc_alive(c).unwrap_or(false))
            .ok_or_else(|| "no living carrier".to_string())?;
        let tile = with(lead, |c| c.invoke("get_Tile", &json!([])))?;
        let (lx, ly) = parse_xy(&tile).ok_or("carrier tile unreadable")?;
        let d2 = (lx as f64 - self.prop_tile.0).powi(2) + (ly as f64 - self.prop_tile.1).powi(2);
        if d2 > PROP_ARRIVE_SQ {
            return Ok(Step::Continue);
        }
        self.hauled = take_loot(self)?;
        send_squad_home(self.scav_h, self.squad_id, self.home)?;
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: scavenge: {}'s scavengers reached {} and took {} stack(s); heading home",
                self.scav_name, self.prop_name, self.hauled,
            ),
        );
        Ok(Step::Transition)
    }

    /// Resolve what happens when the mission agent returns home.
    /// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
    fn on_returning(&mut self, _now: f32) -> Result<Step, String> {
        let lead = self
            .carriers
            .iter()
            .copied()
            .find(|&c| is_npc_alive(c).unwrap_or(false))
            .ok_or_else(|| "no living carrier".to_string())?;
        if dist_sq_to_building(lead, self.scav_h)? > HOME_ARRIVE_SQ {
            return Ok(Step::Continue);
        }
        if self.hauled > 0 {
            reinforce_all(self, true, 1.0);
            crate::chronicle::post(&format!(
                "{}'s scavengers came home from {} with a haul",
                self.scav_name, self.prop_name
            ));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: scavenge: {} comes home with {} stack(s) from {}; the camp trusts reaching out more",
                    self.scav_name, self.hauled, self.prop_name,
                ),
            );
        } else {
            reinforce_all(self, false, 0.5);
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: scavenge: {}'s scavengers came home empty from {}",
                    self.scav_name, self.prop_name
                ),
            );
        }
        Ok(Step::Complete)
    }

    /// Release the mission squad and managed handles when the mission ends.
    /// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
    fn cleanup(self) {
        let mut handles = vec![self.scav_h, self.prop_h];
        handles.extend(&self.carriers);
        remove_squad_and_drop(self.scav_h, self.squad_id, &handles);
    }

    /// Describe the active work for status output.
    /// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
    fn label(&self) -> String {
        format!("{} scavenging {}", self.scav_name, self.prop_name)
    }
}

/// Teach the scavengers and their faction from the mission outcome.
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
fn reinforce_all(m: &Mission, up: bool, magnitude: f64) {
    genome::reinforce_collective(
        m.scav_id,
        &m.voter_ids,
        &[genome::EXPANSIONISM],
        up,
        magnitude,
    );
}

/// Move up to the haul budget from the prop's inventory into the
/// carriers' own inventories, each hauling up to STACKS_PER_CARRIER.
/// The base's organizer stows the carried goods once they are home.
/// Stays here because it applies Survivalist's scavenging missions rules through the game's classes, fields, content, and actions.
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
            let count = inv.list_len().unwrap_or(0);
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
