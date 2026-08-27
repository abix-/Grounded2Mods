# misery-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `assets.rs` | [ ] Turn every shipped mesh from the asset registry into a `PieceDef` with its measured size and marker offset, and its role from the shape classifier | `asset_inventory` returns all 2398 meshes as pieces, each carrying a size, a marker offset and a role. |
| 1 | `assets.rs` | [ ] Load each unloaded mesh with `load_asset` before measuring it, so measurements cover every shipped mesh and not only the loaded third | Every mesh in the parts list has a non-zero size. |
| 1 | `ue::gmalloc` | [ ] Read the `imm8` from `mov rax,[rcx]; call [rax+imm8]` inside `FMemory::Malloc` and set `MALLOC_SLOT` to it | Vendor lists grow with no `grow failed` line, and disconnecting after a vendor pass does not crash. |
| 2 | `autoload.rs` | [ ] Load the slot the player picks, not the `"Save 1_Auto"` autosave the game instance sometimes holds at the menu | The world that comes up is the save named in the menu row, not the autosave. |
| 2 | `research` | [ ] Find which of `Button_284` and `Button_372` on a save row loads and which deletes, without calling either blind | Named in research.md 26.9, with the evidence that settles it. |
| 3 | `ops` | [ ] Fix `inspect_address` answering `found: false` for live widget addresses | Inspecting a menu widget returns its fields. |
| 1 | `strange.rs` | [ ] Run a world and check props and monuments sit on the ground after the move to `ue::spawn` and `ue::trace` | Operator confirms props are on the ground, not floating or buried. |
| 2 | `lib.rs` | [ ] Restore `stack_10x`, whose block was lost in the feature bisect and never put back (`STACK_TWEAK` is the unused-static warning on every build) | The feature is back and confirmed working live or named as broken. |
| 2 | `spawning.rs` | [ ] Record whether the hub spawn point re-reads count and class after `set_spawn_point` writes | Observation from the tamed dwarf spot written into research.md 25.4. |
| 3 | `strange.rs` | [ ] Make a rolled phenomenon that places nothing say why: `teleport_nest` logged `rolls [...]` then `placed 0 prop(s)` | Every roll either places a prop or logs the reason it could not. |
| 3 | `nag.rs` | [ ] Move the find-the-live-object and checked-parm-block helpers duplicated in nag.rs and autoload.rs into ueforge | Neither file defines its own; both call ueforge. |
| 3 | `dispatch.rs` | [ ] Resolve `GGameThreadId` and assert against it, instead of comparing the engine tick thread to the ProcessEvent thread | A test proves it ran on the game thread with no save loaded. |
| 3 | `autoload.rs` | [ ] Construct an `FString` in memory the game owns so `SGK SetSaveGameSlotName` can name any slot | Any listed save loads by name from the control plane. |
| 3 | `vendors.rs` | [ ] Read the vendor list from config on each load and expose it in an ImGui tab | Vendor lists apply from config on load, and the tab edits the items, `SELL_PRICE_PCT` and `SEWING_KIT_COST`. |
| 3 | `debug.rs` | [ ] Make the tuned constants op parameters: yaw convention, offsets, caps | A wrong constant is found and corrected without redeploying. |
| 3 | `ue::platform` | [ ] Cache the resolved patternsleuth offsets to disk so a reloaded image skips the scan | A hot reload survives and the control plane answers without a restart. |
| 3 | `worldgen` | [ ] Find why `GenerateCustomBiom(1)` generates nothing while Factory works under natural shinings | Factory forcible on demand, or the different path written into worldgen.md. |
| 3 | `worldgen` | [ ] Test whether a spawned fifth generator can run `RunGenerationFromSeed` with a custom grid and pool | Go or no-go written into worldgen.md. |
| 5 | `strange.rs` | [ ] Move `live_squares` and `active_tile_size` out of the phenomena module into one named for the world grid | places.rs no longer reaches into strange.rs. |
| 5 | `autoload.rs` | [ ] Decide whether auto-load fires again after quitting to the main menu, and make the code match | Behaviour chosen and stated in the module docs. |
| 5 | `autoload.rs` | [ ] Run the missing-save path with the save file moved aside | Auto-load skips and logs the reason, with no `LoadLevel` call. |
| 5 | `worldgen` | [ ] Find how preset levels are packaged (pak, cooked umap) and whether a cloned, renamed square loads | Go or no-go written into worldgen.md. |
| 8 | `worldgen` | [ ] Build one new square and roll it into a pool | A square that never existed streams in-game, verified live. |
| 10 | `skills` | [ ] Find the melee damage address for the strength stat | Offset documented and proved with a write test. |
| 10 | `skills` | [ ] Find the player max health address for the constitution stat | Offset documented and proved with a write test. |
| 10 | `skills` | [ ] Hook or poll kill events as an XP source | A kill delivers XP to the tracker. |
| 10 | `skills` | [ ] Find the craft completion event as an XP source | A craft delivers XP to the tracker. |
| 15 | `skills` | [ ] Implement XP, levelling and stat allocation | Skill catalog, tracker and level-up ops registered. See docs/rpg.md. |
| 15 | `skills` | [ ] Save and load RPG state as JSON | Level, XP and allocations survive a save and reload. |
| 50 | `shining.rs` | [ ] Find what depends on shining regeneration before shipping a permanent freeze | Dependencies documented, and the freeze shipped or a workaround named. |
