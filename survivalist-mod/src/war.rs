//! AI-vs-AI war ignition (faction-war phase: "AI factions fight
//! each other"; docs/faction-war.md scorecard pillar 1).
//!
//! Vanilla's revenge trigger (Community.OnMemberDied,
//! Community.cs:1425) only fires when the killer belongs to the
//! player community or a player ally, so organized invasions only
//! ever aim at the player. This module generalizes it: a prefix
//! on `Community.OnMemberDied` (arg0 = the dead member, a class)
//! reads the killer from the member, and when BOTH communities
//! are AI settlements at Hostile with each other, invokes the
//! game's own `SetInvasionTarget(killerCommunity, 7 days)` on the
//! victim community, mirroring the vanilla player path. The
//! vanilla path still handles player/ally killers; this only adds
//! the AI-vs-AI case, so it never doubles up.
//!
//! Ops:
//! - `war_status`: every community with type, live member count,
//!   invasion target, and squads. The observability surface for
//!   verifying wars live.
//! - `war_ignite {attacker, defender, days?}`: force a war between
//!   two named communities through the game's own SetRelationship
//!   + SetInvasionTarget. The live-verification probe (watch the
//!   attacker's squad form via war_status afterwards).

use std::ffi::c_void;

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::hook::{self, HOOK_REGISTRY, HookCtx};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{community_manager, ctype, display_name, for_each_community, handle_of, own};

/// Install the game hooks that activate this system.
/// Stays here because it applies Survivalist's faction war rules through the game's classes, fields, content, and actions.
pub fn install() {
    match hook::patch_prefix_ctx("Community", "OnMemberDied", HookCtx::Arg0, on_member_died) {
        Ok(h) => {
            HOOK_REGISTRY.register(h);
            mono::log(
                LogLevel::Info,
                "survivalist-mod: war -- revenge trigger generalized (Community.OnMemberDied prefix)",
            );
        }
        Err(e) => {
            mono::log(
                LogLevel::Error,
                &format!("survivalist-mod: war revenge patch FAILED: {e}"),
            );
        }
    }
}

/// Expose this system status and controls through the mod control endpoint.
/// Stays here because it applies Survivalist's faction war rules through the game's classes, fields, content, and actions.
pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "war_status",
            "Every community: type, live members, nemesis flag, invasion target, squads. The faction-war observability surface.",
            "{}",
            war_status,
        ),
        OpDef::new(
            "war_ignite",
            "Force a war between two named communities via the game's own SetRelationship + SetInvasionTarget. Live-verification probe for AI-vs-AI war.",
            "{attacker: str, defender: str, days?: number}",
            war_ignite,
        ),
        OpDef::new(
            "war_end",
            "End a war between two named communities via the game's own Ceasefire (first name is recorded as the side that sued). Operator relief valve; the invasion drops on its own once the pair is no longer hostile.",
            "{loser: str, winner: str}",
            war_end,
        ),
    ]);
}

// ---- the generalized revenge trigger --------------------------------------

/// Observe a death for revenge, bounty, and threat-contract consequences.
/// Stays here because it applies Survivalist's faction war rules through the game's classes, fields, content, and actions.
extern "C" fn on_member_died(ctx: *const c_void) -> i32 {
    let h = ctx as isize as i32;
    if h != 0 {
        let member = own(h);
        // Individual SELECTION: a dead survivor's genome leaves
        // the pool (and their vote with it). Their learned traits
        // spread only if they lived; a survivor of a disastrous
        // raid who then dies takes their caution to the grave.
        if let Some(id) = member.read_field("Id").ok().and_then(|v| v.as_i64()) {
            crate::genome::drop_individual(id);
        }
        // A dead bounty mark: claimed by the player or lapsed.
        crate::bounty::on_death(&member);
        // A dead threat member: counts toward clear-the-threat.
        crate::threat::on_death(&member);
        if let Err(e) = try_ai_revenge(&member) {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: war revenge check failed: {e}"),
            );
        }
    }
    0 // always run the original OnMemberDied
}

/// Let an AI camp retaliate when another faction kills one of its people.
/// Stays here because it applies Survivalist's faction war rules through the game's classes, fields, content, and actions.
fn try_ai_revenge(member: &MonoObject) -> Result<(), String> {
    let Some(killer_h) = handle_of(&member.read_field("Killer")?) else {
        return Ok(()); // no killer (disease, fall, script)
    };
    let killer = own(killer_h);
    let Some(vc_h) = handle_of(&member.read_field("Community")?) else {
        return Ok(());
    };
    let victim_com = own(vc_h);
    let Some(kc_h) = handle_of(&killer.read_field("Community")?) else {
        return Ok(());
    };
    let killer_com = own(kc_h);

    let vid = victim_com.read_field("Id")?.as_i64().unwrap_or(-1);
    let kid = killer_com.read_field("Id")?.as_i64().unwrap_or(-2);
    if vid == kid {
        return Ok(()); // infighting is not war
    }

    // Both sides must be real AI settlements: the vanilla trigger
    // already covers player/ally killers, and ambient groups
    // (zombies, wandering raiders) have no town to wage war from.
    if victim_com.invoke("IsAISettlement", &json!([]))? != json!(true) {
        return Ok(());
    }
    if killer_com.invoke("IsAISettlement", &json!([]))? != json!(true) {
        return Ok(());
    }

    // Only at war (mirrors the vanilla Hostile check).
    let rel = victim_com.invoke("GetRelationship", &json!([{ "handle": kc_h }]))?;
    if rel != json!("Hostile") {
        return Ok(());
    }

    // Don't churn an invasion that is already running.
    if handle_of(&victim_com.read_field("InvasionTarget")?).is_some() {
        return Ok(());
    }

    // Mirror the vanilla revenge shape: 7-day invasion window.
    victim_com.invoke(
        "SetInvasionTarget",
        &json!([{ "handle": kc_h }, 7.0, false]),
    )?;

    let vname = display_name(&victim_com);
    let kname = display_name(&killer_com);
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: war -- {vname} sets a revenge invasion on {kname} (member killed)"
        ),
    );
    Ok(())
}

// ---- ops -------------------------------------------------------------------

/// Report every living faction and its current enemies.
/// Stays here because it applies Survivalist's faction war rules through the game's classes, fields, content, and actions.
fn war_status(_args: &Json) -> Result<Json, String> {
    MAIN_QUEUE.run_result("war_status", std::time::Duration::from_secs(5), || {
        let mut out = Vec::new();
        for_each_community(|com| {
            let com = &com;
            let name = display_name(com);
            let ctype = ctype(com);
            let members = com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0);
            // Skip the noise: ambient one-off groups with nobody in them.
            if members == 0 && ctype != "Player" {
                return Ok(true);
            }
            let nemesis = com.read_field("Nemesis").unwrap_or(Json::Null);
            let invasion_target = match handle_of(&com.read_field("InvasionTarget")?) {
                Some(h) => Json::String(display_name(&own(h))),
                None => Json::Null,
            };
            let mut squads = Vec::new();
            if let Some(sq_h) = handle_of(&com.read_field("Squads")?) {
                let sq_list = own(sq_h);
                let n = sq_list.list_len_or_zero()?;
                for i in 0..n {
                    if let Some(s_h) = sq_list.list_handle(i)? {
                        let squad = own(s_h);
                        let behaviour = squad
                            .read_field("Behaviour")
                            .map(|v| v.as_str().unwrap_or("?").to_string())
                            .unwrap_or_else(|_| "?".to_string());
                        let n_members = match handle_of(&squad.read_field("Members")?) {
                            Some(m_h) => own(m_h).list_len_or_zero()?,
                            None => 0,
                        };
                        squads.push(json!({"behaviour": behaviour, "members": n_members}));
                    }
                }
            }
            out.push(json!({
                "name": name,
                "type": ctype,
                "members": members,
                "nemesis": nemesis,
                "invasion_target": invasion_target,
                "squads": squads,
            }));
            Ok(true)
        })?;
        Ok(json!({"communities": out}))
    })
}

/// Start a real faction war and invasion between two named camps.
/// Stays here because it applies Survivalist's faction war rules through the game's classes, fields, content, and actions.
fn war_ignite(args: &Json) -> Result<Json, String> {
    let attacker = args
        .get("attacker")
        .and_then(Json::as_str)
        .ok_or("missing arg 'attacker' (community display name)")?
        .to_string();
    let defender = args
        .get("defender")
        .and_then(Json::as_str)
        .ok_or("missing arg 'defender' (community display name)")?
        .to_string();
    let days = args.get("days").and_then(Json::as_f64).unwrap_or(7.0);

    MAIN_QUEUE.run_result("war_ignite", std::time::Duration::from_secs(5), move || {
        let mut attacker_h: Option<i32> = None;
        let mut defender_h: Option<i32> = None;
        for_each_community(|com| {
            let name = display_name(&com);
            let mut keep = false;
            if name.eq_ignore_ascii_case(&attacker) && attacker_h.is_none() {
                attacker_h = Some(com.handle().0);
                keep = true;
            }
            if name.eq_ignore_ascii_case(&defender) && defender_h.is_none() {
                defender_h = Some(com.handle().0);
                keep = true;
            }
            if keep {
                std::mem::forget(com); // hold the handle past the loop
            }
            Ok(!(attacker_h.is_some() && defender_h.is_some()))
        })?;
        let a = attacker_h.ok_or(format!("attacker community '{attacker}' not found"))?;
        let d = defender_h.ok_or(format!("defender community '{defender}' not found"))?;

        let cm = community_manager()?;
        cm.invoke(
            "SetRelationship",
            &json!([{ "handle": a }, { "handle": d }, "Hostile"]),
        )?;
        let att = own(a);
        att.invoke("SetInvasionTarget", &json!([{ "handle": d }, days, false]))?;
        let dname = display_name(&own(d));
        let aname = display_name(&att);
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: war_ignite -- {aname} vs {dname} ({days} days)"),
        );
        Ok(json!({"ignited": true, "attacker": aname, "defender": dname, "days": days}))
    })
}

/// End hostility between two named camps through the game relationship system.
/// Stays here because it applies Survivalist's faction war rules through the game's classes, fields, content, and actions.
fn war_end(args: &Json) -> Result<Json, String> {
    let loser = args
        .get("loser")
        .and_then(Json::as_str)
        .ok_or("war_end: needs `loser`")?
        .to_string();
    let winner = args
        .get("winner")
        .and_then(Json::as_str)
        .ok_or("war_end: needs `winner`")?
        .to_string();
    MAIN_QUEUE.run_result("war_end", std::time::Duration::from_secs(5), move || {
        let mut loser_h: Option<i32> = None;
        let mut winner_h: Option<i32> = None;
        for_each_community(|com| {
            let name = display_name(&com);
            if name == loser {
                loser_h = Some(com.handle().0);
                std::mem::forget(com);
            } else if name == winner {
                winner_h = Some(com.handle().0);
                std::mem::forget(com);
            }
            Ok(true)
        })?;
        let (Some(lh), Some(wh)) = (loser_h, winner_h) else {
            return Err(format!(
                "war_end: could not find both '{loser}' and '{winner}'"
            ));
        };
        let cm = community_manager()?;
        cm.invoke(
            "SetRelationship",
            &json!([{ "handle": lh }, { "handle": wh }, "Ceasefire", true]),
        )?;
        drop(crate::common::own(lh));
        drop(crate::common::own(wh));
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: war -- operator ceasefire: {loser} sues {winner} for peace"),
        );
        Ok(json!({"ceasefire": true, "loser": loser, "winner": winner}))
    })
}
