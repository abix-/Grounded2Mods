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
| [Shining timer control](docs/research.md) (inspection and live control) | 5/10 |
| [Vendor modification](docs/research.md) (per-load inventory changes) | 4/10 |
| [Gameplay research](docs/research.md) (dispatch, spawning, phenomena, assets) | 4/10 |
| Movement speed modification | 3/10 |
| HTTP control plane on port 17176 | 9/10 |

Auto-load, generated rooms, random placement, and 10x stacks are
currently disabled. See [the open issues](docs/todo.md) for their
verification gates.

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
