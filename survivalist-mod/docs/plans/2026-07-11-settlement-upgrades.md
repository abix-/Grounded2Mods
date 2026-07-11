# Settlement upgrades implementation plan

> Execute inline, task by task, operator watching. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** The board's rank-1 pillar (docs/status.md "Settlement
upgrades"): the player sinks hours and materials into making
their settlement stronger: every structure improvable, in place,
incrementally, with real costs, until the costs outweigh the
benefits. The resource sink for a rich player.

**Architecture:** the proven type-ladder trick applied to
STRUCTURES, plus a REAL "Upgrade" option in the game's own
right-click action menu (operator-locked 2026-07-11: "real
modding not janky modding"; the earlier build-menu-marker
affordance is REJECTED). The menu work happens in the C# shim
(Unityforge.Shim.Survivalist), which compiles against the game
assembly and can therefore construct the menu's structs, switch
on the real enums, and patch with full type safety. A DRY
generator writes the leveled structure types; the C# patches add
the menu option, gate it, take the materials, and swap the type.

**Tech stack:** story XML (Props, same pipeline as the quality
items), a PowerShell generator, C# Harmony patches in the shim
(embedded Harmony 2.4.2), a small Rust op surface for
observability.

## Research findings (2026-07-11, decompile + vanilla data)

- A structure's hit points are TYPE data: PropPrototype.MaxDamage
  (ConcreteWall 20, WoodenChest 10); the placed instance
  accumulates Prop.Damage and dies at GetMaxDamage()
  (Prop.cs: "Damage += damage; if (Damage >= maxDamage)
  OnDestroyed"). Storage capacity is TYPE data too
  (PropPrototype.MaxInventoryWeight; WoodenChest 400). So leveled
  structure TYPES carry the whole ladder, exactly like the
  quality items.
- Structures and their build recipes are per-story XML: Props/
  *.xml (the type data) and Recipes.xml (the build menu: RecipeType
  Toolbox + Construction skill + real Ingredients + CraftingTime;
  every story's Recipes.xml MERGES by key at load: Story.cs:443).
  The mod folder ships both, like it already ships Equipment and
  Scripts.
- The 36 player-buildable structures are enumerable from vanilla
  Recipes.xml ProductType entries: fences/walls (WoodFence,
  WireFence, PicketFence, ConcreteWall), gates (Wood/Wire/
  Picket), towers (WatchTower, ConcreteWatchTower, Pillbox),
  storage (WoodenChest, SteelChest), housing (Tent 1-3, Shack,
  WoodCabin, WoodBarn), work props (WorkBench, Forge, Kiln,
  Still, Well, ...).
- THE ACTION MENU CHAIN (the real affordance, all patchable from
  the C# shim with direct type access):
  - Population: GameCursor.GetAvailableActions adds per-object
    entries as `new AvailableAction(CursorAction.X, character,
    target, enabledReason)` with skill gating and disabled
    reasons (the Repair entry at GameCursor.cs:4036 is the exact
    pattern: toolbox check, own-community check, Construction
    skill gate). A POSTFIX appends the Upgrade entry.
  - Dispatch: the click lands in Hud's `case CursorAction.X:`
    switch (Repair at Hud.cs:2373) which emits an InputAction. A
    PREFIX on that method handles our action value and skips the
    original for it.
  - The action id: a sentinel CursorAction value far above the
    vanilla range (enums are ints; vanilla switches ignore
    unknown values by design).
  - Label: one patch on the action-name lookup so the sentinel
    renders as "Upgrade" (exact lookup method pinned during the
    build; the shim can read it directly).
- UPGRADE COST FROM THE GAME'S OWN DATA: every structure declares
  RepairResourceType / RepairResourceNeeded (wood walls repair
  with wood, concrete with cement). Upgrade cost = the same
  resource, scaled by target level (knob). No invented cost
  tables; the resource identity always fits the structure.
- OPEN (Task 1 verifies live): writing a placed structure's
  Prototype reference takes effect (MaxDamage reads the new
  type) and survives a save round-trip (Prop serialization of
  the type reference).

## Design decisions (locked with the operator's vision)

- LADDERS: every player-buildable structure family gets levels
  (config: 10) with the SAME prefab (visuals unchanged, stats
  up): MaxDamage always; MaxInventoryWeight for storage;
  RepairResourceNeeded scales along. Level N's display name:
  "Wooden Chest +N".
- DIMINISHING RETURNS: benefit per level shrinks (config list,
  e.g. +50%, +40%, +32%, ...) while the marker's cost stays
  flat, so "upgrade until the costs outweigh the benefits"
  emerges from the curve, not from a hard wall. All knobs in one
  generator config block, like the quality generator.
- THE AFFORDANCE: a real "Upgrade" option in the right-click
  action menu on the player's own upgradeable structures,
  implemented in the C# shim. Gated like Repair: own community,
  Construction skill (>= the base type's repair skill + level
  knob), and the materials present (the structure's own repair
  resource, scaled by target level), with the vanilla disabled
  reasons shown when a gate fails. On click: the materials are
  consumed for REAL (the character's carried stack first, then
  camp stores), the structure's type swaps up one level in
  place, and the status bar notes the camp standing stronger.
  V1 is click-to-upgrade with real costs; a walk-to-and-work
  crew flow (the full Repair-role feel) is a later polish pass.
- PLAYER-ONLY for the first build (the scan runs on the player
  camp); the same ladders open to AI camps later (their growth
  system already builds from records), which is the symmetry
  story.
- Chronicle line on each upgrade (public, visible progress):
  "{camp}'s {structure} stands stronger (+N)".

## File structure

- Create: `survivalist-mod/scripts/generate_upgrades.ps1` (the
  ladder generator: leveled Props XML)
- Create: `survivalist-mod/story/Props/*.xml` (generated)
- Create: `unityforge/cs-shim-survivalist/UpgradeMenu.cs` (the
  C# Harmony patches: menu population postfix, click dispatch
  prefix, label patch, the upgrade execution: gates, real
  material consumption, type swap)
- Create: `survivalist-mod/src/upgrade.rs` (upgrade_probe +
  upgrade_status ops: the observability surface; the C# side
  reports counters through a small bridge surface or the ops
  read game state directly)
- Modify: `survivalist-mod/src/lib.rs` (ops),
  `survivalist-mod/scripts/build_and_deploy.ps1` (copy Props/)

---

### Task 1: the risky mechanic, verified live first

- [ ] upgrade_probe op (Rust): given a structure (nearest to the
  player camp centre of a named type), write its Prototype to a
  named type and read GetMaxDamage back. Permanent diagnostic
  (repo rule).
- [ ] Verify live on two vanilla types (swap a WoodenChest's
  type to SteelChest and back: both exist, no mod data needed,
  no restart needed).
- [ ] Verify save round-trip: operator saves and reloads, probe
  reads the type still swapped. THE GATE: if the type reference
  does not survive the save, the fallback is delete-and-replace
  via Recipe.PlaceProp (researched entry point), and the plan
  gets amended before Task 2.
- [ ] Commit.

### Task 2: the ladders (generator + data)

- [ ] generate_upgrades.ps1: reads the vanilla Props XML for the
  upgradeable set (from vanilla Recipes.xml ProductType entries,
  minus non-structures like Snowman/Grave/traps: config list),
  writes `<Base>_L<N>.xml` for levels 1..10 with the config
  curve applied (MaxDamage, MaxInventoryWeight when present,
  RepairResourceNeeded; NativeName "+N" suffix; same
  PrefabNames). Knobs: per-level benefit multipliers
  (diminishing), cost scale, level cap.
- [ ] Run it; spot-check one wall and one chest ladder.
- [ ] Deploy script copies story/Props/*.xml.
- [ ] Commit.

### Task 3: the menu (C# shim patches)

- [ ] UpgradeMenu.cs in the survivalist shim: a sentinel
  CursorAction value; a postfix on the menu population adding
  "Upgrade" for the player's own upgradeable structures with the
  vanilla gates (own community, Construction skill, materials
  present, next level exists), disabled reasons included; a
  prefix on Hud's click dispatch handling the sentinel: consume
  the repair resource scaled by target level (character stack
  first, then camp stores, real Take), swap the structure's
  Prototype up one level, refresh, status-bar line; the label
  patch so the option reads "Upgrade" with the cost.
- [ ] Patches install via the shim's existing Harmony instance
  and log an install line (Player.log), same discipline as the
  Rust hooks.
- [ ] Build both halves, deploy (shim DLL changes need the
  restart, not a hot reload: note it), verify install lines.
- [ ] Commit.

### Task 4: live verify + re-rate

- [ ] After the operator's restart: hover an own wall shows
  "Upgrade" with the cost; clicking with materials consumes them
  and the wall becomes +1 (probe reads the higher MaxDamage);
  without materials the option shows disabled with the vanilla
  reason; a save round-trip keeps the +1; ten clicks climb the
  ladder with diminishing gains.
- [ ] Re-rate the status row honestly; commit.
