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
use common::{api_or_skip, offsets_live, read_bytes, selector_of};
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

    let r = api.op("walk_class", json!({"class": CHAR_COMP}));
    if !r.ok {
        println!("walk_class failed");
        return;
    }
    let arr = r.result["instances"].as_array().cloned().unwrap_or_default();
    let cc = arr.iter().find(|i| {
        let name = i["full_name"].as_str().unwrap_or("");
        name.contains("PersistentLevel") && i["is_cdo"].as_bool() != Some(true)
    });
    let Some(cc) = cc else {
        println!("no live BP_CharacterComponent_C");
        return;
    };
    let Some(cc_sel) = selector_of(cc) else { return };

    let inv_ptr_bytes = read_bytes(&api, &cc_sel, INV_PTR_OFFSET, 8);
    let Some(ipb) = inv_ptr_bytes else {
        println!("failed to read PlayerInventory pointer");
        return;
    };
    let inv_ptr = u64::from_le_bytes(ipb[..8].try_into().unwrap());
    if inv_ptr == 0 {
        println!("PlayerInventory pointer is null");
        return;
    }
    let inv_sel = format!("addr:0x{inv_ptr:x}");
    println!("PlayerInventory at {inv_sel}");

    let header = read_bytes(&api, &inv_sel, MOVEMENT_SPEEDS_MAP, 16);
    let Some(hdr) = header else {
        println!("failed to read TMap header");
        return;
    };
    let elem_ptr = u64::from_le_bytes(hdr[0..8].try_into().unwrap());
    let num = i32::from_le_bytes(hdr[8..12].try_into().unwrap());
    println!("TMap: {num} entries, elements at 0x{elem_ptr:x}");

    if elem_ptr == 0 || num <= 0 {
        println!("empty map");
        return;
    }

    let elem_sel = format!("addr:0x{elem_ptr:x}");
    let total_bytes = num as u64 * TMAP_STRIDE;
    let elem_data = read_bytes(&api, &elem_sel, 0, total_bytes);
    let Some(data) = elem_data else {
        println!("failed to read element data");
        return;
    };

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
        let old = f64::from_le_bytes(data[slot * 24 + 8..slot * 24 + 16].try_into().unwrap());
        let value_offset = base + 8;
        let ok = write_bytes_op(&api, &elem_sel, value_offset, &ov.value.to_le_bytes());
        if ok {
            println!("{}: {} -> {} (slot {slot})", ov.label, old, ov.value);
        } else {
            println!("{}: write failed", ov.label);
        }
    }

    println!("\nverifying...");
    let verify = read_bytes(&api, &elem_sel, 0, total_bytes);
    let Some(vdata) = verify else {
        println!("verify read failed");
        return;
    };
    for slot in 0..num as usize {
        let base = slot * TMAP_STRIDE as usize;
        if base + 16 > vdata.len() { break; }
        let key = vdata[base];
        let value = f64::from_le_bytes(vdata[base+8..base+16].try_into().unwrap());
        println!("  key {key}: {value:.1}");
    }
}
