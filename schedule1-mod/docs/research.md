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
2. NPCs: how NPCs spawn, path, and despawn; what the cartel/goon
   NPC classes are (the vanilla cartel update added hostile NPCs
   and ambushes).
3. Combat: health, damage application, death, and aggro classes
   for player and NPCs.
4. Where kills are observable: the Harmony hook point for combat
   XP.
