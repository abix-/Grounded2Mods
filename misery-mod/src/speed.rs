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

/// The player, found once a session. `find_actor` reads every
/// object in the game, and this tab used to do that on every
/// frame it was open.
pub static PLAYER: ueforge::ue::actor::LiveActor =
    ueforge::ue::actor::LiveActor::new("BP_SGKMasterCharacter_C");

pub fn register_player_selector() {
    ueforge::selector::SELECTOR_REGISTRY.register(ueforge::selector::SelectorDef {
        prefix: "live_player",
        summary: "MISERY player retained for the current world",
        resolver: |selector| {
            (selector == "live_player").then(|| {
                PLAYER.retained().ok_or_else(|| {
                    "MISERY player is not retained for the current world".to_string()
                })
            })
        },
    });
}
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
    let actor = PLAYER.ptr().ok_or("no live player character found")?;
    unsafe { follow_ptr_chain(actor, &[CHAR_COMP_OFFSET, INV_PTR_OFFSET]) }
}

#[derive(Clone)]
pub struct MapEntry {
    pub key: u8,
    pub speed: f64,
}

/// What the tab shows.
///
/// Nothing but this mod changes these numbers, so the tab does
/// not read them. They are read once, when the tab first has a
/// world to read from, and replaced when `set_multiplier` writes
/// new ones. A frame with the tab open then does no work at all.
///
/// Cleared when the world ends, along with the player pointer, so
/// the next world reads its own.
static SHOWN: modforge::read_once::ReadOnce<Vec<MapEntry>> = modforge::read_once::ReadOnce::new();

/// Returns every player movement speed currently active in MISERY.
/// Stays here because it translates MISERY's movement-state map into this feature's values.
pub fn current_all() -> Result<Vec<MapEntry>, String> {
    // Counted so "the tab reads once" is a number rather than a
    // claim. With the tab open for 30 seconds this should read 1.
    let _m = modforge::counters::measure("misery-speed-read");
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

    // We just changed them, so what the tab shows is now wrong.
    // Reading them back would be asking the game to tell us what
    // we told it.
    SHOWN.forget();
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

    // Registered so the world ending clears it along with
    // everything else read out of that world.
    SHOWN.forget_with(|| SHOWN.forget());
    let Some(shown) = SHOWN.get(|| current_all().ok()) else {
        ui::text_disabled("No player loaded.");
        return;
    };
    for e in &shown {
        ui::text(&format!("  key {:2}  {:.0}", e.key, e.speed));
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
