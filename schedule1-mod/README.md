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
cargo build --release -p schedule1-mod
```

Output: `target/x86_64-pc-windows-msvc/release/schedule1_mod.dll`

The C# shim (`Unityforge.Shim.Melon`) is built separately:

```sh
dotnet build unityforge/cs-shim-melonloader/Unityforge.Shim.Melon.csproj \
  -c Release \
  -p:MelonLoaderDir="<game-root>/MelonLoader"
```

## Deploy

Copy both DLLs into the MelonLoader mods directory:
- `schedule1_mod.dll` (renamed to `schedule1_mod.unityforge.dll`)
- `Unityforge.Shim.Melon.dll`

## Features

| Feature | Rating |
|---|---:|
| [Combat-XP levelling](docs/status.md) (persistence, auto-spend, Heavy Hands) | 7/10 |
| [Cash loot drops](docs/status.md) (kill credit and toughness scaling) | 6/10 |
| [Mob farming areas](docs/status.md) (regional garrisons and rolled mob types) | 2/10 |
| [Faction war](docs/status.md) (influence loss and takeover groundwork) | 3/10 |
| HTTP research and control plane on port 17175 | 10/10 |

## Documentation

- [Current status](docs/status.md)
- [Research](docs/research.md)
- [Certainty tracking](docs/certainty-tracking.md)
- [Open issues](docs/todo.md)
- [Plan](docs/plan.md)

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
