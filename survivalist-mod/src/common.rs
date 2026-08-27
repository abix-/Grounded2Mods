//! Shared bridge helpers for the survivalist modules (war,
//! growth, development). Extracted at the third consumer.

use serde_json::{Value as Json, json};
pub use modforge::item::GoodsFilter;
use modforge::item::{GoodsCandidate, GoodsTransferPlanner};
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

pub use unityforge::mono::{
    json_handle as handle_of, owned_object as own, unity_xy as parse_xy, with_object as with,
};

/// The running world's seed (Session.RandomSeed, persisted in the
/// save): the identity key for the genome memory sidecar.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn session_seed() -> Result<i64, String> {
    let session = MonoType::find("Session")
        .and_then(|t| t.singleton_instance())
        .ok_or("Session.Instance not found (no game loaded?)")?;
    session
        .read_field("RandomSeed")?
        .as_i64()
        .ok_or_else(|| "RandomSeed is not a number".into())
}

/// Open the loaded world's community manager.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
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
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn for_each_community(
    mut f: impl FnMut(MonoObject) -> Result<bool, String>,
) -> Result<(), String> {
    let cm = community_manager()?;
    let list_h = handle_of(&cm.read_field("Communities")?).ok_or("Communities list is null")?;
    let list = own(list_h);
    let count = list.list_len()?;
    for i in 0..count {
        let Some(item_h) = list.list_handle(i)? else {
            continue;
        };
        if !f(own(item_h))? {
            break;
        }
    }
    Ok(())
}

/// Read a camp's player-visible name.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn display_name(com: &MonoObject) -> String {
    com.invoke("GetDisplayNameString", &json!([]))
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "<unnamed>".to_string())
}

/// Read the game type that determines a camp's behavior.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn ctype(com: &MonoObject) -> String {
    com.read_field("CommunityType")
        .map(|v| v.as_str().unwrap_or("?").to_string())
        .unwrap_or_else(|_| "?".to_string())
}

/// Read an object's world or tile position.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn pos_of(obj: &MonoObject) -> Option<(f32, f32)> {
    if let Ok(v) = obj.read_field("PosXZ") {
        if let Some(p) = parse_xy(&v) {
            return Some(p);
        }
    }
    obj.read_field("Tile").ok().and_then(|v| parse_xy(&v))
}

/// Give a freshly-spawned band a squad and send every living member
/// walking to `tile`. A mod-spawned off-map arrival
/// (incursion::spawn_band_at_edge) otherwise just wanders on its own
/// SurvivorGoal; this points it at the camp or husk it is bound for.
/// Returns the squad id.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn march_band_to(com_h: i32, tile: (i64, i64), behaviour: &str) -> Result<i64, String> {
    let goal = json!({"x": tile.0, "y": tile.1});
    with(com_h, |com| -> Result<i64, String> {
        let squad_h = handle_of(&com.invoke("AddSquad", &json!([behaviour, 0]))?)
            .ok_or("AddSquad gave no squad")?;
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let n = mlist.list_len_or_zero()?;
            for i in 0..n {
                if let Some(h) = mlist.list_handle(i)? {
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
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
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

/// Classify an item through Survivalist's nutrition API, then apply
/// Modforge's engine-independent goods filter.
pub fn goods_match(filter: GoodsFilter, item: &MonoObject) -> bool {
    if filter == GoodsFilter::Any {
        return true;
    }
    let is_food = item
        .invoke("GetNutrition", &json!([]))
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
        > 0.0;
    filter.matches(is_food)
}

/// Secure (settlement upgrades): a hostile taking tests the
/// building's locks before draining it. The level and the roll
/// live in the C# shim beside the other track knobs
/// (Upgrades.cs SecureBlocks); a shim without the entry fails
/// open (the locks do not hold).
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
fn secure_blocks(building: &MonoObject) -> bool {
    let Some(id) = building.read_field("Id").ok().and_then(|v| v.as_i64()) else {
        return false;
    };
    match mono::invoke_static("SettlementUpgrades", "SecureBlocks", &json!([id])) {
        Ok(v) => v == json!(true),
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: secure check failed: {e}"),
            );
            false
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
///
/// `hostile` marks a taking against the owner's will (theft,
/// predation, tribute): each building's Secure locks (settlement
/// upgrades) are tested and a held lock keeps that whole
/// building's stores. Willing loads (a camp loading its own
/// wares, paying for goods or work) never test locks.
/// Stays here because Survivalist observes and mutates its managed
/// inventories; Modforge owns selection, limits, and distribution.
pub fn carry_off_stored_goods(
    from: &MonoObject,
    carriers: &[i32],
    max_stacks: i64,
    filter: GoodsFilter,
    hostile: bool,
) -> Result<i64, String> {
    let Some(b_h) = handle_of(&from.read_field("Buildings")?) else {
        return Ok(0);
    };
    let blist = own(b_h);
    let nb = blist.list_len_or_zero()?;
    let mut planner =
        GoodsTransferPlanner::new(filter, max_stacks.max(1) as usize, carriers.len());
    for bi in 0..nb {
        let Some(bh) = blist.list_handle(bi)? else {
            continue;
        };
        let building = own(bh);
        if !planner.can_take_from(hostile && secure_blocks(&building)) {
            continue;
        }
        let Some(inv_h) = handle_of(&building.read_field("Inventory")?) else {
            continue;
        };
        let inv = own(inv_h);
        // Per pass: find the most VALUABLE item the filter wants
        // (the world values quality: every act that loads loot or
        // payment through here grabs the good blade first, docs/
        // status.md "Quality system"); Take() shrinks the
        // container, so re-scan each time.
        loop {
            let count = inv.list_len().unwrap_or(0);
            let mut items = Vec::new();
            let mut candidates = Vec::new();
            for i in 0..count {
                let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([i]))?) else {
                    continue;
                };
                let item = own(item_h);
                let is_food = if filter == GoodsFilter::Any {
                    false
                } else {
                    goods_match(GoodsFilter::Food, &item)
                };
                let value = if filter.matches(is_food) {
                    item
                        .invoke("GetPrototype", &json!([]))
                        .ok()
                        .as_ref()
                        .and_then(handle_of)
                        .map(|ph| {
                            let p = own(ph);
                            p.read_field("BasePrice")
                                .ok()
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0)
                        })
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                items.push(item);
                candidates.push(GoodsCandidate { value, is_food });
            }
            let Some(selection) = planner.next(&candidates) else {
                break;
            };
            let item = items.swap_remove(selection.candidate);
            drop(items);
            let item_h = item.handle().0;
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
            let carrier = own(carriers[selection.carrier]);
            let _ = carrier.invoke(
                "Add",
                &json!([{ "handle": carrier.handle().0 }, { "handle": taken_h }]),
            );
            std::mem::forget(carrier);
            planner.record_success();
            if planner.complete() {
                return Ok(planner.transferred() as i64);
            }
        }
    }
    Ok(planner.transferred() as i64)
}

/// Count a community's stored stacks matching the filter, up to
/// `cap` (early exit; cap 1 is a cheap "has any" test). The work
/// pillar's "can they pay" and "what does it pay" reads.
/// Stays here because it traverses Survivalist's managed buildings
/// and inventories; Modforge owns the goods filter.
pub fn count_stored_goods(com: &MonoObject, filter: GoodsFilter, cap: i64) -> i64 {
    let Some(b_h) = com
        .read_field("Buildings")
        .ok()
        .as_ref()
        .and_then(handle_of)
    else {
        return 0;
    };
    let mut found = 0i64;
    let blist = own(b_h);
    let nb = blist.list_len().unwrap_or(0);
    for bi in 0..nb {
        let Some(bh) = blist.list_handle(bi).ok().flatten() else {
            continue;
        };
        let building = own(bh);
        let Some(inv_h) = building
            .read_field("Inventory")
            .ok()
            .as_ref()
            .and_then(handle_of)
        else {
            continue;
        };
        let inv = own(inv_h);
        let n = inv.list_len().unwrap_or(0);
        for i in 0..n {
            let Some(item_h) = inv
                .invoke("GetItem", &json!([i]))
                .ok()
                .as_ref()
                .and_then(handle_of)
            else {
                continue;
            };
            let item = own(item_h);
            if goods_match(filter, &item) {
                found += 1;
                if found >= cap {
                    return found;
                }
            }
        }
    }
    found
}

/// Reclaim mission squads a prior mod generation left behind. A
/// Trade-behaviour squad on a Normal/Looter AI SETTLEMENT can only
/// be ours: vanilla gives Trade squads solely to roving and
/// temporary communities (FixupTraders, the abandon-settlement
/// migration). A hot reload empties the Rust-side mission lists,
/// so at init any such squad is an orphan; disbanding it hands the
/// member back to normal settlement AI, which walks them home.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
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
        let count = slist.list_len_or_zero()?;
        // Collect first: RemoveSquad mutates the list.
        let mut orphans = Vec::new();
        for i in 0..count {
            let Some(h) = slist.list_handle(i)? else {
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

// ---- mission helpers --------------------------------------------------------

/// True when the character at `h` is alive and not a zombie.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn is_npc_alive(h: i32) -> Result<bool, String> {
    Ok(with(h, |t| t.invoke("get_AliveAndNotZombie", &json!([])))? == json!(true))
}

/// Squared tile distance from the character at `agent_h` to the
/// nearest building of the community at `com_h`.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn dist_sq_to_building(agent_h: i32, com_h: i32) -> Result<f64, String> {
    let tile = with(agent_h, |t| t.invoke("get_Tile", &json!([])))?;
    Ok(with(com_h, |c| {
        c.invoke("GetDistSqToNearestBuilding", &json!([tile]))
    })?
    .as_f64()
    .unwrap_or(f64::MAX))
}

/// Retarget an existing squad toward `home`.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn send_squad_home(com_h: i32, squad_id: i64, home: (i64, i64)) -> Result<(), String> {
    let dest = json!({"x": home.0, "y": home.1});
    with(com_h, |com| -> Result<(), String> {
        if let Ok(sq) = com.invoke("GetSquad", &json!([squad_id])) {
            if let Some(sq_h) = handle_of(&sq) {
                let squad = own(sq_h);
                squad.write_field("GoalTile", &dest)?;
                com.invoke(
                    "SetSquadAction",
                    &json!([{ "handle": sq_h }, "GoTo", 0, dest, null, false]),
                )?;
            }
        }
        Ok(())
    })
}

/// Remove a squad from the community and release a list of owned
/// handles.
/// Stays here because it uses Survivalist's exact community, squad, inventory, and object conventions.
pub fn remove_squad_and_drop(com_h: i32, squad_id: i64, handles: &[i32]) {
    with(com_h, |com| {
        if let Ok(sq) = com.invoke("GetSquad", &json!([squad_id])) {
            if let Some(sq_h) = handle_of(&sq) {
                let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
            }
        }
    });
    for &h in handles {
        drop(own(h));
    }
}
