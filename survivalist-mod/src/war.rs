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
use unityforge::bridge::MonoHandle;
use unityforge::hook::{self, HOOK_REGISTRY, HookCtx};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

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
    ]);
}

/// Wrap a handle we own; Drop releases it back to the shim table.
///
/// SAFETY: caller asserts the handle came fresh out of a bridge
/// response (read_field / invoke / ctx dispatcher) and is not
/// wrapped anywhere else.
fn own(h: i32) -> MonoObject {
    unsafe { MonoObject::from_handle(MonoHandle(h)) }
}

fn handle_of(v: &Json) -> Option<i32> {
    v.get("handle").and_then(Json::as_i64).map(|h| h as i32)
}

// ---- the generalized revenge trigger --------------------------------------

extern "C" fn on_member_died(ctx: *const c_void) -> i32 {
    let h = ctx as isize as i32;
    if h != 0 {
        let member = own(h);
        if let Err(e) = try_ai_revenge(&member) {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: war revenge check failed: {e}"),
            );
        }
    }
    0 // always run the original OnMemberDied
}

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
        &format!("survivalist-mod: war -- {vname} sets a revenge invasion on {kname} (member killed)"),
    );
    Ok(())
}

fn display_name(com: &MonoObject) -> String {
    com.invoke("GetDisplayNameString", &json!([]))
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "<unnamed>".to_string())
}

// ---- ops -------------------------------------------------------------------

/// Run `f` on the Unity main thread and wait for its result
/// (same oneshot shape as unityforge's write_field op).
fn on_main_thread<F>(f: F) -> Result<Json, String>
where
    F: FnOnce() -> Result<Json, String> + Send + 'static,
{
    use std::sync::Arc;

    use parking_lot::Mutex;
    let result: Arc<Mutex<Option<Result<Json, String>>>> = Arc::new(Mutex::new(None));
    let r2 = result.clone();
    MAIN_QUEUE.push(move || {
        *r2.lock() = Some(f());
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(r) = result.lock().take() {
            return r;
        }
        if std::time::Instant::now() >= deadline {
            return Err("war op: main-thread queue timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn community_manager() -> Result<MonoObject, String> {
    let session = MonoType::find("Session")
        .and_then(|t| t.singleton_instance())
        .ok_or("Session.Instance not found (no game loaded?)")?;
    let cm_h = handle_of(&session.read_field("CommunityManager")?)
        .ok_or("Session.CommunityManager is null")?;
    Ok(own(cm_h))
}

/// Visit every community. `f` takes OWNERSHIP of each wrapper:
/// dropping it releases the handle; `std::mem::forget` keeps the
/// handle alive for use after the loop. Returns true to keep
/// iterating.
fn for_each_community(mut f: impl FnMut(MonoObject) -> Result<bool, String>) -> Result<(), String> {
    let cm = community_manager()?;
    let list_h = handle_of(&cm.read_field("Communities")?).ok_or("Communities list is null")?;
    let list = own(list_h);
    let count = list
        .invoke("get_Count", &json!([]))?
        .as_i64()
        .ok_or("get_Count did not return a number")?;
    for i in 0..count {
        let Some(item_h) = handle_of(&list.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        if !f(own(item_h))? {
            break;
        }
    }
    Ok(())
}

fn war_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let mut out = Vec::new();
        for_each_community(|com| {
            let com = &com;
            let name = display_name(com);
            let ctype = com
                .read_field("CommunityType")
                .map(|v| v.as_str().unwrap_or("?").to_string())
                .unwrap_or_else(|_| "?".to_string());
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
                let n = sq_list.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
                for i in 0..n {
                    if let Some(s_h) = handle_of(&sq_list.invoke("get_Item", &json!([i]))?) {
                        let squad = own(s_h);
                        let behaviour = squad
                            .read_field("Behaviour")
                            .map(|v| v.as_str().unwrap_or("?").to_string())
                            .unwrap_or_else(|_| "?".to_string());
                        let n_members = match handle_of(&squad.read_field("Members")?) {
                            Some(m_h) => own(m_h)
                                .invoke("get_Count", &json!([]))?
                                .as_i64()
                                .unwrap_or(0),
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

    on_main_thread(move || {
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
