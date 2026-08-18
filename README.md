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
   │ grounded2-mod │    │   wwm-mod     │    │  horsey-mod  │
   │ outworld-     │    │ il2cpp-smoke  │    │ (Horsey Game,│
   │ station-mod   │    │               │    │  PE inject)  │
   │               │    │               │    │              │
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

## modforge. The foundation

Everything game-agnostic lives here: the localhost HTTP control
plane, the op registry, the selector grammar that lets ops
address running-process state by name, the RPG module
(`Effect` / `Trigger` / `Skill`), the scanner (typed memory
reads / writes / pattern scans), the winproc helpers (module
base, address-rebase, VirtualProtect), the per-line-flushed
log, the shutdown registry, and the settings file loader.

## The two engine forges

An engine forge is a thin crate that binds modforge into one
host *engine* runtime and contributes engine-specific
machinery only. Any game built on that engine sits on top.

### ueforge. UE5 / UE4SS

The first forge. Owns the UE SDK shim (UObject / UClass /
UFunction / GObjects / TypedField), the `ue4ss_mod!` macro,
the C++ shim, ProcessEvent vtable hooks with per-hook drain
on shutdown, ImGui bindings, hot-reload (Phase A + B), and
five opinionated mod-shape modules:

| Module     | What you write per-game                             |
|------------|-----------------------------------------------------|
| RPG        | A catalog of `Skill<E>` rows. 9 of 10 universal shapes covered by `StandardEffect`. |
| Stacks     | `StackTweak::new(table, offset, default_mult, skip)` |
| Difficulty | `DifficultyKnob::new(class, offset)` per knob       |
| Inventory  | `ViewportBinder` trait impl                         |
| Damage     | `DamageBinder` trait impl                           |

Test framework: `ueforge::client::{research, diff, scenario}`
collapses test boilerplate to Pester-style one-liners.

See [`ueforge/README.md`](ueforge/README.md).

### unityforge. Unity (Mono + IL2CPP)

Binds modforge into Unity games via a BepInEx-loaded
`Unityforge.Shim.{Mono,Il2Cpp}.dll` C# shim that LoadLibrarys
the per-game Rust cdylib and dispatches per-frame Update +
generation-versioned hot reload. Ships a Mono bridge
(reflection over loaded assemblies, Harmony patching,
GameObject / Component / field access), an IL2CPP bridge, and
a Unity-side Input bridge. Generation-versioned hot reload
(never `FreeLibrary`; each reload is a `LoadLibrary` of a
freshly-named gen file) avoids the FreeLibrary crash class.

See [`unityforge/`](unityforge/) and [`docs/unityforge-plan.md`](docs/unityforge-plan.md).

## Game-side mods

| Crate                     | Game              | Bound via    | What it does                                                                 |
|---------------------------|-------------------|--------------|------------------------------------------------------------------------------|
| `grounded2-mod`           | Grounded 2        | ueforge      | Factorio-style RPG / level-up. 13 skills, target ~25.                        |
| `outworld-station-mod`    | Outworld Station  | ueforge      | Stack-size tweak (DT_Materials.MaxCanStack multiplier). Validates ueforge on a second UE5 game. |
| `wwm-mod`                 | Wild West Miner   | unityforge   | RPG / level-up + demo-end block (TutorialManager.CompleteDemo prefix). Mono. |
| `il2cpp-smoke`            | (smoke target)    | unityforge   | End-to-end test of the IL2CPP path before shipping a real IL2CPP game mod.   |
| `horsey-mod`             | Horsey Game       | modforge (PE inject) | Cheats + research surface. Sleep-safe fatigue suppressor, money/year/horse ops, debug-mode unlock. Carries its own injector EXE. |

`horsey-mod` is the odd one out: Horsey Game has no
third-party plugin loader, so the crate consumes modforge
directly and ships a small injector EXE that
`CreateRemoteThread`s a `LoadLibraryW` into the running
process. It also owns the binary-patch infrastructure
(VirtualProtect / write / FlushInstructionCache /
revert-on-detach) and hot reload via timestamped staged DLLs
(cargo's output is never the file the game has loaded, so
it's never locked). All Horsey-specific. See
[`horsey-mod/README.md`](horsey-mod/README.md) and
[`horsey-mod/research/`](horsey-mod/research/) for the Horsey Game research.

Per-game research notes:

- [`grounded2-mod/docs/`](grounded2-mod/docs/). Damage
  internals (combat / fall / environmental), inventory,
  catalog, persistence.
- [`wwm-mod/docs/research.md`](wwm-mod/docs/research.md).
  Wild West Miner managed-side research log.
- [`horsey-mod/research/`](horsey-mod/research/). Horsey Game decompilation
  + function annotations + RE notes (subtree-merged from the
  former `horsey-mods` repo).

## Research tooling

### decomp. Binary-to-Rust output backend (exploratory archive)

> **2026-05-14 honest assessment**: this exists but probably
> shouldn't be a daily driver. Read
> [`decomp/docs/retrospective.md`](decomp/docs/retrospective.md)
> before investing further. Ghidra's existing C decomp at
> [`horsey-mod/research/decompiled/all_functions.c`](horsey-mod/research/decompiled/all_functions.c)
> covers the actual RE workflow. `decomp` adds Rust syntax
> as a cosmetic-but-not-pivotal win, at the cost of WSL-only
> builds, 0.15% coverage relative to Ghidra, and a naming
> layer that depends on Ghidra anyway.

Built on [r2sleigh](https://github.com/radareorg/r2sleigh) (the
radare2 org's SLEIGH-based pure-Rust decompiler stack with
full x86-64, SSA pipeline, real structurer, type
inference). Walks r2sleigh's public `r2dec::ast::CFunction`
and emits `unsafe fn`-shaped Rust pseudocode for human
reading.

CLI: `decomp print --addr 0xADDR` / `batch` / `dump-il`.
Names recovered from Ghidra's INDEX.md plus key-funcs/
filename slugs. Sample output:

```rust
pub unsafe fn price_or_score_formula(arg1: i64, arg2: i64) {
    rbx = fn_1400285e0();
    fn_1400ca670();
    ecx = *(fn_1400285e0() + 596_i64);
    *(fn_1400285e0() + 596_i64) = *(rcx + 596_i64) + 1_i64;
    ...
}
```

Sample artifacts shipped at
[`horsey-mod/research/decompiled/rust-r2sleigh/`](horsey-mod/research/decompiled/rust-r2sleigh/):
13 of 18 documented Horsey key-funcs with friendly names
recovered.

**Build:** WSL only (libsla-sys' Ghidra C++ source needs
Windows MSVC compat work; see
[`decomp/docs/polish-ladder.md`](decomp/docs/polish-ladder.md)
item 1). Decomp is intentionally NOT a cargo workspace
member yet for the same reason. Clone r2sleigh as a
sibling, `cargo build --release` in WSL.

See [`decomp/README.md`](decomp/README.md) for the
one-page intro and [`decomp/docs/`](decomp/docs/) for the
ladder.

#### History: falcon-printer (retired 2026-05-14)

`decomp/` replaces [`falcon-printer/`](falcon-printer/),
the prototype that taught us what passes we needed. The
retired crate's docs are preserved at
[`falcon-printer/docs/`](falcon-printer/docs/) (strategy
migration plan, ecosystem survey, middle-end passes
walkthrough, architecture). The Cargo.toml + src/ are
deleted; git history is the archive.

## Repository layout

```
.
+- modforge/                  -- the foundation crate
+- ueforge/                   -- UE5 / UE4SS forge (60+ submodules, 5 modules)
+- unityforge/                -- Unity Mono + IL2CPP forge (with C# shim)
+- grounded2-mod/             -- Grounded 2 RPG / level-up mod (ueforge)
+- outworld-station-mod/      -- Outworld Station mod (ueforge)
+- wwm-mod/                   -- Wild West Miner mod (unityforge / Mono)
+- il2cpp-smoke/              -- IL2CPP smoke target (unityforge / IL2CPP)
+- horsey-mod/                -- Horsey Game mod (PE inject; modforge directly)
+- horsey-mod/research/                -- Horsey Game research (decomp, RE notes, plans)
+- decomp/                    -- Binary-to-Rust decompiler (r2sleigh-based; WSL-only)
+- falcon-printer/            -- prototype that decomp replaces; archive (docs only)
+- docs/                      -- workspace-level (todo, changelog, research)
+- Cargo.toml                 -- workspace manifest
+- README.md                  -- this file
```

Per-crate docs live in each crate's `docs/` folder.

## Install (per game)

Each game-side mod has its own deploy command:

```sh
# Grounded 2 (UE5 / UE4SS)
cargo deploy install -p grounded2-mod

# Outworld Station (UE5 / UE4SS)
cargo deploy install -p outworld-station-mod

# Wild West Miner (Unity Mono / BepInEx)
wwm-mod/scripts/build_and_deploy.ps1

# Horsey Game (native PE / inject)
cargo build -p horsey-mod --release
target/x86_64-pc-windows-msvc/release/horsey-inject.exe \
  --dll target/x86_64-pc-windows-msvc/release/horsey.dll
```

`cargo deploy install -p <mod>` (for the ueforge mods)
auto-detects the Steam install via the
`[package.metadata.ueforge]` config in the crate's
`Cargo.toml`, copies the DLL + a default `settings.json` into
UE4SS's `Mods/` directory, and registers the mod in
`mods.txt`.

For Vortex packaging: `cargo deploy package -p <mod>` produces
a Vortex-installable zip in `dist/`.

### Hot reload

Each forge has its own hot-reload story:

- **ueforge**: Ctrl+R inside UE4SS. The framework swaps a
  side-file (`main-new.dll`) into place during `on_shutdown`;
  UE4SS reloads the new image. Hook teardown + thread joins +
  vtable restoration are framework-side. State on disk
  survives.
- **unityforge**: Generation-versioned. Run
  `wwm-mod/scripts/build_and_deploy.ps1 -Hot` to stage a new
  `*.unityforge.gen<N>.dll`. The C# shim's per-second watcher
  picks it up and `HotSwap`s. Old generation is quiesced (its
  code stays mapped); never `FreeLibrary`.
- **horsey-mod**: `horsey-inject.exe --reload`. POSTs
  `_shutdown` to release the listener, `CreateRemoteThread`s
  `FreeLibrary` on the old HMODULE, deletes the old staged
  file, stages the new build to a fresh timestamped path,
  loads it.

## Status (2026-05-14)

- **modforge**: foundation crate stable. Server with
  `SO_REUSEADDR` (hot-reload swaps bind cleanly), op registry,
  selector grammar, RPG primitives, scanner, shutdown
  registry, settings.
- **ueforge**: five framework modules shipped (rpg / stacks /
  difficulty / inventory / damage), hot-reload (Phase A + B)
  complete, Pester-style test DSL complete.
- **unityforge**: generation-versioned hot reload built and
  deployed (Phase 4). Mono + IL2CPP shims.
  `HarmonyBridge.PatchPrefix/Postfix` known-broken (queued for
  fix); Mono `MonoBridge.ListMethods` shipped (+ ABI v4).
- **horsey-mod** (Horsey Game mod, PE inject): injector +
  HTTP control plane shipped, hot reload works,
  `no_tire`-by-default replaced by split-flag fatigue
  suppressor (race-eligible without breaking sleep).
  Binary-patch infra landed; first patch
  (`sleep_safe_no_tire`) wip (pattern-scan disambiguator
  unsolved).
- **grounded2-mod**: 13 skills live including Lifesteal in the
  damage hook. Tested against Grounded 2 Steam build
  `++Augusta+release-0.4.0.2-CL-2673661`.
- **outworld-station-mod**: stacks tweak shipped.
- **wwm-mod**: demo-end block shipped (Harmony prefix on
  `TutorialManager.CompleteDemo`); RPG side parked while the
  Harmony bridge is repaired.

Open work tracked in [`docs/todo.md`](docs/todo.md), ordered
by leverage. Chronology of milestones in
[`docs/changelog.md`](docs/changelog.md).

## Build prerequisites

- Windows 10/11 x64
- Rust toolchain (rustup; stable pinned via `rust-toolchain.toml`)
- Visual Studio Build Tools 2022+ with the C++ workload
- For ueforge mods: the target game's UE4SS install
- For unityforge mods: the target game's BepInEx install
- For framework dev: clone with `--recurse-submodules`. Dear
  ImGui v1.92.1 lives in a submodule.

## Docs

Workspace-level (open work + chronology) lives at the root:

- [`docs/README.md`](docs/README.md). Workspace docs index
- [`docs/todo.md`](docs/todo.md). Open work across all crates
- [`docs/changelog.md`](docs/changelog.md). Milestones, newest first
- [`docs/unityforge-plan.md`](docs/unityforge-plan.md). The
  unityforge architecture plan (phases 1-4)
- [`wwm-mod/docs/research.md`](wwm-mod/docs/research.md).
  Wild West Miner managed-side research log

Per-crate docs:

- [`ueforge/README.md`](ueforge/README.md) + [`ueforge/docs/`](ueforge/docs/).
- [`unityforge/`](unityforge/).
- [`horsey-mod/README.md`](horsey-mod/README.md).
- [`grounded2-mod/docs/`](grounded2-mod/docs/).
- [`decomp/README.md`](decomp/README.md) + [`decomp/docs/`](decomp/docs/). Active binary-to-Rust RE tool (r2sleigh-based).
- [`falcon-printer/`](falcon-printer/). Retired prototype that decomp replaces; docs preserved as historical archive.
- [`horsey-mod/research/`](horsey-mod/research/). Horsey Game research notes.

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
- The game studios whose titles we mod (Obsidian Entertainment
  for [Grounded 2](https://grounded2.obsidian.net/), the
  Outworld Station team, the Wild West Miner team, and the
  Horsey Game team). We modify only what the official games
  ship under fair-use modding norms; no game assets are
  redistributed.
