//! Research: the pickup creation call (docs/research.md 4b, the
//! open half): what instantiates a ground pickup at a position.
//!
//! Known from the 2026-08-08 metadata scan: DropCash / DropItem /
//! CollectCash exist as FishNet RPCs on ONE class that also has
//! ragdoll + Min/MaxRandomCash methods; a lastDroppedItem:
//! NetworkedItemPickup property exists; TrashManager has a public
//! CreateTrashItem(String, Vector3, ...) -> TrashItem. The
//! declaring class is not recoverable from flat strings, so this
//! test finds it LIVE: inspect a real NPC, follow every
//! ScheduleOne-typed component hanging off it, and list each
//! one's methods hunting for the drop/cash/pickup creators.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_pickup. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, first_handle, handle_of, ping_or_skip, player_position};
use serde_json::{Value, json};

const WORDS: [&str; 6] = ["Drop", "Cash", "Loot", "Pickup", "Create", "Spawn"];

/// Print declared methods on `class` whose name contains one of
/// WORDS; FishNet reader/writer plumbing skipped. Returns hits.
fn print_loot_methods(api: &modforge::client::Api<Value>, class: &str) -> usize {
    let r = api.op("list_methods", json!({"class": class}));
    if !r.ok {
        println!("  list_methods({class}) failed: {:?}", r.error);
        return 0;
    }
    let methods = r.result["methods"].as_array().cloned().unwrap_or_default();
    let mut hits = 0;
    for m in &methods {
        let name = m["name"].as_str().unwrap_or("");
        if name.starts_with("RpcReader") || name.starts_with("RpcWriter") {
            continue;
        }
        if m["declared_on"].as_str() != Some(class) {
            continue;
        }
        if !WORDS.iter().any(|w| name.contains(w)) {
            continue;
        }
        hits += 1;
        println!(
            "  {class} :: {}({}) -> {}{}",
            name,
            m["params"].as_i64().unwrap_or(-1),
            m["return"].as_str().unwrap_or("?"),
            if m["static"].as_bool() == Some(true) { " [static]" } else { "" },
        );
    }
    hits
}

/// Report every live CashPickup: name, world position, active
/// flags, Value. Answers whether the scene originals (the clone
/// sources) are visible objects at all, and where everything is.
#[test]
fn diag_cash_pickups() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some(all) = common::walk(&api, "ScheduleOne.ItemFramework.CashPickup") else {
        return;
    };
    for i in &all {
        let Some(h) = i["handle"].as_i64() else { continue };
        let name = i["name"].as_str().unwrap_or("?");
        let pos = api
            .op("read_field", json!({"handle": h, "field": "transform"}))
            .result;
        let pos = match handle_of(&pos) {
            Some(th) => {
                let p = api.op("invoke_method", json!({"handle": th, "method": "get_position", "args": []}));
                common::parse_vec3(&p.result)
            }
            None => None,
        };
        let go = api.op("invoke_method", json!({"handle": h, "method": "get_gameObject", "args": []}));
        let (mut active_self, mut active_hier) = (None, None);
        if let Some(gh) = handle_of(&go.result) {
            active_self = api
                .op("invoke_method", json!({"handle": gh, "method": "get_activeSelf", "args": []}))
                .result
                .as_bool();
            active_hier = api
                .op("invoke_method", json!({"handle": gh, "method": "get_activeInHierarchy", "args": []}))
                .result
                .as_bool();
        }
        let value = api.op("read_field", json!({"handle": h, "field": "Value"})).result;
        println!(
            "{name}: pos={pos:?} activeSelf={active_self:?} activeInHierarchy={active_hier:?} value={value}"
        );
    }
}

/// Print every Spawn/Despawn overload FishNet's ServerManager
/// declares (param counts), to match the invoke exactly.
#[test]
fn diag_spawn_signatures() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let r = api.op("list_methods", json!({"class": "Il2CppFishNet.Managing.Server.ServerManager"}));
    if !r.ok {
        println!("list_methods failed: {:?}", r.error);
        return;
    }
    for m in r.result["methods"].as_array().cloned().unwrap_or_default() {
        let name = m["name"].as_str().unwrap_or("");
        if name.contains("Spawn") || name.contains("Despawn") {
            println!(
                "  {}({}) -> {} [{}]",
                name,
                m["params"].as_i64().unwrap_or(-1),
                m["return"].as_str().unwrap_or("?"),
                m["declared_on"].as_str().unwrap_or("?"),
            );
        }
    }
}

/// Print the player's current world position and stop.
#[test]
fn where_is_the_player() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    match player_position(&api) {
        Some((x, y, z)) => println!("player at ({x:.1}, {y:.1}, {z:.1})"),
        None => println!("no player position; not in a save?"),
    }
}

#[test]
fn find_pickup_creation_call() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // A live object's fields name every component class on it.
    // Sweep the three combat-relevant roots.
    let mut classes: Vec<String> = Vec::new();
    for root in [
        "ScheduleOne.NPCs.NPC",
        "ScheduleOne.PlayerScripts.Player",
        "ScheduleOne.Cartel.CartelGoon",
    ] {
        let Some(h) = first_handle(&api, root) else {
            println!("{root}: no live instance");
            continue;
        };
        let inspect = api.op("inspect_object", json!({"handle": h}));
        if !inspect.ok {
            println!("inspect_object({root}) failed: {:?}", inspect.error);
            continue;
        }
        let before = classes.len();
        if let Some(fields) = inspect.result["fields"].as_object() {
            for (_name, v) in fields {
                let Some(ty) = v.get("il2cpp_type").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(vh) = v.get("handle").and_then(Value::as_i64) {
                    api.op("release_handle", json!({"handle": vh}));
                }
                if ty.contains("ScheduleOne") && !classes.iter().any(|c| c == ty) {
                    classes.push(ty.to_string());
                }
            }
        }
        println!("{root}: {} new component class(es)", classes.len() - before);
    }
    for c in &classes {
        println!("  seen: {c}");
    }

    // Hunt the loot creators on each, plus the standing suspects.
    for extra in [
        "Il2CppScheduleOne.Trash.TrashManager",
        "Il2CppScheduleOne.ItemFramework.ItemPickup",
        "Il2CppScheduleOne.ItemFramework.NetworkedItemPickup",
        "Il2CppScheduleOne.ItemFramework.CashPickup",
        "Il2CppScheduleOne.Economy.DeadDrop",
        "Il2CppScheduleOne.NPCs.CharacterClasses.SewerGoblin",
        "Il2CppScheduleOne.Money.MoneyManager",
        "Il2CppScheduleOne.ItemFramework.ItemManager",
        "Il2CppScheduleOne.Registry",
        "Il2CppScheduleOne.PlayerScripts.PlayerInventory",
        "Il2CppScheduleOne.ItemFramework.ItemSlot",
        "Il2CppScheduleOne.AvatarFramework.Equipping.AvatarEquipment",
    ] {
        if !classes.iter().any(|c| c == extra) {
            classes.push(extra.to_string());
        }
    }
    let mut total = 0;
    for class in &classes {
        total += print_loot_methods(&api, class);
    }
    println!("{total} loot-shaped declared method(s) across {} classes", classes.len());

    // A live CashPickup, if any, shows how ground cash is shaped.
    if let Some(h) = first_handle(&api, "ScheduleOne.ItemFramework.CashPickup") {
        let inspect = api.op("inspect_object", json!({"handle": h}));
        println!(
            "CashPickup[0] inspect:\n{}",
            serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
        );
    }
}

/// The creation experiment (FishNet's documented runtime pattern:
/// Instantiate an existing networked object, then
/// ServerManager.Spawn it): clone a live CashPickup at the
/// player's feet with Value=100. Run ONLY with the operator
/// in-game to check the result: cash visible, pickable, +$100.
#[test]
fn clone_cash_pickup_at_player() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; not in a save?");
        return;
    };
    println!("player at ({px:.1}, {py:.1}, {pz:.1})");
    let Some(originals) = common::walk(&api, "ScheduleOne.ItemFramework.CashPickup") else {
        return;
    };
    let Some(cash) = originals
        .iter()
        .find(|i| !i["name"].as_str().unwrap_or("").contains("(Clone)"))
        .and_then(|i| i["handle"].as_i64())
    else {
        println!("no live non-clone CashPickup to clone");
        return;
    };

    // Instantiate returns a base-typed UnityEngine.Object proxy
    // (no CashPickup members reachable); re-find each clone via
    // walk_class, which downcasts hits to the real proxy type.
    // Unity names clones "<original>(Clone)"; each processed
    // clone is renamed so the next drop is unambiguous.
    let drops = 5;
    for n in 0..drops {
        let clone = api.op(
            "invoke_static",
            json!({"class": "UnityEngine.Object", "method": "Instantiate",
                   "args": [{"$handle": cash}]}),
        );
        if handle_of(&clone.result).is_none() {
            println!("Instantiate failed: {:?} {}", clone.error, clone.result);
            return;
        }
        let Some(after) = common::walk(&api, "ScheduleOne.ItemFramework.CashPickup") else {
            return;
        };
        let Some(ch) = after
            .iter()
            .find(|i| i["name"].as_str().unwrap_or("").ends_with("(Clone)"))
            .and_then(|i| i["handle"].as_i64())
        else {
            println!("clone {n} not found in the post-instantiate walk");
            return;
        };

        // A small circle around the player's feet.
        let angle = (n as f64) * std::f64::consts::TAU / (drops as f64);
        let (dx, dz) = (1.5 * angle.cos(), 1.5 * angle.sin());
        let transform = api.op("read_field", json!({"handle": ch, "field": "transform"}));
        if let Some(th) = handle_of(&transform.result) {
            let mv = api.op(
                "invoke_method",
                json!({"handle": th, "method": "set_position",
                       "args": [{"x": px + dx, "y": py, "z": pz + dz}]}),
            );
            if !mv.ok {
                println!("drop {n}: set_position failed: {:?}", mv.error);
            }
        }
        // The clone came from an INACTIVE prefab template ("$10
        // Pickup" / "Dynamic Amount Cash Pickup" live parked in a
        // hidden container): activate it explicitly.
        let go = api.op("invoke_method", json!({"handle": ch, "method": "get_gameObject", "args": []}));
        let mut active_ok = false;
        if let Some(gh) = handle_of(&go.result) {
            let act = api.op(
                "invoke_method",
                json!({"handle": gh, "method": "SetActive", "args": [true]}),
            );
            active_ok = act.ok;
        }

        // FishNet destroys un-spawned NetworkObjects (every clone
        // from the earlier runs vanished from the walk), so
        // ServerManager.Spawn is mandatory. The manager comes from
        // FishNet's InstanceFinder static.
        let nob = api.op("invoke_method", json!({"handle": ch, "method": "get_NetworkObject", "args": []}));
        let sm = api.op(
            "invoke_static",
            json!({"class": "Il2CppFishNet.InstanceFinder", "method": "get_ServerManager", "args": []}),
        );
        let spawn_ok = match (handle_of(&nob.result), handle_of(&sm.result)) {
            (Some(nh), Some(sh)) => {
                // Spawn(NetworkObject|GameObject, NetworkConnection,
                // Scene): nulls land as null / default(Scene).
                let mut ok = false;
                for arg in [json!({"$handle": nh}), go.result.clone()] {
                    let spawn = api.op(
                        "invoke_method",
                        json!({"handle": sh, "method": "Spawn",
                               "args": [arg, null, {}]}),
                    );
                    if spawn.ok {
                        ok = true;
                        break;
                    }
                    println!("drop {n}: Spawn attempt failed: {:?}", spawn.error);
                }
                ok
            }
            _ => {
                println!(
                    "drop {n}: no NetworkObject/ServerManager: nob={} sm={} ({:?})",
                    nob.result, sm.result, sm.error
                );
                false
            }
        };

        let val = api.op("write_field", json!({"handle": ch, "field": "Value", "value": 100.0}));
        let vis = api.op("invoke_method", json!({"handle": ch, "method": "UpdateCashStackVisuals", "args": []}));
        let renamed = api.op(
            "invoke_method",
            json!({"handle": ch, "method": "set_name",
                   "args": [format!("CashPickup_modforge_{n}")]}),
        );
        println!(
            "drop {n}: placed at ({:.1}, {:.1}, {:.1}) active_ok={active_ok} spawn_ok={spawn_ok} value_ok={} visuals_ok={} rename_ok={}",
            px + dx, py, pz + dz, val.ok, vis.ok, renamed.ok,
        );
    }
    println!("OPERATOR CHECK: {drops} stacks of $100 around you? Can you pick them up?");
}
