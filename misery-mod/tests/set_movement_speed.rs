//! Write MovementSpeed on the live BP_CharacterComponent_C.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 MISERY_SPEED=800 cargo test -p misery-mod \
//!   --test set_movement_speed -- --ignored --nocapture
//! ```

mod common;
use common::{api_or_skip, as_f64, offsets_live, read_bytes, selector_of};
use serde_json::json;

const CHAR_COMP: &str = "BP_CharacterComponent_C";
const MOVEMENT_SPEED: u64 = 0x200;

#[test]
#[ignore = "writes to live game"]
fn set_movement_speed() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    let speed: f64 = std::env::var("MISERY_SPEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(800.0);

    let r = api.op("walk_class", json!({"class": CHAR_COMP}));
    if !r.ok {
        println!("walk_class failed: {:?}", r.error);
        return;
    }
    let arr = r.result["instances"].as_array().cloned().unwrap_or_default();

    // Find the live player instance (under PersistentLevel, not the template).
    let inst = arr.iter().find(|i| {
        let name = i["full_name"].as_str().unwrap_or("");
        name.contains("PersistentLevel") && i["is_cdo"].as_bool() != Some(true)
    });
    let Some(inst) = inst else {
        println!("no live player {CHAR_COMP} found");
        return;
    };
    let Some(sel) = selector_of(inst) else { return };
    let name = inst["full_name"].as_str().unwrap_or("?");
    println!("target: {name}");

    let before = read_bytes(&api, &sel, MOVEMENT_SPEED, 8).and_then(|b| as_f64(&b));
    println!("before: MovementSpeed = {before:?}");

    let w = api.op(
        "write_bytes",
        json!({"instance_selector": sel, "offset": MOVEMENT_SPEED,
               "bytes_hex": hex::encode(speed.to_le_bytes())}),
    );
    assert!(w.ok, "write_bytes failed: {:?}", w.error);
    println!("wrote MovementSpeed = {speed}");

    let after = read_bytes(&api, &sel, MOVEMENT_SPEED, 8).and_then(|b| as_f64(&b));
    println!("after: MovementSpeed = {after:?}");

    // Sample a few times to see if the game overwrites it.
    for i in 0..4 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let v = read_bytes(&api, &sel, MOVEMENT_SPEED, 8).and_then(|b| as_f64(&b));
        println!("  t={}s  MovementSpeed = {v:?}", (i + 1) * 2);
    }
}
