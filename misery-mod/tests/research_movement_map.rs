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
use common::{api_or_skip, offsets_live, read_bytes, selector_of};
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

    // Find the inventory by following BP_CharacterComponent_C's
    // PlayerInventory pointer at +0x218, because walk_class on
    // BP_PlayerInventory_C returns 0 after the game update.
    let r = api.op("walk_class", json!({"class": "BP_CharacterComponent_C"}));
    if !r.ok {
        println!("walk_class(BP_CharacterComponent_C) failed");
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
    println!("char component: {}", cc["full_name"].as_str().unwrap_or("?"));

    // Read the PlayerInventory object pointer at +0x218
    let inv_ptr_bytes = read_bytes(&api, &cc_sel, 0x218, 8);
    let Some(ipb) = inv_ptr_bytes else {
        println!("failed to read PlayerInventory pointer");
        return;
    };
    let inv_ptr = u64::from_le_bytes(ipb[..8].try_into().unwrap());
    if inv_ptr == 0 {
        println!("PlayerInventory pointer is null");
        return;
    }
    let sel = format!("addr:0x{inv_ptr:x}");
    println!("PlayerInventory at {sel}");

    // TMap header: first 8 bytes are a pointer to the sparse array
    // data, then i32 Num at +0x08 inside the sparse array header.
    // But UE's TMap layout in memory at the property offset is the
    // TSparseArray inline, not a pointer. Read a chunk and decode.
    //
    // Actually, the TMap is inline at the property offset. UE TMap
    // in memory:
    //   +0x00: Elements.Data.Data (pointer to element array)
    //   +0x08: Elements.Data.Num (i32)
    //   +0x0C: Elements.Data.Max (i32)
    //   +0x10: Elements.AllocationFlags (bitarray pointer)
    //   +0x18: Elements.AllocationFlags.Num (i32)
    //   ... more fields
    //   +0x28: Hash.Data (pointer)
    //   +0x30: Hash.Num (i32)
    //   ... etc
    //
    // We need the element pointer and count.

    // Read the TMap header (first 16 bytes)
    let header = read_bytes(&api, &sel, MOVEMENT_SPEEDS_MAP, 16);
    let Some(hdr) = header else {
        println!("failed to read TMap header");
        return;
    };
    println!("TMap header raw: {}", hex::encode(&hdr));

    let elem_ptr = u64::from_le_bytes(hdr[0..8].try_into().unwrap());
    let num = i32::from_le_bytes(hdr[8..12].try_into().unwrap());
    let max = i32::from_le_bytes(hdr[12..16].try_into().unwrap());
    println!("Elements ptr: 0x{elem_ptr:x}, Num: {num}, Max: {max}");

    if elem_ptr == 0 || num <= 0 {
        println!("empty map");
        return;
    }

    // Read all elements (stride 24 per slot). Since Num == Max,
    // all slots should be populated.
    let total_bytes = max as u64 * 24;
    let elem_sel = format!("addr:0x{elem_ptr:x}");
    let elem_data = read_bytes(&api, &elem_sel, 0, total_bytes);
    let Some(data) = elem_data else {
        println!("failed to read element data");
        return;
    };

    println!("\nMovementSpeeds map ({num} entries):");
    println!("{:>5} {:>10}", "Key", "Speed");

    for slot in 0..num as usize {
        let base = slot * 24;
        if base + 16 > data.len() { break; }

        let key = data[base];
        let value = f64::from_le_bytes(data[base+8..base+16].try_into().unwrap());
        let hash_next = i32::from_le_bytes(data[base+16..base+20].try_into().unwrap());
        let hash_idx = i32::from_le_bytes(data[base+20..base+24].try_into().unwrap());
        println!("{key:5} {value:10.1}   (hash_next={hash_next}, hash_idx={hash_idx})");
    }

    // Also dump raw hex per slot for debugging
    println!("\nRaw slots:");
    for slot in 0..num as usize {
        let base = slot * 24;
        if base + 24 > data.len() { break; }
        println!("  slot {slot}: {}", hex::encode(&data[base..base+24]));
    }
}
