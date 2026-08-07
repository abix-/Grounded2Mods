# schedule1-mod: todo

> Work items for [`schedule1-plan.md`](schedule1-plan.md). Do the
> items in order; each has an exit gate before the next starts.

## MelonLoader entry for the IL2CPP shim

- [ ] Verify `HarmonyBridge.cs:84` (the HarmonyX-only call)
      exists in MelonLoader's Harmony fork; add a guarded
      fallback in the shared file if not.
- [ ] Confirm the Il2CppInterop surface the shim uses matches
      what MelonLoader ships; resolve refs from the game's
      `MelonLoader/net6/` dir, not NuGet.
- [ ] `unityforge/cs-shim-melonloader/` MelonMod entry:
      OnInitializeMelon loads the Rust cdylib and bridge,
      OnUpdate ticks, OnApplicationQuit shuts down. Links
      `cs-shim-common/*.cs` + `Il2CppBridge.cs`.

## Generation-loader parity

- [ ] Mirror the generation-loader into the MelonLoader entry so
      Rust hot reload works without a game restart (existing
      todo.md item for the IL2CPP shim).

## Smoke on Schedule 1

- [ ] Deploy `il2cpp_smoke.unityforge.dll` + the MelonLoader shim
      into the operator's Schedule 1 `Mods/` folder.
- [ ] Exit gate: ping, smoke_state (runtime = IL2CPP),
      walk_class, smoke_read/smoke_write round trip, postfix
      fires; driven as repo tests via the modforge client. The 7
      existing Vortex mods still load clean.

## Research Schedule 1 internals

- [ ] New crate `schedule1-mod/` (cdylib, unityforge + modforge,
      own HTTP port) added to the workspace.
- [ ] `schedule1-mod/docs/research.md` +
      `docs/certainty-tracking.md` started.
- [ ] Map regions: the class that owns town areas + its state.
- [ ] NPCs: spawn/path/despawn; the cartel/goon NPC classes.
- [ ] Combat: health, damage, death, aggro classes.
- [ ] The kill-observation Harmony hook point for combat XP.

## Combat RPG levelling

- [ ] XP on NPC kill via one Harmony postfix on the death path.
- [ ] Player combat stats as SkillDefs with unityforge field
      effects (list drafted from research findings).
- [ ] Persistence per save slot via `modforge::rpg::store`.
- [ ] Exit gate: kill grants XP in-game, leveled stat visibly
      changes, save/reload persists, `skill_state` op agrees.

## Faction war (in slices, each verified in-game)

- [ ] Ownership map: factions own regions; `faction_state` op.
- [ ] NPC-vs-player contests: attacks on the player and their
      dealing areas, frequency up, enemy stats scale with player
      level.
- [ ] Territory pressure: losing a region costs the player
      (customers/dealers in that region).
- [ ] NPC-vs-NPC contests: factions fight each other for regions
      without player involvement.
- [ ] Director split: random event rolls and adaptive pressure as
      two separate layers, never merged.
