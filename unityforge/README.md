# unityforge

Unity Mono/IL2CPP framework. Peer of `ueforge` for Unity games.
Depends on `modforge` for the engine-agnostic core (envelope, ops
registry, server, settings, counters, log). Adds Unity-specific
surfaces.

## Modules

| Module | Purpose |
|---|---|
| `mono` | Reflection wrappers (calls through the C# shim bridge) |
| `hook` | HarmonyLib wrapper + HookRegistry |
| `main_thread_queue` | Game-thread dispatch (parallel to ueforge's pe_queue) |
| `ops` | Unity generic-primitive op handlers |
| `selector` | Unity-side selector resolvers |
| `rpg::std_effect` | Mono Effect implementations |
| `client` | Re-exports `modforge::client` for test code |

## C# shims

The Rust cdylib needs a managed-code entry point to get loaded
into Unity. Three shim variants exist:

| Shim | Loader | Target |
|---|---|---|
| `cs-shim-mono` | BepInEx 5 (Mono) | WWM, other Mono games |
| `cs-shim-melonloader` | MelonLoader | Schedule 1, IL2CPP games |
| `cs-shim-survivalist` | Game's own mod loader | Survivalist: Invisible Strain |

Each shim scans its directory for `*.unityforge.dll` and calls
`LoadLibrary` + `unityforge_init`.

## Build

```sh
k3sc cargo-lock build --release -p unityforge
```

The framework itself is a library crate. Game mods depend on it
and build their own cdylib.

## File layout

```
unityforge/
  Cargo.toml
  README.md
  src/
    lib.rs
    mono.rs
    hook.rs
    ops.rs
    selector.rs
    client/mod.rs
    rpg/
  cs-shim-mono/
  cs-shim-melonloader/
  cs-shim-survivalist/
  cpp/                    # imgui vendored source
```
