//! Research starter: enumerate every DataTable in the running
//! game, then narrow to ones that look like item / material /
//! stack tables.
//!
//! Status: pre-offsets. Returns "ue runtime not initialized"
//! until OWS PlatformOffsets are dumped and filled into
//! `src/lib.rs::STEAM`. Once that lands, this test prints a
//! catalogue we can scan to identify the stack-bearing table.
//!
//! Test pattern: starts as `eprintln!` of observations. Promote
//! individual lines to `assert!` once we know what to expect.

mod common;

use serde_json::json;

#[test]
fn list_all_datatables() {
    let Some(api) = common::try_api() else {
        eprintln!("OWS_DEBUG_PORT unset — skipping");
        return;
    };
    let r = api.op(
        "walk_class",
        json!({ "class": "DataTable", "max": 10000, "include_cdo": false }),
    );
    if !r.ok {
        eprintln!(
            "walk_class failed: {:?} (offsets not filled in yet?)",
            r.error
        );
        return;
    }
    let total = r.result["total"].as_u64().unwrap_or(0);
    let returned = r.result["returned"].as_u64().unwrap_or(0);
    eprintln!("DataTable instances: total={total}, returned={returned}");
    let instances = match r.result["instances"].as_array() {
        Some(a) => a,
        None => {
            eprintln!("no 'instances' array in response");
            return;
        }
    };
    for inst in instances {
        let name = inst["name"].as_str().unwrap_or("?");
        let full = inst["full_name"].as_str().unwrap_or("?");
        eprintln!("{name:<48} ({full})");
    }
}

#[test]
fn find_likely_item_tables() {
    let Some(api) = common::try_api() else { return };
    let r = api.op(
        "walk_class",
        json!({ "class": "DataTable", "max": 10000, "include_cdo": false }),
    );
    if !r.ok {
        eprintln!("walk_class failed: {:?}", r.error);
        return;
    }
    let Some(instances) = r.result["instances"].as_array() else {
        return;
    };
    let candidates: Vec<_> = instances
        .iter()
        .filter(|i| {
            let n = i["name"].as_str().unwrap_or("").to_lowercase();
            n.contains("material")
                || n.contains("item")
                || n.contains("resource")
                || n.contains("stack")
        })
        .collect();
    eprintln!(
        "DataTables matching item / material / stack / resource: {}",
        candidates.len()
    );
    for c in &candidates {
        eprintln!(
            "  addr={} name={} full={}",
            c["addr"].as_str().unwrap_or("?"),
            c["name"].as_str().unwrap_or("?"),
            c["full_name"].as_str().unwrap_or("?")
        );
    }
}
