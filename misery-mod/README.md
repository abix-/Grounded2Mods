# misery-mod

MISERY (UE 5.4 survival horror) runtime mod. Rust cdylib loaded
by UE4SS as `main.dll`.

## Game

- **Game:** MISERY
- **Engine:** Unreal Engine 5.4
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Features

| Feature | Description | Rating |
|---|---|---:|
| [Shining](src/shining.rs) | Inspects, freezes, sets, and extends the emission countdown. | 5/10 |
| [Gameplay](src/gameplay.rs) | Edits live survival and world settings. | 4/10 |
| [Speed](src/speed.rs) | Applies a live movement multiplier through the UI. | 3/10 |
| [Vendors](src/vendors.rs) | Expands buy and sell lists on each load. | 4/10 |
| [Notice](src/nag.rs) | Dismisses the playtest notice at the main menu. | 4/10 |
| [Spawning](src/spawning.rs) | Scales NPC threat budgets with past emissions. | 4/10 |
| [Phenomena](src/strange.rs) | Places anomalies, camps, caches, lights, and hazards. | 3/10 |
| [Assets](src/assets.rs) | Queries and streams shipped assets. | 4/10 |
| [Pieces](../ueforge/src/ue/pieces.rs) | Captures and rebuilds Blueprint props and static meshes. | 6/10 |
| [Monuments](src/places.rs) | Arranges captured structures into generated places. | 4/10 |
| [Rooms](src/rooms.rs) | Builds rooms from the game's construction kit; disabled at startup. | 6/10 |
| [World generation](tests/research_worldgen.rs) | Regenerates areas and mixes square pools through research operations. | 5/10 |
| [Stacks](src/lib.rs) | Multiplies item stacks; disabled at startup. | 2/10 |
| [Auto-load](src/autoload.rs) | Loads a save through an incorrect host path; disabled. | 1/10 |
| [Control plane](src/debug.rs) | HTTP inspection and control on port 17176. | 9/10 |

Startup status and verification gaps are tracked in
[the open issues](docs/todo.md).

## Build

```sh
cargo build --release -p misery-mod
```

Output: `target/x86_64-pc-windows-msvc/release/misery_mod.dll`,
deployed as `main.dll` (the name UE4SS requires in the dlls
folder). The build artifact carries the crate's own name so two
mods in this workspace cannot overwrite each other's DLL.

## Deploy

```sh
cargo deploy install -p misery-mod
```

Installs to `MISERY\Binaries\Win64\ue4ss\Mods\MiseryMod\dlls\main.dll`.

## Documentation

- [Research](docs/research.md)
- [Open issues](docs/todo.md)
- [Performance](docs/performance.md)
- [Pieces](docs/pieces.md)
- [World generation](docs/worldgen.md)
- [RPG direction](docs/rpg.md)

## File layout

```
misery-mod/
  Cargo.toml
  README.md
  docs/
    research.md
    todo.md
  scripts/
    restart.ps1
  src/
    lib.rs
  tests/
    common/mod.rs          # misery-specific Api + helpers
    research_*.rs          # live-game research probes
    set_*.rs               # live-game write tests
    freeze_timer.rs
```
