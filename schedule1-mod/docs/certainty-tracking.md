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
| ScheduleOne.Map.Map is a live singleton (1 instance) owning Regions: MapRegionData[]; also holds PoliceStation, MedicalCentre, UNLOCK_ALL_REGIONS | proven | live walk_class + inspect + read_field via tests/research_map.rs 2026-08-07 | walk into MapRegionData elements (array access op missing) |
| RegionDict is not an instance member of Map under that name | proven | live read_field returned not-found 2026-08-07 | dnSpy to find its real owner (likely static or other class) |
| ScheduleOne.Cartel.CartelInfluence is a live singleton (1 instance) with DefaultRegionInfluence: RegionInfluenceData[] | proven | live walk_class + read_field 2026-08-07 | read per-region values; GetInfluence(EMapRegion) invoke not yet run (crash cut the session) |
| Blanket-invoking proxy property getters off the main thread crashes the game (0xc0000005) | proven | inspect_object on CartelInfluence killed the process 2026-08-07; fixed in unityforge ef55aecd (on_main + field-backed-only inspect) | none |
| Vanilla cartel activity classes exist: ScheduleOne.Cartel.{Ambush, CartelActivities, CartelAmbushLocation, CartelDealManager, RobDealer, StealDeadDrop, SprayGraffiti} | cited | same metadata scan | live walk; find how activities are scheduled |
| GetNPCsInRegion(EMapRegion) -> List<NPC> exists as a static | cited | NativeMethodInfoPtr_GetNPCsInRegion_Public_Static_List_1_NPC_EMapRegion_0 in same scan; owning class not yet identified | dnSpy to find the declaring class |
