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
use common::{api_or_skip, offsets_live};
use modforge::client::{self, ClassInstance};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

type Api = common::Api;

const VENDOR_ACTOR_CLASS: &str = "BP_MasterVendorBuildPart_C";
const VENDOR_COMP_OFFSET: u64 = 0x3B8;
const BUY_LIST_OFFSET: u64 = 0x2D8;
const SELL_LIST_OFFSET: u64 = 0x2E8;
const SELL_STRIDE: u64 = 0x38;
const BUY_STRIDE: u64 = 0x40;

fn find_all_vendors(api: &Api) -> Vec<ClassInstance> {
    client::walk_class_instances(api, VENDOR_ACTOR_CLASS, 100)
}

fn get_component_addr(api: &Api, actor_addr: u64) -> Option<u64> {
    client::read_component_ptr(api, actor_addr, VENDOR_COMP_OFFSET)
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

fn read_sell_entries(api: &Api, comp_addr: u64) -> Option<Vec<SellEntry>> {
    let hdr = client::read_tarray_header(api, comp_addr, SELL_LIST_OFFSET)?;
    if hdr.num <= 0 || hdr.num > 500 { return None; }
    let data = client::read_bytes(api, hdr.ptr, 0, (hdr.num as u64) * SELL_STRIDE);
    if data.is_empty() { return None; }
    let mut entries = Vec::new();
    for i in 0..hdr.num as usize {
        let base = i * SELL_STRIDE as usize;
        let item_ptr = client::from_le_u64(&data, base);
        let fname_idx = client::from_le_u32(&data, base + 0x08);
        let fname_num = client::from_le_u32(&data, base + 0x0C);
        let item_name = client::fname_from_parts(api, fname_idx, fname_num)
            .unwrap_or_else(|| format!("(?idx={fname_idx:#x} num={fname_num})"));
        let price_num = client::from_le_i32(&data, base + 0x20);
        let cat_num = client::from_le_i32(&data, base + 0x30);
        entries.push(SellEntry { item_ptr, fname_idx, fname_num, item_name, price_num, cat_num });
    }
    Some(entries)
}

fn read_buy_entries(api: &Api, comp_addr: u64) -> Option<Vec<BuyEntry>> {
    let hdr = client::read_tarray_header(api, comp_addr, BUY_LIST_OFFSET)?;
    if hdr.num <= 0 || hdr.num > 500 { return None; }
    let data = client::read_bytes(api, hdr.ptr, 0, (hdr.num as u64) * BUY_STRIDE);
    if data.is_empty() { return None; }
    let mut entries = Vec::new();
    for i in 0..hdr.num as usize {
        let base = i * BUY_STRIDE as usize;
        let item_ptr = client::from_le_u64(&data, base);
        let fname_idx = client::from_le_u32(&data, base + 0x08);
        let fname_num = client::from_le_u32(&data, base + 0x0C);
        let item_name = client::fname_from_parts(api, fname_idx, fname_num)
            .unwrap_or_else(|| format!("(?idx={fname_idx:#x} num={fname_num})"));
        let price_num = client::from_le_i32(&data, base + 0x20);
        let stock = client::from_le_i32(&data, base + 0x28);
        let cat_num = client::from_le_i32(&data, base + 0x38);
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
        println!("  {} @ {}", v.name, v.addr_selector);
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
        let Some(comp_addr) = get_component_addr(&api, v.addr) else {
            println!("  (no vendor component)\n");
            continue;
        };

        if let Some(sells) = read_sell_entries(&api, comp_addr) {
            println!("  SELL LIST (vendor buys these from player): {} entries", sells.len());
            for (i, e) in sells.iter().enumerate() {
                println!("    {i:>2}. {:<40} prices={} cats={}", e.item_name, e.price_num, e.cat_num);
            }
        } else {
            println!("  SELL LIST: could not read");
        }

        if let Some(buys) = read_buy_entries(&api, comp_addr) {
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
    let _ = item_list_addr;

    let vendors = find_all_vendors(&api);
    let Some(v) = vendors.first() else {
        println!("no vendors");
        return;
    };
    let Some(comp_addr) = get_component_addr(&api, v.addr) else {
        println!("no component");
        return;
    };
    let hdr = client::read_tarray_header(&api, comp_addr, SELL_LIST_OFFSET);
    let Some(hdr) = hdr else { return; };
    if hdr.num <= 0 { return; }
    let first_entry = client::read_bytes(&api, hdr.ptr, 0, 0x18);
    if first_entry.len() < 0x18 { return; }
    let item_ptr = client::from_le_u64(&first_entry, 0);
    println!("first sell entry item_ptr = {item_ptr:#x}");

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

    let vendors = find_all_vendors(&api);
    let Some(v) = vendors.first() else { return; };
    let Some(comp_addr) = get_component_addr(&api, v.addr) else { return; };
    if let Some(sells) = read_sell_entries(&api, comp_addr) {
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
    let Some(comp_addr) = get_component_addr(&api, v.addr) else { return; };

    let sell_hdr = client::read_tarray_header(&api, comp_addr, SELL_LIST_OFFSET);
    let Some(sell_hdr) = sell_hdr else { return; };
    if sell_hdr.num <= 0 { return; }
    let entry0 = client::read_bytes(&api, sell_hdr.ptr, 0, SELL_STRIDE);
    if entry0.len() < SELL_STRIDE as usize { return; }

    let price_ptr = client::from_le_u64(&entry0, 0x18);
    let price_num = client::from_le_i32(&entry0, 0x20);
    println!("sell entry 0 price: ptr={price_ptr:#x} num={price_num}");

    if price_num > 0 && price_num < 20 {
        let price_data = client::read_bytes(&api, price_ptr, 0, 128);
        if price_data.is_empty() {
            println!("could not read price data");
            return;
        }
        println!("price array raw hex ({} bytes):", price_data.len());
        for (i, chunk) in price_data.chunks(16).enumerate() {
            let offset = i * 16;
            let hex_str: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("  {offset:04x}: {}", hex_str.join(" "));
        }
        for off in (0..32).step_by(4) {
            if off + 4 > price_data.len() { break; }
            let val = client::from_le_u32(&price_data, off);
            let name = client::fname_from_parts(&api, val, 0)
                .unwrap_or_else(|| format!("(?idx={val:#x})"));
            println!("  price[{off:#x}] u32={val:#x} fname={name}");
        }
    }

    let buy_hdr = client::read_tarray_header(&api, comp_addr, BUY_LIST_OFFSET);
    let Some(buy_hdr) = buy_hdr else { return; };
    if buy_hdr.num <= 0 { return; }
    let buy_entry0 = client::read_bytes(&api, buy_hdr.ptr, 0, BUY_STRIDE);
    if buy_entry0.len() < BUY_STRIDE as usize { return; }

    let buy_price_ptr = client::from_le_u64(&buy_entry0, 0x18);
    let buy_price_num = client::from_le_i32(&buy_entry0, 0x20);
    println!("\nbuy entry 0 price: ptr={buy_price_ptr:#x} num={buy_price_num}");

    if buy_price_num > 0 && buy_price_num < 20 {
        let price_data = client::read_bytes(&api, buy_price_ptr, 0, 256);
        if price_data.is_empty() { return; }
        println!("buy price array raw hex ({} bytes):", price_data.len());
        for (i, chunk) in price_data.chunks(16).enumerate() {
            let offset = i * 16;
            let hex_str: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("  {offset:04x}: {}", hex_str.join(" "));
        }
        for off in (0..64).step_by(4) {
            if off + 4 > price_data.len() { break; }
            let val = client::from_le_u32(&price_data, off);
            let name = client::fname_from_parts(&api, val, 0)
                .unwrap_or_else(|| format!("(?idx={val:#x})"));
            println!("  buy_price[{off:#x}] u32={val:#x} fname={name}");
        }
    }
}

/// Summary: unique item pointers across all vendors.
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
        let Some(comp_addr) = get_component_addr(&api, v.addr) else { continue; };
        if let Some(sells) = read_sell_entries(&api, comp_addr) {
            for e in &sells {
                all_ptrs.entry(e.item_ptr).or_default().push(format!("{} sell", v.name));
            }
        }
        if let Some(buys) = read_buy_entries(&api, comp_addr) {
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

/// Add Resource_Plastic to ResourseSaler's sell list.
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
    let Some(comp_addr) = get_component_addr(&api, v.addr) else {
        println!("no vendor component");
        return;
    };
    let comp_sel = format!("addr:0x{comp_addr:X}");
    println!("ResourseSaler component: {comp_sel}");

    let buy_hdr = client::read_tarray_header(&api, comp_addr, BUY_LIST_OFFSET).unwrap();
    let buy_entry1 = client::read_bytes(&api, buy_hdr.ptr, BUY_STRIDE, BUY_STRIDE);
    let plastic_fname_idx = client::from_le_u32(&buy_entry1, 0x08);
    let plastic_fname_num = client::from_le_u32(&buy_entry1, 0x0C);
    let plastic_name = client::fname_from_parts(&api, plastic_fname_idx, plastic_fname_num)
        .unwrap_or_default();
    println!("Resource_Plastic FName: idx={plastic_fname_idx:#x} num={plastic_fname_num} => {plastic_name}");
    assert!(plastic_name.contains("Plastic"), "expected Plastic, got {plastic_name}");

    let sell_hdr = client::read_tarray_header(&api, comp_addr, SELL_LIST_OFFSET).unwrap();
    println!("SellList: ptr={:#x} num={} max={}", sell_hdr.ptr, sell_hdr.num, sell_hdr.max);

    if let Some(sells) = read_sell_entries(&api, comp_addr) {
        for e in &sells {
            if e.item_name.contains("Plastic") {
                println!("Resource_Plastic already in sell list at this index, skipping");
                return;
            }
        }
    }

    let template = client::read_bytes(&api, sell_hdr.ptr, 0, SELL_STRIDE);
    println!("template entry (Resource_Glass): {} bytes", template.len());

    let mut new_entry = template.clone();
    new_entry[0x08..0x0C].copy_from_slice(&plastic_fname_idx.to_le_bytes());
    new_entry[0x0C..0x10].copy_from_slice(&plastic_fname_num.to_le_bytes());

    let new_offset = (sell_hdr.num as u64) * SELL_STRIDE;
    let hex_str: String = new_entry.iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op("write_bytes", json!({
        "instance_selector": format!("addr:0x{:X}", sell_hdr.ptr),
        "offset": new_offset,
        "bytes_hex": hex_str
    }));
    if !r.ok {
        println!("write_bytes failed: {:?}", r.error);
        return;
    }
    println!("wrote new entry at offset {new_offset:#x}");

    let new_count: i32 = sell_hdr.num + 1;
    let count_hex: String = new_count.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op("write_bytes", json!({
        "instance_selector": comp_sel,
        "offset": SELL_LIST_OFFSET + 8,
        "bytes_hex": count_hex
    }));
    if !r.ok {
        println!("write num/max failed: {:?}", r.error);
        return;
    }
    println!("bumped num to {new_count}");

    let verify_hdr = client::read_tarray_header(&api, comp_addr, SELL_LIST_OFFSET).unwrap();
    println!("verify: num={} max={}", verify_hdr.num, verify_hdr.max);

    let last = client::read_bytes(&api, sell_hdr.ptr, new_offset, SELL_STRIDE);
    let verify_idx = client::from_le_u32(&last, 0x08);
    let verify_num = client::from_le_u32(&last, 0x0C);
    let verify_name = client::fname_from_parts(&api, verify_idx, verify_num).unwrap_or_default();
    println!("last entry: {verify_name}");
    assert!(verify_name.contains("Plastic"), "verify failed: got {verify_name}");
    println!("SUCCESS: Resource_Plastic added to ResourseSaler sell list");
}

/// Find Resource_SewingKit's FName and the DataTable it lives in.
#[test]
fn find_sewing_kit_fname() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

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

    let vendors = find_all_vendors(&api);
    let v = vendors.iter().find(|v| v.name.contains("Resourse"));
    let Some(v) = v else {
        println!("ResourseSaler not found");
        return;
    };
    let Some(comp_addr) = get_component_addr(&api, v.addr) else {
        println!("no vendor component");
        return;
    };

    let buy_hdr = client::read_tarray_header(&api, comp_addr, BUY_LIST_OFFSET);
    let Some(buy_hdr) = buy_hdr else {
        println!("could not read buy list header");
        return;
    };
    println!("\nResourseSaler buy list: num={} max={}", buy_hdr.num, buy_hdr.max);

    for i in 0..buy_hdr.num {
        let offset = (i as u64) * BUY_STRIDE;
        let entry = client::read_bytes(&api, buy_hdr.ptr, offset, 0x18);
        if entry.len() < 0x18 { continue; }
        let dt_ptr = client::from_le_u64(&entry, 0);
        let fname_idx = client::from_le_u32(&entry, 0x08);
        let fname_num = client::from_le_u32(&entry, 0x0C);
        let field_10 = client::from_le_u64(&entry, 0x10);
        let name = client::fname_from_parts(&api, fname_idx, fname_num)
            .unwrap_or_else(|| format!("(?idx={fname_idx:#x})"));
        println!("  buy[{i:>2}] dt={dt_ptr:#x} fname_idx={fname_idx:#x} fname_num={fname_num} field_10={field_10:#x} => {name}");
    }

    let sell_hdr = client::read_tarray_header(&api, comp_addr, SELL_LIST_OFFSET);
    if let Some(sh) = sell_hdr {
        println!("\nResourseSaler sell list: num={} max={}", sh.num, sh.max);
    }

    let r = api.try_op("list_row_names", json!({"table_name": "MasterItemList"}));
    if let Ok(r) = r {
        if r.ok {
            let rows = r.result["rows"].as_array().cloned().unwrap_or_default();
            for (idx, row) in rows.iter().enumerate() {
                if row.as_str() == Some("Resource_SewingKit") {
                    println!("\nResource_SewingKit is row {idx} in MasterItemList");
                    break;
                }
            }
        }
    }

    println!("\nScanning all vendors for any existing SewingKit entry...");
    for v2 in &vendors {
        let Some(cs) = get_component_addr(&api, v2.addr) else { continue };
        if let Some(sells) = read_sell_entries(&api, cs) {
            for (i, e) in sells.iter().enumerate() {
                if e.item_name.contains("SewingKit") {
                    println!("  FOUND in {}'s sell list[{i}]: fname_idx={:#x} fname_num={}", v2.name, e.fname_idx, e.fname_num);
                }
            }
        }
        if let Some(buys) = read_buy_entries(&api, cs) {
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

/// Read-only: resolve GMalloc via patternsleuth.
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

/// Grow ResourseSaler's buy list and add Resource_SewingKit.
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
    let Some(comp_addr) = get_component_addr(&api, v.addr) else {
        println!("no vendor component");
        return;
    };
    let comp_sel = format!("addr:0x{comp_addr:X}");
    println!("ResourseSaler component: {comp_sel}");

    let buy_hdr = client::read_tarray_header(&api, comp_addr, BUY_LIST_OFFSET).unwrap();
    println!("buy list before: ptr={:#x} num={} max={}", buy_hdr.ptr, buy_hdr.num, buy_hdr.max);

    if let Some(buys) = read_buy_entries(&api, comp_addr) {
        for e in &buys {
            if e.item_name.contains("SewingKit") {
                println!("Resource_SewingKit already in buy list, skipping");
                return;
            }
        }
    }

    let slow_api: Api = Api::at(17176, "/debug").with_timeout(Duration::from_secs(120));

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

    let buy_hdr = client::read_tarray_header(&api, comp_addr, BUY_LIST_OFFSET).unwrap();
    println!("buy list after grow: ptr={:#x} num={} max={}", buy_hdr.ptr, buy_hdr.num, buy_hdr.max);
    assert!(buy_hdr.num < buy_hdr.max, "still no room after grow");

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

    let template = client::read_bytes(&api, buy_hdr.ptr, 0, BUY_STRIDE);
    println!("template entry: {} bytes from buy[0]", template.len());

    let mut new_entry = template.clone();
    new_entry[0x08..0x0C].copy_from_slice(&sewing_fname_idx.to_le_bytes());
    new_entry[0x0C..0x10].copy_from_slice(&sewing_fname_num.to_le_bytes());
    new_entry[0x28..0x2C].copy_from_slice(&1i32.to_le_bytes());

    let new_offset = (buy_hdr.num as u64) * BUY_STRIDE;
    let hex_str: String = new_entry.iter().map(|b| format!("{b:02x}")).collect();
    let r = api.op("write_bytes", json!({
        "instance_selector": format!("addr:0x{:X}", buy_hdr.ptr),
        "offset": new_offset,
        "bytes_hex": hex_str
    }));
    if !r.ok {
        println!("write_bytes failed: {:?}", r.error);
        return;
    }
    println!("wrote new entry at offset {new_offset:#x}");

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

    let verify_hdr = client::read_tarray_header(&api, comp_addr, BUY_LIST_OFFSET).unwrap();
    println!("verify: num={} max={}", verify_hdr.num, verify_hdr.max);

    let last = client::read_bytes(&api, verify_hdr.ptr, new_offset, BUY_STRIDE);
    let verify_idx = client::from_le_u32(&last, 0x08);
    let verify_num = client::from_le_u32(&last, 0x0C);
    let verify_name = client::fname_from_parts(&api, verify_idx, verify_num).unwrap_or_default();
    println!("last entry: {verify_name}");
    assert!(verify_name.contains("SewingKit"), "verify failed: got {verify_name}");
    println!("SUCCESS: Resource_SewingKit added to ResourseSaler buy list");
}

/// Dump the complete ItemList DataTable row names.
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
