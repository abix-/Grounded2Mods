# ueforge docs

> **Authoritative on:** the ueforge framework. One file per
> subsystem, every load-bearing API explained, every design
> rationale captured.

`../README.md` is the entry point + crate-level audit table +
"how to bootstrap a new game" guide. The doctrine docs in this
directory explain *how* to write code on top of ueforge well.
The per-subsystem files explain *what* each subsystem does.

## The five framework modules

ueforge ships opinionated modules for the five most common UE5
mod patterns. A new game's mod picks from these menus and
writes only game-specific knobs (UE class names, field offsets,
UFunction parm shapes); the per-game extension surface is
`&'static` config + opt-in trait impl.

| Module | Crate path | Lift |
|---|---|---|
| RPG | `ueforge::rpg` | Skill catalog + XP curve + `CreatureRegistry` + per-slot persistence + ImGui tab + the 8-type Effect library (PlayerFloatEffect / SubcomponentFloat / Additive / U32Mask / Multiply / ClassFieldsMultiply / Runtime / StatusEffectApply) + `TriggerDef` + composition model. |
| Stacks | `ueforge::stacks` | Inventory stack-size data-table tweak. `StackTweak::new(table, offset, default_mult, skip)` + multiplier atomic + on-first-sight worker. |
| Difficulty | `ueforge::difficulty` | Per-class CDO field tweak (drain rates / damage multipliers / regen). `DifficultyKnob::new(class, offset)` + `apply_to_cdos`. |
| Inventory | `ueforge::inventory::viewport` | Viewport-paging hook framework: mouse-wheel scroll + per-widget viewport-start state + synthetic-refresh re-entrance guard + post-refresh rebind. |
| Damage | `ueforge::damage` | Universal damage-event hook: multicast trampoline + parm decode + FDamageInfo lookup + Player/Other classification + before/after dispatch (Critical / Evasion / Lifesteal / Thorns / kill credit). |

Modules are independent, opt-in via use sites. A pure
stack-size mod only consumes `stacks`. An RPG-only mod only
consumes `rpg`. A mod that uses all five picks one knob from
each menu and ignores the rest. **Each universal pattern is
defined ONCE in ueforge.**

## Doctrine

| File | Authoritative on |
|---|---|
| [architecture.md](architecture.md) | Universal Def -> Registry -> Instance -> Controller pattern (k8s-style CRD model) + per-subject compliance scorecard. Read first when adding a new module. |
| [PERFORMANCE.md](PERFORMANCE.md) | Hot-path discipline. Zero allocs, bail early, no mutexes on empty path, bounded everything. The 10/10 bar. |
| [RESEARCH.md](RESEARCH.md) | TDD investigation methodology for UE5 games. Standard probes, doctrines on data tables / damage / instigators, snapshot-not-log validation. |
| [inspection.md](inspection.md) | Worked examples for inspecting UE5 mods at the pak / asset level. Generic UE5 modding methodology. Works against any UE 5.x game. |
| [mod-formats.md](mod-formats.md) | Six common UE mod shapes (pak / DLL proxy / UE4SS Lua / UE4SS C++ / injected / loose / official); pak-vs-DLL capability matrix; why ueforge targets UE4SS CPPMods. |

## Historical (frozen)

| File | Authoritative on |
|---|---|
| [rust-port.md](rust-port.md) | Frozen: the C++ winhttp-proxy -> Rust cdylib port plan. Captures the architecture and migration sequence; the framework has evolved further since then. |
| [ue4ss-port.md](ue4ss-port.md) | Frozen: the standalone-injector -> UE4SS CPPMod pivot. Why we ride on UE4SS, the C++ shim design, the CppUserModBase mirror. |

## Subsystem reference

| Subsystem | File | Headline |
|---|---|---|
| Lifecycle | [lifecycle.md](lifecycle.md) | `ue4ss_mod!`, `ModInfo`, C++ shim, on_unreal_init, on_shutdown, hot-reload (Ctrl+R, full Phase A+B), build/deploy |
| UE5 engine reference | [ue-engine.md](ue-engine.md) | Generic UE5 platform: pak / IoStore container, UObject layout, shipping-build constraints, UE4SS hooking caveats, cooked PropertyTag layout, general-purpose modding tools |
| UE5 status-effect system | [status-effects.md](status-effects.md) | The universal "data-table-row + component + native query" pattern every UE5 RPG game uses; binding via the framework's `StatusEffectApply` Effect type |
| UE SDK | [ue-sdk.md](ue-sdk.md) | UObject family + `ClassRef` + `TypedField<T>` + GObjects + `PlayerRef` + `ue::field` byte ops + `ue::actor` (class-chain / controller helpers) + `ue::damage_info` (`DamageInfoLayout`) + `ue::pe_call::call_ufunction` |
| Hooks | [hooks.md](hooks.md) | `ProcessEventHook` + `HookRegistry::shutdown_all` (hot-reload safety) + `function_table!` + `install_with_backoff` + `install_immediate_or_log` + per-Entry `active_calls` drain |
| Game-thread queue | [pe-queue.md](pe-queue.md) | `Queue`, `GameThread`, and `game_thread::serve` (drains from `UEngine::Tick`, so queued work runs at the main menu too) |
| Actor transforms | rustdoc on `ue::transform` | Where an actor is: `RootComponent` to `USceneComponent`, read location / rotation / scale, static mesh + its half-extent, `set_mobility`, `set_static_mesh` |
| Asset registry | rustdoc on `assets` | Every asset the game ships whether loaded or not (`assets_of_class`), and `load_asset` to pull one into memory. Sees what a GObjects walk cannot |
| Counters & rings | [counters.md](counters.md) | `counter!`, `peak!`, `time_scope`, `EventRing<T>`, `counter_json!` |
| RPG framework | [rpg.md](rpg.md) | `SkillDef`, `Tracker`, `Effect` trait + 8 framework Effect types, `TriggerDef`, `CreatureRegistry`, `tab::render`, `SlotKeyResolver`, `VanillaCache`, `HealthBinding` |
| HTTP control plane | [http.md](http.md) | `server::spawn` + `SpawnHandle::stop` + `Config::auth_token` + builtin ops + selectors |
| Test client | [testing.md](testing.md) | `Api<S>` + `client` (DT walks / class lookups / typed reads) + `client::diff` (snapshot deltas) + `client::scenario` (Pester-style skill DSL) |
| ImGui | [imgui.md](imgui.md) | `ui` module, `Tab` registration, scanner tab |
| Settings | [settings.md](settings.md) | `Settings<T>` atomic JSON store + `Settings::watch` (mtime hot-reload) + `WatchHandle` |
| Workers | [worker.md](worker.md) | `worker::spawn` named thread + panic catch |
| Data tables | [data-table.md](data-table.md) | `FieldTweak<T>`, `ClassFieldTweak<T>`, row mutation, timing rules |
| Memory tools | [memory-tools.md](memory-tools.md) | Cheat-Engine-style scanner, freezes, `inspect_address`, `winproc` |
| Offline `.uasset` parsing | [uasset.md](uasset.md) | `uasset::extract_printable_strings` / `parse_name_table` / `find_int_property` + `dump-strings` / `read-property` CLIs |
| Native (C++) surface | [native.md](native.md) | Why C++ is in the repo (UE4SS plugin ABI + vendored ImGui), what stays in Rust, the doctrine that keeps it that way |

Per-module docs (`stacks.md`, `difficulty.md`, `inventory.md`,
`damage.md`, `debug.md`) pending; the rustdoc on each module's
`mod.rs` is canonical until they land.

## Reading order

New to ueforge:

1. `../README.md`. What + why
2. [architecture.md](architecture.md). The layering pattern every module follows
3. [lifecycle.md](lifecycle.md). Minimum viable mod skeleton
4. [ue-sdk.md](ue-sdk.md). Type system you'll use everywhere
5. [hooks.md](hooks.md). If you hook anything (you will)
6. [PERFORMANCE.md](PERFORMANCE.md). Before writing hot-path code
7. [RESEARCH.md](RESEARCH.md). Before debugging anything

Pick the rest as work demands; cross-references are dense.

## Contributing

When you change ueforge code, update the relevant doc in the
**same commit**. Stale docs lie; rotted docs cost the next
reader days. The audit table in `../README.md` should also
reflect any new public surface.
