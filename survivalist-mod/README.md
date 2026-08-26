# survivalist-mod

Survivalist: Invisible Strain mod. Rust cdylib loaded by the
game's official mod loader (`Story.LoadDLLs`), not BepInEx.

## Game

- **Game:** Survivalist: Invisible Strain
- **Engine:** Unity (Mono)
- **Mod loader:** built-in (`Story.LoadDLLs -> Main.Load`)
- **Framework:** unityforge (re-exports modforge)

## Features

| Feature | Description | Rating |
|---|---|---:|
| [Infections](src/infection.rs) | Forces every new injury to enter uninfected. | 3/10 |
| [Genomes](src/genome.rs) | Persists heritable aggression, expansionism, defensiveness, and guile. | 4/10 |
| [Learning](src/genome.rs) | Reinforces faction and voter traits from outcomes. | 4/10 |
| [War](src/war.rs) | Drives AI revenge, invasion targets, and forced ignition. | 3/10 |
| [Survival](src/survival.rs) | Turns hunger and collapse into raids, defection, and surrender. | 3/10 |
| [Predation](src/predation.rs) | Lets victors absorb survivors and portable goods. | 3/10 |
| [Growth](src/growth.rs) | Suppresses conjured population and recruits real refugees. | 3/10 |
| [Annexes](src/development.rs) | Plans real settlement construction through vanilla builders. | 3/10 |
| [Upgrades](src/upgrade.rs) | Tracks per-structure and settlement-wide improvements. | 3/10 |
| [Quality](src/quality.rs) | Rolls item tiers at the edge, in shops, and after crafting. | 3/10 |
| [Theft](src/steal.rs) | Sends thieves whose detection can ignite an organic war. | 3/10 |
| [Trade](src/trade.rs) | Moves real food and payment between AI settlements. | 3/10 |
| [Robbery](src/rob.rs) | Runs walking ambush parties and item demands. | 3/10 |
| [Scavenging](src/scavenge.rs) | Sends parties to recover ownerless goods. | 2/10 |
| [Assassination](src/murder.rs) | Sends stealth attackers after enemy leaders. | 2/10 |
| [Bounties](src/bounty.rs) | Offers contracts on enemy leaders. | 3/10 |
| [Threats](src/threat.rs) | Pays the player to clear raiders near camps. | 3/10 |
| [Work board](src/board.rs) | Publishes work in the quest journal and closes outcomes. | 3/10 |
| [Couriers](src/courier.rs) | Delivers payment from real settlement stores. | 3/10 |
| [Storyteller](src/storyteller.rs) | Paces weighted events and the dread loop. | 2/10 |
| [Incursions](src/incursion.rs) | Brings raiders, military, refugees, signals, and strangers from off-map. | 2/10 |
| [Horde](src/horde.rs) | Sends scaled zombie pressure toward the largest settlement. | 2/10 |
| [Strangers](src/stranger.rs) | Hides friendly, wary, or aggressive arrival intent. | 2/10 |
| [Vendors](src/vendor.rs) | Carries real camp goods between settlements. | 2/10 |
| [Settlers](src/settler.rs) | Lets off-map groups reclaim dead bases. | 2/10 |
| [Uniques](src/unique.rs) | Introduces and tracks one-of-a-kind items once per save. | 2/10 |
| [Chronicle](src/chronicle.rs) | Narrates public events through the game HUD. | 3/10 |
| [Control plane](src/lib.rs) | HTTP inspection and control on port 17173. | 9/10 |

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
