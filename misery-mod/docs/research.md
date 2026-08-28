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

Moved to [`worldgen.md`](worldgen.md), the authoritative doc
for world generation: the four generators, grids, tile sizes,
level pools, area selection, and the remix work. Nothing
worldgen lives here any more.

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

The class model came from the UE4SS object dump. A live expedition
walk on 2026-08-27 found placed `BP_WoodenBoxResource_C` actors and
selected the nearest one by world-space distance from the player.

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
It allocates a new buffer, copies the old entries, and
updates the TArray header. The old buffer is leaked
(tiny, harmless).

**CORRECTED 2026-08-26.** This section used to say the
Rust allocator "works reliably for TArray buffers since UE
never tries to `Free` or `Realloc` vendor list arrays
during gameplay". That is FALSE and it cost two sessions.

The engine does tear those arrays down, on the way to the
main menu and on disconnect, which is long after the write
that caused it. A pointer `FMallocBinned2` never handed out
fails its own canary check and kills the process:

```text
FMallocBinned2 Attempt to realloc an unrecognized block
canary == 0x1e != 0xe3
```

The canary byte is whatever Rust's allocator left in front
of the block, so it differs run to run (`0x65` and `0x1e`
both seen).

Anything the engine may later grow or free MUST come from
`ue::gmalloc::alloc_zeroed`. The slot ambiguity noted here
is solved: `FMalloc::Malloc` is **slot 5**, measured from
the running image, section 27.

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

### 25.4 Area grids, tile sizes, level pools

Moved to [`worldgen.md`](worldgen.md) sections 3 and 4 (the
authoritative worldgen doc).

### 25.5 The scaling spawner (shipped 2026-08-25)

`src/spawning.rs`. A watcher thread censuses live hostiles
every 5s grouped by owning square (class prefix stripped from
the full name; leaving it in made every NPC class its own
square key and over-spawned, fixed same day). Each newly
streamed square rolls once:

- 20% quiet chance: nothing spawns.
- Otherwise extras = vanilla * random(0..2) * emissions/30,
  capped at 8 per square. Average extras equal the vanilla
  count (a doubling) at emission level 30.
- Each extra is a copy of a placed NPC, or with chance
  0.10 + 0.01/emission (cap 0.5) a cross-biome escalation
  drawn from the live hostile class pool (Tamed, DeerNeutral,
  Boar excluded).
- Pack chance 0.05 + 0.002/emission (cap 0.15): 3 to 5 of one
  pool class at a single anchor.
- Session cap 60. Squares re-roll when they stream back in.
  Emission level = max EmissionsPast across the generators;
  `spawning_override` op forces it for testing,
  `spawning_stats` reports counters.

Spawns execute on the game thread (section 26.3 recipe) at
300 to 800 units of scatter around a random anchor NPC.

Live proof at emissions=42: squares rolled 9/8/8/0 extras
including a pack of 3 BP_UN_ZombieSoilder_C; log lines in the
mod log, one roll per square.

### 25.6 What we do not know (spawning)

- Whether BP_DwarfSpawn_C re-reads Spawn AI / count after its
  initial spawn (the 25.1 writes await in-game observation).
- What consumes EnemySpawnRate.
- Whether any expedition preset tiles contain spawn points
  (only the hub's one has been seen; the 14 field NPCs had
  none).
- How the tile pool per grid cell is chosen and how large it
  is (drives how much variety a world refresh can produce).

## 26. Game-thread dispatch (solved 2026-08-25)

### 26.1 ProcessEvent vtable index is 0x4D in this game

The mod shipped with `PROCESS_EVENT_IDX = 0x4C` ("stable across
UE 5.x"). In MISERY the true index is **0x4D**, measured live:
UE4SS logs the resolved ProcessEvent address at startup
(`UE4SS.log: "ProcessEvent address 0x..."`); scanning the
GameInstance object's vtable for that address finds it at slot
0x4D (`research_dispatch::vtable_compare`). Actor vtables hold
AActor's ProcessEvent override at the same index, so the base
address is only findable in a plain UObject's vtable.

Consequences of the wrong 0x4C, now explained:
- ProcessEventHook installed cleanly but never fired (patched a
  virtual the engine never calls on that class).
- `call_ufunction` invoked the wrong virtual: "returns Ok but
  has no visible effect" (the old nag screen mystery, section
  on failed attempts in the todo history).

### 26.8 Memory the engine will free must come from the engine

**The crash.** Disconnecting from a session killed the game:

```text
LowLevelFatalError [MallocBinned2.cpp] [Line: 1322]
FMallocBinned2 Attempt to realloc an unrecognized block
000001F6FA450000  canary == 0x65 != 0xe3
```

**The cause.** `ueforge::ue::tarray::grow_raw` allocated the
bigger buffer with Rust's allocator and wrote that pointer into
the engine's `TArray` header:

```rust
let new_ptr = unsafe { std::alloc::alloc_zeroed(layout) };
*(header_ptr as *mut *mut u8) = new_ptr;
```

From then on the engine owned an array whose memory Rust
allocated. When the engine reallocs it, `FMallocBinned2` reads
the canary bytes that sit before one of its own blocks, finds
Rust's data there, and aborts.

The delay is what makes this nasty: the vendor pass grows those
arrays on every load, and the crash lands minutes later at
teardown, which looks like a disconnect bug rather than an
allocation bug. Timing that proved it, live:

```text
[21:16:19] vendor_mirror: added 15 items ...   7 vendors grown
[21:16:47] vendors: gone (main menu?)          crash
```

**The rule.** Anything the engine will later grow or free must be
allocated by the engine. `GMalloc` is a global pointer to an
`FMalloc` object; `Malloc` is a virtual method on it, so nothing
needs resolving beyond the global, which patternsleuth already
gives us and which `resolve_and_init` now stashes via
`ue::gmalloc::set_global`.

**Two mistakes made fixing it, both worth keeping.**

*Resolving lazily.* The first version resolved `GMalloc` on first
use, which fired a fresh patternsleuth scan from inside the
vendor pass, on the game thread. The game died at that exact
second. patternsleuth scans in a rayon pool, and 26.4 already
records what that costs at a bad moment. Resolve at init, inside
the scan that already runs.

*Inferring the vtable slot.* `FMalloc::Malloc` was taken to be
slot 2, reasoned from patternsleuth's own GMalloc patterns, which
match inside `FMemory::Malloc` and contain
`48 8B 01 FF 50 10`, a call through `[rax+0x10]`. That reasoning
looks sound and was wrong for this build: the call returned null
and the process died the same second.

An unverified vtable slot is a call to an arbitrary engine
function with arbitrary arguments. `MALLOC_SLOT` is therefore
`None` until it is MEASURED out of the running image, and
`alloc_zeroed` refuses rather than calling. `grow_raw` fails
loudly and never falls back to Rust's heap, because growing with
the wrong allocator is worse than not growing:

```text
[21:28:13] vendor_mirror: grow failed: TArray grow: engine allocator (GMalloc) unavailable
```

The cost of that safe state is vendors losing their added items.
Nothing else is affected.

**How to measure it.** Read the displacement rather than reason
about it: find `mov rax,[rcx]; call [rax+imm8]` inside
`FMemory::Malloc` in the live image. The `imm8` is the byte
offset; the slot is that over eight.

### 26.6 The main menu: UE4SS's on_update is NOT the game thread

Loading a save from the mod needs to call Blueprint functions
while the player is on the main menu, where no player character
exists and so the existing ProcessEvent hook never fires.

**What was tried.** `RC::CppUserModBase` declares an
`on_update()` that UE4SS calls every frame regardless of whether
a world is loaded (`ueforge/cpp/ueforge_cppusermodbase.hpp:81`).
Our shim overrode only `on_unreal_init`, so this callback sat
unused. It is now wired: `ueforge_mod_update` ->
`ueforge::frame::run_update`, with `frame::on_update(f)` for
subscribers.

It does fire at the menu. Measured live, sitting on the menu
with no save loaded:

```text
{"frames":3153,"fires":0,"drain_calls":3153,...}
pe_ping -> game_thread: true
```

`frames` climbing with `fires` at 0 means UE4SS is calling us
every frame while the ProcessEvent hook has never once fired.
Queued work now runs at the menu, where it used to fail with
`timed out after 3s waiting for game-thread drain`.

**But it is the wrong thread.** `frame::current_thread_id()`
records the OS thread on both sides, and once a save loaded and
the ProcessEvent hook fired even once, the two disagreed:

```text
frame_thread: 508      <- UE4SS on_update
hook_thread: 19556     <- ProcessEvent, which is game-thread only
```

ProcessEvent only ever runs on the game thread, so 19556 is the
game thread and UE4SS calls `on_update` from its own thread.

**What that cost.** Blueprint calls issued from `on_update`
LOOKED like they worked: `SGK SetLoadSaveGame(true)` read back
as `01`, `LoadLevel` returned ok, and the save loaded. Then the
session crashed. Calling ProcessEvent off the game thread is
undefined; a call that appears to succeed proves nothing.

**Where it stands.** `on_update` is good for polling, reading
counters, and anything that does not enter the engine. It must
NOT be used to call UFunctions.

**The answer: hook `UEngine::Tick`.** This is settled ground in
Unreal modding, and UE4SS does it in this very install. From
`ue4ss/UE4SS-settings.ini`:

```ini
HookEngineTick = 1
; Method for resolving GameEngine::Tick address
; Valid values: Scan, VTable
; Scan: Use PatternSleuth AOB scan (fallback to VTable if scan fails)
EngineTickResolveMethod = Scan
HookGameViewportClientTick = 1
```

UE4SS offers Lua mods `ExecuteInGameThread`, whose docs say it
runs "using either the ProcessEvent hook or the EngineTick
hook". Those are the only two mechanisms. Its C++ mod API
exposes neither: `ueforge_cppusermodbase.hpp` has only the
lifecycle virtuals, so a C++ mod resolves Tick itself.

Tick runs once per frame on the game thread for the life of the
process, with no world and no player required, which is exactly
what the menu needs.

**Three independent ways to find it, all agreeing live:**

UE4SS logs its own resolution at startup (`ue4ss/UE4SS.log`):

```text
[PS] Found GameEngineTick: 0x7ff6426c4030
GameEngine::Tick address (vtable: 0x7ff6426c4030; scan: 0x7ff6426c4030)
GameViewportClient::Tick address 0x7ff642731300
```

The patternsleuth scan and the vtable lookup produced the same
address. Image base is `0x7ff63f0b0000`, so Tick is at
image-relative `0x3614030`.

And read directly off the live engine object
(`research_gamethread::engine_vtable`):

```text
engine: GameEngine /Engine/Transient.GameEngine_2147482621
vtable: 0x7FF64578CD18
  [ 95] 0x7FF6426C4030   <== GameEngine::Tick
```

**`GameEngine::Tick` is vtable slot 95** (UE 5.4). There is
exactly one live `GameEngine`, findable by class-chain search
for `GameEngine` under `/Engine/Transient`.

This is the same mechanism the mod already uses:
`ProcessEventHook` writes one vtable slot at a configured index
(`process_event_idx`, `0x4D` here). Tick is the same write at
index 95 on the engine object. Only the signature differs:
`void Tick(UGameEngine*, float DeltaSeconds, bool bIdleMode)`.

Note UE4SS has already placed a prehook on the Tick FUNCTION
BODY. Patching the vtable SLOT means the original we capture is
that function address, which routes through UE4SS's detour, so
the two chain rather than recurse. This is unlike the widget
vtable, where a second slot install would capture our own
trampoline.

**Shipped and confirmed live 2026-08-26.**
`ueforge::hook::engine_tick` patches the slot;
`src/dispatch.rs` installs it from `on_update` (a safe use of
UE4SS's thread: it only polls) and drains the queue from the
tick handler.

On the main menu, no save loaded:

```text
tick_installed: true   tick_fires: 1756   tick_panics: 0
drain_calls: 1756      <- every drain came from the engine tick
fires: 0               <- ProcessEvent never fired
frame_thread: 20928    <- UE4SS on_update
tick_thread: 21488     <- UEngine::Tick
pe_ping -> game_thread: true
```

`drain_calls` equalling `tick_fires` is the check that
`on_update` no longer touches the queue.

After loading a save through it, the thread question is settled
rather than inferred:

```text
tick_thread:  21488
hook_thread:  21488   <- ProcessEvent, game-thread only
frame_thread: 20928   <- UE4SS on_update
fires: 110  tick_panics: 0  panics: 0
```

`research_dispatch::engine_tick_serves_the_main_menu` asserts
the match, and `on_update_serves_the_main_menu` asserts the
NON-match, so the wrong assumption cannot come back quietly.

Related hazard: only ONE ProcessEvent hook may sit on the shared
widget vtable. `install_for_object` records whatever is in the
slot as "the original", so a second install records our own
trampoline and recurses forever. Also, installing from the first
class-chain match in GObjects order produced a hook that fired 3
times in a session, while installing from the notice fires
constantly; install from a live instance you can name.

### 26.7 Loading a save from the mod: the recipe

Found by listing LIVE class functions (`class_functions`, added
to ueforge; menu widgets are created after startup so the
discovery cache does not have them).

`BP_SGKGameInstance_C` holds the intent, and
`BP_HostNewGameServer_C` starts the level:

| Function | Class | Parms |
|---|---|---|
| `SGK SetSaveGameSlotName` | `BP_SGKGameInstance_C` | FString, 16 bytes |
| `SGK SetLoadSaveGame` | `BP_SGKGameInstance_C` | bool, 1 byte |
| `SGK GetSaveGameSlotName` / `SGK GetLoadSaveGame` | same | the readers |
| `LoadLevel` | `BP_HostNewGameServer_C` | none |
| `FindExistingSave` | `BP_HostNewGameServer_C` | FString in, bool out |
| `DeleteExistingSave` | `BP_HostNewGameServer_C` | FString. Never call. |

The slot name is the plain text shown in the menu: reading it
live returned `"Save 1"`. It is an `FString`, decoded as
`{ TCHAR* Data; int32 Num; int32 Max; }` with UTF-16 characters
behind the pointer. Because the game instance already holds the
name, loading needs no FString construction: set the bool, then
call `LoadLevel`.

`LoadLevel` is ALSO the New Game path. The bool is the only
thing that makes it load rather than start fresh, so it must be
set and read back before `LoadLevel` is called.

**The template trap.** Class-chain searches return the widget
template inside the `/Game/...WidgetTree` package as well as the
live widget under `/Engine/Transient`. Calling the template
returns ok and does nothing. Filter on `/Engine/Transient`.
Both `BP_HostNewGameServer` and `BP_HostLoadGameServer` are
instances of the same class; only the name tells them apart.

**Auto-load, live 2026-08-26.** `src/autoload.rs` does the whole
thing on launch, so a restart lands in the saved game with no
keys and no clicks:

```text
[20:17:57] feature autoload: applying
[20:17:58] nag: pressed InpActEvt_SpaceBar_K2Node_InputKeyEvent_1
[20:18:00] autoload: loading the saved game
```

It runs exactly once per launch (one log line; a `SETTLED` flag
stops the poller). Ticks before the load menu widget exists
return `waiting` and are silent. Quitting to the menu later does
NOT re-trigger it.

Every step is guarded, because `LoadLevel` is also the New Game
path and a wrong turn would start a fresh game over the player's
save. It aborts before `LoadLevel` if no slot name is held, if
`FindExistingSave` says the save is missing, or if the load flag
does not read back as set.

`FindExistingSave` takes 2 parms in 17 bytes: an FString in and
the answer in BYTE 16, measured rather than assumed
(`research_load::find_existing_save_layout`):

```text
existing slot "Save 1": 500f05c420020000070000000800000001
empty slot:             0000000000000000000000000000000000
```

**Works, live 2026-08-26.** Operator confirmed the save loaded
and played, with no clicks and no crash, once the calls ran on
the game thread via the `UEngine::Tick` hook (26.6). The same
calls made from UE4SS's `on_update` had loaded the save too, and
then crashed the session; a call that appears to work off the
game thread proves nothing.

`research_load::load_current_slot` drives it. It is `#[ignore]`d
because it starts a level load, so run it deliberately.

**One line of that recipe was wrong, fixed 2026-08-26.**
`LoadLevel` is right, but it was called on the wrong object:
`BP_HostLoadGameServer` hosts a server and generates a world, so
every launch came up somewhere the player had never been. The
object to call is `BP_SingleplayerNewGameMenu`. See 26.9.

### 26.9 Three load screens and three host objects (2026-08-26)

`research_singleplayer_load.rs`, read-only, run at the main menu.
Every line below is from its output.

The menu the player clicks is Singleplayer, then Load Game, then
a save. Underneath, each of those two screens exists three times
over, and the class name does not tell them apart. Only the
object name does.

Three load screens, all of class `BP_LoadGameMenu_C`:

```text
BP_SinglePlayerLoadSaveMenu    the singleplayer one
BP_HostLoadSaveMenu            host a server
BP_LoadGameMenu                in-game
```

Three objects of class `BP_HostNewGameServer_C`, each with its
own 0-parm `LoadLevel`:

```text
BP_SingleplayerNewGameMenu
BP_HostNewGameServer
BP_HostLoadGameServer          <- what autoload calls
```

So 26.7 called `LoadLevel` on the host-a-server object. That
starts a server and generates a world, which is exactly the
symptom.

There is no fourth host object for singleplayer-load, so
singleplayer load is `BP_SingleplayerNewGameMenu` with
`SGK SetLoadSaveGame(true)`: the flag is what turns the new-game
widget into a load.

**Confirmed live 2026-08-26** by `tests/load_singleplayer.rs`,
from the main menu:

```text
slot name: "Save 1"
load flag reads back: 01
LoadLevel called on BP_SingleplayerNewGameMenu
world up after 11.2600474s
```

The proof it is the player's save and not a new world is the
emission count in the mod log, `emissions=42`. A new world starts
at 1. `src/autoload.rs` now calls this object.

**A world number in a square's name proves nothing.** Six runs
gave 5760, 244, 10776, 15820, 9387, 10748, and the last of those
was a confirmed load of the player's save. The number belongs to
the preset square that streamed in, not to the world.

**Reading the object list during a load crashes the game.** The
first run of this test polled `walk_class_chain` every three
seconds while the level loaded, from the control plane's own
thread, and took the process down:

```text
exception   0xc0000005  ACCESS_VIOLATION
address     0x7ffcba8ab2ce  main.dll + 0x5b2ce
fault       reading 0x22548e5edd0
```

The faulting address is in the same range as the menu widgets
listed a minute earlier, and those are destroyed when the level
loads. `ueforge::ops::on_game_thread` now routes `walk_class`,
`walk_class_chain`, `class_functions`, `inspect_address` and
`class_outer_samples` through the game thread. The same poll then
ran through a full load without faulting. Answers carry
`game_thread: false` when a mod serves no queue and the walk had
to run on the calling thread.

`main.dll` in a MISERY dump is THIS MOD: `scripts/restart.ps1`
builds `misery_mod.dll` and deploys it under the name UE4SS
requires. Symbols only resolve if the deployed DLL and the local
`.pdb` are the same build.

**What a save row can be told to do.** Each load screen owns a
`BP_SaveGamePanel` of class `BP_LoadGameMenuPanel_C`:

```text
DeleteExistingSave                                         FString
InitializeSavePanel                                        none
NoSelected / YesSelected                                   none
BndEvt__Button_284_..._OnButtonClickedEvent__Delegate...   none
BndEvt__Button_372_..._OnButtonClickedEvent__Delegate...   none
```

The two buttons are named only by number. One loads and one
deletes, and nothing in the function list says which. Do not call
either until that is settled some other way.

**The slot name is not what the menu shows.** Read live at the
main menu:

```text
SGK GetLoadSaveGame     -> 0
SGK GetSaveGameSlotName -> "Save 1_Auto"
```

`"Save 1_Auto"` is the autosave, not the `"Save 1"` recorded in
26.7. Auto-load reads that name back and loads it, so it was
never loading the save the player picks.

**`inspect_address` is no use here.** It answered `found: false`
for every live widget address, with `address not within any
UObject`. Comparing the three host objects field by field needs
`read_bytes`.

### 26.2 The game-thread queue

`src/dispatch.rs`: a `ueforge::pe_queue::GameThread` drained from
a ProcessEventHook on `BP_SGKMasterCharacter_C`, installed with
retry-backoff from the LIVE player instance's vtable
(`ProcessEventHook::install_for_object`, added to ueforge; the
find_class_fast CDO route can resolve a stale class, 22.13).
The player class receives ProcessEvent every frame in play:
205k fires in minutes, zero panics.

Proof (`research_dispatch::game_thread_ping`, live 2026-08-25):
a job enqueued from the HTTP worker thread executed on the game
thread; drained_cmds 0 -> 1.

Ops: `pe_ping` (run a no-op job on the game thread), `pe_stats`
(fires, drain counters, panic count).

Limitation: this hook only fires while a save is loaded
(the player class has no instances at the main menu), so
main-menu-time dispatch needs a ProcessEvent hook on something
alive at the menu instead. See 26.6.

### 26.5 The playtest notice, and two ways to crash the game

The notice (`WD_PlaytestNote01_C`) needed a spacebar press. The
old workaround synthesised one; this replaces it with real
suppression. Three lessons came out of it, two of them from
crashes.

**Finding it.** The class is ABSENT from the object dump (it was
not loaded when that was written) but is live at startup, so it
must be read from the running game. `find_object` with
`require_level = false` finds it: widgets are not actors.

**Reaching the game thread at the main menu.** UMG widgets may
only be touched from the game thread, and this mod's usual drain
site is the player character, which does not exist at the menu.
The way through is to hook the widget's OWN class: when the
engine calls anything on it, we are on the game thread holding
the widget.

**CRASH 1: Blueprint widget classes share a vtable.** They add
no C++ virtuals, so they all use the base `UUserWidget` vtable.
Hooking "the notice's class" therefore hooks EVERY widget, and
the handler ran for all of them. It collapsed the main menu.
Any handler installed this way MUST check what it has been
handed:

```rust
let is_nag = this.class().map(|c| c.as_object().name() == NAG_CLASS)
```

**CRASH 2: RemoveFromParent destroys a hooked object.** Hiding
the widget is not dismissing it. Live reading at the black
screen showed only three widgets instantiated: the cursor, the
notice, and a `BP_RadiationLoadCircle` INSIDE the notice. So
the notice is the loading screen, and collapsing it leaves it
present and still taking input, which is what the black screen
is.

Calling `RemoveFromParent` on it killed the game:

```
LowLevelFatalError: Pure virtual function being called
(stack full of repeated frames)
```

Destroying an object whose vtable we have patched means the
next virtual call lands in a half-destroyed object. Never
destroy a hooked object from inside its own hook.

**SOLVED: press the notice's own spacebar handler.** Collapsing
and disabling was not dismissal either. The widget stayed
instantiated and kept swallowing input, and the black screen
stayed. The answer was to stop touching the widget's
presentation at all and run the same code a real keypress runs.

Reading the live class (`nag_stats`, which walks
`UClass::iter_functions` on the live object) gives four
functions and names the answer outright:

```text
Get_KeyIcon_1_Brush
InpActEvt_SpaceBar_K2Node_InputKeyEvent_1
InpActEvt_Gamepad_FaceButton_Bottom_K2Node_InputKeyEvent_0
ExecuteUbergraph_WD_PlaytestNote01
```

`InpActEvt_SpaceBar_K2Node_InputKeyEvent_1` IS the spacebar
handler. The gamepad entry beside it is the same event bound to
the A button, which confirms the pair is the dismissal. Calling
it from inside the existing hook (already on the game thread,
already holding the widget) makes the game tear its own notice
down, so nothing is hidden and nothing is destroyed.

Read the parm size off the `UFunction`, do not assume it. This
one declares 1 parm and 24 bytes, an `FKey`. Undersizing the
block lets the callee write past the buffer. `UFunction` gained
`parms_size()` and `num_parms()` for this
(`ueforge/src/ue/offsets.rs::ufunction`).

Live proof, two consecutive cold starts, operator confirmed the
notice never appeared and no key was pressed:

```text
[19:07:04] nag: hooked WD_PlaytestNote01_C
[19:07:04] nag: pressed InpActEvt_SpaceBar_K2Node_InputKeyEvent_1 (24 parm bytes, 1 parms)
[19:07:06] FieldTweak<i32> applied to 175 rows of ItemList
```

That last line matters: on the collapse-and-disable build the
same `ItemList` lookup timed out after 30s and the log went
silent, because the game never got past the notice.

**The general lesson.** A Blueprint widget that waits for a key
names its handler after that key. When something must be
dismissed, list the live class's functions and look for
`InpActEvt_*` before inventing any way to hide, disable, or
destroy the widget.

### 26.4 Hot reload: why it still crashes (2026-08-26)

Attempted, failed, diagnosed. The skill's "never hot reload"
rule stands, but the reason is now known and partly fixed.

**What was fixed.** All four of this mod's watchers were raw
`thread::spawn(loop { sleep; work })` with no stop path. A DLL
unload leaves those threads executing freed code, which is
fatal on its own. They are now
`modforge::rpg::poller::spawn_interval` workers: stop flag,
condvar wake, joined at shutdown, panic-counted. misery
registers `poller::shutdown_all` at shutdown order 50 so they
stop BEFORE hooks tear down at 100; otherwise they tick on,
queueing into a drain nobody serves. This is worth keeping
regardless of hot reload.

**What still kills it.** With the pollers fixed, the reload
sequence gets further than before and the swap itself succeeds:

```
11:47:46 UE4SS: Stopping C++ mod 'MiseryMod' for uninstall
11:47:51 UE4SS: Setting up mods...          (5.4s later)
11:47:51 UE4SS: Starting C++ mod 'MiseryMod'
11:47:51 mod:   cleaned up main-old.dll from previous hot-reload
11:47:51 mod:   image_base = 0x7ff63f0b0000
         (nothing further; the game dies here)
```

The NEW image dies during startup, right where
`resolve_and_init` runs patternsleuth's scan: `image_base` is
logged, `patternsleuth resolved: ...` never is.

**Leading hypothesis: rayon.** patternsleuth scans in parallel
via rayon, which creates a process-global thread pool that is
never shut down. Those threads hold code addresses in the FIRST
image; after the unload that memory is gone, so the second
scan's dispatch into the pool jumps into freed code. Nothing in
the mod can stop rayon's pool, which is why this is deeper than
the poller bug. Unconfirmed: the evidence is the crash point
plus rayon being in the dependency tree, not a stack trace.

**Also noted.** Shutdown blocked for 5.4 seconds, about one poll
interval, so a watcher was mid-tick in a full GObjects walk when
stop() tried to join it. Long ticks make shutdown slow even when
they are stoppable.

**If pursued later**, the cheapest experiment is to skip the
second scan entirely: the resolved offsets are image-relative
and stable for a given exe, so they could be cached to disk on
first run and reused on reload, avoiding any call into rayon in
the new image. That tests the hypothesis and would be the fix if
it holds. Other unstoppable threads (the tiny_http server pool)
may surface next.

Until then: `restart.ps1` remains the only supported path.
`reload.ps1` exists and correctly verifies the swap, but the
game does not survive it.

### 26.3 Spawning an NPC works (confirmed 2026-08-25)

`research_spawn::spawn_one_npc`, live: copied the class of a
live hostile (BP_Assembly_C), called
`AIBlueprintHelperLibrary:SpawnAIFromClass` through the `call`
op (game thread), got a non-null pawn back, census 77 -> 78,
and the operator confirmed the spawned NPC in the world.

The recipe:

- `call` op registered via `ueforge::debug::register_pe_call`
  against the dispatch DrainSite (`src/dispatch.rs`).
- Function: `/Script/AIModule.AIBlueprintHelperLibrary:
  SpawnAIFromClass`, invoked on the library CDO. Parm block
  0x60 bytes: WorldContextObject 0x00 (the player actor),
  PawnClass 0x08 (donor NPC's UObject +0x10 class ptr),
  BehaviorTree 0x10 (null; these NPCs drive themselves),
  Location 0x18 (3 doubles), Rotation 0x30, bNoCollisionFail
  0x48, Owner 0x50 (null), ReturnValue 0x58 (the pawn).
- Player position via `Actor:K2_GetActorLocation` through the
  same `call` op (0x18-byte parm block, FVector return at 0).

This is the whole mechanism the NPC spawn multiplier needs:
enumerate a tile's placed hostiles, re-spawn copies of their
classes nearby.

## 27. The engine's allocator: slot 5, measured (2026-08-26)

Anything the engine will later grow or free MUST be allocated by
the engine. Hand it a Rust buffer and it works until the engine
reallocs that array, at which point `FMallocBinned2` looks for its
marker in the bytes before the block, finds Rust's data, and kills
the process:

```text
FMallocBinned2 Attempt to realloc an unrecognized block
000001F6FA450000  canary == 0x65 != 0xe3
```

That crash fired on DISCONNECT, long after the write that caused
it, because disconnect is when the engine tears down the arrays a
mod grew during play.

Calling the engine's allocator means calling a virtual, which
means knowing its slot. **Slot 2 was GUESSED** from patternsleuth's
own pattern bytes and tried live: the call returned null and the
process died the same second. So `ue::gmalloc::MALLOC_SLOT` was set
to `None`, and every vendor grow logged `grow failed: engine
allocator (GMalloc) unavailable` instead of risking it.

### What the binary actually does

```text
48 8B 0D 3F C1 A0 06   mov rcx, [GMalloc]
48 8B 01               mov rax, [rcx]      vtable load comes FIRST
44 8B C3               mov r8d, ebx        alignment
48 8B D7               mov rdx, rdi        size
...                    epilogue
48 FF 60 28            jmp [rax+0x28]      TAIL JUMP, slot 5
```

`0x28 / 8 = 5`. Three independent call sites agree:
`0x7ff63c833eea`, `0x7ff63c88b7b0`, `0x7ff63c88b865`.

The two argument registers are the whole discriminator. Every
`FMalloc` virtual is reached as `mov rax,[rcx]` then a call
through the vtable, so `Free(void*)` and `Realloc(...)` look
identical to `Malloc(Count, Alignment)` unless you look at which
arguments get loaded.

### Two wrong answers first, and the test caught both

`misery-mod/tests/research_gmalloc.rs` and the
`measure_malloc_slot` control re-derive this from the running
image at any time, so a game patch that moves it says so.

- **Seven different slots.** Anchoring on the xref to the GMalloc
  global alone matched every FMalloc virtual. The test asserts
  every site must agree and FAILED, rather than handing back a
  number to set. That assertion is the point of the test.
- **No match at all.** Adding the argument loads as the
  discriminator, but putting them BEFORE the vtable load. This
  build does the opposite. Rather than guess a third encoding, a
  `gmalloc_call_sites` control dumped the bytes and they were read.

### Live proof

```text
[03:32:48] gmalloc: FMalloc 0x19e29e9f2e0 vtable 0x7ff6416f1938 slot 5 -> 0x7ff63c869200, first alloc 1568 bytes
[03:32:48] vendor_mirror: added 10 items (10 custom priced)
[03:32:48] vendor_mirror: added 15 items (15 custom priced)
[03:32:48] vendor_food: added 18 items (0 custom priced)
[03:32:48] vendor_sewingkit: added 1 items (1 custom priced)
```

73 items across seven vendors, and no `grow failed` line.

### The slot was necessary but not sufficient

With slot 5 set, the vendor GROWTH stopped failing and 73 items
were added. Going to the main menu then STILL crashed the same
way, `canary == 0x1e != 0xe3`.

A second Rust allocation was hiding in the same feature:
`vendors.rs::set_custom_price` built each entry's price array
with `std::alloc::alloc_zeroed` and wrote that pointer into an
engine structure. It was justified by the claim in 24.12 that UE
never frees vendor price arrays, which is false.

**The lesson is not about this one line.** Fixing the allocator in
one place is worth nothing while another place in the same feature
still hands the engine a Rust buffer.

### How to find the next one

The dangerous shape is narrow: **a buffer WE allocated, whose
pointer is written into engine memory and left there.** Two greps
across every crate find it:

```text
std::alloc | alloc_zeroed | Layout::from_size_align
as u64).to_le_bytes()      <- a pointer being stored
```

Audited 2026-08-26 across misery, grounded2, outworld-station,
schedule1, horsey, survivalist, wwm, ueforge and modforge. One
site, the one above. What the greps also turn up, and why each is
FINE:

| Shape | Why it is safe |
|---|---|
| `parms.as_mut_ptr()` into `process_event` | a parm block borrowed for one call; the engine reads it and does not keep it |
| a world-context pointer written into a parm block | that object is the ENGINE's already, not ours |
| `Box::into_raw` on a detour | our own object; the engine never sees it |

So the rule: passing a Rust buffer to the engine is fine. STORING
one where the engine keeps it is what kills the process, and it
kills it later, somewhere else.

## 28. You cannot build an FName from a string (2026-08-27)

Every `FName` the framework has was READ off an object that
already existed: a class, an actor, an asset entry. Nothing
constructs one from a string we choose.

That is a bigger limit than it sounds. Any engine call whose
argument is a name WE pick, rather than one we found, is out of
reach. It blocked reading the asset registry's cooked tags:
`AssetRegistryHelpers::GetTagValue` exists and is callable
(4 parms, 129 bytes), but its second argument is the tag name as
an `FName`, and we cannot make one.

### SOLVED the same day

patternsleuth already ships the resolver, `FNameCtorWchar`, for
`FName::FName(wchar_t const*, EFindName)`. It simply was not
wired up. `ue::fname::from_str` calls it, and the
`string_to_fname` control exposes it.

**Find, not Add.** `EFindName` decides what happens when the name
is not in the pool: add it, or say it is not there. Finding is
what a mod wants. A name the game cooked in already exists, and a
name that does not is a question with a real answer; adding would
turn that into a silent yes.

Proven live 2026-08-27 by `tests/research_fname.rs`:

```text
StaticMesh                -> found, round trip "StaticMesh"
ThisNameCannotExist_...   -> not found, and NOT invented
```

The round trip is the proof. A non-zero `FName` only says
something came back; the same text says it is the right one. And
the miss matters as much as the hit: if find-mode quietly added
names, every "does this build have X" question would answer yes.

### What it immediately unlocked

Which asset registry tag names this build actually cooked in:

```text
ApproxSize    yes     Triangles   yes     Vertices     yes
Bounds        yes     Materials   yes     LODs         yes
MinLOD        yes     UVChannels  yes     PhysicsAsset yes
CollisionPrims yes    NaniteEnabled yes   BoundsExtent no
```

**`ApproxSize` and `Bounds` both exist**, which are the two that
would carry a mesh's dimensions. So `GetTagValue` can now be
asked for them, and the parts list may not need to load 1,500
meshes after all. See parts.md.

## 29. Reading a live actor can fault (2026-08-27)

Reading an actor means following its transform, then its mesh
component, then the mesh. Some actors in a streamed level have a
component pointer that does not resolve, and dereferencing one
takes the whole process down:

```text
EXCEPTION_ACCESS_VIOLATION reading address 0x0000008000000018
  ueforge::ue::parts::read_level (+0x1e0)
```

**The clue was in the output before the crash.** Actors were
coming back named `<bogus-fname>` and `<none>`. That is the FName
side already refusing to trust what it read: `FName::is_plausible`
rejects a garbage index rather than handing it to `AppendString`.
The mesh side had no such refusal.

So: `read_level` now wraps the per-actor read in
`modforge::seh::guard`. One bad actor becomes one skipped actor,
counted and logged, rather than a dump. The guard existed and
nothing had used it.

**The general rule.** Anything that walks live objects and
dereferences what it finds should assume some of them will be
unreadable. A streamed world is being built and torn down while
we look at it, and "this pointer is in a live object so it is
valid" is not true.

## 30. A research sweep must be bounded (2026-08-27)

Pairing every actor within 9 m across eleven loaded levels wrote
a **900 MB** file, to a disk that was 99% full, and every control
call timed out waiting for it.

Two separate mistakes, worth keeping apart:

- **No cap.** A control that writes should refuse past a size and
  say so. Nothing stopped this one.
- **No filter.** Pairing everything is meaningless as well as
  huge: a cliff 8 m from a rock is not a join. The interesting
  pairs are between building parts, and `parts.json` already
  knows which meshes are `Panel`, `Slab` or `Post`.

The earlier run that produced the clean wall-to-floor result
looked fine because it read four levels and printed the top
forty, so the noise never surfaced. A result that looks right is
not evidence that the method is.

**Also:** a large response cannot come back over the control
plane at all. 50,000 sightings is 12 MB of JSON and the client
times out reading it. Evidence belongs on disk, where it also
accumulates across sessions and can be re-derived from without
the game running.

## 31. Player input: Enhanced Input mapping decode (2026-08-27)

MISERY uses Unreal 5.4 Enhanced Input, not legacy PlayerInput.

### Pointer chain to the input objects

```
live_player (BP_SGKPlayerCharacter_C)
  +0x2C8  Controller  -> BP_SGKController_C
  +0x160  InputComponent -> EnhancedInputComponent

BP_SGKController_C
  +0x408  PlayerInput -> EnhancedPlayerInput
  field "Mapping Context" -> InputMappingContext (SGKCharacterInputs)
```

### InputMappingContext: SGKCharacterInputs

The controller holds a "Mapping Context" field pointing to an
`InputMappingContext` object named `SGKCharacterInputs`. Its
`Mappings` TArray (at offset +0x30 inside the object) holds 89
`FEnhancedActionKeyMapping` entries.

**Entry layout (stride 0x50 = 80 bytes):**

```
+0x00  16 bytes  (unknown, often zeros)
+0x10  16 bytes  (unknown)
+0x20  8 bytes   UInputAction* (action pointer)
+0x28  8 bytes   FName (key name)
+0x30  16 bytes  (zeros)
+0x40  4 bytes   0x100 constant
+0x44  4 bytes   (padding)
+0x48  8 bytes   PlayerMappableKeySettings pointer
```

Each action appears 2 or 3 times: keyboard binding, a `<none>`
slot (for remapping), and sometimes a gamepad binding.

### Complete action-to-key table

| Action | Keyboard | Gamepad |
|---|---|---|
| ForwardInput | W | Gamepad_LeftY |
| BackwardInput | S | Gamepad_LeftY |
| LeftInput | A | Gamepad_LeftX |
| RightInput | D | Gamepad_LeftX |
| InteractInput | E | Gamepad_FaceButton_Left |
| JumpInput | SpaceBar | Gamepad_FaceButton_Bottom |
| CrouchInput | LeftControl | (none) |
| CrawlInput | C | (none) |
| CrawlInput_Gamepad | RightControl | Gamepad_FaceButton_Right |
| SprintInput | LeftShift | Gamepad_LeftThumbstick |
| FireInput | LeftMouseButton | Gamepad_RightTriggerAxis |
| AimInput | RightMouseButton | Gamepad_LeftTriggerAxis |
| ReloadInput | R | Gamepad_FaceButton_Top |
| TurnInput | MouseX | Gamepad_RightX |
| LookupDownInput | MouseY | Gamepad_RightY |
| ToggleInventoryInput | Tab | Gamepad_Special_Left |
| ChatInput | Enter | Gamepad_DPad_Right |
| CompasInput | Q | Gamepad_RightThumbstick |
| ToggleCameraViewInput | V | (none) |
| ToggleFlashlightAttachmentInput | MiddleMouseButton, L | Steam_Back_Left |
| RotateBuildPartInput | R | Gamepad_FaceButton_Top |
| QuickSlot1Input | One | (none) |
| QuickSlot2Input | Two | (none) |
| QuickSlot3Input | Three | (none) |
| QuickSlot4Input | Four | (none) |
| QuickSlot5Input | Five | (none) |
| CycleQuickSlotInput | MouseWheelAxis | (none) |
| QuickSlotWheelInput | (none) | Gamepad_LeftShoulder |
| VoiceChatnput | CapsLock | Steam_Back_Right |
| WhisleInput | (none) | Gamepad_RightShoulder |
| HideHUDInput | One | (none) |
| UpContextMenuInput | MouseScrollUp | Gamepad_DPad_Up |
| DownContentMenuInput | MouseScrollDown | Gamepad_DPad_Down |
| AltModifierInput | LeftAlt | (none) |
| ShiftModifierInput | LeftShift | (none) |
| EmoteWheelInput | (none) | Gamepad_DPad_Left |
| MenuBack | (none) | Gamepad_FaceButton_Right |

### EnhancedPlayerInput fields of interest

```
EnhancedPlayerInput (at Controller+0x408)
  +0x538  EnhancedActionMappings  TArray (88 entries, stride unknown)
  +0x598  ActionInstanceData      TArray (39 entries)
  +0x688  KeysPressedThisTick     TArray
  +0x6D8  InputsInjectedThisTick  TArray
```

The EnhancedActionMappings on EnhancedPlayerInput is a flattened
copy with trigger/modifier data baked in. Its stride is larger
than 0x50 and was not decoded. The InputMappingContext.Mappings
array above is the cleaner source for the action-to-key table.

### What this means for bot input

The bot needs to inject key state that the Enhanced Input system
reads on its normal tick. The next research step is finding where
"is W currently held" lives in memory, which is one of:

1. `KeysPressedThisTick` (+0x688 on EnhancedPlayerInput)
2. `ActionInstanceData` (+0x598), which holds per-action state
   including elapsed time and trigger state
3. `InputsInjectedThisTick` (+0x6D8), which UE provides for
   programmatic input injection

The test file is `misery-mod/tests/research_player_input.rs`.

### Key state observation: idle vs holding W (2026-08-28)

Took two snapshots of the entire EnhancedPlayerInput object (0xA00
bytes): one while idle, one while physically holding W in-game.

**KeysPressedThisTick (+0x688) is always empty between ticks.**
It is populated during `ProcessInputStack` and cleared before the
next frame. Reading it between frames always shows zero entries.
Same for InputsInjectedThisTick (+0x6D8).

**ActionInstanceData holds the persistent per-action state.** The
39-entry array at +0x598 has stride 0x70 per entry. When W is
held, the ForwardInput entry changes:

```
ActionInstanceData entry layout (0x70 bytes per entry):
  +0x00  UInputAction* (pointer to the action object)
  +0x08  same pointer repeated
  +0x10  u8 trigger state: 0 = not triggered, 2 = triggered/held
  +0x18  f32 timestamp (when the trigger last fired)
  +0x40  f64 action value (0.0 idle, 1.0 full forward)
  +0x58  f64 elapsed hold time
```

Observed ForwardInput action pointer: `0x1BE044EC700`.
When W is held: trigger state = 2, action value = 1.0, elapsed
time increases each frame. When released: trigger state = 0,
action value = 0.0.

**The parent UPlayerInput also stores key state** in what appears
to be a TMap region between offsets +0x5E8 and +0x650 on the
EnhancedPlayerInput object. Several values in this range change
when W is held. This is likely `UPlayerInput::KeyStateMap`, which
tracks per-FKey pressed/released state as bitflags. The exact
layout of TMap entries has not yet been decoded.

### What the bot must write

To simulate W the same way the player does, the bot must write
to the same data that the real input processing writes to. The
candidates, in order of how close they are to the real path:

1. **KeyStateMap** (the TMap in the +0x5E8 region): this is where
   the real input stack records "W is pressed." If the bot writes
   here, the Enhanced Input system reads it on the next tick and
   fires the ForwardInput action through its normal trigger and
   modifier pipeline. This is the closest to the real player path.

2. **ActionInstanceData**: writing trigger state = 2 and value =
   1.0 directly into the ForwardInput entry would skip the
   key-to-action mapping but still let the game's movement code
   see the action as triggered. Less faithful to the real path.

3. **InputsInjectedThisTick**: UE's own programmatic injection
   TArray. If populated before ProcessInputStack runs, the engine
   treats these as real key events. Requires knowing the exact
   entry format.

The next step is decoding the KeyStateMap TMap to find the W
entry and understand its format.

### Find-out-what-writes: the write/exec watchpoint (2026-08-28)

This section replaces an earlier running log that contained claims
later disproven. It states only what is backed by pasted live
output, and marks the rest OPEN. RVAs are image-relative to
`MISERY-Win64-Shipping.exe` and identical across restarts of the
same build; absolute addresses (with ASLR) change every launch.

The method: instead of guessing an InputKey address and calling it
(every prior attempt, all failed), watch the memory a real key
press changes and let the CPU report which instruction wrote it,
then which code ran. Standard "find out what writes to this
address" plus an execution breakpoint, via CPU debug registers.

**The tool (PROVEN).** `modforge::winproc::capture_write_watchpoint`,
exposed as the `watch_writes` op (`{addr, len, duration_ms, mode}`,
mode = write / readwrite / exec). It arms DR0 across every thread,
installs a vectored exception handler, and records each hit's
instruction, argument registers (rcx/rdx/r8/r9), and the exe-code
return addresses on the stack, as RVAs. Arming verified live every
run: 123 of 123 threads, GetLastError 0.

**The read/write ops are guarded (PROVEN).** `read_bytes` and
`write_bytes` on a raw `addr:` selector previously faulted and
KILLED the game when handed a stale or code pointer (crashed MISERY
twice during this work). They now `VirtualQuery`-check every page of
the range first. Test `read_op_guards_bad_pointer`: reads and writes
of 0x1, 0xDEAD00000000, 0x7FFFFFFF0000 all return a clean error and
the game stays alive.

**Proven about the input path:**

1. The ForwardInput TRIGGER byte (ActionInstanceData entry + 0x10,
   `entry = ActionInstanceData.data + index * 0x70`) is written on a
   W press by the store instruction at RVA + 0x42f14d2, from one
   game thread. This is the Enhanced Input trigger-evaluation path,
   which runs AFTER raw key input, so it is NOT InputKey.
2. Writing the ForwardInput value at entry + 0x40 does nothing to
   the player (proven earlier by `inject_forward_input`), and it is
   not written on a key tap either. It is output.
3. EnhancedPlayerInput + 0x5E8 / + 0x5F0 catch no writes on a tap;
   the earlier before/after diff there was a net change, not a
   store site.
4. The old prologue-scan InputKey candidate (RVA + 0x320d680) never
   runs on input: an exec breakpoint on it across 123 threads caught
   ZERO hits while a control function fired 14 times in the same
   window. Every past attempt that computed that address and called
   it was calling code the game does not run for input. Retired.
5. A per-key `FKey -> FKeyState` TSet lives on EnhancedPlayerInput.
   In ONE session, the block at object-offset + 0x788 received W's
   FKeyState write (24 writes on a press) via store RVA + 0x42f25d0;
   the input-thread call chain to it was captured. CAVEAT: the block
   is located by a runtime heuristic scan, and a later run found
   DIFFERENT candidate blocks (+ 0x1D0, + 0x538, + 0x548) and caught
   no writes. So the + 0x788 object-offset is NOT confirmed stable
   across sessions; only the store RVA + 0x42f25d0 is stable.
6. None of the nine input-thread functions on the captured
   KeyStateMap store chain receives W's FKey in its argument
   registers (`find_inputkey_by_param`, 73 s sweep, no crash). So
   InputKey is not among the captured frames; it is higher up the
   stack than the handler's stack window reached at the time
   (`WATCH_STACK_BYTES` was 1 KiB; now raised to 4 KiB, not yet
   re-captured successfully).

**OPEN / NOT DETERMINED:**

- Which function is `APlayerController::InputKey`. NOT found.
- + 0x39f1210 (this = controller) and + 0x3cb9360 (this = EPI) are
  input-thread controller / EnhancedPlayerInput methods on the store
  chain, but their captured arguments are NOT an FInputKeyParams
  (rdx at + 0x39f1210 is a UObject container; W's FName index 0xD08
  is absent). An earlier revision of this doc named these as
  InputKey; that naming is RETRACTED. rcx matching the controller /
  EPI proves only the class of `this`, not that the function is
  InputKey.
- The FInputKeyParams layout in this build (not yet read from a
  confirmed InputKey frame).

**What a correct next attempt looks like:** with the deeper stack
capture, re-run `find_inputkey_write` (needs a real W press held
during the watched window) to capture the chain up past the store,
then `find_inputkey_by_param` on the newly revealed higher frames.
The target is the frame whose argument register points to a struct
whose first 8 bytes are W's FName (0xD08) and whose next 8 are W's
FKeyDetails pointer.

### Prior art: how InputKey is actually called (2026-08-28)

The from-scratch reverse-engineering above was unnecessary. Calling
InputKey in a UE game is a solved problem. Two facts from public
code, found via `gh`:

**1. The real FInputKeyParams layout** (Epic's
`Engine/Source/Runtime/Engine/Classes/GameFramework/PlayerInput.h`,
mirrored on GitHub; `bl-sdk/oak2-mod-manager` uses the same):

```
FKey            Key;                // +0x00, 24 bytes (FName + TSharedPtr<FKeyDetails>)
FInputDeviceId  InputDevice;        // +0x18, 4
EInputEvent     Event;              // +0x1C, 4  (0 = IE_Pressed, 1 = IE_Released)
int32           NumSamples;         // +0x20, 4
float           DeltaTime;          // +0x24, 4
FVector         Delta;              // +0x28, 24 (3 doubles)
bool            bIsGamepadOverride; // +0x40, 1
                                    // size 0x48
```

The mod's earlier hand-rolled struct put Delta right after the key,
so Event landed at the wrong offset. That, not a wrong address, is
why every prior "call InputKey" returned true and did nothing.
`misery-mod/src/input.rs` now uses the correct layout.

**2. InputKey is a vtable virtual on the PlayerInput object.**
`bl-sdk` calls it as `InputKey(UEnhancedPlayerInput* this,
FInputKeyParams* params)` from the EnhancedPlayerInput vtable at an
index near 85 (its value for Borderlands' engine). `try_inputkey`
now calls `EnhancedPlayerInput.vtable[slot](epi, &params)` with the
params in a 256-byte zeroed buffer (so a wrong slot that reads past
the struct hits zeros, not a fault).

**SOLVED on MISERY (UE 5.4): InputKey is EnhancedPlayerInput vtable
slot 88** (RVA + 0x42f5970 this build). `find_inputkey_slot_via_iskeydown`
proves it against the game's own state query, no gameplay needed:

```
baseline IsInputKeyDown(W) = false
slot 88: after InputKey(W, pressed)  IsInputKeyDown(W) = true
         after InputKey(W, released) IsInputKeyDown(W) = false
```

Calling `EnhancedPlayerInput.vtable[88](epi, &params)` records W in
the KeyStateMap exactly as a physical press does, and release clears
it. Two things were required and are both in `try_inputkey` now:

1. The correct `FInputKeyParams` layout above.
2. A REAL FKey with a valid FKeyDetails pointer. The
   InputMappingContext (SGKCharacterInputs) FKeys have NULL details,
   so they are useless; the EnhancedActionMappings / KeyStateMap
   blocks on the EPI object carry FKeys with a valid details pointer.
   `keystatemap_w_entries` lifts one; native input code dereferences
   FKeyDetails, so a name-only FKey crashes it (that was the cause of
   several game crashes during this work).

`dump_epi_vtable_rvas` shows the EnhancedInput overrides occupy slots
87-99; of those, 91 and 93 hang/crash when called as InputKey (they
are other virtuals, e.g. InputAxis). Slot 88 is the one.

This is verified by KeyStateMap state, not by player movement.

### End-to-end test: InputKey injects the key but does NOT move the character (2026-08-28)

`test_inputkey_movement` called slot 88 for W while in active
gameplay and sampled both the player location and the ForwardInput
action state:

```
press slot 88: returned=true
holding W: moved 0.0  ForwardInput trigger=0 value=0.00   (x6)
total moved: 0.0
```

So slot 88 correctly sets the legacy KeyStateMap (IsInputKeyDown =
true) but the `ForwardInput` Enhanced Input ACTION never fires and
the character does not move. MISERY's movement is bound through
Enhanced Input actions, and a raw out-of-band InputKey does not
drive the action evaluation.

### The right mechanism: InjectInputForAction (Enhanced Input)

Prior art (`Lyall/WukongTweak`, PalWorld/Wukong SDK dumps, found via
`gh`): the sanctioned way to drive an Enhanced Input action
programmatically is
`IEnhancedInputSubsystemInterface::InjectInputForAction(UInputAction*
Action, FInputActionValue RawValue, TArray<UInputModifier*>
Modifiers, TArray<UInputTrigger*> Triggers)`, a BlueprintCallable
UFunction on `UEnhancedInputLocalPlayerSubsystem`. It injects the
action for one tick, so it is re-called each frame while the input
is held. This targets the `ForwardInput` (etc.) UInputAction
directly and is what actually moves the character. InputKey (slot
88) stays the correct answer for raw key state, but movement runs
through the action layer, so the bot injects at the action layer.

