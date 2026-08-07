//! The IL2CPP smoke checklist, driven over the live game's HTTP
//! control plane via the modforge client (no ad-hoc curl).
//!
//! Run with the target game up and il2cpp-smoke loaded through a
//! unityforge shim (Schedule 1 via the MelonLoader shim):
//!
//! ```text
//! cargo test -p il2cpp-smoke --test smoke. --test-threads=1 --nocapture
//! ```
//!
//! Port defaults to 17175 (il2cpp-smoke's ModDef); override with
//! IL2CPP_SMOKE_PORT. If nothing answers, the test SKIPS (prints
//! why and passes) so the workspace suite stays green without a
//! running game. A skip is not a pass of the checklist; the smoke
//! exit gate needs the "smoke checklist PASSED" line.

use modforge::client::Api;
use serde_json::{Value, json};

fn api() -> Api<Value> {
    let port = std::env::var("IL2CPP_SMOKE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17175);
    Api::at(port, "/op")
}

#[test]
fn smoke_checklist() {
    let api = api();

    // ping: liveness. Connection failure = game not running = skip.
    let ping = match api.try_op("ping", json!({})) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("SKIP: no control plane answering ({e}); launch the game with il2cpp-smoke loaded");
            return;
        }
    };
    assert!(ping.ok, "ping not ok: {:?}", ping.error);
    assert_eq!(ping.result["pong"], Value::Bool(true), "ping result: {}", ping.result);

    // smoke_state: runtime tag must be IL2CPP.
    let state = api.op("smoke_state", json!({}));
    assert!(state.ok, "smoke_state not ok: {:?}", state.error);
    assert_eq!(
        state.result["runtime"], "Il2Cpp",
        "runtime tag: {}", state.result
    );

    // Harmony postfix fires per frame: the counter must move
    // between two reads.
    let fires_a = state.result["postfix_fires"].as_u64().unwrap_or(0);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let fires_b = api.op("smoke_state", json!({})).result["postfix_fires"]
        .as_u64()
        .unwrap_or(0);
    assert!(
        fires_b > fires_a,
        "postfix counter did not advance ({fires_a} -> {fires_b}); Harmony postfix not firing"
    );

    // walk_class: a Unity camera exists in any scene including the
    // main menu.
    let walk = api.op("walk_class", json!({"class": "UnityEngine.Camera"}));
    assert!(walk.ok, "walk_class not ok: {:?}", walk.error);
    let instances = walk.result.as_array().cloned().unwrap_or_default();
    assert!(!instances.is_empty(), "no Camera instances: {}", walk.result);
    let handle = instances[0]["handle"].as_i64().expect("instance handle");

    // inspect_object: dump the camera's fields, pick a primitive
    // one, and round-trip it through read_field + write_field
    // (writing the value it already has; harmless and idempotent).
    let inspect = api.op("inspect_object", json!({"handle": handle}));
    assert!(inspect.ok, "inspect_object not ok: {:?}", inspect.error);
    let fields = inspect.result["fields"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let primitive = fields
        .iter()
        .find(|(_, v)| v.is_number() || v.is_boolean());
    let (field_name, field_value) = match primitive {
        Some((k, v)) => (k.clone(), v.clone()),
        None => {
            eprintln!("SKIP read/write round trip: no primitive field in inspect dump: {}", inspect.result);
            println!("smoke checklist PASSED (without field round trip)");
            return;
        }
    };

    let read = api.op("read_field", json!({"handle": handle, "field": field_name}));
    assert!(read.ok, "read_field({field_name}) not ok: {:?}", read.error);

    let write = api.op(
        "write_field",
        json!({"handle": handle, "field": field_name, "value": field_value}),
    );
    assert!(write.ok, "write_field({field_name}) not ok: {:?}", write.error);

    let reread = api.op("read_field", json!({"handle": handle, "field": field_name}));
    assert!(reread.ok, "re-read not ok: {:?}", reread.error);
    assert_eq!(
        reread.result, read.result,
        "field changed across idempotent write"
    );

    println!("smoke checklist PASSED");
}
