# grounded2-mod

Grounded 2 RPG mod with kill-driven progression, persistent
skills, survival tweaks, and expanded inventory.

## Game

- **Game:** Grounded 2
- **Engine:** Unreal Engine 5
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Features

| Feature | Rating |
|---|---:|
| [RPG skill system](docs/rpg.md) (14 skills, kill XP, per-slot persistence) | 7/10 |
| [Damage pipeline](docs/damage.md) (damage, armor, fall, impact, lifesteal) | 6/10 |
| [Backpack expansion](docs/inventory.md) (slot count, mouse-wheel scroll) | 8/10 |
| [Gameplay settings](docs/features.md) (hunger and thirst multipliers) | 8/10 |
| ImGui RPG and engine-inspection tabs | 6/10 |
| Optional HTTP control plane on port 17171 | 9/10 |

## Build

```sh
cargo build --release -p grounded2-mod
```

Output: `target/x86_64-pc-windows-msvc/release/grounded2_mod.dll`,
deployed as `main.dll` (the name UE4SS requires in the dlls
folder). The build artifact carries the crate's own name so two
mods in this workspace cannot overwrite each other's DLL.

## Deploy

```sh
cargo deploy install -p grounded2-mod
```

Installs to
`Augusta\Binaries\WinGRTS\ue4ss\Mods\Grounded2Mod\dlls\main.dll`.

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
    rpg/
    patch.rs
    survival.rs
  tests/
    common/mod.rs
    research_probes.rs
    explore_*.rs
```
