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
| [Combat kill credit](docs/status.md) (recent-player-hit attribution with downing deduplication) | 10/10 |
| [Combat XP and levelling](docs/status.md) | 7/10 |
| [Heavy Hands](docs/status.md) (persistent punch-damage skill with auto-spend) | 7/10 |
| [Per-save RPG persistence](docs/status.md) | 7/10 |
| [Toughness-scaled cash drops](docs/status.md) | 6/10 |
| [Persistent regional garrisons](docs/status.md) | 2/10 |
| [Rolled mob types](docs/status.md) (tough, armed, and veteran) | 2/10 |
| [Cartel influence loss](docs/status.md) (garrison deaths weaken the region) | 3/10 |
| [Region takeover trigger](docs/status.md) | 3/10 |
| [Combat event tracing](docs/research.md) | 9/10 |
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
