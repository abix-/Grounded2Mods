//! Cash drops on player kills, riding the creation recipe
//! proven by tests/research_pickup.rs (docs/research.md 4b):
//! clone the game's inactive "Dynamic Amount Cash Pickup"
//! template, activate, position at the downed NPC, FishNet
//! ServerManager.Spawn (mandatory; un-spawned clones get
//! destroyed), set Value, UpdateCashStackVisuals.
//!
//! Amount scales with mob toughness (the downed NPC's
//! MaxHealth), per the operator. Exact numbers live here behind
//! the spoiler firewall (docs/plan.md): the operator
//! reads the shape, not the rolls.
//!
//! Runs queued on the main thread, one frame after the kill, so
//! nothing heavy executes inside the Die/KnockOut prefix.

use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicI32, Ordering};

use serde_json::{Value as Json, json};

use unityforge::bridge::MonoHandle;
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

const TEMPLATE_NAME: &str = "Dynamic Amount Cash Pickup";

/// Cached handle for the inactive CashPickup template object.
static TEMPLATE_HANDLE: AtomicI32 = AtomicI32::new(0);
/// Cached handle for the FishNet ServerManager singleton.
static SERVER_MGR_HANDLE: AtomicI32 = AtomicI32::new(0);

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

pub(crate) fn handle_of(v: &Json) -> Option<i64> {
    v.get("handle").and_then(Json::as_i64)
}

/// Vector3 arrives as the shim's ToString "(x, y, z)".
pub(crate) fn parse_vec3(v: &Json) -> Option<(f64, f64, f64)> {
    let s = v.as_str().or_else(|| v.get("str").and_then(Json::as_str))?;
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let mut parts = s.split(',').map(|p| p.trim().parse::<f64>());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Ok(x)), Some(Ok(y)), Some(Ok(z))) => Some((x, y, z)),
        _ => None,
    }
}

/// SAFETY wrapper: own a handle so Drop releases it.
pub(crate) fn own(h: i64) -> MonoObject {
    // SAFETY: caller passes a handle just acquired from the shim
    // for this call path; ownership transfers here.
    unsafe { MonoObject::from_handle(MonoHandle(h as i32)) }
}

fn cached_template() -> Result<i32, String> {
    let h = TEMPLATE_HANDLE.load(Ordering::Relaxed);
    if h != 0 {
        return Ok(h);
    }
    let ty = MonoType::find("Il2CppScheduleOne.ItemFramework.CashPickup")
        .ok_or("CashPickup type not found")?;
    let walked = ty.walk(true)?;
    let list = walked.as_array().cloned().unwrap_or_default();
    let mut found = None;
    for i in &list {
        let name = i["name"].as_str().unwrap_or("");
        let Some(ih) = i["handle"].as_i64() else { continue };
        if found.is_none() && name == TEMPLATE_NAME {
            found = Some(ih as i32);
        } else {
            drop(own(ih));
        }
    }
    let th = found.ok_or("cash template not found in scene")?;
    TEMPLATE_HANDLE.store(th, Ordering::Relaxed);
    Ok(th)
}

fn cached_server_mgr() -> Result<ManuallyDrop<MonoObject>, String> {
    let h = SERVER_MGR_HANDLE.load(Ordering::Relaxed);
    if h != 0 {
        return Ok(ManuallyDrop::new(unsafe { MonoObject::from_handle(MonoHandle(h)) }));
    }
    let sm = mono::invoke_static("Il2CppFishNet.InstanceFinder", "get_ServerManager", &json!([]))?;
    let sh = handle_of(&sm).ok_or("no ServerManager")? as i32;
    SERVER_MGR_HANDLE.store(sh, Ordering::Relaxed);
    Ok(ManuallyDrop::new(unsafe { MonoObject::from_handle(MonoHandle(sh)) }))
}

fn spawn_cash(x: f64, y: f64, z: f64, amount: f64) -> Result<(), String> {
    let template_h = cached_template()?;

    let clone_v = mono::invoke_static(
        "UnityEngine.Object",
        "Instantiate",
        &json!([{"$handle": template_h}]),
    )?;
    drop(own(handle_of(&clone_v).ok_or("Instantiate returned no handle")?));

    // Re-find the clone as a CashPickup (the Instantiate return
    // is typed as UnityEngine.Object; the shim cannot resolve
    // CashPickup methods on it). Unity names clones
    // "<template>(Clone)"; rename it so the next drop finds a
    // clean slate.
    let ty = MonoType::find("Il2CppScheduleOne.ItemFramework.CashPickup")
        .ok_or("CashPickup type not found")?;
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
    // CashPickup inherits NetworkBehaviour which exposes
    // NetworkObject as a property (avoids the GetComponent
    // type-resolution issue in IL2CPP).
    let net_obj = cash.invoke("get_NetworkObject", &json!([]))?;
    let net_h = handle_of(&net_obj).ok_or("no NetworkObject on cash clone")?;
    let sm = cached_server_mgr()?;
    sm.invoke("Spawn", &json!([{"$handle": net_h}, null, {}]))?;
    drop(own(net_h));

    cash.write_field("Value", &json!(amount))?;
    cash.invoke("UpdateCashStackVisuals", &json!([]))?;
    cash.invoke("set_name", &json!(["CashPickup_loot"]))?;
    Ok(())
}
