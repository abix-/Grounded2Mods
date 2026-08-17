//! Movement speed control via the MovementSpeeds TMap on
//! BP_PlayerInventory_C.
//!
//! walk_class fails for all Blueprint component classes
//! (section 22.13 of misery-research.md). The inventory is
//! reached by finding the player actor (which walk_class does
//! find), then following pointer offsets:
//!   actor +0x740 -> BP_CharacterComponent_C
//!          +0x218 -> BP_PlayerInventory_C
//!           +0xFE8 -> MovementSpeeds TMap

use ueforge::ue;

const ACTOR_CLASS: &str = "BP_SGKMasterCharacter_C";
const CHAR_COMP_OFFSET: usize = 0x740;
const INV_PTR_OFFSET: usize = 0x218;
const MOVEMENT_SPEEDS_MAP: usize = 0xFE8;
const TMAP_STRIDE: usize = 24;

const BASE_SPEEDS: &[(u8, f64)] = &[
    (2, 250.0),
    (3, 600.0),
    (5, 100.0),
    (6, 250.0),
    (7, 600.0),
    (9, 100.0),
    (10, 350.0),
    (11, 100.0),
];

fn read<T: Copy>(ptr: *const u8, offset: usize) -> T {
    unsafe { (ptr.add(offset) as *const T).read_unaligned() }
}

fn write<T: Copy>(ptr: *const u8, offset: usize, value: T) {
    unsafe { (ptr.add(offset) as *mut T).write_unaligned(value) }
}

fn inventory_ptr() -> Result<*const u8, String> {
    let rt = ue::try_runtime().ok_or("ue runtime not initialized")?;
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return Err("gobjects view invalid".into());
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        let class = match obj.class() {
            Some(c) => c,
            None => continue,
        };
        if class.as_object().name() != ACTOR_CLASS {
            continue;
        }
        if !obj.full_name().contains("PersistentLevel") {
            continue;
        }
        let actor = obj.as_ptr();
        let comp: u64 = read(actor, CHAR_COMP_OFFSET);
        if comp == 0 {
            continue;
        }
        let inv: u64 = read(comp as *const u8, INV_PTR_OFFSET);
        if inv != 0 {
            return Ok(inv as *const u8);
        }
    }
    Err("no live player character found".into())
}

fn tmap_element_ptr(inv: *const u8) -> Result<(*const u8, i32), String> {
    let elem_ptr: u64 = read(inv, MOVEMENT_SPEEDS_MAP);
    let num: i32 = read(inv, MOVEMENT_SPEEDS_MAP + 8);
    if elem_ptr == 0 || num <= 0 {
        return Err("MovementSpeeds map is empty".into());
    }
    Ok((elem_ptr as *const u8, num))
}

fn find_slot(elements: *const u8, num: i32, key: u8) -> Option<usize> {
    (0..num as usize).find(|&s| {
        let k: u8 = read(elements, s * TMAP_STRIDE);
        k == key
    })
}

fn read_speed(elements: *const u8, slot: usize) -> f64 {
    read(elements, slot * TMAP_STRIDE + 8)
}

fn write_speed(elements: *const u8, slot: usize, value: f64) {
    write(elements, slot * TMAP_STRIDE + 8, value);
}

pub struct MapEntry {
    pub key: u8,
    pub speed: f64,
}

pub fn current_all() -> Result<Vec<MapEntry>, String> {
    let inv = inventory_ptr()?;
    let (elems, num) = tmap_element_ptr(inv)?;
    let mut out = Vec::new();
    for slot in 0..num as usize {
        let key: u8 = read(elems, slot * TMAP_STRIDE);
        let speed = read_speed(elems, slot);
        out.push(MapEntry { key, speed });
    }
    Ok(out)
}

pub fn set_multiplier(mult: f64) -> Result<(), String> {
    let inv = inventory_ptr()?;
    let (elems, num) = tmap_element_ptr(inv)?;

    for &(key, base) in BASE_SPEEDS {
        if let Some(s) = find_slot(elems, num, key) {
            write_speed(elems, s, base * mult);
        }
    }

    ueforge::log::log(format_args!("speed: {mult}x applied to all entries"));
    Ok(())
}
