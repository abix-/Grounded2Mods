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

5. NPC supply (custom NPCs): ANSWERED IN PRINCIPLE 2026-08-08
   (tests/research_pool_grow.rs + research_prefabs.rs). The
   vanilla cartel fields exactly 5 goon objects (GoonPool.goons,
   fixed array; UnspawnedGoonCount tracks them; killed goons
   recycle). Cloning a live goon CANNOT be spawned: each init
   layer faults in turn (NPCInventory null lists unless the
   clone is activated first; ConfigureGoonSettings RPC needs an
   initialized NetworkObject; ServerManager.Spawn dies in
   NPC.OnStartServer on missing registry state); full stacks in
   the test output 2026-08-08. THE way (S1API's, read from its
   source): clone a REGISTERED FishNet prefab from
   NetworkManager.SpawnablePrefabs (104 entries live; the NPC
   base here is "BaseEmployee"; also "Player"; NO goon or police
   prefab), normalize inactive, spawnablePrefabs.AddObject the
   new prefab, then network-spawn instances of it. Decision:
   custom NPCs (our goons, police, player garrison) ride S1API
   itself (already in the operator's mod stack); the shim
   defines the NPC types and exposes spawn_custom_npc.
6. Where kills are observable: ANSWERED 2026-08-07.
   `NPCHealth.Die` and `NPCHealth.KnockOut` are both
   Harmony-patchable live (harmony_probe patched and unpatched
   them clean on the running game). Combat XP hooks there;
   attribution (was it the player's kill) still needs the
   damage-source trail from question 3.
7. Goon behavior (patrol/area defense): RESEARCHED 2026-08-08.
   Minted goons wander from their posts because GoonNpc has no
   schedule and OnCreated does not call Schedule.Enable(). The
   S1API behavior stack (StationaryBehaviour, CombatBehaviour,
   FleeBehaviour, ScheduleBehaviour, etc.) initializes on every
   custom NPC via NPC.InitializeBehaviourComponents, but without
   a configured schedule or hold behavior, idle goons drift.
   Our Rust hold orders only fire when the player is in aggro
   range (HOLD_ORDER_EVERY=12s); outside that range, nothing
   pins the goon to its post.

   SYSTEMS RESEARCHED (S1API source, 2026-08-08):
   a) S1API Schedule system (PrefabScheduleBuilder.WalkTo).
      Game-clock based: times are minutes from midnight in 24h
      format (900=9AM, 1800=6PM). Designed for civilian daily
      routines. A WalkTo entry fires once per in-game day at its
      scheduled time. There is no looping or repeat mechanism.
      Two WalkTo entries (point A at 6AM, point B at noon) would
      move the goon twice per day, not continuously patrol.
      Source: S1API/Entities/Schedule/NPCScheduleBuilder.cs,
      S1API/docs/scheduling-system.md. NOT suited for continuous
      back-and-forth patrol.
   b) NPCMovement (S1API.Entities.NPCMovement). Exposes
      SetDestination(Vector3), Stop(), Warp(Vector3),
      IsMoving (bool), SpeedMultiplier, CanGetTo(Vector3),
      FaceDirection, FacePoint. SetDestination uses the game's
      NavMesh pathfinding. Already proven live (goon spawning
      uses Movement.Warp for positioning). A patrol loop: send
      to waypoint A, poll IsMoving, when false send to waypoint
      B, repeat. Driven by our existing 4s tick. This is the
      game's own movement system, just alternated by our tick.
      Source: S1API/Entities/NPCMovement.cs.
   c) Vanilla FootPatrolRoute (ScheduleOne.NPCs.Behaviour).
      PROBED LIVE 2026-08-08 (tests/research_patrol.rs):
      10 FootPatrolRoute instances exist in the scene, each a
      MonoBehaviour with RouteName (string), Waypoints
      (Transform[]), StartWaypointIndex (int). Routes found:
      North-west loop (11 wp), North Loop 1 (9 wp), North
      Loop 2 (8 wp), Uptown Loop (21 wp), Residential Loop
      (10 wp), Town Square Loop (6 wp), Casino Loop (15 wp),
      Motel Loop (13 wp), Docks Loop (12 wp), Western Loop
      (13 wp). All waypoints have Y near 0 (street level) or
      negative (lower areas like docks at -2.5, west at -4.0).

      PatrolGroup (ScheduleOne.NPCs.Behaviour.PatrolGroup)
      wraps a Members list, CurrentWaypoint index, Route ref.
      Methods: AdvanceGroup(), IsGroupReadyToAdvance(),
      IsPaused(), DisbandGroup(). Zero live instances (no
      police patrols active in the operator's save).

      LawManager.StartFootpatrol(FootPatrolRoute, int) creates
      a PatrolGroup by pulling officers from the nearest police
      station. It does NOT accept arbitrary NPCs. The patrol
      system is police-specific at the creation layer.

      CRITICAL FINDING 1: all three classes (FootPatrolRoute,
      PatrolGroup, LawManager) show ZERO methods in IL2CPP
      metadata (list_methods returns empty). NPC and
      NPCMovement also show zero methods. This is the 0.4.6f12
      metadata stripping issue. The methods exist and work
      through S1API's compiled Il2CppInterop bindings, but
      cannot be called via our control plane's invoke_method.
      We MUST go through the C# shim (NpcFactory).

      CRITICAL FINDING 2: FootPatrolBehaviour is the actual
      behavior component that drives patrol. Live on every
      PoliceOfficer (10 instances, matching 10 officers).
      Fields (probed live):
        Group: PatrolGroup (back-ref to the group)
        Priority: 3 (in the behavior stack)
        BehaviourIndex: 19
        Active/Enabled: both false when not patrolling
        Name: "Foot patrol"
        UseFlashlight: true
        Npc: back-ref to owning NPC
        consecutivePathingFailures: 0
      The chain (proven by field refs): PoliceOfficer owns
      FootPatrolBehaviour. FootPatrolBehaviour.Group points
      to a PatrolGroup. PatrolGroup.Route points to a
      FootPatrolRoute (the waypoints). INFERRED (from S1API
      source, not observed): when enabled, it reads the
      current waypoint, tells the NPC to walk there, and
      AdvanceGroup moves all members to the next waypoint
      when all arrive.
      Source: tests/research_patrol.rs live inspect.

      Also found on PoliceOfficer:
        SentryBehaviour (stationary guard post)
        PursuitBehaviour (chase player)
        VehiclePatrolBehaviour (car patrol)
        CheckpointBehaviour (checkpoint duty)
        BodySearchBehaviour (search player)
      These are all ScheduleOne.NPCs.Behaviour.Behaviour
      subclasses in the behavior stack, same as
      CombatBehaviour and FleeBehaviour on custom NPCs.

      OPEN QUESTION: can we AddComponent a
      FootPatrolBehaviour to a minted S1API NPC, create a
      PatrolGroup, assign a route, and let the game's own
      patrol system drive movement? If yes, no custom patrol
      loop is needed. If the component requires police-
      specific initialization, we fall back to option A
      (SetDestination from the shim).

   d) Vanilla CartelGoon patrol. The game's own CartelGoon has
      patrol/area-defense behavior baked in, but it is NOT
      exposed through S1API's CartelGoon wrapper (only WarpTo,
      AttackPlayer, Attack, Despawn, SetDefaultWeapon). The
      internal behavior lives in the compiled IL2CPP binary.
      Source: S1API/Cartel/CartelGoon.cs wraps
      ScheduleOne.Cartel.CartelGoon.

   OPTIONS (ranked by practicality):
   Option A: NPCMovement.SetDestination via NpcFactory shim.
     Add shim factory methods: SendToPosition(index, x, y, z)
     and IsMoving(index). These call S1API's NPCMovement API
     (which has working Il2CppInterop bindings, unlike the
     stripped metadata). Each goon gets two waypoints (its post
     and a nearby offset). The Rust tick (already 4s) checks
     IsMoving and alternates destinations. Uses the game's own
     NavMesh pathfinding. When the player enters aggro range,
     CombatBehaviour takes priority (it outranks movement in
     the behavior stack). After combat, the tick resumes patrol.
     Pros: uses existing S1API API, integrates with behavior
     stack priority, minimal shim changes (two methods).
     Cons: patrol cadence is tick-driven (4s poll), not
     frame-perfect.
   Option B: S1API Schedule (WalkTo + game clock).
     NOT viable for continuous patrol. The schedule system fires
     actions at specific times of day, once per day. A goon
     schedule with two WalkTo entries would move twice per day,
     then idle. There is no repeat/loop mechanism.
   Option C: Reuse vanilla FootPatrolRoute data + custom driver.
     The 10 existing FootPatrolRoutes have great waypoint data
     covering all map regions. We could read their waypoints
     and feed them into option A's SetDestination loop. This
     reuses the level designers' route planning without needing
     to construct PatrolGroup objects or hook into the police
     dispatch system. The route data is just Transform[]
     positions. We pick 2 nearby waypoints from the region's
     route as the goon's patrol path.
     Pros: reuses level-designer waypoints, no new coordinates
     to author. Cons: still needs option A's movement driver.

   RECOMMENDATION: Option A + C combined. Read waypoint
   positions from existing FootPatrolRoutes (per region) at
   startup. Use those as the patrol endpoints. Drive movement
   via NpcFactory.SendToPosition / IsMoving in the Rust tick.
   The vanilla route data gives us good patrol paths for free;
   the S1API movement API drives the actual walking.

   VANILLA PATROL ROUTE CATALOG (live probe 2026-08-08,
   tests/research_patrol.rs). Each route is a MonoBehaviour
   with a Transform[] of waypoints. All loops (the last
   waypoint leads back to the first). Waypoint Y is street
   level (0.0) or lower for sunken areas (docks at -2.5,
   western district at -4.0).

   How they work: FootPatrolRoute is pure data (waypoints +
   name). PatrolGroup is the runtime driver: it holds Members
   (list of NPCs), CurrentWaypoint (index into the route),
   and methods AdvanceGroup (move all members to the next
   waypoint), IsGroupReadyToAdvance (all members near the
   current waypoint), IsPaused, DisbandGroup. The game's
   LawManager.StartFootpatrol creates a PatrolGroup, assigns
   police officers from the nearest station, and the group
   loops through waypoints indefinitely. The route itself
   has no timing or speed data; those come from the NPCs'
   movement speed.

   1. North-west loop (11 waypoints)
      Covers: Northtown west side + Westville
      (27.6, 0, 31) -> (1.9, 0, 9.7) -> (-33.9, -3, 2.2)
      -> (-33.4, -3, 42.8) -> (-67.3, -3, 42.8)
      -> (-150.2, -4, 96.7) -> (-150.2, -4, 118.3)
      -> (-58.4, -4, 120.1) -> (-56.3, 0, 95)
      -> (-14.6, 0, 95) -> (-7.9, 0, 46.7)
      Long loop from town center out to the far western edge
      and back along the north.

   2. North Loop 1 (9 waypoints)
      Covers: Northtown north + Westville east
      (13.5, 0, 47) -> (-34.4, 0, 47) -> (-53.8, 0, 54)
      -> (-54.2, 0, 69.9) -> (-58.4, -4, 120.7)
      -> (-67, -4, 144.1) -> (-21.6, -4, 149.9)
      -> (-22.5, -4, 126.4) -> (-22.4, 0, 45)
      Sweeps north from Motel row, loops through the far
      north residential area.

   3. North Loop 2 (8 waypoints)
      Covers: Northtown north (tighter than North Loop 1)
      (10.6, 0, 54.9) -> (-14.3, 0, 55.2)
      -> (-14.6, -4, 128.7) -> (-34.6, -4, 141.5)
      -> (-38.6, -4, 173.5) -> (-44.6, -4, 126.4)
      -> (-22.5, -4, 126.4) -> (-22.4, 0, 45)
      Shares return waypoints with North Loop 1. Extends
      further north (Z=173.5, the map's northern edge).

   4. Uptown Loop (21 waypoints)
      Covers: Uptown (east side of map, high rank area)
      (27.5, 0, 47) -> (47, 0, 47) -> (47, 0, 69.5)
      -> (85, 0, 79.5) -> (111.5, 0, 72.5)
      -> (111.5, 0, 55) -> (127, 0, 55)
      -> (127.5, 0, 86.1) -> (135.5, 0, 81.2)
      -> (135.3, 0, 54.6) -> (118.5, 0, 47.1)
      -> (118.5, 0, 36.9) -> (124.5, 0, 36.9)
      -> (124.5, 0, 14.9) -> (95.5, 0, 15.1)
      -> (95.5, 0, 27.1) -> (85.7, 0, 34)
      -> (70, 0, 34) -> (70, 0, 43.4)
      -> (51.5, 0, 43.8) -> (35.1, 0, 42)
      Dense coverage of the wealthy eastern district. All at
      Y=0 (flat terrain). Longest route at 21 waypoints.

   5. Residential Loop (10 waypoints)
      Covers: Suburbia (south-east, residential area)
      (27.5, 0, 32) -> (27.5, 0, 7) -> (87.5, 0, 7)
      -> (95.5, 0, 3) -> (95.6, 4, -101)
      -> (-5.9, 0, -99.8) -> (-14.6, 0, -83.7)
      -> (-13.8, 0, -37.9) -> (27.3, 0, -14.6)
      -> (27.5, 0, 33.6)
      Large southern loop. Z goes deep negative (-101),
      the map's southern edge. One waypoint at Y=4
      (elevated road?).

   6. Town Square Loop (6 waypoints)
      Covers: Downtown (central business district)
      (27.5, 0, 47) -> (37.5, 0, 47) -> (37.5, 0, 55)
      -> (87.5, 0, 55) -> (87.5, 0, 15) -> (27.5, 0, 15)
      Tight rectangular loop. Only 6 waypoints. The town
      center / commercial area.

   7. Casino Loop (15 waypoints)
      Covers: Downtown west + Northtown south
      (27.5, 0, 32) -> (27.5, 0, 19.5) -> (2, 0, 10.5)
      -> (2, 0, 32.5) -> (-14.5, 0, 32.5)
      -> (-14.5, 0, 74.3) -> (7.4, 0, 74.3)
      -> (27.5, 0, 81) -> (29.6, 0, 99.9)
      -> (75, 0, 100.1) -> (67, 0, 78)
      -> (66, 0, 55) -> (70, 0, 45)
      -> (70, 0, 18.3) -> (38.6, 0, 18.1)
      Crosses between the casino area and the northern
      commercial strip.

   8. Motel Loop (13 waypoints)
      Covers: Northtown south + Westville east edge
      (13.5, 0, 47) -> (-12.5, 0, 47) -> (-14, 0, 55)
      -> (-14.5, 0, 95) -> (-52.5, 0, 95)
      -> (-54, 0, 62.5) -> (-54.4, 0, 54.2)
      -> (-34.3, 0, 46.7) -> (-23, 0, 46.5)
      -> (-14.5, 0, 44) -> (-14.5, 0, 32.5)
      -> (2, 0, 10.5) -> (27.5, 0, 19.5)
      Covers the motel row and surrounding streets.

   9. Docks Loop (12 waypoints)
      Covers: Docks (south-west industrial area)
      (27.5, 0, 27.5) -> (27.5, 0, 19) -> (-14.5, 0, 4)
      -> (-34, -2.5, 2) -> (-58.4, -2.5, -55.1)
      -> (-75.2, -2.5, -26.3) -> (-83.2, -2.5, -33.5)
      -> (-63.4, -2.5, -67.7) -> (-50.2, -2.5, -68.6)
      -> (-13.8, 0, -47.6) -> (34.5, 0, -18.9)
      -> (35.4, 0, 30)
      Drops to Y=-2.5 in the dock/harbor area. Covers the
      industrial waterfront south-west of town.

   10. Western Loop (13 waypoints)
       Covers: Westville (far western residential)
       (12.5, 0, 54.8) -> (-14.4, 0.7, 55.9)
       -> (-33.7, 0.7, 54.9) -> (-145.4, -4, 95.6)
       -> (-158.3, -4, 96.5) -> (-158.1, -4, 44.1)
       -> (-138.2, -4, 24.6) -> (-129.1, -4, 24.4)
       -> (-122.2, -4, 34.8) -> (-122, -4, 78.4)
       -> (-105.6, -4, 72.7) -> (-35.1, 0, 47.2)
       -> (5, 0, 47)
       Reaches the map's western edge (X=-158). Deep into
       the Westville residential area at Y=-4.

   ROUTE-TO-REGION MAPPING (best match based on waypoint
   coverage vs known region layout):
   Northtown: North Loop 1, North Loop 2, Motel Loop,
              Casino Loop (partial)
   Westville: North-west loop, Western Loop
   Downtown:  Town Square Loop, Casino Loop (partial)
   Docks:     Docks Loop
   Suburbia:  Residential Loop
   Uptown:    Uptown Loop

   KEY OBSERVATIONS for garrison patrol:
   - Routes are closed loops (last waypoint near first).
   - No timing data on routes; speed comes from the NPC.
   - Waypoints are spaced 20-80m apart (walkable segments).
   - Each region has at least one route with 6-21 waypoints.
   - For two-waypoint garrison patrol, pick any two adjacent
     waypoints from the region's route. The goon walks between
     them, covering a small section of the route.
   - Alternatively, assign the full route for wider coverage.
     The goon walks the whole loop if given all waypoints.

   FULL BEHAVIOUR COMPONENT INVENTORY (live probe 2026-08-08,
   tests/research_behaviours.rs). Every Behaviour subclass
   found in the scene, with instance counts and key fields.

   BEHAVIOURS S1API ALREADY ADDS TO CUSTOM NPCs
   (via NPC.InitializeBehaviourComponents, source:
   S1API/Entities/NPC.cs:3442):
     NPCBehaviour          - the manager/container for all others
     CoweringBehaviour     - cower when scared (Priority 25)
     HeavyFlinchBehaviour  - flinch on heavy hit
     FleeBehaviour         - run away (Priority 28)
     GenericDialogueBehaviour - conversation
     RequestProductBehaviour  - customer buy flow
     CallPoliceBehaviour      - call police on player
     CombatBehaviour       - fighting (namespace: ScheduleOne.Combat,
                             NOT ScheduleOne.NPCs.Behaviour)
     StationaryBehaviour   - stand still (Priority 18)
     FaceTargetBehaviour   - look at a target
     ConsumeProductBehaviour - use products
     UnconsciousBehaviour  - knocked out state
     DeadBehaviour         - death state
     ScheduleBehaviour     - follow game-clock schedule (Priority -1,
                             only if NPCScheduleManager exists)

   POLICE-ONLY BEHAVIOURS (on PoliceOfficer, not added by S1API):
     FootPatrolBehaviour     - walk waypoints (Priority 3, 10 live)
     VehiclePatrolBehaviour  - drive patrol route (Priority 32, 10 live)
     SentryBehaviour         - guard a SentryLocation (Priority 1, 10 live)
                               Fields: AssignedLocation (SentryLocation ref),
                               FlashlightMaxTime=500
     PursuitBehaviour        - chase a target (Priority 40, 10 live)
                               Fields: Target (ICombatTargetable), DebugTarget,
                               IsTargetImmediatelyVisible, IsTargetRecentlyVisible
     CheckpointBehaviour     - man a road checkpoint (Priority 2, 10 live)
                               Fields: AssignedCheckpoint (ECheckpointLocation),
                               Checkpoint (RoadCheckpoint ref)
     BodySearchBehaviour     - search the player (Priority 35, 10 live)
                               Fields: ShowPostSearchDialogue=true

   OTHER BEHAVIOURS FOUND IN SCENE:
     IdleBehaviour           - wait at a point (Priority 0, 29 live)
                               Fields: IdlePoint (nullable, null on inspected)
                               Name: "Wait ouside" (game typo)
                               29 instances vs 152 NPCs total. Ownership
                               proven: hired, employees, S1API minted NPCs
                               (see idle_behaviour_owners test).

   NOT FOUND (speculative names that do not exist):
     WanderBehaviour, GuardBehaviour, PatrolBehaviour,
     HoldPositionBehaviour, AmbushBehaviour

   Instance count summary (152 total NPCs in this save):
     152: StationaryBehaviour, FleeBehaviour, CoweringBehaviour,
          ScheduleBehaviour (every NPC has these)
      29: IdleBehaviour (subset of NPCs)
      10: FootPatrolBehaviour, VehiclePatrolBehaviour,
          SentryBehaviour, PursuitBehaviour,
          CheckpointBehaviour, BodySearchBehaviour
          (10 = the police officers)

   THE ADDCOMPONENT RECIPE IS PROVEN. S1API itself uses
   AddComponent to add CombatBehaviour (from a different
   namespace) to custom NPCs. The pattern:
     1. Create child GameObject under NPCBehaviour.transform
     2. AddComponent<TheBehaviour>() on the child
     3. Assign to the NPCBehaviour property (if one exists)
     4. Initialize required fields (events, refs)
     5. Call RepairBehaviourOwnership + RefreshBehaviourStack
   This same pattern should work for FootPatrolBehaviour and
   SentryBehaviour. The key question is whether NPCBehaviour
   has properties/slots for the police-specific behaviors
   (FootPatrolBehaviour, SentryBehaviour, etc.) or whether
   they only need to be children in the component hierarchy
   for the priority stack to pick them up.

   PRIORITY STACK (lower = lower priority, higher wins):
     -1  ScheduleBehaviour (baseline, always yields)
      0  IdleBehaviour
      1  SentryBehaviour
      2  CheckpointBehaviour
      3  FootPatrolBehaviour
     18  StationaryBehaviour
     25  CoweringBehaviour
     28  FleeBehaviour
     32  VehiclePatrolBehaviour
     35  BodySearchBehaviour
     40  PursuitBehaviour

   For garrison goons (ANALYSIS, not tested):
   SentryBehaviour (Priority 1) and FootPatrolBehaviour
   (Priority 3) sit low in the stack, so higher-priority
   behaviours (combat, flee, cowering) would preempt them
   based on the priority ordering. Whether these behaviours
   work on non-police NPCs is unproven. SentryBehaviour has
   the officer field problem. FootPatrolBehaviour has no
   officer field but native code is opaque.

   NOTE: all behaviours show 0 methods in IL2CPP metadata
   (the 0.4.6f12 stripping). All logic is in the native
   binary. We interact through fields only. This is fine:
   setting Active/Enabled and assigning refs (Group, Route,
   AssignedLocation) is all we need.

   DEEP FIELD DOCUMENTATION (live probe 2026-08-08,
   tests/research_behaviours.rs behaviour_deep_inspect).
   Fields listed are the ones unique to each behaviour (the
   shared Behaviour base fields like Active, Enabled, Priority,
   Name, Npc, beh, onBegin/onEnd/onEnable/onDisable, and all
   FishNet NetworkBehaviour boilerplate are omitted).

   FOOTPATROLBEHAVIOUR (Priority 3, 10 live, police only)
   Inferred purpose (from field names, not observed): walk
   through waypoints on a FootPatrolRoute.
   Unique fields:
     Group            - PatrolGroup ref (the runtime driver)
     UseFlashlight    - bool (true on police)
     flashlightEquipped - bool (runtime state)
     FLASHLIGHT_MAX_TIME - int (500)
   How it works (INFERRED from field refs + S1API source,
   not directly observed): Group points to a PatrolGroup,
   which holds a FootPatrolRoute and a member list. S1API
   source has AdvanceGroup (move to next waypoint) and
   IsGroupReadyToAdvance. Active/Enabled bools are on the
   component (proven). Actual runtime behavior not observed.
   Dependencies: needs a PatrolGroup assigned to Group, and
   that PatrolGroup needs a FootPatrolRoute with waypoints.
   The officer field is NOT on FootPatrolBehaviour (proven).
   HYPOTHESIS: FootPatrolBehaviour works on non-police NPCs
   if given a valid PatrolGroup. The native code could still
   check the NPC type internally. Not tested on minted NPCs.

   VEHICLEPATROLBEHAVIOUR (Priority 32, 10 live, police only)
   Inferred purpose (from field names): drive a patrol route
   in a police vehicle.
   Unique fields:
     Agent            - VehicleAgent ref (the driving AI)
     CurrentWaypoint  - int (current waypoint index)
     Route            - nullable (null when not driving)
     Vehicle          - nullable (null when not driving)
     aggressiveDrivingEnabled - bool (true)
     isDriving        - getter (runtime state)
   Dependencies: needs a vehicle and a VehicleAgent. Not
   useful for goons (they walk, not drive).

   SENTRYBEHAVIOUR (Priority 1, 10 live, police only)
   Inferred purpose (from field names): guard a fixed location
   (stand at a point, optionally walk a short sentry route).
   Unique fields:
     AssignedLocation - SentryLocation ref (nullable; null
                        when no location assigned)
     _standPoint      - Transform (the exact point to stand)
     _currentRoute    - SentryRoute ref (a short patrol loop
                        around the sentry post)
     _currentRoutePointIndex - int (position in sentry route)
     _minutesAtCurrentPoint  - int (game-minutes stood here)
     _movementModifiersApplied - bool
     UseFlashlight    - bool (true)
     FlashlightMaxTime - int (500)
     flashlightEquipped - bool
     officer          - PoliceOfficer ref (HARD DEP on police)
   How it works (INFERRED from field names, not observed):
   the sentry walks to _standPoint, stands there for
   _minutesAtCurrentPoint game-minutes, then moves to the
   next point on _currentRoute (if any). Lowest non-schedule
   priority (1), so everything preempts it.
   PROBLEM for goons: has an "officer" field typed as
   PoliceOfficer. All 10 live instances have this field
   non-null. HYPOTHESIS: the native code dereferences officer
   and would crash or no-op on null. This has NOT been tested.
   We do not know what the native code does with the field.
   ALTERNATIVE: SentryBehaviour's core idea (stand at a point)
   is simple. StationaryBehaviour (Priority 18) already does
   "stand still" but has no target position. For hold-post,
   we may be able to use IdleBehaviour (has IdlePoint) or
   simply enable StationaryBehaviour after warping the goon
   to the post location.

   PURSUITBEHAVIOUR (Priority 40, 10 live, police only)
   Inferred purpose (from field names): chase and engage a
   target. Has arrest-specific fields (baton, taser, gun,
   arrest circle).
   Unique fields:
     Target           - ICombatTargetable (the chase target)
     TargetPlayer     - Player ref (the player being chased)
     TargetVelocityTracker - SmoothedVelocityCalculator
     VirtualPunchWeapon - AvatarMeleeWeapon
     Weapon_Baton     - AvatarWeapon
     Weapon_Gun       - AvatarWeapon
     Weapon_Taser     - AvatarWeapon
     DefaultMovementSpeed - float (0.6)
     DefaultSearchTime    - float (9999.0, nearly infinite)
     GiveUpRange          - float (9999.0, nearly infinite)
     GiveUpAfterSuccessfulHits - int (0 = never)
     CombatOnStart    - bool (false)
     IsSearching      - getter
     IsTargetImmediatelyVisible - getter
     IsTargetRecentlyVisible    - getter
     TimeSinceTargetReacquired  - getter
     ArrestCircle_MaxOpacity    - float (0.35)
     ArrestCircle_MaxVisibleDistance - float (5.0)
     DEBUG            - bool (false)
     PlayAngryVO      - bool (true)
   ANALYSIS: has arrest-specific fields (weapons, arrest
   circle). CombatBehaviour already handles goon fighting.
   Not tested on non-police NPCs.

   CHECKPOINTBEHAVIOUR (Priority 2, 10 live, police only)
   Inferred purpose (from field names): man a road checkpoint
   and search vehicles.
   Unique fields:
     AssignedCheckpoint - ECheckpointLocation (enum, 0)
     Checkpoint       - RoadCheckpoint ref
     CurrentSearchedVehicle - LandVehicle (nullable)
     Initiator        - Player ref (who triggered it)
     IsSearching      - getter
     currentLookTime  - float
     dialogueDatabase - getter
     MaxStealthLevel  - int (0, on BodySearch not Checkpoint,
                        but appears in the component)
     ShowPostSearchDialogue - bool (true)
   ANALYSIS: requires RoadCheckpoint game objects and vehicle
   search fields. Not tested on non-police NPCs.

   BODYSEARCHBEHAVIOUR (Priority 35, 10 live, police only)
   Inferred purpose (from field names): search the player's
   body for contraband.
   Unique fields:
     TargetPlayer     - Player ref
     ArrestCircle_MaxOpacity - float (0.35)
     ArrestCircle_MaxVisibleDistance - float (5.0)
     MaxStealthLevel  - int (0)
     ShowPostSearchDialogue - bool (true)
   ANALYSIS: police arrest mechanic based on field structure.
   Not tested on non-police NPCs.

   IDLEBEHAVIOUR (Priority 0, 29 live)
   Inferred purpose (from field names): wait at a point.
   Unique fields:
     IdlePoint        - nullable (Transform, null on inspected)
     facingDir        - bool (false)
   PROVEN: carried by hired NPCs (14 inactive), employee NPCs
   (7 active: chemists, cleaners, handlers, botanists), AND
   our minted S1API NPCs (S1API_GoonNpc, S1API_PoliceNpc,
   S1API_PlayerNpc, all inactive). Our minted NPCs carry it.
   UNPROVEN: where it comes from (S1API, base prefab, or
   game init). UNPROVEN: whether setting IdlePoint and
   enabling it actually holds the NPC at that point.
   Source: tests/research_behaviours_deep.rs idle_behaviour_owners
   5 orphaned instances (24-28) throw NullReferenceException
   on get_Npc (no parent NPC set, leftover from AddComponent).

   STATIONARYBEHAVIOUR (Priority 18, 152 live, every NPC)
   Inferred purpose (from field names): stand still (no target
   point field exists on this type).
   Unique fields: none beyond the base Behaviour fields.
   Already on every NPC. High priority (18) means it preempts
   patrol, sentry, idle, and checkpoint. When Active, the NPC
   stops moving. Could work as a blunt hold-post: warp the
   goon to the post, enable StationaryBehaviour, disable it
   when they should patrol or fight. But it has no target
   position, so if the NPC drifts before activation, they
   freeze wherever they ended up.

   COMBATBEHAVIOUR (Priority 50, 162 live, every NPC + extras)
   Purpose: fight a target (proven by AttackEntity tests).
   Namespace: ScheduleOne.Combat (NOT ScheduleOne.NPCs.Behaviour)
   Unique fields:
     Target           - ICombatTargetable (the fight target)
     TargetVelocityTracker - SmoothedVelocityCalculator
     VirtualPunchWeapon - AvatarMeleeWeapon
     DefaultMovementSpeed - float (0.6)
     DefaultSearchTime    - float (30.0)
     GiveUpRange          - float (20.0)
     GiveUpAfterSuccessfulHits - int (0 = never)
     CombatOnStart    - bool (false)
     IsSearching      - getter
     IsTargetImmediatelyVisible  - getter
     IsTargetRecentlyVisible     - getter
     TimeSinceTargetReacquired   - getter
     PlayAngryVO      - bool (true)
     DEBUG            - bool (false)
     _defaultWeapon   - nullable
     currentWeapon    - nullable
     consecutiveMissedShots - int
     currentSearchDestination - Vector3
     hasSearchDestination     - bool
     lastKnownTargetPosition  - Vector3
     successfulHits           - int
     timeSinceLastReposition  - float
     timeSinceLastSighting    - float
     timeWithinAttackRange    - float
     visionEventReceived      - bool
     rangedWeaponRoutine      - nullable (coroutine)
     searchRoutine            - nullable (coroutine)
     onSuccessfulHit          - nullable (event)
   162 instances vs 152 NPCs: 10 extra instances exist.
   HYPOTHESIS: extras are PursuitBehaviour sharing the
   component (PursuitBehaviour extends CombatBehaviour in
   IL2CPP). Not verified. Already on every minted NPC via
   S1API. The key combat API: SetAndAttackTarget (proven in
   goon-vs-goon combat tests).

   SUMMARY FOR GARRISON GOONS:

   CANDIDATES for goons (no officer field, UNTESTED on minted NPCs):
     FootPatrolBehaviour - no officer field (proven). Whether it
                           works on non-police NPCs is unknown.
     IdleBehaviour       - exists on our minted NPCs (proven).
                           Whether setting IdlePoint holds them
                           is unknown.

   Has officer field (UNTESTED whether null crashes):
     SentryBehaviour     - officer field always PoliceOfficer
                           on live instances (proven). What
                           happens with null officer is unknown.

   ANALYSIS (not tested, reasoning from field structure):
     VehiclePatrolBehaviour - needs vehicle + VehicleAgent
     PursuitBehaviour - has arrest UI, weapons, police mechanics
     CheckpointBehaviour - needs RoadCheckpoint objects
     BodySearchBehaviour - police arrest mechanic

   Already on minted NPCs (proven by walk_class):
     StationaryBehaviour, CombatBehaviour, FleeBehaviour,
     CoweringBehaviour, ScheduleBehaviour, and 8 others.
     Source: S1API InitializeBehaviourComponents (cited,
     not traced per-call)

   NPCBEHAVIOUR PROPERTY SLOTS (proven, npcbehaviour_slots test):
   NPCBehaviour has named property slots for these behaviours:
     CallPoliceBehaviour, CombatBehaviour, ConsumeProductBehaviour,
     CoweringBehaviour, DeadBehaviour, FaceTargetBehaviour,
     FleeBehaviour, GenericDialogueBehaviour, HeavyFlinchBehaviour,
     RagdollBehaviour, RequestProductBehaviour, StationaryBehaviour,
     SummonBehaviour, UnconsciousBehaviour, ScheduleManager
   NPCBehaviour has NO property slots for:
     FootPatrolBehaviour, VehiclePatrolBehaviour, SentryBehaviour,
     PursuitBehaviour, CheckpointBehaviour, BodySearchBehaviour,
     VehiclePursuitBehaviour, IdleBehaviour
   The behaviourStack list is what matters, not named slots.
   HYPOTHESIS: behaviours join the stack by being child
   components of the NPCBehaviour GameObject. Inferred from
   S1API AddComponent pattern, not directly tested with a
   new component on a minted NPC. Named slots are convenience
   accessors for the most common behaviours.
   Source: tests/research_behaviours_deep.rs npcbehaviour_slots

   BEHAVIOUR STACK COMPARISON (proven live,
   tests/research_behaviours_deep.rs npcbehaviour_behaviour_list):

   Civilian NPC stack (18 entries, priority-ordered):
     [0]  DeadBehaviour             pri=1000
     [1]  UnconsciousBehaviour      pri=500
     [2]  RagdollBehaviour          pri=100
     [3]  HeavyFlinchBehaviour      pri=80
     [4]  CombatBehaviour           pri=50
     [5]  CallPoliceBehaviour       pri=30
     [6]  FleeBehaviour             pri=28
     [7]  CoweringBehaviour         pri=25
     [8]  FaceTargetBehaviour       pri=20
     [9]  StationaryBehaviour       pri=18
     [10] GenericDialogueBehaviour  pri=15
     [11] ConsumeProductBehaviour   pri=12
     [12] Behaviour (SummonBeh?)    pri=10
     [13] RequestProductBehaviour   pri=9
     [14] Behaviour                 pri=4
     [15] Behaviour                 pri=4
     [16] Behaviour (IdleBeh?)      pri=0
     [17] ScheduleBehaviour         pri=-1

   Police NPC stack (23 entries, 5 extra vs civilian):
     Same 18 as civilian, PLUS:
     VehiclePursuitBehaviour  pri=45 (NEW, not found in earlier scan)
     PursuitBehaviour         pri=40
     BodySearchBehaviour      pri=35
     VehiclePatrolBehaviour   pri=32
     FootPatrolBehaviour      pri=3
     CheckpointBehaviour      pri=2
     SentryBehaviour          pri=1

   On the inspected police NPC (OfficerGreen, sentry inactive):
     enabledBehaviours = 1 entry (the pri=0 Behaviour, IdleBeh)
     activeBehaviour = that same pri=0 Behaviour
   OBSERVATION on one officer (OfficerGreen): with sentry
   inactive, enabledBehaviours=1 and activeBehaviour=pri0.
   HYPOTHESIS: the stack always falls back to the lowest
   enabled behaviour. One data point, not proven as a rule.

   SENTRYBEHAVIOUR CONFIRMED POLICE-ONLY (proven,
   tests/research_behaviours_deep.rs sentry_behaviour_live_state):
   All 10 SentryBehaviour instances have a non-null "officer"
   field typed as Il2CppScheduleOne.Police.PoliceOfficer.
   4 of 10 are active+enabled (Murphy, Davis, Howard, Bailey),
   6 are inactive. Active ones all have:
     AssignedLocation = SentryLocation (non-null)
     _standPoint = Transform (non-null)
     _currentRoute = SentryLocation+SentryRoute (non-null)
   Inactive ones have all three as null.
   SentryLocation (ScheduleOne.Law, 16 instances):
     AssignedOfficers: List<PoliceOfficer>
     Routes: List<SentryRoute>
   SentryRoute (nested type: SentryLocation+SentryRoute):
     MinutesPerPoint = 15 (game-minutes at each point)
     RoutePoints = Transform[] (the walk points)
   OBSERVATION: all 10 SentryBehaviour instances have non-null
   PoliceOfficer in the officer field. HYPOTHESIS: tightly
   coupled to police, not usable for goons. Not tested with
   null officer on a minted NPC.

   FOOTPATROLBEHAVIOUR CHAIN (proven live,
   tests/research_behaviours_deep.rs foot_patrol_chain_live):
   7 of 10 officers have a PatrolGroup assigned to their
   FootPatrolBehaviour. 3 have Group=null (no patrol assigned).
   All 10 are currently inactive (on sentry or idle duty).
   PatrolGroups retain route assignment and waypoint progress
   even when the behaviour is inactive. Routes found on live
   groups: "Uptown Loop" (21wp), "Town Square Loop" (6wp),
   "North-west lop" (11wp), "Docks Loop" (12wp).
   Members=0 on all groups (officers not currently patrolling).
   FootPatrolBehaviour has NO officer field. It only needs
   Group (PatrolGroup) to function. PatrolGroup only needs
   Route (FootPatrolRoute) and Members (List<NPC>).

   HYPOTHETICAL APPROACHES (none tested on minted NPCs):
   Approach 1: IdleBehaviour. Exists on our minted goons
     (proven). Set IdlePoint, enable it. UNTESTED: whether
     this holds the NPC at the point.
   Approach 2: AddComponent<FootPatrolBehaviour>. Create a
     PatrolGroup + route. UNTESTED: whether AddComponent
     works for this type on a minted NPC.
   Approach 3: NPCMovement.SetDestination tick loop.
     UNTESTED: whether SetDestination works on minted NPCs
     (only Warp is proven).

   NEXT STEP: prove one of these on a live minted goon.
   The simplest test: spawn a goon, find its IdleBehaviour,
   set IdlePoint, enable it, observe whether the goon stays.
