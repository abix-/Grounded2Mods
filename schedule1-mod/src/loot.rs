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

use std::sync::atomic::{AtomicI32, Ordering};

use serde_json::json;

use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, MonoType};

const TEMPLATE_NAME: &str = "Dynamic Amount Cash Pickup";

/// Cached handle for the inactive CashPickup template object.
static TEMPLATE_HANDLE: AtomicI32 = AtomicI32::new(0);
/// Cached handle for the FishNet ServerManager singleton.
static SERVER_MGR_HANDLE: AtomicI32 = AtomicI32::new(0);

/// Queue a cash drop at the downed NPC's feet. `toughness` is
/// the NPC's max health; the payout rolls off it.
/// Stays here because Schedule 1 defines the payout and cash pickup recipe; Unityforge owns main-thread dispatch and managed calls.
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

/// Finds and retains Schedule 1's inactive cash-pickup template for later drops.
/// Stays here because the concrete class and template name are game content; Unityforge owns type walking and managed handles.
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
        let Some(ih) = mono::json_handle(i) else {
            continue;
        };
        if found.is_none() && name == TEMPLATE_NAME {
            found = Some(ih);
        } else {
            drop(mono::owned_object(ih));
        }
    }
    let th = found.ok_or("cash template not found in scene")?;
    TEMPLATE_HANDLE.store(th, Ordering::Relaxed);
    Ok(th)
}

/// Finds and retains FishNet's live server manager so Schedule 1 cash clones can enter the networked world.
/// Stays here because this cache serves the game's cash-spawn recipe; Unityforge owns static invocation and handle wrappers.
fn cached_server_mgr() -> Result<i32, String> {
    let h = SERVER_MGR_HANDLE.load(Ordering::Relaxed);
    if h != 0 {
        return Ok(h);
    }
    let sm = mono::invoke_static(
        "Il2CppFishNet.InstanceFinder",
        "get_ServerManager",
        &json!([]),
    )?;
    let sh = mono::json_handle(&sm).ok_or("no ServerManager")?;
    SERVER_MGR_HANDLE.store(sh, Ordering::Relaxed);
    Ok(sh)
}

/// Clones, positions, network-spawns, prices, and refreshes one Schedule 1 cash pickup.
/// Stays here because every class, method, field, and ordering rule is game-specific; Unityforge owns generic managed operations.
fn spawn_cash(x: f64, y: f64, z: f64, amount: f64) -> Result<(), String> {
    let template_h = cached_template()?;

    let clone_v = mono::invoke_static(
        "UnityEngine.Object",
        "Instantiate",
        &json!([{"$handle": template_h}]),
    )?;
    drop(mono::owned_object(
        mono::json_handle(&clone_v).ok_or("Instantiate returned no handle")?,
    ));

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
        let Some(h) = mono::json_handle(i) else {
            continue;
        };
        if clone_h.is_none() && name.ends_with("(Clone)") {
            clone_h = Some(h);
        } else {
            drop(mono::owned_object(h));
        }
    }
    let cash = mono::owned_object(clone_h.ok_or("clone not found after Instantiate")?);

    // Activate (the template is parked inactive) + position.
    let go = cash.invoke("get_gameObject", &json!([]))?;
    let go_h = mono::json_handle(&go).ok_or("no gameObject handle")?;
    let go_obj = mono::owned_object(go_h);
    go_obj.invoke("SetActive", &json!([true]))?;
    let transform = cash.read_field("transform")?;
    if let Some(th) = mono::json_handle(&transform) {
        let t = mono::owned_object(th);
        t.invoke("set_position", &json!([{"x": x, "y": y + 0.3, "z": z}]))?;
    }

    // FishNet spawn so the world owns it (proven mandatory).
    // CashPickup inherits NetworkBehaviour which exposes
    // NetworkObject as a property (avoids the GetComponent
    // type-resolution issue in IL2CPP).
    let net_obj = cash.invoke("get_NetworkObject", &json!([]))?;
    let net_h = mono::json_handle(&net_obj).ok_or("no NetworkObject on cash clone")?;
    let sm_h = cached_server_mgr()?;
    mono::with_object(sm_h, |sm| {
        sm.invoke("Spawn", &json!([{"$handle": net_h}, null, {}]))
    })?;
    drop(mono::owned_object(net_h));

    cash.write_field("Value", &json!(amount))?;
    cash.invoke("UpdateCashStackVisuals", &json!([]))?;
    cash.invoke("set_name", &json!(["CashPickup_loot"]))?;
    Ok(())
}
