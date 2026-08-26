# survivalist-mod

Survivalist: Invisible Strain mod. Rust cdylib loaded by the
game's official mod loader (`Story.LoadDLLs`), not BepInEx.

## Game

- **Game:** Survivalist: Invisible Strain
- **Engine:** Unity (Mono)
- **Mod loader:** built-in (`Story.LoadDLLs -> Main.Load`)
- **Framework:** unityforge (re-exports modforge)

## Features

| Feature | Rating |
|---|---:|
| [Faction personality / evolution](docs/faction-war.md) | 4/10 |
| [Settlement upgrades](docs/plans/2026-07-11-settlement-upgrades.md) | 3/10 |
| [Quality system](docs/faction-war.md) (tiered loot and craft rolls) | 3/10 |
| [Act repertoire](docs/faction-war.md) (trade, theft, robbery, scavenging, murder) | 3/10 |
| [AI-vs-AI war](docs/faction-war.md) (ignition, sustain, ceasefire) | 3/10 |
| [Town growth](docs/faction-war.md) (annexes, recruits, builders) | 3/10 |
| [Storyteller / director](docs/status.md) (incursions, strangers, adaptive pressure) | 2/10 |
| [Named uniques](docs/status.md) (one-of-a-kind world-owned items) | 2/10 |
| [Ecosystem work](docs/status.md) (bounties, threats, couriers, work board) | 3/10 |
| [Chronicle](docs/status.md) (in-game narration of public events) | 3/10 |
| HTTP control plane on port 17173 | 9/10 |

[Current status](docs/status.md) separates code-landed features
from behavior verified in the running game.

## Build

```sh
cargo build --release -p survivalist-mod
```

Output: `target/x86_64-pc-windows-msvc/release/survivalist_mod.dll`

Build the C# bridge against the game's managed assemblies:

```powershell
dotnet build unityforge\cs-shim-survivalist\Unityforge.Shim.Survivalist.csproj `
  -c Release `
  -p:UnityDir="<game-root>\Survivalist Invisible Strain_Data\Managed"
```

## Deploy

```powershell
.\survivalist-mod\scripts\build_and_deploy.ps1
```

The deploy script builds both components, copies the story XML,
and accepts `-GameDir`, `-ModName`, `-NoCopy`, and `-Hot`.

## Documentation

- [Current status](docs/status.md)
- [Faction simulation and design](docs/faction-war.md)
- [Runtime research](docs/research.md)
- [Settlement upgrades](docs/plans/2026-07-11-settlement-upgrades.md)
- [Bounty arc](docs/plans/2026-07-10-bounty-arc.md)

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
  story/
```
