# Settlement upgrades implementation plan

> Execute inline, task by task, operator watching. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** The board's rank-1 pillar (docs/status.md "Settlement
upgrades"): the player sinks hours and materials into making
their settlement stronger: every structure improvable, in place,
incrementally, with real costs, until the costs outweigh the
benefits. The resource sink for a rich player.

**Architecture:** the proven type-ladder trick applied to
STRUCTURES, plus the game's own build menu as the upgrade
affordance. A DRY generator writes leveled structure types; the
player orders an upgrade by placing a real construction (an
upgrade marker) from the build menu, built by real builders from
real materials; on completion the mod swaps the adjacent
structure to its next level in place and removes the marker.

**Tech stack:** story XML (Props + Recipes, same pipeline as the
quality items and work-board quests), a PowerShell generator, one
Rust module on the established mission-scan pattern.

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
- The action menu (right-click) is a baked CursorAction enum
  switch in GameCursor: adding a menu entry means a multi-point
  UI patch chain. REJECTED for the first build; the build menu
  affordance needs zero UI surgery.
- OPEN (Task 1 verifies live): writing a placed structure's
  Prototype reference over the bridge takes effect (MaxDamage
  reads the new type) and survives a save round-trip (Prop
  serialization of the type reference).

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
- THE AFFORDANCE: one build-menu entry, "Structure Upgrade"
  (RecipeType Toolbox, Construction skill, real materials, real
  crafting time). The player places it NEXT TO the target
  structure; builders build it like anything else. On completion
  the mod upgrades the nearest upgradeable structure within 3
  tiles by one level and removes the marker. No adjacent
  upgradeable structure = the marker stands and retries, so
  order of operations never eats materials.
- PLAYER-ONLY for the first build (the scan runs on the player
  camp); the same ladders open to AI camps later (their growth
  system already builds from records), which is the symmetry
  story.
- Chronicle line on each upgrade (public, visible progress):
  "{camp}'s {structure} stands stronger (+N)".

## File structure

- Create: `survivalist-mod/scripts/generate_upgrades.ps1` (the
  ladder generator: leveled Props XML + the marker prop + the
  mod's Recipes.xml with the upgrade recipe)
- Create: `survivalist-mod/story/Props/*.xml` (generated) and
  `survivalist-mod/story/Recipes.xml` (generated)
- Create: `survivalist-mod/src/upgrade.rs` (marker scan, type
  swap, upgrade_status + upgrade_probe ops)
- Modify: `survivalist-mod/src/lib.rs` (mod + tick + ops),
  `survivalist-mod/scripts/build_and_deploy.ps1` (copy Props/
  and Recipes.xml)

---

### Task 1: the risky mechanic, verified live first

- [ ] upgrade_probe op: given a structure (nearest to the player
  camp centre of a named type), write its Prototype to a named
  type over the bridge and read GetMaxDamage back. Permanent
  diagnostic (repo rule).
- [ ] Verify live on two vanilla types (swap a WoodenChest's
  type to SteelChest and back: both exist, no mod data needed,
  no restart needed).
- [ ] Verify save round-trip: operator saves and reloads, probe
  reads the type still swapped. THE GATE: if the type reference
  does not survive the save, the whole design falls back to
  delete-and-replace via Recipe.PlaceProp (researched entry
  point), and the plan gets amended before Task 2.
- [ ] Commit.

### Task 2: the ladders (generator + data)

- [ ] generate_upgrades.ps1: reads the vanilla Props XML for the
  upgradeable set (from vanilla Recipes.xml ProductType entries,
  minus non-structures like Snowman/Grave/traps: config list),
  writes `<Base>_L<N>.xml` for levels 1..10 with the config
  curve applied (MaxDamage, MaxInventoryWeight when present,
  RepairResourceNeeded; NativeName "+N" suffix; same PrefabNames)
  plus the marker prop (UpgradeMarker.xml: a small buildable
  prop, CanBeDemolished, low MaxDamage) and the mod Recipes.xml
  (one "Structure Upgrade" recipe producing the marker:
  Construction skill 3, real materials, CraftingTime ~60).
- [ ] Run it; spot-check one wall and one chest ladder.
- [ ] Deploy script copies story/Props/*.xml and story/Recipes.xml.
- [ ] Commit.

### Task 3: the swap (upgrade.rs)

- [ ] Marker scan on the mission-tick pattern (30s cadence):
  player camp props of the marker type with construction
  complete; for each, nearest prop within 3 tiles whose type
  name is in a ladder (base or _L<N>, next level exists); write
  the Prototype up one level, delete the marker (the game's own
  demolish/delete path), chronicle + log; upgrade_status op
  (ladder data loaded, upgrades applied, last upgrade).
- [ ] Graceful degradation: ladder data not loaded (restart
  pending) logs once and the scan idles; a marker with no
  adjacent upgradeable structure stands and retries.
- [ ] Build, deploy, verify ops answer; commit.

### Task 4: live verify + re-rate

- [ ] After the operator's restart: the upgrade recipe shows in
  the build menu; place a marker by a fence; builders build it
  from real materials; the fence becomes +1 (probe reads the
  higher MaxDamage); the marker disappears; the chronicle line
  posts; a save round-trip keeps the +1.
- [ ] Re-rate the status row honestly; commit.
