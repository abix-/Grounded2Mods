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

## modforge. The foundation

Everything game-agnostic lives here: the localhost HTTP control
plane, the op registry, the selector grammar that lets ops
address running-process state by name, the RPG module
(`Effect` / `Trigger` / `Skill`), the scanner (typed memory
reads / writes / pattern scans), the winproc helpers (module
base, address-rebase, VirtualProtect), the per-line-flushed
log, the shutdown registry, and the settings file loader.

The placement rule (operator, 2026-08-19): anything generic,
anything that could apply to other games or mods, goes in
modforge. A consumer repo carries only what is specific to that
one game.

Decided 2026-08-19: the game systems proven in survivalist-mod
(storyteller, genome, acts, quality, named uniques, the work
board, upgrades, chronicle, the edge) will be lifted into this
crate as modules, like the RPG module, so one dependency serves
mods OR a standalone game. The first standalone consumer is the
topside game (private repo; its docs/authority.md carries the
ownership map). Nothing is lifted yet.

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

Open work tracked in [`docs/todo.md`](docs/todo.md).
Milestones in [`docs/changelog.md`](docs/changelog.md).

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
