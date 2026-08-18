# schedule1-mod

Schedule 1 (Unity IL2CPP) mod. Rust cdylib loaded by
MelonLoader via the `Unityforge.Shim.Melon` C# entry point.

## Game

- **Game:** Schedule 1
- **Engine:** Unity (IL2CPP)
- **Mod loader:** MelonLoader
- **Framework:** unityforge (re-exports modforge)

## Build

```sh
k3sc cargo-lock build --release -p schedule1-mod
```

Output: `target/x86_64-pc-windows-msvc/release/schedule1_mod.dll`

The C# shim (`Unityforge.Shim.Melon`) is built separately:

```sh
dotnet build -c Release unityforge/cs-shim-melonloader/Unityforge.Shim.Melon.csproj
```

## Deploy

Copy both DLLs into the MelonLoader mods directory:
- `schedule1_mod.dll` (renamed to `schedule1_mod.unityforge.dll`)
- `Unityforge.Shim.Melon.dll`

## Features

- HTTP control plane for live research
- NPC behavior research (combat, retaliation, patrol)
- Combat system investigation

## Research

Research tests and findings live in `tests/` and `docs/`.
Set `S1_DEBUG_PORT` and run with `--test-threads=1 --nocapture`.

## File layout

```
schedule1-mod/
  Cargo.toml
  README.md
  docs/
    research.md
    certainty-tracking.md
    todo.md
  scripts/
    restart.ps1
  src/
    lib.rs
  tests/
    research_*.rs
```
