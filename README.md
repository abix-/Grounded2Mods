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
| Control plane | [Local HTTP server](modforge/src/server.rs), [request envelope](modforge/src/envelope.rs), [operation registry](modforge/src/ops.rs), [typed arguments](modforge/src/args.rs), [snapshots](modforge/src/snapshots/mod.rs), [debug views](modforge/src/debug.rs), and [HTTP client](modforge/src/client/mod.rs) |
| Lifecycle and state | [Settings](modforge/src/settings.rs), [logging](modforge/src/log.rs), [shutdown ordering](modforge/src/shutdown.rs), [hot-reload protocol](modforge/src/hot_reload.rs), [counters](modforge/src/counters.rs), [bounded rings](modforge/src/ring.rs), and [workers](modforge/src/worker.rs) |
| Runtime tooling | [Memory access](modforge/src/memory.rs), [scanners](modforge/src/scanner.rs), [pattern sleuthing](modforge/src/patterns/mod.rs), [safe native calls](modforge/src/vanilla/mod.rs), [function hooks](modforge/src/hook.rs), [structured-exception guards](modforge/src/seh.rs), [Windows process inspection](modforge/src/winproc.rs), and [research helpers](modforge/src/research.rs) |
| Input and UI | [Native input and coordinate handling](modforge/src/input/mod.rs), [actions, bindings, queues, and replay journals](modforge/src/actions.rs), [declarative UI](modforge/src/ui/mod.rs), and [player HUD state](modforge/src/hud.rs) |
| Items and progression | [Item definitions, inventory, and unique-item ledger](modforge/src/item.rs), [quality](modforge/src/quality.rs), [weighted rolls](modforge/src/roll.rs), [crafting](modforge/src/crafting.rs), [upgrades](modforge/src/upgrade.rs), and the full [RPG effect, trigger, and skill stack](modforge/src/rpg/mod.rs) |
| Simulation systems | [Actors](modforge/src/actor.rs), [decisions](modforge/src/brain.rs), [combat](modforge/src/combat.rs), [factions](modforge/src/faction.rs), [genomes](modforge/src/genome.rs), [survival classification](modforge/src/survival.rs), [missions and contracts](modforge/src/mission.rs), [storyteller pacing and adaptive pressure](modforge/src/storyteller.rs), and the [unknown dread loop](modforge/src/unknown.rs) |
| World building | [Biomes](modforge/src/biome.rs), [structures](modforge/src/structure.rs), [monuments](modforge/src/monument.rs), and [deterministic world generation](modforge/src/worldgen.rs) |
| Verification and delivery | [Shared testkit](modforge/src/testkit/mod.rs), [runtime harness and build helpers](modforge/src/harness/mod.rs), and [deploy CLI](modforge/src/bin/modforge_deploy.rs) |

See [`modforge/README.md`](modforge/README.md) and
[`modforge/docs/`](modforge/docs/).

### ueforge

The Unreal Engine 5 and UE4SS binding layer.

| Capability | What it provides |
|---|---|
| Mod lifecycle | [`ue4ss_mod!` and UE4SS entry points](ueforge/src/mod_main.rs), [shared C++ shim](ueforge/cpp/ueforge_shim.cpp), [feature installation](ueforge/src/features.rs), [shutdown](ueforge/src/shutdown.rs), and [hot reload](ueforge/src/hot_reload.rs) |
| UE runtime | [UObject and UClass](ueforge/src/ue/uobject.rs), [UFunction](ueforge/src/ue/pe_call.rs), [GObjects](ueforge/src/ue/core_types.rs), [FName](ueforge/src/ue/fname.rs), [FString](ueforge/src/ue/fstring.rs), [TArray](ueforge/src/ue/tarray.rs), [reflected fields](ueforge/src/ue/field.rs), [parameters](ueforge/src/parms.rs), and [platform offsets](ueforge/src/ue/offsets.rs) |
| Hooks and game thread | [ProcessEvent hooks](ueforge/src/hook/process_event.rs), [vtable hooks](ueforge/src/hook/vtable.rs), [fall hooks](ueforge/src/fall.rs), [damage hooks](ueforge/src/damage/mod.rs), [frame callbacks](ueforge/src/frame.rs), and the [game-thread queue](ueforge/src/game_thread.rs) |
| Data and assets | [DataTable access](ueforge/src/data_table.rs), [typed field tweaks](ueforge/src/tweak.rs), [dynamic tweaks](ueforge/src/dynamic_tweaks.rs), [discovery](ueforge/src/discovery.rs), [asset registry access](ueforge/src/assets.rs), and [uasset inspection](ueforge/src/uasset.rs) |
| Gameplay modules | [RPG](ueforge/src/rpg/mod.rs), [stack sizes and difficulty knobs](ueforge/src/tweak.rs), [inventory viewport paging](ueforge/src/inventory/viewport.rs), [status effects](ueforge/src/ue/status_effect.rs), and [damage dispatch](ueforge/src/damage/mod.rs) |
| Operator UI | [ImGui bindings](ueforge/src/ui.rs) plus [class](ueforge/src/ui_class_browser.rs), [struct](ueforge/src/ui_struct_browser.rs), [DataTable](ueforge/src/ui_data_table_browser.rs), [scanner](ueforge/src/ui_scanner.rs), and [tweak](ueforge/src/ui_tweaks.rs) browsers |
| Control and inspection | [UE selectors](ueforge/src/selector.rs), [standard operations](ueforge/src/ops.rs), [memory tools](ueforge/src/scanner.rs), [counters](ueforge/src/counters.rs), [snapshots](ueforge/src/debug/mod.rs), and the [shared HTTP surface](ueforge/src/server.rs) |
| Testing and builds | [Runtime client](ueforge/src/client/mod.rs), [scenario helpers](modforge/src/client/scenario.rs), [C++ build integration](ueforge/src/build.rs), and [packaging and deployment commands](ueforge/src/build.rs) |

See [`ueforge/README.md`](ueforge/README.md) and
[`ueforge/docs/`](ueforge/docs/).

### unityforge

The Unity Mono and IL2CPP binding layer.

| Capability | What it provides |
|---|---|
| Managed loaders | C# shims for [BepInEx Mono](unityforge/cs-shim-mono/Plugin.cs), [MelonLoader IL2CPP](unityforge/cs-shim-melonloader/Mod.cs), and [Survivalist's built-in loader](unityforge/cs-shim-survivalist/Main.cs) |
| Rust lifecycle | [Rust cdylib initialization, per-frame dispatch, shutdown, generation activation, rollback, and generation-versioned hot reload](unityforge/src/mod_main.rs) |
| Mono bridge | [Assembly and type discovery, singleton lookup, method invocation, field reads and writes, and managed handle ownership](unityforge/src/mono.rs) |
| IL2CPP bridge | [IL2CPP loader and bridge surface](unityforge/src/il2cpp.rs) for native Unity builds |
| Harmony hooks | [Prefix and postfix registration, hook contexts, registry ownership, and safe removal](unityforge/src/hook.rs) through the [managed shim](unityforge/cs-shim-common/HarmonyBridge.cs) |
| Unity runtime | [GameObject and Component access](unityforge/src/unity.rs), [Unity selectors](unityforge/src/selector.rs), [input bridge](unityforge/src/input.rs), and [main-thread work queue](unityforge/src/main_thread_queue.rs) |
| Gameplay bindings | Unity implementations for Modforge RPG [effects](unityforge/src/rpg/std_effect.rs), [triggers](unityforge/src/rpg/trigger_harmony.rs), [skills](unityforge/src/rpg/skill.rs), [tracking](unityforge/src/rpg/tracker.rs), and [slot identity](unityforge/src/rpg/slot_key.rs) |
| Control and testing | Unity [operation handlers](unityforge/src/ops.rs) and [selectors](unityforge/src/selector.rs) with the shared [Modforge HTTP client](unityforge/src/client.rs) |

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
