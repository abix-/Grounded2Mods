//! Write MovementSpeed on the live BP_CharacterComponent_C.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 MISERY_SPEED=800 cargo test -p misery-mod \
//!   --test set_movement_speed -- --ignored --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
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

    let instances = client::walk_class_instances(&api, CHAR_COMP, 100);
    let inst = instances
        .iter()
        .find(|i| i.full_name.contains("PersistentLevel"));
    let Some(inst) = inst else {
        println!("no live player {CHAR_COMP} found");
        return;
    };
    let addr = inst.addr;
    let sel = &inst.addr_selector;
    println!("target: {}", inst.full_name);

    let before = client::read_f64(&api, addr, MOVEMENT_SPEED);
    println!("before: MovementSpeed = {before}");

    let w = api.op(
        "write_bytes",
        json!({"instance_selector": sel, "offset": MOVEMENT_SPEED,
               "bytes_hex": hex::encode(speed.to_le_bytes())}),
    );
    assert!(w.ok, "write_bytes failed: {:?}", w.error);
    println!("wrote MovementSpeed = {speed}");

    let after = client::read_f64(&api, addr, MOVEMENT_SPEED);
    println!("after: MovementSpeed = {after}");

    for i in 0..4 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let v = client::read_f64(&api, addr, MOVEMENT_SPEED);
        println!("  t={}s  MovementSpeed = {v}", (i + 1) * 2);
    }
}
