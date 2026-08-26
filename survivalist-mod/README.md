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
| [No infections](docs/research.md) (injuries enter uninfected) | 3/10 |
| [Faction genomes](docs/faction-war.md) (heritable aggression, expansionism, defensiveness, and guile) | 4/10 |
| [Franchise decisions and learning](docs/faction-war.md) (individual votes reinforce future behavior) | 4/10 |
| [AI-vs-AI war](docs/faction-war.md) (revenge, ambition, hunger, and invasion targets) | 3/10 |
| [Peace and surrender](docs/faction-war.md) (bleeding factions can sue for peace) | 3/10 |
| [Predation and extinction](docs/faction-war.md) (victors absorb survivors and portable goods) | 3/10 |
| [Repopulation suppression](docs/faction-war.md) (settlements cannot conjure replacement people) | 3/10 |
| [Refugee recruitment](docs/faction-war.md) (real arrivals replenish settlements) | 3/10 |
| [Settlement annexes](docs/faction-war.md) (AI planners queue real construction) | 3/10 |
| [Settlement upgrades](docs/plans/2026-07-11-settlement-upgrades.md) (per-structure and settlement-wide tracks) | 3/10 |
| [Quality tiers](docs/faction-war.md) (edge, shop, and crafting rolls) | 3/10 |
| [Theft](docs/faction-war.md) (steal, detection, escape, and organic war ignition) | 3/10 |
| [AI trade](docs/faction-war.md) (real food delivery and payment) | 3/10 |
| [Road robbery](docs/faction-war.md) (walking ambush parties and demands) | 3/10 |
| [Scavenging](docs/faction-war.md) (ownerless loot parties) | 2/10 |
| [Assassination](docs/faction-war.md) (stealth attacks on enemy leaders) | 2/10 |
| [Bounties](docs/plans/2026-07-10-bounty-arc.md) (enemy-leader contracts) | 3/10 |
| [Threat-clearing jobs](docs/status.md) (player-paid camp defense work) | 3/10 |
| [Work board and payment couriers](docs/status.md) (journal offers and real-goods payment) | 3/10 |
| [Storyteller](docs/status.md) (weighted event pacing and dread loop) | 2/10 |
| [Off-map incursions](docs/status.md) (raiders, military, refugees, settlers, signals, and strangers) | 2/10 |
| [Adaptive horde](docs/status.md) (the largest settlement draws scaled zombie packs) | 2/10 |
| [Strangers](docs/status.md) (hidden friendly, wary, or aggressive intent) | 2/10 |
| [Traveling vendors](docs/status.md) (real camp goods carried between settlements) | 2/10 |
| [Settling factions](docs/status.md) (off-map groups reclaim dead bases) | 2/10 |
| [Named uniques](docs/status.md) (one-of-a-kind items enter and change hands once per save) | 2/10 |
| [Chronicle](docs/status.md) (public events narrated in the game HUD) | 3/10 |
| [Player ecosystem participation](docs/faction-war.md) (trade, robbery, war, murder, surrender, and horde targeting) | 2/10 |
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
