//! Smoke test: ping the control plane and list registered ops.
//!
//! ```text
//! FISH_DEBUG_PORT=17174 cargo test -p fish-mod --test smoke -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

#[test]
fn ping() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("ping", json!({}));
    assert!(r.ok, "ping failed: {:?}", r.error);
    println!("ping: {}", r.result);
}

#[test]
fn list_ops() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("list_ops", json!({}));
    assert!(r.ok, "list_ops failed: {:?}", r.error);
    let ops = r.result["ops"].as_array().cloned().unwrap_or_default();
    println!("{} ops registered:", ops.len());
    for op in &ops {
        println!("  {}", op["name"].as_str().unwrap_or("?"));
    }
}
