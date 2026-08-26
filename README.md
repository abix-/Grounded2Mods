# modforge

> **Vibecoded.** This repo is written almost entirely by Claude (Anthropic) under human direction. Architecture, code, tests, docs, decompilation, and the Horsey gene-research all came out of long iterative sessions. If something looks suspiciously well-organised or suspiciously over-engineered, that's why. Read with that context: AI-generated code with human review, not human-authored code with AI assistance.

A Rust workspace for game mods. One foundation crate
(**`modforge`**), two engine-binding "forge" crates that adapt
it to a specific host runtime (UE5, Unity), and per-game mod
crates that consume modforge directly or through a forge.

```
                ┌────────────────────────────┐
                │          modforge          │
                │  HTTP server · op registry │
                │  selector grammar · RPG    │
                │  log · scanner · winproc   │
                │  shutdown · settings       │
                └─────────────┬──────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
    ┌────▼────┐         ┌─────▼─────┐    (no engine; per-game
    │ ueforge │         │unityforge │     crate consumes modforge
    │  UE5    │         │   Unity   │     directly with its own
    │  UE4SS  │         │  Mono /   │     loader, e.g. PE inject)
    │         │         │   IL2CPP  │
    └────┬────┘         └─────┬─────┘
         │                    │
   ┌─────┴─────────┐    ┌─────┴─────────┐    ┌──────────────┐
   │ grounded2-mod │    │ schedule1-mod │    │  horsey-mod  │
   │ misery-mod    │    │ survivalist-  │    │ (Horsey Game,│
   │ outworld-     │    │   mod         │    │  PE inject)  │
   │ station-mod   │    │ wwm-mod       │    │              │
   │               │    │ il2cpp-smoke  │    │              │
   └───────────────┘    └───────────────┘    └──────────────┘
```

An "engine forge" is reusable: any UE5 game sits on ueforge,
any Unity game sits on unityforge. A native-PE game has no
managed loader to lean on, so its per-game crate carries its
own injector + game-specific accessors and consumes modforge
directly. The first such crate is `horsey-mod` (Horsey Game).
If a second native-PE game shows up, the
`inject + HTTP-bind + binary-patch` pattern in horsey-mod
gets lifted into a shared `modforge::inject` module; until
then it lives where it's used.

## Framework capabilities

The placement rule is simple: engine-independent code belongs in
`modforge`; Unreal Engine bindings belong in `ueforge`; Unity
bindings belong in `unityforge`. Game crates keep only game facts,
content, offsets, and behavior.

### modforge

Engine-independent systems usable by a mod or a standalone game.

| Capability | What it provides |
|---|---|
| Control plane | Local HTTP server, request envelope, operation registry, typed arguments, snapshots, and client |
| Lifecycle and state | Settings, logging, shutdown ordering, hot-reload protocol, counters, bounded rings, and workers |
| Runtime tooling | Memory access, scanners, pattern sleuthing, safe native calls, Windows process inspection, and research helpers |
| Input and UI | Native input backends, coordinate handling, and declarative overlay primitives |
| Items and progression | Item definitions and inventory, unique-item ledger, quality rolls, crafting, upgrades, and the full RPG effect/trigger/skill stack |
| Simulation systems | Actors, decisions, combat, factions, genomes, survival classification, missions, contracts, storyteller pacing, and adaptive pressure |
| World building | Biomes, structures, monuments, and deterministic world generation |
| Verification and delivery | Shared testkit, runtime harness, HTTP client, build helpers, and deploy CLI |

See [`modforge/README.md`](modforge/README.md) and
[`modforge/docs/`](modforge/docs/).

### ueforge

The Unreal Engine 5 and UE4SS binding layer.

| Capability | What it provides |
|---|---|
| Mod lifecycle | `ue4ss_mod!`, UE4SS entry points, the shared C++ shim, feature installation, shutdown, and hot reload |
| UE runtime | UObject, UClass, UFunction, GObjects, FName, FString, TArray, reflected fields, parameters, and platform offsets |
| Hooks and game thread | ProcessEvent and vtable hooks, fall and damage hooks, frame callbacks, and the game-thread queue |
| Data and assets | DataTable access, typed field tweaks, dynamic tweaks, discovery, asset registry access, and uasset inspection |
| Gameplay modules | RPG, stack sizes, difficulty knobs, inventory viewport paging, status effects, and damage dispatch |
| Operator UI | ImGui bindings plus class, struct, DataTable, scanner, and tweak browsers |
| Control and inspection | UE selectors, standard operations, memory tools, counters, snapshots, and the shared HTTP surface |
| Testing and builds | Runtime client, scenario helpers, C++ build integration, packaging, and deployment commands |

See [`ueforge/README.md`](ueforge/README.md) and
[`ueforge/docs/`](ueforge/docs/).

### unityforge

The Unity Mono and IL2CPP binding layer.

| Capability | What it provides |
|---|---|
| Managed loaders | C# shims for BepInEx Mono, MelonLoader IL2CPP, and Survivalist's built-in loader |
| Rust lifecycle | Rust cdylib initialization, per-frame dispatch, shutdown, generation activation, rollback, and generation-versioned hot reload |
| Mono bridge | Assembly and type discovery, singleton lookup, method invocation, field reads/writes, and managed handle ownership |
| IL2CPP bridge | IL2CPP loader and bridge surface for native Unity builds |
| Harmony hooks | Prefix and postfix registration, hook contexts, registry ownership, and safe removal through the managed shim |
| Unity runtime | GameObject and Component access, Unity selectors, input bridge, and main-thread work queue |
| Gameplay bindings | Unity implementations for Modforge RPG effects, triggers, skills, tracking, and slot identity |
| Control and testing | Unity operation handlers and selectors with the shared Modforge HTTP client |

See [`unityforge/README.md`](unityforge/README.md) and
[`docs/unityforge-plan.md`](docs/unityforge-plan.md).

## Game-side mods

| Game | Engine / loader | Features | Rating |
|---|---|---|---|
| [Grounded 2](grounded2-mod/) | ueforge (UE5 / UE4SS) | 14-skill RPG (backpack, hunger, thirst, attack damage, armor, move speed, jump height, glide speed, fall resistance, impact resistance, lifesteal, max health, leap distance, health regen), XP levelling, save persistence, damage hook, fall damage hook, inventory hook, ImGui overlay | 4/10 |
| [MISERY](misery-mod/) | ueforge (UE5 / UE4SS) | Emission timer control, 10x stack sizes, playtest nag suppression, 2x movement speed, vendor food expansion, ImGui overlay | 2/10 |
| [Outworld Station](outworld-station-mod/) | ueforge (UE5 / UE4SS) | Stack size multiplier, dynamic data table tweaks, ImGui overlay | 3/10 |
| [Schedule 1](schedule1-mod/) | unityforge (IL2CPP / MelonLoader) | Combat-XP levelling, heavy hands skill, cash loot drops, farming, kill credit, combat trace | 3/10 |
| [Survivalist](survivalist-mod/) | unityforge (native mod loader) | No infections, crafted item quality tiers, faction war (AI vs AI revenge), bounties on enemy leaders, camp threat clearing, named unique items, settlement upgrades, refugee recruitment, structure growth, survival desperation ladder, storyteller drama pacing, genome persistence, trade/steal/rob/scavenge/murder missions, horde raids, incursions, stranger events, courier deliveries | 1/10 |
| [Wild West Miner](wwm-mod/) | unityforge (Mono / BepInEx) | Demo-end block, spacebar jump (Translate +3m), RPG skill catalog (parked) | 2/10 |
| [Horsey Game](horsey-mod/) | modforge (PE inject) | Fatigue suppressor (sleep-safe, race-eligible), money get/set/add, year get/set, no-tire toggle, debug mode toggle, horse roster/read/age/tiredness, vanilla allele get/set, genome get/set, chromosome dump, gene name lookup, heap string scanner, xref finder, memory peek/poke, data/rdata scan, target resolver (field offsets, data globals, cheat globals, gamestate ptr, chromosome table), hot reload, proxy DLL injection | 3/10 |
| [Scrap Mechanic](scrapmechanic-mod/) | Lua (native) | 1000 inventory slots, no inventory loss on death, half fuel consumption, no building restrictions, 3x enemy/loot respawn rates | 3/10 |
| [Quasimorph](quasimorph-mod/) | C# (native mod API) | Empty scaffold (AfterConfigsLoaded + DungeonStarted hooks, no logic) | 1/10 |
| (il2cpp-smoke) | unityforge (IL2CPP) | IL2CPP path end-to-end test | n/a |

> **Rating scale:** 10/10 = ready for 1000 players, fun, zero bugs.

## Reverse engineering

Decompiler implementation details, build instructions, sample
output, the retired Falcon prototype, and the honest assessment of
whether the work was worthwhile live in the dedicated
[`decomp documentation`](decomp/README.md). Read the
[`retrospective`](decomp/docs/retrospective.md) before investing in
that tooling. The root README keeps only the workspace map.

## Build prerequisites

- Windows 10/11 x64
- Rust toolchain (rustup; stable pinned via `rust-toolchain.toml`)
- Visual Studio Build Tools 2022+ with the C++ workload
- For ueforge mods: the target game's UE4SS install
- For unityforge mods: BepInEx (Mono games) or MelonLoader (IL2CPP games)
- For framework dev: clone with `--recurse-submodules`. Dear
  ImGui v1.92.1 lives in a submodule.

## Docs

Workspace-level docs live in [`docs/`](docs/):
[`todo.md`](docs/todo.md),
[`changelog.md`](docs/changelog.md),
[`unityforge-plan.md`](docs/unityforge-plan.md).
Per-mod docs are linked in the table above. Framework docs:
[`ueforge/`](ueforge/README.md),
[`unityforge/`](unityforge/).
Research tooling:
[`decomp/`](decomp/README.md) (r2sleigh-based, WSL-only),
[`falcon-printer/`](falcon-printer/) (retired prototype, docs only).

## Credits

- **UE4SS-RE** for [RE-UE4SS](https://github.com/UE4SS-RE/RE-UE4SS),
  the CPPMod host every UE5 mod here targets.
- **BepInEx** for the Unity plugin loader unityforge attaches
  to.
- **HarmonyX** for the runtime patching library the unityforge
  C# shim uses.
- **x0reaxeax** for [Grounded2Minimal](https://github.com/x0reaxeax/Grounded2Minimal)
  and [G2Dumper](https://github.com/x0reaxeax/G2Dumper).
- **Encryqed** for [Dumper-7](https://github.com/Encryqed/Dumper-7),
  the SDK generator that produced reference headers for every
  UE5 game we target.
- **RLGingerBiscuit** for [G2Utils](https://github.com/RLGingerBiscuit/G2Utils),
  which corroborated class names + inventory bindings on
  Grounded 2.
- **Trumank** for [retoc](https://github.com/trumank/retoc) and
  [repak](https://github.com/trumank/repak), used during early
  pak-prototype work.
- **4sval** for [FModel](https://github.com/4sval/FModel), used
  for cooked asset inspection.
- **Caites** for [Player Tweaks](https://www.nexusmods.com/grounded2/mods/13)
  on Nexus, whose feature list shaped the Grounded 2 catalog.
- The author of [Bigger Backpack](https://www.nexusmods.com/grounded2/mods/37),
  whose mod's breakage motivated the data-side + visible-side
  patching pattern.
- The author of [**RPG System**](https://mods.factorio.com/mod/RPGsystem)
  for Factorio. The headline RPG-style level-up mod whose
  vocabulary `grounded2-mod` and `wwm-mod` borrow verbatim.
- The author of [**RimWorld RPG Mod / Combat Skills RPG**](https://steamcommunity.com/sharedfiles/filedetails/?id=2891939858).
- The authors of the [War3CS / War3FT](https://war3cs2.wiki.gg/)
  Counter-Strike Warcraft mod line, whose flat-skill-catalog
  pattern shapes the RPG catalog layout.
- **MelonLoader** for the Unity IL2CPP mod loader Schedule 1
  targets.
- The game studios whose titles we mod (Obsidian Entertainment
  for Grounded 2, the MISERY team, the Outworld Station team,
  TVGS for Schedule 1, the Survivalist: Invisible Strain team,
  the Wild West Miner team, Axolot Games for Scrap Mechanic,
  the Quasimorph team, and the Horsey Game team). We modify
  only what the official games ship under fair-use modding
  norms; no game assets are redistributed.
