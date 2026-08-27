// DefaultMaxSize patcher for the player's main backpack
// (UInventoryComponent at +0x01E0). Built on
// `ueforge::ue::class_tweak::ClassFieldTweak<i32>` so the vanilla
// snapshot + idempotent re-apply logic is shared with every other
// per-instance field-write in the workspace.

use ueforge::ue::class_tweak::ClassFieldTweak;

pub const DEFAULT_SLOT_COUNT: i32 = 100;
pub const VANILLA_MAIN: i32 = 40;
pub const VANILLA_MOUNT: i32 = 30;
pub const DEFAULT_MAX_SIZE_OFFSET: usize = 0x01E0;

static SLOTS: ClassFieldTweak<i32> =
    ClassFieldTweak::new("InventoryComponent", DEFAULT_MAX_SIZE_OFFSET);

pub struct PatchStats {
    pub scanned: usize,
    pub patched: usize,
    pub skipped_non_player: usize,
}

/// Expands player backpacks while leaving mounts and other inventories at their original sizes.
/// Stays here because the player filter, mount size, and inventory offset are Grounded 2 facts;
/// Ueforge owns the reusable class-field patching behavior.
pub fn run(slot_count: i32) -> PatchStats {
    let stats = match SLOTS.apply(
        // Player-owned only. Mount/saddlebag (vanilla 30) and non-
        // player containers (storage chests, station inputs) get
        // filtered out here.
        |obj| obj.full_name().contains("BP_SurvivalPlayerCharacter"),
        // Vanilla mount stays at 30; lift everyone else to slot_count
        // unless already there.
        |vanilla| {
            if vanilla == VANILLA_MOUNT || vanilla >= slot_count {
                None
            } else {
                Some(slot_count)
            }
        },
    ) {
        Ok(s) => s,
        Err(e) => {
            ueforge::log!("patch: {e}");
            return PatchStats {
                scanned: 0,
                patched: 0,
                skipped_non_player: 0,
            };
        }
    };

    PatchStats {
        scanned: stats.scanned,
        patched: stats.patched,
        skipped_non_player: stats.scanned.saturating_sub(stats.considered),
    }
}
