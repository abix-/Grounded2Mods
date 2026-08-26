# outworld-station-mod

Outworld Station runtime mod for live item-stack and data-table
tweaks.

## Game

- **Game:** Outworld Station
- **Engine:** Unreal Engine 5
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Features

| Feature | Description | Rating |
|---|---|---:|
| [Stacks](src/stacks.rs) | Applies a configurable multiplier from captured vanilla baselines. | 4/10 |
| [Dynamic tweaks](src/settings.rs) | Applies settings-driven data-table changes. | 3/10 |
| [ImGui](src/lib.rs) | Tweak, scanner, table, class, and struct tabs. | 6/10 |
| [Control plane](src/debug.rs) | HTTP inspection and control on port 17172. | 9/10 |

## Build

```sh
cargo build --release -p outworld-station-mod
```

Output:
`target/x86_64-pc-windows-msvc/release/outworld_station_mod.dll`,
deployed as `main.dll` (the name UE4SS requires in the dlls
folder). The build artifact carries the crate's own name so two
mods in this workspace cannot overwrite each other's DLL.

## Deploy

```sh
cargo deploy install -p outworld-station-mod
```

Installs to `OutworldStation\Binaries\Win64\ue4ss\Mods\OutworldStationMod\dlls\main.dll`.

See [the research notes](docs/research.md) for the runtime
DataTable caching behavior that determines when tweaks apply.

## File layout

```
outworld-station-mod/
  Cargo.toml
  README.md
  settings.example.json
  src/
    lib.rs
    stacks.rs
  tests/
    common/mod.rs
    explore_dt_rows.rs
```
