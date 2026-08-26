# grounded2-mod

Grounded 2 RPG mod with kill-driven progression, persistent
skills, survival tweaks, and expanded inventory.

## Game

- **Game:** Grounded 2
- **Engine:** Unreal Engine 5
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Features

| Feature | Description | Rating |
|---|---|---:|
| [Backpack](src/rpg/skills.rs) | Skill-driven inventory capacity. | 8/10 |
| [Inventory scrolling](src/inv_hook.rs) | Mouse-wheel access to slots beyond the 4x10 viewport. | 8/10 |
| [Hunger Resistance](src/rpg/skills.rs) | Reduces hunger drain. | 8/10 |
| [Thirst Resistance](src/rpg/skills.rs) | Reduces thirst drain. | 8/10 |
| [Attack Damage](src/rpg/skills.rs) | Increases outgoing damage. | 7/10 |
| [Armor](src/rpg/skills.rs) | Reduces combat damage. | 7/10 |
| [Move Speed](src/rpg/skills.rs) | Increases movement speeds. | 7/10 |
| [Jump Height](src/rpg/skills.rs) | Increases jump velocity. | 7/10 |
| [Leap Distance](src/rpg/skills.rs) | Increases air control. | 6/10 |
| [Glide Speed](src/rpg/skills.rs) | Increases flight and glide speed. | 6/10 |
| [Fall Resistance](src/rpg/fall_hook.rs) | Reduces fall damage before the native calculation. | 6/10 |
| [Impact Resistance](src/rpg/skills.rs) | Blocks environmental impact damage. | 4/10 |
| [Lifesteal](src/rpg/effects.rs) | Heals from credited combat damage. | 4/10 |
| [Max Health](src/rpg/skills.rs) | Raises maximum health. | 6/10 |
| [Health Regeneration](src/rpg/skills.rs) | Improves combat regeneration. | 6/10 |
| [Kill XP](src/rpg/kill_hook.rs) | Credits creature kills to progression. | 7/10 |
| [Progression](src/rpg/tracker.rs) | Levels, skill points, spending, refunds, and toggles. | 7/10 |
| [Persistence](src/rpg/world_loader.rs) | Loads per-playthrough skill state. | 7/10 |
| [Settings](src/settings.rs) | Runtime inventory, survival, RPG, and debug configuration. | 8/10 |
| [ImGui](src/lib.rs) | RPG, table, class, and struct tabs. | 6/10 |
| [Control plane](src/debug.rs) | Optional HTTP inspection and control on port 17171. | 9/10 |

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
