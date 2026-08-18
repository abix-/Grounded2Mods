# misery-mod

MISERY (UE 5.4 survival horror) runtime mod. Rust cdylib loaded
by UE4SS as `main.dll`.

## Game

- **Game:** MISERY
- **Engine:** Unreal Engine 5.4
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Build

```sh
k3sc cargo-lock build --release -p misery-mod
```

Output: `target/x86_64-pc-windows-msvc/release/main.dll`

## Deploy

```sh
cargo deploy install -p misery-mod
```

Installs to `MISERY\Binaries\Win64\ue4ss\Mods\MiseryMod\dlls\main.dll`.

## Features

- Shining (emission) timer freeze and countdown control
- Vendor buy/sell list modification (add items to any vendor)
- Gameplay settings read/write (hunger, thirst, damage, spawn rate)
- Movement speed research and modification
- HTTP control plane on port 17176

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
