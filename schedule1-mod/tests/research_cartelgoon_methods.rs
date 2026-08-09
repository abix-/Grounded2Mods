//! Dump CartelGoon declared methods to find what handles
//! damage/retaliation differently from base NPC.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_cartelgoon_methods. --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn cartelgoon_methods() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let r = api.op(
        "list_methods",
        json!({"class": "Il2CppScheduleOne.Cartel.CartelGoon"}),
    );
    if !r.ok {
        println!("list_methods failed: {:?}", r.error);
        return;
    }
    let methods = r.result["methods"].as_array().cloned().unwrap_or_default();
    println!("CartelGoon: {} total methods", methods.len());

    // Only show methods declared on CartelGoon itself (not inherited)
    println!("\n=== DECLARED ON CartelGoon ===");
    for m in &methods {
        let declared = m["declared_on"].as_str().unwrap_or("");
        if !declared.contains("CartelGoon") {
            continue;
        }
        println!(
            "  {}({}) -> {}{}",
            m["name"].as_str().unwrap_or("?"),
            m["params"].as_i64().unwrap_or(-1),
            m["return"].as_str().unwrap_or("?"),
            if m["static"].as_bool() == Some(true) { " [static]" } else { "" },
        );
    }

    // Show any method with "attack", "impact", "respond", "damage", "combat" in name
    println!("\n=== COMBAT-RELATED (any declaring class) ===");
    for m in &methods {
        let name = m["name"].as_str().unwrap_or("").to_lowercase();
        if name.contains("attack")
            || name.contains("impact")
            || name.contains("respond")
            || name.contains("damage")
            || name.contains("combat")
            || name.contains("retaliat")
            || name.contains("notify")
            || name.contains("threat")
        {
            println!(
                "  {}({}) -> {} [from: {}]",
                m["name"].as_str().unwrap_or("?"),
                m["params"].as_i64().unwrap_or(-1),
                m["return"].as_str().unwrap_or("?"),
                m["declared_on"].as_str().unwrap_or("?"),
            );
        }
    }
}
