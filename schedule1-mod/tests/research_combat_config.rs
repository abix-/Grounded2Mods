//! Compare combat configuration between a vanilla CartelGoon and
//! a custom (S1API) goon. Goal: find what vanilla goons have set
//! at spawn time that makes them fight back when attacked.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_combat_config -- --test-threads=1 --nocapture
//! ```

mod common;
#[allow(unused_imports)]
use common::{api, handle_of, ping_or_skip, player_position, print_declared_methods};
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

fn read_field_str(
    api: &modforge::client::Api<serde_json::Value>,
    handle: i64,
    field: &str,
) -> String {
    let r = api.op("read_field", json!({"handle": handle, "field": field}));
    if !r.ok {
        return format!("(read failed: {:?})", r.error);
    }
    format!("{}", r.result)
}

fn get_type_name(
    api: &modforge::client::Api<serde_json::Value>,
    handle: i64,
) -> String {
    let r = api.op("invoke_method", json!({"handle": handle, "method": "GetType", "args": []}));
    if !r.ok {
        return "(GetType failed)".into();
    }
    if let Some(s) = r.result.as_str() {
        return s.into();
    }
    if let Some(s) = r.result.get("str").and_then(|v| v.as_str()) {
        return s.into();
    }
    if let Some(h) = r.result.get("handle").and_then(|v| v.as_i64()) {
        let name = api.op("invoke_method", json!({"handle": h, "method": "get_FullName", "args": []}));
        if let Some(s) = name.result.as_str() {
            return s.into();
        }
    }
    format!("{}", r.result)
}

/// Read combat fields from an NPC handle and print them.
fn inspect_combat_fields(
    api: &modforge::client::Api<serde_json::Value>,
    label: &str,
    npc_h: i64,
) {
    println!("\n=== {label}: NPC-level fields ===");

    // NPC type
    let npc_type = get_type_name(api, npc_h);
    println!("  NPC type: {npc_type}");

    // Aggression
    println!("  Aggression: {}", read_field_str(api, npc_h, "Aggression"));

    // Key booleans
    for field in [
        "field_Private_Boolean_0",
        "field_Private_Boolean_1",
        "field_Private_Boolean_2",
    ] {
        println!("  {field}: {}", read_field_str(api, npc_h, field));
    }

    // Responses component
    let resp_r = api.op("read_field", json!({"handle": npc_h, "field": "Responses"}));
    if let Some(rh) = handle_of(&resp_r.result) {
        let resp_type = get_type_name(api, rh);
        println!("  Responses type: {resp_type}");

        // Inspect all fields on the Responses object
        let inspect = api.op("inspect_object", json!({"handle": rh}));
        if inspect.ok {
            if let Some(fields) = inspect.result["fields"].as_object() {
                println!("  Responses has {} fields:", fields.len());
                for (k, v) in fields {
                    let vs = format!("{v}");
                    if vs.len() > 120 {
                        println!("    {k} = {}...", &vs[..120]);
                    } else {
                        println!("    {k} = {vs}");
                    }
                }
            }
        }
        api.op("release_handle", json!({"handle": rh}));
    } else {
        println!("  Responses: null or unreadable");
    }

    // Awareness component
    let aware_r = api.op("read_field", json!({"handle": npc_h, "field": "awareness"}));
    if let Some(ah) = handle_of(&aware_r.result) {
        println!("\n=== {label}: Awareness fields ===");
        let inspect = api.op("inspect_object", json!({"handle": ah}));
        if inspect.ok {
            if let Some(fields) = inspect.result["fields"].as_object() {
                println!("  Awareness has {} fields:", fields.len());
                for (k, v) in fields {
                    let vs = format!("{v}");
                    if vs.len() > 120 {
                        println!("    {k} = {}...", &vs[..120]);
                    } else {
                        println!("    {k} = {vs}");
                    }
                }
            }
        }
        api.op("release_handle", json!({"handle": ah}));
    }

    // Behaviour component
    let beh_r = api.op("read_field", json!({"handle": npc_h, "field": "Behaviour"}));
    if let Some(bh) = handle_of(&beh_r.result) {
        println!("\n=== {label}: Behaviour fields ===");
        let inspect = api.op("inspect_object", json!({"handle": bh}));
        if inspect.ok {
            if let Some(fields) = inspect.result["fields"].as_object() {
                println!("  Behaviour has {} fields:", fields.len());
                for (k, v) in fields {
                    let vs = format!("{v}");
                    if vs.len() > 120 {
                        println!("    {k} = {}...", &vs[..120]);
                    } else {
                        println!("    {k} = {vs}");
                    }
                }
            }
        }
        api.op("release_handle", json!({"handle": bh}));
    }
}

#[test]
fn compare_combat_config() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // --- Vanilla CartelGoon ---
    let pool = api.op(
        "walk_class",
        json!({"class": "Il2CppScheduleOne.Cartel.GoonPool"}),
    );
    if !pool.ok {
        println!("GoonPool not found");
        return;
    }
    let instances = pool.result.as_array().cloned().unwrap_or_default();
    let ph = instances.first().and_then(|i| i["handle"].as_i64()).unwrap();
    let goons_r = api.op("read_field", json!({"handle": ph, "field": "goons"}));
    let gh = handle_of(&goons_r.result).unwrap();
    let item_r = api.op(
        "invoke_method",
        json!({"handle": gh, "method": "get_Item", "args": [0]}),
    );
    let vanilla_h = handle_of(&item_r.result).unwrap();
    inspect_combat_fields(&api, "VANILLA CartelGoon", vanilla_h);

    // --- Custom goon via S1API ---
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position");
        return;
    };
    let Some(spawn) = factory_call(&api, "SpawnGoon", json!([px + 5.0, py, pz + 5.0])) else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = spawn["index"].as_i64().unwrap_or(0);
    println!("\n\nspawned custom goon index={idx}, waiting 8s for settle...");
    std::thread::sleep(std::time::Duration::from_secs(8));

    // Inspect the custom goon's combat config via the shim
    println!("\n\n=== CUSTOM S1API goon: InspectCombatConfig ===");
    let custom_config = factory_call(&api, "InspectCombatConfig", json!([idx]));
    if let Some(cfg) = custom_config {
        println!("{}", serde_json::to_string_pretty(&cfg).unwrap_or_default());
    } else {
        println!("InspectCombatConfig failed");
    }

    // Also dump NPCResponses methods for reference
    println!("\n\n=== NPCResponses base class methods ===");
    print_declared_methods(&api, "Il2CppScheduleOne.NPCs.Responses.NPCResponses");

    println!("\n=== NPCResponses_Civilian methods ===");
    print_declared_methods(&api, "Il2CppScheduleOne.NPCs.Responses.NPCResponses_Civilian");
}
