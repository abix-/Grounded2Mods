# schedule1-mod: certainty tracking

Every claim about vanilla game internals carries a status and
its evidence. IL2CPP reverse engineering produces
plausible-but-wrong claims constantly; a claim is asserted only
when evidence-cited, otherwise it stays a hypothesis. Discipline
inherited from the Schedule1Mods repo
(docs/certainty-tracking.md there).

Statuses: **proven** (in-game observation or passing live test),
**cited** (dnSpy / metadata read of the interop assemblies,
not yet observed live), **hypothesis** (needs evidence).

| Claim | Status | Evidence | Gap |
| --- | --- | --- | --- |
| Game 0.4.6f11 strips types still referenced (e.g. UnityEngine.Camera+GateFitMode) | proven | zero GateFitMode defs in Cpp2IL dump while refs remain; generator crashed on it 2026-08-07 | none |
| ATM.set_WEEKLY_DEPOSIT_LIMIT removed in 0.4.6 | proven | absent from Cpp2IL dump metadata scan 2026-08-07; Infinite_ATM throws MissingMethodException per frame | none |
| Game types live under Il2CppScheduleOne.* in interop assemblies | proven | mod error stacks name them; EmployeeReset builds against them | none |
| Harmony patches work through the shim on this game | proven | il2cpp-smoke postfix on UnityEngine.Time::get_realtimeSinceStartup fired per frame (smoke test PASSED 2026-08-07) | game-class patch not yet proven, only engine-class |
| ScheduleOne.Map.Map is a live singleton (1 instance) owning Regions: MapRegionData[]; also holds PoliceStation, MedicalCentre, UNLOCK_ALL_REGIONS | proven | live walk_class + inspect + read_field via tests/research_map.rs 2026-08-07 | none |
| EMapRegion: 0=Northtown 1=Westville 2=Downtown 3=Docks 4=Suburbia 5=Uptown; rank requirements 0/1/3/5/7/9; 6 regions total | proven | full Regions[] walk via handle chaining, tests/research_map.rs 2026-08-07 | none |
| MapRegionData carries Name, Region, IsUnlocked, UnlockedByDefault, RankRequirement{Rank,Tier}, StartingNPCs: NPC[], AdjacentRegions, RegionBounds: PolygonalZone, RegionDeliveryLocations | proven | element inspects in the same walk | none |
| Cartel influence live per region: Northtown 0.0, Westville 0.3, Downtown 0.1, Docks 0.65, Suburbia 1.0, Uptown 0.85 (operator's save) | proven | regionInfluence walk + GetInfluence(0..5) both agree, 2026-08-07 | save-specific values; ChangeInfluence not yet exercised |
| RegionDict is not an instance member of Map under that name | proven | live read_field returned not-found 2026-08-07 | dnSpy to find its real owner (likely static or other class) |
| ScheduleOne.Cartel.CartelInfluence is a live singleton (1 instance) with DefaultRegionInfluence: RegionInfluenceData[] | proven | live walk_class + read_field 2026-08-07 | read per-region values; GetInfluence(EMapRegion) invoke not yet run (crash cut the session) |
| Blanket-invoking proxy property getters off the main thread crashes the game (0xc0000005) | proven | inspect_object on CartelInfluence killed the process 2026-08-07; fixed in unityforge ef55aecd (on_main + field-backed-only inspect) | none |
| Vanilla cartel activity classes exist: ScheduleOne.Cartel.{Ambush, CartelActivities, CartelAmbushLocation, CartelDealManager, RobDealer, StealDeadDrop, SprayGraffiti} | cited | same metadata scan | live walk; find how activities are scheduled |
| NPCHealth.Die and NPCHealth.KnockOut are Harmony-patchable on the live game (the combat-XP hook points) | proven | harmony_probe patched + unpatched both clean, tests/research_npcs.rs 2026-08-07; OnDied does not resolve on NPCHealth | wire a real postfix and see it fire on an in-game kill |
| NPCHealth surface: Health SyncVar<float>, MaxHealth, IsDead, IsKnockedOut, TakeDamage, Die(), KnockOut(), Revive(), NotifyAttackedByPlayer(int), UnityEvents onDie/onKnockedOut/onDieOrKnockedOut/onRevive; holds its owning npc | proven | list_methods + live inspect of an NPCHealth instance 2026-08-07 | TakeDamage caller (damage source) unknown |
| 135 NPC instances live; NPCManager keeps static NPCRegistry: List<NPC> + static GetNPC | proven | walk_class census + list_methods 2026-08-07 | registry not yet walked |
| Cartel singleton owns Status/Activities/Influence/GoonPool/DealManager, ticks via HourPass, is saveable | proven | list_methods + walk (1 instance) via tests/research_cartel.rs 2026-08-07 | ECartelStatus values unknown |
| GoonPool.SpawnGoon(Vector3) works end to end: returns a CartelGoon, spawnedGoons count increments, goon is visible and interactable in-game | proven | tests/research_spawn.rs spawned 1 then 3 goons at the player's position; operator confirmed in-game 2026-08-07 | none |
| A spawned goon with no task walks to the nearest exit building and leaves (GoonPool.GetNearestExitBuilding is the cleanup path) | proven | operator observation 2026-08-07: goon interactable, then walked away into a house | how to task a goon (attack player / guard area) = behaviour research |
| Player position readable live: Player.transform -> get_position (Vector3 arrives as ToString, Newtonsoft cannot serialize UnityEngine.Vector3) | proven | tests/research_spawn.rs 2026-08-07; 2 Player instances walk (local + remote slot?) | why 2 Player instances |
| ItemPickup is the ground-loot object: ItemToGive: ItemDefinition, Pickup(), DestroyOnPickup, onPickup; 70 live. DeadDrop: 25 live, static DeadDrops + GetRandomEmptyDrop | proven | list_methods + walk 2026-08-07 | pickup CREATION call unknown (what instantiates one) |
| CartelGoon method list overflows list_methods' 64KB buffer | proven | list_methods failed with buffer-too-small 2026-08-07; FIXED same day (256KB, hot reload) | none |
| CartelGoon.AttackEntity(ICombatTargetable, bool) makes a spawned goon hunt and attack the player | proven | tests/research_attack.rs 2026-08-08; operator: "IT WORKS I got attacked" | second arg semantics unverified (passed true) |
| The {"$handle": N} live-object invoke arg works on IL2CPP (Player passed as ICombatTargetable) | proven | same run: AttackEntity accepted the player handle | Mono backend variant untested in a live game |
| Game 0.4.6f12 breaks interop generation at a fourth site (Pass16ScanMethodRefs xref decode) | proven | generator crash log 2026-08-07; patched (scan failure = no metadata init), regeneration completed and game modded OK 2026-08-08 | offer all four patches upstream |
| GetNPCsInRegion(EMapRegion) -> List<NPC> exists as a static | cited | NativeMethodInfoPtr_GetNPCsInRegion_Public_Static_List_1_NPC_EMapRegion_0 in same scan; owning class not yet identified | dnSpy to find the declaring class |
| NotifyAttackedByPlayer fires on EVERY player melee hit, same frame as TakeDamage (the kill-attribution signal) | proven | combat_trace prefix hooks; operator punched a Dealer + an NPC down, 5 NotifyAttackedByPlayer/TakeDamage pairs each, tests/research_killcredit.rs 2026-08-08 | ranged-weapon hits not yet traced; the int arg's meaning unknown |
| Melee to 0 health raises KnockOut, not Die (XP must credit both down paths) | proven | same trace: health 13.7 -> KnockOut at 0.0, no Die event, both NPCs | what raises Die (lethal weapons?) not yet observed |
| All four NPCHealth combat entry points (TakeDamage, NotifyAttackedByPlayer, Die, KnockOut) prefix-patch clean simultaneously on the live game | proven | combat_trace_start patched 4/4, dropped 4/4, game stayed healthy through two fights 2026-08-08 | none |
| An unknown class name in TypeCache.Resolve crashed the whole game (GetTypes over the f12 interop Il2Cppmscorlib throws TypeLoadException) | proven | crash log 2026-08-08 naming TypeCache.Resolve; fixed same day (loadable-subset + skip), proven by later unknown-name probes failing soft | none |
| Cash spawn recipe works end to end: Instantiate the inactive "Dynamic Amount Cash Pickup" template, SetActive, position, InstanceFinder.ServerManager.Spawn(GameObject, null, default Scene), write Value, UpdateCashStackVisuals | proven | tests/research_pickup.rs; operator picked up spawned $100 stacks in-game 2026-08-08 | item (non-cash) pickup creation not yet exercised; ItemPickup templates likely analogous |
| Un-spawned NetworkObject clones are destroyed by the engine within seconds | proven | 17 clones from pre-Spawn runs vanished from walk_class between runs 2026-08-08 | exact destroyer unidentified (FishNet client or scene GC) |
| CashPickup templates "$10 Pickup" + "Dynamic Amount Cash Pickup" sit inactive (activeInHierarchy=false) at one hidden point; they are the only CashPickup instances in a fresh save | proven | diag_cash_pickups walk + active flags 2026-08-08 | none |
| ServerManager.Spawn is Spawn(NetworkObject|GameObject, NetworkConnection, Scene), 3 params both overloads | proven | diag_spawn_signatures list_methods 2026-08-08 | none |
| Static field-backed property SETTERS on 0.4.6f12 game classes crash natively (get is fine): set_MaxHealth killed the game 3x, even writing the unchanged value; direct il2cpp_field_static_set_value also corrupted (delayed crash) | proven | static_prop_probe bisection 2026-08-08; getters PUNCH_RANGE/HealthRecoveryPerMinute/MaxHealth all fine | root cause presumed the patched generator's skipped metadata init (4th patch site); fix belongs in the generator clone |
| INSTANCE property writes are safe on this game: MinPunchDamage 20 -> 12 -> 20 round trip, game healthy | proven | instance_prop_probe 2026-08-08; CashPickup.Value write earlier same day | per-field lottery still conceivable; probe new fields before shipping skills on them |
| Vanilla punch damage 20 (min) / 35 (max); PUNCH_RANGE 1.25; vanilla HealthRecoveryPerMinute 0.5; MaxHealth 100 | proven | live reads 2026-08-08 | none |
| Heavy Hands effect math lands exactly: level N writes vanilla * (1 + 4.0 * sqrt(N/100)) on both punch props | proven | levelling test: L1 28/49, L2 31.3/54.8, L3 33.9/59.2 vs vanilla 20/35, 2026-08-08 | none |
| RPG state persists per save slot across game restarts (slot key = save folder name via LoadManager) | proven | xp/level/skill survived 4+ relaunches, slot SaveGame_2, 2026-08-08 | second save slot not yet exercised |
| Hot reload recaptures live values as "vanilla": a generation swap while an effect is applied poisons the baseline | proven | gen2 captured boosted 49 as MaxPunchDamage vanilla 2026-08-08; restored by hand | persist vanilla in the store or re-zero effects before swap (framework fix, backlog) |
