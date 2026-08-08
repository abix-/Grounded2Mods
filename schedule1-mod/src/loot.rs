//! Cash drops on player kills, riding the creation recipe
//! proven by tests/research_pickup.rs (docs/research.md 4b):
//! clone the game's inactive "Dynamic Amount Cash Pickup"
//! template, activate, position at the downed NPC, FishNet
//! ServerManager.Spawn (mandatory; un-spawned clones get
//! destroyed), set Value, UpdateCashStackVisuals.
//!
//! Amount scales with mob toughness (the downed NPC's
//! MaxHealth), per the operator. Exact numbers live here behind
//! the spoiler firewall (docs/schedule1-plan.md): the operator
//! reads the shape, not the rolls.
//!
//! Runs queued on the main thread, one frame after the kill, so
//! nothing heavy executes inside the Die/KnockOut prefix.

use serde_json::{Value as Json, json};

use unityforge::bridge::MonoHandle;
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

const TEMPLATE_NAME: &str = "Dynamic Amount Cash Pickup";

/// Queue a cash drop at the downed NPC's feet. `toughness` is
/// the NPC's max health; the payout rolls off it.
pub fn drop_cash_at(x: f64, y: f64, z: f64, toughness: f32) {
    // Spoiler firewall: payout roll.
    let base = (toughness.max(20.0) * 0.35) as f64;
    let amount = (base * (0.7 + fastrand::f64() * 0.8)).round().max(5.0);
    MAIN_QUEUE.push(move || {
        if let Err(e) = spawn_cash(x, y, z, amount) {
            mono::log(LogLevel::Warn, &format!("schedule1-mod: loot drop failed: {e}"));
        } else {
            mono::log(LogLevel::Info, &format!("schedule1-mod: loot dropped (${amount:.0})"));
        }
    });
}

fn handle_of(v: &Json) -> Option<i64> {
    v.get("handle").and_then(Json::as_i64)
}

/// SAFETY wrapper: own a handle so Drop releases it.
fn own(h: i64) -> MonoObject {
    // SAFETY: caller passes a handle just acquired from the shim
    // for this call path; ownership transfers here.
    unsafe { MonoObject::from_handle(MonoHandle(h as i32)) }
}

fn spawn_cash(x: f64, y: f64, z: f64, amount: f64) -> Result<(), String> {
    let ty = MonoType::find("Il2CppScheduleOne.ItemFramework.CashPickup")
        .ok_or("CashPickup type not found")?;

    // The inactive template, by its scene name.
    let walked = ty.walk(true)?;
    let list = walked.as_array().cloned().unwrap_or_default();
    let mut template = None;
    for i in &list {
        let name = i["name"].as_str().unwrap_or("");
        let Some(h) = i["handle"].as_i64() else { continue };
        if template.is_none() && name == TEMPLATE_NAME {
            template = Some(h);
        } else {
            drop(own(h));
        }
    }
    let template = template.ok_or("cash template not found in scene")?;

    let clone = mono::invoke_static(
        "UnityEngine.Object",
        "Instantiate",
        &json!([{"$handle": template}]),
    )?;
    drop(own(handle_of(&clone).ok_or("Instantiate returned no handle")? as i64));
    drop(own(template));

    // The clone comes back base-typed; re-find it as a CashPickup
    // via the walk (Unity names clones "<template>(Clone)"),
    // then rename it so the next drop is unambiguous.
    let walked = ty.walk(true)?;
    let list = walked.as_array().cloned().unwrap_or_default();
    let mut clone_h = None;
    for i in &list {
        let name = i["name"].as_str().unwrap_or("");
        let Some(h) = i["handle"].as_i64() else { continue };
        if clone_h.is_none() && name.ends_with("(Clone)") {
            clone_h = Some(h);
        } else {
            drop(own(h));
        }
    }
    let cash = own(clone_h.ok_or("clone not found after Instantiate")?);

    // Activate (the template is parked inactive) + position.
    let go = cash.invoke("get_gameObject", &json!([]))?;
    let go_h = handle_of(&go).ok_or("no gameObject handle")?;
    let go_obj = own(go_h);
    go_obj.invoke("SetActive", &json!([true]))?;
    let transform = cash.read_field("transform")?;
    if let Some(th) = handle_of(&transform) {
        let t = own(th);
        t.invoke("set_position", &json!([{"x": x, "y": y + 0.3, "z": z}]))?;
    }

    // FishNet spawn so the world owns it (proven mandatory).
    let sm = mono::invoke_static("Il2CppFishNet.InstanceFinder", "get_ServerManager", &json!([]))?;
    let sm_h = handle_of(&sm).ok_or("no ServerManager")?;
    let sm_obj = own(sm_h);
    sm_obj.invoke("Spawn", &json!([{"$handle": go_h}, null, {}]))?;

    cash.write_field("Value", &json!(amount))?;
    cash.invoke("UpdateCashStackVisuals", &json!([]))?;
    cash.invoke("set_name", &json!(["CashPickup_loot"]))?;
    Ok(())
}
