// G2 save-slot key resolver. The actual walk lives in
// `ueforge::rpg::SlotKeyResolver`; this module wires the G2-specific
// class name + offset.
//
// AInGameGameState is the in-game UWorld's GameState. It carries
// `PlaythroughGuid` (FGuid, 16 bytes) at offset 0x032C
// (`Maine_classes.hpp:28544`). This is the same stable GUID used to
// disambiguate saves on disk. Exactly what we need as a
// persistence filename key.
//
// Earlier attempt used `USaveLoadManager.SaveInProgressSaveGameHeaderData`
// at +0x90, but that field is null except during an active save
// operation. AInGameGameState is the right surface for "we're in a
// world, give me the playthrough id."

use ueforge::rpg::SlotKeyResolver;

const PLAYTHROUGH_GUID_OFFSET: usize = 0x032C;

static RESOLVER: SlotKeyResolver = SlotKeyResolver::new("InGameGameState", PLAYTHROUGH_GUID_OFFSET);

/// Returns the current playthrough's stable identifier as a 32-char
/// hex string suitable as a filename. None when not in-world.
/// Stays here because the game-state class and playthrough GUID offset are Grounded 2 facts;
/// Ueforge owns the reusable slot-key resolver.
pub fn current_slot_key() -> Option<String> {
    ueforge::counters::bump(&crate::counters::WORLD_LOADER_GOBJECTS_WALKS);
    RESOLVER.resolve()
}
