//! Compare the NPCResponses type on vanilla cartel goons vs custom goons.
//! The hypothesis: vanilla goons use a different NPCResponses subclass
//! that triggers combat on damage, while S1API custom goons use
//! NPCResponses_Civilian which triggers flee/cower/call police.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_responses --test-threads=1. --nocapture
//! ```

mod common;
use common::{api, handle_of, ping_or_skip, print_declared_methods};
use serde_json::json;

fn get_type_name(api: &modforge::client::Api<serde_json::Value>, handle: i64) -> String {
    let type_r = api.op(
        "invoke_method",
        json!({"handle": handle, "method": "GetType", "args": []}),
    );
    if !type_r.ok {
        return format!("? (GetType failed: {:?})", type_r.error);
    }
    // The result might be a string directly or a handle
    if let Some(s) = type_r.result.as_str() {
        return s.to_string();
    }
    if let Some(s) = type_r.result.get("str").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    let Some(th) = handle_of(&type_r.result) else {
        return format!("? (type result: {})", type_r.result);
    };
    // Try get_FullName, then get_Name
    for method in ["get_FullName", "get_Name", "ToString"] {
        let r = api.op(
            "invoke_method",
            json!({"handle": th, "method": method, "args": []}),
        );
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
    "? (no name resolved)".to_string()
}

fn dump_responses_type(
    api: &modforge::client::Api<serde_json::Value>,
    npc_handle: i64,
    label: &str,
) {
    // Read the Responses field on the NPC
    let resp_r = api.op(
        "read_field",
        json!({"handle": npc_handle, "field": "Responses"}),
    );
    if !resp_r.ok {
        println!("  {label}: cannot read Responses field: {:?}", resp_r.error);
        return;
    }
    let Some(rh) = handle_of(&resp_r.result) else {
        println!("  {label}: Responses is null");
        return;
    };

    let type_name = get_type_name(api, rh);
    println!("  {label} Responses type: {type_name}");

    // Also check Awareness.Responses to see if it matches
    let aware_r = api.op(
        "read_field",
        json!({"handle": npc_handle, "field": "Awareness"}),
    );
    if aware_r.ok {
        if let Some(ah) = handle_of(&aware_r.result) {
            let aware_resp = api.op(
                "read_field",
                json!({"handle": ah, "field": "Responses"}),
            );
            if aware_resp.ok {
                if let Some(arh) = handle_of(&aware_resp.result) {
                    let aware_type = get_type_name(api, arh);
                    println!("  {label} Awareness.Responses type: {aware_type}");
                    api.op("release_handle", json!({"handle": arh}));
                } else {
                    println!("  {label} Awareness.Responses: null");
                }
            }
            api.op("release_handle", json!({"handle": ah}));
        }
    }

    api.op("release_handle", json!({"handle": rh}));
}

#[test]
fn compare_responses_type() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Get a vanilla cartel goon from the GoonPool
    println!("=== VANILLA CARTEL GOON (from GoonPool) ===");
    let pool = api.op(
        "walk_class",
        json!({"class": "Il2CppScheduleOne.Cartel.GoonPool"}),
    );
    if pool.ok {
        let instances = pool.result.as_array().cloned().unwrap_or_default();
        if let Some(first) = instances.first() {
            if let Some(ph) = first["handle"].as_i64() {
                // Read goons list
                let goons_r = api.op(
                    "read_field",
                    json!({"handle": ph, "field": "goons"}),
                );
                if goons_r.ok {
                    if let Some(gh) = handle_of(&goons_r.result) {
                        let count_r = api.op(
                            "invoke_method",
                            json!({"handle": gh, "method": "get_Count", "args": []}),
                        );
                        let count = count_r.result.as_i64().unwrap_or(0);
                        println!("  GoonPool has {count} goons");

                        if count > 0 {
                            // Get first goon
                            let item_r = api.op(
                                "invoke_method",
                                json!({"handle": gh, "method": "get_Item", "args": [0]}),
                            );
                            if item_r.ok {
                                if let Some(goon_h) = handle_of(&item_r.result) {
                                    // What type is the goon itself?
                                    let goon_type = get_type_name(&api, goon_h);
                                    println!("  vanilla goon type: {goon_type}");

                                    dump_responses_type(&api, goon_h, "vanilla goon");
                                    api.op("release_handle", json!({"handle": goon_h}));
                                }
                            }
                        }
                        api.op("release_handle", json!({"handle": gh}));
                    }
                } else {
                    // Try Goons (capitalized)
                    let goons_r2 = api.op(
                        "read_field",
                        json!({"handle": ph, "field": "Goons"}),
                    );
                    if goons_r2.ok {
                        if let Some(gh) = handle_of(&goons_r2.result) {
                            let count_r = api.op(
                                "invoke_method",
                                json!({"handle": gh, "method": "get_Count", "args": []}),
                            );
                            let count = count_r.result.as_i64().unwrap_or(0);
                            println!("  GoonPool has {count} goons (Goons field)");

                            if count > 0 {
                                let item_r = api.op(
                                    "invoke_method",
                                    json!({"handle": gh, "method": "get_Item", "args": [0]}),
                                );
                                if item_r.ok {
                                    if let Some(goon_h) = handle_of(&item_r.result) {
                                        let goon_type = get_type_name(&api, goon_h);
                                        println!("  vanilla goon type: {goon_type}");

                                        dump_responses_type(&api, goon_h, "vanilla goon");
                                        api.op("release_handle", json!({"handle": goon_h}));
                                    }
                                }
                            }
                            api.op("release_handle", json!({"handle": gh}));
                        }
                    } else {
                        println!("  cannot read goons/Goons field");
                    }
                }
                api.op("release_handle", json!({"handle": ph}));
            }
        }
    } else {
        println!("  GoonPool not found");
    }

    // Now spawn a custom goon and check its responses type
    println!("\n=== CUSTOM GOON (S1API) ===");
    let spawn = api.op(
        "invoke_static",
        json!({
            "class": "Unityforge.Shim.Schedule1.NpcFactory",
            "method": "SpawnGoon",
            "args": [0.0, 100.0, 0.0]
        }),
    );
    if spawn.ok {
        let s = spawn.result.as_str().unwrap_or("");
        let parsed: serde_json::Value =
            serde_json::from_str(s).unwrap_or(serde_json::Value::Null);
        if parsed["ok"].as_bool() == Some(true) {
            let idx = parsed["index"].as_i64().unwrap_or(0);
            println!("  spawned custom goon index={idx}");
            println!("  waiting 5s for spawn pipeline...");
            std::thread::sleep(std::time::Duration::from_secs(5));

            // Get the NPC handle through GetBehaviourState which returns the handle
            let state = api.op(
                "invoke_static",
                json!({
                    "class": "Unityforge.Shim.Schedule1.NpcFactory",
                    "method": "GetBehaviourState",
                    "args": [idx]
                }),
            );
            if state.ok {
                let ss = state.result.as_str().unwrap_or("");
                let sv: serde_json::Value =
                    serde_json::from_str(ss).unwrap_or(serde_json::Value::Null);
                println!("  custom goon state: {}", sv);
            }

            // To get the NPC handle, walk all S1API NPCs and find ours
            // Actually, use the Minted list from NpcFactory
            let list = api.op(
                "invoke_static",
                json!({
                    "class": "Unityforge.Shim.Schedule1.NpcFactory",
                    "method": "ListAll",
                    "args": []
                }),
            );
            if list.ok {
                let ls = list.result.as_str().unwrap_or("");
                let lv: serde_json::Value =
                    serde_json::from_str(ls).unwrap_or(serde_json::Value::Null);
                println!("  custom NPCs: {}", lv);
            }
        } else {
            println!("  SpawnGoon failed: {}", parsed["error"]);
        }
    } else {
        println!("  SpawnGoon invoke failed: {:?}", spawn.error);
    }
}

#[test]
fn list_response_class_methods() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== NPCResponses (base, used by vanilla cartel goons) ===");
    print_declared_methods(&api, "Il2CppScheduleOne.NPCs.Responses.NPCResponses");

    println!("\n=== NPCResponses_Civilian (used by S1API custom NPCs) ===");
    print_declared_methods(&api, "Il2CppScheduleOne.NPCs.Responses.NPCResponses_Civilian");
}
