# outworld-station-mod

Outworld Station (UE 5) runtime mod. First feature: item stack
size adjustments via DataTable mutation at runtime.

## Game

- **Game:** Outworld Station
- **Engine:** Unreal Engine 5
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Features

| Feature | Rating |
|---|---:|
| [Stack size adjustments](docs/research.md) (DataTable mutation at runtime) | 4/10 |
| HTTP control plane | 9/10 |

## Build

```sh
k3sc cargo-lock build --release -p outworld-station-mod
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

## File layout

```
outworld-station-mod/
  Cargo.toml
  README.md
  src/
    lib.rs
  tests/
    common/mod.rs
    explore_dt_rows.rs
```
