//! Research: full inventory of Behaviour components in the live
//! game. Walks every known Behaviour subclass from
//! ScheduleOne.NPCs.Behaviour, counts instances, inspects one
//! of each to see fields and state.
//!
//! Goal: understand which components could be added to minted
//! NPCs for patrol, guard, and hold-post behavior.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_behaviours -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, handle_of, ping_or_skip, walk, print_declared_methods};
use serde_json::json;

const BEHAVIOUR_CLASSES: &[&str] = &[
    // base class (custom NPCs get some of these via S1API)
    "ScheduleOne.NPCs.Behaviour.StationaryBehaviour",
    "ScheduleOne.NPCs.Behaviour.CombatBehaviour",
    "ScheduleOne.NPCs.Behaviour.FleeBehaviour",
    "ScheduleOne.NPCs.Behaviour.CoweringBehaviour",
    "ScheduleOne.NPCs.Behaviour.ScheduleBehaviour",
    // police-specific (from PoliceOfficer inspect)
    "ScheduleOne.NPCs.Behaviour.FootPatrolBehaviour",
    "ScheduleOne.NPCs.Behaviour.VehiclePatrolBehaviour",
    "ScheduleOne.NPCs.Behaviour.SentryBehaviour",
    "ScheduleOne.NPCs.Behaviour.PursuitBehaviour",
    "ScheduleOne.NPCs.Behaviour.CheckpointBehaviour",
    "ScheduleOne.NPCs.Behaviour.BodySearchBehaviour",
    // possible additional types (may or may not exist)
    "ScheduleOne.NPCs.Behaviour.IdleBehaviour",
    "ScheduleOne.NPCs.Behaviour.WanderBehaviour",
    "ScheduleOne.NPCs.Behaviour.GuardBehaviour",
    "ScheduleOne.NPCs.Behaviour.PatrolBehaviour",
    "ScheduleOne.NPCs.Behaviour.HoldPositionBehaviour",
    "ScheduleOne.NPCs.Behaviour.AmbushBehaviour",
    // the base abstract class itself
    "ScheduleOne.NPCs.Behaviour.Behaviour",
];

#[test]
fn behaviour_inventory() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== Behaviour subclass inventory ===\n");

    for class in BEHAVIOUR_CLASSES {
        let short = class.rsplit('.').next().unwrap_or(class);
        let instances = walk(&api, class);
        match instances {
            Some(list) if !list.is_empty() => {
                println!("\n--- {short}: {} instance(s) LIVE ---", list.len());

                // Inspect the first instance to see fields
                if let Some(first) = list.first() {
                    if let Some(fh) = first["handle"].as_i64() {
                        let inspect = api.op(
                            "inspect_object",
                            json!({"handle": fh}),
                        );
                        println!(
                            "  inspect[0]:\n{}",
                            serde_json::to_string_pretty(
                                &inspect.result
                            )
                            .unwrap_or_default()
                        );
                        api.op(
                            "release_handle",
                            json!({"handle": fh}),
                        );
                    }
                }
                // Release remaining handles
                for inst in list.iter().skip(1) {
                    if let Some(h) = inst["handle"].as_i64() {
                        api.op(
                            "release_handle",
                            json!({"handle": h}),
                        );
                    }
                }
            }
            Some(_) => {
                println!("\n--- {short}: 0 instances (type exists but none live) ---");
            }
            None => {
                println!("\n--- {short}: type NOT FOUND ---");
            }
        }
    }

    // Also inspect the NPCBehaviour component on different NPC
    // types to see which behaviors each NPC type carries
    println!("\n\n=== Behaviour stacks by NPC type ===\n");

    let npc_types = [
        "ScheduleOne.NPCs.Behaviour.PoliceOfficer",
        "ScheduleOne.Law.PoliceOfficer",
        "ScheduleOne.Levelling.CartelGoon",
        "ScheduleOne.NPCs.NPC",
    ];

    for npc_class in &npc_types {
        let short = npc_class.rsplit('.').next().unwrap_or(npc_class);
        let instances = walk(&api, npc_class);
        match instances {
            Some(list) if !list.is_empty() => {
                println!("\n--- {short} ({} live): inspecting first ---", list.len());
                if let Some(first) = list.first() {
                    if let Some(nh) = first["handle"].as_i64() {
                        // Get all components on this NPC
                        let comps = api.op(
                            "invoke_method",
                            json!({
                                "handle": nh,
                                "method": "GetComponents",
                                "args": [],
                            }),
                        );
                        if comps.ok {
                            if let Some(ch) = handle_of(&comps.result) {
                                let count = api.op(
                                    "invoke_method",
                                    json!({
                                        "handle": ch,
                                        "method": "get_Length",
                                        "args": [],
                                    }),
                                );
                                if let Some(n) = count.result.as_i64() {
                                    println!("  {} components total", n);
                                    for i in 0..n {
                                        let item = api.op(
                                            "invoke_method",
                                            json!({
                                                "handle": ch,
                                                "method": "get_Item",
                                                "args": [i],
                                            }),
                                        );
                                        if item.ok {
                                            // Print the type name
                                            if let Some(ih) = handle_of(&item.result) {
                                                let gt = api.op(
                                                    "invoke_method",
                                                    json!({
                                                        "handle": ih,
                                                        "method": "GetType",
                                                        "args": [],
                                                    }),
                                                );
                                                if gt.ok {
                                                    if let Some(th) = handle_of(&gt.result) {
                                                        let tn = api.op(
                                                            "invoke_method",
                                                            json!({
                                                                "handle": th,
                                                                "method": "get_FullName",
                                                                "args": [],
                                                            }),
                                                        );
                                                        let name = tn.result
                                                            .as_str()
                                                            .or_else(|| tn.result.get("str").and_then(|s| s.as_str()))
                                                            .unwrap_or("?");
                                                        // Filter to only Behaviour components
                                                        if name.contains("Behaviour") || name.contains("Behavior") {
                                                            println!("  [{i}] {name}");
                                                        }
                                                        api.op("release_handle", json!({"handle": th}));
                                                    }
                                                }
                                                api.op("release_handle", json!({"handle": ih}));
                                            }
                                        }
                                    }
                                }
                                api.op("release_handle", json!({"handle": ch}));
                            }
                        } else {
                            // Fallback: just inspect the NPC
                            let inspect = api.op(
                                "inspect_object",
                                json!({"handle": nh}),
                            );
                            println!(
                                "  GetComponents failed, inspect:\n{}",
                                serde_json::to_string_pretty(
                                    &inspect.result
                                )
                                .unwrap_or_default()
                            );
                        }
                        api.op("release_handle", json!({"handle": nh}));
                    }
                }
                for inst in list.iter().skip(1) {
                    if let Some(h) = inst["handle"].as_i64() {
                        api.op("release_handle", json!({"handle": h}));
                    }
                }
            }
            Some(_) => {
                println!("\n--- {short}: 0 instances ---");
            }
            None => {
                println!("\n--- {short}: type NOT FOUND ---");
            }
        }
    }
}

/// Deep inspection of each police-only behaviour and
/// IdleBehaviour: full method list + all readable fields on a
/// live instance. This is the documentation run.
#[test]
fn behaviour_deep_inspect() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let targets = [
        "ScheduleOne.NPCs.Behaviour.FootPatrolBehaviour",
        "ScheduleOne.NPCs.Behaviour.VehiclePatrolBehaviour",
        "ScheduleOne.NPCs.Behaviour.SentryBehaviour",
        "ScheduleOne.NPCs.Behaviour.PursuitBehaviour",
        "ScheduleOne.NPCs.Behaviour.CheckpointBehaviour",
        "ScheduleOne.NPCs.Behaviour.BodySearchBehaviour",
        "ScheduleOne.NPCs.Behaviour.IdleBehaviour",
        "ScheduleOne.NPCs.Behaviour.StationaryBehaviour",
        "ScheduleOne.Combat.CombatBehaviour",
    ];

    for class in &targets {
        let short = class.rsplit('.').next().unwrap_or(class);
        println!("\n============================================================");
        println!("=== {short}: declared methods ===");
        print_declared_methods(&api, class);

        println!("\n=== {short}: all methods (incl inherited) ===");
        let r = api.op(
            "list_methods",
            json!({"class": class}),
        );
        if r.ok {
            let methods = r.result["methods"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            println!("  {} total methods", methods.len());
            for m in &methods {
                let declared = m["declared_on"]
                    .as_str()
                    .unwrap_or("?");
                // Skip the NetworkBehaviour boilerplate
                if declared.contains("FishNet")
                    || declared.contains("UnityEngine")
                    || declared == "System.Object"
                {
                    continue;
                }
                println!(
                    "  {}({}) -> {} [from: {}]{}",
                    m["name"].as_str().unwrap_or("?"),
                    m["params"].as_i64().unwrap_or(-1),
                    m["return"].as_str().unwrap_or("?"),
                    declared,
                    if m["static"].as_bool() == Some(true) {
                        " [static]"
                    } else {
                        ""
                    },
                );
            }
        } else {
            println!("  list_methods failed: {:?}", r.error);
        }

        // Walk instances, inspect first one with full field dump
        println!("\n=== {short}: live instance fields ===");
        let instances = walk(&api, class);
        match instances {
            Some(list) if !list.is_empty() => {
                println!("  {} instance(s)", list.len());
                if let Some(first) = list.first() {
                    if let Some(fh) = first["handle"].as_i64() {
                        let inspect = api.op(
                            "inspect_object",
                            json!({"handle": fh}),
                        );
                        if let Some(fields) =
                            inspect.result["fields"].as_object()
                        {
                            for (k, v) in fields {
                                // Skip FishNet/Unity noise
                                if k.starts_with("_rpc")
                                    || k.starts_with("_sync")
                                    || k.starts_with("_buffered")
                                    || k.starts_with("_lastReceived")
                                    || k.starts_with("_lastReplicate")
                                    || k.starts_with("_lastSent")
                                    || k.starts_with("_lastRecon")
                                    || k.starts_with("_lastMayChange")
                                    || k.starts_with("_networkConnection")
                                    || k.starts_with("_networkObject")
                                    || k.starts_with("_observersRpc")
                                    || k.starts_with("_serverRpc")
                                    || k.starts_with("_targetRpc")
                                    || k.starts_with("_reconcileRpc")
                                    || k.starts_with("_replicateRpc")
                                    || k.starts_with("_rpcHash")
                                    || k.starts_with("_rpcLinks")
                                    || k.starts_with("_rpcMethod")
                                    || k.starts_with("_transport")
                                    || k.starts_with("_onStart")
                                    || k.starts_with("_onStop")
                                    || k.starts_with("_prediction")
                                    || k.starts_with("_initializedOnce")
                                    || k.starts_with("_remaining")
                                    || k.starts_with("_syncType")
                                    || k.starts_with("_syncVar")
                                    || k.starts_with("_componentIndex")
                                    || k == "m_CachedPtr"
                                    || k == "isWrapped"
                                    || k == "m_CancellationTokenSource"
                                {
                                    continue;
                                }
                                // Compact print
                                let val_str = if v.is_string() {
                                    format!("\"{}\"", v.as_str().unwrap())
                                } else if v.is_object() {
                                    if let Some(t) = v.get("il2cpp_type") {
                                        format!(
                                            "<{}>",
                                            t.as_str().unwrap_or("?")
                                        )
                                    } else {
                                        v.to_string()
                                    }
                                } else {
                                    v.to_string()
                                };
                                println!("  {k} = {val_str}");
                            }
                        }
                        api.op(
                            "release_handle",
                            json!({"handle": fh}),
                        );
                    }
                }
                for inst in list.iter().skip(1) {
                    if let Some(h) = inst["handle"].as_i64() {
                        api.op(
                            "release_handle",
                            json!({"handle": h}),
                        );
                    }
                }
            }
            _ => {
                println!("  no live instances or type not found");
            }
        }
    }
}
