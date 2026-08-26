//! Murder: the darkest act of the multidimensional repertoire
//! (docs/faction-war.md "Multidimensional factions").
//!
//! A camp AT WAR whose franchise votes for the knife sends its
//! most guileful free member to assassinate the enemy LEADER: a
//! decapitation strike instead of another raid. The walk out is
//! the same real 1-member squad every act uses; the kill itself
//! is the game's own assassination command
//! (`Character.CommandChokeHold` with HoldType.SlitThroat, the
//! exact entry the player UI issues), so the sneak, the grab, the
//! throat-slit, the victim's struggle, stealth skill, witnesses,
//! and secrecy are all vanilla. Murders inside a guarded camp are
//! RISKY: the operative is frequently seen and mobbed, which is
//! honest.
//!
//! Learning: a clean kill teaches guile up strongly; a dead
//! operative teaches guile and aggression down hard; a blown
//! attempt stings.

use std::sync::atomic::AtomicU32;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::genome::Ballot;
use modforge::mission::{self, Mission as _, Stage, Step};
use unityforge::mono::{self, LogLevel};

use crate::common::{
    ctype, display_name, for_each_community, handle_of, is_npc_alive, own, remove_squad_and_drop,
    send_squad_home, with,
};
use crate::genome;

/// Seconds between murder scans; the knife is rare.
const MURDER_SCAN_PERIOD_SECS: f32 = 240.0;

/// Seconds between mission-advance passes.
const MISSION_TICK_SECS: f32 = 5.0;

/// A voter favors the knife only if their own guile clears this:
/// the darkest act has the highest bar.
const MURDER_GUILE_FLOOR: f64 = 0.6;

/// Squared tile distance at which the operative is close enough
/// to receive the kill command.
const STRIKE_DIST_SQ: f64 = 144.0;

/// Real seconds the strike gets before it counts as blown.
const STRIKE_WINDOW_SECS: f32 = 180.0;

/// A mission that has not resolved by then is abandoned.
const MISSION_TIMEOUT_SECS: f32 = 1800.0;

/// One murder in flight map-wide keeps it dramatic. The mission
/// keeps its four handles alive until cleanup.
struct Mission {
    camp_h: i32,
    camp_id: i64,
    camp_name: String,
    victim_h: i32,
    victim_name: String,
    victim_camp_name: String,
    operative_h: i32,
    operative_name: String,
    squad_id: i64,
    home: (i64, i64),
    stage: Stage,
    strike_phase: bool,
    strike_deadline: f32,
    voter_ids: Vec<i64>,
    deadline: f32,
}

static MISSION: Mutex<Option<Mission>> = Mutex::new(None);
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);

/// The active murder a faction is running, for survival_status.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSION
        .lock()
        .as_ref()
        .filter(|m| m.camp_id == faction_id)
        .map(|m| {
            json!({
                "victim": m.victim_name,
                "of": m.victim_camp_name,
                "operative": m.operative_name,
                "stage": if m.strike_phase { "striking" } else {
                    match m.stage { Stage::Going => "going", Stage::Returning => "returning" }
                },
            })
        })
}

pub fn tick(now: f32) {
    if mission::should_tick(now, MISSION_TICK_SECS, &LAST_TICK_BITS) {
        advance_mission(now);
    }
    if mission::should_tick(now, MURDER_SCAN_PERIOD_SECS, &LAST_SCAN_BITS) {
        if MISSION.lock().is_some() {
            return;
        }
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: murder scan failed: {e}"),
                );
            }
        }
    }
}

// ---- launching ---------------------------------------------------------------

fn launch_scan(now: f32) -> Result<(), String> {
    // The camp with the darkest franchise among those AT WAR.
    let mut plotter: Option<(i32, i64, String, String, i64, i64, f64, Vec<i64>, i32)> = None;
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
        if members < 3 {
            return Ok(true);
        }
        // Only camps at war consider the knife, and only against
        // that enemy.
        let Some(enemy_h) = handle_of(&com.read_field("InvasionTarget")?) else {
            return Ok(true);
        };
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        let looter = t == "Looter";
        let mut ballot = Ballot::new(MURDER_GUILE_FLOOR);
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
        if ballot.has_majority() {
            let eff = ballot.mean_score();
            if plotter.as_ref().map(|p| eff > p.6).unwrap_or(true) {
                if let Some(old) = plotter.replace((
                    com.handle().0,
                    id,
                    display_name(&com),
                    t,
                    members,
                    ballot.votes_for,
                    eff,
                    ballot.voter_ids,
                    enemy_h,
                )) {
                    drop(own(old.0));
                    drop(own(old.8));
                }
                std::mem::forget(com);
                return Ok(true);
            }
        }
        drop(own(enemy_h));
        Ok(true)
    })?;
    let Some((camp_h, camp_id, camp_name, camp_ctype, _, votes, eff, voter_ids, enemy_h)) = plotter
    else {
        return Ok(());
    };

    // The mark: the enemy's leader (cut off the head).
    let enemy = own(enemy_h);
    let victim = handle_of(&enemy.read_field("Leader")?).and_then(|h| {
        let v = own(h);
        let alive = v
            .invoke("get_AliveAndNotZombie", &json!([]))
            .map(|x| x == json!(true))
            .unwrap_or(false);
        if alive {
            std::mem::forget(v);
            Some(h)
        } else {
            None
        }
    });
    let enemy_name = display_name(&enemy);
    drop(enemy);
    let Some(victim_h) = victim else {
        drop(own(camp_h));
        return Ok(());
    };
    let victim_name = with(victim_h, |v| {
        v.invoke("GetDisplayNameString", &json!([]))
            .ok()
            .and_then(|x| x.as_str().map(str::to_string))
            .unwrap_or_else(|| "<their leader>".into())
    });

    // The operative: the most guileful free member.
    let operative = with(camp_h, |com| pick_operative(com, &camp_ctype))?;
    let Some((operative_h, operative_name)) = operative else {
        drop(own(camp_h));
        drop(own(victim_h));
        return Ok(());
    };

    // Walk out as a real 1-member squad toward the victim.
    let home = with(camp_h, |com| crate::common::base_centre(com)).unwrap_or((0, 0));
    let vtile = with(victim_h, |v| v.invoke("get_Tile", &json!([])))?;
    let squad_id = with(camp_h, |com| -> Result<i64, String> {
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
        let squad = own(squad_h);
        com.invoke(
            "AddToSquad",
            &json!([{ "handle": operative_h }, { "handle": squad_h }]),
        )?;
        squad.write_field("GoalTile", &vtile)?;
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, vtile.clone(), null, false]),
        )?;
        squad.read_field("Id").map(|v| v.as_i64().unwrap_or(-1))
    })?;

    let franchise = voter_ids.len();
    *MISSION.lock() = Some(Mission {
        camp_h,
        camp_id,
        camp_name: camp_name.clone(),
        victim_h,
        victim_name: victim_name.clone(),
        victim_camp_name: enemy_name.clone(),
        operative_h,
        operative_name: operative_name.clone(),
        squad_id,
        home,
        stage: Stage::Going,
        strike_phase: false,
        strike_deadline: 0.0,
        voter_ids,
        deadline: now + MISSION_TIMEOUT_SECS,
    });

    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: murder -- {} ({}, {} of {} voters for the knife, {:.2}) sends {} to assassinate {}, leader of {}",
            camp_name, camp_ctype, votes, franchise, eff, operative_name, victim_name, enemy_name,
        ),
    );
    Ok(())
}

fn pick_operative(
    com: &unityforge::mono::MonoObject,
    camp_ctype: &str,
) -> Result<Option<(i32, String)>, String> {
    let leader_id = handle_of(&com.read_field("Leader")?).map(|h| {
        own(h)
            .read_field("Id")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(-1)
    });
    let mut best: Option<(i32, String, f64)> = None;
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
                handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null)).is_some();
            let id = member
                .read_field("Id")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            if !alive || !human || !conscious || squadded || Some(id) == leader_id {
                continue;
            }
            let g = genome::individual(id, camp_ctype)[genome::GUILE];
            if best.as_ref().map(|(_, _, bg)| g > *bg).unwrap_or(true) {
                let name = member
                    .invoke("GetDisplayNameString", &json!([]))
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "<unnamed>".into());
                if let Some((old_h, ..)) = best.replace((h, name, g)) {
                    drop(own(old_h));
                }
                std::mem::forget(member);
            }
        }
    }
    Ok(best.map(|(h, name, _)| (h, name)))
}

// ---- advancing ---------------------------------------------------------------

fn advance_mission(now: f32) {
    let mut slot = MISSION.lock();
    let Some(m) = slot.as_mut() else { return };
    let done = match mission::advance(m, now) {
        Ok(d) => d,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!(
                    "survivalist-mod: murder: {}'s plot against {} aborted: {e}",
                    m.camp_name, m.victim_name
                ),
            );
            true
        }
    };
    if done {
        slot.take().unwrap().cleanup();
    }
}

// ---- Mission trait ---------------------------------------------------------

impl mission::Mission for Mission {
    modforge::mission_accessors!();

    fn is_agent_alive(&self) -> Result<bool, String> {
        let alive = is_npc_alive(self.operative_h)?;
        if !alive {
            for &v in &self.voter_ids {
                genome::reinforce_individual(v, genome::GUILE, false, 2.0);
                genome::reinforce_individual(v, genome::AGGRESSION, false, 1.0);
            }
            genome::reinforce(self.camp_id, genome::GUILE, false, 2.0);
            crate::chronicle::post(&format!(
                "an assassin from {} was cut down in {}",
                self.camp_name, self.victim_camp_name
            ));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: murder: {}'s operative {} died going after {}; the knife loses its appeal",
                    self.camp_name, self.operative_name, self.victim_name,
                ),
            );
        }
        Ok(alive)
    }

    fn on_going(&mut self, now: f32) -> Result<Step, String> {
        let victim_alive = is_npc_alive(self.victim_h).unwrap_or(false);

        if self.strike_phase {
            if !victim_alive {
                for &v in &self.voter_ids {
                    genome::reinforce_individual(v, genome::GUILE, true, 1.5);
                }
                genome::reinforce(self.camp_id, genome::GUILE, true, 1.5);
                crate::chronicle::post(&format!(
                    "{}, leader of {}, has been assassinated",
                    self.victim_name, self.victim_camp_name
                ));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: murder: {} of {} is dead by {}'s hand; {} is leaderless",
                        self.victim_name,
                        self.victim_camp_name,
                        self.operative_name,
                        self.victim_camp_name,
                    ),
                );
                send_home_squadless(self)?;
                return Ok(Step::Transition);
            }
            if now >= self.strike_deadline {
                for &v in &self.voter_ids {
                    genome::reinforce_individual(v, genome::GUILE, false, 1.0);
                }
                genome::reinforce(self.camp_id, genome::GUILE, false, 1.0);
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: murder: {}'s attempt on {} is blown; {} runs for home",
                        self.camp_name, self.victim_name, self.operative_name,
                    ),
                );
                send_home_squadless(self)?;
                return Ok(Step::Transition);
            }
            return Ok(Step::Continue);
        }

        if !victim_alive {
            send_squad_home(self.camp_h, self.squad_id, self.home)?;
            return Ok(Step::Transition);
        }
        let otile = with(self.operative_h, |o| o.invoke("get_Tile", &json!([])))?;
        let vtile = with(self.victim_h, |v| v.invoke("get_Tile", &json!([])))?;
        let d = tile_dist_sq(&otile, &vtile);
        if d > STRIKE_DIST_SQ {
            let vx = vtile.get("x").and_then(Json::as_i64).unwrap_or(0);
            let vy = vtile.get("y").and_then(Json::as_i64).unwrap_or(0);
            send_squad_home(self.camp_h, self.squad_id, (vx, vy))?;
            return Ok(Step::Continue);
        }
        with(self.camp_h, |com| {
            if let Ok(sq) = com.invoke("GetSquad", &json!([self.squad_id])) {
                if let Some(sq_h) = handle_of(&sq) {
                    let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
                }
            }
        });
        with(self.operative_h, |o| {
            o.invoke(
                "CommandChokeHold",
                &json!([null, { "handle": self.victim_h }, false, "SlitThroat"]),
            )
        })?;
        self.strike_phase = true;
        self.strike_deadline = now + STRIKE_WINDOW_SECS;
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: murder: {} moves on {} with the knife",
                self.operative_name, self.victim_name,
            ),
        );
        Ok(Step::Continue)
    }

    fn on_returning(&mut self, _now: f32) -> Result<Step, String> {
        let otile = with(self.operative_h, |o| o.invoke("get_Tile", &json!([])))?;
        let home = json!({"x": self.home.0, "y": self.home.1});
        let d = tile_dist_sq(&otile, &home);
        if d > 64.0 {
            return Ok(Step::Continue);
        }
        Ok(Step::Complete)
    }

    fn cleanup(self) {
        remove_squad_and_drop(
            self.camp_h,
            self.squad_id,
            &[self.camp_h, self.victim_h, self.operative_h],
        );
    }

    fn label(&self) -> String {
        format!("{} assassinating {}", self.camp_name, self.victim_name)
    }
}

/// After the strike the squad is gone; walk the operative home in
/// a fresh one.
fn send_home_squadless(m: &mut Mission) -> Result<(), String> {
    let home = json!({"x": m.home.0, "y": m.home.1});
    let squad_id = with(m.camp_h, |com| -> Result<i64, String> {
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
        let squad = own(squad_h);
        com.invoke(
            "AddToSquad",
            &json!([{ "handle": m.operative_h }, { "handle": squad_h }]),
        )?;
        squad.write_field("GoalTile", &home)?;
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, home.clone(), null, false]),
        )?;
        squad.read_field("Id").map(|v| v.as_i64().unwrap_or(-1))
    })?;
    m.squad_id = squad_id;
    Ok(())
}

fn tile_dist_sq(a: &Json, b: &Json) -> f64 {
    let g = |v: &Json, k: &str| v.get(k).and_then(Json::as_f64).unwrap_or(f64::MAX / 4.0);
    let dx = g(a, "x") - g(b, "x");
    let dy = g(a, "y") - g(b, "y");
    dx * dx + dy * dy
}
