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

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use unityforge::mono::{self, LogLevel};

use crate::common::{ctype, display_name, for_each_community, handle_of, own, with};
use crate::genome::{self, Trait};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Going,
    Strike,
    Returning,
}

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
    strike_deadline: f32,
    voter_ids: Vec<i64>,
    deadline: f32,
}

static MISSION: Mutex<Option<Mission>> = Mutex::new(None);
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);

/// The active murder a faction is running, for survival_status.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSION.lock().as_ref().filter(|m| m.camp_id == faction_id).map(|m| {
        json!({
            "victim": m.victim_name,
            "of": m.victim_camp_name,
            "operative": m.operative_name,
            "stage": match m.stage {
                Stage::Going => "going",
                Stage::Strike => "striking",
                Stage::Returning => "returning",
            },
        })
    })
}

pub fn tick(now: f32) {
    let last_tick = f32::from_bits(LAST_TICK_BITS.load(Ordering::Relaxed));
    if now - last_tick >= MISSION_TICK_SECS {
        LAST_TICK_BITS.store(now.to_bits(), Ordering::Relaxed);
        advance_mission(now);
    }
    let last_scan = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last_scan >= MURDER_SCAN_PERIOD_SECS {
        LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
        if MISSION.lock().is_some() {
            return;
        }
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(LogLevel::Warn, &format!("survivalist-mod: murder scan failed: {e}"));
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
        let mut votes = 0i64;
        let mut franchise = 0i64;
        let mut sum = 0.0f64;
        let mut voter_ids = Vec::new();
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
                let g = genome::individual(char_id, &t).get(Trait::Guile);
                franchise += 1;
                sum += g;
                if g >= MURDER_GUILE_FLOOR {
                    votes += 1;
                }
                voter_ids.push(char_id);
            }
        }
        if franchise > 0 && votes * 2 > franchise {
            let eff = sum / franchise as f64;
            if plotter.as_ref().map(|p| eff > p.6).unwrap_or(true) {
                if let Some(old) = plotter.replace((
                    com.handle().0,
                    id,
                    display_name(&com),
                    t,
                    members,
                    votes,
                    eff,
                    voter_ids,
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
    let leader_id = handle_of(&com.read_field("Leader")?)
        .map(|h| own(h).read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1));
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
            let id = member.read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1);
            if !alive || !human || !conscious || squadded || Some(id) == leader_id {
                continue;
            }
            let g = genome::individual(id, camp_ctype).get(Trait::Guile);
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
    let done = advance(m, now).unwrap_or(true);
    if done {
        let m = slot.take().unwrap();
        with(m.camp_h, |com| {
            if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
                if let Some(sq_h) = handle_of(&sq) {
                    let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
                }
            }
        });
        drop(own(m.camp_h));
        drop(own(m.victim_h));
        drop(own(m.operative_h));
    }
}

/// One mission step. Ok(true) = mission over, clean up.
fn advance(m: &mut Mission, now: f32) -> Result<bool, String> {
    let operative_alive =
        with(m.operative_h, |o| o.invoke("get_AliveAndNotZombie", &json!([])))? == json!(true);
    if !operative_alive {
        for &v in &m.voter_ids {
            genome::reinforce_individual(v, Trait::Guile, false, 2.0);
            genome::reinforce_individual(v, Trait::Aggression, false, 1.0);
        }
        genome::reinforce(m.camp_id, Trait::Guile, false, 2.0);
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: murder -- {}'s operative {} DIED going after {}; the knife loses its appeal",
                m.camp_name, m.operative_name, m.victim_name,
            ),
        );
        return Ok(true);
    }
    if now >= m.deadline {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: murder -- {}'s plot against {} fizzled (timeout); {} recalled",
                m.camp_name, m.victim_name, m.operative_name,
            ),
        );
        return Ok(true);
    }

    let victim_alive =
        with(m.victim_h, |v| v.invoke("get_AliveAndNotZombie", &json!([])))
            .map(|v| v == json!(true))
            .unwrap_or(false);

    match m.stage {
        Stage::Going => {
            if !victim_alive {
                // Someone else got them first; go home quietly.
                send_home(m)?;
                m.stage = Stage::Returning;
                return Ok(false);
            }
            let otile = with(m.operative_h, |o| o.invoke("get_Tile", &json!([])))?;
            let vtile = with(m.victim_h, |v| v.invoke("get_Tile", &json!([])))?;
            let d = tile_dist_sq(&otile, &vtile);
            if d > STRIKE_DIST_SQ {
                // The victim moves; keep the squad tracking them.
                with(m.camp_h, |com| -> Result<(), String> {
                    if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
                        if let Some(sq_h) = handle_of(&sq) {
                            let squad = own(sq_h);
                            squad.write_field("GoalTile", &vtile)?;
                            com.invoke(
                                "SetSquadAction",
                                &json!([{ "handle": sq_h }, "GoTo", 0, vtile.clone(), null, false]),
                            )?;
                        }
                    }
                    Ok(())
                })?;
                return Ok(false);
            }
            // Close enough: shed the squad so the kill goal owns
            // the operative, then issue the game's own
            // assassination command.
            with(m.camp_h, |com| {
                if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
                    if let Some(sq_h) = handle_of(&sq) {
                        let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
                    }
                }
            });
            with(m.operative_h, |o| {
                o.invoke(
                    "CommandChokeHold",
                    &json!([null, { "handle": m.victim_h }, false, "SlitThroat"]),
                )
            })?;
            m.stage = Stage::Strike;
            m.strike_deadline = now + STRIKE_WINDOW_SECS;
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: murder -- {} moves on {} with the knife",
                    m.operative_name, m.victim_name,
                ),
            );
            Ok(false)
        }
        Stage::Strike => {
            if !victim_alive {
                // The kill. The dark art paid; the franchise
                // learns it works.
                for &v in &m.voter_ids {
                    genome::reinforce_individual(v, Trait::Guile, true, 1.5);
                }
                genome::reinforce(m.camp_id, Trait::Guile, true, 1.5);
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: murder -- {} of {} is DEAD by {}'s hand; {} is leaderless",
                        m.victim_name, m.victim_camp_name, m.operative_name, m.victim_camp_name,
                    ),
                );
                send_home_squadless(m)?;
                m.stage = Stage::Returning;
                return Ok(false);
            }
            if now >= m.strike_deadline {
                // Blown: the mark lives and the camp knows the
                // cost of a bungled knife.
                for &v in &m.voter_ids {
                    genome::reinforce_individual(v, Trait::Guile, false, 1.0);
                }
                genome::reinforce(m.camp_id, Trait::Guile, false, 1.0);
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: murder -- {}'s attempt on {} is BLOWN; {} runs for home",
                        m.camp_name, m.victim_name, m.operative_name,
                    ),
                );
                send_home_squadless(m)?;
                m.stage = Stage::Returning;
                return Ok(false);
            }
            Ok(false)
        }
        Stage::Returning => {
            let otile = with(m.operative_h, |o| o.invoke("get_Tile", &json!([])))?;
            let home = json!({"x": m.home.0, "y": m.home.1});
            let d = tile_dist_sq(&otile, &home);
            Ok(d <= 64.0)
        }
    }
}

/// Retarget the existing squad home (used while it still exists).
fn send_home(m: &Mission) -> Result<(), String> {
    let home = json!({"x": m.home.0, "y": m.home.1});
    with(m.camp_h, |com| -> Result<(), String> {
        if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
            if let Some(sq_h) = handle_of(&sq) {
                let squad = own(sq_h);
                squad.write_field("GoalTile", &home)?;
                com.invoke(
                    "SetSquadAction",
                    &json!([{ "handle": sq_h }, "GoTo", 0, home.clone(), null, false]),
                )?;
            }
        }
        Ok(())
    })
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
