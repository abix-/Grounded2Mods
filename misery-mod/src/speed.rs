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
use ueforge::ue::{follow_ptr_chain, read_at, write_at};

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

fn inventory_ptr() -> Result<*const u8, String> {
    let actor = ue::actor::find_actor(ACTOR_CLASS, None)
        .ok_or("no live player character found")?;
    unsafe { follow_ptr_chain(actor, &[CHAR_COMP_OFFSET, INV_PTR_OFFSET]) }
}

fn tmap_element_ptr(inv: *const u8) -> Result<(*const u8, i32), String> {
    let elem_ptr: u64 = unsafe { read_at(inv, MOVEMENT_SPEEDS_MAP) };
    let num: i32 = unsafe { read_at(inv, MOVEMENT_SPEEDS_MAP + 8) };
    if elem_ptr == 0 || num <= 0 {
        return Err("MovementSpeeds map is empty".into());
    }
    Ok((elem_ptr as *const u8, num))
}

fn find_slot(elements: *const u8, num: i32, key: u8) -> Option<usize> {
    (0..num as usize).find(|&s| {
        let k: u8 = unsafe { read_at(elements, s * TMAP_STRIDE) };
        k == key
    })
}

fn read_speed(elements: *const u8, slot: usize) -> f64 {
    unsafe { read_at(elements, slot * TMAP_STRIDE + 8) }
}

fn write_speed(elements: *const u8, slot: usize, value: f64) {
    unsafe { write_at(elements, slot * TMAP_STRIDE + 8, value) }
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
        let key: u8 = unsafe { read_at(elems, slot * TMAP_STRIDE) };
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

// ---- UI ----

pub fn render() {
    use ueforge::ui;

    ui::text("Movement speed");
    ui::text_disabled(
        "Multiplier applied to all movement states \
         (walk, sprint, crouch, holding weapon, etc).",
    );
    ui::spacing();
    ui::separator();
    ui::spacing();

    match current_all() {
        Ok(entries) => {
            for e in &entries {
                ui::text(&format!("  key {:2}  {:.0}", e.key, e.speed));
            }
        }
        Err(e) => {
            ui::text_disabled("No player loaded.");
            ui::text_disabled(&format!("({e})"));
            return;
        }
    }

    ui::spacing();
    ui::separator();
    ui::spacing();
    ui::text("Quick set");
    for (label, mult) in [("1x (default)", 1.0), ("2x", 2.0), ("3x", 3.0)] {
        if ui::button(label) {
            if let Err(e) = set_multiplier(mult) {
                ueforge::log::log(format_args!("speed: {label} failed: {e}"));
            }
        }
        ui::same_line();
    }
    ui::new_line();
}
