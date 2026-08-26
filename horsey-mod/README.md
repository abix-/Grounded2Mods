# horsey-mod

Native-PE binding of [`modforge`](../modforge) for Horsey Game.

Sibling to `ueforge` (UE5/UE4SS) and `unityforge` (Unity Mono/IL2CPP).
Where those rely on a managed-runtime plugin loader, horsey-mod attaches
via an injector EXE that `CreateRemoteThread`s a `LoadLibraryW` on
`horsey.dll` into the running `Horsey.exe`.

## Status

| Component | Status |
|---|---|
| `horsey.dll`, `horsey-inject.exe`, and `horsey-play.exe` | implemented |
| Fresh-launch DLL injection | implemented |
| HTTP control plane on `127.0.0.1:33077` | implemented, localhost only, no auth |
| Runtime target registry and structural validation | implemented, hardening continues |
| Game-state snapshot, money, year, sleep, and cheat controls | implemented |
| Horse roster discovery and field editing | implemented |
| Sleep-safe no-tire patch | implemented and enabled at attach |
| In-game horse and genome overlay | implemented |
| Synthetic input surface | partially implemented |
| Vanilla and extended allele read/write operations | implemented |
| Extended-gene XML content loading | implemented |
| Extended-gene evaluation, lifecycle, combinator, and render hooks | partially implemented |
| Save sidecar hooks | implemented but unsafe to arm on the current build |
| Scene discovery and horse-transfer automation | research stage |

## What you get out of the box

A localhost HTTP control plane on `127.0.0.1:33077`, powered by
`modforge::server`. Selected endpoints:

| Op | Args | Effect |
|---|---|---|
| `ping` | | Returns `"pong"` |
| `list_ops` | | Lists every registered op |
| `game.read` | | Snapshot of money/year/sleeps/races/horse_count/no_tire/debug_mode |
| `game.money.get` | | Current money |
| `game.money.set` | `{value: u32}` | Set money |
| `game.money.add` | `{value: i32}` | Add to money (saturating) |
| `game.year.get` | | Current year |
| `game.year.set` | `{value: u32}` | Set year |
| `cheats.no_tire.get` | | Read No Tire toggle |
| `cheats.no_tire.set` | `{enabled: bool}` | Toggle fatigue-disabled mode |
| `cheats.debug_mode.get` | | Read debug-mode flag |
| `cheats.debug_mode.set` | `{enabled: bool}` | Force debug mode on/off (skip the "type debug" unlock) |
| `horses.count` | | Number of horses in roster |
| `horses.roster_addr` | `{index: usize}` | Memory address of roster entry |
| `horse.read` | `{addr: hex-string}` | Read horse fields by address |
| `horse.set_age` | `{addr, value: i32}` | Set age |
| `horse.set_max_age` | `{addr, value: i32}` | Set lifespan |
| `horse.clear_tiredness` | `{addr}` | Zero the tired flags |

## Setup

1. Build:
   ```powershell
   cargo build -p horsey-mod --release
   ```
   This produces:
   - `target/x86_64-pc-windows-msvc/release/horsey.dll`
   - `target/x86_64-pc-windows-msvc/release/horsey-inject.exe`

2. Launch `Horsey.exe` normally (via Steam or directly).

3. Inject:
   ```powershell
   target\x86_64-pc-windows-msvc\release\horsey-inject.exe `
     --dll target\x86_64-pc-windows-msvc\release\horsey.dll `
     --fresh
   ```
   The injector finds `Horsey.exe` and `CreateRemoteThread`s
   `LoadLibraryW(horsey.dll)` into it.

4. Check `horsey.log` beside the staged DLL. It should contain:
   ```
   horsey-mod: listening on 127.0.0.1:33077 (auth disabled)
   ```

5. Test:
   ```powershell
   curl.exe -X POST http://127.0.0.1:33077/op `
     -d '{"op":"ping"}'
   ```

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                       modforge                           │
│  HTTP server · op registry · selector grammar · settings │
│  RPG (Effect/Trigger/Skill) · log · scanner · winproc    │
└──────────────────────────────────┬───────────────────────┘
                                   │
        ┌──────────────────────────┼──────────────────────┐
        │                          │                      │
   ┌────▼────┐                ┌────▼─────┐         ┌──────▼──────┐
   │ ueforge │                │unityforge│         │ horsey-mod │
   │  UE5    │                │  Unity   │         │  Native PE  │
   │  UE4SS  │                │ BepInEx  │         │   inject    │
   └─────────┘                └──────────┘         └─────────────┘
```

## Why injector, not proxy DLL

We initially planned to proxy `steam_api64.dll` (rename the original,
drop ours in its place, forward 1,089 Steam API exports). MSVC link.exe's
`.DEF` forwarder support proved brittle: the linker treated `name =
OtherDll.name` as a local alias instead of a PE forwarder, producing 1,089
unresolved-symbol errors.

The injector pattern is:

- **Simpler**: a single `CreateRemoteThread` call.
- **Development-friendly**: stages each DLL generation without locking
  Cargo's build output.
- **No 1,089 forwarders**: only our DllMain is exported.
- **Game-agnostic**: same code injects into any native-PE Windows game.

Tradeoff: the user runs the injector after launching the game (a separate
step). This is fine for development; for end-user distribution we can
add a small launcher EXE that combines "start Horsey via Steam" with
"wait for it to load, then inject".

## Current work

Resolver hardening, extended-gene population support, safe sidecar
persistence, and scene automation are tracked in
[`docs/todo.md`](docs/todo.md).

## Files

| File | Purpose |
|---|---|
| `src/lib.rs` | DllMain + worker thread + server bootstrap |
| `src/bin/inject.rs` | Fresh-launch DLL injector |
| `src/bin/play.rs` | Build, launch, inject, and wait helper |
| `src/targets_registry.rs` | Runtime-resolved function, global, and field targets |
| `src/gamestate.rs` | Typed accessors for the GameState global |
| `src/horse.rs` | Typed accessors for the Horse struct |
| `src/ops.rs` | Horsey-specific op registrations |
| `src/snapshot.rs` | The `HorseyState` snapshot returned in every response |
| `src/overlay.rs` | In-game horse and genome editor |
| `src/genes.rs` | Extended-gene state and evaluation |
