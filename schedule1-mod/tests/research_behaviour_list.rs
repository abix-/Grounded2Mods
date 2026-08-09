//! Compare behaviour stacks: vanilla cartel goon vs custom goon.
//! Find the difference that makes vanilla goons fight back.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_behaviour_list. --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, handle_of, ping_or_skip, player_position, walk};
use serde_json::json;

const FACTORY: &str = "Unityforge.Shim.Schedule1.NpcFactory";

fn factory_call(
    api: &modforge::client::Api<serde_json::Value>,
    method: &str,
    args: serde_json::Value,
) -> Option<serde_json::Value> {
    let r = api.op("invoke_static", json!({"class": FACTORY, "method": method, "args": args}));
    if !r.ok {
        println!("{method}: op failed: {:?}", r.error);
        return None;
    }
    let s = match r.result.as_str() {
        Some(s) => s,
        None => {
            println!("{method}: result not a string: {}", r.result);
            return None;
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            println!("{method}: bad json: {e}: {s}");
            return None;
        }
    };
    if parsed["ok"].as_bool() != Some(true) {
        println!("{method}: not ok: {}", parsed["error"]);
        return None;
    }
    Some(parsed)
}

fn dump_behaviour_stack(api: &modforge::client::Api<serde_json::Value>, handle: i64, label: &str) {
    let beh_r = api.op("read_field", json!({"handle": handle, "field": "behaviour"}));
    if !beh_r.ok {
        let beh_r2 = api.op("read_field", json!({"handle": handle, "field": "Behaviour"}));
        if !beh_r2.ok {
            println!("  {label}: cannot read behaviour field");
            return;
        }
        dump_npcbehaviour(api, &beh_r2.result, label);
        return;
    }
    dump_npcbehaviour(api, &beh_r.result, label);
}

fn dump_npcbehaviour(api: &modforge::client::Api<serde_json::Value>, beh_val: &serde_json::Value, label: &str) {
    let Some(bh) = handle_of(beh_val) else {
        println!("  {label}: behaviour has no handle");
        return;
    };

    // Read behaviourStack
    let stack_r = api.op("read_field", json!({"handle": bh, "field": "behaviourStack"}));
    if !stack_r.ok {
        println!("  {label}: cannot read behaviourStack");
        return;
    }
    let Some(sh) = handle_of(&stack_r.result) else {
        println!("  {label}: behaviourStack has no handle");
        return;
    };

    let count_r = api.op("invoke_method", json!({"handle": sh, "method": "get_Count", "args": []}));
    let count = count_r.result.as_i64().unwrap_or(0);
    println!("\n  === {label}: {count} behaviours in stack ===");

    for i in 0..count {
        let item_r = api.op("invoke_method", json!({"handle": sh, "method": "get_Item", "args": [i]}));
        if !item_r.ok { continue; }
        let Some(ih) = handle_of(&item_r.result) else { continue; };

        let type_r = api.op("invoke_method", json!({"handle": ih, "method": "GetType", "args": []}));
        let type_name = if type_r.ok {
            if let Some(th) = handle_of(&type_r.result) {
                let name_r = api.op("invoke_method", json!({"handle": th, "method": "get_Name", "args": []}));
                let n = name_r.result.as_str()
                    .or_else(|| name_r.result.get("str").and_then(|s| s.as_str()))
                    .unwrap_or("?").to_string();
                api.op("release_handle", json!({"handle": th}));
                n
            } else { "?".to_string() }
        } else { "?".to_string() };

        // Read Priority, Active, Enabled fields
        let pri_r = api.op("read_field", json!({"handle": ih, "field": "Priority"}));
        let pri = pri_r.result.as_i64().unwrap_or(-1);

        let active_r = api.op("read_field", json!({"handle": ih, "field": "Active"}));
        let active = active_r.result.as_bool().unwrap_or(false);

        let enabled_r = api.op("read_field", json!({"handle": ih, "field": "Enabled"}));
        let enabled = enabled_r.result.as_bool().unwrap_or(false);

        println!("  [{i:2}] {type_name:<40} pri={pri:4}  active={active:<5}  enabled={enabled}");

        api.op("release_handle", json!({"handle": ih}));
    }

    // Also read enabledBehaviours and activeBehaviour
    let enabled_r = api.op("read_field", json!({"handle": bh, "field": "enabledBehaviours"}));
    if let Some(eh) = handle_of(&enabled_r.result) {
        let ec = api.op("invoke_method", json!({"handle": eh, "method": "get_Count", "args": []}));
        println!("  enabledBehaviours count: {}", ec.result.as_i64().unwrap_or(0));
        api.op("release_handle", json!({"handle": eh}));
    }

    let active_r = api.op("read_field", json!({"handle": bh, "field": "activeBehaviour"}));
    if let Some(ah) = handle_of(&active_r.result) {
        let at = api.op("invoke_method", json!({"handle": ah, "method": "GetType", "args": []}));
        if let Some(th) = handle_of(&at.result) {
            let n = api.op("invoke_method", json!({"handle": th, "method": "get_Name", "args": []}));
            let name = n.result.as_str()
                .or_else(|| n.result.get("str").and_then(|s| s.as_str()))
                .unwrap_or("?");
            println!("  activeBehaviour: {name}");
            api.op("release_handle", json!({"handle": th}));
        }
        api.op("release_handle", json!({"handle": ah}));
    } else {
        println!("  activeBehaviour: null");
    }

    api.op("release_handle", json!({"handle": sh}));
    api.op("release_handle", json!({"handle": bh}));
}

#[test]
fn compare_vanilla_vs_custom() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Find a vanilla cartel goon
    println!("=== VANILLA CARTEL GOON ===");
    let goons = walk(&api, "ScheduleOne.Levelling.CartelGoon");
    match goons {
        Some(list) if !list.is_empty() => {
            println!("found {} vanilla cartel goons", list.len());
            if let Some(first) = list.first() {
                if let Some(gh) = first["handle"].as_i64() {
                    dump_behaviour_stack(&api, gh, "vanilla cartel goon");
                    api.op("release_handle", json!({"handle": gh}));
                }
            }
            for g in list.iter().skip(1) {
                if let Some(h) = g["handle"].as_i64() {
                    api.op("release_handle", json!({"handle": h}));
                }
            }
        }
        _ => println!("no vanilla cartel goons found in the world"),
    }

    // Find a vanilla police officer
    println!("\n=== VANILLA POLICE OFFICER ===");
    let police = walk(&api, "ScheduleOne.Law.PoliceOfficer");
    match police {
        Some(list) if !list.is_empty() => {
            println!("found {} vanilla police officers", list.len());
            if let Some(first) = list.first() {
                if let Some(ph) = first["handle"].as_i64() {
                    dump_behaviour_stack(&api, ph, "vanilla police");
                    api.op("release_handle", json!({"handle": ph}));
                }
            }
            for p in list.iter().skip(1) {
                if let Some(h) = p["handle"].as_i64() {
                    api.op("release_handle", json!({"handle": h}));
                }
            }
        }
        _ => println!("no vanilla police found"),
    }

    // Spawn a custom goon and dump its stack
    println!("\n=== CUSTOM GOON (freshly spawned) ===");
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position");
        return;
    };
    let spawn = factory_call(&api, "SpawnGoon", json!([px + 5.0, py, pz + 5.0]));
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);
    println!("spawned custom goon index={idx}");
    println!("waiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // Use GetBehaviourState for the custom goon (it goes through S1API)
    let state = factory_call(&api, "GetBehaviourState", json!([idx]));
    if let Some(v) = &state {
        if let Some(stack) = v["stack"].as_array() {
            println!("\n  === custom goon: {} behaviours in stack ===", stack.len());
            for (i, b) in stack.iter().enumerate() {
                let ty = b["type"].as_str().unwrap_or("?");
                let pri = b["pri"].as_i64().unwrap_or(-1);
                let active = b["active"].as_bool().unwrap_or(false);
                let enabled = b["enabled"].as_bool().unwrap_or(false);
                println!("  [{i:2}] {ty:<40} pri={pri:4}  active={active:<5}  enabled={enabled}");
            }
            println!("  enabledBehaviours count: {}", v["enabled_count"]);
            println!("  activeBehaviour: {}", v["active_type"]);
        }
    }
}
