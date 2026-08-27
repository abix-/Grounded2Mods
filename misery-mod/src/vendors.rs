//! Vendor sell list expansion.
//!
//! Uses `ueforge::ue::actor::on_each_load` to re-apply after
//! returning to main menu and loading a new save.
//!
//! New entries clone sell entry 0 as a template and get their
//! own price array paying SELL_PRICE_PCT of the item's vanilla
//! buy cost on the same vendor. Items the vendor does not sell
//! keep the template's price. Vanilla entries are never changed.

use modforge::vendor::{OfferPlanner, VendorItem, VendorOffer, special_offer};
use std::collections::{HashMap, HashSet};
use ueforge::ue;
use ueforge::ue::{TArray, read_at, tarray};
const VENDOR_COMP_OFFSET: usize = 0x3B8;
const SELL_LIST_OFFSET: usize = 0x2E8;
const SELL_STRIDE: usize = 0x38;
const BUY_LIST_OFFSET: usize = 0x2D8;
const BUY_STRIDE: usize = 0x40;
/// Added sell entries pay this percent of the item's vanilla buy
/// cost on the same vendor. Vanilla pays 15 to 50 percent.
const SELL_PRICE_PCT: i32 = 40;

const ALL_FOOD_SELLABLE: &[&str] = &[
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
    "Food_SeedsCarrot",
    "Food_SeedsCucumber",
    "Food_SeedsTomato",
    "Food_SeedsWheat",
];

/// Finds a MISERY vendor's sell-list component so the mod can expand what it buys.
/// Stays here because the component offset is specific to this game's vendor Blueprint.
fn sell_list_ptr(actor: *const u8) -> Option<*mut u8> {
    unsafe { ue::follow_ptr_chain(actor, &[VENDOR_COMP_OFFSET]) }
        .ok()
        .map(|p| p as *mut u8)
}

/// Collects every item a MISERY vendor already accepts from the player.
/// Stays here because the entry layout and item naming come from this game's vendor data.
fn current_sell_names(comp: *const u8) -> HashSet<String> {
    let mut names = HashSet::new();
    let rt = match ue::try_runtime() {
        Some(r) => r,
        None => return names,
    };
    let header = unsafe { comp.add(SELL_LIST_OFFSET) };
    for (_i, elem) in unsafe { tarray::iter_stride(header, SELL_STRIDE) } {
        let fname_idx: u32 = unsafe { read_at(elem, 0x08) };
        let fname_num: u32 = unsafe { read_at(elem, 0x0C) };
        let raw: u64 = (fname_idx as u64) | ((fname_num as u64) << 32);
        let fname = ue::FName::from_u64(raw);
        names.insert(unsafe { rt.name_resolver.to_string(fname) });
    }
    names
}

/// Vanilla cost of each ruble-priced item on this vendor's buy
/// list, from price element 0's quantity (research.md 24.6).
/// Stays here because rubles and the buy-entry offsets are MISERY economy facts.
fn buy_costs(comp: *const u8) -> HashMap<String, i32> {
    let mut costs = HashMap::new();
    let Some(rt) = ue::try_runtime() else {
        return costs;
    };
    let header = unsafe { comp.add(BUY_LIST_OFFSET) };
    for (_i, elem) in unsafe { tarray::iter_stride(header, BUY_STRIDE) } {
        let fname_idx: u32 = unsafe { read_at(elem, 0x08) };
        let fname_num: u32 = unsafe { read_at(elem, 0x0C) };
        let raw = (fname_idx as u64) | ((fname_num as u64) << 32);
        let name = unsafe { rt.name_resolver.to_string(ue::FName::from_u64(raw)) };
        let price_ptr: *const u8 = unsafe { read_at(elem, 0x18) };
        let price_num: i32 = unsafe { read_at(elem, 0x20) };
        if price_ptr.is_null() || price_num < 1 {
            continue;
        }
        // Skip barter prices (Technician pays in weapon parts).
        let cur_idx: u32 = unsafe { read_at(price_ptr, 0x08) };
        let cur_num: u32 = unsafe { read_at(price_ptr, 0x0C) };
        let cur_raw = (cur_idx as u64) | ((cur_num as u64) << 32);
        let currency = unsafe { rt.name_resolver.to_string(ue::FName::from_u64(cur_raw)) };
        if currency != "Resource_Rubles" {
            continue;
        }
        let qty: i32 = unsafe { read_at(price_ptr, 0x10) };
        if qty > 0 {
            costs.insert(name, qty);
        }
    }
    costs
}

/// Give an entry its own single-element price array paying `qty`
/// rubles: clone the template's price element and swap the
/// quantity at +0x10. The allocation is leaked on purpose; UE
/// never frees vendor price arrays (research.md 24.12).
/// Stays here because this clones MISERY's verified vendor-price layout, not a generic Unreal collection.
fn set_custom_price(entry: &mut [u8], template_price_ptr: *const u8, qty: i32) {
    // SAFETY: fixed 0x18-byte layout with 8-byte alignment.
    let buf =
        unsafe { std::alloc::alloc_zeroed(std::alloc::Layout::from_size_align(0x18, 8).unwrap()) };
    if buf.is_null() {
        return;
    }
    // SAFETY: template_price_ptr points at a live 0x18-byte price
    // element read from the vendor's own sell list.
    unsafe {
        std::ptr::copy_nonoverlapping(template_price_ptr, buf, 0x18);
        (buf.add(0x10) as *mut i32).write(qty);
    }
    entry[0x18..0x20].copy_from_slice(&(buf as u64).to_le_bytes());
    entry[0x20..0x24].copy_from_slice(&1i32.to_le_bytes());
    entry[0x24..0x28].copy_from_slice(&1i32.to_le_bytes());
}

/// Resolves selected MISERY item names to the identifiers required by vendor entries.
/// Stays here because it queries this game's MasterItemList and applies feature-specific filters.
fn resolve_item_fnames(wanted: &dyn Fn(&str) -> bool) -> HashMap<String, u32> {
    let Some(table) = ue::datatable::find_by_short_name("MasterItemList") else {
        return HashMap::new();
    };
    let full_map = unsafe { ue::datatable::row_name_map(table) };
    full_map
        .into_iter()
        .filter(|(name, _)| wanted(name))
        .map(|(name, key)| (name, (key & 0xFFFF_FFFF) as u32))
        .collect()
}

/// Serializes the vendor pass; on_each_load fires it from its
/// own thread.
static VENDOR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// One pass over every live vendor, applied on each load.
/// Precedence: vanilla sell lists first, then each vendor's buy
/// mirror, then the GunDealer ammo rule, then the Barman food
/// list. Modforge assigns every item at most once after each
/// successful Unreal list mutation.
/// Stays here because vendor discovery, role order, and applying offers are MISERY integration.
pub fn apply_all(_first: *const u8) {
    let _guard = VENDOR_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let actors = ueforge::ue::actor::find_actors_by_chain("BP_MasterVendorBuildPart_C");
    let comps: Vec<*mut u8> = actors.iter().filter_map(|a| sell_list_ptr(*a)).collect();
    ueforge::log::log(format_args!("vendors: {} vendor(s) found", comps.len()));

    let sellable: HashSet<String> = comps.iter().flat_map(|c| current_sell_names(*c)).collect();
    let mut planner = OfferPlanner::new(sellable, SELL_PRICE_PCT);

    // Buy mirror: everything a vendor sells, he also buys back,
    // at SELL_PRICE_PCT of his own charge price.
    for comp in &comps {
        let costs = buy_costs(*comp);
        let new = planner.plan(
            buy_entry_names(*comp)
                .into_iter()
                .map(|(name, fname_idx)| VendorItem {
                    name,
                    id: fname_idx,
                }),
            &costs,
        );
        if append_entries::<SELL_STRIDE>(*comp, SELL_LIST_OFFSET, "vendor_mirror", &new) {
            planner.commit(&new);
        }
    }

    // GunDealer: buys every ammo and magazine type in the game.
    if let Some(comp) = find_vendor_comp("BP_GunDealerReal_C") {
        let costs = buy_costs(comp);
        let new = planner.plan(
            resolve_item_fnames(&|n| n.starts_with("Ammo_") || n.starts_with("Magazine_"))
                .into_iter()
                .map(|(name, fname_idx)| VendorItem {
                    name,
                    id: fname_idx,
                }),
            &costs,
        );
        if append_entries::<SELL_STRIDE>(comp, SELL_LIST_OFFSET, "vendor_ammo", &new) {
            planner.commit(&new);
        }
    }

    // Barman: buys every edible food not spoken for elsewhere.
    if let Some(comp) = find_vendor_comp("BP_Barman_C") {
        let costs = buy_costs(comp);
        let new = planner.plan(
            resolve_item_fnames(&|n| ALL_FOOD_SELLABLE.contains(&n))
                .into_iter()
                .map(|(name, fname_idx)| VendorItem {
                    name,
                    id: fname_idx,
                }),
            &costs,
        );
        append_entries::<SELL_STRIDE>(comp, SELL_LIST_OFFSET, "vendor_food", &new);
    }

    // ResourseSaler: permanently sells the sewing kit.
    if let Some(comp) = find_vendor_comp("BP_ResourseSaler_C") {
        add_sewing_kit(comp);
    }
}

/// Finds the live component for one named MISERY vendor type.
/// Stays here because callers select game-specific vendor Blueprint classes.
fn find_vendor_comp(class_name: &str) -> Option<*mut u8> {
    ueforge::ue::actor::find_actor(class_name, None).and_then(sell_list_ptr)
}

/// The sewing kit is needed for crafting but vanilla never sells
/// it (research.md 24.11). Sold by the ResourseSaler.
const SEWING_KIT_COST: i32 = 50;

/// Adds the missing sewing kit to its intended MISERY resource seller.
/// Stays here because the item, vendor, stock, and price are specific game-content choices.
fn add_sewing_kit(comp: *mut u8) {
    let existing: HashSet<String> = buy_entry_names(comp)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    if existing.contains("Resource_SewingKit") {
        return;
    }
    let fnames = resolve_item_fnames(&|n| n == "Resource_SewingKit");
    let Some((name, fname_idx)) = fnames.into_iter().next() else {
        ueforge::log::log(format_args!("vendor_sewingkit: not in MasterItemList"));
        return;
    };
    let Some(offer) = special_offer(
        &existing,
        VendorItem {
            name,
            id: fname_idx,
        },
        Some(SEWING_KIT_COST),
        Some(1),
    ) else {
        return;
    };
    let new = [offer];
    append_entries::<BUY_STRIDE>(comp, BUY_LIST_OFFSET, "vendor_sewingkit", &new);
}

/// Item name + FName index of every entry on this vendor's buy
/// list. The FName comes straight from the buy entry, so no
/// DataTable lookup is needed.
/// Stays here because it reads MISERY's verified vendor-entry layout.
fn buy_entry_names(comp: *const u8) -> Vec<(String, u32)> {
    let mut items = Vec::new();
    let Some(rt) = ue::try_runtime() else {
        return items;
    };
    let header = unsafe { comp.add(BUY_LIST_OFFSET) };
    for (_i, elem) in unsafe { tarray::iter_stride(header, BUY_STRIDE) } {
        let fname_idx: u32 = unsafe { read_at(elem, 0x08) };
        let fname_num: u32 = unsafe { read_at(elem, 0x0C) };
        let raw = (fname_idx as u64) | ((fname_num as u64) << 32);
        let name = unsafe { rt.name_resolver.to_string(ue::FName::from_u64(raw)) };
        items.push((name, fname_idx));
    }
    items
}

/// Append entries to the vendor list at `list_offset` (sell or
/// buy; STRIDE picks the layout), growing the TArray when
/// needed. Entry 0 is cloned as the template; vanilla entries
/// are never modified. Returns true when everything was written.
/// Stays here because both supported layouts and their ownership behavior were measured in MISERY.
fn append_entries<const STRIDE: usize>(
    comp: *mut u8,
    list_offset: usize,
    label: &str,
    entries: &[VendorOffer<u32>],
) -> bool {
    if entries.is_empty() {
        return false;
    }

    // SAFETY: comp is a live BP_VendorComponent_C; list_offset
    // is one of the two documented TArray headers on it.
    let arr = unsafe { &mut *(comp.add(list_offset) as *mut TArray<[u8; STRIDE]>) };
    let total_needed = arr.num + entries.len() as i32;
    if total_needed > arr.max {
        // SAFETY: grow copies old entries into a fresh Rust
        // allocation; the old buffer is leaked on purpose.
        if let Err(e) = unsafe { arr.grow(total_needed + 10) } {
            ueforge::log::log(format_args!("{label}: grow failed: {e}"));
            return false;
        }
    }
    let data = arr.data as *mut u8;
    if data.is_null() {
        ueforge::log::log(format_args!("{label}: null list pointer"));
        return false;
    }

    // clone entry 0 as template
    let mut template = vec![0u8; STRIDE];
    // SAFETY: every vanilla vendor list has at least one entry.
    unsafe {
        std::ptr::copy_nonoverlapping(data, template.as_mut_ptr(), STRIDE);
    }
    let template_price_ptr =
        u64::from_le_bytes(template[0x18..0x20].try_into().unwrap()) as *const u8;

    let mut num = arr.num;
    let mut priced = 0usize;
    for e in entries {
        let mut entry = template.clone();
        entry[0x08..0x0C].copy_from_slice(&e.id.to_le_bytes());
        entry[0x0C..0x10].copy_from_slice(&0u32.to_le_bytes());
        if let Some(pay) = e.price {
            if !template_price_ptr.is_null() {
                set_custom_price(&mut entry, template_price_ptr, pay);
                priced += 1;
            }
        }
        if let Some(stock) = e.stock {
            entry[0x28..0x2C].copy_from_slice(&stock.to_le_bytes());
        }
        // SAFETY: grow above guarantees num < max, so this slot
        // is inside the allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(
                entry.as_ptr(),
                data.add((num as usize) * STRIDE),
                STRIDE,
            );
        }
        num += 1;
    }

    arr.num = num;
    ueforge::log::log(format_args!(
        "{label}: added {} items ({priced} custom priced)",
        entries.len()
    ));
    true
}
