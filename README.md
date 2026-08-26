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
| HTTP control plane | [Arguments](modforge/src/args.rs) for typed JSON values, an [envelope](modforge/src/envelope.rs) for requests and responses, an [operations](modforge/src/ops.rs) registry, a local [server](modforge/src/server.rs), an HTTP [client](modforge/src/client/mod.rs), and RPG [debug](modforge/src/debug.rs) views |
| Snapshot API | [Snapshots](modforge/src/snapshots/mod.rs) are reserved; no concrete snapshot type exists yet |
| Configuration and lifecycle | [Settings](modforge/src/settings.rs) persistence, [logging](modforge/src/log.rs), [shutdown](modforge/src/shutdown.rs) ordering, UE4SS [reload](modforge/src/hot_reload.rs), and guarded [workers](modforge/src/worker.rs) |
| Telemetry | [Counters](modforge/src/counters.rs) for hot paths and bounded event [rings](modforge/src/ring.rs) |
| Native calls and hooks | [Hooks](modforge/src/hook.rs) via function detours, [vanilla](modforge/src/vanilla/mod.rs) calls by address, and [SEH](modforge/src/seh.rs) guards |
| Inspection and research | [Patterns](modforge/src/patterns/mod.rs) for byte scanning, [scanning](modforge/src/scanner.rs) and freezing values, [process](modforge/src/winproc.rs) inspection, binary [research](modforge/src/research.rs) recipes, and [minidumps](modforge/src/bin/minidump.rs) |
| Input | [Input](modforge/src/input/mod.rs) synthesis, plus [actions](modforge/src/actions.rs), bindings, queues, and replay journals |
| UI | [UI](modforge/src/ui/mod.rs) declarations and player [HUD](modforge/src/hud.rs) state |
| Items | [Items](modforge/src/item.rs), inventory, and the unique-item ledger; [quality](modforge/src/quality.rs) tiers; and [crafting](modforge/src/crafting.rs) recipes |
| Progression | [Upgrades](modforge/src/upgrade.rs) with persistence, plus [RPG](modforge/src/rpg/mod.rs) effects, triggers, skills, XP, state, and persistence |
| Actors and AI | [Actors](modforge/src/actor.rs) definitions and spawning data, [decisions](modforge/src/brain.rs), and actor [memory](modforge/src/memory.rs) for knowledge and threats |
| Combat | [Combat](modforge/src/combat.rs) covers damage, health, protection, hit resolution, firing, and weapon timing |
| Factions | [Factions](modforge/src/faction.rs) definitions, registries, and relationships |
| Genomes | [Genomes](modforge/src/genome.rs) provide traits, reinforcement, heredity, voting, and persistence |
| Survival | [Survival](modforge/src/survival.rs) provides needs, rates, conditions, and settlement classification |
| Missions | [Missions](modforge/src/mission.rs) cover multi-stage, go-and-return, one-stage, and contract lifecycles |
| Storytelling | [Storytelling](modforge/src/storyteller.rs) provides event pacing and adaptive pressure, plus a [dread](modforge/src/unknown.rs) loop |
| World generation | [Biomes](modforge/src/biome.rs), [structures](modforge/src/structure.rs), [monuments](modforge/src/monument.rs), deterministic [worlds](modforge/src/worldgen.rs), and population [rolls](modforge/src/roll.rs) |
| Testing | [Testkit](modforge/src/testkit/mod.rs) assertions and a Steam-game [harness](modforge/src/harness/mod.rs) |
| Delivery | [Deploy](modforge/src/bin/modforge_deploy.rs) CLI for building, installing, uninstalling, and packaging |

See [`modforge/README.md`](modforge/README.md) and
[`modforge/docs/`](modforge/docs/).

### ueforge

The Unreal Engine 5 and UE4SS binding layer.

| Capability | What it provides |
|---|---|
| Mod lifecycle | [Lifecycle](ueforge/src/mod_main.rs) provides `ue4ss_mod!` and UE4SS entry points; the [shim](ueforge/cpp/ueforge_shim.cpp), [features](ueforge/src/features.rs), [shutdown](ueforge/src/shutdown.rs), and [reload](ueforge/src/hot_reload.rs) complete the path |
| UE runtime | [Objects](ueforge/src/ue/uobject.rs) cover UObject and UClass, with [functions](ueforge/src/ue/pe_call.rs), [GObjects](ueforge/src/ue/core_types.rs), [names](ueforge/src/ue/fname.rs), [strings](ueforge/src/ue/fstring.rs), [arrays](ueforge/src/ue/tarray.rs), reflected [fields](ueforge/src/ue/field.rs), [parameters](ueforge/src/parms.rs), and platform [offsets](ueforge/src/ue/offsets.rs) |
| Hooks and game thread | [ProcessEvent](ueforge/src/hook/process_event.rs), [vtables](ueforge/src/hook/vtable.rs), [falls](ueforge/src/fall.rs), [damage](ueforge/src/damage/mod.rs), [frames](ueforge/src/frame.rs), and the game-thread [queue](ueforge/src/game_thread.rs) |
| Data and assets | [DataTables](ueforge/src/data_table.rs), typed [tweaks](ueforge/src/tweak.rs), [dynamic](ueforge/src/dynamic_tweaks.rs) tweaks, [discovery](ueforge/src/discovery.rs), [assets](ueforge/src/assets.rs), and [uassets](ueforge/src/uasset.rs) |
| Gameplay modules | [RPG](ueforge/src/rpg/mod.rs), [stacks](ueforge/src/tweak.rs) and difficulty knobs, [inventory](ueforge/src/inventory/viewport.rs) paging, [statuses](ueforge/src/ue/status_effect.rs), and [damage](ueforge/src/damage/mod.rs) dispatch |
| Operator UI | [ImGui](ueforge/src/ui.rs) bindings with [class](ueforge/src/ui_class_browser.rs), [struct](ueforge/src/ui_struct_browser.rs), [DataTable](ueforge/src/ui_data_table_browser.rs), [scanner](ueforge/src/ui_scanner.rs), and [tweak](ueforge/src/ui_tweaks.rs) browsers |
| Control and inspection | [Selectors](ueforge/src/selector.rs), [operations](ueforge/src/ops.rs), [scanning](ueforge/src/scanner.rs), [counters](ueforge/src/counters.rs), [snapshots](ueforge/src/debug/mod.rs), and [HTTP](ueforge/src/server.rs) control |
| Testing and builds | [Client](ueforge/src/client/mod.rs), [scenarios](modforge/src/client/scenario.rs), and [builds](ueforge/src/build.rs) with C++ integration, packaging, and deployment |

See [`ueforge/README.md`](ueforge/README.md) and
[`ueforge/docs/`](ueforge/docs/).

### unityforge

The Unity Mono and IL2CPP binding layer.

| Capability | What it provides |
|---|---|
| Managed loaders | C# shims for [BepInEx](unityforge/cs-shim-mono/Plugin.cs) Mono, [MelonLoader](unityforge/cs-shim-melonloader/Mod.cs) IL2CPP, and [Survivalist](unityforge/cs-shim-survivalist/Main.cs) |
| Rust lifecycle | [Lifecycle](unityforge/src/mod_main.rs) covers cdylib initialization, per-frame dispatch, shutdown, generation activation, rollback, and hot reload |
| Mono bridge | [Mono](unityforge/src/mono.rs) provides assembly and type discovery, singleton lookup, invocation, field access, and managed handles |
| IL2CPP bridge | [IL2CPP](unityforge/src/il2cpp.rs) provides the loader and bridge for native Unity builds |
| Harmony hooks | [Hooks](unityforge/src/hook.rs) provide prefixes, postfixes, contexts, ownership, and safe removal through the managed [shim](unityforge/cs-shim-common/HarmonyBridge.cs) |
| Unity runtime | [Objects](unityforge/src/unity.rs) cover GameObject and Component access, with [selectors](unityforge/src/selector.rs), [input](unityforge/src/input.rs), and a main-thread [queue](unityforge/src/main_thread_queue.rs) |
| Gameplay bindings | Unity implementations for Modforge RPG [effects](unityforge/src/rpg/std_effect.rs), [triggers](unityforge/src/rpg/trigger_harmony.rs), [skills](unityforge/src/rpg/skill.rs), [tracking](unityforge/src/rpg/tracker.rs), and slot [identity](unityforge/src/rpg/slot_key.rs) |
| Control and testing | Unity [operations](unityforge/src/ops.rs) and [selectors](unityforge/src/selector.rs) with the shared Modforge HTTP [client](unityforge/src/client.rs) |

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
