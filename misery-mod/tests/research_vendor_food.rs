//! Research test: find which edible food items the Barman does
//! NOT currently accept from the player (sell list gap), then
//! add them all.
//!
//! ```text
//! set MISERY_DEBUG_PORT=17176
//! cargo test -p misery-mod --test research_vendor_food \
//!   --nocapture --test-threads=1
//! ```

mod common;
use common::{api_or_skip, offsets_live, read_bytes, Api};
use serde_json::json;
use std::collections::HashSet;
use std::time::Duration;

const VENDOR_ACTOR_CLASS: &str = "BP_MasterVendorBuildPart_C";
const VENDOR_COMP_OFFSET: u64 = 0x3B8;
const SELL_LIST_OFFSET: u64 = 0x2E8;
const SELL_STRIDE: u64 = 0x38;

fn read_u64_le(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
}

fn read_u32_le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn read_i32_le(bytes: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn try_fname(api: &Api, comparison_index: u32, number: u32) -> String {
    let raw: u64 = (comparison_index as u64) | ((number as u64) << 32);
    let r = api.op("fname_to_string", json!({"fname": raw}));
    if r.ok {
        if let Some(s) = r.result.as_str() { return s.to_string(); }
        if let Some(s) = r.result["name"].as_str() { return s.to_string(); }
        if let Some(s) = r.result["string"].as_str() { return s.to_string(); }
    }
    format!("(?idx={comparison_index:#x} num={number})")
}

struct VendorInstance {
    name: String,
    selector: String,
}

fn find_all_vendors(api: &Api) -> Vec<VendorInstance> {
    let r = api.op("walk_class", json!({"class": VENDOR_ACTOR_CLASS}));
    if !r.ok { return vec![]; }
    let instances = r.result["instances"].as_array().cloned().unwrap_or_default();
    instances
        .iter()
        .filter(|i| i["is_cdo"].as_bool() != Some(true))
        .filter_map(|i| {
            Some(VendorInstance {
                name: i["name"].as_str()?.to_string(),
                selector: i["addr_selector"].as_str()?.to_string(),
            })
        })
        .collect()
}

fn get_component_selector(api: &Api, actor_sel: &str) -> Option<String> {
    let bytes = read_bytes(api, actor_sel, VENDOR_COMP_OFFSET, 8)?;
    let addr = read_u64_le(&bytes, 0);
    if addr == 0 { return None; }
    Some(format!("addr:{addr:#x}"))
}

struct TArrayHeader {
    ptr: u64,
    num: i32,
    max: i32,
}

fn read_tarray_header(api: &Api, sel: &str, offset: u64) -> Option<TArrayHeader> {
    let bytes = read_bytes(api, sel, offset, 16)?;
    Some(TArrayHeader {
        ptr: read_u64_le(&bytes, 0),
        num: read_i32_le(&bytes, 8),
        max: read_i32_le(&bytes, 12),
    })
}

struct SellEntry {
    fname_idx: u32,
    fname_num: u32,
    item_name: String,
}

fn read_sell_entries(api: &Api, comp_sel: &str) -> Option<Vec<SellEntry>> {
    let hdr = read_tarray_header(api, comp_sel, SELL_LIST_OFFSET)?;
    if hdr.num <= 0 || hdr.num > 500 { return None; }
    let arr_sel = format!("addr:{:#x}", hdr.ptr);
    let data = read_bytes(api, &arr_sel, 0, (hdr.num as u64) * SELL_STRIDE)?;
    let mut entries = Vec::new();
    for i in 0..hdr.num as usize {
        let base = i * SELL_STRIDE as usize;
        let fname_idx = read_u32_le(&data, base + 0x08);
        let fname_num = read_u32_le(&data, base + 0x0C);
        let item_name = try_fname(api, fname_idx, fname_num);
        entries.push(SellEntry { fname_idx, fname_num, item_name });
    }
    Some(entries)
}

const ALL_FOOD_SELLABLE: &[&str] = &[
    // ready to eat (27)
    "Food_BankaCucumber",
    "Food_BankaTomaatos",
    "Food_Bread",
    "Food_BreadGood",
    "Food_CannedWater",
    "Food_Carrot",
    "Food_Caviar",
    "Food_Caviar_Crab",
    "Food_Cheese",
    "Food_Chips",
    "Food_ChocoBunny",
    "Food_Chocolate",
    "Food_CoackroachCooked",
    "Food_Cucumber",
    "Food_DeerMeatCooked",
    "Food_DriedFish",
    "Food_KabachkiGood",
    "Food_Kasha",
    "Food_Mandarin",
    "Food_Mushroom",
    "Food_MushroomCooked",
    "Food_Rice",
    "Food_Sardini",
    "Food_Sausage",
    "Food_Sugar",
    "Food_SwamperMeatCooked",
    "Food_Tomato",
    // seeds (4)
    "Food_SeedsCarrot",
    "Food_SeedsCucumber",
    "Food_SeedsTomato",
    "Food_SeedsWheat",
];

/// Report which edible food items are missing from the Barman's
/// sell list (items the vendor will NOT buy from the player).
#[test]
fn find_barman_food_gap() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    let v = vendors.iter().find(|v| v.name.contains("Barman"));
    let Some(v) = v else {
        println!("Barman not found among {} vendors", vendors.len());
        for v in &vendors { println!("  {}", v.name); }
        return;
    };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else {
        println!("no vendor component");
        return;
    };

    let sells = read_sell_entries(&api, &comp_sel).unwrap_or_default();
    let hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET).unwrap();
    println!("Barman sell list: {} entries (num={} max={})", sells.len(), hdr.num, hdr.max);

    let existing: HashSet<String> = sells.iter().map(|e| e.item_name.clone()).collect();
    println!("\ncurrently accepted:");
    for e in &sells {
        println!("  {}", e.item_name);
    }

    let missing: Vec<&&str> = ALL_FOOD_SELLABLE.iter()
        .filter(|name| !existing.contains(**name))
        .collect();

    println!("\nmissing ({} items):", missing.len());
    for name in &missing {
        println!("  {}", name);
    }

    let total_needed = hdr.num + missing.len() as i32;
    println!("\ncurrent num={}, need {} slots total, current max={}", hdr.num, total_needed, hdr.max);
    if total_needed > hdr.max {
        println!("TArray grow needed: from max={} to at least {}", hdr.max, total_needed + 10);
    } else {
        println!("enough slack, no grow needed");
    }
}

/// Add all missing edible food to the Barman's sell list.
/// Grows TArray if needed, clones entry 0 as template,
/// resolves each missing item's FName from ItemList, writes entries.
#[test]
fn add_all_food_to_barman_sell_list() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    let v = vendors.iter().find(|v| v.name.contains("Barman"));
    let Some(v) = v else {
        println!("Barman not found");
        return;
    };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else {
        println!("no vendor component");
        return;
    };
    println!("Barman component: {comp_sel}");

    // read current sell list to find what is already there
    let sells = read_sell_entries(&api, &comp_sel).unwrap_or_default();
    let hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET).unwrap();
    println!("sell list before: num={} max={}", hdr.num, hdr.max);

    let existing: HashSet<String> = sells.iter().map(|e| e.item_name.clone()).collect();
    let missing: Vec<&str> = ALL_FOOD_SELLABLE.iter()
        .filter(|name| !existing.contains(**name))
        .copied()
        .collect();
    println!("{} items already present, {} to add", existing.len(), missing.len());

    if missing.is_empty() {
        println!("nothing to add");
        return;
    }

    // resolve FNames for all missing items from MasterItemList
    // (ItemList may return 0 rows; MasterItemList is the composite
    // table that contains all items including food)
    let r = api.op("list_row_fnames", json!({"table_name": "MasterItemList"}));
    if !r.ok {
        println!("list_row_fnames(MasterItemList) failed: {:?}", r.error);
        return;
    }
    let rows = r.result["rows"].as_array().cloned().unwrap_or_default();
    let mut fname_map: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
    for row in &rows {
        let name = row["name"].as_str().unwrap_or("");
        let idx = row["fname_idx"].as_u64().unwrap_or(0) as u32;
        let num = row["fname_num"].as_u64().unwrap_or(0) as u32;
        fname_map.insert(name.to_string(), (idx, num));
    }

    // check all missing items have FNames
    let mut resolved: Vec<(&str, u32, u32)> = Vec::new();
    for name in &missing {
        if let Some((idx, num)) = fname_map.get(*name) {
            resolved.push((name, *idx, *num));
        } else {
            println!("WARNING: {} not found in ItemList, skipping", name);
        }
    }
    println!("{} items resolved from ItemList", resolved.len());

    if resolved.is_empty() {
        println!("nothing resolved, aborting");
        return;
    }

    // grow TArray if needed
    let total_needed = hdr.num + resolved.len() as i32;
    if total_needed > hdr.max {
        let new_max = total_needed + 10;
        println!("growing sell list from max={} to {}", hdr.max, new_max);
        let slow_api: Api = Api::at(17176, "/debug").with_timeout(Duration::from_secs(120));

        let r = slow_api.try_op("tarray_grow", json!({
            "instance_selector": comp_sel,
            "offset": SELL_LIST_OFFSET,
            "stride": SELL_STRIDE,
            "new_max": new_max
        }));
        match r {
            Ok(r) if r.ok => {
                println!("tarray_grow ok: {}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
            }
            Ok(r) => {
                println!("tarray_grow FAILED: {:?}", r.error);
                return;
            }
            Err(e) => {
                println!("tarray_grow network error: {e}");
                return;
            }
        }
    }

    // re-read header after potential grow
    let hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET).unwrap();
    let arr_sel = format!("addr:{:#x}", hdr.ptr);
    println!("sell list after grow: ptr={:#x} num={} max={}", hdr.ptr, hdr.num, hdr.max);
    assert!(hdr.num + resolved.len() as i32 <= hdr.max, "still not enough room");

    // read entry 0 as template (0x38 bytes)
    let template = read_bytes(&api, &arr_sel, 0, SELL_STRIDE).unwrap();
    println!("template from sell[0]: {} bytes", template.len());

    // write each new entry
    let mut current_num = hdr.num;
    for (name, fname_idx, fname_num) in &resolved {
        let mut entry = template.clone();
        entry[0x08..0x0C].copy_from_slice(&fname_idx.to_le_bytes());
        entry[0x0C..0x10].copy_from_slice(&fname_num.to_le_bytes());

        let write_offset = (current_num as u64) * SELL_STRIDE;
        let hex: String = entry.iter().map(|b| format!("{b:02x}")).collect();
        let r = api.op("write_bytes", json!({
            "instance_selector": arr_sel,
            "offset": write_offset,
            "bytes_hex": hex
        }));
        if !r.ok {
            println!("FAILED writing {}: {:?}", name, r.error);
            return;
        }
        current_num += 1;
        println!("  added {} (fname_idx={:#x}), slot {}", name, fname_idx, current_num - 1);
    }

    // bump num
    let count_hex: String = current_num.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op("write_bytes", json!({
        "instance_selector": comp_sel,
        "offset": SELL_LIST_OFFSET + 8,
        "bytes_hex": count_hex
    }));
    if !r.ok {
        println!("FAILED writing num: {:?}", r.error);
        return;
    }
    println!("bumped num to {current_num}");

    // verify
    let verify_hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET).unwrap();
    println!("\nverify: num={} max={}", verify_hdr.num, verify_hdr.max);

    let verify_sells = read_sell_entries(&api, &comp_sel).unwrap_or_default();
    println!("Barman now accepts {} items:", verify_sells.len());
    for (i, e) in verify_sells.iter().enumerate() {
        println!("  {i:>2}. {}", e.item_name);
    }

    let final_set: HashSet<String> = verify_sells.iter().map(|e| e.item_name.clone()).collect();
    let still_missing: Vec<&&str> = ALL_FOOD_SELLABLE.iter()
        .filter(|name| !final_set.contains(**name))
        .collect();
    if still_missing.is_empty() {
        println!("\nSUCCESS: all {} edible food items now accepted by Barman", ALL_FOOD_SELLABLE.len());
    } else {
        println!("\nWARNING: {} items still missing: {:?}", still_missing.len(), still_missing);
    }
}
