# Synthetic input: prior-art research

> **Status:** in progress (started 2026-05-16). Driving doc for
> [P0. Synthetic input](../../docs/todo.md#p0-synthetic-input-let-the-mod-and-claude-drive-the-game-like-a-human-2026-05-16).
> Mechanism: harvest existing solutions for synthesizing mouse + keyboard
> input INTO a running game / GUI app, decide which L1 (OS-global), L2
> (per-window message), and L3 (game-internal poke) shapes to adopt for
> `modforge::input`. Research-only; no code lands until PR-1..PR-10 are
> done and a recommendation is on file at the bottom of this doc.

## Implementation status (2026-08-27)

End state after the 2026-05-16 session: **all three layers shipped + smoke green**.

| Slice | Commit | What landed |
|---|---|---|
| I-1: primitives (L1, L2, traits) | `e344c5dd` | `modforge::input::{l1, l2, ops}` + `Backend`/`Button`/`Key`/`InputSurface`. Hand-rolled over existing `windows-sys` (no new deps). |
| I-2: HTTP cmdlets (first slice) | `e344c5dd` | `input.mouse.{move,click}`, `input.key.{down,up,press}`, `input.cursor.get`, `input.foreground.hwnd`. Wired through horsey-mod at attach. |
| I-2a: HWND-by-PID | `4f6c6dda` | `input.find_hwnd_by_pid` + `input.self.hwnd` so L2 tests can target the game window regardless of desktop focus. |
| I-5: smoke harness | `befe13ad` | `horsey-mod/tests/input_smoke.rs`. End-to-end "Claude synthesizes input -> game receives it" path verified against running Horsey. |
| Sleuth fix | `befe13ad` | `sleuth.rs:613` `hint_rva` fallback bug (off by `0x40_00_00_00` on pre-rebased VAs at the x64 preferred base `0x140000000`). |
| I-2c: drag/scroll/combo | `f7706861` | `input.mouse.{drag,scroll}` + `input.combo`. L1 + L2 backends; drag interpolates `steps` over `duration_ms`. |
| I-2d-recon: HK1 calibration probe | `8803ee0c` | `horsey-mod/tests/input_hk1_calibration.rs`. 5-point screen<->game-coord mapping capture; runs in-save. |
| I-4: L3 surface + HorseyInputSurface | `6796b7bc` | `Backend::L3` + `set_input_surface`/`input_surface` slot + per-cmdlet `l3_or_fallback`. `HorseyInputSurface` v1 writes `LOC+0x174`/`+0x178` cursor floats directly. Graceful degrade when no save loaded. |
| Unit tests | `054053d3` | 8 parser tests (`Backend`/`Button`/`Key`). |
| I-6: live action journal | `d18f3726` | Versioned operation actions, observed-value waits, and assertions in `modforge::client::live_journal`; recorded, saved, loaded, replayed, and restored live against MISERY movement speed. |
| I-7: waypoint routes | `d331b31d`, `96242512` | Versioned world-space routes, recorded travel steps, trail reduction, closed-loop steering, relative mouse input, stuck evidence, and journal-owned route actions. Five unit tests and the 9.31-second live MISERY outward-and-return replay are green. The waypoint-graph A* added in this slice is rejected as MISERY's pathfinding authority. |
| I-8: engine navigation route discovery | `2db361e5` | UFunction layout discovery, live endpoint projection, Unreal player-controller navigation, automatic diagnostic sampling, nearby-door diagnosis, bounded interaction recovery, and a cold MISERY spawn-to-expedition proof. The final cold run saved exactly three stop waypoints and two travel steps, opened the bunker door once, entered the expedition once, and completed in 23.94 seconds. |
| I-9: live target discovery and looting | in progress | Exact pushed build `b9d41c7f` entered the expedition scan-free, rejected two unreachable crates, selected the lowest-cost reachable `BP_StashMid_C`, and traversed its route. Remaining: aim through player-controller yaw and pitch input when foreground mouse capture is absent, then open and loot the retained crate. |
| I-10: player-input route execution | replacement open | A restarted MISERY test proved the Unreal console-command experiment did not move the player. That implementation was removed. Ueforge now rejects player commands until the local player's real `APlayerController::InputKey` path is implemented and proven live. |

**Cmdlets shipped** (14 total under `input.*`):

```
input.mouse.move    input.mouse.move_rel    input.mouse.click    input.mouse.drag    input.mouse.scroll
input.key.down      input.key.up         input.key.press
input.axis          input.combo
input.cursor.get    input.foreground.hwnd    input.find_hwnd_by_pid    input.self.hwnd
```

**Final crate decision: DID NOT adopt `enigo`.** Cargo.toml inspection showed it pulls the `windows` crate alongside our existing `windows-sys`. Hand-rolling L1 over `windows-sys` was ~150 LOC for zero new deps. Decision overrides the PR-7 recommendation; rationale preserved as a lesson: the original recommendation was based on stars/license/maintenance, the final call factored in the dependency-tree side effect that only became visible during Cargo.toml inspection.

**Engine-internal findings** (Horsey, from decomp pass):

- Per-frame input pump combines `PeekMessage`/`DispatchMessage` (WndProc) + `GetCursorPos`/`GetAsyncKeyState(1..6)` polling. L1 `SendInput` already feeds both paths; L2 PostMessage covers WndProc only.
- `RegisterRawInputDevices` + `GetRawInputData` present but secondary (probably camera mode).
- Engine-side mouse-state struct accessed via `FUN_14018c5c0()` (still-unresolved accessor); writing into it bypasses the pump entirely. Deferred to v2 of `HorseyInputSurface`.
- Keyboard modifier flags at `FUN_140183330(0)+0xe1` (LSHIFT), `+0xe5` (RSHIFT), `+0xe3` (LWIN), `+0xe7` (RWIN). General keys route through `FUN_140183990`. Deferred to v2.

**Open items for future sessions:**

- Discover and loot the nearest placed MISERY expedition box with the box itself as the target waypoint.
- HorseyInputSurface v2: direct mouse-state struct writes + direct keyboard buffer writes (per the engine-internal findings above).
- HK1 Shift+Click transfer migration. Now unblocked by L3; pending an in-save smoke run.
- Cross-game proof: grounded2-mod or schedule1 ships its own `InputSurface` impl.

## TL;DR

**Three-layer architecture validated by prior art:**

- **L1 (Win32 OS-global):** depend on **`enigo`** (`enigo-rs/enigo`, 1715 stars, MIT, edition 2024, last commit 2026-03-30). Pulls `windows` crate, `SendInput`. Use `default-features = false` (default features include Linux-only `x11rb`). Set `dwExtraInfo` tag so our own hooks can recognize self-sent events.
- **L2 (Win32 per-window):** hand-rolled `modforge::input::win32_message` over `windows-sys`. ~80 LOC. Mirrors pywinauto's `click()` (vs `click_input()`). Uses `PostMessage`/`SendMessage` to the game HWND.
- **L3 (game-internal):** per-game `InputSurface` trait. For UE games (Grounded 2), route through `APlayerController::InputKey(FInputKeyParams)` per UnrealCV / Lyra. For Unity, hook `Input.GetKey*` or use `InputSystem.QueueStateEvent`. For raw-Win32 (Horsey), discover and poke the engine-private mouse / key globals.

**Why not other crates:** `rdev` carries a listen-side hook we don't need; `autopilot` carries an `image` dep; `winput` is small and clean but 21 stars and inactive; `mouce` is mouse-only; `InputBot` is too opinionated.

**Input replay format:** JSON event stream with absolute `t_ms`, `type` enum, `backend` selector in context. See PR-9.

**Live action journal:** injected games are not deterministic simulations owned by Modforge. Their journal records semantic control-plane operations, then waits for observable conditions and asserts the resulting values. Raw timed input remains one operation type, not the authority for progress.

**Bot navigation:** the route, A*, waypoint, door, loot-target, and player-input design is owned by [bot-navigation.md](../../docs/bot-navigation.md). This document owns input mechanism research only.

**Anti-cheat:** all current targets are LOW risk. Tag `dwExtraInfo` and restore foreground anyway (cheap insurance).

**Implementation path forward:** I-1 (modforge::input scaffolding with `enigo` + `windows-sys::PostMessage`) is unblocked; I-4 (L3 poke) gets defined per consumer mod, with horsey-mod's I-R steps (per-game) driving the first concrete `InputSurface` impl.

---

## Method

All searches use the `gh` CLI (per user direction). Each PR-N section
captures: search terms tried, top hits with stars/license/last-commit,
mechanism (L1/L2/L3), API shape excerpt, fit notes.

A repo qualifies for "evaluate further" if: maintained within the last
~2 years OR battle-tested historical (>1000 stars), permissively
licensed (MIT/Apache/BSD/ISC/Zlib; copyleft flagged but not auto-out),
and either Windows-supporting or pattern-portable.

Search lexicon used across PRs:

- L1 keywords: `SendInput`, `mouse_event`, `keybd_event`, `INPUT struct`
- L2 keywords: `PostMessage`, `WM_LBUTTONDOWN`, `WM_KEYDOWN`, `WM_INPUT`
- L3 keywords: per-engine (UE: `FSlateApplication`, `UPlayerInput`,
  `FInputKeyParams`; Unity: `Input.GetKey`, `InputSystem`; SDL: `SDL_PumpEvents`)
- Anti-cheat-friendly synonyms: `humanize`, `bezier`, `interception driver`
- Replay: `input replay`, `macro record`, `event playback`

`gh search code` rate-limits aggressively; we batch reads and cache
findings in this file rather than re-querying.

---

## PR-1: Game-mod input libraries

_Status: first pass complete (2026-05-16)._

### Search terms

| Query | Hits surveyed | Notes |
|---|---|---|
| `gh search code "PostMessageW WM_LBUTTONDOWN" --language=rust` | 1 (`ekicyou/areka`) | small, demonstrates the exact L2 shape we want |
| `gh search code "FInputKeyParams" --language=cpp` | 8 (UE forks, UnrealCV, Lyra, GSS) | confirms `UPlayerInput::InputKey(FInputKeyParams)` as canonical UE L3 |
| `gh search repos "rust input simulation windows"` | empty | covered by named-crate dive in PR-7 |

### Top candidates

| Repo | Stars | License | Last commit | Mechanism | Notes |
|---|---|---|---|---|---|
| `unrealcv/unrealcv` | 2178 | MIT | 2026-04-15 | L3 (UE-internal) | HTTP server BURIED in a UE plugin, exposes `vset /action/keyboard <key> <dt>` and routes through `PlayerController->InputKey(FInputKeyParams)`. Same architecture as us. |
| `gamaspacestation/gss_release` (Lyra fork) | n/a | Epic source | n/a | L3 (UE-internal) | Epic's own `ULyraSimulatedInputWidget::InputKeyValue` calls `UEnhancedPlayerInput::InputKey(FInputKeyParams)`. Authoritative reference. |
| `bl-sdk/oak2-mod-manager` | n/a | MIT | recent | L3 (UE-internal) | Borderlands 3 / Oak2 modding SDK; uses keybinds inside UE process. |
| `ekicyou/areka` | n/a | n/a | n/a | L2 (Win32) | Smallest Rust example of `PostMessageW(WM_LBUTTONDOWN)` clicking a target window. Useful as a literal `PostMessage` recipe. |

### Findings

- **Two clear winning architectures:**
  - **In-engine (L3) for UE:** route through `UPlayerInput::InputKey` / `UEnhancedPlayerInput::InputKey`. Works regardless of focus, regardless of raw-input mode, regardless of fullscreen. UnrealCV proves the HTTP-driven shape end-to-end.
  - **Win32 (L1/L2) for raw-Win32 games like Horsey:** `SendInput` (L1) is universal but steals focus. `PostMessage` (L2) targets one HWND, doesn't steal focus, ignored by games that use raw-input or poll `GetAsyncKeyState`.
- **L2 viability is per-game.** Test inside the consumer mod, not at the modforge layer. The modforge `Mouse`/`Keyboard` abstraction must let the consumer pick L1 vs L2 vs L3 per call; default per game.
- **Same-process advantage:** because we're injected, we can call the game's WndProc directly with `SendMessage` (synchronous, returns the LRESULT) instead of `PostMessage` (async, fire-and-forget). Worth A/B'ing for HK1 transfer.

---

## PR-2: UI automation frameworks (concept-mining, not adoption)

_Status: first pass complete (2026-05-16)._

### pywinauto

- Repo: `pywinauto/pywinauto`. **6019 stars, BSD-3-Clause, active (2026-04-13).**
- Key API split (we will copy this verbatim):
  - **`ctrl.click()`** -> `_perform_click(ctrl, ...)` -> `PostMessage(hwnd, WM_LBUTTONDOWN/UP, ...)`. L2. Works on hidden windows; no focus theft.
  - **`ctrl.click_input()`** -> `mouse_event` (L1). "More realistic"; works on raw-input apps; steals cursor.
- Coordinate convention: client-relative by default, `absolute=True` switches to screen-px.
- Chord encoding: separate `pressed=` argument for held modifiers (`pressed="control shift"`). Cleaner than AHK's interleaved syntax.

### AutoHotkey v2

- Repo: `AutoHotkey/AutoHotkey`. **12377 stars, GPL-2.0**, active.
- **GPL-2 is contagious for code linking.** We do NOT vendor AHK code; we extract ideas only.
- Send modes (worth mirroring as a `Backend` enum value): `SendInput` (L1, atomic batch), `SendPlay` (journal-playback hook, deprecated post-Win7), `SendEvent` (legacy `keybd_event` / `mouse_event`), `SendMessage` (per-window WM_KEY/WM_MOUSE). The split matches our L1 vs L2 distinction directly.
- Notable AHK trick: `SendInput` is wrapped with hook-suspend so the script's own hotkey hook doesn't re-trigger. We need the same when we add input-listening later. Use `dwExtraInfo` tag (enigo already exposes this).
- Modifier-chord encoding: `Send "^!{F4}"` for ctrl+alt+F4. Not adopting; pywinauto-style separate modifier list is cleaner for JSON.

### Microsoft UI Automation (UIA)

- Available via `windows`/`windows-sys` crates. **Out of scope** for game input (it targets UIAutomation-tagged controls; games are usually a DirectX/OpenGL surface with no UIA tree).
- May matter later if we extend to non-game GUI testing (the timberbot Python client could use it).

### AutoIt

- Closed-source, well-documented. API shape note: AutoIt's `MouseClick("left", x, y, 1)` is identical to what we want for the HTTP cmdlet shape.
- Not vendoring; cite for API ergonomics.

### Verdict (PR-2)

Adopt pywinauto's L1 vs L2 method split (`click` vs `click_input`) at the modforge level. Adopt AHK's `dwExtraInfo`-tag pattern for marking our own synthetic events. Modifier-chord encoding: pywinauto-style separate list, JSON friendly.

---

## PR-3: Game-bot ecosystems

_Status: first pass complete (2026-05-16)._

### RuneLite

- Repo: `runelite/runelite`. **5375 stars, BSD-2-Clause, active.**
- Architecture: a re-distributable client that injects into the (Java) RuneScape client process and exposes a plugin API. **Same architecture class as us** (in-process mod with side-car HTTP) but on a managed runtime.
- Input layer (Java AWT): `runelite-client/src/main/java/net/runelite/client/input/MouseAdapter.java` + `MouseManager.java`. Plugins subscribe to `MouseEvent` callbacks; bots that synthesize input call straight into the AWT event queue (`Robot.mouseMove` / `Robot.mousePress`). That maps to our L1 (`SendInput`).
- **Pattern to copy:** the `Callbacks` / `Hooks` split. RuneLite has a single `Callbacks` interface implemented by the patched client; the patched client calls back into the modding side for every input event. We have the analogue with the modforge `Hook<F>` retour wrapper.
- **Pattern to NOT copy:** RuneLite enforces no-bot-plugin policy via Jagex contract. Doesn't apply to us.

### OSRSBox / RSPS tooling

- Smaller communities, mostly closed bots. Patterns documented in RuneLite + AHK communities; no incremental info beyond PR-3 above.

### League / Dota / MMO botting

- Mostly closed or GPL with anti-cheat dance. We do not target online competitive games; skip.

### Humanize-the-curve pattern

Common across the botting field: instead of an instant `MoveTo(x,y)`, interpolate over `N` ticks along a Bezier curve with jitter, to defeat naive AC heuristics. Worth a `Mouse::move_humanized { duration_ms, jitter_px }` method as a follow-up; not v1.

### Verdict (PR-3)

- Adopt the `Hook<F>` / `Callbacks` event-style read path for future "Claude observes the game's own click stream" work.
- Defer humanize-curve. v1 is teleport + click.

---

## PR-4: Reverse-engineered input pokes per engine

_Status: first pass complete for UE; other engines pending._

### Per-engine targets

| Engine | Input class(es) | Known RE projects | Status |
|---|---|---|---|
| UE4 / UE5 | `UPlayerInput`, `FSlateApplication`, `FInputKeyParams`, `UEnhancedPlayerInput` | UE4SS, Lyra source, UnrealCV | researched |
| Unity (Mono) | `UnityEngine.Input` | BepInEx samples, MelonLoader docs | pending |
| Unity (IL2CPP) | `UnityEngine.Input` via Il2CppInterop | Il2CppInterop docs, MelonLoader | pending |
| SDL2 | `SDL_PumpEvents`, `SDL_Event` queue | SDL source, official tutorials | pending |
| Raw Win32 (Horsey-shape) | `GetAsyncKeyState`, `GetCursorPos`, custom message-pump | per-game RE | game-specific (I-R) |

### Unreal Engine 4/5

**Canonical L3 path:** `APlayerController::InputKey(FInputKeyParams)` (UE 4.26+) or, with Enhanced Input enabled, `UEnhancedPlayerInput::InputKey(FInputKeyParams)`.

Two confirmed live consumers:

- **UnrealCV** (`unrealcv/unrealcv/Source/UnrealCV/Private/Commands/ActionHandler.cpp`):
  ```cpp
  FExecStatus FActionHandler::Keyboard(const TArray<FString>& Args) {
      // Args[0] = key name, Args[1] = delta time
      const FKey Key(*KeyName);
      float Delta = 1, DeltaTime = FCString::Atof(*Args[1]);
      FInputKeyParams KeyParams(Key, Delta, DeltaTime, /*NumSamples=*/1, /*bGamepad=*/false);
      World->GetFirstPlayerController()->InputKey(KeyParams);
      // schedule a release via FTimerManager
  }
  ```
  Run a TCP/HTTP server inside the game process, expose `vset /action/keyboard <key> <dt>`, and route every command through `PlayerController->InputKey`. Same shape as `runtime-control-http`.
- **Lyra** (`gamaspacestation/gss_release/Source/LyraGame/UI/LyraSimulatedInputWidget.cpp`):
  ```cpp
  FInputKeyParams Params;
  Params.Delta = Value;          // FVector for analog
  Params.Key = KeyToSimulate;
  Params.NumSamples = 1;
  Params.DeltaTime = GetWorld()->GetDeltaSeconds();
  Params.bIsGamepadOverride = KeyToSimulate.IsGamepadKey();
  Input->InputKey(Params);
  ```
  Epic-authored. UMG widget routes a touch event into a fake key press. Confirms the API is the supported one.

**Mouse cursor:** UE's mouse position lives on the `FSlateApplication` singleton (`FSlateApplication::Get().GetCursorPos()`). To synthesize a click, the safest L3 path is to route through `FSlateApplication::OnMouseDown` / `OnMouseUp`, which fires the same dispatch as a real click. For PE games where Slate isn't easily reachable, fall back to `PostMessage(hwnd, WM_LBUTTONDOWN, ...)` with a `lParam` carrying client coords; UE's Win32 platform layer translates them.

**Grounded 2 specifics:** UE5 + Enhanced Input. The Enhanced Input subsystem (`UEnhancedInputLocalPlayerSubsystem::GetPlayerInput()`) returns the right object to feed `FInputKeyParams` to. ueforge already exposes UObject-by-name lookup; calling `InputKey` is one more `ProcessEvent`-style invocation away.

### Other engines (sketch; fill in later)

- **Unity (Mono/IL2CPP):** the `UnityEngine.Input` class is read-only; can't poke it directly. The canonical approach in the BepInEx / MelonLoader ecosystem is to either hook the `Input.GetKey*` family and lie on read, or use a 3rd-party library like InputSystem (new Unity input) and synthesize via `InputSystem.QueueStateEvent`. Schedule 1 / Outworld Station likely fall here.
- **SDL2:** `SDL_PumpEvents` reads OS events; `SDL_PushEvent` queues a fabricated one. We'd target `SDL_PushEvent(&SDL_Event{type: SDL_KEYDOWN, ...})`.
- **Raw Win32 (Horsey):** per-game I-R discovery. Working hypothesis: a small WndProc reads `WM_LBUTTONDOWN` / `WM_MOUSEMOVE` into engine-private globals. PostMessage should work; verify in I-R5.

---

## PR-5: Raw-input / `WM_INPUT` handling

_Status: first pass complete (2026-05-16)._

Raw-input games (any game using `RegisterRawInputDevices` to read mouse / keyboard deltas instead of high-level WM_KEY / WM_MOUSE messages) ignore PostMessage by design. The field has converged on driver-level workarounds:

### Interception driver

- Repo: [`oblitum/Interception`](https://github.com/oblitum/Interception)
- **1849 stars, no SPDX license declared, last commit 2021-08-09.** Effectively abandoned but battle-tested over a decade in the bot / accessibility scene.
- Mechanism: a kernel-mode upper filter driver that inserts itself in the keyboard / mouse driver stack. Synthesized events look identical to physical events to anything downstream, including raw-input.
- Trade-off: requires admin to install the driver (`install-interception.exe /install`) and a reboot. Catastrophic for "drop-in mod"; great for a power-user opt-in.
- **No first-class Rust binding found** in `gh search repos --language=rust "interception"`. Existing `interception-*` crates on crates.io are all for HTTP / syscall interception, not the driver. We'd write our own FFI wrapper around `interception.dll` if we ever go this route. ~200 LOC.
- Status flag: **defer**. Horsey doesn't need it (raw Win32 message-pump expected). Revisit only if a target game ignores PostMessage AND we cannot reach an L3.

### ViGEm (Virtual Gamepad Emulation Framework)

- Repo: [`ViGEm/ViGEmBus`](https://github.com/ViGEm/ViGEmBus) (driver) + [`ViGEm/ViGEmClient`](https://github.com/ViGEm/ViGEmClient) (user-mode SDK).
- For gamepad synthesis only. Out of scope for v1 (no current gamepad-driven targets).

### Anti-cheat flagging

Driver-level input synthesis is a primary pattern flagged by EAC / BattlEye / Vanguard. None of our current targets ship with kernel AC; document and stay clear if any future target does.

### Findings (verdict)

- **L2 PostMessage suffices for raw-WM games (Horsey class).**
- **L3 in-engine suffices for managed-runtime games (UE / Unity).**
- **Interception driver is the third-tier escape hatch.** Lift only if we hit a target that uses raw-input + has no reachable L3.

---

## PR-6: Accessibility / assistive tech

_Status: surveyed but limited yield (2026-05-16)._

A11y tools have to drive arbitrary apps from arbitrary input sources (eye, switch, dwell). They've solved L1 robustness, but for our use case (in-process injection) the lessons are smaller than expected.

### Useful lessons

- **All major a11y stacks default to `SendInput` / `mouse_event`.** Camera Mouse (FOSS, MIT-licensed dwell-click for ALS users) uses `SendInput`. Tobii Stream Engine sample code does the same. Confirms L1 is the right choice when we don't have an in-engine path.
- **Click confirmation is decoupled from click intent.** Dwell-click tools snap the cursor, wait `N ms`, then synthesize. This is a great UX for "show me where I'm about to click" if we add a debug overlay. Not v1.
- **Focus restoration:** the better a11y tools `SetForegroundWindow(prev)` after the click batch so the keyboard returns to whatever it was on. Worth replicating when we use L1 from a remote terminal.

### Verdict (PR-6)

- L1 backend choice (enigo) validated.
- Adopt "restore foreground window" post-batch for L1 clicks initiated from outside the game window.

---

## PR-7: Crate audit (Rust)

_Status: first pass complete (2026-05-16)._

Metadata pulled live from `gh api repos/<owner>/<repo>`. License/stars/last-push are authoritative as of today.

| Crate | Repo | Stars | License | Last commit | Mechanism | Win32 backend | Verdict |
|---|---|---|---|---|---|---|---|
| `enigo` | `enigo-rs/enigo` | 1715 | MIT | 2026-03-30 | L1 | `windows` crate -> `SendInput` | **Top L1 candidate.** Edition 2024. Mouse + keyboard, button-only, abs/rel move, scroll, X-buttons. ~5kb compiled stub. |
| `rdev` | `Narsil/rdev` | 728 | MIT | 2026-05-12 | L1 + listen (low-level hook) | `winapi` crate -> `SendInput` | Active. Listening side (hook callbacks) bigger than what we need; selectable as L1-only via not enabling features. |
| `autopilot` | `autopilot-rs/autopilot-rs` | 420 | MIT/Apache-2.0 | 2025-10-29 | L1 | `winapi` crate -> `mouse_event` / `SendInput` | Edition 2024; dual-licensed. Carries `image` dep we don't want. |
| `InputBot` | `obv-mikhail/InputBot` | 461 | MIT | 2025-08-01 | L1 + global hotkeys (listen) | `winapi` -> `SendInput` + low-level hook | Cute API (`Numrow1Key.bind(...)`); too opinionated for a primitive layer. |
| `winput` | `gymore-io/winput` | 21 | MIT | 2024-01-30 | L1 | `winapi` -> `SendInput` | Small + clean (`Vk`, `Button`, `Action`, `send_inputs`). Easiest to vendor / fork if we go DIY. |
| `mouce` | `emrebicer/mouce` | 54 | MIT | 2025-05-17 | L1 (mouse only) | `winapi` -> `mouse_event` | Mouse only; rules itself out. |
| `windows`, `windows-sys` | `microsoft/windows-rs` | n/a | MIT/Apache | active | raw FFI | both | Always-available. If we go DIY (a la `winput` shape) we'd build directly on `windows-sys`. |

### Findings (verdict)

- **`enigo` is the right L1 dependency.** Most stars, most maintained, edition 2024 (matches our workspace), MIT, uses the modern `windows` crate. Mouse + keyboard + scroll. The `Settings { dw_extra_info, ... }` knob lets us tag synthetic events so we can ignore-our-own when we hook key state later.
- **No existing crate covers L2 (PostMessage).** All Rust input crates we audited target the OS-global L1 path. We will hand-roll a thin `modforge::input::win32_message` module over `windows-sys` to PostMessage / SendMessage to a target HWND. ~80 LOC.
- **No existing crate covers L3 (game-internal).** As expected; this is per-game and goes through ueforge / horsey-mod's resolver. Modforge defines the `InputSurface` trait; consumers implement.
- **Hand-rolled DIY is a real option** if `enigo`'s `windows` crate footprint is unacceptable: `winput`'s 600-LOC shape is the template.

### Cargo footprint check (TODO during I-1)

Confirm `enigo` (no features) doesn't pull `tokio` / `serde` / `wayland` / `x11` on the Windows build. Cargo.toml inspection shows `default = ["x11rb"]` which is Linux-only and won't compile on Windows. We need `default-features = false` on our side.

---

## PR-8: In-process injection precedents

_Status: first pass complete (2026-05-16)._

The closest architectural matches: already-in-process mods that synthesize input. Closest match wins on adoption priority.

### UE4SS

- Repo: `UE4SS-RE/RE-UE4SS`. **2494 stars, MIT, very active (2026-05-16).**
- `gh search code --owner UE4SS-RE "SendInput OR PostMessage"` returned **empty**. UE4SS routes input through UE's own structures, not Win32, which is exactly what we want for grounded2-mod.
- UE4SS exposes Lua bindings for `RegisterKeyDownEvent` (listen side) but the canonical "synthesize a key" path inside a UE4SS mod is `APlayerController::InputKey` (same as UnrealCV / Lyra in PR-4).

### BepInEx / MelonLoader (Unity)

- BepInEx: 7826 stars, LGPL-2.1, active. MelonLoader: 3892 stars, Apache-2.0, active.
- Standard Unity mod pattern: hook `UnityEngine.Input.GetKey*` to lie on read, or use `InputSystem.QueueStateEvent` for the new Unity Input System.
- LGPL-2.1 is link-friendly for our use (we don't statically embed BepInEx; we extract the pattern).
- **Relevant for Schedule 1 / Outworld Station / Timberbot lanes.** Not Horsey or Grounded 2.

### ReShade

- DX11/DX12 overlay; hooks `IDXGISwapChain::Present` for rendering and `WndProc` chain for input capture (Alt+key shortcuts). It synthesizes input only to its own ImGui overlay, not to the underlying game. Useful as a model for **input capture** (PR-9 record side); not for input **synthesis**.

### Special K

- Repo: `SpecialKO/SpecialK`. **1872 stars, GPL-3.0**, active. **GPL-3 forbids us linking.**
- Cite for ideas only. Special K's gamepad-emulation path is a great L3 reference for any future controller work but we're not building that.

### ImGui Win32 backend

- `ocornut/imgui` `backends/imgui_impl_win32.cpp`. The backend swallows `WM_LBUTTONDOWN` etc. when ImGui wants the mouse. We already use ImGui in horsey-mod overlay. Two implications:
  - Our overlay can **intercept** input destined for the game (good for confirmation dialogs).
  - When we synthesize via L2 PostMessage to the game HWND, our own ImGui overlay will see the message in WndProc subclassing order. Need to test ordering; may need to bypass ImGui's WndProc tap when our `dwExtraInfo` tag is present.

### Verdict (PR-8)

- For UE games: route through `InputKey` per PR-4 (UE4SS confirms).
- For Unity games: hook `Input.GetKey*` or `InputSystem.QueueStateEvent` (BepInEx + MelonLoader patterns).
- For raw-Win32 games (Horsey): L2 PostMessage, but **tag with `dwExtraInfo` so our ImGui overlay can recognize and pass-through our own synthesized events.**

---

## PR-9: Replay / record-playback formats

_Status: first pass complete (2026-05-16)._

### Surveyed

| Format | Source | Shape | Determinism | Notes |
|---|---|---|---|---|
| AutoHotkey script | `AutoHotkey/AutoHotkey` (12377 stars, GPL-2) | Imperative text DSL (`Send "^!a"`, `Click 100 200`, `Sleep 50`) | NO (wall-clock sleeps) | Human-editable. Sleeps drift; replays diverge across machines. |
| BizHawk `.bk2` | `TASEmulators/BizHawk` (2655 stars, MIT/custom) | Zip of per-frame input table (rows = frames, cols = buttons) | YES (frame-perfect) | Frame-locked. Requires deterministic engine. Overkill for us; games we target aren't deterministic. |
| FCEUX `.fm2` | `TASEmulators/fceux` (1488 stars, GPL-2) | Same shape as `.bk2`; NES-only | YES | Frame-locked. Same caveat. |
| OBS Replay | OBS Studio | Video, not input | n/a | Cite only for timing model. |
| `xdotool` script | `jordansissel/xdotool` (3794 stars, BSD-3) | Shell commands with explicit delays (`key alt+r`, `mousemove 100 200`, `sleep 0.05`) | NO | Linux-only mechanism (X11 XTEST); syntax is portable. |
| Cheat Engine macros | `cheat-engine/cheat-engine` (18278 stars, no SPDX) | Lua inside table file | NO | Baroque. Skip. |

### Recommended shape: JSON event stream with absolute t_ms

Inspired by `xdotool` (explicit delays, human-readable) plus pywinauto's L1/L2 split:

```json
{
  "version": 1,
  "schema": "modforge.input.replay@v1",
  "events": [
    { "t_ms":    0, "type": "mouse.move",  "x": 500, "y": 400 },
    { "t_ms":   50, "type": "key.down",    "key": "shift" },
    { "t_ms":   80, "type": "mouse.click", "button": "left", "x": 500, "y": 400 },
    { "t_ms":  100, "type": "key.up",      "key": "shift" }
  ],
  "context": {
    "game_window_title": "Horsey",
    "client_size": [1920, 1080],
    "backend": "l2.postmessage"
  }
}
```

Properties:

- **Absolute `t_ms`** from `events[0]`, not deltas. Trivial to verify monotonicity; easy to seek to a timestamp.
- **`type` field** = `mouse.move | mouse.click | mouse.drag | mouse.scroll | key.down | key.up | key.press | combo`.
- **`backend` in `context`** = `l1.sendinput | l2.postmessage | l3.{engine}`. Reproduces the exact path used at record time.
- **`schema` versioned.** Future fields don't break old replays.

### Verdict (PR-9)

- Adopt the JSON shape above for `modforge::input::Replay`.
- The shipped first slice records what the cmdlets execute. The next MISERY slice records at L3 by observing Unreal's normalized key, button, and relative look-axis input together with the player's world transform. Win32 `WH_MOUSE_LL` and `WH_KEYBOARD_LL` capture remains a fallback for games without an accessible engine input path.
- Keep Topside's fixed-tick `actions::Journal` for simulations whose step and seed Modforge owns.
- For injected games, record versioned operation actions, condition waits, and assertions. A replay advances from a wait only when the observation matches or fails with the last observed value. Wall-clock delays bound a wait; they never decide that gameplay completed.

### Bot navigation

The authoritative route, A*, waypoint, door, loot-target, and player-input
design moved to [bot-navigation.md](../../docs/bot-navigation.md). This prior-art document
retains only the input research and implementation history that led to it.

---

## PR-10: Anti-cheat / legitimacy notes

_Status: first pass (2026-05-16). Per-game stances are best-effort from public knowledge; revisit if any target ships an AC._

| Game | Online MP? | AC | Stance on synthetic input | Notes |
|---|---|---|---|---|
| Horsey Game | no (single-player) | none | LOW risk | Fully single-player; no leaderboards. |
| Grounded 2 | co-op only | none observed | LOW risk | Co-op with friends; not a competitive ladder. PvE. |
| Schedule 1 | single-player | none observed | LOW risk | Single-player narrative. |
| Outworld Station | single-player | none observed | LOW risk | Single-player simulation. |
| Timberborn (Timberbot) | single-player | none | LOW risk | Single-player city-builder. |

### Patterns the AC field flags

For future reference if we ever target a competitive game:

- **Driver-level synthesis (Interception)**: flagged by EAC, BattlEye, Vanguard.
- **`SendInput` from a non-foreground process**: heuristically flagged by some AC; safe pattern is to set the game window foreground before the batch.
- **Hidden hardware ID delta** (`INPUT.mi.dwExtraInfo == 0`): some AC checks this. Tag with a fake nonzero `dwExtraInfo` to look like legit driver state.
- **Identical mouse-delta patterns** (perfect Bezier, zero jitter): heuristically flagged. Defeated by humanize-curve (deferred per PR-3).

### Verdict (PR-10)

All current target games are LOW risk. Document the patterns above so we know what to switch on if a future target is competitive.

---

## Recommendation

### Stack

| Layer | Choice | Reason |
|---|---|---|
| L1 | `enigo` 0.6.x, `default-features = false`, Windows-only feature set | Most-starred, most-maintained, MIT, edition 2024, clean `windows`-crate FFI. Beats DIY on bug surface; matches our `Mouse`/`Keyboard` shape. |
| L2 | Hand-rolled `modforge::input::win32_message` over `windows-sys::Win32::UI::WindowsAndMessaging` | No suitable crate exists; ~80 LOC; targeted per HWND; no focus theft. Lets us pick between `PostMessage` (async) and `SendMessage` (sync, since we are in-process). |
| L3 | Per-consumer `InputSurface` trait | Engine-specific. UE: `APlayerController::InputKey(FInputKeyParams)`. Unity: `Input.GetKey*` hook or `InputSystem.QueueStateEvent`. Raw Win32: per-game poke of engine globals (Horsey I-R). |

### Module shape

```rust
// modforge::input  (sketch, lands in I-1)

pub enum Backend { L1SendInput, L2WindowMessage, L3GameInternal }
pub enum Axis { MouseX, MouseY }

pub trait Mouse {
    fn move_abs(&self, x: i32, y: i32, b: Backend) -> Result<()>;
    fn move_rel(&self, dx: i32, dy: i32, b: Backend) -> Result<()>;
    fn click(&self, btn: Button, x: i32, y: i32, b: Backend) -> Result<()>;
    fn drag(&self, btn: Button, from: (i32,i32), to: (i32,i32), dur_ms: u32, b: Backend) -> Result<()>;
    fn scroll(&self, dx: i32, dy: i32, b: Backend) -> Result<()>;
}

pub trait Keyboard {
    fn down(&self, k: Key, b: Backend) -> Result<()>;
    fn up(&self,   k: Key, b: Backend) -> Result<()>;
    fn press(&self, k: Key, hold_ms: u32, b: Backend) -> Result<()>;
    fn type_str(&self, s: &str, b: Backend) -> Result<()>;
}

pub struct Combo { pub mods: Vec<Key>, pub then: Box<dyn FnOnce() -> Result<()>> }

pub trait InputSurface {
    fn name(&self) -> &'static str;
    fn click(&self, btn: Button, x: i32, y: i32) -> Result<()>;
    fn key(&self, k: Key, down: bool) -> Result<()>;
    fn axis(&self, axis: Axis, value: f32, delta_time: f32) -> Result<()>;
}
```

### HTTP cmdlets (I-2)

- `input.mouse.move {x, y, backend?}`
- `input.mouse.click {button, x?, y?, backend?}`
- `input.mouse.drag {button, from:[x,y], to:[x,y], duration_ms?, backend?}`
- `input.mouse.scroll {dx, dy, backend?}`
- `input.key.down|up|press|type {key|text, hold_ms?, backend?}`
- `input.axis {axis, value, delta_time, backend?}`
- `input.combo {keys: [...], then: {...}}`
- `input.state.get {}` -> reads game's own mouse/key state for verification
- `input.replay.play {events: [...]}`
- `input.replay.record.{start,stop}` (deferred to v2)

### Defaults

- `backend` defaults to `l2.postmessage` when a target HWND is known and the consumer hasn't registered an `InputSurface`.
- `backend` defaults to `l3.<engine>` when `InputSurface::register` has been called.
- `dwExtraInfo` tag: `0xC1A5DE_xx` per backend (cute, identifiable in logs).

### Open follow-ups (not blockers)

- Humanize-curve mouse moves (PR-3). Defer to v2.
- MISERY fallback recording of real key, button, relative look-axis, position, and camera samples for paths the Unreal navigation mesh cannot describe.
- Win32 real-input capture via `WH_MOUSE_LL` + `WH_KEYBOARD_LL` for games without an L3 capture surface.
- Interception driver FFI for raw-input-only games. Defer until we hit one.
