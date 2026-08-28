//! Research: why minted goons walk INTO their melee target
//! (models colliding) instead of holding swing range. Suspicion:
//! S1API adds the combat component fresh, so its serialized
//! config is zeroed where vanilla prefabs carry tuned values.
//! Compare a vanilla NPC's ScheduleOne.Combat.CombatBehaviour
//! numeric fields against a minted goon's.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_combat_tuning. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, handle_of, ping_or_skip, walk};
use serde_json::json;

fn numeric_fields(api: &modforge::client::Api<serde_json::Value>, h: i64) -> Vec<(String, f64)> {
    let inspect = api.op("inspect_object", json!({"handle": h}));
    let mut out = Vec::new();
    if let Some(fields) = inspect.result["fields"].as_object() {
        for (name, v) in fields {
            if let Some(n) = v.as_f64() {
                out.push((name.clone(), n));
            }
        }
    }
    out
}

/// The owning NPC's display name for a combat component.
fn owner_name(api: &modforge::client::Api<serde_json::Value>, h: i64) -> String {
    for field in ["npc", "Npc", "NPC"] {
        let r = api.op("read_field", json!({"handle": h, "field": field}));
        if let Some(nh) = handle_of(&r.result) {
            let name = api.op(
                "invoke_method",
                json!({"handle": nh, "method": "get_name", "args": []}),
            );
            api.op("release_handle", json!({"handle": nh}));
            if let Some(s) = name.result.as_str() {
                return s.to_string();
            }
        }
    }
    "?".into()
}

#[test]
fn vanilla_vs_minted_combat_config() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some(instances) = walk(&api, "ScheduleOne.Combat.CombatBehaviour") else {
        return;
    };
    let mut vanilla: Option<(String, Vec<(String, f64)>)> = None;
    let mut minted: Option<(String, Vec<(String, f64)>)> = None;
    for inst in &instances {
        let Some(h) = inst["handle"].as_i64() else {
            continue;
        };
        let owner = owner_name(&api, h);
        let is_minted =
            owner.contains("Hired") || owner.contains("Loyal") || owner.contains("Beat");
        if is_minted && minted.is_none() {
            minted = Some((owner, numeric_fields(&api, h)));
        } else if !is_minted && vanilla.is_none() && owner != "?" {
            vanilla = Some((owner, numeric_fields(&api, h)));
        }
        if vanilla.is_some() && minted.is_some() {
            break;
        }
    }
    let (Some((vn, vf)), Some((mn, mf))) = (vanilla, minted) else {
        println!("need one vanilla and one minted combat component live (mint goons first)");
        return;
    };
    println!("vanilla owner: {vn}; minted owner: {mn}");
    println!("{:<40} {:>12} {:>12}", "field", "vanilla", "minted");
    for (name, v) in &vf {
        let m = mf.iter().find(|(n, _)| n == name).map(|(_, x)| *x);
        let differs = m.map(|x| (x - v).abs() > 1e-4).unwrap_or(true);
        if differs {
            println!(
                "{name:<40} {v:>12.3} {:>12}",
                m.map(|x| format!("{x:.3}")).unwrap_or("-".into())
            );
        }
    }
}
