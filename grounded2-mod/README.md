# grounded2-mod

Grounded 2 (UE 5) RPG mod. Adds a skill/leveling system with
XP from kills and crafting, persistent across saves.

## Game

- **Game:** Grounded 2
- **Engine:** Unreal Engine 5
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Build

```sh
k3sc cargo-lock build --release -p grounded2-mod
```

Output: `target/x86_64-pc-windows-msvc/release/main.dll`

## Deploy

```sh
cargo deploy install -p grounded2-mod
```

Installs to `Grounded2\Binaries\Win64\ue4ss\Mods\Grounded2Mod\dlls\main.dll`.

## Features

- RPG skill system (13 skills: attack damage, armor, health regen,
  max health, lifesteal, fall resistance, impact resistance, movement
  speed, sprint speed, backpack size, critical chance, evasion, thorns)
- XP from kills and crafting, sqrt leveling curve
- Per-slot save/load (JSON sidecar)
- HTTP control plane on port 17171
- ImGui overlay tab

## Documentation

Detailed docs live in `docs/`:

| File | Subject |
|---|---|
| [building.md](docs/building.md) | Build, install, deploy |
| [features.md](docs/features.md) | User-facing feature list |
| [rpg.md](docs/rpg.md) | RPG subsystem internals |
| [damage.md](docs/damage.md) | Damage pipeline internals |
| [inventory.md](docs/inventory.md) | Backpack patch internals |
| [testing.md](docs/testing.md) | Test setup |

## File layout

```
grounded2-mod/
  Cargo.toml
  README.md
  docs/
  src/
    lib.rs
    skills.rs
    effects.rs
  tests/
    common/mod.rs
    research_probes.rs
    explore_*.rs
```
