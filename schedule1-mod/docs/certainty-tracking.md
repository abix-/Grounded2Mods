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
