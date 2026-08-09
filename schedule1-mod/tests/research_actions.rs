//! Compare component types between vanilla cartel goon and custom goon.
//! Check Actions, Responses, Awareness, and any other components that
//! could explain why vanilla goons fight back but custom goons do not.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_actions. --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, handle_of, ping_or_skip};
use serde_json::json;

fn get_full_type(api: &modforge::client::Api<serde_json::Value>, handle: i64) -> String {
    let type_r = api.op(
        "invoke_method",
        json!({"handle": handle, "method": "GetType", "args": []}),
    );
    if !type_r.ok {
        return format!("? (failed: {:?})", type_r.error);
    }
    if let Some(s) = type_r.result.as_str() {
        return s.to_string();
    }
    if let Some(s) = type_r.result.get("str").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    let Some(th) = handle_of(&type_r.result) else {
        return format!("? (result: {})", type_r.result);
    };
    for method in ["get_FullName", "get_Name", "ToString"] {
        let r = api.op("invoke_method", json!({"handle": th, "method": method, "args": []}));
        if r.ok {
            if let Some(s) = r.result.as_str() {
                api.op("release_handle", json!({"handle": th}));
                return s.to_string();
            }
            if let Some(s) = r.result.get("str").and_then(|v| v.as_str()) {
                api.op("release_handle", json!({"handle": th}));
                return s.to_string();
            }
        }
    }
    api.op("release_handle", json!({"handle": th}));
    "?".to_string()
}

fn dump_npc_components(api: &modforge::client::Api<serde_json::Value>, handle: i64, label: &str) {
    println!("\n  === {label} ===");
    println!("  NPC type: {}", get_full_type(api, handle));

    for field in ["Responses", "Actions", "Awareness", "Behaviour", "Health"] {
        let r = api.op("read_field", json!({"handle": handle, "field": field}));
        if r.ok {
            if let Some(fh) = handle_of(&r.result) {
                let ft = get_full_type(api, fh);
                println!("  {field}: {ft}");
                api.op("release_handle", json!({"handle": fh}));
            } else {
                println!("  {field}: null");
            }
        } else {
            println!("  {field}: (field not found)");
        }
    }
}

#[test]
fn compare_components() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Vanilla cartel goon from GoonPool
    println!("=== VANILLA CARTEL GOON ===");
    let pool = api.op(
        "walk_class",
        json!({"class": "Il2CppScheduleOne.Cartel.GoonPool"}),
    );
    if pool.ok {
        let instances = pool.result.as_array().cloned().unwrap_or_default();
        if let Some(first) = instances.first() {
            if let Some(ph) = first["handle"].as_i64() {
                let goons_r = api.op("read_field", json!({"handle": ph, "field": "goons"}));
                if goons_r.ok {
                    if let Some(gh) = handle_of(&goons_r.result) {
                        let count_r = api.op(
                            "invoke_method",
                            json!({"handle": gh, "method": "get_Count", "args": []}),
                        );
                        let count = count_r.result.as_i64().unwrap_or(0);
                        if count > 0 {
                            let item_r = api.op(
                                "invoke_method",
                                json!({"handle": gh, "method": "get_Item", "args": [0]}),
                            );
                            if item_r.ok {
                                if let Some(goon_h) = handle_of(&item_r.result) {
                                    dump_npc_components(&api, goon_h, "vanilla goon");
                                    api.op("release_handle", json!({"handle": goon_h}));
                                }
                            }
                        }
                        api.op("release_handle", json!({"handle": gh}));
                    }
                }
                api.op("release_handle", json!({"handle": ph}));
            }
        }
    }

    // Custom goon (most recently spawned, index 14)
    println!("\n=== CUSTOM GOON ===");
    let r = api.op(
        "invoke_static",
        json!({
            "class": "Unityforge.Shim.Schedule1.NpcFactory",
            "method": "SpawnGoon",
            "args": [0.0, 100.0, 0.0]
        }),
    );
    if r.ok {
        let s = r.result.as_str().unwrap_or("");
        let parsed: serde_json::Value = serde_json::from_str(s).unwrap_or_default();
        if parsed["ok"].as_bool() == Some(true) {
            let idx = parsed["index"].as_i64().unwrap_or(0);
            println!("  spawned index={idx}, waiting 10s...");
            std::thread::sleep(std::time::Duration::from_secs(10));

            // Get the S1NPC handle to read fields directly
            // Use GetS1NpcHandle shim method if available, or walk NPCs
            // For now, walk all NPCs and find the newest one
            let npcs = api.op(
                "walk_class",
                json!({"class": "Il2CppScheduleOne.NPCs.NPC"}),
            );
            if npcs.ok {
                let list = npcs.result.as_array().cloned().unwrap_or_default();
                println!("  {} total NPCs in world", list.len());
                // The custom goon should be the last one
                if let Some(last) = list.last() {
                    if let Some(nh) = last["handle"].as_i64() {
                        dump_npc_components(&api, nh, "custom goon (last NPC)");
                        api.op("release_handle", json!({"handle": nh}));
                    }
                }
                // Release all other handles
                for npc in list.iter().rev().skip(1) {
                    if let Some(h) = npc["handle"].as_i64() {
                        api.op("release_handle", json!({"handle": h}));
                    }
                }
            }
        }
    }
}
