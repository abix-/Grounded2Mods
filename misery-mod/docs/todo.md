# misery-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `assets.rs` | [ ] Turn every shipped mesh from the asset registry into a `PieceDef` with its measured size and marker offset, and its role from the shape classifier | `asset_inventory` returns all 2398 meshes as pieces, each carrying a size, a marker offset and a role. |
| 1 | `assets.rs` | [ ] Load each unloaded mesh with `load_asset` before measuring it, so measurements cover every shipped mesh and not only the loaded third | Every mesh in the parts list has a non-zero size. |
| 1 | `ops` | [ ] Stop `discover_class_detail` crashing the game on a native engine class: it faults in `UClass::iter_native_properties` reading a tiny address (worldgen.md 10) | Pointing it at `LevelStreamingDynamic` returns its fields instead of killing the process. |
| 1 | `ops` | [ ] Stop `fname_to_string` killing the game when handed a value that is not a real name: it panics and the panic unwinds out of the mod (worldgen.md 10) | A bogus FName returns an error over the control plane and the game keeps running. |
| 2 | `ops` | [ ] Make NO control-plane call able to kill the game: catch panics at the op boundary the way the hook trampolines already do | A deliberately bad argument to every registered op returns an error, proven by a test. |
| 2 | `ops` | [ ] Guard `read_bytes` with `modforge::seh::guard` so a bad address returns an error instead of killing the game | Reading a deliberately bad address returns an error, proven by a test. |
| 2 | `research` | [ ] Find a level's own actor list offset, once `read_bytes` is guarded | The NPCs in one square are read as a short list, with no object search. |
| 2 | `autoload.rs` | [ ] Load the slot the player picks, not the `"Save 1_Auto"` autosave the game instance sometimes holds at the menu | The world that comes up is the save named in the menu row, not the autosave. |
| 2 | `research` | [ ] Find which of `Button_284` and `Button_372` on a save row loads and which deletes, without calling either blind | Named in research.md 26.9, with the evidence that settles it. |
| 2 | `lib.rs` | [ ] Restore `stack_10x`, whose block was lost in the feature bisect and never put back (`STACK_TWEAK` is the unused-static warning on every build) | The feature is back and confirmed working live or named as broken. |
| 2 | `spawning.rs` | [ ] Record whether the hub spawn point re-reads count and class after `set_spawn_point` writes | Observation from the tamed dwarf spot written into research.md 25.4. |
| 2 | `vendors.rs` | [ ] Disconnect after a vendor pass and confirm no `FMallocBinned2` canary crash, now that grows use the engine allocator (research.md 27) | Operator disconnects after a vendor pass with no crash. |
| 3 | `vendors.rs` | [ ] `find_vendor_comp` takes its class at runtime so it cannot use `LiveActor`. Decide whether that matters: it runs once per vendor type per load, not on a hot path | Either converted, or a line in performance.md saying why it stays. |
| 3 | `ops` | [ ] Fix `inspect_address` answering `found: false` for live widget and streaming-level addresses | Inspecting one returns its fields. |
| 3 | `dispatch.rs` | [ ] Resolve `GGameThreadId` and assert against it, instead of comparing the engine tick thread to the ProcessEvent thread | A test proves it ran on the game thread with no save loaded. |
| 3 | `autoload.rs` | [ ] Construct an `FString` in memory the game owns so `SGK SetSaveGameSlotName` can name any slot | Any listed save loads by name from the control plane. |
| 3 | `vendors.rs` | [ ] Read the vendor list from config on each load and expose it in an ImGui tab | Vendor lists apply from config on load, and the tab edits the items, `SELL_PRICE_PCT` and `SEWING_KIT_COST`. |
| 3 | `debug.rs` | [ ] Make the tuned constants op parameters: yaw convention, offsets, caps | A wrong constant is found and corrected without redeploying. |
| 3 | `ue::platform` | [ ] Cache the resolved patternsleuth offsets to disk so a reloaded image skips the scan | A hot reload survives and the control plane answers without a restart. |
| 3 | `worldgen` | [ ] Find why `GenerateCustomBiom(1)` generates nothing while Factory works under natural shinings | Factory forcible on demand, or the different path written into worldgen.md. |
| 3 | `worldgen` | [ ] Test whether a spawned fifth generator can run `RunGenerationFromSeed` with a custom grid and pool | Go or no-go written into worldgen.md. |
| 5 | `modforge::ui` | [ ] `Cached<T>` has one user, the Shining tab, where a once-a-second read matches a countdown displayed in whole seconds. Decide whether one user justifies a shared type or whether it should move back into `shining.rs` | Kept with the reason written down, or moved. |
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
