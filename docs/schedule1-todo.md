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

- [x] Deploy `il2cpp_smoke.unityforge.dll` + the MelonLoader shim
      into the operator's Schedule 1 `Mods/` folder. Done
      2026-08-07.
- [x] Exit gate: ping, smoke_state (runtime = IL2CPP),
      walk_class, inspect_object, read_field/write_field round
      trip, postfix fires. PASSED 2026-08-07 against the live
      game via `il2cpp-smoke/tests/smoke.rs` (modforge client,
      port 17175). Hot reload also proven live: gen1 dropped
      while the game ran, swap logged, no restart.
      The "7 existing mods load clean" clause is NOT met, but for
      an upstream reason, not ours: see the 0.4.6 findings below.

### What we know, 2026-08-07 (game updated 0.4.5f2 -> 0.4.6f11)

- Steam auto-updated Schedule 1 to 0.4.6f11. MelonLoader's
  interop generation (Il2CppInterop, bundled by MelonLoader
  0.7.2 AND latest 0.7.3, AND upstream BepInEx/Il2CppInterop
  master) crashes on the new build. Every modded Schedule 1
  player is bricked on 0.4.6 until upstream ships a fix.
- Root cause, verified: the 0.4.6 binary strips types that are
  still referenced (verified: `UnityEngine.Camera+GateFitMode`
  has zero definitions in the Cpp2IL dump while dumped types
  still reference it). The generator dereferences null on such
  refs in three places (Pass11ComputeTypeSpecifics,
  AssemblyRewriteContext.RewriteTypeRef,
  RewriteGlobalContext.JudgeSpecificsByOriginalType).
- Our fix: patched upstream Il2CppInterop master (base commit
  81a6f78) at those three sites to treat unresolvable types
  conservatively (non-blittable / Il2CppSystem.Object /
  reference type). Proof: `Il2CppInterop.CLI generate` against
  the game's own Cpp2IL dump completes and emits all 148 interop
  assemblies; in-game regeneration then succeeded and MelonLoader
  loaded all 9 mods. The patched
  `Il2CppInterop.Generator.dll` + `Il2CppInterop.Common.dll`
  (AssemblyVersion pinned 1.5.1.0 to satisfy MelonLoader's strict
  binding) are deployed to the game's `MelonLoader/net6/` and to
  the Vortex staging package `MelonLoader.x64`. Source clone +
  diff live in the session scratchpad; the fix should be offered
  upstream (BepInEx/Il2CppInterop) so a stock MelonLoader works
  again.
- MelonLoader 0.7.3 is installed (upgraded from 0.7.2 via the
  Vortex staging package; 0.7.2 backup kept in scratchpad).
- Third-party mod fallout on 0.4.6 (upstream, not ours):
  Infinite_ATM throws MissingMethodException EVERY FRAME
  (`ATM.set_WEEKLY_DEPOSIT_LIMIT` removed from the game;
  verified absent from the Cpp2IL dump). Remedy: disable
  Infinite_ATM in Vortex until its author updates. S1API logs
  several one-shot patch failures at startup (e.g.
  `ChemistryStationCanvas` gone). eMployee/DealerPlus territory
  untested in play.
- Port fact: modforge's default control-plane port 17173 is held
  by eufy-capture on this machine (bind fails WSAEACCES 10013).
  il2cpp-smoke moved to 17175; give `schedule1-mod` its own port
  too.
- The MelonLoader shim path is proven end-to-end: shim loads via
  MelonLoader, unityforge init, Harmony postfix installed and
  firing (Time.get_realtimeSinceStartup), control plane
  answering, generation-based hot reload working. The
  research phase can start on top of this.
- 2026-08-07 late: game updated again (0.4.6f11 -> 0.4.6f12),
  forcing interop regeneration, and the generator crashed at a
  FOURTH site the first three patches missed:
  Pass16ScanMethodRefs -> XrefScanMetadataGenerationUtil.
  FindMetadataInitForMethod throws ArgumentOutOfRangeException
  decoding some f12 method bodies. Patched in the same clone
  (scan failure = "no metadata init" for that method, the
  existing no-answer path), rebuilt with the 1.5.1.0 pin,
  deployed to game net6 + Vortex staging (now hardlinked, so
  one copy serves both). Upstream offer now covers four sites.

## The goal checklist (added 2026-08-07)

The operator's goal: FF7-style grind loop, then conquest. Run
around an area farming mobs, kill them, get loot, level up, then
take region control from factions. Every unchecked box below is
the concrete path there, in order. Sections after this one carry
the detail per box.

- [x] Research proven live: region owner, mob classes, death
      path, kill hook, loot path (game must be running). All
      five proven by 2026-08-08; evidence in
      schedule1-mod/docs/research.md + certainty-tracking.md.
- [x] Kill an NPC, gain XP, level a combat stat, stat visibly
      changes, survives save/reload. DONE 2026-08-08: operator
      knocked out an NPC, log shows "+25 XP for the kill (total
      875) LEVEL UP -> 9"; auto-spend raised Heavy Hands; state
      survived 4+ relaunches.
- [x] Kill an NPC, loot drops (cash first), pick it up in-game.
      DONE 2026-08-08: kill dropped a rolled cash stack at the
      body (toughness-scaled), operator picked it up.
- [ ] Walk into a region, hostile mobs are there to farm; they
      respawn on a timer; their stats scale with player level.
- [ ] Regions show an owner faction; `faction_state` op agrees
      with what the player sees.
- [ ] Take a region: clear its mobs/influence and ownership
      flips to the player; losing one costs the player.
- [ ] Factions fight each other for regions with no player
      involvement.

## Research Schedule 1 internals

- [x] New crate `schedule1-mod/` (cdylib, unityforge + modforge,
      own HTTP port 17175) added to the workspace. Built and
      deployed to Mods/ 2026-08-07 (replaces the smoke dll).
- [x] `schedule1-mod/docs/research.md` +
      `docs/certainty-tracking.md` started. Also added a
      `harmony_probe` op to unityforge ops (no-op prefix patch,
      report, unpatch) to prove per-target patchability on game
      classes without a restart.
- [x] Map regions: the class that owns town areas + its state.
      ANSWERED 2026-08-07: ScheduleOne.Map.Map singleton owns
      Regions: MapRegionData[] (6 regions, EMapRegion 0-5 =
      Northtown/Westville/Downtown/Docks/Suburbia/Uptown, unlock
      ranks 0/1/3/5/7/9); CartelInfluence singleton holds 0..1
      influence per region. Full detail in
      schedule1-mod/docs/research.md.
- [x] NPCs: spawn/path/despawn; the cartel/goon NPC classes.
      GoonPool.SpawnGoon + CartelGoon.AttackEntity proven
      in-game 2026-08-07/08.
- [x] Combat: health, damage, death, aggro classes. ANSWERED
      2026-08-08: NotifyAttackedByPlayer fires per player hit
      (attribution); melee-to-0 raises KnockOut, not Die.
- [x] The kill-observation Harmony hook point for combat XP.
      Die + KnockOut prefixes installed and FIRED live during
      real fights (combat_trace, 2026-08-08).
- [x] Loot path: how item pickups / dead drops are created in
      the world; prove spawning one at a position via a test.
      PROVEN 2026-08-08: cash template clone + FishNet spawn,
      operator picked it up; recipe in research.md 4b.
- [ ] Mob spawn path: prove spawning a hostile cartel/goon NPC
      at a position via the vanilla machinery, and that it
      fights, dies, and despawns clean.

## Combat RPG levelling

- [x] XP on player kills: prefixes on NPCHealth
      NotifyAttackedByPlayer + Die + KnockOut with per-NPC
      attribution and dedupe (src/killcredit.rs, 2026-08-08).
- [x] Skills as SkillDefs: Heavy Hands (punch damage, instance
      props, exact math proven). Vitality + regeneration ON ICE:
      static field-backed setters crash 0.4.6f12 (see below).
- [x] Persistence per save slot (LoadManager save folder name);
      proven across 4+ relaunches. Endless curve + auto-spend
      per the operator.
- [x] Exit gate PASSED 2026-08-08: operator's kill logged
      "+25 XP ... LEVEL UP -> 9", auto-spend applied, punches
      confirmed stronger ("IT WORKED"), persistence proven
      across relaunches, skill_state agrees.
- [ ] Fix the interop generator's 4th patch site properly (the
      skipped metadata init breaks static field-backed property
      SETTERS: set_MaxHealth crashes the game; getters fine).
      Clone lives in the factoriobot session scratchpad. Then
      vitality + regeneration come off the ice.
- [ ] Framework: hot reload recaptures live (already-boosted)
      values as vanilla baselines; persist vanilla in the store
      or re-zero effects on shutdown.

## Loot drops (after levelling works)

- [x] Loot table v1: cash drop on NPC kill, amount scaled by mob
      toughness (rolled; specifics behind the spoiler firewall
      in src/loot.rs), via the proven cash-template clone +
      FishNet spawn recipe. CONFIRMED in-game 2026-08-08.
- [ ] Exit gate remainder: no orphaned pickups after
      save/reload (drops are not saveable scene objects; verify
      what happens to unclaimed drops across a reload).
- [ ] Item drops (beyond cash) once cash drops are proven.

## Mob farming areas (after loot works)

Design direction from the operator (2026-08-08): Diablo 2/3 and
Path of Exile style mob variety keeps the killing fun. Mobs roll
MODIFIER TYPES (the Diablo champion/rare affix model: extra
fast, extra strong, regenerating, deadly, tough, and similar);
harder mobs roll MORE types at once; more types = more XP and
better loot. The affix list, visuals (how the player reads a
mob's types), and per-region difficulty come with this slice.

- [ ] Per-region mob spawner on the vanilla spawn machinery:
      density per region, respawn timer, despawn when the player
      leaves.
- [ ] Mob modifier types (the Diablo affix model above): roll on
      spawn, applied via the goon's own stats (NPCHealth
      MaxHealth, movement speed, damage); affix count scales
      with region difficulty; XP and loot scale with affix
      count.
- [ ] Mob stats scale with player level (never trivializes).
- [ ] Exit gate: operator farms one region for several respawn
      cycles; kills grant XP + loot; MelonLoader log clean.

## Custom NPCs (the war's supply; next session's slice)

Operator decision 2026-08-08: the war needs far more than the
vanilla 5-goon pool; build custom NPCs: GOONS, POLICE, and
PLAYER NPCs to start. Research done (research.md question 5):
live-goon cloning is a dead end (three init-layer failure
stacks captured); the working recipe is S1API's registered-
prefab path, and S1API is already in the operator's mod stack.

- [x] DONE 2026-08-08 (consolidated in the shim per the
      operator): S1ApiNpcs.cs defines GoonNpc / PoliceNpc /
      PlayerNpc (S1API subclasses) + NpcFactory public statics
      (SpawnGoon/SpawnPolice/SpawnPlayerNpc/CustomNpcCount)
      reached via invoke_static; construction + reflection call
      into S1API's RegisterCustomNpcForNetworking queues the
      real network-spawn pipeline. PROVEN IN-GAME: five minted
      NPCs visible and solid beside the player
      (tests/research_custom_npcs.rs). The 5-goon supply cap is
      dead.
- [x] Minted-NPC combat PROVEN 2026-08-08: two minted goons,
      armed via S1API (knife + baton, weapons by Resources
      path, guns available: M1911/PumpShotgun/Revolver),
      ordered onto EACH OTHER via
      CombatBehaviour.SetAndAttackTarget, fought with no player
      involvement. NpcFactory: index-tracked mints +
      AttackPlayer / AttackNpc / Arm.
- [ ] Melee separation polish: NPC-vs-NPC brawlers path into
      each other (models collide) while correctly attacking
      (timeWithinAttackRange counts; field-backed combat config
      identical to vanilla per research_combat_tuning.rs).
      Look at non-field-backed combat props or add separation.
- [ ] Cosmetics: real appearances (police uniform, goon looks)
      and name pools via S1API's appearance/identity APIs; the
      operator saw default bodies + placeholder names (expected
      for v1).
- [ ] Re-base farming's forces on minted NPCs (vanilla goons
      stay for vanilla systems); prove in-game: spawn 10+ of
      our goons, they fight, die, pay XP/loot/influence.
- [ ] Arm them: weapons cost cash (the war economy money sink);
      vanilla Ambush carries the arming machinery
      (RangedWeapons/MeleeWeapons, rank-gated) worth studying.

## Faction war (in slices, each verified in-game)

- [ ] Ownership map: factions own regions; `faction_state` op.
- [ ] NPC-vs-player contests: attacks on the player and their
      dealing areas, frequency up, enemy stats scale with player
      level.
- [ ] Territory pressure: losing a region costs the player
      (customers/dealers in that region).
- [ ] Player takeover: clearing a region's mobs/influence flips
      ownership to the player; holding it pays off (customers/
      dealers safe there).
- [ ] NPC-vs-NPC contests: factions fight each other for regions
      without player involvement.
- [ ] Director split: random event rolls and adaptive pressure as
      two separate layers, never merged.
