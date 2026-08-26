# misery-mod

MISERY (UE 5.4 survival horror) runtime mod. Rust cdylib loaded
by UE4SS as `main.dll`.

## Game

- **Game:** MISERY
- **Engine:** Unreal Engine 5.4
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Features

| Feature | Rating |
|---|---:|
| [Shining timer control](docs/research.md) (inspect, freeze, set, and extend the countdown) | 5/10 |
| [Gameplay settings editor](docs/research.md) (live survival and world settings) | 4/10 |
| [Movement speed control](docs/research.md) (live multiplier and ImGui tab) | 3/10 |
| [Vendor inventory expansion](docs/research.md) (buy and sell lists reapplied per load) | 4/10 |
| [Playtest notice removal](docs/research.md) (automatic main-menu dismissal) | 4/10 |
| [Scaling NPC spawns](docs/research.md) (threat budgets rise with past emissions) | 4/10 |
| [Alternate-reality phenomena](docs/worldgen.md) (anomalies, camps, caches, lights, and hazards) | 3/10 |
| [Asset inventory and loading](docs/pieces.md) (query and stream shipped assets) | 4/10 |
| [Level piece capture and rebuilding](docs/worldgen.md) (Blueprint props and static meshes) | 6/10 |
| [Generated monuments](docs/worldgen.md) (captured structures arranged into new places) | 4/10 |
| [Procedural rooms](docs/worldgen.md) (walls, doors, windows, floors, and ceilings from the game kit; currently disabled at startup) | 6/10 |
| [World regeneration and square-pool mixing](docs/worldgen.md) | 5/10 |
| Item stack multiplication (implemented, currently disabled at startup) | 2/10 |
| Save auto-load (implemented against the wrong host path, currently disabled) | 1/10 |
| HTTP control plane on port 17176 | 9/10 |

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
