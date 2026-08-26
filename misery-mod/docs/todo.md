# misery-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
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
