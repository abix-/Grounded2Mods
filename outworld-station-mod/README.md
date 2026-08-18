# outworld-station-mod

Outworld Station (UE 5) runtime mod. First feature: item stack
size adjustments via DataTable mutation at runtime.

## Game

- **Game:** Outworld Station
- **Engine:** Unreal Engine 5
- **Mod loader:** UE4SS
- **Framework:** ueforge (re-exports modforge)

## Build

```sh
k3sc cargo-lock build --release -p outworld-station-mod
```

Output: `target/x86_64-pc-windows-msvc/release/main.dll`

## Deploy

```sh
cargo deploy install -p outworld-station-mod
```

Installs to `OutworldStation\Binaries\Win64\ue4ss\Mods\OutworldStationMod\dlls\main.dll`.

## Features

- Item stack size adjustments (DataTable `DT_Materials` field `MaxCanStack`)
- HTTP control plane for live research and runtime tweaks

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
