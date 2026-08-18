//! Deep behaviour research: answer every open question from the
//! live game. No guessing.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_behaviours_deep -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, handle_of, ping_or_skip, walk, count_of};
use serde_json::json;

#[test]
fn sentry_location_and_route() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // What is SentryLocation?
    println!("=== SentryLocation instances ===");
    if let Some(list) = walk(&api, "ScheduleOne.Police.SentryLocation") {
        println!("{} SentryLocation(s)", list.len());
        for (i, inst) in list.iter().enumerate().take(3) {
            if let Some(h) = inst["handle"].as_i64() {
                let inspect = api.op("inspect_object", json!({"handle": h}));
                if let Some(fields) = inspect.result["fields"].as_object() {
                    println!("\n  SentryLocation[{i}]:");
                    for (k, v) in fields {
                        if k.starts_with("_rpc")
                            || k.starts_with("_sync")
                            || k.starts_with("_buffered")
                            || k.starts_with("_network")
                            || k.starts_with("_lastRec")
                            || k.starts_with("_lastRep")
                            || k.starts_with("_lastSent")
                            || k.starts_with("_lastMay")
                            || k.starts_with("_observers")
                            || k.starts_with("_server")
                            || k.starts_with("_target")
                            || k.starts_with("_reconcile")
                            || k.starts_with("_replicate")
                            || k.starts_with("_transport")
                            || k.starts_with("_onStart")
                            || k.starts_with("_onStop")
                            || k.starts_with("_prediction")
                            || k.starts_with("_initialized")
                            || k.starts_with("_remaining")
                            || k.starts_with("_component")
                            || k.starts_with("_rpcHash")
                            || k.starts_with("_rpcLinks")
                            || k.starts_with("_rpcMethod")
                            || k == "m_CachedPtr"
                            || k == "isWrapped"
                            || k == "m_CancellationTokenSource"
                            || k == "pooledPtr"
                        {
                            continue;
                        }
                        let val = compact_val(v);
                        println!("    {k} = {val}");
                    }
                }
                api.op("release_handle", json!({"handle": h}));
            }
        }
        for inst in list.iter().skip(3) {
            if let Some(h) = inst["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }
    } else {
        // Try other namespaces
        println!("  not found under ScheduleOne.Police");
        for ns in [
            "ScheduleOne.NPCs.Behaviour.SentryLocation",
            "ScheduleOne.Law.SentryLocation",
        ] {
            if let Some(list) = walk(&api, ns) {
                println!("  found under {ns}: {} instance(s)", list.len());
                for inst in &list {
                    if let Some(h) = inst["handle"].as_i64() {
                        api.op("release_handle", json!({"handle": h}));
                    }
                }
                break;
            }
        }
    }

    // What is SentryRoute?
    println!("\n=== SentryRoute instances ===");
    for ns in [
        "ScheduleOne.Police.SentryRoute",
        "ScheduleOne.NPCs.Behaviour.SentryRoute",
        "ScheduleOne.Law.SentryRoute",
    ] {
        if let Some(list) = walk(&api, ns) {
            println!("  found under {ns}: {} instance(s)", list.len());
            for (i, inst) in list.iter().enumerate().take(2) {
                if let Some(h) = inst["handle"].as_i64() {
                    let inspect = api.op("inspect_object", json!({"handle": h}));
                    if let Some(fields) = inspect.result["fields"].as_object() {
                        println!("\n  SentryRoute[{i}]:");
                        for (k, v) in fields {
                            if skip_noise(k) { continue; }
                            println!("    {k} = {}", compact_val(v));
                        }
                    }
                    api.op("release_handle", json!({"handle": h}));
                }
            }
            for inst in list.iter().skip(2) {
                if let Some(h) = inst["handle"].as_i64() {
                    api.op("release_handle", json!({"handle": h}));
                }
            }
            break;
        }
    }
}

#[test]
fn npcbehaviour_slots() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // NPCBehaviour: what property slots exist for behaviours?
    // Inspect a live NPCBehaviour to see ALL fields that reference
    // behaviour types.
    println!("=== NPCBehaviour: full field catalog ===");
    if let Some(list) = walk(&api, "ScheduleOne.NPCs.Behaviour.NPCBehaviour") {
        println!("{} NPCBehaviour(s)", list.len());
        if let Some(first) = list.first() {
            if let Some(h) = first["handle"].as_i64() {
                let inspect = api.op("inspect_object", json!({"handle": h}));
                if let Some(fields) = inspect.result["fields"].as_object() {
                    println!("\n  Fields containing 'Behaviour' or behaviour-related:");
                    for (k, v) in fields {
                        // Show ALL fields that reference behaviour types
                        let val = compact_val(v);
                        if k.contains("Behaviour")
                            || k.contains("behaviour")
                            || k.contains("Combat")
                            || k.contains("Patrol")
                            || k.contains("Sentry")
                            || k.contains("Pursuit")
                            || k.contains("Checkpoint")
                            || k.contains("BodySearch")
                            || k.contains("Idle")
                            || k.contains("Stationary")
                            || k.contains("Schedule")
                            || k.contains("Flee")
                            || k.contains("Cowering")
                            || k.contains("Dead")
                            || k.contains("Unconscious")
                            || k.contains("FaceTarget")
                            || k.contains("RequestProduct")
                            || k.contains("ConsumeProduct")
                            || k.contains("CallPolice")
                            || k.contains("GenericDialogue")
                            || k.contains("HeavyFlinch")
                        {
                            println!("    {k} = {val}");
                        }
                    }
                    println!("\n  ALL remaining fields:");
                    for (k, v) in fields {
                        if skip_noise(k) { continue; }
                        if k.contains("Behaviour")
                            || k.contains("behaviour")
                            || k.contains("Combat")
                            || k.contains("Patrol")
                            || k.contains("Sentry")
                            || k.contains("Pursuit")
                            || k.contains("Checkpoint")
                            || k.contains("BodySearch")
                            || k.contains("Idle")
                            || k.contains("Stationary")
                            || k.contains("Schedule")
                            || k.contains("Flee")
                            || k.contains("Cowering")
                            || k.contains("Dead")
                            || k.contains("Unconscious")
                            || k.contains("FaceTarget")
                            || k.contains("RequestProduct")
                            || k.contains("ConsumeProduct")
                            || k.contains("CallPolice")
                            || k.contains("GenericDialogue")
                            || k.contains("HeavyFlinch")
                        {
                            continue;
                        }
                        println!("    {k} = {}", compact_val(v));
                    }
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
}

#[test]
fn idle_behaviour_owners() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Which NPCs carry IdleBehaviour? Read the Npc back-ref on
    // each of the 29 instances.
    println!("=== IdleBehaviour: which NPCs carry it ===");
    if let Some(list) = walk(&api, "ScheduleOne.NPCs.Behaviour.IdleBehaviour") {
        println!("{} IdleBehaviour(s)", list.len());
        for (i, inst) in list.iter().enumerate() {
            if let Some(h) = inst["handle"].as_i64() {
                // Read the Npc getter to get the owning NPC
                let npc = api.op("invoke_method",
                    json!({"handle": h, "method": "get_Npc", "args": []}));
                if npc.ok {
                    if let Some(nh) = handle_of(&npc.result) {
                        // Get the NPC's name
                        let name = api.op("invoke_method",
                            json!({"handle": nh, "method": "get_name", "args": []}));
                        let npc_name = name.result.as_str()
                            .or_else(|| name.result.get("str").and_then(|s| s.as_str()))
                            .unwrap_or("?");
                        // Get the NPC's type
                        let gt = api.op("invoke_method",
                            json!({"handle": nh, "method": "GetType", "args": []}));
                        let mut type_name = String::from("?");
                        if gt.ok {
                            if let Some(th) = handle_of(&gt.result) {
                                let tn = api.op("invoke_method",
                                    json!({"handle": th, "method": "get_FullName", "args": []}));
                                type_name = tn.result.as_str()
                                    .or_else(|| tn.result.get("str").and_then(|s| s.as_str()))
                                    .unwrap_or("?").to_string();
                                api.op("release_handle", json!({"handle": th}));
                            }
                        }
                        // Read Active and Enabled
                        let active = api.op("read_field",
                            json!({"handle": h, "field": "_Active_k__BackingField"}));
                        let enabled = api.op("read_field",
                            json!({"handle": h, "field": "_Enabled_k__BackingField"}));
                        println!("  [{i}] name={npc_name} type={type_name} active={} enabled={}",
                            active.result, enabled.result);
                        api.op("release_handle", json!({"handle": nh}));
                    }
                } else {
                    println!("  [{i}] get_Npc failed: {:?}", npc.error);
                }
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }
}

#[test]
fn foot_patrol_chain_live() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Follow the full chain on a live FootPatrolBehaviour:
    // FootPatrolBehaviour -> Group -> PatrolGroup -> Route -> FootPatrolRoute
    println!("=== FootPatrolBehaviour: follow the live chain ===");
    if let Some(list) = walk(&api, "ScheduleOne.NPCs.Behaviour.FootPatrolBehaviour") {
        // Find one that is Active
        for (i, inst) in list.iter().enumerate() {
            if let Some(h) = inst["handle"].as_i64() {
                let active = api.op("read_field",
                    json!({"handle": h, "field": "_Active_k__BackingField"}));
                let enabled = api.op("read_field",
                    json!({"handle": h, "field": "_Enabled_k__BackingField"}));
                let a = active.result.as_bool().unwrap_or(false);
                let e = enabled.result.as_bool().unwrap_or(false);

                // Get owning NPC name
                let npc = api.op("invoke_method",
                    json!({"handle": h, "method": "get_Npc", "args": []}));
                let mut npc_name = String::from("?");
                if npc.ok {
                    if let Some(nh) = handle_of(&npc.result) {
                        let name = api.op("invoke_method",
                            json!({"handle": nh, "method": "get_name", "args": []}));
                        npc_name = name.result.as_str()
                            .or_else(|| name.result.get("str").and_then(|s| s.as_str()))
                            .unwrap_or("?").to_string();
                        api.op("release_handle", json!({"handle": nh}));
                    }
                }

                println!("\n  fpb[{i}] npc={npc_name} active={a} enabled={e}");

                // Follow Group -> PatrolGroup
                let group = api.op("read_field",
                    json!({"handle": h, "field": "_Group_k__BackingField"}));
                if let Some(gh) = handle_of(&group.result) {
                    println!("    Group handle={gh}");
                    let inspect = api.op("inspect_object", json!({"handle": gh}));
                    if let Some(fields) = inspect.result["fields"].as_object() {
                        for (k, v) in fields {
                            if skip_noise(k) { continue; }
                            println!("      {k} = {}", compact_val(v));
                        }
                    }

                    // Follow Route -> FootPatrolRoute
                    let route = api.op("read_field",
                        json!({"handle": gh, "field": "Route"}));
                    if let Some(rh) = handle_of(&route.result) {
                        println!("    Route handle={rh}");
                        let rn = api.op("read_field",
                            json!({"handle": rh, "field": "RouteName"}));
                        println!("      RouteName = {}", rn.result);

                        let wp = api.op("read_field",
                            json!({"handle": rh, "field": "Waypoints"}));
                        if let Some(wh) = handle_of(&wp.result) {
                            if let Some(n) = count_of(&api, wh) {
                                println!("      Waypoints = {n}");
                            }
                            api.op("release_handle", json!({"handle": wh}));
                        }
                        api.op("release_handle", json!({"handle": rh}));
                    } else {
                        println!("    Route = null");
                    }

                    // Read Members
                    let members = api.op("read_field",
                        json!({"handle": gh, "field": "Members"}));
                    if let Some(mh) = handle_of(&members.result) {
                        if let Some(n) = count_of(&api, mh) {
                            println!("    Members = {n}");
                            for j in 0..n {
                                let item = api.op("invoke_method",
                                    json!({"handle": mh, "method": "get_Item", "args": [j]}));
                                if item.ok {
                                    if let Some(ih) = handle_of(&item.result) {
                                        let mn = api.op("invoke_method",
                                            json!({"handle": ih, "method": "get_name", "args": []}));
                                        let mname = mn.result.as_str()
                                            .or_else(|| mn.result.get("str").and_then(|s| s.as_str()))
                                            .unwrap_or("?");
                                        println!("      member[{j}] = {mname}");
                                        api.op("release_handle", json!({"handle": ih}));
                                    }
                                }
                            }
                        }
                        api.op("release_handle", json!({"handle": mh}));
                    }

                    api.op("release_handle", json!({"handle": gh}));
                } else {
                    println!("    Group = null (no patrol group assigned)");
                }

                api.op("release_handle", json!({"handle": h}));
            }
        }
    }
}

#[test]
fn sentry_behaviour_live_state() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Read every SentryBehaviour instance: officer field, active
    // state, assigned location, stand point, current route
    println!("=== SentryBehaviour: all instances live state ===");
    if let Some(list) = walk(&api, "ScheduleOne.NPCs.Behaviour.SentryBehaviour") {
        println!("{} SentryBehaviour(s)", list.len());
        for (i, inst) in list.iter().enumerate() {
            if let Some(h) = inst["handle"].as_i64() {
                let active = api.op("read_field",
                    json!({"handle": h, "field": "_Active_k__BackingField"}));
                let enabled = api.op("read_field",
                    json!({"handle": h, "field": "_Enabled_k__BackingField"}));

                // officer field
                let officer = api.op("read_field",
                    json!({"handle": h, "field": "officer"}));
                let officer_val = compact_val(&officer.result);

                // assigned location
                let loc = api.op("read_field",
                    json!({"handle": h, "field": "_AssignedLocation_k__BackingField"}));
                let loc_val = compact_val(&loc.result);

                // stand point
                let sp = api.op("read_field",
                    json!({"handle": h, "field": "_standPoint"}));
                let sp_val = compact_val(&sp.result);

                // current route
                let cr = api.op("read_field",
                    json!({"handle": h, "field": "_currentRoute"}));
                let cr_val = compact_val(&cr.result);

                // route point index
                let rpi = api.op("read_field",
                    json!({"handle": h, "field": "_currentRoutePointIndex"}));
                // minutes at current point
                let macp = api.op("read_field",
                    json!({"handle": h, "field": "_minutesAtCurrentPoint"}));

                // owning NPC
                let npc = api.op("invoke_method",
                    json!({"handle": h, "method": "get_Npc", "args": []}));
                let mut npc_name = String::from("?");
                if npc.ok {
                    if let Some(nh) = handle_of(&npc.result) {
                        let name = api.op("invoke_method",
                            json!({"handle": nh, "method": "get_name", "args": []}));
                        npc_name = name.result.as_str()
                            .or_else(|| name.result.get("str").and_then(|s| s.as_str()))
                            .unwrap_or("?").to_string();
                        api.op("release_handle", json!({"handle": nh}));
                    }
                }

                println!("\n  sentry[{i}] npc={npc_name}");
                println!("    active={} enabled={}",
                    active.result, enabled.result);
                println!("    officer={officer_val}");
                println!("    assignedLocation={loc_val}");
                println!("    _standPoint={sp_val}");
                println!("    _currentRoute={cr_val}");
                println!("    _currentRoutePointIndex={}", rpi.result);
                println!("    _minutesAtCurrentPoint={}", macp.result);

                // If assigned location is not null, inspect it
                if let Some(lh) = handle_of(&loc.result) {
                    let loc_inspect = api.op("inspect_object",
                        json!({"handle": lh}));
                    if let Some(fields) = loc_inspect.result["fields"].as_object() {
                        println!("    SentryLocation fields:");
                        for (k, v) in fields {
                            if skip_noise(k) { continue; }
                            println!("      {k} = {}", compact_val(v));
                        }
                    }
                    api.op("release_handle", json!({"handle": lh}));
                }

                // If current route is not null, inspect it
                if let Some(crh) = handle_of(&cr.result) {
                    let cr_inspect = api.op("inspect_object",
                        json!({"handle": crh}));
                    if let Some(fields) = cr_inspect.result["fields"].as_object() {
                        println!("    SentryRoute fields:");
                        for (k, v) in fields {
                            if skip_noise(k) { continue; }
                            println!("      {k} = {}", compact_val(v));
                        }
                    }
                    api.op("release_handle", json!({"handle": crh}));
                }

                api.op("release_handle", json!({"handle": h}));
            }
        }
    }
}

#[test]
fn npcbehaviour_behaviour_list() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // NPCBehaviour has a Behaviours list (the priority stack).
    // Read it and dump every entry's type and priority.
    println!("=== NPCBehaviour: Behaviours list on a police NPC ===");
    if let Some(list) = walk(&api, "ScheduleOne.NPCs.Behaviour.NPCBehaviour") {
        // Read behaviourStack on the first NPCBehaviour (any NPC)
        // to see the full ordered list, then find a police officer's
        // NPCBehaviour by walking SentryBehaviour instances back to
        // their NPCBehaviour parent.
        if let Some(first) = list.first() {
            if let Some(h) = first["handle"].as_i64() {
                println!("  civilian/first NPCBehaviour:");
                dump_behaviour_stack(&api, h);
                api.op("release_handle", json!({"handle": h}));
            }
        }
        for inst in list.iter().skip(1) {
            if let Some(h) = inst["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }

        // Now find a police NPCBehaviour via SentryBehaviour.beh
        println!("\n=== NPCBehaviour: behaviour stack on a police NPC ===");
        if let Some(sentries) = walk(&api, "ScheduleOne.NPCs.Behaviour.SentryBehaviour") {
            if let Some(first_sentry) = sentries.first() {
                if let Some(sh) = first_sentry["handle"].as_i64() {
                    let beh = api.op("read_field",
                        json!({"handle": sh, "field": "_beh_k__BackingField"}));
                    if let Some(bh) = handle_of(&beh.result) {
                        println!("  police NPCBehaviour (via SentryBehaviour.beh):");
                        dump_behaviour_stack(&api, bh);
                        api.op("release_handle", json!({"handle": bh}));
                    }
                    api.op("release_handle", json!({"handle": sh}));
                }
            }
            for s in sentries.iter().skip(1) {
                if let Some(h) = s["handle"].as_i64() {
                    api.op("release_handle", json!({"handle": h}));
                }
            }
        }
    }
}

fn dump_behaviour_stack(api: &modforge::client::Api<serde_json::Value>, npcbeh_handle: i64) {
    // Read behaviourStack
    let stack = api.op("read_field",
        json!({"handle": npcbeh_handle, "field": "behaviourStack"}));
    if let Some(sh) = handle_of(&stack.result) {
        if let Some(n) = count_of(api, sh) {
            println!("    behaviourStack: {n} entries");
            for j in 0..n {
                let item = api.op("invoke_method",
                    json!({"handle": sh, "method": "get_Item", "args": [j]}));
                if item.ok {
                    if let Some(ih) = handle_of(&item.result) {
                        let insp = api.op("inspect_object",
                            json!({"handle": ih}));
                        let type_name = insp.result["type"]
                            .as_str()
                            .unwrap_or("?")
                            .to_string();
                        let pri = api.op("read_field",
                            json!({"handle": ih, "field": "Priority"}));
                        let act = api.op("read_field",
                            json!({"handle": ih, "field": "_Active_k__BackingField"}));
                        let ena = api.op("read_field",
                            json!({"handle": ih, "field": "_Enabled_k__BackingField"}));
                        let short = type_name.rsplit('.').next().unwrap_or(&type_name);
                        println!("    [{j}] {short} pri={} active={} enabled={}",
                            pri.result, act.result, ena.result);
                        api.op("release_handle", json!({"handle": ih}));
                    }
                }
            }
        }
        api.op("release_handle", json!({"handle": sh}));
    } else {
        println!("    behaviourStack not readable");
    }

    // Read enabledBehaviours
    let enabled = api.op("read_field",
        json!({"handle": npcbeh_handle, "field": "enabledBehaviours"}));
    if let Some(eh) = handle_of(&enabled.result) {
        if let Some(n) = count_of(api, eh) {
            println!("    enabledBehaviours: {n} entries");
            for j in 0..n {
                let item = api.op("invoke_method",
                    json!({"handle": eh, "method": "get_Item", "args": [j]}));
                if item.ok {
                    if let Some(ih) = handle_of(&item.result) {
                        let insp = api.op("inspect_object",
                            json!({"handle": ih}));
                        let type_name = insp.result["type"]
                            .as_str()
                            .unwrap_or("?")
                            .to_string();
                        let short = type_name.rsplit('.').next().unwrap_or(&type_name);
                        println!("    [{j}] {short}");
                        api.op("release_handle", json!({"handle": ih}));
                    }
                }
            }
        }
        api.op("release_handle", json!({"handle": eh}));
    }

    // Read activeBehaviour
    let active = api.op("read_field",
        json!({"handle": npcbeh_handle, "field": "_activeBehaviour_k__BackingField"}));
    if let Some(ah) = handle_of(&active.result) {
        let insp = api.op("inspect_object", json!({"handle": ah}));
        let type_name = insp.result["type"]
            .as_str()
            .unwrap_or("?")
            .to_string();
        let short = type_name.rsplit('.').next().unwrap_or(&type_name);
        println!("    activeBehaviour = {short}");
        api.op("release_handle", json!({"handle": ah}));
    } else {
        println!("    activeBehaviour = null");
    }
}

fn compact_val(v: &serde_json::Value) -> String {
    if v.is_string() {
        format!("\"{}\"", v.as_str().unwrap())
    } else if v.is_null() {
        "null".to_string()
    } else if v.is_object() {
        if let Some(t) = v.get("il2cpp_type") {
            format!("<{}>", t.as_str().unwrap_or("?"))
        } else if let Some(s) = v.get("str") {
            format!("\"{}\"", s.as_str().unwrap_or("?"))
        } else if let Some(val) = v.get("value") {
            format!("{val}")
        } else {
            v.to_string()
        }
    } else {
        v.to_string()
    }
}

fn skip_noise(k: &str) -> bool {
    k.starts_with("_rpc")
        || k.starts_with("_sync")
        || k.starts_with("_buffered")
        || k.starts_with("_network")
        || k.starts_with("_lastRec")
        || k.starts_with("_lastRep")
        || k.starts_with("_lastSent")
        || k.starts_with("_lastMay")
        || k.starts_with("_observers")
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
        || k.starts_with("_component")
        || k.starts_with("_syncType")
        || k.starts_with("_syncVar")
        || k == "m_CachedPtr"
        || k == "isWrapped"
        || k == "m_CancellationTokenSource"
        || k == "pooledPtr"
}
