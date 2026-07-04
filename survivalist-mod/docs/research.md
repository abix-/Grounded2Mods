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
