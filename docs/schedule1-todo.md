# schedule1-mod: todo

> Work items for [`schedule1-plan.md`](schedule1-plan.md). Do the
> items in order; each has an exit gate before the next starts.

## MelonLoader entry for the IL2CPP shim

- [x] Verify `HarmonyBridge.cs:84` (the HarmonyX-only call)
      exists in MelonLoader's Harmony fork. Verified 2026-08-07:
      line 84 is a comment (UnpatchSelf is documented as avoided,
      never called), and MelonLoader 0.7.2 ships HarmonyX 2.10.2
      (ProductName HarmonyX, ProductVersion 2.10.2 read from the
      installed `MelonLoader/net6/0Harmony.dll`), the same fork
      the shim targets. `__args` (needs 2.1+) is covered. No
      fallback needed.
- [x] Confirm the Il2CppInterop surface the shim uses matches
      what MelonLoader ships. Verified 2026-08-07: MelonLoader
      0.7.2 ships Il2CppInterop.Runtime 1.5.1; Il2CppBridge.cs
      uses only `Il2CppType.From` + `IL2CPP.il2cpp_field_*`,
      present there. Resolve refs from the install's
      `MelonLoader/net6/` + `MelonLoader/Il2CppAssemblies/`
      (HintPath pattern, like the BepInEx csproj), not NuGet.
      Note: the operator's install is Vortex-managed; when
      purged, those files live in the Vortex staging package
      `MelonLoader.x64`. Binding is finally proven by the smoke
      run, not by version numbers.
- [x] `unityforge/cs-shim-melonloader/` MelonMod entry:
      OnInitializeMelon loads the Rust cdylib and bridge,
      OnUpdate ticks, OnApplicationQuit shuts down. Links
      `cs-shim-common/*.cs` + `Il2CppBridge.cs`. Built 2026-08-07:
      compiles clean against MelonLoader 0.7.2 refs + the game's
      Il2CppAssemblies. In-game load untested until the smoke run.
      Fallout fixed while making it compile (the IL2CPP backend
      had never been built): TypeCache + the list_methods body
      moved to cs-shim-common (HarmonyBridge needs them on every
      backend); HarmonyBridge's hard-coded MonoBridge.Acquire
      replaced with an AcquireHandle seam each entry assigns;
      Il2CppBridge.FindType rewritten onto TypeCache.Resolve
      (Il2CppType.From does not take a string); WalkClass converts
      via Il2CppType.From(t); bare `Harmony` qualified (MelonLoader
      exports a legacy Harmony NAMESPACE). Mono + survivalist
      shims rebuilt green after the shared-file changes.

## Generation-loader parity

- [x] Mirror the generation-loader into the MelonLoader entry so
      Rust hot reload works without a game restart. Done by
      construction 2026-08-07: the MelonLoader entry drives
      GenerationLoader (LoadInitial / Tick / ShutdownFinal), same
      as the Mono entry. The BepInEx 6 IL2CPP entry (Plugin.cs)
      still bypasses GenerationLoader; that variant stays on the
      repo todo.md and is not needed for Schedule 1.

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
