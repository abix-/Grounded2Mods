# MISERY mod TODO

## Shipped

- [x] 10x item stack multiplier (FieldTweak on ItemList, 175 rows)
- [x] UE4SS external console enabled
- [x] Shining tab (emission timer control)
- [x] Speed tab (1x/2x/3x movement speed buttons)
- [x] Gameplay tab (hunger/thirst/stamina/damage/enemy knobs)
- [x] Scanner/Classes/Structs browser tabs
- [x] restart.ps1 (build, deploy with validation, launch, wait for control plane)

## Blocked

- [ ] Suppress nag screen (WD_PlaytestNote01_C). Widget found. Three attempts failed:
  1. Raw Visibility write at 0xDC: Slate cache not invalidated, widget stays visible
  2. RemoveFromParent via ProcessEvent from background thread: returns Ok but no effect (must be game thread)
  3. Spacebar via SendInput: rejected
  - Needs: game-thread dispatch via pe_queue DrainSite + ProcessEventHook on a class that fires before main menu

## Fixed

- [x] All tabs survive main menu reload: fixed 2026-08-17. Root cause: all three modules (speed, shining, gameplay) cached object pointers that went stale after returning to main menu and loading a save. Blueprint reinstancing changes UClass pointers, so `find_class_fast` + `is_a` never matches. Fix: removed all pointer caches, scan GObjects fresh each call matching by class name string. Shining module also follows BP_ExpeditionDoor_C +0x448 as fallback when the manager disappears after world regeneration (section 20.4)
- [x] Speed tab and speed 2x default: fixed 2026-08-15. Root cause: `find_class_fast` + `is_a` fails for Blueprint classes after reinstancing. Fix: match objects by class name string instead of UClass pointer comparison, then follow actor +0x740 +0x218 pointer chain to reach the inventory

## Not started

- [ ] Set up pe_queue DrainSite + ProcessEventHook (needed for nag screen and any future ProcessEvent work)
- [ ] Research which class to hook for the drain site (must exist before main menu, fire PE often)
- [ ] RPG system: XP, leveling, stat/skill points (see docs/misery-rpg.md)
- [ ] Strength stat: find melee damage in memory
- [ ] Constitution stat: find player max health in memory
- [ ] Kill detection: hook or poll for XP source
- [ ] Craft detection: find event for XP source
- [ ] RPG persistence (JSON save/load for level, XP, allocations)
- [ ] Vendor list modification: build a mod feature to add items to any vendor's buy or sell list on load. Research complete: sell list expansion (section 24.9), TArray growth for full lists (section 24.12), buy list expansion (section 24.13), bulk sell list expansion (section 24.14) all proven working. Batch addition of 23 food items to Barman confirmed. Needs: item list config, auto-apply on game load, UI tab
- [ ] Research biome number to area mapping (section 19.4)
- [ ] Research what depends on shining regeneration before shipping permanent freeze
