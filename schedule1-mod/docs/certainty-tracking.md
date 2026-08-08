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
| GoonPool.SpawnGoon(Vector3) and SpawnMultipleGoons(Vector3,int,bool) are public; pool tracks spawned/unspawned goons, ReturnToPool recycles; 5 CartelGoons live | proven | list_methods + walk 2026-08-07 | not yet invoked; observe a spawn in-game before relying on it |
| ItemPickup is the ground-loot object: ItemToGive: ItemDefinition, Pickup(), DestroyOnPickup, onPickup; 70 live. DeadDrop: 25 live, static DeadDrops + GetRandomEmptyDrop | proven | list_methods + walk 2026-08-07 | pickup CREATION call unknown (what instantiates one) |
| CartelGoon method list overflows list_methods' 64KB buffer | proven | list_methods failed with buffer-too-small 2026-08-07 | raise the buffer in unityforge ops.rs list_methods |
| GetNPCsInRegion(EMapRegion) -> List<NPC> exists as a static | cited | NativeMethodInfoPtr_GetNPCsInRegion_Public_Static_List_1_NPC_EMapRegion_0 in same scan; owning class not yet identified | dnSpy to find the declaring class |
