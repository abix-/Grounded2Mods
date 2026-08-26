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
| [Shining timer control](docs/research.md) (freeze and countdown) | 5/10 |
| [Vendor modification](docs/research.md) (add items to any vendor's buy/sell list) | 4/10 |
| [Gameplay settings](docs/research.md) (hunger, thirst, damage, spawn rate) | 4/10 |
| Movement speed modification | 3/10 |
| HTTP control plane on port 17176 | 9/10 |

## Build

```sh
k3sc cargo-lock build --release -p misery-mod
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

## Research

Research tests live in `tests/` and run against the live game.
Findings are documented in `docs/research.md`. Set
`MISERY_DEBUG_PORT=17176` and run with `--test-threads=1 --nocapture`.

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
