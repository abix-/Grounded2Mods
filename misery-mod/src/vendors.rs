//! Vendor sell list expansion.
//!
//! Uses `ueforge::ue::actor::on_each_load` to re-apply after
//! returning to main menu and loading a new save.

use std::collections::{HashMap, HashSet};
use std::time::Duration;
use ueforge::ue;
use ueforge::ue::{read_at, TArray};

const VENDOR_ACTOR_CLASS: &str = "BP_MasterVendorBuildPart_C";
const VENDOR_COMP_OFFSET: usize = 0x3B8;
const SELL_LIST_OFFSET: usize = 0x2E8;
const SELL_STRIDE: usize = 0x38;

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

fn sell_list_ptr(actor: *const u8) -> Option<*mut u8> {
    unsafe { ue::follow_ptr_chain(actor, &[VENDOR_COMP_OFFSET]) }
        .ok()
        .map(|p| p as *mut u8)
}

fn sell_tarray(comp: *const u8) -> &'static mut TArray<u8> {
    unsafe { &mut *(comp.add(SELL_LIST_OFFSET) as *mut TArray<u8>) }
}

fn current_sell_names(comp: *const u8) -> HashSet<String> {
    let mut names = HashSet::new();
    let rt = match ue::try_runtime() {
        Some(r) => r,
        None => return names,
    };
    let arr = sell_tarray(comp);
    if arr.is_empty() { return names; }
    let data = arr.data;
    for i in 0..(arr.num as usize) {
        let base = i * SELL_STRIDE;
        let fname_idx: u32 = unsafe { read_at(data, base + 0x08) };
        let fname_num: u32 = unsafe { read_at(data, base + 0x0C) };
        let raw: u64 = (fname_idx as u64) | ((fname_num as u64) << 32);
        let fname = ue::FName::from_u64(raw);
        names.insert(unsafe { rt.name_resolver.to_string(fname) });
    }
    names
}

fn resolve_food_fnames() -> HashMap<String, u32> {
    let Some(table) = ue::datatable::find_by_short_name("MasterItemList") else {
        return HashMap::new();
    };
    let full_map = unsafe { ue::datatable::row_name_map(table) };
    full_map.into_iter()
        .filter(|(name, _)| name.starts_with("Food_"))
        .map(|(name, key)| (name, (key & 0xFFFF_FFFF) as u32))
        .collect()
}

fn expand_barman_sell_list(actor: *const u8) {
    let Some(comp) = sell_list_ptr(actor) else {
        ueforge::log::log(format_args!("vendor_food: no vendor component"));
        return;
    };

    let existing = current_sell_names(comp);
    let fname_map = resolve_food_fnames();
    if fname_map.is_empty() {
        ueforge::log::log(format_args!("vendor_food: MasterItemList not loaded"));
        return;
    }

    let missing: Vec<(&str, u32)> = ALL_FOOD_SELLABLE.iter()
        .filter(|name| !existing.contains(**name))
        .filter_map(|name| Some((*name, *fname_map.get(*name)?)))
        .collect();

    if missing.is_empty() {
        ueforge::log::log(format_args!("vendor_food: all food already accepted"));
        return;
    }

    let arr = sell_tarray(comp);
    let total_needed = arr.num + missing.len() as i32;
    if total_needed > arr.max {
        let new_max = total_needed + 10;
        // TArray<u8> with SELL_STRIDE-byte elements: grow by
        // element count * stride so the typed grow works.
        let raw_arr = unsafe {
            &mut *(comp.add(SELL_LIST_OFFSET) as *mut TArray<[u8; SELL_STRIDE]>)
        };
        if let Err(e) = unsafe { raw_arr.grow(new_max) } {
            ueforge::log::log(format_args!("vendor_food: grow failed: {e}"));
            return;
        }
    }

    let arr = sell_tarray(comp);
    let data = arr.data;
    if data.is_null() {
        ueforge::log::log(format_args!("vendor_food: null pointer after grow"));
        return;
    }

    // clone entry 0 as template
    let mut template = vec![0u8; SELL_STRIDE];
    unsafe {
        std::ptr::copy_nonoverlapping(data, template.as_mut_ptr(), SELL_STRIDE);
    }

    let mut num = arr.num;
    for (_name, fname_idx) in &missing {
        let mut entry = template.clone();
        entry[0x08..0x0C].copy_from_slice(&fname_idx.to_le_bytes());
        entry[0x0C..0x10].copy_from_slice(&0u32.to_le_bytes());
        unsafe {
            std::ptr::copy_nonoverlapping(
                entry.as_ptr(),
                data.add((num as usize) * SELL_STRIDE),
                SELL_STRIDE,
            );
        }
        num += 1;
    }

    arr.num = num;
    ueforge::log::log(format_args!(
        "vendor_food: added {} food items to Barman",
        missing.len()
    ));
}

pub fn apply_on_load() {
    ue::actor::on_each_load(
        "vendor_food",
        Duration::from_secs(3),
        || ue::actor::find_actor(VENDOR_ACTOR_CLASS, Some("Barman")),
        expand_barman_sell_list,
    );
}
