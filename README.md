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
| [Arguments](modforge/src/args.rs) | Typed JSON argument helpers for operation handlers |
| [Envelope](modforge/src/envelope.rs) | Uniform operation requests and responses |
| [Operations](modforge/src/ops.rs) | Registered operations, dispatch, discovery, and metrics |
| [Server](modforge/src/server.rs) | Local HTTP transport for a mod's control plane |
| [Client](modforge/src/client/mod.rs) | Blocking HTTP client for runtime tests and tools |
| [Debug](modforge/src/debug.rs) | Engine-independent RPG state and catalog views |
| [Snapshots](modforge/src/snapshots/mod.rs) | Reserved projection snapshot API; no concrete snapshot type exists yet |
| [Settings](modforge/src/settings.rs) | Persistent JSON configuration with atomic saves |
| [Logging](modforge/src/log.rs) | File and console logging for Windows DLL mods |
| [Shutdown](modforge/src/shutdown.rs) | Ordered teardown through a shared handler registry |
| [Reload](modforge/src/hot_reload.rs) | Automatic UE4SS hot reload after deployment |
| [Workers](modforge/src/worker.rs) | Named worker threads with panic guards |
| [Counters](modforge/src/counters.rs) | Lock-free hot-path counts, peaks, and timings |
| [Rings](modforge/src/ring.rs) | Bounded event capture with oldest-entry eviction |
| [Hooks](modforge/src/hook.rs) | Native function detours with original-call support |
| [Vanilla](modforge/src/vanilla/mod.rs) | Arbitrary native function calls by resolved address |
| [SEH](modforge/src/seh.rs) | Structured-exception guards around foreign calls |
| [Patterns](modforge/src/patterns/mod.rs) | IDA-style byte patterns and patternsleuth resolution |
| [Scanning](modforge/src/scanner.rs) | In-process value scans, rescans, and freezes |
| [Processes](modforge/src/winproc.rs) | Windows thread, CPU, memory, and region inspection |
| [Research](modforge/src/research.rs) | Reusable binary-research recipes driven over HTTP |
| [Minidumps](modforge/src/bin/minidump.rs) | Crash exception and owning-module inspection |
| [Input](modforge/src/input/mod.rs) | Synthetic keyboard and mouse input backends |
| [Actions](modforge/src/actions.rs) | Bindings, action queues, fixed-tick stamps, and replay journals |
| [UI](modforge/src/ui/mod.rs) | Engine-independent declarative UI definitions |
| [HUD](modforge/src/hud.rs) | Player-facing panels, prompts, and inventory decisions |
| [Items](modforge/src/item.rs) | Item definitions, inventory, creation, and unique-item ledger |
| [Quality](modforge/src/quality.rs) | Quality tiers, odds, siblings, and stat jitter |
| [Crafting](modforge/src/crafting.rs) | Recipe definitions, registry, and execution |
| [Upgrades](modforge/src/upgrade.rs) | Upgrade levels, requirements, policy math, and persistence |
| [RPG](modforge/src/rpg/mod.rs) | Effects, triggers, skills, XP, state, and persistence |
| [Actors](modforge/src/actor.rs) | Actor definitions, registries, records, and personalities |
| [Decisions](modforge/src/brain.rs) | Pure AI decisions from perception and retained state |
| [Memory](modforge/src/memory.rs) | Actor knowledge, injuries, and last-seen threats |
| [Combat](modforge/src/combat.rs) | Damage, health, protection, hit resolution, firing, and weapon timing |
| [Factions](modforge/src/faction.rs) | Faction definitions, registries, and relationships |
| [Genomes](modforge/src/genome.rs) | Traits, reinforcement, heredity, voting, and persistence |
| [Survival](modforge/src/survival.rs) | Needs, rates, conditions, and settlement classification |
| [Missions](modforge/src/mission.rs) | Multi-stage, go-and-return, one-stage, and contract lifecycles |
| [Storytelling](modforge/src/storyteller.rs) | Event pacing, adaptive pressure, and active-event tracking |
| [Dread](modforge/src/unknown.rs) | Vague signs, delayed payoffs, escalation, and foreshadowing |
| [Biomes](modforge/src/biome.rs) | Biome definitions and allowed world content |
| [Structures](modforge/src/structure.rs) | Engine-independent buildings, rooms, furniture, and validation |
| [Monuments](modforge/src/monument.rs) | Deterministic monument types, arrangements, loot, and gates |
| [Worlds](modforge/src/worldgen.rs) | Deterministic terrain, biomes, monument sites, and roads |
| [Rolls](modforge/src/roll.rs) | Progress-scaled budgets and weighted selection |
| [Testkit](modforge/src/testkit/mod.rs) | Cross-game research and assertion primitives |
| [Harness](modforge/src/harness/mod.rs) | Build, launch, inject, probe, and stop Steam games |
| [Deploy](modforge/src/bin/modforge_deploy.rs) | CLI for building, installing, uninstalling, and packaging |

See [`modforge/README.md`](modforge/README.md) and
[`modforge/docs/`](modforge/docs/).

### ueforge

The Unreal Engine 5 and UE4SS binding layer.

| Capability | What it provides |
|---|---|
| [Lifecycle](ueforge/src/mod_main.rs) | `ue4ss_mod!`, UE4SS entry points, mod definitions, tabs, and shutdown orchestration |
| [Shim](ueforge/cpp/ueforge_shim.cpp) | Shared C++ UE4SS loader and ImGui bridge |
| [Features](ueforge/src/features.rs) | Registered feature installation |
| [Shutdown](ueforge/src/shutdown.rs) | UE-aware teardown through the shared registry |
| [Reload](ueforge/src/hot_reload.rs) | UE4SS generation swap and hot reload |
| [Objects](ueforge/src/ue/uobject.rs) | UObject and UClass access |
| [Functions](ueforge/src/ue/pe_call.rs) | Reflected UFunction invocation |
| [GObjects](ueforge/src/ue/core_types.rs) | Global object array layouts and traversal |
| [Names](ueforge/src/ue/fname.rs) | FName resolution and caching |
| [Strings](ueforge/src/ue/fstring.rs) | FString access and conversion |
| [Arrays](ueforge/src/ue/tarray.rs) | TArray layout and safe walking |
| [Fields](ueforge/src/ue/field.rs) | Reflected and offset-based field access |
| [Parameters](ueforge/src/parms.rs) | Typed UFunction parameter buffers |
| [Offsets](ueforge/src/ue/offsets.rs) | Platform-specific UE runtime offsets |
| [ProcessEvent](ueforge/src/hook/process_event.rs) | ProcessEvent hook installation and dispatch |
| [Vtables](ueforge/src/hook/vtable.rs) | Vtable slot inspection and patching |
| [Falls](ueforge/src/fall.rs) | Reusable fall-event hooks |
| [Damage](ueforge/src/damage/mod.rs) | Damage hooks, decoding, classification, and dispatch |
| [Frames](ueforge/src/frame.rs) | Per-frame callback registration |
| [Queue](ueforge/src/game_thread.rs) | Game-thread work scheduling |
| [DataTables](ueforge/src/data_table.rs) | DataTable definitions, schemas, and access |
| [Tweaks](ueforge/src/tweak.rs) | Typed field, stack-size, and difficulty tweaks |
| [Dynamic](ueforge/src/dynamic_tweaks.rs) | Runtime-defined tweak registry |
| [Discovery](ueforge/src/discovery.rs) | Runtime UE data discovery |
| [Assets](ueforge/src/assets.rs) | Asset registry access |
| [Uassets](ueforge/src/uasset.rs) | Unreal asset inspection |
| [RPG](ueforge/src/rpg/mod.rs) | UE implementations for Modforge RPG systems |
| [Inventory](ueforge/src/inventory/viewport.rs) | Inventory viewport paging and rebinding |
| [Statuses](ueforge/src/ue/status_effect.rs) | Status-effect lookup and application |
| [ImGui](ueforge/src/ui.rs) | Rust bindings for the shared ImGui surface |
| [Classes](ueforge/src/ui_class_browser.rs) | Runtime class browser |
| [Structs](ueforge/src/ui_struct_browser.rs) | Runtime struct browser |
| [Tables](ueforge/src/ui_data_table_browser.rs) | Runtime DataTable browser |
| [Scanner](ueforge/src/ui_scanner.rs) | Interactive memory-scanner UI |
| [TweakUI](ueforge/src/ui_tweaks.rs) | Interactive tweak controls |
| [Selectors](ueforge/src/selector.rs) | UE object selector registry |
| [Operations](ueforge/src/ops.rs) | UE operation handlers and registration |
| [Scanning](ueforge/src/scanner.rs) | UE memory scan operations |
| [Counters](ueforge/src/counters.rs) | UE runtime counter integration |
| [Snapshots](ueforge/src/debug/mod.rs) | UE process and gameplay debug snapshots |
| [HTTP](ueforge/src/server.rs) | UE control-plane server integration |
| [Client](ueforge/src/client/mod.rs) | UE runtime test client |
| [Scenarios](modforge/src/client/scenario.rs) | Shared scenario helpers for runtime tests |
| [Builds](ueforge/src/build.rs) | C++ integration, packaging, and deployment |

See [`ueforge/README.md`](ueforge/README.md) and
[`ueforge/docs/`](ueforge/docs/).

### unityforge

The Unity Mono and IL2CPP binding layer.

| Capability | What it provides |
|---|---|
| [BepInEx](unityforge/cs-shim-mono/Plugin.cs) | C# loader shim for BepInEx Mono |
| [MelonLoader](unityforge/cs-shim-melonloader/Mod.cs) | C# loader shim for MelonLoader IL2CPP |
| [Survivalist](unityforge/cs-shim-survivalist/Main.cs) | C# shim for Survivalist's built-in loader |
| [Lifecycle](unityforge/src/mod_main.rs) | Rust cdylib initialization, per-frame dispatch, shutdown, generation activation, rollback, and hot reload |
| [Mono](unityforge/src/mono.rs) | Assembly and type discovery, singleton lookup, invocation, field access, and managed handles |
| [IL2CPP](unityforge/src/il2cpp.rs) | Loader and bridge for native Unity builds |
| [Hooks](unityforge/src/hook.rs) | Harmony prefixes, postfixes, contexts, ownership, and safe removal |
| [Shim](unityforge/cs-shim-common/HarmonyBridge.cs) | Managed Harmony bridge used by Rust hooks |
| [Objects](unityforge/src/unity.rs) | GameObject and Component access |
| [Selectors](unityforge/src/selector.rs) | Unity object selector registry and resolution |
| [Input](unityforge/src/input.rs) | Unity input bridge |
| [Queue](unityforge/src/main_thread_queue.rs) | Main-thread work scheduling |
| [Effects](unityforge/src/rpg/std_effect.rs) | Unity implementations of standard Modforge RPG effects |
| [Triggers](unityforge/src/rpg/trigger_harmony.rs) | Harmony-backed RPG triggers |
| [Skills](unityforge/src/rpg/skill.rs) | Unity skill bindings |
| [Tracking](unityforge/src/rpg/tracker.rs) | Unity RPG tracker integration |
| [Identity](unityforge/src/rpg/slot_key.rs) | Unity save-slot identity resolution |
| [Operations](unityforge/src/ops.rs) | Unity operation handlers |
| [Client](unityforge/src/client.rs) | Shared Modforge HTTP client integration |

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
