//! Write new values into the MovementSpeeds TMap on
//! BP_PlayerInventory_C via the character component pointer.
//!
//! Walk=500, Sprint=1200, Crouch=200.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod \
//!   --test set_movement_map -- --ignored --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

const CHAR_COMP: &str = "BP_CharacterComponent_C";
const INV_PTR_OFFSET: u64 = 0x218;
const MOVEMENT_SPEEDS_MAP: u64 = 0xFE8;
const TMAP_STRIDE: u64 = 24;

struct SpeedOverride {
    key: u8,
    value: f64,
    label: &'static str,
}

const OVERRIDES: &[SpeedOverride] = &[
    SpeedOverride { key: 2, value: 500.0, label: "walk" },
    SpeedOverride { key: 3, value: 1200.0, label: "sprint" },
    SpeedOverride { key: 5, value: 200.0, label: "crouch" },
];

fn write_bytes_op(api: &common::Api, sel: &str, offset: u64, data: &[u8]) -> bool {
    let r = api.op(
        "write_bytes",
        json!({"instance_selector": sel, "offset": offset,
               "bytes_hex": hex::encode(data)}),
    );
    r.ok
}

#[test]
#[ignore = "writes to live game"]
fn set_movement_map_speeds() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    let instances = client::walk_class_instances(&api, CHAR_COMP, 100);
    let cc = instances.iter().find(|i| i.full_name.contains("PersistentLevel"));
    let Some(cc) = cc else {
        println!("no live BP_CharacterComponent_C");
        return;
    };
    let cc_addr = cc.addr;

    let Some(inv_ptr) = client::read_component_ptr(&api, cc_addr, INV_PTR_OFFSET) else {
        println!("PlayerInventory pointer is null or read failed");
        return;
    };
    println!("PlayerInventory at addr:0x{inv_ptr:x}");

    let hdr = client::read_bytes(&api, inv_ptr, MOVEMENT_SPEEDS_MAP, 16);
    if hdr.len() < 16 {
        println!("failed to read TMap header");
        return;
    }
    let elem_ptr = client::from_le_u64(&hdr, 0);
    let num = client::from_le_i32(&hdr, 8);
    println!("TMap: {num} entries, elements at 0x{elem_ptr:x}");

    if elem_ptr == 0 || num <= 0 {
        println!("empty map");
        return;
    }

    let total_bytes = num as u64 * TMAP_STRIDE;
    let data = client::read_bytes(&api, elem_ptr, 0, total_bytes);
    if data.is_empty() {
        println!("failed to read element data");
        return;
    }

    for ov in OVERRIDES {
        let slot = (0..num as usize).find(|&s| {
            let base = s * TMAP_STRIDE as usize;
            base < data.len() && data[base] == ov.key
        });
        let Some(slot) = slot else {
            println!("key {} ({}) not found in map", ov.key, ov.label);
            continue;
        };
        let base = slot as u64 * TMAP_STRIDE;
        let old = client::from_le_f64(&data, slot * 24 + 8);
        let value_offset = base + 8;
        let ok = write_bytes_op(&api, &format!("addr:0x{elem_ptr:x}"), value_offset, &ov.value.to_le_bytes());
        if ok {
            println!("{}: {} -> {} (slot {slot})", ov.label, old, ov.value);
        } else {
            println!("{}: write failed", ov.label);
        }
    }

    println!("\nverifying...");
    let vdata = client::read_bytes(&api, elem_ptr, 0, total_bytes);
    if vdata.is_empty() {
        println!("verify read failed");
        return;
    }
    for slot in 0..num as usize {
        let base = slot * TMAP_STRIDE as usize;
        if base + 16 > vdata.len() { break; }
        let key = vdata[base];
        let value = client::from_le_f64(&vdata, base + 8);
        println!("  key {key}: {value:.1}");
    }
}
