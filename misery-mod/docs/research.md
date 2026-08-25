# MISERY research

Everything known about modding MISERY, and the current plan.
Started 2026-08-13. Facts here are either read from disk, read
from a running game, or explicitly marked unverified. If a line
is not marked unverified, something checked it.

## 1. The goal

**More time per mission.** MISERY is an extraction game: a
mission gives you a limited window before something forces you
out. We want that window longer, or the clock slower.

**The clock is the emission ("shining") timer, found 2026-08-13.**
It is `TimeUntilEmmision` on `BP_GlobalManager_C`, configured by
`ShiningsTimer` in the difficulty settings. Full details in
section 8. The operator's recollection of a "shining" was
correct.

## 2. The game

| Property | Value |
|---|---|
| Install path | `C:\Games\Steam\steamapps\common\MISERY\` |
| Steam app id | 2119830 |
| Size on disk | 4.83 GB |
| Engine | **Unreal Engine 5.4** |
| Shipping exe | `MISERY\Binaries\Win64\MISERY-Win64-Shipping.exe` (134,655,488 bytes) |
| Launcher shim | `MISERY.exe` at the install root |
| Content format | IoStore (`MISERY-Windows.utoc` / `.ucas`, 4.4 GB) plus a 118 MB `.pak` |
| Game plugins | `SteamCorePro` only |
| Other SDKs | EOS SDK (`EOSSDK-Win64-Shipping.dll`) |

Engine version is from the branch string `++UE5+Release-5.4`
stored UTF-16 in the shipping exe, and confirmed independently
by UE4SS's own scan reporting `Found EngineVersion: 5.4`.

### 2.1 Content is encrypted

`retoc list --path MISERY-Windows.utoc` fails with
`missing encryption key`. Unlike Grounded 2, MISERY encrypts its
IoStore index, so offline asset browsing (retoc, FModel) is
blocked without recovering the AES key from the running process.

This does not block the mod. The plan is a runtime mod through
UE4SS, which reads live objects out of the process where
everything is already decrypted. It only means discovery happens
by walking live objects instead of grepping asset names.

## 3. UE4SS is installed and working

Installed by cloning the working Outworld Station setup (also
UE 5.4), not by downloading a fresh build:

```
MISERY\MISERY\Binaries\Win64\
  dwmapi.dll              proxy loader
  ue4ss\UE4SS.dll         v3.0.1 Beta, git SHA 06474186
  ue4ss\UE4SS-settings.ini
  ue4ss\Mods\             stock helper mods only
```

The stock mods that came with it: `Tweaks`, `Keybinds`,
`ConsoleEnablerMod`, `ConsoleCommandsMod`,
`CheatManagerEnablerMod`, `BPModLoaderMod`,
`BPML_GenericFunctions`, plus `SplitScreenMod` and `LineTraceMod`
disabled. `OutworldStationTweaks` was deleted from the folder and
from `mods.txt`; another game's mod must not load here.

First launch, from `ue4ss\UE4SS.log`:

| Symbol | Result |
|---|---|
| EngineVersion | 5.4 |
| GUObjectArray | `0x7ff72a748ed0` |
| GMalloc | `0x7ff72a630030` |
| FName::ToString | `0x7ff723dabc10` |
| FName::FName(wchar_t*) | `0x7ff723d8d650` |
| StaticConstructObject_Internal | `0x7ff723fa78d0` |
| GNatives | `0x7ff72a747970` |
| ConsoleManagerSingleton | `0x7ff723c38360` |
| FUObjectHashTables::Get | **not found** |

Scan took 357 ms. The one miss is `FUObjectHashTables::Get`,
which UE4SS says can be supplied as a custom AOB in
`UE4SS_Signatures/GUObjectHashTables.lua`. Whether anything we
need depends on it is unverified; nothing has failed yet because
of it.

The in-game console is enabled (tilde or F10) and the cheat
manager is enabled, both from the stock mods. The log shows the
cheat manager constructing on player restart, so the hooks fire
against a live game.

Addresses above are from one process launch and will differ next
run. They are recorded to show the scan works, not for reuse.

## 4. Where the mod will live

In this repo, as a `misery-mod` crate in the modforge workspace,
built on `ueforge` exactly like `outworld-station-mod` (the
closest prior art: same engine minor, same IoStore layout, same
shipping constraints).

The crate shape, copied from `outworld-station-mod/Cargo.toml`:

```toml
[lib]
name = "main"                       # UE4SS loads dlls/main.dll
crate-type = ["cdylib", "rlib"]

[package.metadata.ueforge]
mod_folder_name = "MiseryMod"
game_name_regex = "MISERY"
game_sub_path   = "MISERY/Binaries/Win64"
zip_prefix      = "misery-mod"
target_dir      = "target/misery-mod"
```

plus a one-line `build.rs` (`ueforge::build::CppShim::new().compile()`)
and `"misery-mod"` added to the workspace members list.

**Created and loaded 2026-08-13. See section 12.**

## 5. What we are doing, in order

Each step names what must be true before the next one starts.
Nothing gets built on an assumption that has not been watched.

**Step 1. Identify the clock. DONE 2026-08-13.**
`TimeUntilEmmision` on `BP_GlobalManager_C`, with
`ShiningsTimer` as the configured interval. Section 8.

**Step 2. Read it live. DONE 2026-08-13. See section 14.**
It counts down at exactly 1.000 per wall-clock second, so the
unit is real seconds.
Find the live `BP_GlobalManager_C` instance, read
`TimeUntilEmmision`, `EmissionsCount`, `FreezeTimer?`,
`FirstEmissionOffset`, `EmissionRandomDeviation`, and the
GameInstance's `DifficultyPreset` + `ShiningsTimer`. Sample the
countdown several times a few seconds apart.
Done when: the number moves, its direction and rate are known,
and its unit is worked out (seconds vs game-minutes) by
comparing the rate against wall-clock seconds.

**Step 3. Watch a whole cycle. DONE 2026-08-13. See section 15.**
`EmissionsCount` went 0 -> 1, the countdown reset to about 1150,
and `ShiningsTimer` is 22 **minutes**, so the reset is
`ShiningsTimer * 60` with the 12% jitter applied. Raising the
setting should lengthen every future window.

**Step 4. Write it. DONE 2026-08-13. See section 17.**
Writing `FreezeTimer?` stops the countdown dead, proven by an
A/B (moved 6 in 6s unfrozen, 0 in 6s frozen). The other two
routes, raising `ShiningsTimer` and topping up
`TimeUntilEmmision`, are untried and no longer needed unless
freezing turns out to break something.

**Step 5. Make it stick. NOT STARTED.**
The freeze is a bare memory write that dies with the process.
Turning it into a feature means the mod holding the flag from
init, or pointing ueforge's `freeze` op at the address.
Done when: the operator can launch the game and have the timer
behave without a test being run by hand.

(Step 5 used to say "create the crate last". That was wrong and
is corrected in 5.1: the crate comes first, because the
research is done in Rust.)

**Step 6. Ship it** as a runtime tweak with a settings file,
following the outworld-station-mod pattern, with the multiplier
exposed rather than hardcoded.

### 5.1 The crate comes first. Research is done in Rust

**The `misery-mod` crate is step 0, not step 5.** Research runs
through it, in Rust, so that research converts directly into the
shipped mod instead of being thrown away.

That is the model every other mod in this repo already follows:

- `grounded2-mod/tests` holds `explore_*` and `research_*` tests
  next to the `op_*` and regression tests, all driving the same
  `ueforge` control plane.
- `schedule1-mod/tests` and `wwm-mod/tests` do the same against
  the unityforge control plane.

So the order is: stand up the crate with `ueforge`'s control
plane, then write `research_emission.rs` against it, then let
the winning write from step 4 become a real feature in the same
crate. A throwaway UE4SS Lua script would answer step 2 slightly
sooner and leave nothing behind. Not the model.

Sampled values get written into this doc as they are read, and
the test that read them stays in the repo.

## 6. Open questions

Answered:

1. ~~What is "shining"?~~ The emission timer. Section 8.
2. ~~Where does the clock live?~~ A Blueprint actor,
   `BP_GlobalManager_C`, not the GameMode or GameState.

Still open, in rough priority order. The full list of missing
data is section 8.7.

1. Is the clock per world or per mission? The actor is a global
   manager that also owns world generation and the day/night
   cycle, which points at per world, but that is inference.
2. What unit is `TimeUntilEmmision` in?
3. Does raising `ShiningsTimer` alone change anything, or is the
   value only read when the countdown resets?
4. Is a client-side write safe in multiplayer? Both emission
   properties replicate and the game checks `HasAuthority`.
5. Does `FUObjectHashTables::Get` not resolving cost us
   anything?
6. Can the AES key be recovered if offline asset browsing turns
   out to be worth it?

## 7. Next action

Read `TimeUntilEmmision` off the live `BP_GlobalManager_C`
instance repeatedly and confirm it counts down in step with the
in-game clock. Name-matching found the property; only a moving
number proves it is the one the player feels.

Step 1 is done: the clock is identified (section 8). Steps 2
and 3 (read live, then write) are what remains before any crate
gets created.

## 8. The emission ("shining") system

Found in the object dump (Ctrl+J, 54 MB,
`ue4ss\UE4SS_ObjectDump.txt`) by searching for `shining` and
`emission`. The game spells it **"Emmision"** in one place and
"Emission" in others; both spellings matter when searching.

"Shining" and "emission" are the same mechanic: the gameplay
settings call the interval `ShiningsTimer`, and the runtime
countdown calls it `TimeUntilEmmision`.

### 8.1 The live clock: BP_GlobalManager_C

`/Game/Blueprints/BP_GlobalManager.BP_GlobalManager_C`, its
whole instance-variable block, with offsets from the dump:

| Offset | Type | Name |
|---|---|---|
| 0x2A8 | Int | `EmissionsCount` |
| **0x2B0** | **Double** | **`TimeUntilEmmision`** |
| **0x2B8** | **Bool** | **`FreezeTimer?`** |
| 0x2BC | Int | `CurrentWorldSeed` |
| 0x2C0 | Object | `DoorReference` |
| 0x2C8 | Byte | `CurrentGeneratedLevel` |
| 0x2D0 | Delegate | `StartgenerationEventDispatch` |
| 0x2E0 | Struct | `StartDoorLocation` |
| 0x2F8 | Bool | `CustomBiomSelected` |
| 0x2F9 | Bool | `LoadedSave` |
| 0x2FA | Bool | `FirstSave` |
| 0x300 | Double | `FirstEmissionOffset` |
| 0x308 | Double | `EmissionRandomDeviation` |

`TimeUntilEmmision` is the countdown itself. `FreezeTimer?`
(the question mark is part of the name) is a bool the game
already has for stopping it. `FirstEmissionOffset` delays the
first one; `EmissionRandomDeviation` jitters the interval.

Related functions on the same Blueprint: `OnRep_EmissionsCount`,
`SkipEmissions`, `TryRespawnPlayers`,
`TryRespawnPlayersIfAllDead`, `RespawnPlayerFromEmission`.

Offsets are Blueprint property offsets from this build's dump.
They are stable for a build but not across game patches, and
the mod should resolve properties by name rather than hardcode
them.

### 8.2 The configured interval: S_GameplaySettings

`/Game/SurvivalGameKitV2/Blueprints/Other/Structs/S_GameplaySettings`,
reached from Blueprints via `GetGameplaySettings`, keyed by a
`DifficultyPreset` byte. Field names carry the usual Blueprint
GUID suffixes, stripped here:

| Offset | Type | Name |
|---|---|---|
| **0x00** | **Double** | **`ShiningsTimer`** |
| 0x08 | Double | `DayLength` |
| 0x10 | Double | `NightLength` |
| 0x18 | Double | `WeatherCycleDuration` |
| 0x20 | Double | `InitialSeason` |
| 0x28 | Double | `HungerSpeed` |
| 0x30 | Double | `ThirstSpeed` |
| 0x38 | Double | `StaminaDrainRate` |
| 0x40 | Double | `HeadshotDamageMultiplier` |
| 0x48 | Double | `DamageMultiplier` |
| 0x50 | Double | `ItemsDurabilityDamageMultiplier` |
| 0x58 | Double | `AnomaliesDamageToPlayer` |
| 0x60 | Double | `AnomaliesSpawnRate` |
| 0x68 | Double | `EnemySpawnRate` |
| 0x70 | Double | `EnemyDamageToPlayer` |
| 0x78 | Double | `EnemySpeed` |
| 0x80 | Double | `RadiationPower` |
| 0x88 | Double | `InsanityPower` |
| 0x90 | Double | `AmmoScarcity` |
| 0x98 | Double | `FoodScarcity` |
| 0xA0 | Double | `HealsScarcity` |
| 0xA8 | Double | `RespawnHealthMultiplier` |
| 0xB0 | Double | `WeightLimitMultiplier` |
| 0xB8 | Bool | `FriendlyFire` |
| 0xB9 | Bool | `Shitting` |
| 0xBA | Bool | `Permadeath` |
| 0xBB | Bool | `RespawnOnEmission` |
| 0xBC | Bool | `CollisionBetweenPlayers` |

This is the game's whole difficulty knob set, and it is a far
richer target than the one timer we came for.

### 8.3 Three ways to get more time

Not yet tested, in order of bluntness:

1. Set `FreezeTimer?` true. Stops the countdown outright.
2. Raise `TimeUntilEmmision` whenever it drops. Extends the
   current window without touching config.
3. Raise `ShiningsTimer` in the gameplay settings so every
   interval is longer from the start. Cleanest, but it is a
   difficulty-preset value, so where it is read and whether a
   runtime write sticks are both unknown.

### 8.4 The replication caveat

`EmissionsCount` is replicated (`OnRep_EmissionsCount`), and
that function checks `HasAuthority` twice. The emission clock is
server-authoritative. In single player the client is the server
and a local write should hold. In multiplayer a client-side
write may be ignored, corrected, or desync. Untested either way.

### 8.5 Function surface on BP_GlobalManager_C

Every function the Blueprint declares:

| Function | Notes |
|---|---|
| `CountUntilEmmision` | the tick. One local, a `Subtract_DoubleDouble` result, so it is `TimeUntilEmmision -= delta` |
| `FreezeTime` | no params, no locals. Presumed to set `FreezeTimer?` true |
| `UnfreezeTime` | the counterpart |
| `SkipEmissions` | takes an `int EmissionsCount`, checks `HasAuthority`, adds to the count |
| `OnRep_TimeUntilEmmision` | replication callback, so the clock itself replicates |
| `OnRep_EmissionsCount` | replication callback, checks `HasAuthority` twice |
| `TryRespawnPlayers` | reads `GetGameplaySettings` and the `DifficultyPreset` |
| `TryRespawnPlayersIfAllDead` | same, wrapped in an all-dead check |
| `StartNight`, `ResetNightTime` | day/night cycle |
| `GenerateBiom`, `GenerateCustomBiom`, `SelectRandomBiom` | world generation |
| `UnloadPreviousLevels`, `UnloadStreamingLevels` | level streaming |
| `AutoSave` | |
| `ReceiveBeginPlay`, `ExecuteUbergraph_BP_GlobalManager` | standard |

`FreezeTime` / `UnfreezeTime` being real callable functions
matters: calling one is safer than writing the bool, because
whatever else the function does stays consistent.

### 8.6 Where the settings live at runtime

`GetGameplaySettings` is declared on
`BP_SGKGameInstance_C` (and on the matching interface
`BP_SGKGameInstanceInterface_C`). The GameInstance holds the
live copy:

| Offset | Type | Name |
|---|---|---|
| 0x210 | Byte | `DifficultyPreset` |
| **0x218** | **Struct** | **`GameplaySettings`** (S_GameplaySettings) |

So the live `ShiningsTimer` is at GameInstance + 0x218 + 0x00,
since `ShiningsTimer` is the struct's first field. The
GameInstance outlives level loads, which makes it the right
place to write a persistent change and the wrong place to
assume the game re-reads it.

There is a `GameSettingsUpdated` multicast delegate on the same
object. Whether anything re-reads the settings when it fires is
unverified, and it is the obvious lever if a plain write does
not take effect.

The difficulty enum is
`/Game/SurvivalGameKitV2/Blueprints/Other/Enum/E_Difficulty`.
Its values have not been read yet.

### 8.7 Data we still do not have

Named so nobody assumes these are known:

1. **Live values.** No number has been read out of a running
   game yet. Everything above is structure from the dump.
2. **What the HUD shows.** No widget in the dump references
   emission or shining by name, so how the player sees the
   countdown is unexplained. Possibly a PDA, the sky, or a
   differently-named widget.
3. **Whether the clock is saved.** `BP_SGKSaveGame_C` has no
   field matching emission, time, shining, day, night or seed.
   That suggests the countdown resets on load, but the save
   class may simply not expose it under those names.
4. **`E_Difficulty` values**, and which preset the operator's
   game is running.
5. **Whether `FreezeTime` does what its name says.** Not
   called, not traced.
6. **Multiplayer behaviour.** Both emission properties
   replicate. Single player should be fine; a client write in
   multiplayer is unproven.
7. **Units.** `TimeUntilEmmision` is a double, but whether it
   is seconds, game-minutes, or a normalised day fraction is
   unknown until it is watched moving.

### 8.8 The mechanic itself, from what is around it

Not confirmed by play, but the surrounding names tell a
consistent story: emissions happen on a repeating timer
(`ShiningsTimer` interval, `FirstEmissionOffset` delay for the
first, `EmissionRandomDeviation` jitter), the count of them so
far is tracked and replicated (`EmissionsCount`), players caught
out can be killed and respawned (`RespawnOnEmission`,
`RespawnPlayerFromEmission`, `TryRespawnPlayers`), and the world
generator sits on the same actor. It reads like a STALKER-style
world-wide blowout rather than a per-mission extraction
deadline. Marked unverified until watched.

## 9. Terminology: expedition

The in-game word for a run is **expedition**. The doc used
"mission" before this; expedition is the game's term and the
operator's, so it is the one to use.

`BP_ExpeditionDoor_C` is the door you go through to start one.
Its instance variables include a `GlobalManager` reference at
offset 0x448 pointing straight at `BP_GlobalManager_C`, the
actor that owns the emission clock and the world generator.
That link is the structural confirmation that expeditions and
the emission timer are the same system: the door hands you to
the manager that is counting down.

One `BP_ExpeditionDoor_C` instance was live in the dumped
session (`BP_ExpeditionDoor_C_2147479978`).

## 10. The siren

`/Game/Sounds/S_2minSiren` (with `S_2minSiren_Cue`) is a sound
asset in the shipped content. The name says the siren is a
**two minute warning**, which matches the operator's account of
sirens going off followed shortly by death.

What is NOT confirmed: who plays it, and what it is two minutes
before. The object dump lists objects and property layouts but
not property default values, so the reference from a Blueprint
to this cue cannot be traced statically from it. Confirming the
link needs either a live read of whatever holds the reference,
or a hook on the emission path.

Working hypothesis, unverified: `CountUntilEmmision` ticks
`TimeUntilEmmision` down, something fires the siren at the two
minute mark, and at zero the emission kills anyone not in
shelter. `RespawnOnEmission` and `RespawnPlayerFromEmission`
exist for exactly that death.

## 11. Platform offsets (resolved)

The mod's own `on_unreal_init` logged
`image_base = 0x7ff722cd0000`, which is what was missing. From
that base and the same run's UE4SS scan:

| Symbol | Absolute | Image-relative |
|---|---|---|
| GUObjectArray | `0x7ff72a748ed0` | `0x07A78ED0` |
| FName::ToString | `0x7ff723dabc10` | `0x010DBC10` |
| ProcessEvent | `0x7ff723f7b780` | `0x012AB780` |

Filled into `misery-mod/src/lib.rs::STEAM`. Two separate
launches reported identical absolute addresses, so this build
loads at a fixed base in practice, but the base is still read at
runtime and only the offsets are stored.

`g_names` and `g_world` stay zero: UE4SS logs no FNamePool or
GWorld address for this exe, and the ops in use do not need
them.

`g_objects_layout` is set to `WrappedChunked` (UE 5.4 stock,
same as Outworld Station). Unverified on this build; the first
successful `walk_class` proves or disproves it.

## 12. The crate exists

`misery-mod/` is created, in the workspace, built and deployed
to `MISERY\MISERY\Binaries\Win64\ue4ss\Mods\MiseryMod\dlls\`.
It loaded on first launch and its control plane answers on
**127.0.0.1:17176/debug**.

Contents: `Cargo.toml` with the ueforge deploy metadata,
a one-line `build.rs`, `src/lib.rs` (ModDef, init hooks,
platform offsets, three ueforge browser tabs) and
`src/debug.rs` (the control plane, no game-specific ops yet).

Deploy is `cargo run --release -p modforge --bin modforge-deploy
-- install -p misery-mod --game-path "C:\Games\Steam\steamapps\common\MISERY"`.
A second deploy while the game runs writes `main-new.dll` and
the running mod hot-reloads it.

## 13. The research harness (step 2, in progress)

`misery-mod/tests/research_emission.rs` with
`misery-mod/tests/common/mod.rs`, driving the control plane the
same way grounded2-mod's tests do. Enable with
`MISERY_DEBUG_PORT=17176`; without it the tests skip instead of
failing.

Probes: `control_plane_answers` (op catalog + whether the
offsets took), `resolve_offsets_against_config` (patternsleuth's
own resolvers vs the hand-computed offsets in `lib.rs::STEAM`),
`find_global_manager`, `read_emission_fields` (all five emission
fields at their dumped offsets), and `sample_countdown` (six
reads two seconds apart, which is what turns a named property
into a proven clock).

### 13.1 Blocked on the hot reload

The control plane answers and the op catalog is intact, but
`offsets_known = false`: the DLL currently running in the game
is still the stub-zeros build. The rebuilt DLL is staged next to
it as `main-new.dll`; the mod's watcher saw it and synthesized
Ctrl+R, but UE4SS only takes that keypress with the game window
focused.

`resolve_offsets` cannot break the tie either: it answers
`ueforge runtime not initialized`, because the runtime it would
scan for is the thing the zero offsets prevented from starting.

So every object read is blocked until the game reloads the mod.
Ctrl+R with MISERY focused, or a restart.

### 13.2 Ops the control plane already gives us

Worth knowing before writing any feature code, because two of
them may make the emission feature nearly free:

| Op | Why it matters here |
|---|---|
| `freeze` | holds a value at an address at N Hz, re-resolving when stale. Pointed at `TimeUntilEmmision` this IS "more expedition time", with no game code touched |
| `scan_memory` / `scan_rescan` / `scan_session` | Cheat-Engine-style value scanning, the fallback if the dumped offset turns out wrong |
| `read_bytes` / `write_bytes` | raw reads and writes against a selector plus offset |
| `walk_class`, `inspect_address` | find the live `BP_GlobalManager_C` and describe it |
| `discover_classes` / `discover_class_detail` / `discover_structs` | resolve property offsets by name at runtime instead of hardcoding the dump's numbers |
| `tweak_apply` / `tweak_revert` / `tweak_persisted_*` | declarative DataTable tweaks with vanilla capture and persistence, if any of this ends up table-driven |
| `resolve_offsets` | patternsleuth's resolvers vs the configured offsets, for detecting drift after a game patch |

`discover_class_detail` is the one that removes the hardcoded
0x2B0 from the eventual feature: ask the game for the property
offset by name at runtime, and a patch that moves it stops
mattering.

## 14. Step 2 done: the clock is real, and it is seconds

Read live from `misery-mod` through `research_emission.rs`,
2026-08-13, while the operator was in the open world
(`/Game/NewMapGENTEST.NewMapGENTEST.PersistentLevel`).

One live instance, `BP_GlobalManager_C_1`, at
`addr:0x186EC07E780`, no CDO confusion.

| Field | Offset | Value |
|---|---|---|
| `EmissionsCount` | 0x2A8 | 0 |
| `TimeUntilEmmision` | 0x2B0 | 727.0, then 681.0 a minute later |
| `FreezeTimer?` | 0x2B8 | false |
| `FirstEmissionOffset` | 0x300 | 300.0 |
| `EmissionRandomDeviation` | 0x308 | 0.12 |

### 14.1 The unit is real-time seconds

Six samples, two seconds apart:

```
t=  0.00s  681
t=  2.00s  679  (-1.000/s)
t=  4.00s  677  (-1.000/s)
t=  6.00s  675  (-1.000/s)
t=  8.00s  673  (-1.000/s)
t= 10.00s  671  (-1.000/s)
```

Exactly minus one per wall-clock second, five samples running.
`TimeUntilEmmision` is a countdown in real seconds. 681 seconds
is about 11 minutes 20 seconds to the next shining.

`FirstEmissionOffset` 300 reads as five minutes of grace before
the first one. `EmissionRandomDeviation` 0.12 reads as plus or
minus 12% jitter on the interval. `EmissionsCount` 0 means no
shining had happened yet in that session.

This also confirms the `S_2minSiren` story is at least
arithmetically possible: a two minute warning is the countdown
passing 120.

### 14.2 Control plane details learned the hard way

Both cost a round trip, both are now handled in
`tests/common/mod.rs`:

- `walk_class` DOES return instances, in
  `{class, instances: [...], returned, total}`. Parsing the
  result as a bare array reports "0 instances" for a class
  that is right there. Same envelope mistake as the Unity
  helper made earlier the same day.
- `read_bytes` names its arguments `instance_selector`,
  `offset`, `length`, and answers with the payload in
  **`bytes_hex`**. A decoder that silently returns None on an
  unexpected key turns a working read into no output at all;
  the helper now prints the shape it got.

Selector catalog: `addr:0x...`, `first_class:<Name>`,
`class:<Name>` (alias), `singleton:<Name>` (the CDO).

### 14.3 Offsets confirmed by patternsleuth

`resolve_offsets` against the running image:

| Symbol | Configured | Resolved | Match |
|---|---|---|---|
| `g_objects` | 0x7A78ED0 | 0x7A78ED0 | yes |
| `append_string` | 0x10DBC10 | 0x10DBC10 | yes |
| `g_names` | 0x0 | 0x79C2180 | no, was never filled in |

The hand-computed offsets were right. patternsleuth also
resolved `g_names`, which UE4SS never logged, so
`lib.rs::STEAM` can take `0x079C_2180` for it.

Also confirmed: `GObjectsLayout::WrappedChunked` is correct for
this build, since `walk_class` found a real instance through it.

## 15. Step 3 done: a full cycle, and the units

Watched live at base, 2026-08-13, by
`misery-mod/tests/research_cycle.rs`.

### 15.1 The clock runs at base

Sampled at base: 225, 223, 221, 219, 217, 215, at exactly
-1.000/s, same `BP_GlobalManager_C_1` instance as in the open
world, `FreezeTimer?` still false. **Base is not a time-out.**
The countdown is world-wide and always running, which fits the
same actor also owning world generation and the day/night cycle.

### 15.2 The siren is real and it is the two minute warning

The operator heard the siren while the countdown was in the 90s,
having crossed 120 about thirty seconds earlier. `S_2minSiren`
is confirmed as the warning, no longer an inference from an
asset name.

### 15.3 What happens at zero

`EmissionsCount` went **0 -> 1** and `TimeUntilEmmision` reset to
roughly **1150**, caught at 1135 twenty-seven seconds later and
falling at the usual 1 per second.

### 15.4 The configured interval is in MINUTES

Read from the live GameInstance
(`BP_SGKGameInstance_C`, `addr:0x186C5333100`):

| Field | Offset | Value |
|---|---|---|
| `DifficultyPreset` | 0x210 | 4 |
| `ShiningsTimer` | 0x218 | **22** |
| `DayLength` | 0x220 | 22 |
| `NightLength` | 0x228 | 8 |
| `WeatherCycleDuration` | 0x230 | 1 |

22 does not match a ~1150 second reset until you convert:
22 minutes is 1320 seconds, and `EmissionRandomDeviation` is
0.12, so the interval lands in 1162..1478 seconds. The observed
reset of about 1150-1162 sits at the bottom of that window.

So **`ShiningsTimer` is minutes, the countdown is seconds, and
the reset is `ShiningsTimer * 60` with plus or minus 12%
jitter.** `DayLength` 22 and `NightLength` 8 read as minutes too,
which corroborates the unit.

This answers the question step 3 existed to answer: the reset
reads the configured value, so **raising `ShiningsTimer` should
lengthen every future window**, and it takes effect at the next
reset rather than immediately.

### 15.5 One unexplained observation

At one point the countdown sat at exactly 1020.0 for 30 seconds
across six samples, then the run ended. `FreezeTimer?` stayed
false throughout, so the game's own freeze flag was not
involved.

Most likely the game was paused or unfocused (the operator was
alt-tabbed typing at the time), and Unreal stops ticking. Not
confirmed, and worth pinning down before shipping anything that
assumes the clock always runs: it means a paused game does not
burn expedition time.

## 16. Where the presets come from: DifficultyList

`discover_data_tables` found 19 loaded tables. The relevant one:

```
DataTable /Game/SurvivalGameKitV2/Blueprints/Other/DifficultyList.DifficultyList
row struct: S_GameplaySettings
4 rows: Explorer, Adventurer, Survivor, Hardcore
```

So the per-preset gameplay settings, `ShiningsTimer` included,
are authored as one `S_GameplaySettings` row per difficulty in
`DifficultyList`, and the GameInstance's live struct is filled
from the chosen row.

Full table list, for future work: `DifficultyList`,
`CraftingRecipesList`, `MasterCraftingRecipeList`, `ItemList`,
`MasterItemList`, `CookingList`, `MasterCookingList`,
`ItemSpawnerList`, `MasterSpawnerList`, `InventoryGridLayout`,
`MasterGridLayoutList`, `BuildPartList`, `MasterBuildPartList`,
`DeviceList`, `MasterDeviceList`, `LOOK_Presets`, `DT_Weather`,
`DT_Artifacts`, `DT_PlayerStatDescr`. 141,236 objects scanned.

### 16.1 Four rows, but DifficultyPreset reads 4

The table has exactly four rows and the live `DifficultyPreset`
byte is 4. If the enum is zero-based over those four rows, 4 is
past the end, which suggests a fifth value the table does not
carry, most likely a custom / modified preset. Unconfirmed: the
`E_Difficulty` enum entries have not been read.

This matters. If the operator's game is on a custom preset, the
live struct may have been written from the menu rather than
copied from a row, and editing the row would do nothing for the
current save.

### 16.2 The row decoder returns garbage for this struct

`dump_data_table` on `DifficultyList` returns four correctly
named rows but a broken schema: one field, offset 390, whose
name is a run of unrelated text
(`_BunkerChunck_C_2147480518.BP_WallGridComponent2...`) and
whose values decode to nonsense like `8.1215e-320` and
`2.127e+178`.

The row names are right and the row struct is correctly
identified as `S_GameplaySettings`, so the table itself is
found; it is the per-field walk that is wrong. The struct is a
Blueprint-authored `UScriptStruct` with GUID-suffixed field
names (section 8.2), which is the obvious suspect, and
`discover_struct_detail` cannot find `S_GameplaySettings` or
`E_Difficulty` by short name either.

Consequence: **the per-preset `ShiningsTimer` values are still
unknown.** Only the live GameInstance copy has been read
successfully (22 minutes on preset 4).

Two ways forward, neither tried:
1. Fix the row walk in ueforge for Blueprint structs, which
   would help every game in the workspace, not just this one.
2. Skip the table. The live struct reads correctly at a known
   offset, so the mod can write there and never touch the
   table. Whether that survives a preset re-apply is the open
   question from 16.1.

## 17. Step 4 done: the timer can be frozen

Writing 1 to `FreezeTimer?` (`BP_GlobalManager_C` + 0x2B8) stops
the countdown. Proven by an A/B on the live game, run one test
at a time so the two writes could not interleave:

```
unfrozen:  161 -> 155 in 6s   (moved 6)
frozen:    139 -> 139 in 6s   (moved 0)
```

The flag reads back as written in both directions, and the
countdown follows it. `misery-mod/tests/freeze_timer.rs`, with
`freeze` and `unfreeze` as separate #[ignore]d tests.

This also settles a doubt from 15.5: a stalled countdown is not
automatically a paused game. Here the stall is the flag, read
back as 1, with the game running.

`write_bytes` names its payload `bytes_hex`, matching what
`read_bytes` returns.

### 17.1 What this is not, yet

A memory write that dies with the process. It does not survive a
restart or a level reload, and nothing re-applies it. Turning it
into a feature means either the mod holding the flag on from
init, or pointing ueforge's `freeze` op at the address so it is
re-written continuously.

Still untested: whether a frozen timer breaks anything that
expects emissions to happen (`EmissionsCount` never increments,
so anything gated on emission count would stall), and what any
of this does in multiplayer, where both emission properties
replicate.

## 18. What a shining does

Split by how well it is known, because the difference matters.

### 18.1 Measured

- The countdown falls 1.000 per real second from ~22 minutes.
- At 120 the siren plays. The operator heard it live with the
  countdown in the 90s, so `S_2minSiren` is confirmed as the two
  minute warning.
- At zero, `EmissionsCount` increments (0 -> 1 observed) and
  `TimeUntilEmmision` resets to `ShiningsTimer * 60` with up to
  12% deviation.
- Writing `FreezeTimer?` stops all of the above (section 17).

### 18.2 Found in the object dump, not yet watched

Every emission-related member outside `BP_GlobalManager_C`:

| Owner | Member | Offset |
|---|---|---|
| `BP_WorldGeneration_Base_C` | `EmissionCountForRefresh` (Int) | 0x2B8 |
| `BP_WorldGeneration_Base_C` | `EmissionsPast` (Int) | 0x2F8 |
| `BP_PlayerInventory_C` | `RespawnOnEmission` (Bool) | 0x133A |
| `BP_PlayerInventory_C` | `RespawnPlayerFromEmission` (Function) | |
| `S_GameplaySettings` | `RespawnOnEmission` (Bool) | 0xBB |

Two consequences follow from the names and the layout, neither
observed yet:

1. **Shinings regenerate the world.** The world generator counts
   `EmissionsPast` against `EmissionCountForRefresh`, so the
   shining is the game's world-reset event, not only a hazard.
2. **Shinings can kill and respawn the player.** The respawn
   path lives on the player *inventory* component, which hints
   that gear is involved in what is kept or lost. The code
   behind it has not been read.

`NE_FireEmission_1` in `NS_BombExplosion1` / `NS_Grad_Explosion`
is unrelated: Niagara VFX naming, not this system.

### 18.3 Live counters

`walk_class(BP_WorldGeneration_Base_C)` returns **4 instances**,
subclassed (the first is `BP_FactoryGeneration_C_2`). Read from
that one:

```
EmissionCountForRefresh  +0x2B8 = 0
EmissionsPast            +0x2F8 = 0
```

Zero for the refresh threshold is unexplained: either refresh is
disabled for that generator, or the value is filled in later.
The other three instances have not been read.

### 18.4 Still unknown

- What a shining does to an exposed player: instant kill, damage
  over time, or nothing without line of sight to the sky.
- Whether shelter matters, and what counts as shelter. There are
  `BP_BunkerChunck_C` actors in the level.
- Whether being at base is safe. The one observed shining fired
  while the operator was at base and nothing notable happened.
- What the other three world generators hold.

The way to close all four: be somewhere exposed at zero and read
the counters either side of it.

### 18.5 Respawn on shining is ON for this save

Read live:

```
GameplaySettings.RespawnOnEmission            = 1
PlayerInventory (live).RespawnOnEmission      = 1
PlayerInventory (template).RespawnOnEmission  = 0
```

The flag is copied from the difficulty settings onto the
player's inventory component, and it is on. So a shining is
expected to kill and respawn the player under some condition
that is still unknown.

Note the template: `walk_class(BP_PlayerInventory_C)` returns
two instances, and the first is
`BP_PlayerInventory_GEN_VARIABLE`, a Blueprint archetype whose
values are authoring defaults, not live state. It read 0 while
the real component read 1. **Any probe that grabs the first
instance can silently read a template.** Filter on the full name
(the live one is under `PersistentLevel`).

### 18.6 Bunkers

The level is full of bunker actors: `BP_BunkerChunck_C` (642
mentions), `BP_BunkerFloor_C`, `BP_BunkersVolume_C`, and
`BP_DistantEffectCheckerBunker_C` (audio muffling).

`BP_BunkersVolume_C` is a post-process volume: it owns a `Box`
component, a `PostProcess` component, exposure and chromatic
timelines and a `Drunkefffect` (the game's spelling). Its
functions are `RemoveOldItems`, `StartNight`, `ResetNightTime`
and the timeline callbacks.

Notable: it has `StartNight` and `ResetNightTime`, the same pair
`BP_GlobalManager_C` has. Nothing on it mentions emission, so
whether being inside a bunker protects the player during a
shining is **not** established by the volume itself. It looks
like the interior look-and-sound volume, not a safety check.

Nothing anywhere in the object dump ties shelter to emission
survival. If bunkers are the survival mechanic, the check lives
in Blueprint bytecode, which the dump does not show.

### 18.7 What a shining looks like in game (operator, 2026-08-14)

Three shinings were forced with `set_countdown.rs` (counts 3->4,
4->5, 5->6) and the operator reported the same thing each time:

**The game plays a loading screen, the player keeps their
inventory and their bunker setup, and the world outside
changes.**

So a shining is the world-regeneration event, which confirms
what 18.2 inferred from `BP_WorldGeneration_Base_C` counting
`EmissionsPast` against `EmissionCountForRefresh`. It is not
primarily a hazard that kills you: it is the run ending and the
map being rebuilt, with your persistent stuff carried over.

That reframes the goal. "More time per expedition" means "longer
before the world is regenerated out from under you", and
freezing `FreezeTimer?` (section 17) gives exactly that: the
expedition never ends on its own.

Newly open, and worth checking before shipping the freeze:

1. Does anything the player wants *depend* on the regeneration?
   Loot respawn, new areas, quest or mission rollover. A frozen
   timer means it never happens, which could stall progression
   rather than help it.
2. Does the game expect `EmissionsCount` to keep climbing? The
   world generator's `EmissionCountForRefresh` read 0 on the one
   generator sampled (18.3), so what that threshold governs is
   still unknown.
3. `RespawnOnEmission` is on (18.5) but the operator was not
   killed by any of the three. So that flag probably governs
   where you are put after the rebuild, not a death.

## 19. How an expedition area becomes active

### 19.1 The four generators are four biomes

`walk_class(BP_WorldGeneration_Base_C)` returns four subclassed
instances, one per area:

| Actor | EmissionCountForRefresh | EmissionsPast |
|---|---|---|
| `BP_MeadowsWorldGeneration_C` | 5 | 7 (climbing with every shining) |
| `BP_FactoryGeneration_C` | 0 | 0 |
| `BP_BunkerWorldGeneration_C` | 0 | 0 |
| `BP_PaneliWorldGeneration_C` | 0 | 0 |

Only the active one counts. `EmissionsPast` **accumulates**: it
went 6 -> 7 across a forced shining and did not reset, so it is
a running total of shinings, not a countdown to the next
refresh.

### 19.2 What a generator is

`BP_WorldGeneration_Base_C` is a grid-based level streamer:

| Field | Offset |
|---|---|
| `GridFirstIndex_X` / `GridLastIndex_X` | 0x2A8 / 0x2AC |
| `GridFirstIndex_Y` / `GridLastIndex_Y` | 0x2B0 / 0x2B4 |
| `EmissionCountForRefresh` | 0x2B8 |
| `TileSize` | 0x2C0 |
| `Levels` (array) | 0x2C8 |
| `LevelsRefreshed` (array) | 0x2D8 |
| `StreamingLevels` (array) | 0x2E8 |
| `EmissionsPast` | 0x2F8 |
| `Random Stream` | 0x2FC |

Functions: `GenerateNewRandomLevels`, `RunGenerationFromSeed`,
`UnloadLevels`, `UnloadStreamingLevels`,
`CheckIfAllLevelsArevisible`, `BeginCheckIfAllLevelsAreLoad`.

So an area is a grid of streamed level tiles, generated from a
seed, refreshed against the shining count.

### 19.3 Selection lives on BP_GlobalManager_C

Read live during an expedition:

```
CurrentGeneratedLevel  +0x2C8 = 2      (Byte)
CustomBiomSelected     +0x2F8 = 0      (Bool)
LoadedSave             +0x2F9 = 1
CurrentWorldSeed       +0x2BC = 6022
```

Three functions drive it, and their locals say what they do:

- **`SelectRandomBiom`** rolls `RandomIntegerInRange` three
  times, does `GreaterEqual` comparisons and switches on an
  integer. This is the roll that picks an area, gated on
  `HasAuthority`.
- **`GenerateCustomBiom`** takes `CurrentGeneratedLevel` as its
  **parameter** (offset 0 in its frame). This is the "pick a
  specific area" entry point.
- **`GenerateBiom`** calls `GetActorOfClass` four times, once
  per generator actor, then switches on an enum. This is the
  dispatch: it finds the generator matching
  `CurrentGeneratedLevel` and runs it.

**So an area becomes active when `GenerateBiom` runs with
`CurrentGeneratedLevel` set to that area's number.** Normally
`SelectRandomBiom` sets that byte at random (`CustomBiomSelected`
is 0, so this save is on the random path).

### 19.4 What we do not know

- The mapping from number to area. Current value is 2 while
  Meadows is the live generator, so 2 is probably Meadows, but
  that is one data point and the enum has not been read.
- Whether `CustomBiomSelected` is reachable in normal play (a
  biome-choice UI) or is unused. Nothing in the object dump
  references it besides the manager itself.
- Whether writing `CurrentGeneratedLevel` before generation is
  enough, or whether `GenerateCustomBiom` must be called. The
  control plane has no function-call op, so only the write is
  testable today.

### 19.5 The experiment to run

Write `CurrentGeneratedLevel` to a different number, then start
an expedition through `BP_ExpeditionDoor_C` and see which
generator's counters come alive. Repeating that for 0..3 maps
every number to its area. It changes only which world gets
generated, and the operator keeps inventory and bunker across a
regeneration anyway (18.7).

## 20. The manager disappears after world regeneration

### 20.1 The problem

After a shining fires and the world regenerates,
`walk_class(BP_GlobalManager_C)` returns 0 instances. The timer
is still running (the game keeps going, the next shining will
fire on schedule), but the mod's search cannot find the object
anymore.

This breaks everything in the Shining tab: the status readout
says "No expedition running", the freeze checkbox does nothing,
and the timer controls all fail. All of them start by calling
`shining::manager()`, which uses `first_class:BP_GlobalManager_C`
to find the object.

The problem appeared after hot-reloading the mod (`main-new.dll`)
and then a world regeneration. Whether both are needed to trigger
it, or just the regeneration, is unknown.

### 20.2 The object still exists

Other actors in the same level are found by `walk_class`.
`BP_ExpeditionDoor_C` holds a `GlobalManager` reference at
offset 0x448 that points straight at the manager (section 9).
The game is clearly still using the object. The search just
cannot see it.

### 20.3 A CDO read crashed the game (2026-08-14)

While diagnosing this, a test read memory from the manager's
Class Default Object using a `singleton:BP_GlobalManager_C`
selector. CDOs are template copies the engine keeps as a
reference. Their memory layout is different from live actor
instances. Reading actor-layout offsets (0x2B0, 0x2B8, etc.)
from a CDO reads garbage memory and crashed the game mid-session.

**NEVER use `singleton:` selectors to read actor property
offsets.** They are only safe for reading class metadata, not
instance data. The probe has been removed from
`research_emission.rs`.

### 20.4 Fix path: follow the door pointer

`BP_ExpeditionDoor_C` has a `GlobalManager` object reference at
offset 0x448 (section 9). If `walk_class` finds the door, the
mod can read the pointer at +0x448 to get the manager's address
directly, then use `addr:0x...` to read it.

This avoids relying on `walk_class(BP_GlobalManager_C)` entirely
and should survive world regenerations, since the door persists.

Not yet implemented. Needs a live game to test.

## 21. UE4SS crash after game update (2026-08-14)

A Steam update to MISERY changed the exe. The game crashed on
launch with UE4SS enabled. The mod was not the cause.

### 21.1 What happened

1. Game without UE4SS (dwmapi.dll renamed): works.
2. Game with UE4SS, all mods disabled in mods.txt: works.
3. Game with UE4SS, stock mods enabled: crashes.

The crash was in one of the stock UE4SS mods, not in the UE4SS
core or in MiseryMod. The config was copied from Outworld
Station and included mods we do not need.

### 21.2 What was disabled

`BPModLoaderMod` and `BPML_GenericFunctions` were enabled but
not needed. BPModLoaderMod loads Blueprint .pak mods from a
folder. We only use a DLL mod. These are now disabled.

### 21.5 Stale OWS Tweaks DLL removed (2026-08-17)

The `Tweaks/dlls/main.dll` in the MISERY UE4SS mods folder was
the Outworld Station tweaks mod DLL, left over from when the
UE4SS setup was copied from Outworld Station (section 3). It
contained strings like "Outworld Station Tweaks", "OWS Tweaks",
and opened a console window titled "ows tweaks" on every launch.
Deleted. The stock UE4SS Tweaks mod is Lua-based and does not
need a DLL.

### 21.3 Current mods.txt

```
CheatManagerEnablerMod : 1
ConsoleCommandsMod : 1
ConsoleEnablerMod : 1
BPModLoaderMod : 0
BPML_GenericFunctions : 0
SplitScreenMod : 0
LineTraceMod : 0
Keybinds : 1
Tweaks : 1
MiseryMod : 1
```

## 22. Movement speed

### 22.1 Where it lives

Player movement speed is on `BP_CharacterComponent_C`, a
component attached to `BP_SGKMasterCharacter_C` (the player
character). Two fields matter:

| Offset | Type | Name | Live value | Template value |
|---|---|---|---|---|
| 0x200 | Double | `MovementSpeed` | 250.0 | 600.0 |
| 0x278 | Double | `MaxWalkSpeed` | 1000.0 | 1000.0 |

`MovementSpeed` is the actual speed the player moves at.
`MaxWalkSpeed` is the cap (sprint speed ceiling).

The engine's own `CharacterMovementComponent` (on the same
actor, named `CharMoveComp`) has:

| Offset | Type | Name | Value |
|---|---|---|---|
| 0x248 | Float | `MaxWalkSpeed` | 1000.0 |
| 0x24C | Float | `MaxWalkSpeedCrouched` | 1000.0 |

### 22.2 Why the live value is lower than the template

The template (`BP_CharacterComponent_GEN_VARIABLE`) has
`MovementSpeed` = 600.0, but the live player reads 250.0.
Something reduces it at runtime, probably carry weight or a
status effect. The `Sprinting` bool at 0x19D was false during
the read.

### 22.3 Other movement components in the world

`walk_class(CharacterMovementComponent)` returns 56 instances.
Most are enemies:

| Actor | MaxWalkSpeed |
|---|---|
| BP_ZombieSoilder_C | 80.0 |
| BP_Twins_C | 70.0 |
| BP_CrayFish_C | 450.0 |

### 22.4 Related: BP_PlayerInventory_C

`BP_PlayerInventory_C` has a `MovementSpeeds` map (keyed by a
byte, probably character state: walk/sprint/crouch) and an
`UpdateMaxMovementSpeed` function that takes a `CharacterState`
byte. The inventory likely manages speed per state and writes
the result to the character component.

### 22.5 Write tests: no visible effect

Writing `MovementSpeed` on the live `BP_CharacterComponent_C`
from 250 to 800 and then 1200 had no visible effect. The value
stuck in memory (sampled for 8 seconds, not overwritten), but
the player did not move faster.

Writing `MaxWalkSpeed` on the engine's
`CharacterMovementComponent` from 1000 to 2000 also had no
visible effect.

The game's `UpdateMaxMovementSpeed` on `BP_PlayerInventory_C`
reads the `MovementSpeeds` TMap, picks the entry for the current
`CharacterState`, and writes it to the character component. The
actual movement speed the engine uses is probably read from
somewhere else, or the character component value is copied into
the engine CMC on state changes and both our writes were
overwritten on the next state transition.

### 22.6 The MovementSpeeds TMap

`BP_PlayerInventory_C` has `MovementSpeeds` at offset 0xFE8.
It is a `TMap<Byte, Double>` keyed by `E_MovementSpeed` (a
Blueprint enum with at least 10 values: 0 through 9, with
generic `NewEnumerator` display names in the dump).

TMap header reads (two separate sessions, both 8 entries):

| Session | Elements pointer | Num | Max |
|---|---|---|---|
| First | `0x2101c06acc0` | 8 | 8 |
| Second (via inventory pointer) | `0x1fbe1f78c80` | 8 | 8 |

Successfully read on 2026-08-14 after fixing ueforge's
`check_object_bounds` to skip the `obj.class()` call for
`addr:` selectors (non-UObject heap pointers have garbage in the
class field, causing an access violation). Fix is in
`ueforge/src/ops.rs`.

| Key | Speed | Notes |
|-----|-------|-------|
| 2 | 250.0 | matches live MovementSpeed (walk) |
| 3 | 600.0 | likely sprint |
| 5 | 100.0 | likely crouch |
| 6 | 250.0 | unknown state |
| 7 | 600.0 | unknown state |
| 9 | 100.0 | unknown state |
| 10 | 350.0 | unknown state |
| 11 | 100.0 | unknown state |

Keys are `E_MovementSpeed` enum values. The enum has generic
`NewEnumerator` display names in the dump, so the mapping to
human-readable states is inferred from the values. Walk = 250
matches the live read. Sprint = 600 matches the template
`MovementSpeed` on `BP_CharacterComponent_C`.

The TMap element array is a separate heap allocation (not inline
in the object). Stride is 24 bytes per entry: key (u8) + 7
padding + value (f64) + HashNextId (i32) + HashIndex (i32).

### 22.7 Finding the inventory without walk_class

`walk_class(BP_PlayerInventory_C)` returns 0 instances, even
though the component exists on the player. The character
component has a `PlayerInventory` object reference at offset
0x218 (from the dump). Following that pointer from the live
character component reaches the inventory without relying on a
direct class walk.

Confirmed working 2026-08-14: read pointer at +0x218 from the
live character component, got `0x1fbc84aec50`, used
`addr:0x1fbc84aec50` to read the TMap header at +0xFE8
successfully.

As of 2026-08-15, `walk_class(BP_CharacterComponent_C)` ALSO
returns 0 live instances, so this pointer chain must start from
the player actor instead. See section 22.13.

### 22.8 discover_class_detail crashes the game

`discover_class_detail` on `BP_PlayerInventory_C` crashed the
game on 2026-08-14. This op walks the full class property chain
and can hit bad memory on large Blueprint classes. Do not call
it on this class. Use the object dump for schema information
instead.

### 22.9 Also on BP_MasterHoldable_C

`BP_MasterHoldable_C` has `UseHoldableMovementSpeeds` (Bool,
0x358) and its own `MovementSpeeds` map (0x360). When a holdable
item is equipped and `UseHoldableMovementSpeeds` is true, the
holdable's map overrides the inventory's map. No live holdable
instances were found in the test session (player was not holding
anything).

### 22.10 Write test: TMap write works

Writing walk=500, sprint=1200, crouch=200 into the TMap element
array via `addr:` selector confirmed working on 2026-08-14.
Player immediately felt faster. The game reads speed from
this map, not from the `MovementSpeed` property on the character
component (which was a dead end, section 22.5).

### 22.11 Speed tab added to the mod

Speed control is now built into the mod as a UI tab ("Speed"),
same pattern as the Shining tab. Files: `speed.rs` (backend),
`ui_speed.rs` (tab). The tab shows current walk/sprint/crouch
values, has sliders to set new ones, and an Apply button. A
"Reset to default" button restores the game's original values
(250/600/100).

Not yet tested in the mod UI (hot reload crashed the game on
deploy). Needs a cold start to verify.

### 22.12 Hot reload is unreliable

Hot reload (deploy `main-new.dll`, mod synthesizes Ctrl+R) has
crashed the game on every attempt during this session.
Do not use hot reload for MISERY. Deploy as `main.dll` and
restart the game.

### 22.13 walk_class fails for ALL Blueprint component classes

Tested 2026-08-15 with user in-game, walking around. Results:

| Class | walk_class result |
|-------|-------------------|
| `BP_CharacterComponent_C` | 0 live instances (UClass found, is_a matches nothing) |
| `BP_PlayerInventory_C` | 0 live instances |
| `CharacterMovementComponent` | 34 total, all CDOs (Default__), 0 live |
| `ActorComponent` | 0 live |
| `Actor` | 19205 live instances (works) |
| `BP_SGKMasterCharacter_C` | 1 live instance (works) |

Actor classes are found. Component classes are not. The UClass
pointer that `find_class_fast` caches appears to differ from the
class pointer on live component instances after Blueprint
reinstancing. `is_a` walks the super chain comparing UClass
pointers, so a stale cached UClass never matches.

The fix: find the player actor (which walk_class does find),
then follow pointer offsets to reach the character component
and inventory.

Pointer chain from the UE4SS object dump:

```
BP_SGKMasterCharacter_C (actor)
  +0x740  -> BP_CharacterComponent_C
               +0x218  -> BP_PlayerInventory_C
                            +0xFE8  -> MovementSpeeds TMap
```

Property dump evidence (line numbers from UE4SS_ObjectDump.txt):
- Line 138237: `ObjectProperty BP_SGKMasterCharacter_C:BP_CharacterComponent [o: 740]`
- Line 136682: `ObjectProperty BP_CharacterComponent_C:PlayerInventory [o: 218]`
- Line 152438: `MapProperty BP_PlayerInventory_C:MovementSpeeds [o: FE8]`

The current `speed.rs` uses walk_class on BP_CharacterComponent_C
and fails. It must be rewritten to find the actor first, then
follow 0x740 then 0x218 to reach the inventory.

### 21.4 Lesson

When copying UE4SS from another game, strip stock mods down to
what is actually used. BPModLoaderMod in particular can crash on
a game update because it hooks into asset loading paths that
change between builds. The core UE4SS loader and the DLL mod
system survived the same update without issue.

## 23. Items, containers, and storage

Research from the UE4SS object dump. No live reads yet.

### 23.1 Inventory class hierarchy

The game uses SurvivalGameKitV2 (SGK). Inventory is a component
tree, not a flat array.

| Class | Role |
|-------|------|
| `BP_MasterInventory_C` | Base class for all inventories |
| `BP_MasterItemInventory_C` | Base for item-holding inventories (extends MasterInventory) |
| `BP_PlayerInventory_C` | Player's main inventory (extends EquipmentInventory) |
| `BP_EquipmentInventory_C` | Equipment slots (extends MasterItemInventory) |
| `BP_WeaponInventory_C` | Weapon slots (extends MasterItemInventory) |
| `BP_GlobalInventoryManager_C` | Singleton manager for all inventories |

### 23.2 World containers

| Class | What it is |
|-------|-----------|
| `BP_MasterStorageBuildPart_C` | Base class for placeable storage furniture |
| `BP_WoodenBoxResource_C` | Wooden box (a storage build part) |
| `BP_GradBigCrate_C` | Large crate in bunkers |
| `BP_AirCrate_C` | Airdrop crate |
| `BP_DestroyedStorageBag_C` | Loot bag dropped when storage is destroyed |

### 23.3 Items

Items in the world are actors. Items in inventories are
presumably data in the inventory component's internal arrays
(not actors). The item class hierarchy:

| Class | Role |
|-------|------|
| `BP_MasterHoldable_C` | Base for anything the player can hold |
| `BP_MasterWeapon_C` | Weapons (extends MasterHoldable) |
| `BP_SkeletalMasterItem_C` | Items with skeletal meshes |
| `BP_MasterItemSpawner_C` | Spawns items in the world |

World items (pickups on the ground) have `_WI` suffix:
`BP_9mm_WI_C`, `BP_762mm_WI_C`.

### 23.4 UI widgets

| Class | Role |
|-------|------|
| `BP_InventoryHUD_C` | Main inventory screen |
| `BP_InventoryPanel_C` | A panel within the inventory |
| `BP_InventoryGrid_C` | Grid layout for items |
| `BP_MasterInventoryGrid_C` | Base grid class |
| `BP_InventoryCell_C` | Single cell in the grid |
| `BP_InventoryItemIcon_C` | Item icon in a cell |
| `BP_ContainerWindow_C` | UI for opening a container |
| `BP_ItemTooltip_C` | Tooltip when hovering an item |
| `BP_ItemOptionsMenu_C` | Right-click context menu |
| `BP_EquipmentInventoryList_C` | Equipment slot list |

### 23.5 Inventory interface

`BP_SGKInventoryInterface_C` is a Blueprint interface that
inventory components implement. This defines the contract for
adding/removing/querying items.

### 23.6 S_ContainerDetails struct

`S_ContainerDetails` is a struct used by both
`BP_MasterInventory_C` (at offset 0x178, `InventoryDetails`)
and `BP_PlayerInventory_C` (at offset 0x11D0,
`PlayerInventoryDetails`). It controls container capacity and
weight.

| Offset | Type | Name |
|--------|------|------|
| 0x00 | Bool | CanContainItems |
| 0x08 | Text | ContainerName |
| 0x18 | Int | ContainerInventoryCells (slot count) |
| 0x1C | Int | ContainerColumns (grid width) |
| 0x20 | Bool | UseWeight |
| 0x24 | Float | MaxWeight |
| 0x28 | Byte | ContainerRestrictionType |
| 0x30 | Array | RestrictionItems |
| 0x40 | Byte | ContainerType |
| 0x41 | Bool | AllowContainerWindow |
| 0x42 | Bool | UseItemCountLimit |
| 0x44 | Int | ItemCountLimit |
| 0x48 | Struct | CustomGridLayout |
| 0x58 | Array | StartingItems |

To increase box capacity: write `ContainerInventoryCells` and
`ContainerColumns`. To increase weight limit: write `MaxWeight`.
To remove weight limit: set `UseWeight` to false.

### 23.7 BP_MasterInventory_C properties

| Offset | Type | Name |
|--------|------|------|
| 0xA0 | Struct | UberGraphFrame |
| 0xA8 | Array | UsingPlayers |
| 0xB8 | Double | CurrentWeight |
| 0xC0 | Int | ItemCount |
| 0xC8 | Array | CraftingQueue |
| 0x150 | Array | Inventory (items in this container) |
| 0x160 | Object | ParentInventory |
| 0x178 | Struct | InventoryDetails (S_ContainerDetails) |
| 0x1E0 | Object | EquippedInventory |
| 0x280 | Double | DecayMultiplier |
| 0x2A0 | Double | TotalWeight |

### 23.8 S_GameplaySettings and weight

`S_GameplaySettings` (on the difficulty/game config) has
`WeightLimitMultiplier` at offset 0xB0 (Double). This is a
global multiplier applied to weight limits. Changing it would
affect all containers and the player inventory at once.

Other useful settings in the same struct:

| Offset | Type | Name |
|--------|------|------|
| 0x00 | Double | ShiningsTimer |
| 0x28 | Double | HungerSpeed |
| 0x30 | Double | ThirstSpeed |
| 0x38 | Double | StaminaDrainRate |
| 0x40 | Double | HeadshotDamageMultiplier |
| 0x48 | Double | DamageMultiplier |
| 0x68 | Double | EnemySpawnRate |
| 0x70 | Double | EnemyDamageToPlayer |
| 0x78 | Double | EnemySpeed |
| 0xA8 | Double | RespawnHealthMultiplier |
| 0xB0 | Double | WeightLimitMultiplier |

### 23.9 What we don't know yet

- The internal data format for items in the `Inventory` array
  (TArray of structs, struct type unknown).
- How items transfer between inventories (pick up, drop, move
  between containers).
- Whether `BP_GlobalInventoryManager_C` is the authority for
  persistence or just a runtime coordinator.
- How save/load works for inventory contents across sessions.
- `S_GameplaySettings` is at +0x218 on `BP_SGKGameInstance_C`
  (the GameInstance), NOT on `BP_GlobalManager_C`. Confirmed by
  live scan: ShiningsTimer=22, DayLength=22, NightLength=8,
  HungerSpeed=0.49, ThirstSpeed=0.51, HeadshotDamage=2.0,
  RespawnHealth=0.5, WeightLimit=3.0. All 24 Double fields and
  5 Bool fields are exposed in the Gameplay tab.

## 24. The vendor system

Found in the UE4SS object dump, 2026-08-17. Live reads confirmed
2026-08-17 via `research_vendors` test suite.

### 24.1 BP_VendorComponent_C

`/Game/SurvivalGameKitV2/Components/BP_VendorComponent`, the
component that drives all vendor behavior. Attached to vendor
actors like `BP_Technician_C` and the base class
`BP_MasterVendorBuildPart_C`.

| Offset | Type | Name |
|--------|------|------|
| 0x2C8 | Double | `RestockTime` |
| 0x2D0 | Bool | `UseStockLimits` |
| 0x2D1 | Bool | `Restock` |
| 0x2D8 | Array | `BuyList` (TArray of S_VendorBuy) |
| 0x2E8 | Array | `SellList` (TArray of S_VendorSell) |
| 0x2F8 | Double | `CurrentRestockTime` |
| 0x300 | Array | `VenderStock` (TArray of Int) |

Functions: `SaveVenderData`, `LoadComponentData`,
`RestockCheckTimer`, `StockLimitCheck`.

The game spells it "Vender" in some places and "Vendor" in
others, like "Emmision" / "Emission". Both spellings matter
when searching.

### 24.2 S_VendorSell (what the vendor buys from the player)

`/Game/SurvivalGameKitV2/Blueprints/Other/Structs/S_VendorSell`

| Offset | Type | Name (GUID-suffixed in dump) |
|--------|------|------|
| 0x00 | Struct | `Item` (item reference, same struct as S_VendorBuy) |
| 0x18 | Array | `Price` (array of item/quantity structs, the payment the vendor gives) |
| 0x28 | Array | `Category` (array of Byte, which vendor tab categories this appears in) |

### 24.3 S_VendorBuy (what the player buys from the vendor)

`/Game/SurvivalGameKitV2/Blueprints/Other/Structs/S_VendorBuy`

| Offset | Type | Name (GUID-suffixed in dump) |
|--------|------|------|
| 0x00 | Struct | `Item` (item reference) |
| 0x18 | Array | `Price` (array of item/quantity structs, the cost) |
| 0x28 | Int | `Stock` (how many the vendor has) |
| 0x30 | Array | `Category` (array of Byte) |

### 24.4 Vendor actors

Vendor actors live under
`/Game/SurvivalGameKitV2/Blueprints/BuildParts/Traders/`.
The base class is `BP_MasterVendorBuildPart_C` under
`BuildParts/`. The vendor component is at offset 0x3B8 on
each actor.

Seven vendor actors confirmed live in the safe hub:

| Actor | Address (session) |
|-------|-------------------|
| `BP_Barman_C_1` | 0x1B89B2DE470 |
| `BP_GunDealerReal_C_1` | 0x1B89B2D2F50 |
| `BP_Hunter_C_1` | 0x1B89B2D4300 |
| `BP_Medic_C_2` | 0x1B89B2D46F0 |
| `BP_ResourseSaler_C_3` | 0x1B89B2D2770 |
| `BP_Technician2_C_1` | 0x1B89B2D9D90 |
| `BP_Vasya_C_5` | 0x1B89B2D2B60 |

All seven inherit from `BP_MasterVendorBuildPart_C` and each
has a `BP_VendorComponent_C` instance.

### 24.5 Vendor UI widgets

| Class | Role |
|-------|------|
| `BP_VendorMenu_C` | The vendor screen. Has `BuyPanel`, `SellPanel`, `VenderInventory`, `PlayerInventory` refs |
| `BP_VendorListing_C` | One row in the buy/sell list. Has `SellListing` (Bool), `VenderSellListing` (S_VendorSell), `VenderBuyListing` (S_VendorBuy) |
| `BP_VendorListingTooltips_C` | Tooltip for a vendor listing |

`BP_VendorMenu_C` functions: `InitializeVenderMenu`,
`PopulateBuyList`, `PopulateSellList`,
`PopulateBuyCategorySelection`, `PopulateSellCategorySelection`,
`StringToCraftingCategory`. The populate functions take a
`ListingCategory` byte and loop over the buy/sell arrays,
filtering by category.

### 24.6 Entry layout (confirmed live, 2026-08-17)

The Item field (bytes 0x00 to 0x18) is an FDataTableRowHandle:
8-byte pointer to a shared allocation (not a UObject, not in
the GObjects array) plus an 8-byte FName (comparison_index i32
+ number u32). The FName resolves to item row names like
`Food_BreadGood`, `Weapon_AK47`, `Resource_Rubles`.

S_VendorSell stride is 0x38. S_VendorBuy stride is 0x40 (the
Stock int at 0x28 pushes Category to 0x30).

All prices use `Resource_Rubles` (FName `0x192e11`) as the
currency item. The price quantity is at offset 0x10 within the
price array element (e.g. 0x14 = 20 rubles). Sell list
entries have 1 price element. Buy list entries also have 1
price element despite the earlier raw read showing num=4; this
needs re-examination.

### 24.7 Complete vendor inventories (live, 2026-08-17)

All data from `research_vendors::dump_all_vendors` test.

**Barman** (BP_Barman_C_1)
- Buys from player (sell list, 8): Food_SeedsCarrot,
  Food_SeedsCucumber, Food_SeedsTomato, Food_SeedsWheat,
  Food_Cucumber, Food_Tomato, Food_Carrot, Food_BreadGood.
- Sells to player (buy list, 11): Food_CoackroachRaw,
  Food_BreadGood, Food_KabachkiGood, Food_SgushenkaGood,
  Food_TushenkaGood, Consumable_Cigaret, Drink_WaterBottle,
  Drink_Vodka, Drink_Beer, Drink_Energetic, Food_MRE.

**GunDealer** (BP_GunDealerReal_C_1)
- Buys from player (sell list, 11): Weapon_PM, Weapon_TT33,
  Weapon_Obrez, Weapon_Mosin, Weapon_PPSH, Weapon_TOZ,
  Weapon_Kiparis, Weapon_AK74, Weapon_SVD, Weapon_VAL,
  Weapon_Saiga12.
- Sells to player (buy list, 15): Holdable_Detector,
  Ammo_7.62, Ammo_9mm, Ammo_Buckshot, Ammo_5.45, Ammo_9.39,
  Holdable_RGD5, Magazine_PM, Magazine_TT33, Magazine_PPSH,
  Magazine_Kiparis, Magazine_AK74, Magazine_Saiga12,
  Magazine_Val, Magazine_SVD.

**Hunter** (BP_Hunter_C_1)
- Buys from player (sell list, 23): Resource_BoarHead,
  Resource_DeerHead, BuildPart_DeerSkin, Food_DeerMeatRaw,
  Resource_SwamperMoss, Food_SwamperMeatRaw,
  Resource_TwinsFeet, Resource_TwinsHead,
  Equipment_RatWolfHide, Resource_RatwolfHead,
  Resource_AssemblyFlesh, Resource_AssemblyMetal,
  Food_Caviar_Crab, Resource_CrabEye, Resource_CrabLeg,
  Resource_GraftBones, Resource_GraftFlesh,
  Resource_CrayFishTail, Resource_ChuvirlaFist,
  Resource_ChuvirlaHead, Resource_ScreamerHead,
  Resource_ScreamerLungs, Resource_GhoulParts.
- Sells to player (buy list, 5): Weapon_TOZ,
  Food_CoackroachCooked, Food_DeerMeatCooked,
  Food_SwamperMeatCooked, BuildPart_CarcassCatfish.

**Medic** (BP_Medic_C_2)
- Buys from player (sell list, 12): Artifact_Calculator,
  Artifact_Clocks, Artifact_Dosimeter, Artifact_GP5Filter,
  Artifact_Iron, Artifact_Nevolyashka, Artifact_Rubiks,
  Artifact_Shkatulka, Artifact_Soap, Artifact_Star,
  Artifact_ToyTruck, Artifact_Yola.
- Sells to player (buy list, 7): Consumable_Bandage,
  Consumable_Vitamins, Consumable_Medkit,
  Consumable_CarMedkit, Consumable_AntiRadPils,
  Consumable_AntiRad, Consumable_AntiDipressPils.

**ResourseSaler** (BP_ResourseSaler_C_3)
- Buys from player (sell list, 21): Resource_Glass,
  Resource_Electronics, Resource_Scrap, Resource_Wood,
  Resource_Gasoline, Resource_Coalbag, Resource_Drill,
  Resource_LeghtBulb, Resource_Gamebrik, Resource_Radio,
  Resource_Wires, Resource_ArmorPlate, Resource_ItemAntenna,
  Resource_ItemTrash_01 through Resource_ItemTrash_08.
- Sells to player (buy list, 14): Resource_Glass,
  Resource_Plastic, Resource_Electronics, Resource_Scrap,
  Resource_Wood, Resource_Gasoline, Resource_Coalbag,
  Resource_BegginerToolbox, Resource_RegularToolbox,
  Resource_AdvancedToolbox, Resource_GunPartsAssultRifle,
  Resource_GunPartsPistols, Resource_GunPartsRifle,
  Resource_GunPartsShotguns.

**Technician** (BP_Technician2_C_1)
- Buys from player (sell list, 11): Resource_Weapon_PM,
  Resource_Weapon_TT33, Resource_Weapon_Obrez,
  Resource_Weapon_Mosin, Resource_Weapon_PPSH,
  Resource_Weapon_TOZ, Resource_Weapon_Kiparis,
  Resource_Weapon_AK74, Resource_Weapon_SVD,
  Resource_Weapon_VAL, Resource_Weapon_Saiga12.
- Sells to player (buy list, 11, prices=4 each): Weapon_PM,
  Weapon_TT33, Weapon_Obrez, Weapon_Mosin, Weapon_PPSH,
  Weapon_Kiparis, Weapon_TOZ, Weapon_AK74, Weapon_VAL,
  Weapon_Saiga12, Weapon_SVD.

**Vasya** (BP_Vasya_C_5)
- Buys from player (sell list, 15): Equipment_SportUniform,
  Equipment_FurCoat, Equipment_BanditSuit,
  Equipment_FieldJacket, Equipment_TuristOutfit,
  Equipment_WorkerSuit, Equipment_HunterSuit,
  Equipment_HunterVest, Equipment_KitelJacket,
  Equipment_MiliaryVest, Equipment_PoliceBushlat,
  Equipment_PoliceVest, Equipment_TentCape,
  Equipment_WinterUniform, Equipment_SovietTacticalRig.
- Sells to player (buy list, 10): Equipment_Gasmask,
  Equipment_HamsterGasmask, Equipment_GasmaskPKM2,
  Equipment_Hazmatsuit, Resource_BrokenHazmatSuit,
  Equipment_SchoolBackpack, Equipment_SovietBackpack,
  Equipment_TacticalBackpack, Equipment_HunterBackpack,
  Equipment_HikingBackpack.

All buy list entries have stock=1. All sell list prices=1
except Technician buy list which has prices=4 (likely
Resource_Weapon parts as multi-item crafting cost).

### 24.8 How selling works (confirmed live, 2026-08-17)

The `SellList` on `BP_VendorComponent_C` is the whitelist of
items the vendor will accept from the player. Each entry names
the item and what the vendor pays for it. `PopulateSellList`
loops over this array, filtered by category, to build the sell
UI.

Selling is a whitelist: if an item is not in the `SellList`,
the vendor will not buy it.

### 24.9 Runtime sell list modification (confirmed, 2026-08-17)

Adding an entry to a vendor's `SellList` at runtime works
immediately. The vendor menu reads the live TArray contents
when it opens. No restart, no reload, no re-initialization
needed.

Proven by adding Resource_Plastic to ResourseSaler's sell
list via `research_vendors::add_plastic_to_resourcesaler_sell_list`.
The item appeared in the vendor's sell tab and the player
could sell plastic to the vendor on the same session.

**How to add an entry:**
1. Read the SellList TArray header from the vendor component
   (offset 0x2E8: pointer, num, max).
2. Check that num < max (there is slack in the allocation).
   All observed vendors have max > num (e.g. 21/26).
3. Clone an existing sell list entry as a template (0x38
   bytes). This preserves the shared item pointer at 0x00,
   the price array, and the category array.
4. Overwrite bytes 0x08..0x0C with the target item's FName
   comparison_index, and 0x0C..0x10 with 0 (FName number).
   Get the FName from any existing entry that references the
   same item (e.g. the buy list, or another vendor's list).
5. Write the new 0x38-byte entry at offset num * 0x38 from
   the array data pointer.
6. Increment num by 1 at the TArray header (offset 0x2E8 + 8
   on the component). Leave max unchanged.

The price and category arrays in the cloned template point to
existing allocations. The new entry reuses those pointers, so
the vendor pays the same price as the template item. To set a
custom price, the price array data would need its own
allocation (not yet implemented).

### 24.10 FDataTableRowHandle layout (confirmed, 2026-08-17)

The 0x18-byte Item field in each buy/sell entry is:

| Offset | Size | Content |
|--------|------|---------|
| 0x00 | 8 | DataTable pointer (shared across all entries) |
| 0x08 | 4 | FName comparison_index (item row name) |
| 0x0C | 4 | FName number (always 0) |
| 0x10 | 8 | Unknown field (always 0x1 in all observed entries) |

All 14 ResourseSaler buy entries share the same DataTable
pointer (`0x239cff8cf00` in this session). This pointer is
the same across all vendors, buy and sell.

### 24.11 Resource_SewingKit (confirmed working, 2026-08-17)

Resource_SewingKit exists in the game. Confirmed via:
- Texture files at `/Game/Textures/Icons/Resource/SewingKit/`
  (`T_SewingKit_InvIcon`, `T_SewingKit_QuickIcon`)
- Row 148 in `MasterItemList` DataTable (496 rows)
- Also present in `CraftingRecipesList` (102 rows) and
  `MasterCraftingRecipeList` (102 rows)
- FName: comparison_index=`0x192f7a`, number=0

Resource_SewingKit is NOT in `ItemList` (the DataTable all
existing vendor entries reference). It is only in
`MasterItemList`, which is a CompositeDataTable. Despite
this mismatch, adding it to ResourseSaler's buy list using
the same DataTable pointer as existing entries works. The
vendor UI displays the item correctly and the player can
buy it. The FDataTableRowHandle's DataTable pointer does
not need to match the table the row actually lives in.

### 24.12 TArray growth (confirmed working, 2026-08-17)

ResourseSaler's buy list had num=14, max=14 (no slack).
The sell list expansion technique (section 24.9) relies on
max > num. When there is no slack, the TArray must be grown.

Solved via `tarray_grow` op in the mod's control plane.
Uses `std::alloc::alloc_zeroed` (Rust standard allocator,
backed by the Windows process heap) to allocate a new
buffer, copies the old entries, and updates the TArray
header. The old buffer is leaked (tiny, harmless).

GMalloc was resolved via patternsleuth for future use, but
the vtable slot layout needs further research (slots 1
through 4 all resolve to the same address, making the
`Malloc` slot ambiguous). The Rust allocator works
reliably for TArray buffers since UE never tries to
`Free` or `Realloc` vendor list arrays during gameplay.

Proven by growing ResourseSaler's buy list from 14/14 to
14/30 via `research_vendors::add_sewingkit_to_resourcesaler_buy_list`.

**How to grow a full TArray:**
1. Read the TArray header (pointer, num, max) from the
   component at the list's offset.
2. Call `tarray_grow` with the component selector, offset,
   stride, and desired new_max. The op allocates a new
   buffer, copies old entries, zeros new slots, and
   updates the header's pointer and max.
3. After growth, num < max, so the standard entry-addition
   procedure (section 24.9 steps 3 through 6) works.

### 24.13 Runtime buy list modification (confirmed, 2026-08-17)

Adding an entry to a vendor's `BuyList` at runtime works
immediately, same as sell list modification (section 24.9).

Proven by adding Resource_SewingKit to ResourseSaler's buy
list via `research_vendors::add_sewingkit_to_resourcesaler_buy_list`.
The item appeared in the vendor's buy tab and the player
could purchase the sewing kit on the same session.

**How to add a buy list entry:**
1. Read the BuyList TArray header from the vendor component
   (offset 0x2D8: pointer, num, max).
2. If num >= max, grow the array first (section 24.12).
3. Get the target item's FName via `list_row_fnames` op
   on the appropriate DataTable (e.g. MasterItemList).
4. Clone an existing buy list entry as a template (0x40
   bytes). This preserves the DataTable pointer at 0x00,
   the price array, and the category array.
5. Overwrite bytes 0x08..0x0C with the target item's FName
   comparison_index, and 0x0C..0x10 with 0 (FName number).
6. Set stock at offset 0x28 (i32, e.g. 1).
7. Write the new 0x40-byte entry at offset num * 0x40 from
   the array data pointer.
8. Increment num by 1 at the TArray header (offset 0x2D8 + 8
   on the component). Leave max unchanged.

The price and category arrays in the cloned template point
to existing allocations. The new entry reuses those pointers,
so the item costs the same as the template item.

### 24.14 Bulk sell list expansion (confirmed, 2026-08-17)

Added all 31 edible food items (27 ready to eat + 4 seeds) to
the Barman's sell list so the player can sell any edible food.
The Barman originally accepted only 8 items. 23 new entries
were added in one batch.

Proven by `research_vendor_food::add_all_food_to_barman_sell_list`.
All 31 items appeared in the vendor's sell tab immediately.

**Procedure (batch sell list expansion):**
1. Read the SellList TArray header (offset 0x2E8).
2. Compare existing entries against the target item list.
3. Resolve FNames for all missing items via `list_row_fnames`
   on MasterItemList (ItemList may return 0 rows depending
   on game state; MasterItemList is the reliable source).
4. If num + missing_count > max, grow the TArray first
   (section 24.12). Barman grew from 8/8 to 8/41.
5. Clone sell entry 0 as a 0x38-byte template.
6. For each missing item: overwrite bytes 0x08..0x10 with
   the item's FName (comparison_index + number=0), write
   at offset num * 0x38, increment a running counter.
7. Write the final count to the TArray header (offset
   0x2E8 + 8 on the component).

The template's DataTable pointer (0x00), price array (0x18),
and category array (0x28) are reused. All new items cost
the same as entry 0 and share its category.

### 24.15 All 19 DataTables (confirmed, 2026-08-17)

Discovered via `discover_data_tables` with `refresh=true`,
180,870 GObjects scanned:

| Table | Rows |
|-------|------|
| DifficultyList | (not counted) |
| CraftingRecipesList | 102 |
| ItemList | 496 |
| MasterCraftingRecipeList | 102 |
| MasterCookingList | (not counted) |
| CookingList | (not counted) |
| MasterSpawnerList | (not counted) |
| ItemSpawnerList | (not counted) |
| InventoryGridLayout | (not counted) |
| BuildPartList | (not counted) |
| MasterBuildPartList | (not counted) |
| MasterItemList | 496 |
| MasterGridLayoutList | (not counted) |
| MasterDeviceList | (not counted) |
| DeviceList | (not counted) |
| LOOK_Presets | (not counted) |
| DT_Weather | (not counted) |
| DT_Artifacts | (not counted) |
| DT_PlayerStatDescr | (not counted) |

### 24.16 GMalloc vtable (researched, 2026-08-17)

GMalloc resolved via patternsleuth at image-relative offset.
The global pointer dereferences to an FMalloc object. The
vtable has 10+ slots. Slots 1 through 4 all point to the
same address (`0x7ff62d4b81a0`), which is unusual. Slot 0
is the destructor. The identical slots may be stub overrides
or merged function bodies in the concrete allocator class.
The Malloc slot has not been positively identified. Further
research needed if GMalloc allocation is ever required
instead of the Rust standard allocator.

### 24.17 What we do not know yet

- Whether expedition vendors (outside the safe hub) have
  different sell/buy lists.
- Whether the category byte maps to `E_CraftingCategory` or
  a vendor-specific enum. Barman sell list has cats=2, all
  others have cats=1.
- The Technician buy list has prices=4 per entry (all other
  vendors have prices=1). Need to decode those 4 price
  elements to understand if the cost is multi-resource
  (e.g. rubles + gun parts + scrap).
- ~~How to allocate a new price array for custom pricing on
  added entries~~ Solved 2026-08-25: `vendors.rs` allocates a
  private single-element price array per added entry (Rust
  allocator, leaked on purpose) and writes the quantity at
  +0x10. Live-verified via the dump test.
- Which GMalloc vtable slot is Malloc (slots 1 through 4
  are identical, slot 5 or later may be the real one).

## 25. NPC spawning

Class layouts from the UE4SS object dump; live findings from
the `research_spawners` test suite, 2026-08-25.

### 25.0 The live answer (confirmed 2026-08-25)

**The NPCs in the expedition areas are placed, not spawned.**
`walk_class_chain(BP_MasterAICharacter_C)` during an expedition
returned 14 NPCs, every one owned by a world preset tile:

```
WorldPresets/NormalVillage/3353_5_3.L_Swamp01: 4x BP_Swamper_C,
  1x BP_PlagueDoctor_C, 3x BP_NormalBandit_C
WorldPresets/.../3353_3_5.L_River_LoggingCamp: 3x BP_AIDwardWild_C
WorldPresets/.../3353_4_4.L_Meadows_CurveRoad_Drainage:
  1x BP_Swamper_C, 1x BP_DeerNeutral_C
WorldPresets/NormalVillage/3353_4_3.L_Meadows01: 1x BP_DeerNeutral_C
```

The generator (section 19) streams hand-built preset tiles onto
the grid; each preset ships with its NPCs already placed in the
level. Same tile, same crowd, every visit. Changing counts
therefore means creating new actors, not editing a spawner
number: there is no per-location spawn list to edit.

The only live spawner is ONE `BP_DwarfSpawn_C` point in the hub
map (`NewMapGENTEST.PersistentLevel.BP_AISpawnPoint_C_0`),
spawning 1x `BP_AIDwardTamed_C`. `BP_AISpawningVolume_C` (25.2)
has ZERO placed instances; it is an unused asset from the
SmartAI pack. `BP_BomberSpawn_C` is the bomber plane / airdrop
scheduler, not an NPC spawner.

Enemy characters extend `BP_MasterAICharacter_C` (classes under
`/Game/Blueprints/AI/`: BP_AIDwardWild_C, BP_Swamper_C,
BP_Ghoul_C, BP_Boar_C, BP_DeerNeutral_C, BP_NormalBandit_C,
BP_PlagueDoctor_C, ...). `walk_class` returns 0 for these
(section 22.13); the `walk_class_chain` op added 2026-08-25
matches by class-chain name and finds them.

### 25.1 BP_DwarfSpawn_C (the spawn point)

`/Game/SmartAI/Blueprints/AI/BP_DwarfSpawn`. Despite the name,
a generic NPC spawn point: one NPC class + count per point.

| Offset | Type | Name |
|--------|------|------|
| 0x2C8 | Bool | Enable Spawn AI |
| 0x2D0 | Class | Spawn AI (which NPC class) |
| 0x2D8 | Int | Spawn AI Count |
| 0x2DC | Bool | Change Default Behaviour |
| 0x2DD | Byte | Starting Behaviour |
| 0x2E0 | Double | Spawn Time |
| 0x2E8 | Double | Spawn Time Deviation |
| 0x2F0 | Bool | Respawn AI |
| 0x2F8 | Double | Respawn Time |
| 0x300 | Double | Respawn Time Variation |
| 0x308 | Array | Spawned AI |
| 0x319 | Bool | Use Player Proximity Activation |
| 0x320 | Double | Player Activation Range |
| 0x338 | Bool | Player In Area |
| 0x33C | Int | Current AI Spawned |
| 0x340 | Array | AI Respawning Timers |
| 0x358 | Object | AIBase |
| 0x370 | Array | Way Points |
| 0x3A0 | Name | StreamLevelPackageName |
| 0x3A8 | Struct | Location |

Live hub point values: enable=1, respawn=0, use_prox=0,
range=512, spawn_time=0.2, respawn_time=5.

**Writes land (confirmed 2026-08-25):** `set_spawn_point_more`
set count 1 -> 5 and respawn on; `set_spawn_point_entity` set
Spawn AI to BP_Swamper_C (class ptr read from a live swamper's
UObject +0x10). Both verified by read-back. Whether the game's
Blueprint logic re-reads these values after its initial spawn
is NOT yet confirmed.

### 25.2 BP_AISpawningVolume_C (unused in this game)

`/Game/SmartAI/Blueprints/AI/BP_AISpawningVolume`: an invisible
box carrying a list of what to spawn. Zero placed instances
live; documented for completeness only.

Its list entries are `S_AISpawner`
(`/Game/SmartAI/Blueprints/Structs/S_AISpawner`): AICharacter
class at 0x00, SpawnCount int at 0x08.

### 25.3 Doubling NPC counts (paths after the live findings)

Because the crowds are placed in the preset tiles, there is no
spawner number that doubles them. The options:

1. **Spawn extra actors next to the placed ones.** Needs the
   engine spawn function on the game thread: blocked on the
   pe_queue DrainSite + ProcessEventHook todo item (the same
   one blocking the nag screen).
2. **Borrow the game's own spawn point logic.** The hub
   BP_DwarfSpawn_C accepted count=5 and class=BP_Swamper_C via
   memory writes (25.1). If its Blueprint logic re-reads those
   values (respawn timer or activation), spawn points could be
   retargeted, or new points spawned near placed NPCs once
   path 1 exists anyway.
3. **EnemySpawnRate** (S_GameplaySettings +0x68, writable in
   the Gameplay tab): untested what it scales; with no live
   spawners in the field, it may only affect the difficulty
   preset's damage-style knobs or nothing observable.

### 25.4 What we do not know

- Whether BP_DwarfSpawn_C re-reads Spawn AI / count after its
  initial spawn (the 25.1 writes await in-game observation).
- What consumes EnemySpawnRate.
- Whether any expedition preset tiles contain spawn points
  (only the hub's one has been seen; the 14 field NPCs had
  none).
- How the tile pool per grid cell is chosen and how large it
  is (drives how much variety a world refresh can produce).
