//! Research test for the vendor buy/sell system.
//!
//! BP_MasterVendorBuildPart_C actors in the safe hub each have a
//! BP_VendorComponent_C at offset 0x3B8. That component holds:
//!   BuyList  TArray<S_VendorBuy>  at +0x2D8
//!   SellList TArray<S_VendorSell> at +0x2E8
//!
//! S_VendorSell stride is 0x38. S_VendorBuy stride is 0x40
//! (extra Stock int at 0x28 pushes Category to 0x30).
//!
//! The Item field (bytes 0x00..0x18) appears to be an
//! FDataTableRowHandle: 8-byte DataTable pointer + 8-byte FName
//! (comparison_index i32 + number u32).
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_vendors -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live, read_bytes, Api};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

const VENDOR_ACTOR_CLASS: &str = "BP_MasterVendorBuildPart_C";
const VENDOR_COMP_OFFSET: u64 = 0x3B8;
const BUY_LIST_OFFSET: u64 = 0x2D8;
const SELL_LIST_OFFSET: u64 = 0x2E8;
const SELL_STRIDE: u64 = 0x38;
const BUY_STRIDE: u64 = 0x40;

fn read_u64_le(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
}

fn read_u32_le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn read_i32_le(bytes: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

/// Resolve an FName from its 8-byte representation (comparison_index
/// as low u32, number as high u32, combined into one u64).
fn try_fname(api: &Api, comparison_index: u32, number: u32) -> String {
    let raw: u64 = (comparison_index as u64) | ((number as u64) << 32);
    let r = api.op("fname_to_string", json!({"fname": raw}));
    if r.ok {
        if let Some(s) = r.result.as_str() {
            return s.to_string();
        }
        if let Some(s) = r.result["name"].as_str() {
            return s.to_string();
        }
        if let Some(s) = r.result["string"].as_str() {
            return s.to_string();
        }
    }
    format!("(?idx={comparison_index:#x} num={number})")
}

struct VendorInstance {
    name: String,
    selector: String,
}

fn find_all_vendors(api: &Api) -> Vec<VendorInstance> {
    let r = api.op("walk_class", json!({"class": VENDOR_ACTOR_CLASS}));
    if !r.ok {
        println!("walk_class failed: {:?}", r.error);
        return vec![];
    }
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
    item_ptr: u64,
    fname_idx: u32,
    fname_num: u32,
    item_name: String,
    price_num: i32,
    cat_num: i32,
}

struct BuyEntry {
    item_ptr: u64,
    fname_idx: u32,
    fname_num: u32,
    item_name: String,
    price_num: i32,
    stock: i32,
    cat_num: i32,
}

fn read_sell_entries(api: &Api, comp_sel: &str) -> Option<Vec<SellEntry>> {
    let hdr = read_tarray_header(api, comp_sel, SELL_LIST_OFFSET)?;
    if hdr.num <= 0 || hdr.num > 500 { return None; }
    let arr_sel = format!("addr:{:#x}", hdr.ptr);
    let data = read_bytes(api, &arr_sel, 0, (hdr.num as u64) * SELL_STRIDE)?;
    let mut entries = Vec::new();
    for i in 0..hdr.num as usize {
        let base = i * SELL_STRIDE as usize;
        let item_ptr = read_u64_le(&data, base);
        let fname_idx = read_u32_le(&data, base + 0x08);
        let fname_num = read_u32_le(&data, base + 0x0C);
        let item_name = try_fname(api, fname_idx, fname_num);
        let price_num = read_i32_le(&data, base + 0x20);
        let cat_num = read_i32_le(&data, base + 0x30);
        entries.push(SellEntry { item_ptr, fname_idx, fname_num, item_name, price_num, cat_num });
    }
    Some(entries)
}

fn read_buy_entries(api: &Api, comp_sel: &str) -> Option<Vec<BuyEntry>> {
    let hdr = read_tarray_header(api, comp_sel, BUY_LIST_OFFSET)?;
    if hdr.num <= 0 || hdr.num > 500 { return None; }
    let arr_sel = format!("addr:{:#x}", hdr.ptr);
    let data = read_bytes(api, &arr_sel, 0, (hdr.num as u64) * BUY_STRIDE)?;
    let mut entries = Vec::new();
    for i in 0..hdr.num as usize {
        let base = i * BUY_STRIDE as usize;
        let item_ptr = read_u64_le(&data, base);
        let fname_idx = read_u32_le(&data, base + 0x08);
        let fname_num = read_u32_le(&data, base + 0x0C);
        let item_name = try_fname(api, fname_idx, fname_num);
        let price_num = read_i32_le(&data, base + 0x20);
        let stock = read_i32_le(&data, base + 0x28);
        let cat_num = read_i32_le(&data, base + 0x38);
        entries.push(BuyEntry { item_ptr, fname_idx, fname_num, item_name, price_num, stock, cat_num });
    }
    Some(entries)
}

/// List all vendor actors in the safe hub.
#[test]
fn list_vendor_actors() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    println!("{} vendor actor(s):", vendors.len());
    for v in &vendors {
        println!("  {} @ {}", v.name, v.selector);
    }
}

/// Dump sell and buy lists for ALL vendors.
#[test]
fn dump_all_vendors() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    println!("{} vendor(s) found\n", vendors.len());

    for v in &vendors {
        println!("=== {} ===", v.name);
        let Some(comp_sel) = get_component_selector(&api, &v.selector) else {
            println!("  (no vendor component)\n");
            continue;
        };

        // sell list (what vendor buys from player)
        if let Some(sells) = read_sell_entries(&api, &comp_sel) {
            println!("  SELL LIST (vendor buys these from player): {} entries", sells.len());
            for (i, e) in sells.iter().enumerate() {
                println!("    {i:>2}. {:<40} prices={} cats={}", e.item_name, e.price_num, e.cat_num);
            }
        } else {
            println!("  SELL LIST: could not read");
        }

        // buy list (what player buys from vendor)
        if let Some(buys) = read_buy_entries(&api, &comp_sel) {
            println!("  BUY LIST (player buys these from vendor): {} entries", buys.len());
            for (i, e) in buys.iter().enumerate() {
                println!("    {i:>2}. {:<40} stock={:<4} prices={} cats={}", e.item_name, e.stock, e.price_num, e.cat_num);
            }
        } else {
            println!("  BUY LIST: could not read");
        }
        println!();
    }
}

/// Check whether the Item pointer (offset 0x00 in each entry)
/// points to the ItemList DataTable. If so, this confirms the
/// Item field is an FDataTableRowHandle.
#[test]
fn check_item_is_datatable_handle() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    // get the ItemList DataTable address
    let r = api.op("discover_data_tables", json!({"name": "ItemList"}));
    if !r.ok {
        println!("discover_data_tables(ItemList) failed: {:?}", r.error);
        return;
    }
    let item_list_addr = r.result["addr"].as_str()
        .or_else(|| r.result["address"].as_str())
        .or_else(|| r.result["tables"].as_array()
            .and_then(|t| t.first())
            .and_then(|t| t["addr"].as_str().or_else(|| t["address"].as_str())));
    println!("ItemList DataTable lookup result: {}", serde_json::to_string_pretty(&r.result).unwrap_or_default());

    // now read a sell entry's item pointer from the first vendor
    let vendors = find_all_vendors(&api);
    let Some(v) = vendors.first() else {
        println!("no vendors");
        return;
    };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else {
        println!("no component");
        return;
    };
    let hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET);
    let Some(hdr) = hdr else { return; };
    if hdr.num <= 0 { return; }
    let arr_sel = format!("addr:{:#x}", hdr.ptr);
    let Some(first_entry) = read_bytes(&api, &arr_sel, 0, 0x18) else { return; };
    let item_ptr = read_u64_le(&first_entry, 0);
    println!("first sell entry item_ptr = {item_ptr:#x}");

    // try inspect_address on the item_ptr to see if it is a UDataTable
    let r2 = api.op("inspect_address", json!({"addr": format!("{item_ptr:#x}")}));
    println!("inspect_address({item_ptr:#x}): ok={}, result={}", r2.ok,
        serde_json::to_string(&r2.result).unwrap_or_default());
}

/// Try resolving item identifiers by cross-referencing with
/// ItemList DataTable row names. Dumps the DataTable and builds
/// an FName index to row name map.
#[test]
fn resolve_items_via_itemlist() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    // dump ItemList sample (small batch to avoid timeout)
    let r = api.try_op("dump_data_table", json!({"table_name": "ItemList", "max_rows": 10}));
    let r = match r {
        Ok(r) => r,
        Err(e) => { println!("dump_data_table failed: {e}"); return; }
    };
    if !r.ok {
        println!("dump_data_table error: {:?}", r.error);
        return;
    }
    let rows = r.result["rows"].as_array().cloned().unwrap_or_default();
    println!("ItemList sample: {} rows", rows.len());
    for row in &rows {
        let name = row["row_name"].as_str().unwrap_or("?");
        println!("  row: {name}");
    }

    // get sell list from first vendor and print the fname indices
    let vendors = find_all_vendors(&api);
    let Some(v) = vendors.first() else { return; };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else { return; };
    if let Some(sells) = read_sell_entries(&api, &comp_sel) {
        println!("\n{} sell entries, fname indices:", v.name);
        for (i, e) in sells.iter().enumerate() {
            println!("  {i}: idx={:#x} num={} resolved={}", e.fname_idx, e.fname_num, e.item_name);
        }
    }
}

/// Read the Price array elements for the first sell entry of
/// each vendor to understand the price struct layout.
#[test]
fn probe_price_arrays() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    let Some(v) = vendors.first() else { return; };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else { return; };

    // read sell list raw to get the price array pointer from entry 0
    let sell_hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET);
    let Some(sell_hdr) = sell_hdr else { return; };
    if sell_hdr.num <= 0 { return; }
    let arr_sel = format!("addr:{:#x}", sell_hdr.ptr);
    let Some(entry0) = read_bytes(&api, &arr_sel, 0, SELL_STRIDE) else { return; };

    let price_ptr = read_u64_le(&entry0, 0x18);
    let price_num = read_i32_le(&entry0, 0x20);
    println!("sell entry 0 price: ptr={price_ptr:#x} num={price_num}");

    if price_num > 0 && price_num < 20 {
        let price_sel = format!("addr:{price_ptr:#x}");
        // read a generous chunk to probe element size
        let Some(price_data) = read_bytes(&api, &price_sel, 0, 128) else {
            println!("could not read price data");
            return;
        };
        println!("price array raw hex ({} bytes):", price_data.len());
        for (i, chunk) in price_data.chunks(16).enumerate() {
            let offset = i * 16;
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("  {offset:04x}: {}", hex.join(" "));
        }
        // try fname on first few u32s
        for off in (0..32).step_by(4) {
            if off + 4 > price_data.len() { break; }
            let val = read_u32_le(&price_data, off);
            let name = try_fname(&api, val, 0);
            println!("  price[{off:#x}] u32={val:#x} fname={name}");
        }
    }

    // also check a buy entry price (4 elements)
    let buy_hdr = read_tarray_header(&api, &comp_sel, BUY_LIST_OFFSET);
    let Some(buy_hdr) = buy_hdr else { return; };
    if buy_hdr.num <= 0 { return; }
    let buy_arr_sel = format!("addr:{:#x}", buy_hdr.ptr);
    let Some(buy_entry0) = read_bytes(&api, &buy_arr_sel, 0, BUY_STRIDE) else { return; };

    let buy_price_ptr = read_u64_le(&buy_entry0, 0x18);
    let buy_price_num = read_i32_le(&buy_entry0, 0x20);
    println!("\nbuy entry 0 price: ptr={buy_price_ptr:#x} num={buy_price_num}");

    if buy_price_num > 0 && buy_price_num < 20 {
        let price_sel = format!("addr:{buy_price_ptr:#x}");
        let Some(price_data) = read_bytes(&api, &price_sel, 0, 256) else { return; };
        println!("buy price array raw hex ({} bytes):", price_data.len());
        for (i, chunk) in price_data.chunks(16).enumerate() {
            let offset = i * 16;
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("  {offset:04x}: {}", hex.join(" "));
        }
        for off in (0..64).step_by(4) {
            if off + 4 > price_data.len() { break; }
            let val = read_u32_le(&price_data, off);
            let name = try_fname(&api, val, 0);
            println!("  buy_price[{off:#x}] u32={val:#x} fname={name}");
        }
    }
}

/// Summary: unique item pointers across all vendors.
/// If every vendor shares the same item_ptr at offset 0x00,
/// that confirms it is a single DataTable reference.
#[test]
fn shared_item_pointer_check() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    let mut all_ptrs: HashMap<u64, Vec<String>> = HashMap::new();

    for v in &vendors {
        let Some(comp_sel) = get_component_selector(&api, &v.selector) else { continue; };
        if let Some(sells) = read_sell_entries(&api, &comp_sel) {
            for e in &sells {
                all_ptrs.entry(e.item_ptr).or_default().push(format!("{} sell", v.name));
            }
        }
        if let Some(buys) = read_buy_entries(&api, &comp_sel) {
            for e in &buys {
                all_ptrs.entry(e.item_ptr).or_default().push(format!("{} buy", v.name));
            }
        }
    }

    println!("{} unique item_ptr value(s) across all vendors:", all_ptrs.len());
    for (ptr, sources) in &all_ptrs {
        println!("  {ptr:#x}: used by {} entries ({} ...)", sources.len(),
            sources.iter().take(3).cloned().collect::<Vec<_>>().join(", "));
    }

    if all_ptrs.len() == 1 {
        println!("\nAll entries share one pointer. This is likely the ItemList DataTable.");
    }
}

/// Add Resource_Plastic to ResourseSaler's sell list by expanding
/// the TArray from 21 to 22 entries. Clones an existing entry as
/// a template and swaps the item FName to Resource_Plastic's.
///
/// Resource_Plastic's FName is read from the buy list (entry 1).
/// The template entry is copied from sell list entry 0
/// (Resource_Glass), keeping the same price and category structure.
#[test]
fn add_plastic_to_resourcesaler_sell_list() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    let v = vendors.iter().find(|v| v.name.contains("Resourse"));
    let Some(v) = v else {
        println!("ResourseSaler not found");
        return;
    };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else {
        println!("no vendor component");
        return;
    };
    println!("ResourseSaler component: {comp_sel}");

    // read buy list to get Resource_Plastic's FName (entry 1)
    let buy_hdr = read_tarray_header(&api, &comp_sel, BUY_LIST_OFFSET).unwrap();
    let buy_sel = format!("addr:{:#x}", buy_hdr.ptr);
    let buy_entry1 = read_bytes(&api, &buy_sel, BUY_STRIDE, BUY_STRIDE).unwrap();
    let plastic_fname_idx = read_u32_le(&buy_entry1, 0x08);
    let plastic_fname_num = read_u32_le(&buy_entry1, 0x0C);
    let plastic_name = try_fname(&api, plastic_fname_idx, plastic_fname_num);
    println!("Resource_Plastic FName: idx={plastic_fname_idx:#x} num={plastic_fname_num} => {plastic_name}");
    assert!(plastic_name.contains("Plastic"), "expected Plastic, got {plastic_name}");

    // read sell list header
    let sell_hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET).unwrap();
    let sell_arr_sel = format!("addr:{:#x}", sell_hdr.ptr);
    println!("SellList: ptr={:#x} num={} max={}", sell_hdr.ptr, sell_hdr.num, sell_hdr.max);

    // check it is not already there
    if let Some(sells) = read_sell_entries(&api, &comp_sel) {
        for e in &sells {
            if e.item_name.contains("Plastic") {
                println!("Resource_Plastic already in sell list at this index, skipping");
                return;
            }
        }
    }

    // read entry 0 as template (Resource_Glass, 0x38 bytes)
    let template = read_bytes(&api, &sell_arr_sel, 0, SELL_STRIDE).unwrap();
    println!("template entry (Resource_Glass): {} bytes", template.len());

    // build new entry: copy template, overwrite FName at 0x08..0x10
    let mut new_entry = template.clone();
    new_entry[0x08..0x0C].copy_from_slice(&plastic_fname_idx.to_le_bytes());
    new_entry[0x0C..0x10].copy_from_slice(&plastic_fname_num.to_le_bytes());

    // write new entry at slot [num] (past current last entry)
    let new_offset = (sell_hdr.num as u64) * SELL_STRIDE;
    let hex: String = new_entry.iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op("write_bytes", json!({
        "instance_selector": sell_arr_sel,
        "offset": new_offset,
        "bytes_hex": hex
    }));
    if !r.ok {
        println!("write_bytes failed: {:?}", r.error);
        return;
    }
    println!("wrote new entry at offset {new_offset:#x}");

    // bump num from 21 to 22 (max is already 26, leave it)
    let new_count: i32 = sell_hdr.num + 1;
    let count_hex: String = new_count.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect();
    // num is at SELL_LIST_OFFSET + 8
    let r = api.op("write_bytes", json!({
        "instance_selector": comp_sel,
        "offset": SELL_LIST_OFFSET + 8,
        "bytes_hex": count_hex
    }));
    if !r.ok {
        println!("write num/max failed: {:?}", r.error);
        return;
    }
    println!("bumped num and max to {new_count}");

    // verify: re-read sell list and check last entry
    let verify_hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET).unwrap();
    println!("verify: num={} max={}", verify_hdr.num, verify_hdr.max);

    let last = read_bytes(&api, &sell_arr_sel, new_offset, SELL_STRIDE).unwrap();
    let verify_idx = read_u32_le(&last, 0x08);
    let verify_num = read_u32_le(&last, 0x0C);
    let verify_name = try_fname(&api, verify_idx, verify_num);
    println!("last entry: {verify_name}");
    assert!(verify_name.contains("Plastic"), "verify failed: got {verify_name}");
    println!("SUCCESS: Resource_Plastic added to ResourseSaler sell list");
}

/// Find Resource_SewingKit's FName and the DataTable it lives in.
/// Resource_SewingKit is in MasterItemList but NOT in ItemList.
/// The existing vendor buy entries all share a DataTable pointer
/// at offset 0x00. This test reads that pointer, resolves each
/// buy entry's FName, and searches all 19 DataTables for SewingKit.
/// READ-ONLY: no writes, no inspect_address, no discover_class_detail.
#[test]
fn find_sewing_kit_fname() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    // find which DataTables contain Resource_SewingKit
    let tables = [
        "ItemList", "MasterItemList", "CraftingRecipesList",
        "MasterCraftingRecipeList", "BuildPartList", "MasterBuildPartList",
        "DT_Artifacts", "DeviceList", "MasterDeviceList",
        "CookingList", "MasterCookingList", "ItemSpawnerList",
        "MasterSpawnerList", "DifficultyList", "InventoryGridLayout",
        "MasterGridLayoutList", "LOOK_Presets", "DT_Weather",
        "DT_PlayerStatDescr",
    ];
    for table in &tables {
        let r = api.try_op("list_row_names", json!({"table_name": table}));
        let r = match r {
            Ok(r) => r,
            Err(e) => { println!("  {table}: error {e}"); continue; }
        };
        if !r.ok {
            println!("  {table}: op failed {:?}", r.error);
            continue;
        }
        let rows = r.result["rows"].as_array().cloned().unwrap_or_default();
        let has_sewing = rows.iter().any(|v| {
            v.as_str().map_or(false, |s| s.contains("SewingKit"))
        });
        if has_sewing {
            println!("  {table}: CONTAINS Resource_SewingKit ({} rows total)", rows.len());
        }
    }

    // read the DataTable pointer from ResourseSaler buy entry 0
    let vendors = find_all_vendors(&api);
    let v = vendors.iter().find(|v| v.name.contains("Resourse"));
    let Some(v) = v else {
        println!("ResourseSaler not found");
        return;
    };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else {
        println!("no vendor component");
        return;
    };

    let buy_hdr = read_tarray_header(&api, &comp_sel, BUY_LIST_OFFSET);
    let Some(buy_hdr) = buy_hdr else {
        println!("could not read buy list header");
        return;
    };
    println!("\nResourseSaler buy list: num={} max={}", buy_hdr.num, buy_hdr.max);

    let buy_arr_sel = format!("addr:{:#x}", buy_hdr.ptr);
    // read all buy entries to dump DataTable ptr and FName for each
    for i in 0..buy_hdr.num {
        let offset = (i as u64) * BUY_STRIDE;
        let Some(entry) = read_bytes(&api, &buy_arr_sel, offset, 0x18) else { continue };
        let dt_ptr = read_u64_le(&entry, 0);
        let fname_idx = read_u32_le(&entry, 0x08);
        let fname_num = read_u32_le(&entry, 0x0C);
        let field_10 = read_u64_le(&entry, 0x10);
        let name = try_fname(&api, fname_idx, fname_num);
        println!("  buy[{i:>2}] dt={dt_ptr:#x} fname_idx={fname_idx:#x} fname_num={fname_num} field_10={field_10:#x} => {name}");
    }

    // also read sell list header to report slack
    let sell_hdr = read_tarray_header(&api, &comp_sel, SELL_LIST_OFFSET);
    if let Some(sh) = sell_hdr {
        println!("\nResourseSaler sell list: num={} max={}", sh.num, sh.max);
    }

    // resolve the FName for Resource_SewingKit by scanning MasterItemList rows
    let r = api.try_op("list_row_names", json!({"table_name": "MasterItemList"}));
    if let Ok(r) = r {
        if r.ok {
            let rows = r.result["rows"].as_array().cloned().unwrap_or_default();
            // find which index Resource_SewingKit is at, then resolve its FName
            for (idx, row) in rows.iter().enumerate() {
                if row.as_str() == Some("Resource_SewingKit") {
                    println!("\nResource_SewingKit is row {idx} in MasterItemList");
                    break;
                }
            }
        }
    }

    // try resolving Resource_SewingKit FName by scanning all vendor entries
    // across all vendors for any entry that already sells/buys it
    println!("\nScanning all vendors for any existing SewingKit entry...");
    for v2 in &vendors {
        let Some(cs) = get_component_selector(&api, &v2.selector) else { continue };
        if let Some(sells) = read_sell_entries(&api, &cs) {
            for (i, e) in sells.iter().enumerate() {
                if e.item_name.contains("SewingKit") {
                    println!("  FOUND in {}'s sell list[{i}]: fname_idx={:#x} fname_num={}", v2.name, e.fname_idx, e.fname_num);
                }
            }
        }
        if let Some(buys) = read_buy_entries(&api, &cs) {
            for (i, e) in buys.iter().enumerate() {
                if e.item_name.contains("SewingKit") {
                    println!("  FOUND in {}'s buy list[{i}]: fname_idx={:#x} fname_num={}", v2.name, e.fname_idx, e.fname_num);
                }
            }
        }
    }

    println!("\nDone. Key question: can we use the same DataTable pointer ({:#x}) with Resource_SewingKit's FName,", 0x239cff8cf00u64);
    println!("even though the item is in MasterItemList not ItemList?");
    println!("If MasterItemList is a CompositeDataTable wrapping ItemList, the pointer might resolve correctly.");
}

/// Read-only: resolve GMalloc via patternsleuth and dump the
/// vtable entries for inspection. No writes, no allocation.
#[test]
fn inspect_gmalloc_vtable() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let slow_api: Api = Api::at(17176, "/debug").with_timeout(Duration::from_secs(120));
    println!("resolving GMalloc via patternsleuth (up to 120s)...");
    let r = slow_api.try_op("inspect_gmalloc", json!({}));
    match r {
        Ok(r) if r.ok => {
            println!("{}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
        }
        Ok(r) => {
            println!("ERROR: {:?}", r.error);
        }
        Err(e) => {
            println!("NETWORK ERROR (game may have crashed): {e}");
        }
    }
}

/// Grow ResourseSaler's buy list from max=14 to max=30 via
/// GMalloc->Realloc, then add Resource_SewingKit as entry 15.
///
/// Step 1: grow the TArray via tarray_grow op
/// Step 2: get Resource_SewingKit's FName via list_row_fnames
/// Step 3: clone an existing buy entry as template
/// Step 4: write the new entry with SewingKit's FName
/// Step 5: bump num by 1
#[test]
fn add_sewingkit_to_resourcesaler_buy_list() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let vendors = find_all_vendors(&api);
    let v = vendors.iter().find(|v| v.name.contains("Resourse"));
    let Some(v) = v else {
        println!("ResourseSaler not found");
        return;
    };
    let Some(comp_sel) = get_component_selector(&api, &v.selector) else {
        println!("no vendor component");
        return;
    };
    println!("ResourseSaler component: {comp_sel}");

    // read buy list header
    let buy_hdr = read_tarray_header(&api, &comp_sel, BUY_LIST_OFFSET).unwrap();
    println!("buy list before: ptr={:#x} num={} max={}", buy_hdr.ptr, buy_hdr.num, buy_hdr.max);

    // check if SewingKit is already there
    if let Some(buys) = read_buy_entries(&api, &comp_sel) {
        for e in &buys {
            if e.item_name.contains("SewingKit") {
                println!("Resource_SewingKit already in buy list, skipping");
                return;
            }
        }
    }

    // patternsleuth scans the full 134MB exe, needs a long timeout
    let slow_api: Api = Api::at(17176, "/debug").with_timeout(Duration::from_secs(120));

    // step 1a: inspect GMalloc (read-only, safe)
    println!("resolving GMalloc via patternsleuth (up to 120s)...");
    let r = slow_api.try_op("inspect_gmalloc", json!({}));
    match r {
        Ok(r) if r.ok => {
            println!("GMalloc inspection: {}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
        }
        Ok(r) => {
            println!("inspect_gmalloc error: {:?}", r.error);
            return;
        }
        Err(e) => {
            println!("inspect_gmalloc network error (game may have crashed): {e}");
            return;
        }
    }

    // step 1b: grow the TArray if needed
    if buy_hdr.num >= buy_hdr.max {
        println!("buy list is full (num=max={}), growing to 30...", buy_hdr.max);
        let r = slow_api.try_op("tarray_grow", json!({
            "instance_selector": comp_sel,
            "offset": BUY_LIST_OFFSET,
            "stride": BUY_STRIDE,
            "new_max": 30
        }));
        match r {
            Ok(r) if r.ok => {
                println!("tarray_grow result: {}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
            }
            Ok(r) => {
                println!("tarray_grow FAILED: {:?}", r.error);
                return;
            }
            Err(e) => {
                println!("tarray_grow network error (game may have crashed): {e}");
                return;
            }
        }
    }

    // re-read header after grow
    let buy_hdr = read_tarray_header(&api, &comp_sel, BUY_LIST_OFFSET).unwrap();
    println!("buy list after grow: ptr={:#x} num={} max={}", buy_hdr.ptr, buy_hdr.num, buy_hdr.max);
    assert!(buy_hdr.num < buy_hdr.max, "still no room after grow");

    // step 2: get Resource_SewingKit's FName
    let r = api.op("list_row_fnames", json!({"table_name": "MasterItemList"}));
    if !r.ok {
        println!("list_row_fnames failed: {:?}", r.error);
        return;
    }
    let rows = r.result["rows"].as_array().cloned().unwrap_or_default();
    let sewing = rows.iter().find(|r| r["name"].as_str() == Some("Resource_SewingKit"));
    let Some(sewing) = sewing else {
        println!("Resource_SewingKit not found in MasterItemList");
        return;
    };
    let sewing_fname_idx = sewing["fname_idx"].as_u64().unwrap() as u32;
    let sewing_fname_num = sewing["fname_num"].as_u64().unwrap() as u32;
    println!("Resource_SewingKit FName: idx={sewing_fname_idx:#x} num={sewing_fname_num}");

    // step 3: clone buy entry 0 as template (0x40 bytes)
    let buy_arr_sel = format!("addr:{:#x}", buy_hdr.ptr);
    let template = read_bytes(&api, &buy_arr_sel, 0, BUY_STRIDE).unwrap();
    println!("template entry: {} bytes from buy[0]", template.len());

    // step 4: build new entry, overwrite FName at 0x08..0x10
    let mut new_entry = template.clone();
    new_entry[0x08..0x0C].copy_from_slice(&sewing_fname_idx.to_le_bytes());
    new_entry[0x0C..0x10].copy_from_slice(&sewing_fname_num.to_le_bytes());
    // set stock to 1 at offset 0x28
    new_entry[0x28..0x2C].copy_from_slice(&1i32.to_le_bytes());

    // write new entry at slot [num]
    let new_offset = (buy_hdr.num as u64) * BUY_STRIDE;
    let hex: String = new_entry.iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op("write_bytes", json!({
        "instance_selector": buy_arr_sel,
        "offset": new_offset,
        "bytes_hex": hex
    }));
    if !r.ok {
        println!("write_bytes failed: {:?}", r.error);
        return;
    }
    println!("wrote new entry at offset {new_offset:#x}");

    // step 5: bump num
    let new_count: i32 = buy_hdr.num + 1;
    let count_hex: String = new_count.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op("write_bytes", json!({
        "instance_selector": comp_sel,
        "offset": BUY_LIST_OFFSET + 8,
        "bytes_hex": count_hex
    }));
    if !r.ok {
        println!("write num failed: {:?}", r.error);
        return;
    }
    println!("bumped num to {new_count}");

    // verify
    let verify_hdr = read_tarray_header(&api, &comp_sel, BUY_LIST_OFFSET).unwrap();
    println!("verify: num={} max={}", verify_hdr.num, verify_hdr.max);

    let verify_arr_sel = format!("addr:{:#x}", verify_hdr.ptr);
    let last = read_bytes(&api, &verify_arr_sel, new_offset, BUY_STRIDE).unwrap();
    let verify_idx = read_u32_le(&last, 0x08);
    let verify_num = read_u32_le(&last, 0x0C);
    let verify_name = try_fname(&api, verify_idx, verify_num);
    println!("last entry: {verify_name}");
    assert!(verify_name.contains("SewingKit"), "verify failed: got {verify_name}");
    println!("SUCCESS: Resource_SewingKit added to ResourseSaler buy list");
}

/// Dump the complete ItemList DataTable row names and write them
/// to docs/itemlist.md. Uses the lightweight list_row_names
/// op (no field decoding, fast).
#[test]
fn dump_full_itemlist() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    let r = api.op("list_row_names", json!({"table_name": "ItemList"}));
    if !r.ok {
        println!("list_row_names error: {:?}", r.error);
        return;
    }
    let rows = r.result["rows"].as_array().cloned().unwrap_or_default();
    println!("ItemList: {} rows", rows.len());

    let mut names: Vec<String> = rows.iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    names.sort();

    let mut out = String::from("# MISERY ItemList\n\n");
    out.push_str(&format!("{} items total, dumped from live game 2026-08-17.\n\n", names.len()));

    let mut current_prefix = String::new();
    for name in &names {
        let prefix = name.split('_').next().unwrap_or("Other");
        if prefix != current_prefix {
            current_prefix = prefix.to_string();
            out.push_str(&format!("\n## {prefix}\n\n"));
        }
        out.push_str(&format!("- {name}\n"));
    }

    let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/itemlist.md"));
    std::fs::write(path, &out).expect("failed to write itemlist doc");
    println!("wrote {} items to {}", names.len(), path.display());
}
