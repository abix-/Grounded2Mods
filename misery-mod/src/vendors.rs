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
use ueforge::ue::{read_at, tarray};
const VENDOR_COMP_OFFSET: usize = 0x3B8;
const SELL_LIST_OFFSET: usize = 0x2E8;
const SELL_STRIDE: usize = 0x38;
const BUY_LIST_OFFSET: usize = 0x2D8;
const BUY_STRIDE: usize = 0x40;
/// Added sell entries pay this percent of the item's vanilla buy
/// cost on the same vendor. Vanilla pays 15 to 50 percent.
const SELL_PRICE_PCT: i32 = 40;

/// Every vendor entry, sell or buy or price, starts with an
/// `FName` naming the item at this offset.
const ITEM_NAME_OFFSET: usize = 0x08;

/// The currency a ruble price is paid in. The Technician barters
/// in weapon parts instead, and those entries are skipped.
const RUBLES: &str = "Resource_Rubles";

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
    let header = unsafe { comp.add(SELL_LIST_OFFSET) };
    for (_i, elem) in unsafe { tarray::iter_stride(header, SELL_STRIDE) } {
        if let Some(name) = unsafe { ue::fname::read_at(elem, ITEM_NAME_OFFSET) } {
            names.insert(name);
        }
    }
    names
}

/// Vanilla cost of each ruble-priced item on this vendor's buy
/// list, from price element 0's quantity (research.md 24.6).
/// Stays here because rubles and the buy-entry offsets are MISERY economy facts.
fn buy_costs(comp: *const u8) -> HashMap<String, i32> {
    let mut costs = HashMap::new();
    let header = unsafe { comp.add(BUY_LIST_OFFSET) };
    for (_i, elem) in unsafe { tarray::iter_stride(header, BUY_STRIDE) } {
        let Some(name) = (unsafe { ue::fname::read_at(elem, ITEM_NAME_OFFSET) }) else {
            continue;
        };
        let price_ptr: *const u8 = unsafe { read_at(elem, 0x18) };
        let price_num: i32 = unsafe { read_at(elem, 0x20) };
        if price_ptr.is_null() || price_num < 1 {
            continue;
        }
        // Skip barter prices (Technician pays in weapon parts).
        let currency = unsafe { ue::fname::read_at(price_ptr, ITEM_NAME_OFFSET) };
        if currency.as_deref() != Some(RUBLES) {
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
/// quantity at +0x10.
///
/// The buffer comes from the ENGINE's allocator, not Rust's.
///
/// This used to be `std::alloc::alloc_zeroed`, on the belief that
/// "UE never frees vendor price arrays" (research.md 24.12). That
/// belief was wrong, and the disproof is a crash: going to the
/// main menu after a vendor pass killed the game with
///
/// ```text
/// FMallocBinned2 Attempt to realloc an unrecognized block
/// canary == 0x1e != 0xe3
/// ```
///
/// `0x1e` is whatever Rust's allocator left in front of the
/// block. The engine DOES tear these arrays down, and a pointer
/// it never handed out fails its own canary check.
/// Stays here because this clones MISERY's verified vendor-price layout, not a generic Unreal collection.
fn set_custom_price(entry: &mut [u8], template_price_ptr: *const u8, qty: i32) {
    // One price element: 0x18 bytes.
    let Some(buf) = ue::gmalloc::alloc_zeroed(0x18, ue::gmalloc::DEFAULT_ALIGNMENT) else {
        // The engine allocator is not reachable. Leaving the entry
        // pointing at the template's price is wrong but harmless;
        // handing the engine a Rust buffer is what crashes it.
        return;
    };
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
        if apply_entries::<SELL_STRIDE>(*comp, SELL_LIST_OFFSET, "vendor_mirror", &new) {
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
        if apply_entries::<SELL_STRIDE>(comp, SELL_LIST_OFFSET, "vendor_ammo", &new) {
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
        apply_entries::<SELL_STRIDE>(comp, SELL_LIST_OFFSET, "vendor_food", &new);
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
    apply_entries::<BUY_STRIDE>(comp, BUY_LIST_OFFSET, "vendor_sewingkit", &new);
}

/// Item name + FName index of every entry on this vendor's buy
/// list. The FName comes straight from the buy entry, so no
/// DataTable lookup is needed.
/// Stays here because it reads MISERY's verified vendor-entry layout.
fn buy_entry_names(comp: *const u8) -> Vec<(String, u32)> {
    let mut items = Vec::new();
    let header = unsafe { comp.add(BUY_LIST_OFFSET) };
    for (_i, elem) in unsafe { tarray::iter_stride(header, BUY_STRIDE) } {
        if let Some((id, name)) = unsafe { ue::fname::read_with_id(elem, ITEM_NAME_OFFSET) } {
            items.push((name, id));
        }
    }
    items
}

/// Applies planned offers to the vendor list at `list_offset`.
/// Ueforge grows the array and clones its template entry.
/// Stays here because MISERY owns the item, price, and stock byte patches.
fn apply_entries<const STRIDE: usize>(
    comp: *mut u8,
    list_offset: usize,
    label: &str,
    entries: &[VendorOffer<u32>],
) -> bool {
    if entries.is_empty() {
        return false;
    }

    let mut priced = 0usize;
    // SAFETY: comp is a live BP_VendorComponent_C; list_offset is one of its
    // verified TArray headers, and every vanilla list has a template entry.
    let result = unsafe {
        tarray::append_cloned_raw(
            comp.add(list_offset),
            STRIDE,
            entries.len(),
            10,
            |index, template, entry| {
                let e = &entries[index];
                entry[0x08..0x0C].copy_from_slice(&e.id.to_le_bytes());
                entry[0x0C..0x10].copy_from_slice(&0u32.to_le_bytes());
                if let Some(pay) = e.price {
                    let template_price_ptr =
                        u64::from_le_bytes(template[0x18..0x20].try_into().unwrap()) as *const u8;
                    if !template_price_ptr.is_null() {
                        set_custom_price(entry, template_price_ptr, pay);
                        priced += 1;
                    }
                }
                if let Some(stock) = e.stock {
                    entry[0x28..0x2C].copy_from_slice(&stock.to_le_bytes());
                }
            },
        )
    };
    let appended = match result {
        Ok(appended) => appended,
        Err(error) => {
            ueforge::log::log(format_args!("{label}: {error}"));
            return false;
        }
    };
    ueforge::log::log(format_args!(
        "{label}: added {} items ({priced} custom priced)",
        appended
    ));
    true
}
