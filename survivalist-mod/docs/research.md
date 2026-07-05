# Survivalist: Invisible Strain research

The mod can read and change the live game through the control
plane on port 17173. The game's tweak surface splits into a
difficulty knob cluster (one object holding ~30 densities and
rates), an AI system built from Goal classes, spawner classes per
thing-that-spawns, and data-driven content the in-game editors
own. All of it is reachable: plain C#, no obfuscation.

How a tweak lands, end to end: resolve a holder object over HTTP
(`list_singletons`), read or write its fields (`read_field` /
`write_field`), or patch a method (Harmony via the Rust hook
path) when behavior rather than data must change. Everything
verified below was checked against the live game 2026-07-04.

## The live-access recipe (verified)

Op args nest under `"args"`:

```
curl http://127.0.0.1:17173/op -d '{"op":"list_singletons","args":{"types":["Session"]}}'
  -> {"singletons":[{"class":"Session","found":true,"handle":N}]}
curl ... '{"op":"read_field","args":{"handle":N,"field":"DifficultySettings"}}'
  -> {"handle":M,"type":"DifficultySettings"}
curl ... '{"op":"inspect_object","args":{"handle":M}}'
  -> every field with live values
```

- `Session.Instance` (plain `public static Session Instance`)
  resolves through the singleton fallback (static `Instance`
  field on the type itself).
- Handle numbers reset on hot reload / handle-table clear; always
  re-resolve, never persist handles.
- `walk_class` only finds UnityEngine.Object subclasses
  (FindObjectsOfType); plain classes need a singleton/static
  path or a field walk from one.

## Difficulty knob cluster (verified live)

`Session.Instance.DifficultySettings` (class
`DifficultySettings`, all public fields; decompile with
`ilspycmd -t DifficultySettings Assembly-CSharp.dll`):

| Area | Fields |
|---|---|
| Zombies | ZombieCrippledPercentage, ZombieRespawnDays, GreenStrainDensity, BlueStrainDensity, RedStrainDensity, WhiteStrainDensity, InvisibleStrainPercentage, InvisibleStrainIncrementPerYear, HordeDensity |
| People | SurvivorCampDensity, SurvivorCampLooterPercentage, SurvivorRepopulationDays, RaiderDensity, RefugeeDensity, TraderDensity |
| Loot / world | LootDensity, VehicleDensity, TownDensity, RoadDensity, RiverDensity, CountrysidePropDensity, FlintDensity, LeadOreDensity, IronOreDensity |
| Animals | RabbitDensity, DeerDensity |
| Rules | SaveTokensRequired, TradersHaveSaveTokens, FriendlyFireSplashDamage |

Live values observed (operator's Medium game): DifficultyName
"Medium", ZombieRespawnDays 1.0, strain densities 25.0,
InvisibleStrainPercentage 0.0.

OPEN QUESTION per field: which are worldgen-time only (baked into
the map at new-game) vs consulted at runtime (respawn /
repopulation ticks). ZombieRespawnDays / SurvivorRepopulationDays
read as runtime-tick candidates; the *Density worldgen fields
likely only matter at map generation. Confirm per field by
finding its readers (`ilspycmd` + grep) before promising a live
write does anything.

## Zombies

- AI: `ZombieGoal`, `ZombieAlertGoal`, `ZombieFrustrationGoal`,
  `ZombieJumpGoal`, `ZombieStumble`.
- Spawning: `ZombieSpawnPoint`, plus the DifficultySettings
  strain densities + ZombieRespawnDays above. The dev's own
  custom-code guide demos a zombie-spawn Harmony patch, so this
  surface is confirmed patchable.
- Stealth interplay: `OnlyVisibleForZombie`, `NotVisibleForZombie`.

## NPC behavior

- The AI is a Goal system: ~40+ `*Goal` classes on a common
  `Goal` base (Alert, AttackFallback, AutoCollect, AutoDeposit,
  BandageSelf, Bored, Build, Bury, Capture, Clothing, Craft,
  Depressed, Drink, Eat, Farming, Find, Flee, Follow, Gather,
  KeepFireAlive, Read, gate open/close/lock, animal goals, ...).
  Behavior tweaks = Harmony patches on the relevant Goal's
  methods (or on `CharacterBehaviour`).
- Actors: `Character`, `CharacterBehaviour`, `CharacterManager`,
  `CharacterSpawner`, `SavedCharacter`.
- Groups: `Community`, `CommunityManager` (+ per-aspect editors:
  area ownership, crop patches, perimeter).
- Dialog/quests: `StoryManager` (speech option evaluation),
  Script editor content (data-side).

## Loot

- `EquipmentSpawner`, `LootableFrom`, `Equipment`,
  `EquipmentContainer`, `EquipmentPrototype`,
  `EquipmentSettings`, plus `DifficultySettings.LootDensity`.
- The game ships `DebugMenuLootLocationAdjuster` (worth reading
  for how loot locations are modeled).
- Also spawners per kind: `LiquidSpawner`, `PropSpawner`,
  `TerrainRockSpawner`, `VehicleSpawner`.

## Content (data-side, no code)

Items/props/quests/dialog are XML under the story folder
(`Liquid/`, `Equipment/`, `Props/`, `Scripts/` per the Story
loader) edited with the in-game editors (Edit Equipment, Edit
Props, Script editor). A content tweak that only changes data
belongs in the SurvivalistTweaks mod folder as XML, not in Rust.

## Game plumbing worth knowing

- `Session.Instance`: the running game session (holds
  DifficultySettings, player records, faction name constants).
- `GameImpl`: top-level game flow (save/load, dialogs, session).
- `Story` / `StoryManager` / `StorySettings`: story + mod
  loading, DLLs, asset bundles, scripts.
- `SaveGameManager`, `BaseObjectManager`, `WorkshopManager`.
- Difficulty UI: `DifficultyDialog`, `DifficultyDebugMenu`
  (the game has debug menus; F8 opens the debug menu per the
  dev's model guide).

## Harmony 2.0.4 constraints (both live-verified the hard way, 2026-07-04)

The game's official Harmony is pardeike 2.0.4 (the workshop
dependency mod), NOT a current Harmony and NOT HarmonyX. Two
patch-time failures came from assuming features it doesn't have:

| Wanted | 2.0.4 reality |
|---|---|
| `harmony.UnpatchSelf()` | Does not exist (HarmonyX name). Use per-target `Unpatch(original, patchMethodInfo)`. Caught at compile time. |
| `object[] __args` patch parameter | Harmony 2.1 feature. 2.0.4 parses it as an invalid indexed parameter and `Harmony.Patch` throws "Parameter __args does not contain a valid index" AT PATCH TIME. Use indexed injection (`object __0`) instead. Runtime-parsed string, so the compiler can NOT catch this class. |

The COMPLETE special-parameter set in 2.0.4, verified against the
source at github.com/pardeike/Harmony tag `v2.0.4.0`,
`Harmony/Internal/MethodPatcher.cs` (the string constants defined
there; nothing else is special):

| Name | Meaning |
|---|---|
| `__instance` | the instance of the patched (instance) method |
| `__originalMethod` | the MethodBase being patched |
| `__result` | the return value |
| `__state` | prefix-to-postfix state |
| `__exception` | finalizer exception |
| `__N` (`__0`, `__1`, ...) | argument by index |
| `___fieldName` | instance field access (three underscores) |
| named parameters | must match the original's argument names |

There is NO `__args` in 2.0.4 (it is a later-Harmony feature).
The parser strips the `__` prefix and int-parses the rest, which
is exactly the "Parameter __args does not contain a valid index"
patch-time exception we hit.

Indexed-parameter caveat (from the same source): the argument is
loaded AS-IS with no conversion or boxing. Declaring the patch
parameter as `object` is safe when the original argument is a
REFERENCE type (our AddInjury case: `Injury` is a class); a
VALUE-type argument must be declared with its exact type, never
`object`. The bridge's arg0 ctx path therefore only supports
reference-type first arguments.

THIRD constraint (live 2026-07-04, mechanism verified against the
same source): `MethodBase __originalMethod` compiles and is a
legal 2.0.4 parameter, but 2.0.4 emits it as `Ldtoken original` +
`Call MethodBase.GetMethodFromHandle`, and this game's Mono
(Unity 6000) cannot resolve that call token inside the dynamic
wrapper: patching throws "Invalid IL code in (wrapper
dynamic-method) ... call 0x00000005" AT PATCH TIME. So on this
stack a patch method may not use `__originalMethod` at all. The
bridge therefore routes each Rust patch through a PRE-COMPILED
STATIC SLOT METHOD (16 per kind in HarmonyBridge.cs) whose
signatures use only token-free `ldarg` emissions: `()`,
`(object __instance)`, `(object __0)`. Those are the shapes the
game's working mods (DisableHUD, SISLootRespawn) also use.

RULE (operator, 2026-07-04): do not guess at Harmony surface.
Before using any Harmony feature here, verify it against the
pardeike Harmony source at tag `v2.0.4.0` or against an existing
working Survivalist mod (DisableHUD on github, the dev's example
mods, workshop mods like SISLootRespawn). This game has official
mod support and plenty of working examples; build on top of them.

## Type resolution gotcha (live-verified 2026-07-04)

Existing C# mods reference game types at COMPILE time against
Assembly-CSharp, so they can never resolve a wrong type. Our
bridge resolves type NAMES at runtime, and Unity itself ships
colliding short names: `UnityEngine.TextCore.Character` shadowed
the game's global-namespace `Character` in the shim's short-name
scan and silently killed the AddInjury patch (returned handle 0,
no exception). Fixed in the shim's TypeCache: exact
namespace-qualified match across ALL assemblies first, short-name
scan only as a fallback; plus loud `HarmonyBridge: type/method
not found` errors on every resolution miss. When a patch fails,
read the player log; the miss is named now.

## Mod deploy facts

- Mod folder:
  `<game>\Survivalist Invisible Strain_Data\StreamingAssets\SurvivalistTweaks\`
  (created mods land under StreamingAssets; the setup guide's
  "game's directory" wording is imprecise).
- Deploy: `survivalist-mod/scripts/build_and_deploy.ps1
  [-ModName SurvivalistTweaks] [-Hot]`.
- Player log:
  `%USERPROFILE%\AppData\LocalLow\Ginormocorp Industries\Survivalist Invisible Strain\Player.log`.
- Quit-to-menu does NOT unload mod DLLs (no `UnloadDLLs`); only a
  story switch does.
