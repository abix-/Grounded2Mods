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
| [Quality system](docs/faction-war.md) (tiered loot, craft rolls) | 3/10 |
| [Act repertoire](docs/faction-war.md) (theft, trade, robbery, murder, extortion) | 3/10 |
| [AI-vs-AI war](docs/faction-war.md) (ignition, sustain, ceasefire) | 3/10 |
| [Town growth](docs/faction-war.md) (annexes, recruits, builders) | 3/10 |
| [Storyteller / director](docs/status.md) (Randy Random incursions, dread loop) | 2/10 |
| [Named uniques](docs/status.md) (one-of-a-kind items from incursions) | 2/10 |
| [More to do](docs/status.md) (ecosystem-generated work, bounties) | 3/10 |
| [Chronicle](docs/status.md) (in-game narration of world events) | 3/10 |
| HTTP control plane | 9/10 |

## Build

```sh
k3sc cargo-lock build --release -p survivalist-mod
```

Output: `target/x86_64-pc-windows-msvc/release/survivalist_mod.dll`

## Deploy

```powershell
.\survivalist-mod\scripts\build_and_deploy.ps1
```

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
