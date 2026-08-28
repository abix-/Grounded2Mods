//! Read the MovementSpeeds TMap on BP_PlayerInventory_C.
//!
//! The map is TMap<Byte, Double> at offset 0xFE8.
//! TMap stride is 24 bytes. Layout per element:
//!   +0x00: key (u8) + 7 bytes padding
//!   +0x08: value (f64)
//!   +0x10: HashNextId (i32) + HashIndex (i32)
//!
//! The TMap header (at the instance pointer + 0xFE8):
//!   +0x00: Elements (TSparseArray pointer)
//!   +0x08: Num (i32)
//!   ...then hash/allocator state
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod \
//!   --test research_movement_map -- --ignored --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

const PLAYER_INV: &str = "BP_PlayerInventory_C";
const MOVEMENT_SPEEDS_MAP: u64 = 0xFE8;

#[test]
#[ignore = "needs live game"]
fn read_movement_speeds_map() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    let instances = client::walk_class_instances(&api, "BP_CharacterComponent_C", 100);
    let cc = instances
        .iter()
        .find(|i| i.full_name.contains("PersistentLevel"));
    let Some(cc) = cc else {
        println!("no live BP_CharacterComponent_C");
        return;
    };
    println!("char component: {}", cc.full_name);

    let Some(inv_ptr) = client::read_component_ptr(&api, cc.addr, 0x218) else {
        println!("PlayerInventory pointer is null or read failed");
        return;
    };
    println!("PlayerInventory at addr:0x{inv_ptr:x}");

    let hdr_bytes = client::read_bytes(&api, inv_ptr, MOVEMENT_SPEEDS_MAP, 16);
    if hdr_bytes.len() < 16 {
        println!("failed to read TMap header");
        return;
    }
    println!("TMap header raw: {}", hex::encode(&hdr_bytes));

    let elem_ptr = client::from_le_u64(&hdr_bytes, 0);
    let num = client::from_le_i32(&hdr_bytes, 8);
    let max = client::from_le_i32(&hdr_bytes, 12);
    println!("Elements ptr: 0x{elem_ptr:x}, Num: {num}, Max: {max}");

    if elem_ptr == 0 || num <= 0 {
        println!("empty map");
        return;
    }

    let total_bytes = max as u64 * 24;
    let data = client::read_bytes(&api, elem_ptr, 0, total_bytes);
    if data.is_empty() {
        println!("failed to read element data");
        return;
    }

    println!("\nMovementSpeeds map ({num} entries):");
    println!("{:>5} {:>10}", "Key", "Speed");

    for slot in 0..num as usize {
        let base = slot * 24;
        if base + 24 > data.len() {
            break;
        }

        let key = data[base];
        let value = client::from_le_f64(&data, base + 8);
        let hash_next = client::from_le_i32(&data, base + 16);
        let hash_idx = client::from_le_i32(&data, base + 20);
        println!("{key:5} {value:10.1}   (hash_next={hash_next}, hash_idx={hash_idx})");
    }

    println!("\nRaw slots:");
    for slot in 0..num as usize {
        let base = slot * 24;
        if base + 24 > data.len() {
            break;
        }
        println!("  slot {slot}: {}", hex::encode(&data[base..base + 24]));
    }
}
