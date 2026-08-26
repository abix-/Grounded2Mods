# Building Grounded 2 - RPG System

> **Authoritative on:** how to build, install, and run the mod
> as a developer. Prerequisites, cargo commands, deploy script
> usage, configuration knobs.

This crate is the Grounded 2 mod itself: Rust cdylib (`main.dll`)
that UE4SS loads as a CPPMod. The C++ shim that satisfies UE4SS's
`RC::CppUserModBase` ABI is compiled in by `ueforge`'s `build.rs`
helper; see [`../../ueforge/docs/lifecycle.md`](../../ueforge/docs/lifecycle.md).

The framework crate (`ueforge`) lives elsewhere in the workspace
and supplies every piece of generic infrastructure. Lifecycle,
UE SDK, hooks, HTTP control plane, ImGui bindings, RPG framework,
build/deploy. This crate ships only the Grounded-2-specific
content (offsets, skill effect variants, catalog rows, hooks,
parm structs).

## One mod, all testing inside it

Project rule: there is exactly one mod -- `Grounded2Mod`. And
every diagnostic, probe, trace, and feature lives inside it. Do not
drop side-channel Lua probe mods or separate debug DLLs into the
game install. Everything routes through our Rust code, gated behind
skill levels, cargo features, or `cfg!(debug_assertions)` so the
user gets one log file (`<DLL_dir>/grounded2_mod.log`) and one
place to read what is happening. If you need a transient probe to
diagnose something, gate it behind a skill flag (e.g. unlock the
probe by leveling the relevant skill) or behind a debug build, then
strip it once the diagnosis is done.

## Prerequisites

- Windows 10/11 x64.
- Rust toolchain. The repo pins `stable` via `rust-toolchain.toml` and
  installs the `x86_64-pc-windows-msvc` target automatically. Install
  `rustup` from <https://rustup.rs> if you don't have it.
- Visual Studio Build Tools 2022+ with the C++ workload. Required for
  the MSVC linker and for compiling the C++ shim against UE4SS's
  ABI.
- A clone of UE4SS at `C:\code\RE-UE4SS` (only the headers are used;
  no UE4SS build needed):
  ```
  git clone https://github.com/UE4SS-RE/RE-UE4SS.git C:\code\RE-UE4SS
  ```
  Don't try `git submodule update --init --recursive`, UE4SS's own
  `.gitmodules` references a 404'd `Re-UE4SS/UEPseudo.git` repo.
  We only need the headers under `UE4SS/include/` and the user's
  installed UE4SS.dll for the import library, not a UE4SS build.
- The user's installed UE4SS for Grounded 2. The repo ships a
  pre-generated `grounded2-mod/ue4ss/UE4SS.lib` (1.9 MB,
  regenerable from any installed UE4SS.dll via the steps in the
  port plan).

## Build

```
cargo build --release
```

Output:

- `target/grounded2-mod/x86_64-pc-windows-msvc/release/grounded2_mod.dll`,
  the mod. It is DEPLOYED as `main.dll` because UE4SS expects
  CPPMods at `Mods/<ModName>/dlls/main.dll`, but the build
  artifact carries the crate's own name so two cdylibs in this
  workspace cannot overwrite each other. Naming them all `main`
  put this mod inside MISERY once; see ueforge/docs/lifecycle.md.
  (Per-package `target_dir` in `[package.metadata.ueforge]` gives
  the crate its own build tree, but only applies to
  `cargo deploy`, not to a plain `cargo build -p`.)

The build target dir is locked via `.cargo/config.toml`.

## Package for Vortex / Nexus distribution

```
.\scripts\deploy.ps1
```

Default mode. Builds the release cdylib and writes
`dist\grounded2-mod-v<version>.zip` ready to upload to Nexus or
import into Vortex. The zip mirrors the UE4SS deployment path so it
extracts directly into the game install:

```
Augusta/Binaries/WinGRTS/ue4ss/Mods/Grounded2Mod/
  dlls/main.dll
  dlls/settings.json
  README.txt
```

`README.txt` inside the archive walks the user through Vortex /
manual install, the UE4SS prerequisite, and the `mods.txt`
registration step.

`dist/` is gitignored.

## Direct local install (skip Vortex / Nexus)

```
.\scripts\deploy.ps1 -Install
```

Builds, auto-detects the local Grounded 2 install (Steam library
registry), verifies UE4SS is installed at
`<game>\Augusta\Binaries\WinGRTS\ue4ss\UE4SS.dll`, copies `main.dll`
and `settings.json` into
`<game>\Augusta\Binaries\WinGRTS\ue4ss\Mods\Grounded2Mod\`, seeds
`dlls\settings.json`, and appends `Grounded2Mod : 1` to `mods.txt`
if it isn't there.
Existing user-edited `settings.json` is preserved.

Override the auto-detect with `-GamePath <path-to-game-root>`.

Uninstall:

```
.\scripts\deploy.ps1 -Uninstall
```

Deletes the `Grounded2Mod` mod folder and strips its line from
`mods.txt`.

## Hot-update while the game is running

```
1. edit Rust
2. cargo deploy install -p grounded2-mod
3. alt-tab to the game, press Ctrl+R
4. new build is live in ~1-2s with state preserved
```

The full mechanism (Phase A research + Phase B implementation,
the FreeLibrary / fresh LoadLibraryExW round-trip, the side-file
swap during `on_shutdown`, the hook teardown + trampoline drain,
the HTTP listener join, the UE4SS-settings.ini gating) is
documented in
[`../../ueforge/docs/lifecycle.md`](../../ueforge/docs/lifecycle.md)
"Hot-reload". Read that for the full framework story.

g2rpg-specific touchpoints:

- `MOD_INFO.on_shutdown` calls `rpg::world_loader::shutdown()`
  to stop the slot poller. ueforge's shutdown sequence then
  takes over (hook teardown -> HTTP listener join -> side-file
  swap).
- Save state lives in `<DLL_dir>/saves/<playthrough-guid>.json`
 . Survives the reload, re-read by the new image on slot
  activation. See [`rpg.md`](rpg.md) "Persistence".
- `settings.json` is hot-watched via `Settings::watch` (if
  enabled). Changes take effect without Ctrl+R; only code
  changes need the reload.

## Quality gates

The same gates the maintainers run before pushing:

```
cargo build --release
cargo clippy --release --all-targets, -D warnings
cargo test --release
```

All three must pass cleanly.

## Configuration

Runtime config lives in `Mods\Grounded2Mod\dlls\settings.json`.
Defaults are baked
in, so the file is optional. Schema:

```json
{
  "inventory": {
    "slot_count": 100
  },
  "survival": {
    "thirst_multiplier": 0.5,
    "hunger_multiplier": 0.5
  }
}
```

- `slot_count`, target main-backpack capacity. Mount/saddlebag
  (vanilla 30) is always preserved.
- `thirst_multiplier`, `hunger_multiplier`, scale the per-second
  drain rate. `1.0` = vanilla, `0.5` = half rate, `0.0` = no drain.

Defaults (used when `settings.json` is missing or a key is absent):

- `slot_count = 100`
- `thirst_multiplier = 0.5`
- `hunger_multiplier = 0.5`

Copy `grounded2-mod/settings.example.json` next to `main.dll` as
`settings.json` if you want to override these.

Compile-time constants:

- `patch.rs`, `DEFAULT_SLOT_COUNT = 100` (fallback if settings load
  fails), `VANILLA_MAIN = 40`, `VANILLA_MOUNT = 30` (skip).
- `inv_hook.rs`, viewport size (4 rows x 10 cols), scroll step (one row).
- `survival.rs`, offsets for `USurvivalComponent` hunger/thirst
  fields.

## Cargo features

- `console` (default on), the DLL spawns a "Grounded 2 - RPG System" console
  window via `AllocConsole` for live log output. Useful for development.
  Build a console-free shipping DLL with:
  ```
  cargo build --release --no-default-features
  ```
  The file log at `<DLL_dir>\grounded2_mod.log` is unaffected.

Trace logging in `inv_hook.rs` is gated by `cfg!(debug_assertions)`. To
enable it, build without `--release`:

```
cargo build
```

Debug builds are larger and slower, but you'll see per-event scroll
diagnostics in the live console.

## Targeting a different game build

Engine offsets live in `grounded2-mod/src/sdk/offsets.rs`, two named
constant blocks (`STEAM` and `XBOX`). They were captured from a Dumper-7
run against build 0.4.0.2. If a future game patch shifts offsets, dump
the new SDK and edit those blocks.

The platform is selected at runtime by matching the host process exe
name. Other builds will fall through to the `Steam` defaults with a
warning.

