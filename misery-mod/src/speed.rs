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
use ueforge::ue::follow_ptr_chain;

const ACTOR_CLASS: &str = "BP_SGKMasterCharacter_C";
const CHAR_COMP_OFFSET: usize = 0x740;
const INV_PTR_OFFSET: usize = 0x218;
const MOVEMENT_SPEEDS_MAP: usize = 0xFE8;

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

/// Finds the player's MISERY inventory component, which owns movement speeds.
/// Stays here because the player classes and component offsets are specific to this game.
fn inventory_ptr() -> Result<*const u8, String> {
    let actor = ue::actor::find_actor(ACTOR_CLASS, None).ok_or("no live player character found")?;
    unsafe { follow_ptr_chain(actor, &[CHAR_COMP_OFFSET, INV_PTR_OFFSET]) }
}

#[derive(Clone)]
pub struct MapEntry {
    pub key: u8,
    pub speed: f64,
}

/// The tab redraws every frame, and reading the speeds means
/// finding the player, which is a full object search. Once a
/// second is plenty for eight numbers on screen.
static SPEEDS: modforge::ui::Cached<Result<Vec<MapEntry>, String>> =
    modforge::ui::Cached::new();
const REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// Returns every player movement speed currently active in MISERY.
/// Stays here because it translates MISERY's movement-state map into this feature's values.
pub fn current_all() -> Result<Vec<MapEntry>, String> {
    let inv = inventory_ptr()?;
    // SAFETY: `inventory_ptr` returns a live BP_PlayerInventory_C object and
    // MOVEMENT_SPEEDS_MAP is its verified TMap<u8, f64> field.
    let inventory = unsafe { &*(inv as *const ue::UObject) };
    let out: Vec<MapEntry> =
        unsafe { ue::tmap::scalar_entries::<u8, f64>(inventory, MOVEMENT_SPEEDS_MAP) }
            .map(|entry| MapEntry {
                key: entry.key(),
                speed: entry.value(),
            })
            .collect();
    if out.is_empty() {
        return Err("MovementSpeeds map is empty".into());
    }
    Ok(out)
}

/// Applies one player-selected multiplier to all normal MISERY movement speeds.
/// Stays here because the baseline speeds and affected states are this mod's gameplay policy.
pub fn set_multiplier(mult: f64) -> Result<(), String> {
    let inv = inventory_ptr()?;
    // SAFETY: `inventory_ptr` returns a live BP_PlayerInventory_C object and
    // MOVEMENT_SPEEDS_MAP is its verified TMap<u8, f64> field.
    let inventory = unsafe { &*(inv as *const ue::UObject) };
    let mut entries: Vec<_> =
        unsafe { ue::tmap::scalar_entries::<u8, f64>(inventory, MOVEMENT_SPEEDS_MAP) }.collect();
    if entries.is_empty() {
        return Err("MovementSpeeds map is empty".into());
    }

    for &(key, base) in BASE_SPEEDS {
        if let Some(entry) = entries.iter_mut().find(|entry| entry.key() == key) {
            // SAFETY: the map is not structurally changed while its values are
            // updated, so the captured slots remain valid for this loop.
            unsafe { entry.write_value(base * mult) };
        }
    }

    ueforge::log::log(format_args!("speed: {mult}x applied to all entries"));
    Ok(())
}

// ---- UI ----

/// Draws the player movement-speed control in the mod menu.
/// Stays here because it presents MISERY's speed policy; Ueforge owns only reusable UI machinery.
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

    match SPEEDS.get(REFRESH, current_all) {
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
            // The numbers on screen just changed.
            SPEEDS.invalidate();
        }
        ui::same_line();
    }
    ui::new_line();
}
