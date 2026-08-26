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
| [Backpack skill](docs/inventory.md) (expanded capacity and mouse-wheel scrolling) | 8/10 |
| [Hunger skill](docs/rpg.md) (reduced hunger drain) | 8/10 |
| [Thirst skill](docs/rpg.md) (reduced thirst drain) | 8/10 |
| [Attack Damage skill](docs/rpg.md) | 7/10 |
| [Armor skill](docs/damage.md) | 7/10 |
| [Move Speed skill](docs/rpg.md) | 7/10 |
| [Jump Height skill](docs/rpg.md) | 7/10 |
| [Leap Distance skill](docs/rpg.md) | 6/10 |
| [Glide Speed skill](docs/rpg.md) | 6/10 |
| [Fall Resistance skill](docs/damage.md) | 6/10 |
| [Impact Resistance skill](docs/damage.md) | 4/10 |
| [Lifesteal skill](docs/damage.md) | 4/10 |
| [Max Health skill](docs/rpg.md) | 6/10 |
| [Health Regeneration skill](docs/rpg.md) | 6/10 |
| [Kill XP and levelling](docs/rpg.md) | 7/10 |
| [Per-playthrough persistence](docs/rpg.md) | 7/10 |
| [Runtime hunger, thirst, and inventory settings](docs/features.md) | 8/10 |
| ImGui RPG, table, class, and struct tabs | 6/10 |
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
