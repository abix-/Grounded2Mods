//! Steal: the guile act of the multidimensional repertoire
//! (docs/faction-war.md "Multidimensional factions").
//!
//! A camp that is poorer than a neighbor and whose franchise votes
//! guile sends ONE thief to that neighbor's stores. The thief
//! travels as a real 1-member squad (the game's own AddSquad /
//! AddToSquad / SetSquadAction machinery, the same path roving
//! traders use), takes up to a couple of stacks from the target's
//! storage buildings with the honest Take/Add transfer predation
//! proved, then walks home.
//!
//! Caught-or-clean is decided by the GAME, not the mod: after the
//! take, the thief calls the game's own
//! `Character.OnStoleSomething(target, ...)`, which runs the real
//! line-of-sight check (`IsCharacterVisibleToAnyMember`). Seen and
//! not allied: the witness shouts StopThief and the game itself
//! sets the pair Hostile. A theft gone wrong is therefore an
//! ORGANIC war ignition through a vanilla path.
//!
//! Learning: every franchise voter learns guile from the outcome
//! (clean haul carried home raises it; getting caught or losing
//! the thief lowers it), the same per-voter plasticity the raid
//! loop uses for aggression.

use std::sync::atomic::AtomicU32;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::genome::{Ballot, majority};
use modforge::mission::{self, Stage, Step};
use unityforge::mono::{self, LogLevel};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, ctype, display_name, dist_sq_to_building,
    for_each_community, handle_of, is_npc_alive, list_len, own, remove_squad_and_drop,
    send_squad_home, with,
};
use crate::genome;

/// Seconds between launch scans. Slower than the survival scan:
/// theft is occasional texture, not a drumbeat.
const STEAL_SCAN_PERIOD_SECS: f32 = 120.0;

/// Seconds between mission-advance passes (arrival checks).
const MISSION_TICK_SECS: f32 = 5.0;

/// A voter favors stealing if their own guile clears this.
const STEAL_GUILE_FLOOR: f64 = 0.5;

/// The target must be this much better fed than the thief's camp:
/// need drives them toward the richer neighbor.
const STEAL_ENVY_MARGIN: f64 = 0.15;

/// Stacks taken per theft. A burglary, not a raid.
const STEAL_MAX_STACKS: i64 = 2;

/// Within this squared tile distance of a target building the
/// thief is "in the stores"; same bar for being back home.
const ARRIVE_DIST_SQ: f64 = 25.0;

/// A mission that has not resolved by then is abandoned.
const MISSION_TIMEOUT_SECS: f32 = 1800.0;

/// At most this many thefts in flight map-wide.
const MAX_ACTIVE_MISSIONS: usize = 4;

/// An in-flight theft. The mission keeps its faction, target, and
/// thief handles alive (the launch scan's release pass skips them)
/// until cleanup releases all three.
struct Mission {
    faction_h: i32,
    faction_id: i64,
    faction_name: String,
    target_h: i32,
    target_name: String,
    thief_h: i32,
    thief_name: String,
    squad_id: i64,
    home: (i64, i64),
    stage: Stage,
    caught: bool,
    stolen: i64,
    voter_ids: Vec<i64>,
    deadline: f32,
}

static MISSIONS: Mutex<Vec<Mission>> = Mutex::new(Vec::new());
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);

/// The active theft a faction is running, for survival_status.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSIONS
        .lock()
        .iter()
        .find(|m| m.faction_id == faction_id)
        .map(|m| {
            json!({
                "target": m.target_name,
                "thief": m.thief_name,
                "stage": match m.stage { Stage::Going => "going", Stage::Returning => "returning" },
            })
        })
}

pub fn tick(now: f32) {
    if mission::should_tick(now, MISSION_TICK_SECS, &LAST_TICK_BITS) {
        mission::advance_all(&MISSIONS, now, |m, e| {
            mono::log(
                LogLevel::Warn,
                &format!(
                    "survivalist-mod: steal: mission for {} aborted: {e}",
                    m.faction_name
                ),
            );
        });
    }
    if mission::should_tick(now, STEAL_SCAN_PERIOD_SECS, &LAST_SCAN_BITS) {
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: steal scan failed: {e}"),
                );
            }
        }
    }
}

// ---- launching ---------------------------------------------------------------

struct Camp {
    handle: i32,
    id: i64,
    name: String,
    ctype: String,
    nutrition: f64,
    centre: (i64, i64),
    votes: i64,
    franchise: i64,
    effective_guile: f64,
    voter_ids: Vec<i64>,
    eligible_thief: bool,
}

fn launch_scan(now: f32) -> Result<(), String> {
    if MISSIONS.lock().len() >= MAX_ACTIVE_MISSIONS {
        return Ok(());
    }

    // Snapshot every AI settlement once (same discipline as the
    // survival scan: one pass, handles kept, released at the end).
    let mut camps: Vec<Camp> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members == 0 {
            return Ok(true);
        }
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        let nutrition = com
            .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
            .as_f64()
            .unwrap_or(1.0);
        let Some(centre) = base_centre(&com) else {
            return Ok(true);
        };
        // A camp can THIEVE only if it can spare a body and is not
        // otherwise occupied: at peace (no invasion), unthreatened,
        // and 3+ members. Any camp can still be a TARGET.
        let at_war = handle_of(&com.read_field("InvasionTarget")?).is_some();
        let threats = list_len(&com, "Threats");
        let can_thieve = members >= 3 && !at_war && threats == 0;

        let mut ballot = Ballot::new(STEAL_GUILE_FLOOR);
        if can_thieve {
            let looter = t == "Looter";
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
                    let g = genome::individual(char_id, &t)[genome::GUILE];
                    ballot.cast(char_id, g);
                }
            }
        }
        let effective_guile = ballot.mean_score();
        camps.push(Camp {
            handle: com.handle().0,
            id,
            name: display_name(&com),
            ctype: t,
            nutrition,
            centre,
            votes: ballot.votes_for,
            franchise: ballot.franchise,
            effective_guile,
            voter_ids: ballot.voter_ids,
            eligible_thief: can_thieve,
        });
        std::mem::forget(com);
        Ok(true)
    })?;

    // The thief camp: voted-yes camps not already thieving, by
    // most guileful franchise first.
    let active: Vec<i64> = MISSIONS.lock().iter().map(|m| m.faction_id).collect();
    let mut thieves: Vec<&Camp> = camps
        .iter()
        .filter(|c| c.eligible_thief && majority(c.votes, c.franchise) && !active.contains(&c.id))
        .collect();
    thieves.sort_by(|a, b| b.effective_guile.partial_cmp(&a.effective_guile).unwrap());

    for camp in thieves {
        // The mark: nearest meaningfully-richer neighbor this camp
        // is neither at war with nor allied to.
        let mut best: Option<(&Camp, i64)> = None;
        for t in &camps {
            if t.handle == camp.handle || t.nutrition < camp.nutrition + STEAL_ENVY_MARGIN {
                continue;
            }
            let rel = with(camp.handle, |c| {
                c.invoke("GetRelationship", &json!([{ "handle": t.handle }]))
            })
            .unwrap_or(json!("?"));
            if rel == json!("Hostile") || rel == json!("Allied") {
                continue;
            }
            let d = (t.centre.0 - camp.centre.0).pow(2) + (t.centre.1 - camp.centre.1).pow(2);
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((t, d));
            }
        }
        let Some((target, _)) = best else { continue };

        if let Err(e) = launch(camp, target, now) {
            mono::log(
                LogLevel::Warn,
                &format!(
                    "survivalist-mod: steal launch failed for {}: {e}",
                    camp.name
                ),
            );
        }
        break; // one new theft per scan
    }

    // Release the snapshot handles, EXCEPT the ones a live mission
    // (including one just launched) still owns.
    let kept: Vec<i32> = {
        let ms = MISSIONS.lock();
        ms.iter().flat_map(|m| [m.faction_h, m.target_h]).collect()
    };
    for c in &camps {
        if !kept.contains(&c.handle) {
            drop(own(c.handle));
        }
    }
    Ok(())
}

fn launch(camp: &Camp, target: &Camp, now: f32) -> Result<(), String> {
    with(camp.handle, |com| {
        // The thief: the highest-guile member that is conscious,
        // not the leader, and not already in a squad.
        let leader_id = handle_of(&com.read_field("Leader")?).map(|h| {
            own(h)
                .read_field("Id")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1)
        });
        let mut thief: Option<(i32, String, f64)> = None;
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
                let conscious = member
                    .invoke("get_IsConscious", &json!([]))
                    .map(|v| v == json!(true))
                    .unwrap_or(false);
                let squadded =
                    handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null))
                        .is_some();
                let id = member
                    .read_field("Id")
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(-1);
                if !alive || !human || !conscious || squadded || Some(id) == leader_id {
                    continue;
                }
                let g = genome::individual(id, &camp.ctype)[genome::GUILE];
                if thief.as_ref().map(|(_, _, bg)| g > *bg).unwrap_or(true) {
                    let name = member
                        .invoke("GetDisplayNameString", &json!([]))
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| "<unnamed>".into());
                    if let Some((old_h, ..)) = thief.replace((h, name, g)) {
                        drop(own(old_h));
                    }
                    std::mem::forget(member);
                }
            }
        }
        let Some((thief_h, thief_name, thief_guile)) = thief else {
            return Ok(()); // nobody free to send
        };

        // Send them as a real 1-member squad through the game's own
        // mission machinery (the roving-trader path): Trade
        // behaviour, so arrival parks them at the mark instead of
        // exiting the map.
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
        let squad = own(squad_h);
        com.invoke(
            "AddToSquad",
            &json!([{ "handle": thief_h }, { "handle": squad_h }]),
        )?;
        let dest = json!({"x": target.centre.0, "y": target.centre.1});
        squad.write_field("GoalTile", &dest)?;
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, dest, null, false]),
        )?;
        let squad_id = squad.read_field("Id")?.as_i64().unwrap_or(-1);
        drop(squad);

        MISSIONS.lock().push(Mission {
            faction_h: camp.handle,
            faction_id: camp.id,
            faction_name: camp.name.clone(),
            target_h: target.handle,
            target_name: target.name.clone(),
            thief_h,
            thief_name: thief_name.clone(),
            squad_id,
            home: camp.centre,
            stage: Stage::Going,
            caught: false,
            stolen: 0,
            voter_ids: camp.voter_ids.clone(),
            deadline: now + MISSION_TIMEOUT_SECS,
        });

        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: steal -- {} ({}, {} of {} voters guileful) sends {} (guile {:.2}) to steal from {}",
                camp.name,
                camp.ctype,
                camp.votes,
                camp.franchise,
                thief_name,
                thief_guile,
                target.name,
            ),
        );
        Ok(())
    })
}

// ---- advancing ---------------------------------------------------------------

// ---- Mission trait ---------------------------------------------------------

impl mission::Mission for Mission {
    modforge::mission_accessors!();

    fn is_agent_alive(&self) -> Result<bool, String> {
        let alive = is_npc_alive(self.thief_h)?;
        if !alive {
            for &v in &self.voter_ids {
                genome::reinforce_individual(v, genome::GUILE, false, 2.0);
            }
            genome::reinforce(self.faction_id, genome::GUILE, false, 2.0);
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: steal: {}'s thief {} died on the job against {}; the camp grows warier",
                    self.faction_name, self.thief_name, self.target_name,
                ),
            );
        }
        Ok(alive)
    }

    fn on_going(&mut self, _now: f32) -> Result<Step, String> {
        let target_alive = with(self.target_h, |t| {
            t.invoke("HasAnyLivingNonZombieMembers", &json!([]))
        })
        .map(|v| v == json!(true))
        .unwrap_or(false);
        if !target_alive {
            return Ok(Step::Complete);
        }
        if dist_sq_to_building(self.thief_h, self.target_h)? > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        self.stolen = with(self.target_h, |t| {
            carry_off_stored_goods(t, &[self.thief_h], STEAL_MAX_STACKS, GoodsFilter::Any, true)
        })?;
        let caught = with(self.thief_h, |t| {
            t.invoke(
                "OnStoleSomething",
                &json!([{ "handle": self.target_h }, null, 25.0 * self.stolen as f64, false]),
            )
        })? == json!(true);
        self.caught = caught;
        if caught {
            for &v in &self.voter_ids {
                genome::reinforce_individual(v, genome::GUILE, false, 1.5);
            }
            genome::reinforce(self.faction_id, genome::GUILE, false, 1.5);
            crate::chronicle::post(&format!(
                "a thief from {} was caught in {}'s stores",
                self.faction_name, self.target_name
            ));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: steal: {} was caught stealing from {} ({} stack(s) in hand); {} answers it the vanilla way",
                    self.thief_name, self.target_name, self.stolen, self.target_name,
                ),
            );
        } else {
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: steal: {} slips out of {}'s stores unseen with {} stack(s)",
                    self.thief_name, self.target_name, self.stolen,
                ),
            );
        }
        send_squad_home(self.faction_h, self.squad_id, self.home)?;
        Ok(Step::Transition)
    }

    fn on_returning(&mut self, _now: f32) -> Result<Step, String> {
        if dist_sq_to_building(self.thief_h, self.faction_h)? > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        if !self.caught && self.stolen > 0 {
            for &v in &self.voter_ids {
                genome::reinforce_individual(v, genome::GUILE, true, 1.0);
            }
            genome::reinforce(self.faction_id, genome::GUILE, true, 1.0);
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: steal: {} makes it home to {} with {}'s goods; the camp grows bolder in its guile",
                    self.thief_name, self.faction_name, self.target_name,
                ),
            );
        }
        Ok(Step::Complete)
    }

    fn cleanup(self) {
        remove_squad_and_drop(
            self.faction_h,
            self.squad_id,
            &[self.faction_h, self.target_h, self.thief_h],
        );
    }

    fn label(&self) -> String {
        format!("{} stealing from {}", self.faction_name, self.target_name)
    }
}
