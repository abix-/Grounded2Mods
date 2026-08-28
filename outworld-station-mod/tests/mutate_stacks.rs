//! Live mutation of DT_Materials.MaxCanStack via the running
//! mod's `/debug` endpoint. No code change to the mod required.
//! this drives `read_bytes` + `write_bytes` only.
//!
//! Run order:
//! - `bump_all_stacks_4x` multiplies every row's MaxCanStack by 4.
//!   Pick something up in-game and verify the new ceiling.
//! - `restore_all_stacks` reads the (post-bump) value, divides
//!   by 4, writes back. Useful for round-tripping tests without
//!   reloading a save.
//!
//! Caveats:
//! - Mutations are in-memory only. They survive across world
//!   loads inside the same game session, but a save+quit+relaunch
//!   resets them (the DataTable is re-read from the .uasset).
//! - Picks/equipment items at MaxCanStack=1 stay at 4 after the
//!   bump (4*1=4) which is harmless but might be visible. Add a
//!   `if cur > 1` guard later if needed.

mod common;

use serde_json::json;

const ROW_MAP_OFFSET: u64 = 0x30;
const STACK_OFFSET: u64 = 0x48;

fn find_dt_materials(api: &common::Api) -> Option<String> {
    let r = api.op(
        "walk_class",
        json!({"class": "DataTable", "max": 10000, "include_cdo": false}),
    );
    if !r.ok {
        return None;
    }
    r.result["instances"]
        .as_array()?
        .iter()
        .find(|i| i["name"].as_str() == Some("DT_Materials"))
        .and_then(|i| i["addr_selector"].as_str().map(|s| s.to_string()))
}

fn read_row_map_slots(api: &common::Api, dt: &str) -> Vec<(u64, u64)> {
    let r = api.op(
        "read_bytes",
        json!({"instance_selector": dt, "offset": ROW_MAP_OFFSET, "length": 16}),
    );
    let bytes = hex_decode(r.result["bytes_hex"].as_str().unwrap());
    let data_ptr = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let data_num = i32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;

    let r = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{data_ptr:X}"),
            "offset": 0,
            "length": data_num * 24,
        }),
    );
    let slot_bytes = hex_decode(r.result["bytes_hex"].as_str().unwrap());
    (0..data_num)
        .map(|i| {
            let off = i * 24;
            let k = u64::from_le_bytes(slot_bytes[off..off + 8].try_into().unwrap());
            let v = u64::from_le_bytes(slot_bytes[off + 8..off + 16].try_into().unwrap());
            (k, v)
        })
        .filter(|(_, v)| *v != 0)
        .collect()
}

fn read_stack(api: &common::Api, row_addr: u64) -> Option<i32> {
    let r = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{row_addr:X}"),
            "offset": STACK_OFFSET,
            "length": 4,
        }),
    );
    if !r.ok {
        return None;
    }
    let b = hex_decode(r.result["bytes_hex"].as_str().unwrap());
    Some(i32::from_le_bytes(b[0..4].try_into().unwrap()))
}

fn write_stack(api: &common::Api, row_addr: u64, new_value: i32) -> bool {
    let bytes = new_value.to_le_bytes();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op(
        "write_bytes",
        json!({
            "instance_selector": format!("addr:0x{row_addr:X}"),
            "offset": STACK_OFFSET,
            "bytes_hex": hex,
        }),
    );
    r.ok
}

#[test]
fn bump_all_stacks_4x() {
    let Some(api) = common::try_api() else { return };
    let Some(dt) = find_dt_materials(&api) else {
        eprintln!("DT_Materials not found");
        return;
    };

    let rows = read_row_map_slots(&api, &dt);
    eprintln!("=== bumping {} DT_Materials rows by 4x ===", rows.len());

    let mut changed = 0;
    for (key, row_addr) in &rows {
        let Some(cur) = read_stack(&api, *row_addr) else {
            continue;
        };
        if cur <= 1 {
            // Equipment / unique items. Skip to avoid scrambling
            // gameplay assumptions.
            continue;
        }
        let new_value = cur.saturating_mul(4);
        if write_stack(&api, *row_addr, new_value) {
            // Verify
            let after = read_stack(&api, *row_addr).unwrap_or(-1);
            if after != new_value {
                eprintln!("  WARN fname=0x{key:016X}: wrote {new_value}, read back {after}");
                continue;
            }
            changed += 1;
        }
    }
    eprintln!("=== mutated {changed} rows. Pick up an item to verify. ===");
}

#[test]
fn restore_all_stacks() {
    // Inverse of bump_all_stacks_4x: divides by 4. Only useful if
    // bump ran first in the same game session and you want to
    // round-trip back without quitting.
    let Some(api) = common::try_api() else { return };
    let Some(dt) = find_dt_materials(&api) else {
        return;
    };

    let rows = read_row_map_slots(&api, &dt);
    let mut restored = 0;
    for (_key, row_addr) in &rows {
        let Some(cur) = read_stack(&api, *row_addr) else {
            continue;
        };
        if cur <= 4 {
            continue;
        } // skip equipment + tiny stacks
        let new_value = cur / 4;
        if write_stack(&api, *row_addr, new_value) {
            restored += 1;
        }
    }
    eprintln!("=== restored {restored} rows (cur / 4) ===");
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
