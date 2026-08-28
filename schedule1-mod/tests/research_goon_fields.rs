//! Inspect all fields on a vanilla CartelGoon that a base NPC
//! does not have (CartelGoon-declared fields).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_goon_fields. --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, handle_of, ping_or_skip};
use serde_json::json;

#[test]
fn goon_unique_fields() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Get a vanilla goon
    let pool = api.op(
        "walk_class",
        json!({"class": "Il2CppScheduleOne.Cartel.GoonPool"}),
    );
    if !pool.ok {
        println!("GoonPool not found");
        return;
    }
    let instances = pool.result.as_array().cloned().unwrap_or_default();
    let ph = instances
        .first()
        .and_then(|i| i["handle"].as_i64())
        .unwrap();
    let goons_r = api.op("read_field", json!({"handle": ph, "field": "goons"}));
    let gh = handle_of(&goons_r.result).unwrap();
    let item_r = api.op(
        "invoke_method",
        json!({"handle": gh, "method": "get_Item", "args": [0]}),
    );
    let goon_h = handle_of(&item_r.result).unwrap();

    // inspect_object to see all fields
    let inspect = api.op("inspect_object", json!({"handle": goon_h}));
    if inspect.ok {
        let fields = inspect.result["fields"].as_object();
        if let Some(f) = fields {
            println!("vanilla goon has {} fields", f.len());
            for (k, v) in f {
                let vs = format!("{}", v);
                if vs.len() > 100 {
                    println!("  {k} = {}...", &vs[..100]);
                } else {
                    println!("  {k} = {vs}");
                }
            }
        }
    } else {
        println!("inspect failed: {:?}", inspect.error);
    }

    api.op("release_handle", json!({"handle": goon_h}));
    api.op("release_handle", json!({"handle": gh}));
    api.op("release_handle", json!({"handle": ph}));
}
