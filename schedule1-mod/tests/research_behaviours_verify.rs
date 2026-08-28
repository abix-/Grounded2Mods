//! Verify hypotheses from the behaviour research. Each test
//! targets a specific unproven claim from certainty-tracking.md.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_behaviours_verify. --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, count_of, handle_of, ping_or_skip, walk};
use serde_json::json;

/// Hypothesis: "stack always falls back to lowest enabled
/// behaviour". Currently proven on one officer only.
/// This test checks enabledBehaviours and activeBehaviour on
/// multiple NPC types: civilians, employees, minted goons.
#[test]
fn stack_fallback_multi_npc() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== Stack fallback: check multiple NPCs ===");
    if let Some(list) = walk(&api, "ScheduleOne.NPCs.Behaviour.NPCBehaviour") {
        let check_count = list.len().min(20);
        println!(
            "{} NPCBehaviour(s), checking first {check_count}",
            list.len()
        );

        for (i, inst) in list.iter().enumerate().take(check_count) {
            if let Some(h) = inst["handle"].as_i64() {
                // Get owning NPC name via the npc field
                let npc_field = api.op(
                    "read_field",
                    json!({"handle": h, "field": "_npc_k__BackingField"}),
                );
                let mut npc_name = String::from("?");
                let mut npc_type = String::from("?");
                if let Some(nh) = handle_of(&npc_field.result) {
                    let name = api.op(
                        "invoke_method",
                        json!({"handle": nh, "method": "get_name", "args": []}),
                    );
                    npc_name = name
                        .result
                        .as_str()
                        .or_else(|| name.result.get("str").and_then(|s| s.as_str()))
                        .unwrap_or("?")
                        .to_string();
                    let gt = api.op(
                        "invoke_method",
                        json!({"handle": nh, "method": "GetType", "args": []}),
                    );
                    if gt.ok {
                        if let Some(th) = handle_of(&gt.result) {
                            let tn = api.op(
                                "invoke_method",
                                json!({"handle": th, "method": "get_FullName", "args": []}),
                            );
                            npc_type = tn
                                .result
                                .as_str()
                                .or_else(|| tn.result.get("str").and_then(|s| s.as_str()))
                                .unwrap_or("?")
                                .to_string();
                            api.op("release_handle", json!({"handle": th}));
                        }
                    }
                    api.op("release_handle", json!({"handle": nh}));
                }

                // Read enabledBehaviours count and activeBehaviour
                let enabled = api.op(
                    "read_field",
                    json!({"handle": h, "field": "enabledBehaviours"}),
                );
                let mut enabled_count: i64 = -1;
                let mut enabled_types = Vec::new();
                if let Some(eh) = handle_of(&enabled.result) {
                    if let Some(n) = count_of(&api, eh) {
                        enabled_count = n;
                        for j in 0..n.min(5) {
                            let item = api.op(
                                "invoke_method",
                                json!({"handle": eh, "method": "get_Item", "args": [j]}),
                            );
                            if item.ok {
                                if let Some(ih) = handle_of(&item.result) {
                                    let insp = api.op("inspect_object", json!({"handle": ih}));
                                    let tn =
                                        insp.result["type"].as_str().unwrap_or("?").to_string();
                                    let pri = api.op(
                                        "read_field",
                                        json!({"handle": ih, "field": "Priority"}),
                                    );
                                    let short = tn.rsplit('.').next().unwrap_or(&tn).to_string();
                                    enabled_types.push(format!("{}(pri={})", short, pri.result));
                                    api.op("release_handle", json!({"handle": ih}));
                                }
                            }
                        }
                    }
                    api.op("release_handle", json!({"handle": eh}));
                }

                let active = api.op(
                    "read_field",
                    json!({"handle": h, "field": "_activeBehaviour_k__BackingField"}),
                );
                let mut active_type = String::from("null");
                let mut active_pri = String::from("?");
                if let Some(ah) = handle_of(&active.result) {
                    let insp = api.op("inspect_object", json!({"handle": ah}));
                    let tn = insp.result["type"].as_str().unwrap_or("?").to_string();
                    active_type = tn.rsplit('.').next().unwrap_or(&tn).to_string();
                    let pri = api.op("read_field", json!({"handle": ah, "field": "Priority"}));
                    active_pri = pri.result.to_string();
                    api.op("release_handle", json!({"handle": ah}));
                }

                let type_short = npc_type.rsplit('.').next().unwrap_or(&npc_type);
                println!(
                    "  [{i}] {npc_name} ({type_short}) enabled={enabled_count} active={active_type}(pri={active_pri}) enabled_list=[{}]",
                    enabled_types.join(", ")
                );

                api.op("release_handle", json!({"handle": h}));
            }
        }
        for inst in list.iter().skip(check_count) {
            if let Some(h) = inst["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }
}

/// Hypothesis: "162 CombatBehaviour instances vs 152 NPCs because
/// PursuitBehaviour extends CombatBehaviour". Check the actual
/// runtime type of each CombatBehaviour instance.
#[test]
fn combat_behaviour_extra_instances() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== CombatBehaviour: check types of all instances ===");
    if let Some(list) = walk(&api, "ScheduleOne.Combat.CombatBehaviour") {
        println!("{} CombatBehaviour instances", list.len());

        let mut type_counts = std::collections::HashMap::new();
        for (i, inst) in list.iter().enumerate() {
            if let Some(h) = inst["handle"].as_i64() {
                let insp = api.op("inspect_object", json!({"handle": h}));
                let tn = insp.result["type"].as_str().unwrap_or("?").to_string();
                let short = tn.rsplit('.').next().unwrap_or(&tn).to_string();
                *type_counts.entry(short.clone()).or_insert(0i32) += 1;
                // Print first few and any non-CombatBehaviour
                if i < 3 || short != "CombatBehaviour" {
                    println!("  [{i}] {short}");
                }
                api.op("release_handle", json!({"handle": h}));
            }
        }
        println!("\n  Type breakdown:");
        for (t, c) in &type_counts {
            println!("    {t}: {c}");
        }
    }
}

/// Question: where does IdleBehaviour come from on minted NPCs?
/// Check if BaseEmployee prefab (before S1API) already has it.
/// Walk NetworkManager.SpawnablePrefabs, find BaseEmployee, check
/// its components for IdleBehaviour.
#[test]
fn idle_behaviour_source() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== IdleBehaviour source: check BaseEmployee prefab ===");

    // Walk NetworkManager to find SpawnablePrefabs
    if let Some(list) = walk(&api, "FishNet.Managing.NetworkManager") {
        if let Some(first) = list.first() {
            if let Some(h) = first["handle"].as_i64() {
                let prefabs = api.op(
                    "read_field",
                    json!({"handle": h, "field": "SpawnablePrefabs"}),
                );
                if let Some(ph) = handle_of(&prefabs.result) {
                    // SpawnablePrefabs has GetObjects
                    let objs = api.op(
                        "invoke_method",
                        json!({"handle": ph, "method": "GetObjects", "args": []}),
                    );
                    if objs.ok {
                        if let Some(oh) = handle_of(&objs.result) {
                            if let Some(n) = count_of(&api, oh) {
                                println!("  {n} spawnable prefabs");
                                for i in 0..n {
                                    let item = api.op(
                                        "invoke_method",
                                        json!({"handle": oh, "method": "get_Item", "args": [i]}),
                                    );
                                    if !item.ok {
                                        continue;
                                    }
                                    if let Some(ih) = handle_of(&item.result) {
                                        let name = api.op(
                                            "invoke_method",
                                            json!({"handle": ih, "method": "get_name", "args": []}),
                                        );
                                        let pname = name
                                            .result
                                            .as_str()
                                            .or_else(|| {
                                                name.result.get("str").and_then(|s| s.as_str())
                                            })
                                            .unwrap_or("?");
                                        if pname.contains("BaseEmployee")
                                            || pname.contains("Employee")
                                        {
                                            println!("  FOUND: {pname} at index {i}");
                                            // Get the prefab's GameObject
                                            let go = api.op("invoke_method",
                                                json!({"handle": ih, "method": "get_gameObject", "args": []}));
                                            if go.ok {
                                                if let Some(gh) = handle_of(&go.result) {
                                                    // GetComponentsInChildren<IdleBehaviour>
                                                    // Since methods are stripped, try
                                                    // GetComponentInChildren with type name
                                                    let comps = api.op("invoke_method",
                                                        json!({"handle": gh, "method": "GetComponents", "args": []}));
                                                    if comps.ok {
                                                        if let Some(ch) = handle_of(&comps.result) {
                                                            if let Some(cn) = count_of(&api, ch) {
                                                                println!(
                                                                    "  {cn} components on {pname}:"
                                                                );
                                                                for ci in 0..cn {
                                                                    let comp = api.op("invoke_method",
                                                                        json!({"handle": ch, "method": "get_Item", "args": [ci]}));
                                                                    if !comp.ok {
                                                                        continue;
                                                                    }
                                                                    if let Some(cih) =
                                                                        handle_of(&comp.result)
                                                                    {
                                                                        let insp = api.op(
                                                                            "inspect_object",
                                                                            json!({"handle": cih}),
                                                                        );
                                                                        let tn =
                                                                            insp.result["type"]
                                                                                .as_str()
                                                                                .unwrap_or("?");
                                                                        if tn.contains("Behaviour")
                                                                            || tn.contains(
                                                                                "Behavior",
                                                                            )
                                                                            || tn.contains("Idle")
                                                                        {
                                                                            println!(
                                                                                "    [{ci}] {tn}"
                                                                            );
                                                                        }
                                                                        api.op(
                                                                            "release_handle",
                                                                            json!({"handle": cih}),
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                            api.op(
                                                                "release_handle",
                                                                json!({"handle": ch}),
                                                            );
                                                        }
                                                    } else {
                                                        println!(
                                                            "  GetComponents failed: {:?}",
                                                            comps.error
                                                        );
                                                    }
                                                    api.op("release_handle", json!({"handle": gh}));
                                                }
                                            }
                                        }
                                        api.op("release_handle", json!({"handle": ih}));
                                    }
                                }
                            }
                            api.op("release_handle", json!({"handle": oh}));
                        }
                    } else {
                        println!("  GetObjects failed: {:?}", objs.error);
                    }
                    api.op("release_handle", json!({"handle": ph}));
                }
                api.op("release_handle", json!({"handle": h}));
            }
        }
        for inst in list.iter().skip(1) {
            if let Some(h) = inst["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }

    // Also check: does a LIVE minted NPC's IdleBehaviour have a
    // different parent GameObject than the NPCBehaviour? That
    // would prove it was added separately vs being on the prefab.
    println!("\n=== IdleBehaviour: check parent GameObject on minted NPC ===");
    if let Some(idles) = walk(&api, "ScheduleOne.NPCs.Behaviour.IdleBehaviour") {
        for (i, inst) in idles.iter().enumerate() {
            if let Some(h) = inst["handle"].as_i64() {
                // Get owning NPC name
                let npc = api.op(
                    "invoke_method",
                    json!({"handle": h, "method": "get_Npc", "args": []}),
                );
                let mut npc_name = String::from("?");
                if npc.ok {
                    if let Some(nh) = handle_of(&npc.result) {
                        let name = api.op(
                            "invoke_method",
                            json!({"handle": nh, "method": "get_name", "args": []}),
                        );
                        npc_name = name
                            .result
                            .as_str()
                            .or_else(|| name.result.get("str").and_then(|s| s.as_str()))
                            .unwrap_or("?")
                            .to_string();
                        api.op("release_handle", json!({"handle": nh}));
                    }
                } else {
                    // Skip orphaned instances
                    api.op("release_handle", json!({"handle": h}));
                    continue;
                }

                // Only check S1API minted NPCs
                if !npc_name.contains("S1API") {
                    api.op("release_handle", json!({"handle": h}));
                    continue;
                }

                // Get the IdleBehaviour's GameObject name
                let go = api.op(
                    "invoke_method",
                    json!({"handle": h, "method": "get_gameObject", "args": []}),
                );
                if go.ok {
                    if let Some(gh) = handle_of(&go.result) {
                        let gname = api.op(
                            "invoke_method",
                            json!({"handle": gh, "method": "get_name", "args": []}),
                        );
                        let go_name = gname
                            .result
                            .as_str()
                            .or_else(|| gname.result.get("str").and_then(|s| s.as_str()))
                            .unwrap_or("?");
                        // Get parent transform
                        let tf = api.op(
                            "invoke_method",
                            json!({"handle": gh, "method": "get_transform", "args": []}),
                        );
                        let mut parent_name = String::from("(root)");
                        if tf.ok {
                            if let Some(tfh) = handle_of(&tf.result) {
                                let parent = api.op(
                                    "invoke_method",
                                    json!({"handle": tfh, "method": "get_parent", "args": []}),
                                );
                                if parent.ok {
                                    if let Some(pth) = handle_of(&parent.result) {
                                        let pgo = api.op("invoke_method",
                                            json!({"handle": pth, "method": "get_gameObject", "args": []}));
                                        if pgo.ok {
                                            if let Some(pgh) = handle_of(&pgo.result) {
                                                let pn = api.op("invoke_method",
                                                    json!({"handle": pgh, "method": "get_name", "args": []}));
                                                parent_name = pn
                                                    .result
                                                    .as_str()
                                                    .or_else(|| {
                                                        pn.result
                                                            .get("str")
                                                            .and_then(|s| s.as_str())
                                                    })
                                                    .unwrap_or("?")
                                                    .to_string();
                                                api.op("release_handle", json!({"handle": pgh}));
                                            }
                                        }
                                        api.op("release_handle", json!({"handle": pth}));
                                    }
                                }
                                api.op("release_handle", json!({"handle": tfh}));
                            }
                        }
                        println!(
                            "  [{i}] NPC={npc_name} IdleBehaviour.gameObject={go_name} parent={parent_name}"
                        );
                        api.op("release_handle", json!({"handle": gh}));
                    }
                }
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }
}

/// Check priority resolution: when multiple behaviours are
/// enabled, does activeBehaviour always have the HIGHEST priority
/// number? Check across many NPCs.
#[test]
fn priority_resolution_direction() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== Priority resolution: highest number wins? ===");
    if let Some(list) = walk(&api, "ScheduleOne.NPCs.Behaviour.NPCBehaviour") {
        let check_count = list.len().min(30);
        println!("{} NPCBehaviour(s), checking {check_count}", list.len());

        let mut consistent = true;
        for (i, inst) in list.iter().enumerate().take(check_count) {
            if let Some(h) = inst["handle"].as_i64() {
                // Get enabled behaviours with priorities
                let enabled = api.op(
                    "read_field",
                    json!({"handle": h, "field": "enabledBehaviours"}),
                );
                let mut max_pri: Option<i64> = None;
                if let Some(eh) = handle_of(&enabled.result) {
                    if let Some(n) = count_of(&api, eh) {
                        for j in 0..n {
                            let item = api.op(
                                "invoke_method",
                                json!({"handle": eh, "method": "get_Item", "args": [j]}),
                            );
                            if item.ok {
                                if let Some(ih) = handle_of(&item.result) {
                                    let pri = api.op(
                                        "read_field",
                                        json!({"handle": ih, "field": "Priority"}),
                                    );
                                    if let Some(p) = pri.result.as_i64() {
                                        max_pri = Some(max_pri.map_or(p, |m: i64| m.max(p)));
                                    }
                                    api.op("release_handle", json!({"handle": ih}));
                                }
                            }
                        }
                    }
                    api.op("release_handle", json!({"handle": eh}));
                }

                // Get active behaviour priority
                let active = api.op(
                    "read_field",
                    json!({"handle": h, "field": "_activeBehaviour_k__BackingField"}),
                );
                let mut active_pri: Option<i64> = None;
                let mut active_name = String::from("null");
                if let Some(ah) = handle_of(&active.result) {
                    let pri = api.op("read_field", json!({"handle": ah, "field": "Priority"}));
                    active_pri = pri.result.as_i64();
                    let insp = api.op("inspect_object", json!({"handle": ah}));
                    active_name = insp.result["type"]
                        .as_str()
                        .unwrap_or("?")
                        .rsplit('.')
                        .next()
                        .unwrap_or("?")
                        .to_string();
                    api.op("release_handle", json!({"handle": ah}));
                }

                let matches = match (max_pri, active_pri) {
                    (Some(m), Some(a)) => m == a,
                    _ => true, // skip if we can't read
                };
                if !matches {
                    consistent = false;
                    println!(
                        "  [{i}] MISMATCH: max_enabled_pri={:?} active_pri={:?} active={}",
                        max_pri, active_pri, active_name
                    );
                }
                if i < 5 || !matches {
                    println!(
                        "  [{i}] max_enabled_pri={:?} active_pri={:?} active={}{}",
                        max_pri,
                        active_pri,
                        active_name,
                        if matches { "" } else { " *** MISMATCH ***" }
                    );
                }

                api.op("release_handle", json!({"handle": h}));
            }
        }
        if consistent {
            println!(
                "\n  RESULT: activeBehaviour always has the highest priority among enabledBehaviours (checked {check_count} NPCs)"
            );
        } else {
            println!(
                "\n  RESULT: INCONSISTENCY FOUND, highest-priority rule does NOT hold for all NPCs"
            );
        }

        for inst in list.iter().skip(check_count) {
            if let Some(h) = inst["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }
}
