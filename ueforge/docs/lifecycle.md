# Lifecycle

> **Authoritative on:** how a ueforge-based mod loads, what runs
> in what order, what `ModDef` controls, what the C++ shim does,
> and how `cargo deploy` ships the result.

A ueforge mod is a Rust cdylib (`main.dll`) that UE4SS loads as a
CPPMod. The cdylib bundles a small C++ shim (compiled in by
ueforge's `build.rs` helper) that satisfies UE4SS's
`RC::CppUserModBase` ABI; the shim forwards every callback into
the Rust side via the extern "C" entry points the
`ue4ss_mod!` macro emits.

You declare three things in your crate:

1. A `static MOD_INFO: ueforge::ModDef` describing your mod.
2. A one-line `ueforge::ue4ss_mod!(MOD_INFO);` invocation.
3. A one-line `build.rs` that calls
   `ueforge::build::CppShim::new().compile()`.

Everything else is bookkeeping ueforge handles for you.

## ModDef

`ueforge::mod_main::ModDef` (re-exported as `ueforge::ModDef`)
declares your mod to UE4SS:

```rust
static MOD_INFO: ueforge::ModDef = ueforge::ModDef {
    name: "Grounded2Mod",                      // shown in UE4SS console
    version: "0.1.0",
    log_file: "grounded2_mod.log",             // file in DLL dir
    console_title: "Grounded 2 - RPG System",            // console window title
    console: cfg!(feature = "console"),          // gate console alloc
    on_unreal_init: bbp_on_unreal_init,          // fn pointer
    on_shutdown: bbp_on_shutdown,                // fn pointer
    tabs: &[ueforge::TabDef {
        name: "RPG",
        render: rpg::tab::render,
    }],
};

ueforge::ue4ss_mod!(MOD_INFO);
```

The `tabs:` array is a `&'static [TabDef]`; the ueforge shim
iterates it during ImGui setup and calls each `TabDef::render`
once per frame while the tab is visible. See
[imgui.md](imgui.md) for how rendering works.

## Lifecycle order

What runs, in order, when a player launches the game with
ueforge-based mod installed:

1. **DLL load.** UE4SS calls into our cdylib's `DllMain`
   (provided by ueforge's bundled C++ shim). The shim captures
   the `HMODULE` and forwards to ueforge's `log::set_dll_module`
   so `dll_dir()` resolves correctly for log + saves.
2. **Logger init.** `ueforge::log::init` runs from the shim,
   opening `<DLL_dir>/<log_file>` for write-through, line-flushed
   I/O.
3. **`MOD_INFO.on_unreal_init` fires** on UE4SS's mod-init
   thread. This is where you spawn a worker thread (always.
   never block the engine init thread), which then:
   - Loads settings via `ueforge::settings::Settings::<S>::load`.
   - Detects the platform via `ueforge::ue::platform::detect`.
   - Calls `ueforge::ue::init_runtime(image_base, offsets)`.
   - Walks GObjects to install hooks.
   - Spawns the slot poller and HTTP server if configured.
4. **Tab render callbacks fire** on UE4SS's render thread once
   per frame, only while the corresponding tab is visible in the
   ImGui window.
5. **`MOD_INFO.on_shutdown` fires** when the mod is unloaded
   (game close, hot-reload). Use this to stop any pollers /
   servers / workers your mod owns.

## The minimum viable mod skeleton

```rust
// src/lib.rs
use ueforge::ue::{PlatformOffsets, init_runtime, platform};

const STEAM: PlatformOffsets = PlatformOffsets {
    g_objects: 0x09F6_7028,
    append_string: 0x0125_2060,
    g_names: 0x09E4_A7B8,
    g_world: 0x09C7_A2E0,
    process_event: 0x0146_E7B0,
    process_event_idx: 0x4C,
};

const PLATFORMS: &[(&str, &PlatformOffsets)] = &[
    ("YourGame-Win64-Shipping.exe", &STEAM),
];

static MOD_INFO: ueforge::ModDef = ueforge::ModDef {
    name: "YourMod",
    version: "0.1.0",
    log_file: "your_mod.log",
    console_title: "YourMod",
    console: cfg!(feature = "console"),
    on_unreal_init: on_unreal_init,
    on_shutdown: on_shutdown,
    tabs: &[ueforge::TabDef { name: "Tweaks", render: render_tab }],
};

ueforge::ue4ss_mod!(MOD_INFO);

fn on_unreal_init() {
    ueforge::worker::spawn("yourmod/init", || unsafe { worker() });
}

fn on_shutdown() {
    ueforge::log!("yourmod: stopping");
}

unsafe fn worker() {
    let image_base = platform::host_image_base();
    let offsets = platform::detect(PLATFORMS).unwrap_or(&STEAM);
    let _rt = unsafe { init_runtime(image_base, offsets) };
    // ... install hooks, spawn pollers / servers ...
}

fn render_tab() {
    ueforge::ui::text("Hello");
}
```

```rust
// build.rs
fn main() {
    ueforge::build::CppShim::new().compile();
}
```

That's a complete mod. ~50 LoC. Everything else in your crate is
domain-specific (skills, hooks, UI bodies, snapshot fields, op
handlers).

## The C++ shim

ueforge ships a generic `RC::CppUserModBase` subclass
(`UespyMod` in `ueforge/cpp/ueforge_shim.cpp`) that provides:

- A `start_mod` factory function UE4SS calls to instantiate the
  mod object.
- A `DllMain` that captures `HMODULE` for `dll_dir()` resolution.
- Forwarding overrides for every `CppUserModBase` virtual
  callback that route into the Rust extern "C" entry points
  emitted by `ueforge::ue4ss_mod!`.

You **do not write a `DllMain`**. You **do not subclass
`CppUserModBase`**. You **do not export `start_mod` /
`uninstall_mod`**. ueforge does all of it, by default, in every
consumer cdylib.

If for some reason you need a custom shim (rare), call
`CppShim::new().skip_default_shim()` in your `build.rs` and
provide your own. grounded2-mod used to do this; the dedup
work removed the custom shim.

## build.rs

The minimum:

```rust
fn main() {
    ueforge::build::CppShim::new().compile();
}
```

What this does:

- Compiles `ueforge/cpp/imgui/` and `ueforge/cpp/ueforge_ui.cpp`
  into a static lib that cargo links into your cdylib.
- Compiles `ueforge/cpp/ueforge_shim.cpp` (and its dependencies
  `ueforge_cppusermodbase.hpp`, `ueforge_imgui_bridge.hpp`) into
  the same static lib.
- Adds the linker flags needed to find `UE4SS.lib` (the import
  library generated from the user's installed `UE4SS.dll`).

Always-safe C++ (imgui + the UI bridge) is also compiled when
your crate is built as a test or build-script binary, so unit
tests and `cargo deploy` runs don't need UE4SS.dll on PATH.

### `UE4SS.lib` symbol presence check

The build script does a naive byte-scan of
`ueforge/ue4ss/UE4SS.lib` for the two MSVC-mangled symbols
`ueforge_shim.cpp` imports:

- `?get_current_imgui_context@UE4SSProgram@RC@@`
- `?get_current_imgui_allocator_functions@UE4SSProgram@RC@@`

If either is missing, the framework build panics with a clear
message pointing at the regen path instead of failing at the
consumer cdylib's link step. `.lib` files are `ar` archives where
mangled names appear as literal substrings, so no `dumpbin`
dependency is required.

## Cargo.toml

ueforge requires three identical lines:

```toml
[dependencies]
ueforge = { path = "../ueforge" }

[build-dependencies]
ueforge = { path = "../ueforge" }

[dev-dependencies]
ueforge = { path = "../ueforge" }
```

Same path three times. There are no feature flags. All public
surface is always available (the build script compiles a single
static lib that lands in every binary that links ueforge.
cdylib, test binary, build-script binary alike).

Plus a `[package.metadata.ueforge]` block that the deploy CLI
reads:

```toml
[package.metadata.ueforge]
mod_name        = "Grounded2Mod"
game_name_regex = "^Grounded\\s*2$"
game_sub_path   = "Augusta\\Binaries\\WinGRTS"
target_dir      = "target/grounded2-mod"
```

`target_dir` gives the mod its own build tree.

It is NOT what stops two cdylibs colliding, and relying on it for
that bit hard on 2026-08-26. `target_dir` only applies when
building through `cargo deploy`, which passes `--target-dir`. A
plain `cargo build -p <mod>` writes to the shared workspace
target, so when every mod declared `[lib] name = "main"` they all
produced `target/<triple>/release/main.dll` and overwrote each
other. Cargo then considered the losing crate fresh and never
rewrote the file, so a deploy shipped whichever mod built last.
The Grounded 2 mod went into MISERY and crashed it on load, with
no mod log line to explain why.

**Give every cdylib its own `[lib] name`** (`misery_mod`,
`grounded2_mod`, `outworld_station_mod`). UE4SS still wants
`main.dll` in the mod's dlls folder, so that is the DEPLOYED
name, applied at copy time by `cargo deploy` and the per-mod
restart scripts. `cargo deploy` reads the crate's cdylib target
name from `cargo metadata` rather than assuming `main.dll`.

## Deploy

The `ueforge-deploy` binary (a `[[bin]]` target inside `ueforge` itself) exposes `cargo deploy ...` via
the workspace `.cargo/config.toml` alias.

Three modes:

```sh
# Default for distribution: build + package zip in dist/
cargo deploy package -p your-mod

# Dev iteration loop: build + copy DLL into the game install
cargo deploy install -p your-mod

# Remove from local install
cargo deploy uninstall -p your-mod
```

Flags:
- `--game-path '<root>'`. Override autodetect.
- `--skip-build`. Redeploy an already-built DLL.

Autodetect walks every Steam library and matches against
`game_name_regex`. The mod lands in
`<game-install>\<game_sub_path>\ue4ss\Mods\<mod_name>\dlls\`.

`mods.txt` registration (`<mod_name> : 1`) is handled by the
deploy CLI.

## On-disk layout after `cargo deploy install`

```
<game install>\<sub_path>\ue4ss\Mods\<mod_name>\dlls\
  main.dll                # the cdylib
  <mod_log_file>          # ueforge::log writes here
  settings.json           # if your mod uses ueforge::settings
  saves/                  # if you use ueforge::rpg::SlotStore
    <slot>.json
```

`dll_dir()` resolves to the `dlls\` folder; every relative path
in your mod (`Settings::load`, `SlotStore::path`) anchors there.

## Shutdown discipline

`on_shutdown` runs in two scenarios:

1. **Process exit**. UE shuts down, UE4SS unloads mods. UObjects
   may already be torn down; minimal cleanup is sufficient since
   the OS reclaims the address space anyway.
2. **Hot-reload** (`Ctrl+R`. See "Hot-reload" below). UE is
   still running, GObjects is still live, and after our
   `on_shutdown` returns UE4SS will call `FreeLibrary` on our
   DLL. This is the strict case.

What `on_shutdown` MUST do (hot-reload-correct):

- **Uninstall every `ProcessEventHook`** before returning. Hooks
  patch UE class vtables; if the DLL is unloaded with patches
  still in place, the next call into the patched slot jumps
  into freed memory and crashes the game.
- **Stop every spawned thread** (HTTP listener, pollers,
  workers, settings watchers) and wait for them to exit. The
  DLL cannot be unloaded while any of its code is on a thread's
  stack.
- **Drain in-flight PE trampolines.** The hook layer's
  `active_calls` counter is checked + spun-on (with a bounded
  timeout) so trampolines that fired before the swap-back
  finish before unload.

What `on_shutdown` should NOT do:

- Touch the engine's UObjects in ways that allocate / hand off
  ownership; the engine may not survive the operation if it's
  also tearing down.
- Write to settings or save files unless already in consistent
  state. Persistence happens transactionally via
  `Tracker<A>::spend_skill_points` etc.
- Call `process_event` from off-thread; the same constraints as
  steady-state apply.

## Hot-reload (Ctrl+R)

UE4SS supports cpp-mod hot-**update** natively. UE4SS does a full
`FreeLibrary` + fresh `LoadLibraryExW` from disk, so whatever
`main.dll` is on disk at reload time becomes the live image.

**The locked-DLL gotcha (verified empirically 2026-05-10).**
While UE4SS has `main.dll` loaded, you **cannot** open it for
write directly -- `LoadLibraryExW` opens with
`FILE_SHARE_READ | FILE_SHARE_DELETE` (no `FILE_SHARE_WRITE`),
so a naive `fs::copy` over the file fails with sharing
violation. But Windows DOES allow renaming or deleting a
loaded DLL (because `SHARE_DELETE` is granted). That's the
opening for the side-file pattern below.

### The side-file pattern (`main-new.dll`)

Verified empirically 2026-05-10. Every step succeeds:

```
deploy time (game running, main.dll loaded):
  cargo deploy install
    -> writes main-new.dll  (NEVER locked: no live image at that path)
    -> first deploy ever:    writes main.dll directly (path doesn't exist)

Ctrl+R time (mod's on_shutdown runs first, OUR DLL still loaded):
  ueforge_mod_shutdown
    -> teardown hooks, join threads, drain trampolines
    -> if main-new.dll exists:
         rename main.dll     -> main-old.dll   (SHARE_DELETE allows)
         rename main-new.dll -> main.dll       (vacant target, plain rename)
    -> return

UE4SS continues:
  ~CppMod()
    -> FreeLibrary(old image)            (main-old.dll now deletable)
  setup_mods()
    -> LoadLibraryExW(main.dll)          (loads the new image)
  start_cpp_mods()
    -> start_mod() -> on_unreal_init     (new code runs)

new DLL's init:
  try_remove(main-old.dll)               (best-effort cleanup)
```

Why this is clean:
- Deploy never has to handle sharing violations: `main-new.dll`
  is always unlocked. Multiple deploys before a Ctrl+R just
  keep overwriting it.
- The mod owns the swap timing: it runs as the last step of
  `on_shutdown`, after Phase B teardown has uninstalled hooks
  + joined threads. If anything in shutdown fails, we can
  log + skip the swap; UE4SS reloads the same image; user
  sees an error and can retry.
- No `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)` games. The
  `.old` file becomes deletable as soon as UE4SS calls
  `FreeLibrary` (next line of UE4SS code after our return).
  The new init cleans it up.
- State on disk (save slots, settings, catalog progress)
  survives. They live in the dlls dir alongside main.dll,
  not inside it; new image reads them on activation.

### Status (2026-05-10). Phase B complete

All five phases (B0-B5) shipped 2026-05-10. The `ueforge_mod_shutdown`
path now does:

1. Game's `on_shutdown` callback (game-specific cleanup. E.g.
   stop pollers).
2. `hook::shutdown_all()`:
   - Sets `process_event::SHUTTING_DOWN = true` so new
     trampoline fires forward to the engine's original
     ProcessEvent without entering our handler.
   - Drops every `ProcessEventHook` registered via
     `hook::register` / `register_many` /
     `install_immediate_or_log`. Each Drop restores the
     vtable slot + spins up to 500ms for in-flight
     trampolines (tracked via per-Entry `active_calls`
     counter) to drain.
3. `server::shutdown_all()`: calls `Server::unblock` + joins
   each registered HTTP listener thread.
4. `settings::shutdown_all()`: stops + joins every
   `Settings::watch` mtime poller registered at watch-spawn
   time. Necessary because UE4SS calls `FreeLibrary` after we
   return; a watcher thread still on a stack inside our DLL
   would crash on its next iteration.
5. `mod_main::finalize_hot_reload_swap`: renames `main.dll`
   -> `main-old.dll`, `main-new.dll` -> `main.dll`. Rolls
   back step 1 if step 2 fails.

After this returns, no code in our DLL is on a thread's stack
or a vtable slot; UE4SS can `FreeLibrary` safely. New image's
first `on_unreal_init` calls `cleanup_old_dll` to remove the
leftover `main-old.dll`.

### Dev loop (Phase B complete)

```
1. edit Rust
2. cargo deploy install -p grounded2-mod
   -> writes main-new.dll (UE4SS still has main.dll mapped)
3. alt-tab to game, press Ctrl+R
   -> UE4SS calls our ueforge_mod_shutdown
      -> game on_shutdown
      -> hook::shutdown_all (vtable restores + trampoline drain)
      -> server::shutdown_all
      -> settings::shutdown_all (watcher threads stop + join)
      -> finalize_hot_reload_swap (main-new.dll -> main.dll)
   -> UE4SS FreeLibrary's old image
   -> UE4SS LoadLibraryExW's main.dll (new image)
   -> our new init runs
4. test the change
```

End-to-end: ~1-2s per iteration. State on disk (save slots,
settings, catalog progress) survives. Hot-update is the dev
loop.

### Configuration

`UE4SS-settings.ini`, `[General]` section:

```ini
EnableHotReloadSystem = 1
HotReloadKey = R              ; default; binds to Ctrl+<key>
```

Both default-on in modern UE4SS builds. Verify in your install.

### Mechanism

`Ctrl+R` triggers `UE4SSProgram::queue_reinstall_mods()` on
UE4SS's event-loop thread. The flow:

1. `uninstall_mods()` -> calls each cpp mod's exported
   `uninstall_mod(m_mod)` -> `~UespyMod()` -> ueforge's
   `ueforge_mod_shutdown` extern "C" -> the game crate's
   `MOD_INFO.on_shutdown` callback. **This is the cleanup
   window.**
2. `m_mods.clear()` -> `~CppMod()` -> `FreeLibrary(main.dll)`.
   Our DLL detaches from the process.
3. `setup_mods()` rescans `mods.txt` + `LoadLibraryExW`s the
   on-disk `main.dll` (the new one, if you `cargo deploy
   install`-ed in step 0).
4. `start_cpp_mods()` -> `start_mod()` -> fresh `UespyMod` ->
   `on_unreal_init` -> our worker init runs again.

### Dev loop

```sh
# 1. Edit Rust code.
# 2. Build + deploy without closing the game:
cargo deploy install -p grounded2-mod

# 3. Alt-tab to game, press Ctrl+R.
# 4. Tail the log to confirm the new init ran:
tail -f "<game-install>/.../BetterBackpack/dlls/grounded2_mod.log"
```

UE4SS's event loop is distinct from the game's UE thread, but
the file-replace works because UE4SS holds the DLL handle, not
the game; deploying overwrites `main.dll` while UE4SS still has
it mapped, then Ctrl+R triggers the FreeLibrary that releases
the old image.

### What survives

- **Cached UE references** (`ClassRef`, `&UFunction` pointers,
  `UClass*` caches): point INTO the game process, not into our
  DLL. New DLL re-resolves on first use; values match (same
  GObjects).
- **Skill state, settings, save files**: live on disk.
  `Tracker<A>` reloads from `<DLL_dir>/<subdir>/<slot>.json`
  on the next slot activation.

### What resets

- Every `static` in our DLL (atomics, OnceLocks, Mutexes) goes
  away with the DLL image. New DLL starts fresh.
- Worker threads exit during `on_shutdown` and are re-spawned
  by the new DLL's init.
- Counter atomics reset to zero. (For perf comparisons across
  reloads, snapshot before + after.)

### What requires care (Phase B work)

The framework's `on_shutdown` plumbing tracks every installed
`ProcessEventHook` in a static registry; reload calls
`HookRegistry::shutdown_all()` before returning to UE4SS. Each
hook drops -> swaps the vtable slot back atomically -> waits
for in-flight trampolines to drain (bounded timeout, typically
500ms). See `ueforge::hook::HookRegistry` (pending) and
`docs/todo.md` "hot-reload" for the implementation plan.

Until Phase B lands, hot-reload of mods that install PE hooks
(kill/fall/inv) is **unsafe**. The old DLL's vtable patches
remain pointing into freed memory. The reload-as-test cycle
works fine for code paths that don't involve PE hooks; for
hook-touching changes, restart the game.

## `dll_dir_wait` happens-before

`log::dll_dir()` reads the atomic that DllMain populates with the
DLL's load path. Workers spawned from `on_unreal_init` MAY race
DllMain on early UE4SS init paths. `log::dll_dir_wait(timeout)`
polls the atomic every 1ms up to the configured timeout; use it
from any worker that needs `dll_dir()` before user code is sure
DllMain has run. The non-blocking `dll_dir()` is fine once the
main worker has reached steady state.

## Logging

`ueforge::log!` formats and writes one line:

```rust
ueforge::log!("rpg: applying {} at level {}", skill_id, level);
```

Output:
- The configured `log_file` in `dll_dir()`.
- The console (if `console: true` in `ModInfo`).

The logger flushes per line; a crash mid-write does not lose the
preceding lines. Logging is cheap but not free. Gate verbose
trace logging behind `cfg!(debug_assertions)` (release builds
compile out the `format!` entirely).

## Feature flags (Cargo)

ueforge has none. You may use cargo features in your own crate
to opt in/out of:

- `console`. Whether the AllocConsole call runs at startup.
  Recommend `default = ["console"]` for dev and a release variant
  that ships without it.
- Diagnostic surfaces: any `cfg!(debug_assertions)` paths.

Don't use cargo features to gate ueforge functionality. The
framework is one crate, no features, by design. Features lead
to combinatorial test matrices and "doesn't build for me"
issues. Ship one ueforge that works for everyone.

## Cross-references

- [ue-sdk.md](ue-sdk.md). The types you'll use inside `worker()`
- [hooks.md](hooks.md). How to install hooks from `worker()`
- [imgui.md](imgui.md). Writing tab render bodies
- [http.md](http.md). Starting the debug HTTP server
- [worker.md](worker.md). Thread spawn details
- [settings.md](settings.md). The settings JSON pattern
- [native.md](native.md). What the C++ shim is doing under the
  hood and why it has to exist
