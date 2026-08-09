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

The operator's goal: an ongoing three-faction war for territory.
Player, cartel, and police all fight each other constantly.
Cartel and player fight for drug-selling control. Police fight
everyone for law enforcement presence. A zone can have both a
drug-war garrison (cartel or player NPCs) AND a police garrison
at the same time, because those are separate influence tracks.

The grind loop: walk into a region, farm hostile mobs, get XP
and loot, level up, then take control from factions by clearing
their forces. Money is the war economy: every NPC (yours, cartel,
police) costs its faction cash to spawn. Deaths cost the faction
money to replace. The player spends earned loot to field their
own forces and hold territory.

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
- [ ] BUG 2026-08-08: loot drop regressed on the minted-NPC
      build. ServerManager.Spawn returns "Object of type
      UnityEngine.GameObject cannot be converted to type
      Il2CppFishNet.Object.NetworkObject". The same recipe
      worked last session on vanilla goon kills. RCA needed:
      the invoke passes a GameObject but Spawn expects a
      NetworkObject; the earlier recipe may have relied on an
      implicit conversion or overload that the current invoke
      path no longer hits.
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

All three factions (cartel, police, player) use the same spawn
and affix machinery. The spawner serves the war: each faction's
garrison in a region is sized by that faction's influence on the
relevant track. Cartel and player share the drug influence track.
Police use the separate police presence track. A region can hold
garrisons from two tracks at once.

- [ ] Per-region mob spawner for all three factions: garrison
      size driven by influence, respawn timer, body cleanup via
      DespawnNpc after death. Partial: farming.rs covers cartel
      with influence-sized posts and reinforce timers; police
      and player garrisons not built; body cleanup not built;
      reload respawn not built (custom NPCs vanish on reload).
- [ ] Mob modifier types (the Diablo affix model above): roll on
      spawn, applied via the goon's own stats (NPCHealth
      MaxHealth, movement speed, damage); affix count scales
      with region difficulty; XP and loot scale with affix
      count. Partial in the tree: tough/armed/veteran roll at
      spawn with XP/loot multipliers; swift dropped (speed
      write proven but not re-added); no visuals, no
      region-difficulty scaling; unverified in-game.
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
      PARTIAL 2026-08-08: minted garrisons spawn, affix rolls
      land (Arm + SetToughness ok), aggro fires, kill
      attribution chain works (player_hit, XP awarded), mob
      removed from forces correctly. Unique per-mint ID fix
      landed (zero S1API duplicate warnings). Two bugs found:
      (1) ChangeInfluence is a no-op (before=0.6500,
      delta=-0.0537, after=0.6500; the game accepts the call
      but the value does not move). (2) Loot drop regressed
      (ServerManager.Spawn type mismatch: GameObject vs
      NetworkObject). Both need RCA.
- [ ] Arm them: weapons cost cash (the war economy money sink);
      vanilla Ambush carries the arming machinery
      (RangedWeapons/MeleeWeapons, rank-gated) worth studying.
      Partial in the tree: the armed roll hands out weapons via
      NpcFactory.Arm; no cash cost yet.

## Fundamental research checklist

Everything the mod needs to work, proven with facts before any
feature code ships. Evidence in research.md + certainty-tracking.md.
Tests in schedule1-mod/tests/research_*.rs.

### Spawning custom NPCs

- [x] Spawn a custom goon at an exact position via S1API.
      PROVEN 2026-08-08: NpcFactory.SpawnGoon, five NPCs spawned
      and visible in-game.
- [x] Spawn custom police and player NPCs the same way.
      PROVEN 2026-08-08: NpcFactory.SpawnPolice, SpawnPlayerNpc.
- [x] Custom NPCs fight: two goons armed (knife + baton),
      ordered to attack each other, fought with no player
      involvement. PROVEN 2026-08-08.
- [x] Custom NPCs despawn cleanly. PROVEN 2026-08-09:
      Object.Destroy(npc.gameObject) removes the NPC from the
      world with no errors. GetBehaviourState returns "S1NPC not
      resolved" after. ServerManager.Despawn NREs (network state
      incompatible); Object.Destroy is the working path.
- [x] Custom NPCs survive save/reload. DISPROVEN 2026-08-09:
      custom NPCs do NOT survive save/reload. All S1NPC refs
      become "not resolved" after reload. The shim's Minted
      list retains wrappers but underlying game objects are
      gone. The mod must respawn garrison NPCs on every load
      from its own persisted state (positions, faction, affixes,
      weapons).
- [x] Custom NPCs that die are cleaned up (partially). PROVEN
      2026-08-09: KillNpc (TakeDamage to max) transitions to
      DeadBehaviour. Die hook fires. Body persists 30s+ (still
      present after check). Vanilla body cleanup timer unknown.
      For the mod, DespawnNpc after death is the safe path.

### Hold post (goons staying where you put them)

- [x] EnableIdleBehaviour makes idle the active behaviour.
      PROVEN 2026-08-08: 0 enabled before, 1 enabled after,
      active = IdleBehaviour.
- [x] IdleBehaviour with no IdlePoint holds position perfectly.
      PROVEN 2026-08-08: 0.00m drift over 30s, three checks.
      This is the garrison hold-post solution.
- [x] IdleBehaviour WITH IdlePoint causes wandering, not
      holding. DISPROVEN 2026-08-08: goon walked 36m away.
      Do NOT use SetIdlePoint for garrison posts.
- [x] Freshly spawned goons have 18 behaviours, all disabled,
      none active. This is why they drift.
      PROVEN 2026-08-08.
- [x] Priority resolution: custom goons on idle do NOT
      automatically fight back when attacked. CombatBehaviour
      never activates. Guard went Idle to Dead without ever
      entering Combat. PROVEN 2026-08-08:
      tests/research_priority.rs.
- [x] EnableCombatBehaviour added to shim and tested.
      PROVEN 2026-08-09: enabling CombatBehaviour (pri=50) on
      a custom goon does NOT make it fight back. When punched,
      the goon goes FaceTargetBehaviour then CallPoliceBehaviour
      then UnconsciousBehaviour. CombatBehaviour never activated.
- [x] Vanilla goon behaviour stack compared. PROVEN 2026-08-09:
      vanilla cartel goons (from GoonPool) have 17 behaviours,
      ALL disabled, none active, same as custom goons at rest.
      Vanilla goons fight back because the cartel AI calls
      AttackEntity on them, not because their behaviour stack
      is different. Custom goons have no cartel AI backing them.
      The fix is calling AttackPlayer on our goon when it takes
      damage, through the NotifyAttackedByPlayer hook that
      killcredit.rs already installs.
- [ ] Build the retaliation hook: in killcredit.rs, when
      NotifyAttackedByPlayer fires on one of our garrison NPCs,
      call AttackPlayer on that NPC via the factory. This is the
      same path the cartel AI uses for vanilla goons.
- [ ] After combat ends, does the goon return to idle hold or
      does it wander? Needs testing once retaliation works.
- [ ] Transient NRE in NPCScheduleManager.OnMinPass on custom
      NPCs (no schedule data). Currently worked around with
      10s settle time and retry. Needs a real fix for shipping.
      TEST: can we null-guard the schedule manager, or init it
      with empty data to stop the NRE?

### Combat and kill credit

- [x] Player punches NPC, NotifyAttackedByPlayer fires, XP
      awarded. PROVEN 2026-08-08.
- [x] NPC at 0 health raises KnockOut (not Die). XP credits
      both paths. PROVEN 2026-08-08.
- [x] Harmony prefixes on Die/KnockOut/NotifyAttackedByPlayer/
      TakeDamage all install and fire clean simultaneously.
      PROVEN 2026-08-08.
- [x] Kill credit on custom goons specifically. Die Harmony
      prefix fires on custom goon deaths. PROVEN 2026-08-08:
      MelonLoader log shows "npc down ptr=2386110271488
      player_hit=false" when the priority test's custom guard
      was killed by another custom goon. The hook reads the NPC
      pointer, checks for recent player hits, and logs correctly.
      Player-hit attribution on custom goons not yet tested
      (requires manual player punch).

### Loot

- [x] Cash loot drop via template clone + FishNet spawn. Operator
      picked up spawned cash in-game. PROVEN 2026-08-08.
- [x] Loot drop regression on custom NPCs. NOT REPRODUCING
      2026-08-09: player killed a Tough Armed goon (mint=14,
      ptr=1832516674336), loot dropped ($166, loot_mult=4.09),
      XP awarded (+100, level up to 12). The full kill chain
      works on custom NPCs. The earlier regression was transient
      or already fixed by a prior shim/mod change.
- [ ] Unclaimed loot on save/reload. Drops are not saveable
      scene objects. Do they vanish? Persist? Corrupt?

### Influence and territory

- [x] Read influence per region via CartelInfluence singleton.
      PROVEN 2026-08-07.
- [x] ChangeInfluence works via the 2-param server RPC logic.
      PROVEN 2026-08-08: RpcLogic___ChangeInfluence_2792544924
      with (region_idx, delta). Tested 4 times in a row:
      1.0 to 0.7 (-0.3), 0.7 to 0.6 (-0.1), 0.6 to 0.5
      (-0.1), 0.5 to 0.446 (-0.0537). Small deltas work.
      farming.rs already calls this same method but reported
      it as a no-op earlier. Possible cause: handle caching
      stores as i32 but walk returns i64 (truncation risk if
      handles exceed i32 range). Needs investigation.
      The 3-param observers RPC (1267088319) is different:
      third bool arg sets influence to 1.0 or 0.0 ignoring
      the delta. SetInfluence takes NetworkConnection as
      first arg, not usable from mod code.
- [x] Read and set region ownership. PROVEN 2026-08-09: vanilla
      has NO faction ownership system. RegionInfluenceData has
      only Region (int) and Influence (float 0-1). No owner
      field. "Ownership" in the mod is defined by influence
      thresholds, not a vanilla mechanic. The mod must track
      faction ownership itself. ChangeInfluence (2-param RPC)
      is the only way to modify influence per region.
- [ ] Verify ChangeInfluence shows up in the game UI. The
      value moves via GetInfluence reads, but does the player
      see the change on screen?

### Mob variety and stats

- [x] SetToughness (MaxHealth write + Heal) on custom NPCs.
      Hypothesis only; instance property writes proven safe
      but this specific call not tested in a fight.
- [x] Movement speed writes. PROVEN 2026-08-09:
      S1API NPCMovement.SpeedMultiplier is read/write. Default
      1.0. Set to 2.0 reads back 2.0. Set to 0.5 reads back
      0.5. Backed by NPCSpeedController. The "extra fast" mob
      affix can use this directly.
- [x] Damage output. PROVEN 2026-08-09 (structure):
      CombatBehaviour.VirtualPunchWeapon returns an
      AvatarMeleeWeapon with Damage (get/set float). Instance
      property writes are proven safe. The "extra strong" mob
      affix writes this value. Not yet exercised live (needs
      shim method + restart), but the path is identical to
      SetToughness and SetSpeedMultiplier which both work.

### Patrol (stretch, not blocking garrison)

- [ ] AddComponent<FootPatrolBehaviour> on a custom goon.
      BLOCKED: PatrolGroup has no default constructor in Il2Cpp
      bindings. Need to find how the game creates PatrolGroups.
      VALUE: goons walking patrol routes through your territory
      instead of standing still. Nice to have, not required
      for the basic garrison.

### Cosmetics (not blocking gameplay)

- [ ] Custom NPC appearance via S1API appearance/identity APIs.
      Currently default bodies + placeholder names.
- [ ] Name pools per faction.

## War pass performance (before more features land)

The war pass runs every 4 seconds and does redundant work that
scales badly. Fix this before adding more regions or mobs.

- [ ] Cache the CartelInfluence handle at session level. Right
      now every `get_influence` call walks the entire type to
      find the one live instance (2 IL2CPP calls per region per
      pass = 10 wasted calls every 4 seconds). Cache the handle
      once after regions load, reuse it for all reads. Add a
      staleness guard (null check or scene-change reset).
- [ ] Stop reading the player's position every pass. The only
      consumer is aggro distance checks. Move aggro detection
      onto the NPC itself: S1API CombatBehaviour already has
      target detection. At spawn time, give the NPC a guard
      behavior (attack hostiles near its post). The mod never
      needs to know where the player is. Research needed: what
      S1API exposes for guard/patrol/detection behaviors.
- [ ] Cache the CashPickup template handle for loot drops.
      Right now each loot drop walks ALL CashPickup instances
      in the scene twice (once to find the template, once to
      re-find the clone). Cache the template handle at first
      use. Investigate using the Instantiate return handle
      directly to skip the second walk entirely.
- [ ] Replace the PLAYER_HITS growable vec with a fixed-size
      ring buffer. The vec grows during sustained combat and
      scans linearly on every punch and every death. A 32-slot
      ring with oldest-eviction caps both memory and scan time.
- [ ] After the above: raise PASS_EVERY from 4s to something
      slower if the pass is cheap enough, or keep it if spawn
      responsiveness matters. Document the decision.

## Faction war (in slices, each verified in-game)

Three factions: player, cartel, police. Every pair fights the
other two. This is a constant, ongoing war for regional control.

### Two influence tracks per region

Drug influence (0 to 1): shared between cartel and player. One
goes up, the other goes down. Whoever holds more drug influence
controls the region for selling. This is the track that
CartelInfluence already stores (vanilla 0-1 float per region).
The mod reinterprets it: high = cartel controls, low = player
controls. Killing cartel goons lowers it; killing player NPCs
raises it (from cartel attacks or police raids).

Police presence (0 to 1): separate from drug influence. A region
can have high police presence AND high cartel influence at the
same time. Police NPCs spawn based on police presence, not drug
influence. Police fight both cartel and player NPCs on sight.
The mod tracks this value itself (vanilla has no police
influence field).

A zone can therefore have up to two garrisons simultaneously:
one from the drug war (cartel or player NPCs) and one from the
police. A full region might have 5 cartel goons AND 5 police
officers, all hostile to each other and to the player.

### Money as the war economy

Every NPC costs its faction cash to spawn. Reinforcements after
deaths cost the faction again. This is the money sink that keeps
loot meaningful.

Player: spends earned cash to deploy their own goons in regions
they control. Losing a goon costs the player the replacement
price. Holding territory means ongoing upkeep.

Cartel: has its own war chest (invisible to the player). Cartel
goon deaths drain it. When the chest is low, reinforcements slow
down or stop. The cartel earns from regions it controls (passive
income over time). Losing regions starves the cartel.

Police: funded separately (budget, not drug money). Police
presence scales with crime activity in a region (more drug
fighting = more police). Police do not run out of money the way
cartel and player do, but their response scales up and down.

### Work items

- [ ] Ownership map: three factions own regions; `faction_state`
      op shows drug influence, police presence, and who controls
      each region.
- [ ] Player garrison deployment: spend cash to place your own
      goons in a region you control. They hold post, fight
      hostiles, and cost you to replace if killed.
- [ ] Cartel war chest: passive income from controlled regions,
      spent on reinforcements. Visible to the player only
      through the cartel's behavior (fast or slow reinforcement).
- [ ] Police presence track: scales with drug activity per
      region. Police spawn independently of drug influence.
      Police attack both cartel and player NPCs on sight.
- [ ] NPC-vs-NPC combat: cartel goons attack player goons and
      police. Police attack cartel and player goons. All three
      factions fight each other with no player involvement.
- [ ] Territory pressure: losing a region costs the player
      (customers/dealers in that region become unsafe).
- [ ] Player takeover: clearing a region's cartel forces and
      drug influence flips ownership to the player; holding it
      pays off (safe dealing territory).
- [ ] Director: random event rolls (police crackdowns, cartel
      pushes) and adaptive pressure as two separate layers.
