# schedule1-mod: research

How to read and change the live game. No scores here; scores
live in [status.md](status.md). Claims about vanilla internals
are tracked in [certainty-tracking.md](certainty-tracking.md).

## Control plane

The mod answers HTTP on `127.0.0.1:17175/op` (the modforge
default 17173 is held by eufy-capture on the operator's
machine). Ops: `ping`, `list_ops`, `walk_class`,
`inspect_object`, `read_field`, `write_field`, `invoke_method`,
`list_singletons`, selectors per unityforge.

Every probe ships as a test under `tests/` using
`modforge::client::Api` (never ad-hoc curl). Run pattern:

```text
cargo test -p schedule1-mod --test <name>. --test-threads=1 --nocapture
```

Tests SKIP (pass with a printed reason) when the game is not
running, so the workspace suite stays green.

## Deploy

```text
k3sc cargo-lock build --release -p schedule1-mod
copy target/x86_64-pc-windows-msvc/release/schedule1_mod.dll
  -> <game>/Mods/schedule1_mod.unityforge.dll
```

The MelonLoader shim requires exactly ONE `*.unityforge.dll` in
Mods/, so this replaces il2cpp_smoke.unityforge.dll. First
deploy needs a game launch; after that, drop
`schedule1_mod.unityforge.gen<N>.dll` for hot reload without a
restart (proven live on this game 2026-08-07).

The game runs IL2CPP (default branch) on MelonLoader 0.7.3 with
the operator's patched Il2CppInterop generator; see
`docs/schedule1-todo.md` in the repo root for that story.

## Environment facts (verified 2026-08-07)

- Game: Schedule I 0.4.6f11, Unity 2022.3.62f2, IL2CPP,
  install `C:\Games\Steam\steamapps\common\Schedule I`.
- Game types live under `Il2CppScheduleOne.*` namespaces in the
  interop assemblies (`MelonLoader/Il2CppAssemblies/`); dnSpy or
  metadata reads over those complement live control-plane probes.
- The operator's Vortex mods share the game; several are broken
  on 0.4.6 (their authors' catch-up, not ours).

## Open research questions

Answers gate the gameplay work (docs/schedule1-plan.md). Each
answer lands here with its evidence and a row in
certainty-tracking.md.

1. Map regions: ANSWERED 2026-08-07, proven live by
   `tests/research_map.rs`. `ScheduleOne.Map.Map` (singleton)
   owns `Regions: MapRegionData[]` (6 regions). Each
   MapRegionData carries Name, Region (EMapRegion), IsUnlocked,
   UnlockedByDefault, RankRequirement, StartingNPCs: NPC[],
   AdjacentRegions, RegionBounds: PolygonalZone,
   RegionDeliveryLocations. EMapRegion mapping (proven live):
   0=Northtown (rank 0), 1=Westville (rank 1), 2=Downtown
   (rank 3), 3=Docks (rank 5), 4=Suburbia (rank 7), 5=Uptown
   (rank 9). `ScheduleOne.Cartel.CartelInfluence` (singleton, a
   FishNet NetworkBehaviour) holds `regionInfluence:
   List<RegionInfluenceData>` ({Region, Influence: 0..1}) and
   answers `GetInfluence(EMapRegion)`. Live values seen:
   Northtown 0.0, Westville 0.3, Downtown 0.1, Docks 0.65,
   Suburbia 1.0, Uptown 0.85. The vanilla cartel machinery
   (`ScheduleOne.Cartel.*`: Ambush, CartelActivities,
   CartelAmbushLocation, RobDealer, StealDeadDrop) already
   models faction pressure on regions.
2. NPCs: PARTLY ANSWERED 2026-08-07 (tests/research_npcs.rs).
   135 live `ScheduleOne.NPCs.NPC` instances in the operator's
   save. `NPCManager` keeps a static `NPCRegistry: List<NPC>`
   and static `GetNPC(name)`. Each NPC owns an `NPCHealth`.
   Still open: the spawn/despawn path (what creates an NPC at
   runtime) and the cartel/goon classes
   (CharacterClasses so far: Oscar, Ray, SewerGoblin; the
   hostile-NPC classes need a dedicated walk).
3. Combat: ANSWERED 2026-08-08 (tests/research_killcredit.rs,
   combat_trace ops in src/combat_trace.rs). `NPCHealth` (a
   FishNet NetworkBehaviour) is the whole per-NPC life state:
   Health as a SyncVar<float>, MaxHealth, IsDead, IsKnockedOut,
   `TakeDamage`, `Die()`, `KnockOut()`, `Revive()`,
   `NotifyAttackedByPlayer(int)`, and UnityEvents onDie /
   onKnockedOut / onDieOrKnockedOut / onRevive.
   KILL ATTRIBUTION PROVEN LIVE: on every player melee hit,
   `NotifyAttackedByPlayer` fires in the same frame as
   `TakeDamage` (operator punched a Dealer and an NPC to 0
   health; 5 hit pairs each, health 100 -> 78.9 -> 57.6 ->
   34.9 -> 13.7 -> down). TakeDamage(Single, Boolean, Boolean)
   carries no attacker, so NotifyAttackedByPlayer IS the
   attribution signal: XP credit = prefix on it recording
   (npc ptr, time), then the down hook checks for a recent
   player hit on that NPC. Punching to 0 raises `KnockOut`,
   NOT `Die`: the XP hook must credit both down paths.
   The prefix reads pre-hit Health, so per-hit damage is
   derivable. Aggro is AttackEntity (question 2b).
2b. Mob spawning: ANSWERED 2026-08-07 (tests/research_cartel.rs).
   `ScheduleOne.Cartel.Cartel` (singleton) is the faction brain:
   Status (ECartelStatus), HourPass tick, and it owns
   Activities, Influence, GoonPool, DealManager. Its `GoonPool`
   (1 live instance) has PUBLIC `SpawnGoon(Vector3) ->
   CartelGoon` and `SpawnMultipleGoons(Vector3, int, bool) ->
   List<CartelGoon>`, plus `ReturnToPool(goon)`, pooled
   spawned/unspawned lists, and a full appearance randomizer.
   5 CartelGoon instances were live at scan time. Mob farming
   rides this pool; no from-nothing spawning needed.
   CartelGoon's own method list overflows list_methods' 64KB
   buffer (control-plane nit, not a blocker).
   SPAWN PROVEN IN-GAME 2026-08-07 (tests/research_spawn.rs):
   goons spawned at the player's position, visible and
   interactable. Lesson: an untasked goon walks to the nearest
   exit building and leaves.
   AGGRO PROVEN IN-GAME 2026-08-08 (tests/research_attack.rs):
   `CartelGoon.AttackEntity(ICombatTargetable, bool)` with the
   player as target makes the goon hunt and attack (operator
   confirmed being attacked). The full farmable-mob loop is
   spawn -> AttackEntity -> fight; the {"$handle": N} invoke
   arg passes live objects.

4b. Loot path: ANSWERED 2026-08-08 (tests/research_pickup.rs;
   operator picked up spawned cash in-game: "THAT WORKED").
   `ScheduleOne.ItemFramework.ItemPickup` (70 live instances)
   is the ground-loot object: `ItemToGive: ItemDefinition`,
   `Pickup()`, `DestroyOnPickup`, `onPickup` UnityEvent; there
   is also a NetworkedItemPickup and a CashPickup (Value:
   float SyncVar). `ScheduleOne.Economy.DeadDrop` (25 live,
   static `DeadDrops` list, `GetRandomEmptyDrop`) is the
   stash-loot alternative.
   THE CREATION RECIPE (proven live, cash): the game parks
   prefab templates "$10 Pickup" and "Dynamic Amount Cash
   Pickup" INACTIVE in a hidden container (walk_class finds
   them, activeInHierarchy=false, both at one point). Spawning
   ground cash at a position:
   1. UnityEngine.Object.Instantiate(template) via the
      invoke_static op (the returned proxy is base-typed;
      re-find the clone via walk_class, which downcasts;
      clones are named "<template>(Clone)").
   2. gameObject.SetActive(true) (clones inherit the
      template's hidden state).
   3. transform.set_position(target).
   4. FishNet: InstanceFinder.ServerManager.Spawn(clone, null,
      default Scene). MANDATORY: un-spawned NetworkObject
      clones are destroyed by the engine within seconds (17
      earlier clones vanished from the walk).
   5. write Value, invoke UpdateCashStackVisuals().
   TrashManager.CreateTrashItem(String, Vector3, Quaternion,
   Vector3, String, Boolean) -> TrashItem is the public
   by-name world-item creation call for trash. DropCash /
   DropItem RPCs exist in metadata but no walked class
   declares them; not needed, the recipe above covers loot.

4. Where kills are observable: ANSWERED 2026-08-07.
   `NPCHealth.Die` and `NPCHealth.KnockOut` are both
   Harmony-patchable live (harmony_probe patched and unpatched
   them clean on the running game). Combat XP hooks there;
   attribution (was it the player's kill) still needs the
   damage-source trail from question 3.
