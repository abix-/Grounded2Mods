# Companion app prior art: Rust+ Desktop

Notes on [`Pronwan/rustplus-desktop`](https://github.com/Pronwan/rustplus-desktop)
and what of it applies to modforge. The Rust+ Desktop app is an unofficial
Windows companion built on Facepunch's Rust+ Companion API. It is a separate
process that talks to the game over an out-of-band protocol (FCM + WebSocket)
and surfaces a rich UI on top.

modforge mods take the opposite approach: the control plane is *embedded* in
the game process, and the UI is whatever HTTP client wants to consume it
(currently `modforge-deploy`, browser tabs, tests). Even so, a lot of the
product surface in Rust+ Desktop is directly portable to "what does the
companion side of a modforge mod look like once you stop treating it as a
debug console."

This doc is a feature inventory and a mapping table, not a plan. The point
is to make sure that when we start fleshing out companion-side UX for ueforge
or unityforge mods (or for Horsey, Grounded 2, Outworld, Schedule 1), we have
a reference of what a mature companion app looks like.

## Why it is relevant

- modforge already has the substrate Rust+ Desktop needs: an HTTP control
  plane, an OpRegistry, selectors, snapshots, settings persistence, counters,
  a ring-buffered event log, and hot reload. See
  [`runtime-control-http`](../../../.claude/skills/runtime-control-http/SKILL.md)
  for the philosophy.
- The companion (Rust+ Desktop) and the embedded server (modforge) are two
  halves of the same picture. Rust+ Desktop has to reverse-engineer a
  protocol; a modforge mod gets to define one. Everything downstream (map
  overlays, event timers, chat alerts, device groups) is a UX layer on top
  of "subscribe to events, read state, drive ops."
- A lot of the Rust+ Desktop work is rediscovering primitives we already
  have. Cache, dead-reckoning, single-instance, tray, auto-update,
  notifications. The interesting bits are the *product* decisions on top.

## Feature -> modforge mapping

### Core / process

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| Bundled runtime installer (.NET + Node + WebView2 + CLI) | `modforge-deploy` packaging | We ship a single Rust binary + injected DLL. No Node, no WebView. |
| Auto-update with progress (speed/size/%) | not present | `modforge-deploy` updates the mod DLL but has no signed-installer flow. Worth adding for end-user mods. |
| Single instance via Named Pipes; `rustplus://` deep links refocus | not present | A `modforge://` URL handler that focuses an existing companion window would be useful once we have one. |
| System tray + auto-start minimized | not present | Companion-side concern. |
| FCM listener with hardening, freeze-proof reconnect | HTTP server + WS event stream | We control both ends. No FCM equivalent needed; events are pushed over WS or pulled from the ring buffer. |
| Proactive token expiry warning | settings + counters | Trivial to surface in a tab. |

### Map / spatial

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| Dynamic map of dynamic markers (cargo, heli, chinook, players, shops) | snapshots + selectors | Modforge ops can stream entity positions on a tick. The renderer is companion-side. |
| Dead-reckoning interpolation across server lag | snapshot timestamps | Snapshots already carry tick/time. Interpolation lives in the renderer. |
| 60 FPS pan/zoom + cinematic transitions | not applicable | Renderer policy. |
| Smart map follow (lock camera to target) | selector + projection | "watch entity X" is a selector subscription. |
| Event dock sidebar | event ring buffer | We already log events; UI is grouping + filtering. |
| Map overlay drawing + share with team | not applicable for SP games | Grounded 2, Horsey, Outworld are single-player. Schedule 1 is too. |
| In-game minimap + crosshair overlay | UE/Unity overlay widget | This is *engine-side*, not companion. Lives in ueforge/unityforge. |
| Custom crosshair editor with pixel tools | not present | Standalone tool; could ship as a companion tab. |

### Event timing / learning

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| Cargo route learning per server/wipe with persisted timings | settings + per-save state | The pattern is: observe an event sequence, persist deltas, predict next occurrence. We have the storage primitives. |
| Live countdowns on map markers | snapshot field | Just a derived timestamp. |
| Chat notifications for event arrival/departure | engine-side toast | UE/Unity engine call from a trigger handler. |

### Devices / automation

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| In-game pairing -> instant UI | hot reload + def registry | New defs (skills, items, entities) show up after a hot reload tick. |
| Share devices/device groups with team | not applicable | SP games. |
| Storage monitor recognition + chat command surface | ops + selectors | "Monitor inventory X" is a selector subscription. |
| Smart Switch chat commands (`!toggle`, `!on`, `!off`) | RPG triggers / op aliases | Op shortcut bindings + chat parser in the engine layer. |
| Configurable smart alarms (popup + audio) | counters + event stream | We can dispatch an op when a counter crosses a threshold. |
| Global device hotkeys, device grouping with bulk control | ops + composition | "Group" = a saved selector + a multi-op. |

### Shops / economy

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| WebView2 shop search (buy/sell orders) | not applicable per-game | Domain-specific. The lesson is: embedded HTML view tab is fine for complex search UIs. |
| Profit trade analytics, trade-route search | derived snapshot views | Just a query over inventory snapshots. |
| Shop alarm system (back-in-stock, suspicious-disappearance) | counters + triggers | Same shape as device alarms. |
| Smart shop clustering | renderer concern | Companion-side. |
| Offline icon cache (SHA1-hashed) | asset cache | We have nothing comparable; if companion needs item icons, copy this pattern: SHA1 -> filename, content-addressed, GC by LRU. |

### Player intelligence

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| 12-week activity grid + 24h heatmap | counters + persistence | Aggregate counter samples to disk on a long tick; render in a companion tab. |
| Player list with custom groups + colors | UI tab + settings | Settings module already handles JSON + debounce. |
| Online status + play time on cards | counters | Each is one counter. |
| Steam profile sidebar | not applicable | |
| BattleMetrics integration | not applicable | |

### Chat / notifications

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| Team chat parsing + auto-replies | engine-side hook | Game-specific chat plumbing. |
| Spawn/death/online/offline/shop event posts | event ring + engine toast | Same surface as everything else. |

### UI architecture

| Rust+ Desktop | modforge analogue | Notes |
|---|---|---|
| WPF MVVM (`Views/`, `ViewModels/`, `Services/`, `Models/`) | `ui` module (declarative tabs) | Our tab API renders per-framework. Per-platform companion (web vs native) is open. |
| WebView2 for the search UI | not present | A browser tab against the HTTP server gives us this for free. |

## Patterns worth stealing

1. **Content-addressed asset cache.** SHA1 -> filename for any icon, image,
   or static asset the companion needs. Bandwidth-free reload, easy GC.
2. **Per-save learned timings.** Mods often want to predict "when does X
   happen again." Persist deltas under a save-scoped settings namespace and
   surface a prediction on a snapshot field.
3. **Event dock as a first-class UX primitive.** Our event ring is currently
   debug-grade. A companion event dock with "click to follow" needs:
   stable event IDs, an entity ref per event, and a "follow" op.
4. **Chat-command bindings to ops.** Many mods want "type `!x` in game to
   run op `Y`." A standard chat parser that targets `OpRegistry` would
   replace a lot of bespoke triggers.
5. **Single-instance + URL handler.** Once we have a companion exe, this is
   the floor for not-annoying UX.
6. **Token / connection expiry warnings as InfoBars.** modforge mods have
   the equivalent: target version drift, attach lost, hot reload failure.
   These should surface in a companion sidebar, not in logs.

## Patterns we should not steal

- **FCM-style out-of-band push.** We embed in the process. WebSocket from
  our own HTTP server is fine.
- **Node.js + JS CLI shim.** We have a Rust client crate and `modforge-deploy`.
- **Mixed-language UI strings.** Their known issue list flags this. Worth
  noting because we already have selector grammars and op names that lean
  toward English-only.

## Where this lands

- Engine-side overlays (minimap, crosshair, chat parser) belong in
  ueforge / unityforge, not modforge.
- Companion-side UX (event dock, player heatmaps, asset cache, tray) lives
  in a future `modforge-companion` crate or in `modforge-deploy`.
- Cross-cutting primitives that are missing from modforge today:
  - Content-addressed asset cache.
  - Per-save settings namespace (we have global; need save-scoped).
  - Chat-command -> op parser.
  - Auto-update protocol for the mod DLL.

Source repo: [`Pronwan/rustplus-desktop`](https://github.com/Pronwan/rustplus-desktop).
