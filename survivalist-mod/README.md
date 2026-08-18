# survivalist-mod

Survivalist: Invisible Strain mod. Rust cdylib loaded by the
game's official mod loader (`Story.LoadDLLs`), not BepInEx.

## Game

- **Game:** Survivalist: Invisible Strain
- **Engine:** Unity (Mono)
- **Mod loader:** built-in (`Story.LoadDLLs -> Main.Load`)
- **Framework:** unityforge (re-exports modforge)

## Build

```sh
k3sc cargo-lock build --release -p survivalist-mod
```

Output: `target/x86_64-pc-windows-msvc/release/survivalist_mod.dll`

## Deploy

```powershell
.\survivalist-mod\scripts\build_and_deploy.ps1
```

## Features

- HTTP control plane for live research
- Load/unload re-init path for story switching

## File layout

```
survivalist-mod/
  Cargo.toml
  README.md
  scripts/
    build_and_deploy.ps1
    generate_quality.ps1
  src/
    lib.rs
```
