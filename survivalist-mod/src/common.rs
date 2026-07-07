//! Shared bridge helpers for the survivalist modules (war,
//! growth, development). Extracted at the third consumer.

use serde_json::{Value as Json, json};
use unityforge::bridge::MonoHandle;
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

/// Wrap a handle we own; Drop releases it back to the shim table.
///
/// SAFETY: caller asserts the handle came fresh out of a bridge
/// response (read_field / invoke / ctx dispatcher) and is not
/// wrapped anywhere else.
pub fn own(h: i32) -> MonoObject {
    unsafe { MonoObject::from_handle(MonoHandle(h)) }
}

pub fn handle_of(v: &Json) -> Option<i32> {
    v.get("handle").and_then(Json::as_i64).map(|h| h as i32)
}

/// The running world's seed (Session.RandomSeed, persisted in the
/// save): the identity key for the genome memory sidecar.
pub fn session_seed() -> Result<i64, String> {
    let session = MonoType::find("Session")
        .and_then(|t| t.singleton_instance())
        .ok_or("Session.Instance not found (no game loaded?)")?;
    session
        .read_field("RandomSeed")?
        .as_i64()
        .ok_or_else(|| "RandomSeed is not a number".into())
}

pub fn community_manager() -> Result<MonoObject, String> {
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
pub fn for_each_community(
    mut f: impl FnMut(MonoObject) -> Result<bool, String>,
) -> Result<(), String> {
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

pub fn display_name(com: &MonoObject) -> String {
    com.invoke("GetDisplayNameString", &json!([]))
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "<unnamed>".to_string())
}

pub fn ctype(com: &MonoObject) -> String {
    com.read_field("CommunityType")
        .map(|v| v.as_str().unwrap_or("?").to_string())
        .unwrap_or_else(|_| "?".to_string())
}

pub fn list_len(owner: &MonoObject, field: &str) -> i64 {
    match owner.read_field(field).ok().as_ref().and_then(handle_of) {
        Some(h) => own(h)
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        None => 0,
    }
}

/// Read an (x, y) pair from either the shim's struct-object form
/// (`{"x": .., "y": ..}`, the current bridge) or the legacy
/// ToString form ("(x, y)").
pub fn parse_xy(v: &Json) -> Option<(f32, f32)> {
    if let Some(o) = v.as_object() {
        let g = |k: &str| o.get(k).and_then(Json::as_f64).map(|f| f as f32);
        if let (Some(x), Some(y)) = (g("x"), g("y")) {
            return Some((x, y));
        }
    }
    let s = v.as_str()?;
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let mut it = s.split(',');
    let x = it.next()?.trim().parse::<f32>().ok()?;
    let y = it.next()?.trim().parse::<f32>().ok()?;
    Some((x, y))
}

pub fn pos_of(obj: &MonoObject) -> Option<(f32, f32)> {
    if let Ok(v) = obj.read_field("PosXZ") {
        if let Some(p) = parse_xy(&v) {
            return Some(p);
        }
    }
    obj.read_field("Tile").ok().and_then(|v| parse_xy(&v))
}

/// Run `f` against a handle without releasing it.
pub fn with<R>(h: i32, f: impl FnOnce(&MonoObject) -> R) -> R {
    let o = own(h);
    let r = f(&o);
    std::mem::forget(o);
    r
}

/// Give a freshly-spawned band a squad and send every living member
/// walking to `tile`. A mod-spawned off-map arrival
/// (incursion::spawn_band_at_edge) otherwise just wanders on its own
/// SurvivorGoal; this points it at the camp or husk it is bound for.
/// Returns the squad id.
pub fn march_band_to(com_h: i32, tile: (i64, i64), behaviour: &str) -> Result<i64, String> {
    let goal = json!({"x": tile.0, "y": tile.1});
    with(com_h, |com| -> Result<i64, String> {
        let squad_h = handle_of(&com.invoke("AddSquad", &json!([behaviour, 0]))?)
            .ok_or("AddSquad gave no squad")?;
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let n = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
            for i in 0..n {
                if let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) {
                    let alive = with(h, |member| {
                        member
                            .invoke("get_AliveAndNotZombie", &json!([]))
                            .map(|v| v == json!(true))
                            .unwrap_or(false)
                    });
                    if alive {
                        let _ = com.invoke(
                            "AddToSquad",
                            &json!([{ "handle": h }, { "handle": squad_h }]),
                        );
                    }
                }
            }
        }
        let squad = own(squad_h);
        squad.write_field("GoalTile", &goal)?;
        let sid = squad.read_field("Id")?.as_i64().unwrap_or(-1);
        drop(squad);
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, goal, null, false]),
        )?;
        Ok(sid)
    })
}

/// Centre of a community's base rect, in tile coordinates.
pub fn base_centre(com: &MonoObject) -> Option<(i64, i64)> {
    let rect = com.read_field("BaseRect").ok()?;
    let o = rect.as_object()?;
    let min = o.get("min")?.as_object()?;
    let max = o.get("max")?.as_object()?;
    let g = |m: &serde_json::Map<String, Json>, k: &str| m.get(k).and_then(Json::as_i64);
    Some((
        (g(min, "x")? + g(max, "x")?) / 2,
        (g(min, "y")? + g(max, "y")?) / 2,
    ))
}

/// Which stored goods a transfer moves. Food is what the game's
/// own nutrition ledger counts: `Equipment.GetNutrition() > 0`
/// (GetHarvestedNutritionAmount sums exactly that).
#[derive(Clone, Copy)]
pub enum GoodsFilter {
    Any,
    Food,
    NonFood,
}

impl GoodsFilter {
    fn matches(self, item: &MonoObject) -> bool {
        match self {
            GoodsFilter::Any => true,
            GoodsFilter::Food | GoodsFilter::NonFood => {
                let n = item
                    .invoke("GetNutrition", &json!([]))
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                match self {
                    GoodsFilter::Food => n > 0.0,
                    _ => n <= 0.0,
                }
            }
        }
    }
}

/// Move a community's building-stored items into the carriers'
/// inventories, round-robin, via the game's own
/// `EquipmentContainer.Take` (removes from source) + `Add` (gives
/// to carrier, honoring real carry capacity), up to `max_stacks`
/// stacks matching `filter`. Returns how many stacks were carried
/// off. Predation drains the husk (high cap, Any); steal takes a
/// burglar's armful; trade loads food and collects non-food
/// payment. Buildings/crops stay put; only portable stored goods
/// move, and only as far as living hands can carry them.
pub fn carry_off_stored_goods(
    from: &MonoObject,
    carriers: &[i32],
    max_stacks: i64,
    filter: GoodsFilter,
) -> Result<i64, String> {
    let Some(b_h) = handle_of(&from.read_field("Buildings")?) else {
        return Ok(0);
    };
    let blist = own(b_h);
    let nb = blist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    let mut carried = 0i64;
    let mut carrier_ix = 0usize;
    for bi in 0..nb {
        let Some(bh) = handle_of(&blist.invoke("get_Item", &json!([bi]))?) else {
            continue;
        };
        let building = own(bh);
        let Some(inv_h) = handle_of(&building.read_field("Inventory")?) else {
            continue;
        };
        let inv = own(inv_h);
        // Per pass: find the first item the filter wants; Take()
        // shrinks the container, so re-scan from the top each time.
        loop {
            let count = inv
                .invoke("get_Count", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let mut pick: Option<i32> = None;
            for i in 0..count {
                let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([i]))?) else {
                    continue;
                };
                let item = own(item_h);
                if filter.matches(&item) {
                    std::mem::forget(item);
                    pick = Some(item_h);
                    break;
                }
            }
            let Some(item_h) = pick else { break };
            let item = own(item_h);
            let amount = item
                .invoke("GetAmount", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            drop(item);
            // Take the whole stack from the building.
            let taken = inv.invoke(
                "Take",
                &json!([{ "handle": bh }, { "handle": item_h }, amount]),
            )?;
            let Some(taken_h) = handle_of(&taken) else {
                break; // Take failed; stop draining this building
            };
            // Hand it to a carrier (round-robin). Add honors carry
            // capacity; anything that doesn't fit is dropped at the
            // site by the game, which is realistic (they took what
            // they could carry).
            let carrier = own(carriers[carrier_ix % carriers.len()]);
            let _ = carrier.invoke(
                "Add",
                &json!([{ "handle": carrier.handle().0 }, { "handle": taken_h }]),
            );
            std::mem::forget(carrier);
            carrier_ix += 1;
            carried += 1;
            if carried >= max_stacks {
                return Ok(carried);
            }
        }
    }
    Ok(carried)
}

/// Reclaim mission squads a prior mod generation left behind. A
/// Trade-behaviour squad on a Normal/Looter AI SETTLEMENT can only
/// be ours: vanilla gives Trade squads solely to roving and
/// temporary communities (FixupTraders, the abandon-settlement
/// migration). A hot reload empties the Rust-side mission lists,
/// so at init any such squad is an orphan; disbanding it hands the
/// member back to normal settlement AI, which walks them home.
pub fn sweep_orphan_trade_squads() {
    let mut removed = 0u32;
    let _ = for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let Some(s_h) = handle_of(&com.read_field("Squads")?) else {
            return Ok(true);
        };
        let slist = own(s_h);
        let count = slist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
        // Collect first: RemoveSquad mutates the list.
        let mut orphans = Vec::new();
        for i in 0..count {
            let Some(h) = handle_of(&slist.invoke("get_Item", &json!([i]))?) else {
                continue;
            };
            let squad = own(h);
            if squad.read_field("Behaviour").ok() == Some(json!("Trade")) {
                std::mem::forget(squad);
                orphans.push(h);
            }
        }
        for h in orphans {
            let _ = com.invoke("RemoveSquad", &json!([{ "handle": h }]));
            drop(own(h));
            removed += 1;
        }
        Ok(true)
    });
    if removed > 0 {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: reclaimed {removed} orphaned mission squad(s) from a prior generation"
            ),
        );
    }
}

/// Run `f` on the Unity main thread and wait for its result
/// (same oneshot shape as unityforge's write_field op).
pub fn on_main_thread<F>(f: F) -> Result<Json, String>
where
    F: FnOnce() -> Result<Json, String> + Send + 'static,
{
    use std::sync::Arc;

    use parking_lot::Mutex;
    let result: Arc<Mutex<Option<Result<Json, String>>>> = Arc::new(Mutex::new(None));
    let r2 = result.clone();
    unityforge::main_thread_queue::MAIN_QUEUE.push(move || {
        *r2.lock() = Some(f());
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(r) = result.lock().take() {
            return r;
        }
        if std::time::Instant::now() >= deadline {
            return Err("op: main-thread queue timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
