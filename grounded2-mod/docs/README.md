# grounded2-mod docs

> **Authoritative on:** the Grounded 2 RPG / level-up mod
> (Grounded2RPG). Per-subject deep dives, all
> game-specific to Grounded 2. Generic UE5 + framework
> material lives in `../../ueforge/docs/`.

## Contents

| File | Subject |
|---|---|
| [building.md](building.md) | Build, install, deploy. Prereqs, cargo commands, deploy script, configuration. |
| [features.md](features.md) | User-facing feature list. What the mod does in-game. |
| [rpg.md](rpg.md) | RPG subsystem: catalog, sqrt curve, persistence, code map, shared Effect implementations, and Grounded 2 configuration. |
| [damage.md](damage.md) | Grounded 2 damage internals: Maine combat / fall / environmental pipelines, instigator resolution, status-effect data table layout. |
| [inventory.md](inventory.md) | Backpack patch internals (Maine InventoryComponent CDO) + viewport rebind binder. |
| [engine.md](engine.md) | Grounded 2 engine specifics: pak, exes, GObjects offsets, shipping caveats. |
| [performance.md](performance.md) | What this mod costs to run. G2-specific numbers (per-call costs on Maine classes, allocation analysis of the kill_hook). |
| [ongoing.md](ongoing.md) | Every piece of code in this mod that runs continuously. Hot-path triage reference for active investigations. |
| [testing.md](testing.md) | g2rpg-specific test setup. Env var name, the bandage-regression test, per-skill scenario use sites. |

## Where to look first

| Symptom | First file to read |
|---|---|
| skill not applying in-game | deployed `grounded2_rpg.log` then [rpg.md](rpg.md) |
| damage / fall / environmental issue | [damage.md](damage.md) |
| inventory / scrolling issue | [inventory.md](inventory.md) |
| save / slot detection | [rpg.md](rpg.md) "Persistence" section |
| build fails | [building.md](building.md) |
| performance / allocation regression | [performance.md](performance.md) + ueforge's [`PERFORMANCE.md`](../../ueforge/docs/PERFORMANCE.md) |
| writing an integration test | [testing.md](testing.md) + ueforge's [`testing.md`](../../ueforge/docs/testing.md) |

## Framework references

g2rpg is built on the `ueforge` framework. Anything generic
(UE5 patterns, framework modules, test client, hot-reload,
historical port docs) lives in `../../ueforge/docs/`:

- [`../../ueforge/README.md`](../../ueforge/README.md). Entry point + audit table
- [`../../ueforge/docs/`](../../ueforge/docs/). Per-module / per-subsystem reference
- [`../../ueforge/docs/PERFORMANCE.md`](../../ueforge/docs/PERFORMANCE.md). Hot-path doctrine
- [`../../ueforge/docs/RESEARCH.md`](../../ueforge/docs/RESEARCH.md). TDD methodology
- [`../../ueforge/docs/testing.md`](../../ueforge/docs/testing.md). Test client + research/diff/scenario DSL
- [`../../ueforge/docs/ue-engine.md`](../../ueforge/docs/ue-engine.md). Generic UE5 platform: pak / IoStore, UObject layout, shipping-build constraints, UE4SS hooking caveats
- [`../../ueforge/docs/status-effects.md`](../../ueforge/docs/status-effects.md). Universal UE5 status-effect-system pattern (what `damage.md`'s long section is the G2 example of)
- [`../../ueforge/docs/inspection.md`](../../ueforge/docs/inspection.md). Generic UE5 pak / asset inspection methodology
- [`../../ueforge/docs/mod-formats.md`](../../ueforge/docs/mod-formats.md). UE mod-format comparison (pak / DLL proxy / UE4SS); why g2rpg is a UE4SS CPPMod
- [`../../ueforge/docs/rust-port.md`](../../ueforge/docs/rust-port.md). Historical: C++ winhttp proxy -> Rust cdylib
- [`../../ueforge/docs/ue4ss-port.md`](../../ueforge/docs/ue4ss-port.md). Historical: custom DLL injection -> UE4SS CPPMod
