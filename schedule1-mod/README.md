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

| Feature | Description | Rating |
|---|---|---:|
| [Kill credit](src/killcredit.rs) | Attributes player hits and deduplicates NPC downs. | 10/10 |
| [Progression](src/skills.rs) | Converts combat XP into levels and skill points. | 7/10 |
| [Heavy Hands](src/skills.rs) | Persistently increases punch damage with auto-spend. | 7/10 |
| [Persistence](src/skills.rs) | Stores RPG state per loaded save. | 7/10 |
| [Cash drops](src/loot.rs) | Drops toughness-scaled cash at defeated NPCs. | 6/10 |
| [Garrisons](src/farming.rs) | Keeps regional cartel forces active across the map. | 2/10 |
| [Mob types](src/farming.rs) | Rolls tough, armed, and veteran variants. | 2/10 |
| [Influence](src/farming.rs) | Garrison deaths weaken cartel control. | 3/10 |
| [Takeover](src/farming.rs) | Flips cleared zero-influence regions to the player. | 3/10 |
| [Combat tracing](src/combat_trace.rs) | Records damage, attack, death, and knockout events. | 9/10 |
| [Control plane](src/lib.rs) | HTTP research and control on port 17175. | 10/10 |

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
