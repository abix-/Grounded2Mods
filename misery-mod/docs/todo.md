# misery-mod open issues

## THE GOAL

Read ALL the pieces of the game out of the game, recognise them
as pieces, and use them to make new stuff on demand.

FIRST every piece is known. THEN we make stuff from them.

No disk step: reading from the game is what keeps this
game-agnostic. Nothing generates anything from a live square or
from whatever a session happened to see.

**Every part already exists and nothing is joined up.** Per the
changelog, all marked done:

- `modforge::structure::PieceDef`, and StructureDef holding pieces
- `modforge` shape classifier: a piece's role from its box
  proportions alone (Slab, Panel, Post, Beam, Block, Clutter),
  engine-agnostic, 9 unit tests
- `misery` asset registry: every shipped mesh whether loaded or
  not. 2398 static meshes, 55 wall pieces
- `ueforge::ue::transform`: `loaded_meshes` (every loaded mesh by
  name), `mesh_bounds` (a mesh's marker offset and half-size),
  `set_actor_mesh`, and reading an actor's transform
- `ueforge::ue::pieces`: read a level's actors as pieces, measure
  meshes, put pieces back, and the ONE conversion between
  Unreal's numbers and modforge's. Endpoints included
- `modforge::structure`: `PieceDef::placed_at`, `group_nearby`
  (cut a loose heap of pieces into things that stand together),
  `Library` (a bounded collection to draw from)
- `modforge::monument::build_at`: choose an arrangement seeded by
  WHERE it stands, lay the buildings out, flatten them into one
  piece list. The same spot always builds the same monument

`harvest.rs` is DELETED (2026-08-26): every part of it was
generic and is now in ueforge, endpoints included. `places.rs` is
344 to 286 lines with its reusable halves extracted; what remains
is its own numbers and the take-and-scatter loop, which is off.

The asset registry answers an HTTP request and the answer is
thrown away: nothing in the mod calls `assets_of_class`. The
classifier never sees a shipped mesh. `rooms.rs` builds mesh
names by hand (`format!("SM_Wall_{w}x{h}")`) with special cases
for the ones that turned out wrong, instead of choosing from
what the game actually has.

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `pieces` | [ ] Turn the asset registry into pieces: every shipped mesh becomes a `PieceDef` with its measured size and marker offset, and the shape classifier names its role | The mod holds a parts list of all 2398 meshes as pieces, each with a role, built from what the game ships. |
| 1 | `pieces` | [ ] Measure the ones that are not loaded: `mesh_info` only reads meshes in memory, so `load_asset` has to pull each one in first. That path is recorded as not yet confirmed working | Every shipped mesh has real measurements, not just the third of them that happen to be loaded. |
| 2 | `rooms` | [ ] Build from the parts list instead of hand-written names: `rooms.rs` picks a piece that fits the slot from the pieces we hold, rather than `format!("SM_Wall_{w}x{h}")` and its special cases | No hardcoded mesh names in rooms.rs. Rooms can use the 55 walls, including the 45-degree corners, not the handful currently named. |
| 2 | `misery` | [ ] Finish the feature bisect. Working so far, each confirmed live: `pe_dispatch`, `nag`, `speed_default`, `vendors`, `spawning`, `strange`. Still off: `rooms`, `assets`, `stack_10x`. OFF deliberately and not to be re-enabled: `harvest`, `places`, `autoload` | Every remaining feature is either confirmed working or named as broken. |
| 3 | `strange` | [ ] A rolled phenomenon placed nothing: `teleport_nest` logged `rolls [...]` then `placed 0 prop(s)`. Either the ground trace found nothing at the points chosen, or that phenomenon's classes did not resolve | A rolled phenomenon always places something, or says why it could not. |
| 1 | `ueforge` | [ ] Measure `FMalloc::Malloc`'s vtable slot out of the running image and set `ue::gmalloc::MALLOC_SLOT`: find `mov rax,[rcx]; call [rax+imm8]` inside `FMemory::Malloc` (the function patternsleuth already anchors in to locate the GMalloc global) and read the `imm8`. Slot 2 was INFERRED from the pattern bytes and was wrong: null return, then a crash | Vendor lists grow again, and a disconnect after a vendor pass does not crash. |
| 1 | `autoload` | [ ] Load the SINGLEPLAYER save, not a hosted server: it calls `LoadLevel` on `BP_HostLoadGameServer`, chosen because it had "Load" in the name. Read what the rows under `BP_SinglePlayerLoadSaveMenu` actually call | Auto-load starts a singleplayer game, confirmed live. |
| 1 | `strange` | [ ] Run the game and confirm phenomena, monuments and rooms still land ON the ground: everything that places anything now goes through the lifted `ue::spawn` and `ue::trace`, and none of it has run since | A world generates with props at the right height, verified live. |
| 3 | `dev` | [ ] Lift the two helpers now duplicated in autoload.rs and nag.rs into ueforge: find the LIVE object (not the `/Game/...` template) and call a UFunction with a parm block checked against `parms_size` | Both lessons are in one place instead of copied per module. |
| 5 | `strange` | [ ] `live_squares` and `active_tile_size` are MISERY facts but sit in a module about phenomena; places.rs reaches into `strange` for them. Consider a home named for what they are | No module reaches into another for something unrelated to its subject. |
| 5 | `autoload` | [ ] Decide whether auto-load should fire again after quitting to the main menu; today `SETTLED` makes it once per launch | Behaviour chosen deliberately and documented. |
| 5 | `autoload` | [ ] Exercise the missing-save path end to end (the guard is measured, the path around it is only reasoned) | Auto-load skips cleanly with the save file moved aside. |
| 3 | `dispatch` | [ ] Resolve `GGameThreadId` so `IsInGameThread` can be asserted directly, instead of comparing the engine tick thread against the ProcessEvent thread (which needs a save loaded first) | A test can assert "this ran on the game thread" cold, at the menu. |
| 3 | `load` | [ ] Load an arbitrary slot, not just the one the game instance already holds: `SGK SetSaveGameSlotName` takes an FString, which has to be constructed in memory the game owns | Any listed save loads by name from the control plane. |
| 5 | `load` | [ ] Verify a slot exists before loading it with `FindExistingSave` (FString in, bool out) rather than trusting the name | load refuses a slot the game does not have. |
| 2 | `rooms` | [ ] Generate multi-room buildings from the kit: several RoomSpecs sharing walls, with a roof | A generated building with more than one room stands and is walkable, verified live. |
| 2 | `rooms` | [ ] Learn assembly from vanilla buildings: record which pieces the designers place adjacent, at what offsets, and bias generation toward it | Adjacency data captured and documented; generated buildings read as native. |
| 3 | `dev` | [ ] Make tuned values op parameters instead of constants (yaw convention, offsets, caps) so hypotheses are testable without a redeploy | A wrong constant can be found and fixed in one session without restarting the game. |
| 3 | `dev` | [ ] Hot reload: cache patternsleuth offsets to disk so the reloaded image never calls into rayon's stale thread pool | A reload survives and the control plane answers without a restart. See research.md 26.4. |
| 3 | `worldgen` | [ ] Research why GenerateCustomBiom(1) generates nothing while Factory works under natural shinings | Factory forcible on demand, or the different path documented in worldgen.md. |
| 3 | `worldgen` | [ ] New-area research: can a spawned fifth generator (or a spare grid region) run RunGenerationFromSeed with a custom grid and pool | Go/no-go with findings documented in worldgen.md. |
| 5 | `worldgen` | [ ] New-square research: how preset levels are packaged (pak, cooked umap), whether a cloned and renamed square can be loaded | Go/no-go with findings documented in worldgen.md. |
| 8 | `worldgen` | [ ] Build one new square end to end and roll it into a pool | A square that never existed streams in-game, verified live. |
| 2 | `spawning` | [ ] Record whether the hub spawn point re-reads count/class after the set_spawn_point writes | Observation from the tamed dwarf spot documented in research.md 25.4. |
| 3 | `vendors` | [ ] Build vendor list config and auto-apply on game load with UI tab | Vendor sell/buy list modifications apply automatically from config on each load. Item list editable from ImGui tab, including SELL_PRICE_PCT and SEWING_KIT_COST. |
| 10 | `skills` | [ ] Find melee damage address for strength stat | Memory offset for player melee damage documented and verified with a write test. |
| 10 | `skills` | [ ] Find player max health address for constitution stat | Memory offset for player max health documented and verified with a write test. |
| 10 | `skills` | [ ] Hook or poll for kill events as XP source | Kill event fires reliably and delivers XP to the tracker. |
| 10 | `skills` | [ ] Find craft completion event as XP source | Craft event fires reliably and delivers XP to the tracker. |
| 15 | `skills` | [ ] Implement RPG system: XP, leveling, stat/skill point allocation | Skill catalog, tracker, and level-up ops registered. See `docs/rpg.md`. |
| 15 | `skills` | [ ] Implement RPG persistence (JSON save/load for level, XP, allocations) | RPG state survives save/reload cycle. |
| 50 | `shining` | [ ] Research what depends on shining regeneration before shipping permanent freeze | Dependencies documented; safe to ship or workaround identified. |
