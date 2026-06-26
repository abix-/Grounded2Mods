# HK1: Shift+Click Smart-Transfer Plan

> **Scope:** the first and only hotkey in v1 (user-locked 2026-05-15 in `todo.md` -> "High-priority features" -> "Hotkeys"). Shift+Click on a horse infers the right destination (truck <-> pasture <-> race line) from the player's current location and the horse's current container, then performs the transfer.
>
> **Status when this doc was written:** no code yet. Research + sequencing only. This document tells the next session what to build, in what order, with which tests, and where the decomp evidence lives.

---

## REVISION 2026-05-17: world-map decomp pass changes the plan

A 2026-05-17 decomp pass for [`world-map-detection.md`](world-map-detection.md) (Findings 1-5e + [`HORSE-PLACES.md` -> Top-level singletons](HORSE-PLACES.md#top-level-singletons)) surfaced primitives that obsolete several decisions below. Apply this revision before reading the older sections. Where revision conflicts with original prose, revision wins.

**What's better now:**

1. **MapState camera (`*DAT_1403f4e00 + 0x254 / +0x258`, both `float`).** Resolves the deferred "truck moves -> house screen pos changes" problem entirely. World->screen projection is exact:
   `screen_xy = (world_xy - camera_xy) * zoom + screen_offset`. Zoom is likely 1.0 on Horsey; one calibration point + live camera floats determines the affine. Re-projection is per-frame and zero-effort, so the captured `home_door_from_truck_spawn` coords from `menu_targets.json` are no longer a fragile fresh-launch-only anchor. They are one valid sample we can use to derive the affine, then drop in favor of live projection from the TMX object table.

2. **`cursor_input_handler` at RVA `0x14009d750` (1577 B, string anchor `"Pointer"`).** THE cursor dispatcher; every click / drag / hover flows through it. Hook this instead of subclassing the game's WndProc via hudhook. We get correct modifier state for free, and we can choose to either pre-empt the click (own it) or let it pass through, deterministically. Replaces section 4.6.

3. **`enter_location_handler` at RVA `0x1401046c0`** (1004 B, anchors `"EnterLocation"`, `"EnterLocationStable"`) and **`truck_enter_location_handler` at RVA `0x1400cd5a0`** (1326 B, `"TruckEnterLocation"`). Hookable scene-transition events. Eliminates the polling loop in `common::ensure_home_scene_loaded`. The helper becomes "click house door, await event". `enter_location_handler` signature is decoded as `(GS*, int scene_id, char suppress_sound)`.

4. **Scene handler `+0x148` = last-interacted Building**, **`+0x270` = bool modal-visible**. Combined with the active-scene-handler pointer reachable as `*(GS[+0x438] + active_scene_id * 8)`, we can answer "what did the player just click" with zero pixels.

5. **`FUN_1400b4a10(Building*, &out_pair) -> float*`** at RVA `0x1400b4a10`. Canonical Building -> tile-coord accessor. Via `vanilla.invoke` instead of guessing per-building offsets.

6. **Truck object at `*(GS + 0x300)`**, position `+0x28 / +0x2c` (float, scaled by `DAT_140303fb4` -> tile coords), velocity `+0x30 / +0x34`. The truck IS the player's overworld position. Same `(+0x28 pos, +0x30 vel)` actor pattern as horses, NPCs, every movable scene entity. World->screen projection of the truck is one camera-subtract away.

7. **`data/horsey.tmx` on disk** ships the full labeled location table (~30 objects: `type="home"`, `"track"`, `"crispr"`, `"circus"`, ...). Parse it once with `quick-xml`, get a complete `LocationId -> (world_x, world_y, gid, type)` map for free. No more guessing scene-id constants per location; cross-reference `type` against the entry-handler-derived scene-id table.

**What changes in the build order:**

- **Stage S0.5 (paddock probe) is no longer blocking.** Scene-id semantics come from the TMX `type` field + decomp scene-id enumeration ([HORSE-PLACES.md -> Scene-id enumeration](HORSE-PLACES.md#scene-id-enumeration-the-active_scene_id-semantic-decode)). We can resolve PADDOCK_SCENE_ID, HOME_SCENE_ID, etc. analytically before the player ever sets foot in those scenes.

- **Stage S1 (input snapshot) shrinks.** Hooking `cursor_input_handler` gives us per-frame mouse state + modifier state in one place; we don't need to read from hudhook's imgui `Ui`. The input snapshot becomes a passive observer published by the cursor-handler hook.

- **Stage S2 (hovered-horse resolver) is partially replaced.** `cursor_input_handler` already knows what the cursor is over (it dispatches the hover effect). Cleanest path: hook the function, read the hovered-thing pointer it computes. Fallback to the active Location's `LOC[0x2e]` (current approach) only if the hook path doesn't surface the pointer cleanly.

- **Stage S3 (container resolver) is unchanged** (backward search across known horse-vectors stays correct).

- **Stage S5 (transfer primitive) gets a new sub-path:** the existing vtable[+0x78] direct-call work is one option; the synthetic-input-through-cursor-handler option is now strictly better for v1, because it drives the game's own click handler end-to-end (the 4 helpers run for free) AND we can re-project drag endpoints per-frame using MapState camera. The "calibrate truck/pasture cursor floats once and hope they stay valid" approach can be retired; we project live.

- **Stage S6 (hotkey wire-up):** instead of subclassing the game's HWND for click swallowing, the `cursor_input_handler` hook decides per-call whether to pass through (handing the click to the game) or short-circuit (we transfer the horse ourselves and tell the game "I handled it"). Cleaner than the WndProc-based "consumed message" plan in section 4.6.

- **Address-resolution table (section 5) grows by 6 entries:** `MAPSTATE_PTR` (data global), `CAMERA_X_OFFSET`, `CAMERA_Y_OFFSET` (MapState offsets, currently `0x254 / 0x258`), `cursor_input_handler` (fn entry), `enter_location_handler` (fn entry), `building_tile_pos` (`FUN_1400b4a10`, fn entry).

**New prereq stage HK1-S-DECOMP (do FIRST, before S1+):**

- [ ] **D1.** Add `MAPSTATE_PTR` to `targets_registry.rs`. Anchor: `TMX_MAP_PARSER`'s body (`FUN_1400fe2e0`) stores into `DAT_1403f4e00`. The store instruction shape is the same as GAMESTATE_PTR's `48 89 1D ?? ?? ?? ??`. Validators: deref returns heap-shaped ptr, struct shape (vtable at +0x00 is in-image, vector at +0x130 looks sane).
- [ ] **D2.** Add `MapState +0x254/+0x258` camera offsets. Recipe: read MapState's vtable to confirm class identity, hardcode the offsets (this is a stable layout; document the cite).
- [ ] **D3.** Pattern-resolve `cursor_input_handler` (RVA `0x14009d750`). String anchor `"Pointer"`, function size 1577 B. Validator: function-bounds shape via `find_function_bounds_via_int3`.
- [ ] **D4.** Pattern-resolve `enter_location_handler` (RVA `0x1401046c0`, anchors `"EnterLocation" / "EnterLocationStable"`).
- [ ] **D5.** Pattern-resolve `building_tile_pos` (`FUN_1400b4a10`).
- [ ] **D6.** Helper `screen::project_world(world_xy) -> (i32, i32)` in modforge (new module). Reads MapState camera, applies the affine. Test: project the truck's world-pos, compare against in-game truck screen position from manual capture, assert within 1 px.

After D1-D6 land, the existing A1-A3 in todo.md (capture house door coords + replay) become **calibration of the projection affine** rather than fragile fresh-launch-only anchors. The "moving truck = moving house" deferred problem dissolves.

### Addendum 2026-05-17b: scene-id enumeration unlocks more

A second decomp pass produced [HORSE-PLACES.md -> Scene-id enumeration](HORSE-PLACES.md#scene-id-enumeration-the-active_scene_id-semantic-decode) and [HORSE-PLACES.md -> Enter-location handler internals](HORSE-PLACES.md#enter-location-handler-internals). Additional simplifications:

- **`HOME_SCENE_ID = 28` (Sweetie's House) is analytic.** The enter-location handler at `:154976-154981` special-cases scene_id 0x1c with the truck-position-offset + state-flag-reset behavior, which matches the "player's home base / teleport-target" semantic. Concretely: clicking the house on the world map transitions us via `enter_location_handler` with `scene_id == 28`. **This kills the rationale for Stage S0.5 entirely.** We don't need to be physically in the paddock to learn its scene_id either; the same enumeration table covers PADDOCK_SCENE_ID = 13 (track + paddock; money-gated swap to 14 when broke).
- **`drop_horse_fail_event` at RVA `0x1400cdae0` (733 B, `"DropHorseFail" / "TruckLeaveLocation"`)** is already located. Section S7's "no destination -> play fail audio" becomes a single `vanilla.invoke` of this function, no new audio plumbing.
- **`enter_location_handler` post-condition** gives `ensure_home_scene_loaded` a deterministic completion signal: the helper hooks `enter_location_handler`, sets a flag when called with `scene_id == HOME_SCENE_ID`, and returns when the flag fires (with timeout fallback to the existing `active_scene_id != baseline` poll). No more guessing what scene we landed in.
- **`dialog_enqueue` at RVA `0x1400d1c40`** is a free observability surface. Hooking it captures every dialog the game shows (`"Welcome to %s"`, the entry sound name, etc.). Useful for HK1 diagnostics and for the broader agent loop. Not on the HK1 critical path; ship after the core transfer works.
- **`race_state_machine` at RVA `0x140094a20`** has phase strings: `RaceGetSet`, `Racing`, `CrossFinishLine`, `WonRace`, `TruckEnterLocation`, `TruckLeaveLocation`. Hooking it gives us per-phase notifications for the future "shift+click during race day" variants (load truck before race, unload to race line, reload after). Out of v1 scope but worth a target-registry entry while we're here.

**Updated HK1-S-DECOMP task list (adds D7-D9):**

- [ ] **D7.** Add `HOME_SCENE_ID = 28` and `PADDOCK_SCENE_ID = 13` as analytic constants in `targets_registry.rs` (no pattern needed; document the decomp cite). Helper `gamestate::is_home_scene_active()` / `is_paddock_scene_active()` reading `active_scene_id`.
- [ ] **D8.** Pattern-resolve `drop_horse_fail_event` (`0x1400cdae0`, anchor `"DropHorseFail"`). Expose as `vanilla.invoke "audio.drop_horse_fail"` for Stage S7.
- [ ] **D9.** Pattern-resolve `dialog_enqueue` (`0x1400d1c40`). Hook with `seh::guard` to publish dialog strings to a circular buffer + HTTP op `dialogs.recent`. Optional for HK1; high leverage for general agent ops.

The original plan below is preserved for sections that remain accurate (S0 probes shipped, the vtable[+0x78] path, scene-table layout discoveries). Treat the revision above as the source of truth where it conflicts.

---

## 1. Goal in one paragraph

The vanilla flow forces the player to click-and-drag every single horse, one at a time, between the truck (mobile carrier), the pasture (home), and the race line (track starting position). Race-day routines look like: pasture -> truck (drag every horse), drive to track, truck -> race line (drag every horse), run race, race line -> truck (drag every horse), drive home, truck -> pasture (drag every horse). HK1 collapses each individual transfer into a single Shift+Click on the horse. The mod reads the player's current location and the horse's current container, picks the obvious destination, and commits the move with the same side effects the vanilla drag-drop produces (fatigue clearing on race-line drop, audio cue, animation, etc.).

Not in v1: bulk transfer, "transfer all eligible", group selection, custom keybinds, per-location toggles. Ship the single-horse single-click case first; bulk variants come later.

---

## 2. What the game already does (decomp evidence)

The game's existing drag-drop machinery is the foundation. We do not re-implement it; we drive it.

### 2.1. The Location object (`Location*` = `LOC` for short)

Most "places that hold horses" are instances of a single C++ class (subclassed per location). The clearest specimen is `FUN_1400d2ab0` (`interact_with_npc_or_item`), which is one Location's click-tick handler. Inferred field map from reading `all_functions_annotated.c:251400-251680`:

| Field on `LOC` | Type | Role |
|---|---|---|
| `LOC[0x0]` (vtable) | `void**` | vtable; methods at `+0x38, +0x60, +0x68, +0x70, +0x88, +0x90` are called by the click handler |
| `LOC+0x174 / LOC+0x2f*4` (`fVar32`/`fVar34`) | `float, float` | last cursor world-position |
| `LOC[0x17] .. LOC[0x18]` | `std::vector<Interactable*>` | "things you can click here" (items, NPCs, drop targets) |
| `LOC[0x1a]` | `i32` | index into items vector, currently-hovered |
| `LOC[0x26]` | `Horse**` | begin of `std::vector<Horse*>` for this location |
| `LOC[0x27]` | `Horse**` | end of same vector |
| `LOC[0x2c]` | `bool` | "click is valid" / "armed" |
| `LOC[0x2d]` | `i32` | index of the horse the player is currently DRAGGING (-1 = none) |
| `LOC[0x2e]` | `i32` | index of the horse the player is currently HOVERING / candidate to grab (-1 = none) |
| `LOC[0x37]` | `i32` | mouse-button / click state for this tick |
| `LOC+0x16c` | `i32` | "last horse picked up" index |
| `LOC+0xd4` | `i32` | currently-dragged item index (the items vector at `LOC[0x17]`, not horses) |
| `LOC+0x234` | `bool` | "ready to leave location" flag |
| `LOC+0x300` | sub-struct ptr | per-frame screen-space state (truck/UI overlays) |

The horse drag-drop flow inside `FUN_1400d2ab0`:

1. While the mouse is hovering a horse, `LOC[0x2e]` is set to that horse's index in `LOC[0x26]`.
2. On click, if `LOC[0x2c]` (armed) is true and `LOC[0x2e] != -1`, the code commits the grab: it calls `vtable[+0x60](LOC, idx)` (pickup VFX/sound), `vtable[+0x70](LOC, idx, click_state)` (state transition: horse is now mid-drag), writes `LOC[0x2d] = LOC[0x2e]`, and calls `FUN_1400b6890(horse_ptr)` (animation start).
3. On release at a valid drop target, the same handler at the destination Location commits the move (a different vtable slot, likely `+0x88` from the symmetric branch).
4. On release at an INVALID target, `FUN_1400cdae0` (`drop_horse_fail_event`) fires the `DropHorseFail` audio cue.

Critically: **the horse's per-location ownership is the `Horse*` membership in `LOC[0x26]`'s vector.** Moving a horse between truck / pasture / race-line is *moving a `Horse*` between two Location objects' horse vectors*. The vtable methods on `LOC` handle the per-side-effects (animation, audio, fatigue reset, scene flags).

### 2.2. Where the truck / pasture / race-line Location objects live

Per `HORSE-PLACES.md`, the scene-table dump, AND the S0 probe results (2026-05-16):

| What | Where | Notes |
|---|---|---|
| `GS+0x438` | ptr to `void*[256]` | Scene/subsystem table; HLT calls it `kRootSceneTable` |
| `GS+0x25C` | `i32` | `active_scene_id`, -1 = overworld / at Home |
| **Slot 0x00** | sub-struct with strings `"My House"` + `"Home"` at +0x18 / +0x40 | **PASTURE = HOME LOCATION.** Its `+0x130/+0x138` vector is the owned-horse list. vtable_rva `0x30f3d0`. Confirmed via `hk1.probe.active_location` (S0). |
| **Slots 0x08..0x38** | `vector<Horse*>` each | **7 race lanes.** All share vtable_rva `0x30f3d0` with slot 0x00 (same Location class). Confirmed via `hk1.probe.scene_slot_vtables` (S0). |
| Slot 0x90 | sub-struct, vtable_rva `0x30b8a8` | "currently selected horse" context: vector + singleton `Horse*` at `+0x148` |
| Slot 0xb0 | sub-struct, vtable_rva `0x307c10`, count 4 | unknown role (4-element list in observed save) |
| Slot 0xb8 | sub-struct, vtable_rva `0x304578`, count 1 | unknown singleton |
| Slot 0xd0 | sub-struct, vtable_rva `0x30a0c0`, count 3 | candidate mirror of owned (3 in some saves) |
| Slot 0xf8 | sub-struct, vtable_rva `0x3037d0` | matches HLT's `kNeighborSceneVtableRva = 0x3037D0` -> NEIGHBOR scene |
| Slot 0x120 | sub-struct, vtable_rva `0x304e08`, count 5 | "horses available for THIS race" source (race-roster, NOT owned) |

Key consequence of the shared vtable: **the pickup vtable slot we resolve from `FUN_1400d2ab0` applies to BOTH the home Location AND each race-lane Location**. One resolver, two destination kinds. (Vtable primer: [`RE-NOTES.md` -> Vtables](RE-NOTES.md#vtables-c-virtual-dispatch-in-disassembly). Two objects with the same `vtable_rva` are instances of the same C++ class and respond to the same virtual method calls.)

The **truck** still isn't classified. Possibilities:
- A per-Location side-struct on the Home object (e.g. at `LOC+0x300`, the "screen-space state" pointer the decomp dereferences).
- A separate slot we haven't classified yet (likely one of 0xb0 / 0xb8 / 0xd0; these have unknown roles and the right "small singleton" shape).
- A field directly on `GameState` outside the `+0x438` table.

Slots 0xb0 (count 4) and 0xd0 (count 3) are the strongest candidates for the truck because: (a) the truck carries multiple horses at once, (b) their counts match the observed in-game truck content in some sessions. Confirming requires (1) loading horses into the truck in-game, (2) re-running `hk1.probe.scene_slot_vtables`, (3) seeing which slot's count changed.

When the player is at the Paddock / Race Track, `active_scene_id` should switch to a positive int. Re-running `hk1.probe.active_location` from inside the paddock will reveal the paddock's slot offset and confirm it uses the same `0x30f3d0` Location class. THIS IS STAGE S0.5 BELOW.

### 2.3. The mouse / hover globals

HLT confirmed two RVAs we can read directly:
- `kRvaMouseScreenX = 0x3ED970`
- `kRvaMouseScreenY = 0x3ED978`

For hit-testing we do not need to recompute "which horse is under the cursor". The active Location already maintains `LOC[0x2e]` for us (the hovered horse index). That value is alive each frame the cursor is over a horse.

If the active Location is harder to reach than expected, a fallback is scene-table slot 0x90's `+0x148` Horse* singleton, which `FUN_14010de40:160376` reads as "currently selected horse" in some contexts.

### 2.4. Two known-good entry points for synthesised transfers

When we choose to call the game's own functions rather than mutate vectors ourselves:

- **`vtable[+0x70](LOC, idx, state)`** -> begin drag (sets `LOC[0x2d]`, plays pickup audio, starts animation).
- **`vtable[+0x88](LOC, item_ptr)`** -> commit drop on a destination (the items branch; horses likely use a sibling slot identifiable by reading the destination's vtable).
- **`FUN_1400b6890(horse_ptr)`** -> horse-animation start used during pickup.
- **`FUN_1400cdae0(LOC)`** -> drop-fail event (used to confirm what the failure path looks like; we want to AVOID triggering it).

These are not yet pattern-resolved. Resolution work is part of the plan.

---

## 3. Three possible implementation strategies (and the recommended one)

### Strategy A. Pure vector surgery

Find the source vector and the destination vector, swap-remove the `Horse*` from the source, push it to the destination.

- **Pro:** trivially understood, no calls into game code.
- **Con:** skips every side effect (fatigue clearing on race-line drop is a known one per `todo.md`, plus animation, audio, scene flags). HLT explicitly notes that "the scene keeps multiple live indices into [its vectors]; do not compact from the DLL" (`world_map_tools.cpp:179`). The same warning applies here. Game state will be subtly wrong and probably crash within seconds.
- **Verdict:** rejected.

### Strategy B. Synthesize click + drop via Location state

Write `LOC[0x2e] = candidate_idx` on the SOURCE Location and `LOC[0x37] = click-down-state`, let the Location tick handler do the pickup, then on the next tick set the DEST Location's "drop here" state and let it commit.

- **Pro:** the game's own machinery runs every side effect.
- **Con:** requires precise multi-frame state coordination, requires the player to actually be at the destination (we can't tick the dest Location if it isn't the active one), and we are racing the game's own input handling. Hard to make deterministic.
- **Verdict:** rejected for v1. May revisit if Strategy D turns out brittle.

### Strategy C. Direct vtable call on source + dest

Resolve the pickup vtable slot (+0x70 on `LOC`) and the drop vtable slot (probably +0x88 or +0x68, TBD by deeper reading), call them in sequence: `src_LOC.vtable[+0x70](src_LOC, src_idx, 1)` then `dst_LOC.vtable[drop_slot](dst_LOC, horse_ptr)`.

- **Pro:** explicit, deterministic, runs all the game's side effects.
- **Con:** dependent on vtable slot stability across builds (HLT had to re-derive vtables when builds shifted). Means we need pattern-anchored resolvers for the vtable slots, not RVAs.
- **Verdict:** **RECOMMENDED for v1.** The vtable slots are stable WITHIN a build, and we have machinery (`modforge::patterns::sleuth`) to re-derive them.

### Strategy D. High-level "transfer horse" function

Find a single game function that takes `(horse_ptr, dest_scene_id_or_kind)` and does the whole transfer. This would be ideal but no such function is identified yet. Likely doesn't exist as a single named entry; the game's transfer flow is event-driven through the click handler. Defer; if we discover one during research, switch to it.

---

## 4. Architecture for HK1

```
+----------------------------------------------------------+
|  hudhook overlay (already shipping)                       |
|  - hooks IDXGISwapChain::Present                          |
|  - subclasses the game's HWND -> sees WM_KEYDOWN/         |
|    WM_LBUTTONDOWN before the game does                    |
+--------------------------+-------------------------------+
                           |
                           v
+----------------------------------------------------------+
|  modforge::input (new module)                            |
|  - publishes a per-frame snapshot:                       |
|      Modifiers { shift, ctrl, alt }                      |
|      Mouse { x_screen, y_screen, lbutton_down,           |
|              lbutton_pressed_this_frame }                |
|  - shared lock-free; horsey-mod reads it from the        |
|    overlay render callback OR a separate detour          |
+--------------------------+-------------------------------+
                           |
                           v
+----------------------------------------------------------+
|  horsey-mod::hk1::shift_click (new module)               |
|  Per-frame:                                              |
|    if !shift || !lbutton_pressed_this_frame: return      |
|    horse = resolve_hovered_horse()       <- step 4.2     |
|    if horse.is_none(): return                            |
|    src = horse.container()               <- step 4.3     |
|    ctx = player.current_location()       <- step 4.4     |
|    dst = pick_destination(src, ctx)      <- step 4.5     |
|    if dst.is_none(): return                              |
|    swallow_click()                       <- step 4.6     |
|    transfer(horse, src, dst)             <- step 4.7     |
+----------------------------------------------------------+
```

### 4.1. Input plumbing

We already use hudhook for the overlay. Hudhook subclasses the game window. The render-loop callback receives the imgui `Ui`, which exposes `io.keys_down`, `io.key_shift`, `io.mouse_clicked[0]`. That is enough; no new hook needed.

Caveat: the overlay only ticks while ImGui is rendering. We want HK1 active even when the overlay window is hidden. Options:
- Keep the overlay's render loop active (one no-op imgui frame per game frame) and read input from it -> simplest.
- Add a second WndProc subclass via hudhook -> only do this if option 1 has measurable overhead.

For v1: option 1. Read input from the overlay's existing per-frame `Ui`.

### 4.2. `resolve_hovered_horse() -> Option<HorsePtr>`

Three signals, in priority order:

1. **Active Location's `LOC[0x2e]` and `LOC[0x26]`.** This is the game's own "horse the cursor is over right now" index. If the active scene exposes a single Location object (which it does for paddock, track, home), we read its `+0x2e * 4` int, then index into `+0x26`'s vector. Need: a resolver for "active Location pointer" given `GS`. The most likely path is `GS+0x438[active_scene_id*8]` -> a sub-struct that either IS the Location or contains a `Location*` at a fixed offset.
2. **Scene slot 0x90's `+0x148`.** "Currently selected horse" singleton. Fallback if (1) is messy at some locations.
3. **Brute hit-test using `kRvaMouseScreenX/Y`.** Iterate the active scene's horse vector, read each horse's screen-space position, test cursor against an AABB. Last resort; expensive and inaccurate.

For v1: implement (1) and (2) only. Skip (3).

### 4.3. `horse.container() -> Container { Truck | Pasture | RaceLine(lane) | Other }`

Read the horse's *current owner*. The horse object has fields for its location/container state, but the offsets are not yet documented in `HORSE-PLACES.md`. Two ways to find this:

- **Forward search:** for each known horse `Horse*` in the owned-list, scan its bytes and look for the `Location*` of its current container. The truck/pasture Location pointers are stable within a save; one of the qwords on the Horse struct is the back-pointer.
- **Backward search:** enumerate the candidate Location objects (pasture LOC, truck LOC, each race-lane slot's container) and check each one's `LOC[0x26]` vector for the horse pointer. Cheap because there are <10 candidates and each vector has <10 horses.

For v1: backward search. It is O(small * small) and avoids guessing field offsets on the Horse object. Implement as `find_container_of(horse_ptr) -> Container` that walks the known list of horse-vectors and reports the first hit.

### 4.4. `player.current_location() -> LocationKind`

Read `active_scene_id` (`GS+0x25C`) and translate via a small lookup table. Scene IDs are stable within a build; we resolve them once at first call.

| Scene id | Location | Heuristic for resolution |
|---|---|---|
| -1 | overworld (no Location) | direct check |
| `paddock_id` | Race Track / Paddock | string `EnterLocationPaddock` -> caller -> scene id constant |
| `home_id` | Pasture | string `Pasture` if present, else by Location vtable comparison |
| other | (other locations, not in v1 routing table) | passthrough |

The scene IDs are themselves binary-derived constants. Resolve them via the string-xref method already used elsewhere (`format-string xref` recipe in `ADDRESS-RESOLUTION.md`).

### 4.5. `pick_destination(src, ctx) -> Option<Container>`

The user-locked rules from `todo.md`:

```
ctx = RaceTrack:
  src = Truck      -> RaceLine(player's_preferred_lane)
  src = RaceLine(_)-> Truck
  else             -> None

ctx = Pasture:
  src = Truck      -> Pasture
  src = Pasture    -> Truck
  else             -> None

ctx = anywhere else:
  src = Truck      -> Pasture
  src = Pasture    -> Truck
  else             -> None  (or future: nearest carrier)
```

"Player's preferred lane" at the track: pick the first non-full race lane (lanes 0x08..0x38, each capped at ~5 per HORSE-PLACES). If all full, return None and (optionally) play the same DropHorseFail audio.

### 4.6. `swallow_click()`

We want the game NOT to also process this click as a drag-start. Hudhook's WndProc subclass can return early on the consumed message, but only while the overlay is the focus capture. Two approaches:

- **A:** mark the message handled in hudhook's WndProc on the frame we act -> game never sees it. Cleanest if the overlay can capture mouse selectively.
- **B:** let the game see the click but also call our transfer. We then have to undo the game's own drag-start state (`LOC[0x2d] = -1`, clear `LOC[0x16c]`). Worse: more state to babysit.

For v1: A. If hudhook's WndProc cannot conditionally swallow, fall back to B.

### 4.7. `transfer(horse, src, dst)`

Strategy C from section 3. Concretely:

1. Resolve pickup vtable slot on `src` (target it as "the slot called from `FUN_1400d2ab0` when `LOC[0x2c]` is true and `LOC[0x2e] != -1`"). This is `vtable[+0x70]`.
2. Resolve drop vtable slot on `dst`. Reread the click-release branch of `FUN_1400d2ab0` (the part that runs when `*(char *)((longlong)param_1 + 0x234) != '\0'` or the symmetric drop branch we haven't yet located precisely). Identify the slot.
3. Call sequence:
   ```rust
   unsafe {
       // Begin drag on src
       let pickup: extern "fastcall" fn(*mut Location, i32, i32) =
           transmute(src.vtable[PICKUP_SLOT]);
       pickup(src.ptr, src_idx, 1);

       // Commit drop on dst
       let drop_: extern "fastcall" fn(*mut Location, *mut Horse) =
           transmute(dst.vtable[DROP_SLOT]);
       drop_(dst.ptr, horse);
   }
   ```
   wrapped in a SEH guard (per HLT pattern) so a vtable shift between builds can't take the game down without us hearing about it.

### 4.8. Settings toggle

Single bool: `hotkeys.shift_click_transfer.enabled`, default `true`. Persisted via existing settings store (`modforge::settings`). UI: one checkbox on the overlay's existing Hotkeys tab (new section).

---

## 5. Address-resolution work (all pattern-anchored per CLAUDE.md)

Every constant below must land in `targets::resolve::*` and be re-derived via `modforge::patterns::sleuth` at injection time. No bare `0x...` literals in source.

| Symbol | Resolver | Anchor strategy |
|---|---|---|
| `MOUSE_SCREEN_X` data global | `data_global::mouse_screen_x` | port HLT's `kRvaMouseScreenX = 0x3ED970`; rederive in our build via xref from `FUN_14009d750` (cursor input) |
| `MOUSE_SCREEN_Y` | `data_global::mouse_screen_y` | same; adjacent qword |
| `ACTIVE_LOCATION_PTR` | `data_global::active_location_ptr` | derive: `GS+0x438[active_scene_id*8]` chain; expose as helper, not a single resolved global |
| `PICKUP_VTABLE_SLOT` (offset 0x70) | `vtable_slot::location_pickup` | read `FUN_1400d2ab0`, find the `call qword ptr [rcx+0x70]` after the `LOC[0x2c] != 0` branch; assert slot value is 0x70 |
| `DROP_VTABLE_SLOT` (offset TBD) | `vtable_slot::location_drop` | read the symmetric branch on release-at-target; identify slot; assert |
| `PADDOCK_SCENE_ID` | `data_global::paddock_scene_id` | string xref `EnterLocationPaddock` -> caller -> constant operand in compare |
| `HOME_SCENE_ID` | similar | string xref `EnterLocationHome` or equivalent |
| `RACE_LANE_SLOTS` | const range 0x08..0x38 | declared as constant in source after confirmation via `FUN_140105260:155484` decomp; double-check via runtime scan |
| `LOC_HORSE_VEC_BEGIN` (0x26 * 8) | `location_offset::horse_vec_begin` | confirm by reading `FUN_1400d2ab0` references |
| `LOC_CANDIDATE_IDX` (0x2e * 8) | `location_offset::candidate_idx` | same |
| `LOC_DRAG_IDX` (0x2d * 8) | `location_offset::drag_idx` | same |

The HK1 module reads everything via these resolvers. If any fails to resolve, HK1 disables itself with a status note in the overlay (same pattern as `enter_scene_id` failure mode in HLT).

---

## 5b. Session log: live-game findings (2026-05-16)

Real-game research that changed the plan. Read this before reattempting transfer.

### Slot 0x00 IS the Home Location (single-Location-for-home-and-truck-and-pasture)

Confirmed: GS+0x438 slot 0x00 contains a Location object whose first 0x60 bytes hold the strings `"My House"` (at +0x18) and `"Home"` (at +0x40). vtable_rva = `0x30f3d0`. The HOME LOCATION holds the OWNED horse list in its `+0x130/+0x138` vector. There is NO separate "truck Location" or "pasture Location"; the truck is a rectangle drawn inside the home scene.

This kills the original plan section 4.3 "backward search across multiple Location vectors". There is only one vector. The truck/pasture distinction lives PER-HORSE, not per-vector.

### `horse + 0x1d0` (u32) is the container "kind", but it's downstream / display-only

Diff of Coupe DeVille's bytes before vs after a real manual drag (pasture → truck): the only changes outside the position floats were:
- `horse + 0x1d0` u32: `7` (truck), `9` (pasture), `0` or `2` after a fresh save/load. Small enum.
- `horse + 0x1dc` u32: sub-state (slot index? frame counter?) varies (`36`, `20`, `0x12`, `0x27`).

Writing `horse + 0x1d0 = 7` directly DOES NOT move the horse visually or logically. The game recomputes from another authoritative source each tick. So this field is a downstream cache, not a control. The user's hypothesis was right.

### `vtable[+0x78]` (RVA 0xde2e0, function `FUN_1400de2e0`) IS the Home Location's drop-commit

Found by reading the click handler `FUN_1400d2ab0:1722`:
```c
cVar4 = (**(code **)(*param_1 + 0x78))(param_1);
```

Resolved at runtime via `image_base + 0x30f3d0 (vtable) + 0x78`. Confirmed by reading the slot. The function is NOT in our decomp dump (binary updated since decomp).

### `vtable[+0x78]` is 3-arg, not 1-arg (the decomp lied)

Ghidra reported `(*param_1)` (one arg). Disassembling the function entry shows otherwise:

```
+0x00a  55 57 41 54 41 56 41 57       push rbp/rdi/r12/r14/r15
+0x012  48 8b ec                       mov rbp, rsp
+0x015  48 81 ec 80 00 00 00           sub rsp, 0x80
+0x021  41 8b d8                       mov ebx, r8d         ; arg3 saved
+0x024  4c 63 e2                       movsxd r12, edx      ; arg2 sign-ext to r12
+0x027  4c 8b f1                       mov r14, rcx         ; this/LOC
+0x02a  48 8b 81 30 01 00 00           mov rax, [rcx+0x130] ; LOC.horses.begin
+0x031  4e 8b 3c e0                    mov r15, [rax+r12*8] ; horses[arg2]   <-- CRASH HERE if arg2 garbage
```

Signature: `(this: *LOC, drag_idx: i32, param3: i32) -> u8`. The arg2 (drag_idx) is sign-extended into r12 and used as the index into LOC's horse vector. Pass it correctly or the function AVs.

### SEH guard turned crashes into log entries

`modforge::seh::guard` is the difference between "crash kills the game, restart, lose 10 minutes" and "crash logs `SEH ACCESS_VIOLATION (code=0xc0000005) at rip=0x...`, game still alive, retry in 5 seconds". Use it around every call into vanilla. Same applies to anything modforge consumers do with the host game.

### Current status: drop-commit returns 1 (drop ACCEPTED) but horse state unchanged

With the correct 3-arg signature + drag_idx pre-computed from `find_horse_index(horse_ptr, LOC.horses)`, `vtable[+0x78]` returns `1` (drop accepted) and the game survives. BUT `horse + 0x1d0` doesn't update.

Reading the click handler more carefully (FUN_1400d2ab0:1722-1804): vtable[+0x78] is just the HIT-TEST. The actual state writes are done by the click handler AFTER vtable[+0x78] returns non-zero, gated on two globals (`DAT_1403d959b`, `DAT_1403ed730`) which probably mean "real user click this frame". Our synthetic call bypasses those.

The four helpers the click handler runs on success:
- `FUN_1400b47e0(horse_ptr)`: likely the actual container-update / drop-physics setup.
- `FUN_1400b3dc0(horse_ptr, LOC[0x13])`: apply parent reference.
- `FUN_1400b6990(horse_ptr, computed_int, horse_byte_1e0)`: finalize physics?
- `FUN_1400ccbd0(LOC, horse_ptr)`: append decoration entries per the decomp body.

To complete HK1 we must invoke all four after vtable[+0x78] returns 1, OR find a higher-level "drag complete" function that runs the whole sequence (the click handler does this but reads global input state we'd have to spoof). Stage S5 will test calling the four helpers explicitly.

### Calibrated cursor coords + targets file

`<dll_dir>/hk1_targets.json` persists calibrated cursor positions in LOC's world coord space (the floats at LOC+0x174/+0x178). Captured from the user's manual drag:
- truck = `(13.263803, 8.902644)`
- pasture = `(3.407552, 3.0829327)`

These are LOC's internal cursor coords, NOT screen pixel coords. They feed directly into the vtable[+0x78] call setup.

---

## 5c. Session log: container-detection findings (2026-06-23)

HK1 slice 1 (detection) built and live-tested. Result: the `+0x1d0`
field is NOT a clean trailer-vs-pasture flag. Reading the decompiled
drop path is the next step (operator-directed).

What shipped:
- `targets::horse_offset::CONTAINER_KIND = 0x1d0` (documented constant).
- `horse::container_kind(horse) -> Option<u32>` accessor.
- `gamestate.owned_horses` now reports `container_kind` (raw) +
  `container` (trailer | pasture | unknown, classifying 7/9).
- `tests/horse_container_detect.rs`: auto gate, green. The op reads +
  classifies the field per owned horse on the live game.
- `tests/hk1_container_watch.rs`: `#[ignore]`d manual-drag watch.

Live watch (operator dragged 2 horses to the trailer):
- Fresh overworld launch: every owned horse reads `+0x1d0 = 0` (unknown).
- `+0x1d0` values observed across the session: 0, 3, 4. NEVER 7 or 9.
  So `+0x1d0` is a richer per-horse sub-state (a slot/index), not the
  2-value flag the 2026-05-16 note assumed.
- End byte-diff (`horse+0x1b0..+0x1f0`) vs baseline:
  - horse[0]: only `+0x1dc` changed (00 -> 0x12).
  - horse[1]: `+0x1d0` 00 -> 04, plus `+0x1d4..+0x1dc` populated with
    floats + a small counter (0x24 at `+0x1dc`).
- Moving a horse DOES change observable bytes in `+0x1d0..+0x1dc`, so the
  move is detectable, but no single byte read cleanly as trailer/pasture.

Capture caveats (fix before trusting any byte-watch):
- The owned-horse count grew 2 -> 3 mid-watch (save still loading at
  baseline). Baseline must wait for a STABLE count, and horses must be
  keyed by a stable id (ptr / name_id), not list index.
- We only compared overworld-before vs after. Per the operator, the
  trailer/pasture result is only confirmed on LEAVE; a capture must
  sample in-pasture, in-trailer-in-scene, and after-leave.

Decision (operator-directed 2026-06-23): stop black-box byte-watching.
Read the decompiled Location click-drag handler `FUN_1400d2ab0`
(`all_functions_annotated.c:251400-251680`) and its four on-drop helpers
(`FUN_1400b47e0`, `FUN_1400b3dc0`, `FUN_1400b6990`, `FUN_1400ccbd0`) to
find the authoritative field the game writes when a horse is committed to
the trailer vs the pasture, and the exact values. Decomp-first per skill
RULE 5 (the game has the feature; read its code).

---

## 5d. Decomp pass + MapState trailer model (2026-06-23). NOT WORKING YET

Read the decompiled drop + enter/leave handlers. Two solid findings, and an
honest dead end on the live read.

Decomp findings:
- `+0x1d0` is a PICKUP-ORDER COUNTER, not a container flag. `FUN_1400d2ab0`
  (the click-drag handler) writes `horse[+0x1d0] = LOC[+0x164]++` on grab
  (line ~544). That is exactly why the watch saw 0/3/4 and never 7/9. The
  2026-05-16 "7=trailer/9=pasture" note was a misread of this counter.
- The trailer horse list lives on the MapState, NOT on the horse.
  `FUN_1400cd5a0` (the truck enter/leave handler) treats the vector at
  `*MAP_STATE_PTR + 0x130/+0x138` (`*DAT_1403f4e00 + 0x130/+0x138`) as the
  truck's carried horses: on arrival it unloads them into the location's
  vector and clears the source; on leave it refills it from the horses in
  the trailer rectangle. This is the "you find out when you leave" mechanism.

Built:
- op `horse.trailer`: reads `*MAP_STATE_PTR + 0x130/+0x138`, lists the
  `Horse*` in the trailer, cross-refs against owned horses.
- `tests/hk1_trailer_list.rs`.

HONEST STATUS. The live read FAILED, and it was read in the WRONG STATE.
On a fresh overworld launch, `*MAP_STATE_PTR` reads NULL (0), so
`horse.trailer` returned "MapState ptr not heap-shaped". This is consistent
with the existing `scene::camera()` / overlay, which already show "MapState
unreadable" in that same state. So MapState is only live in some states
(in / entering a location, where `FUN_1400cd5a0` runs), NOT on the bare
fresh-launch overworld where the test queried it. The test ran fine; the
read was just taken before the pointer is populated.

OPEN: read MapState in a state where it is non-null. Enter a location (or
let the world fully activate / move the truck). And re-check the trailer
list; OR find where trailer membership is tracked while on the bare
overworld. The MapState-trailer model is decomp-grounded and promising; it
was simply read in the wrong state. The next test must drive the game into a
state where MapState is live before reading.

---

## 5e. Trailer model RESOLVED: positional, per-horse saved position (2026-06-23)

The trailer is NOT a list and NOT a flag. It is POSITIONAL, and the persisted
data is each horse's own home-scene position. This reconciles "it has to be
saved somewhere" with "no trailer list exists."

Evidence (decomp):
- Each horse stores its home-scene (x, y) at `+0x1d4 / +0x1d8`.
  - Drop writes it: `FUN_1400d2ab0` (~line 1888) does
    `*(u64*)(horse + 0x1d4) = building_tile_pos`.
  - Enter re-places from it: `FUN_1400cd5a0` (~lines 110-112) reads
    `horse+0x1d4/+0x1d8` as the placement offset when unloading horses into a
    location.
- The trailer is a fixed RECTANGLE region in the home scene. The click handler
  tests a position against it (`FUN_1400d2ab0` ~lines 714-728) using the trailer
  object `LOC[0xf]` plus fixed extents `_DAT_14030eb8c` / `_DAT_14030eb90` (x)
  and `DAT_140303374` / `DAT_14030d9b8` (y). `FUN_1400cd5a0` ~lines 326-334
  writes that rectangle into `LOC[0xf]` on enter.
- A horse is "in the trailer" iff its saved position is inside that rectangle.
  This is the "you find out when you leave" mechanism: leaving runs the
  positional test.

Dead ends ruled out this session:
- `+0x1d0` = pickup-order counter (not container).
- `*MAP_STATE_PTR + 0x130` = transient incoming-horses during a location-enter
  ONLY; `MAP_STATE_PTR` derefs null on the bare overworld (it is the
  in-location tile-map state). Not the persistent store. (op `horse.trailer` +
  test `hk1_trailer_list` proved this: a 30s poll stayed null.)
- The generic leave path (`FUN_1400cdae0` -> `FUN_1400ce9b0`) does per-horse
  cleanup; it does not move horses to a carrier.
- The truck object is `*(GameState + 0x300)` (flag at `+0xac`, rendered by
  `FUN_1400fb3d0`, which draws the "DragHorseHere" trailer prompt). No horse
  list found on it.

DETECTION PATH (overworld-readable): for each owned horse, read `+0x1d4/+0x1d8`
and test against the trailer rectangle. The horse objects persist on the
overworld, so no MapState needed.

OPEN before building:
1. Confirm `+0x1d4/+0x1d8` still holds the home-scene position while on the
   overworld (vs being overwritten by overworld movement. The horse's MAIN
   actor position is `+0x28/+0x2c`).
2. Pin the exact trailer-rectangle bounds from the constants above.

Decomp functions read this session: FUN_1400d2ab0, FUN_1400cd5a0, FUN_1400cdae0,
FUN_14002d7c0, FUN_1400ce9b0, FUN_140088350, FUN_1400b3dc0, FUN_1400fb3d0.

---

## 5f. Field CONFIRMED LIVE (2026-06-23)

`tests/hk1_horse_positions.rs` read each owned horse's two candidate position
fields on the overworld:

| horse | scene `+0x1d4/+0x1d8` | actor `+0x28/+0x2c` |
|---|---|---|
| trailer | (13.18, 9.30) | (0.0, 0.0) |
| pasture | (0.0, 0.0)    | (0.0, 0.0) |

Operator confirmed ground truth: the horse at (13.18, 9.30) IS in the trailer;
the (0,0) horse IS in the pasture.

CONFIRMED:
- Detection field = `+0x1d4/+0x1d8` (scene placement). Readable + meaningful on
  the bare overworld, no MapState needed.
- Actor position `+0x28/+0x2c` is (0,0) on the overworld -> ruled out.
- Trailer horses carry a scene position near (~13, ~9) (matches the earlier HK1
  calibration `trailer=(13.26, 8.90)`; `pasture=(3.41, 3.08)`); pasture horses
  read (0,0) here.

DETECTOR (agreed direction): classify each owned horse trailer vs pasture by
testing its `+0x1d4/+0x1d8` against the trailer region. Replaces the wrong
`0x1d0` classifier currently in `gamestate.owned_horses` / `horse.trailer`.

OPEN:
1. Region boundary: exact decomp rectangle (extents `_DAT_14030eb8c` /
   `_DAT_14030eb90` for x, `DAT_140303374` / `DAT_14030d9b8` for y, defined
   relative to the trailer object. Awkward to apply on the overworld) vs a box
   centered on the confirmed trailer spot ~(13, 9). Verify-tunable either way.
2. Verify: operator moves a horse trailer<->pasture; the readout must flip.

Artifacts to date: ops `horse.trailer` (MapState path, dead end),
`horse::container_kind` + owned_horses `container_kind`/`container` (reads
`0x1d0`, WRONG field. To be replaced by the position test). Tests:
`hk1_horse_positions` (confirms field, GREEN), `hk1_trailer_list` (MapState
null), `horse_container_detect` (0x1d0), `hk1_container_watch` (manual).

---

## 5g. Honest status 2026-06-24 (detector + names + stuck-car)

Nothing is committed. Several threads are half-landed.

### Detector (slice 1). Built, rule UNCONFIRMED
- `gamestate.owned_horses` classifies trailer/pasture by the scene position
  `+0x1d4/+0x1d8` (added `horse::scene_pos`, `horse_offset::SCENE_POS_X/Y`).
  Current rule: non-zero position (near ~13,9) = trailer, `(0,0)` = pasture.
- `+0x1d0` is a PICKUP-ORDER COUNTER, not the container (kept as
  `horse_offset::CONTAINER_KIND` + `horse::container_kind`, unused by the op).
- UNCONFIRMED: the live save has Coupe DeVille `(13.18, 9.30)`=trailer, tomtato
  `(14.50, 8.98)`=trailer, Horse `(0,0)`=pasture. Operator says only ONE was in
  the trailer, so tomtato at `(14.5, 9)` is probably PASTURE. Meaning the
  zero-vs-nonzero rule is too loose and needs a real trailer RECTANGLE.
- The exact rectangle could NOT be pulled from decomp: the click-handler extent
  constants (`_DAT_14030eb8c` etc.) read as garbage in memory (Ghidra flagged
  them overlapping symbols; live dump confirmed). `tests/hk1_trailer_rect.rs`.
- Verification is BLOCKED: (a) `owned_horses` reads the static loaded-save
  snapshot, not live in-scene drags (moves commit on leave); (b) the owned list
  is inconsistent (tomtato sometimes absent); (c) Coupe is un-grabbable so can't
  be moved to test. `tests/hk1_container_watch.rs` move-verify was inconclusive.

### Name table. Re-derived + decoded, but NOT wired into the live op
- Anchor (`resolve_name_table_custom`, rewritten): the name resolver
  `FUN_1400c78c0` ends with `imul rax,rax,0x88` then `add rax,[rip+disp32]`.
  UNIQUE in `.text`. The disp32 -> NAME_TABLE slot at RVA `0x3f45f0` (drifted
  +0x1110 from the stale `0x3f34e0`, same drift as GAMESTATE_PTR). `*slot` =
  heap table base; entry = base + `name_id*0x88`; MSVC `std::string`.
- Live-decoded names (test `hk1_name_table`): 251="Coupe DeVille",
  272="Horse", 250="tomtato".
- OPEN BUG: names STILL read `<none>` in `gamestate.owned_horses`. The registry
  (`HORSEY_RESOLVER`) resolves NAME_TABLE once at attach and caches the MISS
  permanently. A `OnceLock` bypass in `resolve::name_table()` fixed resolution
  but BROKE `owned_horses` (returned 0 horses) and was REVERTED. So the in-mod
  wiring is unsolved; names only decode in the test. `horse::name_by_id` +
  `horse.name_diag` were changed to deref the slot (correct), but
  `resolve::name_table()` is back on the registry (cached None).

### Coupe DeVille un-grabbable. Root cause found, not repaired
- Grabbing is a box2d COLLISION hit-test (`FUN_1400b6fd0`): walks the horse body
  list `horse+0x40..+0x48`, per fixture checks active (`+0x160`!=0), type
  (`+0x150`!=0xd), size (`+0x154`!=0), then cursor-in-shape. No active fixture
  -> not grabbable.
- So Coupe's clickable collision body is missing/disabled (a failed physics
  rebuild `FUN_1400b3dc0`). NOT the car type, NOT a flag. Operator confirmed
  cars are normally draggable. Not yet confirmed by reading his body list;
  repair would mean forcing a physics rebuild (risky).

### Tests added (all diagnostic / `#[ignore]`d; build green)
`horse_container_detect` (gate), `hk1_horse_positions`, `hk1_trailer_list`
(MapState dead end), `hk1_container_watch` (move-verify), `hk1_trailer_rect`
(rect consts = garbage), `hk1_name_table` (decodes names), `hk1_diff_stuck`
(Coupe diff), `rederive_gamestate_ptr`, fixed `gamestate_resolver_lives`
(load-wait).

---

## 5h. Clean two-horse save: full per-scene dump (2026-06-26)

Old save retired. It carried the un-grabbable Coupe DeVille (a bugged horse
whose collision body was missing, per 5g). Operator started a fresh game with
exactly two horses, one pasture and one trailer, to give the detector a clean
test case.

New diagnostic test `tests/horse_full_dump.rs`: for the current scene it dumps
`active_scene_id`, every owned horse's full field set plus the 240-gene working
genome, and a full `gamestate.scan_438_slots` scan; then it attempts a
synthetic home-scene entry and dumps again. Run in attach mode against the live
game (`MODFORGE_ATTACH=1 MODFORGE_SKIP_BUILD=1`).

### Corrections to earlier honest-status

- **The owned-horse read is NOT on the wrong chain.** `gamestate::owned_stable()`
  already reads scene-table slot 0 (`OWNED_SLOT_OFF = 0x0`), the canonical owned
  list. The `+0x90` in the `gamestate.owned_horses` op docstring (`ops.rs:354`)
  is STALE and wrong; it should read "slot 0". The earlier todo speculation that
  the op "walks GS+0x438 to +0x90 to +0x130, may not be canonical" is disproven.
  Slot 0x90 IS a separate 3-horse subsystem (visible in the slot scan), but the
  owned read does not touch it. TODO: fix the stale `ops.rs:354` docstring.
- **The 1-vs-2 owned-count fluctuation is scene-state, not a chain bug.** In the
  home scene (`active_scene_id = 0`) slot 0 holds BOTH horses. Earlier reads that
  returned only 1 were taken in a different / partial state (the comprehensive
  dump now prints `active_scene_id` so this is no longer ambiguous).
- **Pasture horses do NOT read (0,0).** The 5f assumption that a pasture horse
  reads (0,0) on the overworld was an artifact of the OLD save. In the fresh game
  BOTH horses have real non-zero scene positions, so the current "non-zero =
  trailer" classifier in `gamestate.owned_horses` mislabels BOTH as trailer. The
  zero-vs-nonzero rule is dead; a real trailer RECTANGLE is required.

### Live data (home scene, active_scene_id = 0, slot 0 count = 2)

| field | Horse A | Horse B |
|---|---|---|
| addr | 0x18718e9f560 | 0x18719017c70 |
| name_id | 345 | 344 |
| name | "Horse" | "Horse" (both carry the default name) |
| species | 0 | 0 |
| age / max_age | 5 / 9 | 2 / 9 |
| skill | 0 | 0 |
| tired_a / tired_b | 0 / 0 | 0 / 1 |
| litter_stat | 1 | 1 |
| scene_pos (+0x1d4/+0x1d8) | (18.788, 7.972) | (14.517, 8.490) |
| container (loose rule) | trailer | trailer |
| genome non-zero / max | 85 / 240, max 3 | 92 / 240, max 3 |

Trailer-boundary hypothesis from these two points plus prior calibration
(trailer cluster x ~13-15, y ~8-9): Horse B at (14.5, 8.5) = trailer, Horse A at
(18.8, 8.0) = pasture. The x boundary sits between 14.5 and 18.8. OPEN: operator
to confirm which horse (age 5 vs age 2) is in the trailer, then pin the
rectangle.

Slot scan (home scene), for reference: slot 0 = owned (2); slots 0x08..0x38 = 7
race lanes (5 each); 0x90 = 3-horse subsystem; 0xb0 = 4; 0xd0 = 3; 0x120 = 5
(race roster); singletons elsewhere.

### Name-lookup bug FIXED (2026-06-26)

The blank-name bug from 5g is fixed. `resolve::name_table()` no longer routes
through the registry's resolve cache, which stored the first resolve attempt
permanently; a single transient miss at attach (before the image is fully
scannable) stuck forever, so names read blank in the live op even though the
custom resolver succeeds afterward (that is why `tests/hk1_name_table.rs`
decoded fine but `gamestate.owned_horses` did not). The function now calls
`resolve_name_table_custom` directly and caches ONLY a successful non-zero slot,
so a miss is always retried. Verified live: both owned horses now decode to
"Horse" (the fresh-game default name) in `gamestate.owned_horses`. This did NOT
break `owned_horses` (the earlier `OnceLock` bypass did, returning 0 horses; the
`AtomicUsize` success-only cache does not). Note: the `name_diag` op reports the
SSO capacity (15) in its `size_at_18` field, not the length; the real length is
at entry+0x10, which `horse::name_by_id` reads correctly.

Confirmed dynamic and scene context: all the data in this section was captured
live while the player is in the "My House" scene (`active_scene_id = 0`, the Home
Location at scene-table slot 0, whose object holds the strings "My House" / "Home"
at +0x18 / +0x40). That is the scene where BOTH owned horses are loaded into slot
0 and readable with full data; on the bare overworld only one is in slot 0. After
this fix, renaming both horses in-game (to "alpha" / "bravo") updated the live op
immediately with no reinjection. The name_ids stay constant (344 / 345); only the
name-table strings change.

### Full roster, both horses (home scene, 2026-06-26)

The two horses were renamed in-game to "alpha" and "bravo" (from the default
"Horse") to confirm the name fix updates live; tell them apart by name / age /
position. Memory addresses are per-session and omitted (they change every
launch); the rest is save-persistent.

| field | bravo (id 345) | alpha (id 344) |
|---|---|---|
| name | "bravo" | "alpha" |
| name_id | 345 | 344 |
| species | 0 (normal horse) | 0 (normal horse) |
| age / max_age | 5 / 9 | 2 / 9 |
| skill | 0 | 0 |
| tired_a / tired_b | 0 / 0 | 0 / 1 |
| litter_stat | 1 | 1 |
| scene_pos (+0x1d4/+0x1d8) | (18.788, 7.972) | (14.517, 8.490) |
| container (loose rule, WRONG) | trailer | trailer |
| container (CONFIRMED actual) | pasture | trailer |
| genome non-zero / max tier | 85 / 240, 3 | 92 / 240, 3 |

### Trailer vs pasture CONFIRMED via overworld (operator-confirmed 2026-06-26)

Operator confirmed in-game: alpha is in the trailer, bravo is in the pasture.
Ground truth, established two independent ways:

1. **Position (in-scene signal).** alpha sits at scene_pos (14.517, 8.490),
   inside the trailer cluster (x ~13-15, y ~8-9); bravo sits at (18.788, 7.972),
   outside it. Matches the hypothesis exactly.
2. **Leave-the-scene behavior (the clean discriminator).** Driving to the
   overworld (`active_scene_id = -1`) moves trailer horses into the truck carrier
   and removes them from the home location vector (slot 0); pasture horses stay.
   Observed live:

   | scene | slot 0 (owned / home vector) |
   |---|---|
   | "My House" (id 0) | 2 horses (alpha + bravo) |
   | overworld (id -1) | 1 horse (bravo only); alpha off-list, in the truck |

   So on the overworld, slot-0 membership IS the trailer/pasture flag: present =
   pasture (bravo), missing = trailer (alpha). This is the decomp "you find out
   when you leave" mechanism (5d/5e), now confirmed against named horses.

Detector implication: the current "non-zero scene_pos = trailer" rule is wrong
(it tags bravo, the pasture horse, as trailer). Two correct paths: (a) in-scene,
test scene_pos against the real trailer RECTANGLE (alpha ~14.5 inside, bravo
~18.8 outside); (b) on the overworld, use slot-0 membership directly. Pin the
rectangle bounds from these two confirmed points plus prior calibration.

---

## 6. Sequenced delivery, one ship + checkpoint between each

Per CLAUDE.md, each stage ships its own commit with: tests that prove the primitive works, real game verification (Claude drives `horsey-play` + tests), zero unstaged scope creep.

### Stage HK1-S0. Research probes (no production code) [DONE 2026-05-16]

- `tests/hk1_probe_loc_field_layout.rs`: passes. With `active_scene_id = -1` (overworld/home), probe walks slot 0x00 and reports `loc_ptr`, `vtable_rva=0x30f3d0`, `loc_horse_count=2` (matches user save), `loc_drag_idx=-1`, `loc_cand_idx=-1`, `loc_armed=0`. Raw bytes reveal strings `"My House"` + `"Home"` confirming slot 0x00 IS the Home Location.
- `tests/hk1_probe_scene_slot_vtables.rs`: passes. Classifies 30 slots; slot 0x00 + slots 0x08..0x38 all share vtable_rva 0x30f3d0 = the shared Location class. Slot 0xf8 matches HLT's `kNeighborSceneVtableRva = 0x3037D0`.
- `tests/hk1_probe_mouse_globals.rs`: passes (no error) but values read as `0xffffffff` raw bits -> NaN floats. HLT's mouse RVAs are STALE in our build (binary updated 0a2689fe). S1 must re-anchor via xref from `cursor_input_handler` (FUN_14009d750).

Backing ops in `src/ops.rs`: `hk1.probe.active_location`, `hk1.probe.scene_slot_vtables`, `hk1.probe.mouse_globals`.

### Stage HK1-S0.5. Paddock active_location probe [TODO, requires player at paddock]

Same `hk1.probe.active_location` op, but with player physically at the Race Track location in-game (so `active_scene_id` switches to a positive integer). Expected: probe reports the paddock slot offset, confirms vtable_rva is the same `0x30f3d0` Location class, dumps `LOC[0x26]` horse-vec which holds the horses currently at the track. This pins the paddock scene id constant for the `LocationKind` lookup table in section 4.4.

**Ship gate:** S0 + S0.5 both reproduce the decomp evidence in live memory.

### Stage HK1-S1. Input snapshot

- `modforge::input` module exposing `read_snapshot() -> InputSnapshot { shift, lbutton_pressed, mouse: (x, y) }`.
- Read from hudhook's `Ui` inside the overlay render loop, publish to a `parking_lot::RwLock<InputSnapshot>` or atomic struct.
- HTTP op `input.snapshot` for tests: returns the most recent snapshot as JSON.
- Test: `tests/input_snapshot_updates.rs` polls the HTTP op while a fixture nudges the mouse (driven via `SendInput`); assert `x` changes.

**Ship gate:** test passes; `input.snapshot` shows live cursor data.

### Stage HK1-S2. Hovered-horse resolver

- `horse::hovered_horse() -> Option<HorsePtr>` reading the active Location's `+0x26 / +0x2e`.
- Active-Location resolver: `gamestate::active_location() -> Option<LocationPtr>` doing the `GS+0x438[active_scene_id*8]` walk.
- HTTP op `horse.hovered`. Returns `{horse_ptr, name, name_id, container_hint}`.
- Test: `tests/horse_hovered.rs` env-driven; tester runs game, hovers a known horse, calls op, asserts horse name matches.

**Ship gate:** hovering known horse returns its pointer + name in the HTTP op.

### Stage HK1-S3. Container resolver (backward search)

- `horse::container_of(horse_ptr) -> Container` walking the known list of horse-vectors (owned slot 0x00, race lanes 0x08..0x38, paddock LOC, home LOC, truck LOC). First hit wins.
- HTTP op `horse.container`. Input: `horse_ptr`. Output: `{kind, location_ptr, index}`.
- Test: `tests/horse_container.rs` cycles a horse through 2-3 locations in-game (manually) and asserts the op reports the right container each time.

**Ship gate:** moving a horse manually changes the reported container.

### Stage HK1-S4. Destination picker (pure logic)

- `hotkeys::pick_destination(src: Container, ctx: LocationKind) -> Option<Container>`.
- No game-state reads inside (it is a pure mapping from inputs to a kind). Easy to unit-test.
- Unit tests covering every rule in section 4.5.

**Ship gate:** all rule rows in section 4.5 have a passing assert.

### Stage HK1-S5. Transfer primitive (Strategy C, partial 2026-05-16)

Current state:
- `hk1::transfer_horse(horse_ptr, dest)` stages LOC[0x29]=horse_ptr, LOC[0x2d]=drag_idx, LOC[0x37]=1, LOC[0x174/0x178]=target cursor. Calls `vtable[+0x78](LOC, drag_idx, 1)` inside `seh::guard`. Returns `1` (drop accepted) without crashing. Game survives. BUT no visible/logical state change yet.
- HTTP ops `hk1.read_cursor`, `hk1.set_target`, `hk1.transfer`, `hk1.probe.locate_horse`, `hk1.probe.scene_slot_vtables`, `hk1.probe.active_location`, `hk1.loc_bytes`, `mem.poke` are all live.
- Overlay buttons: `Save cursor as TRUCK/PASTURE`, `Snapshot` (logs LOC + horse bytes), `>> Truck`, `>> Pasture`.
- Each button click writes pre/post snapshots + transfer parameters to `<dll_dir>/hk1_overlay.log`.

What's still missing for transfer to be visible:
- After `vtable[+0x78]` returns `1`, call the four helper functions the click handler's success branch runs:
  - `FUN_1400b47e0(horse_ptr)`
  - `FUN_1400b3dc0(horse_ptr, LOC[0x13])`
  - `FUN_1400b6990(horse_ptr, computed_int, *(horse_ptr + 0x1e0))`
  - `FUN_1400ccbd0(LOC, horse_ptr)`
- Pattern-resolve each at runtime + invoke under `seh::guard` (any one of them faulting just logs).

Test: `tests/horse_transfer_truck_pasture.rs`. Game has player at home, horse in pasture. Test calls op with `dst=truck`. Asserts visible: `horse + 0x1d0` matches the truck value AND a sprite-position change. Then `dst=pasture`, asserts round-trip.

**Ship gate:** round-trip transfer via HTTP works without crashing; visible in game (horse animates the same way as a manual drag-drop).

### Stage HK1-S6. Hotkey wire-up

- `hotkeys::shift_click::tick()` called from the overlay render loop.
- Reads input snapshot. On Shift+Click rising-edge, runs the full pipeline (resolve hovered -> resolve container -> resolve ctx -> pick destination -> call transfer).
- Swallow the click (Strategy A from 4.6); if that proves impossible in hudhook, fall back to B.
- Settings entry `hotkeys.shift_click_transfer.enabled` (default true) gates the whole thing.
- Test: `tests/hk1_e2e.rs`. Drives `horsey-play` + synthesised input. Assert one Shift+Click moves a horse pasture->truck.

**Ship gate:** real Shift+Click in the running game moves horses correctly.

### Stage HK1-S7. Failure paths + audio

- When `pick_destination` returns `None` (e.g. all race lanes full), play the `DropHorseFail` cue: `FUN_1400cdae0` is the existing emitter, but calling it requires an active Location. Simpler: directly invoke the audio play function for `DropHorseFail` if pattern-resolved; else silently ignore for v1.
- Status note in the overlay's footer: "HK1: transferred Foo to truck" or "HK1: race lanes full".

**Ship gate:** edge cases produce visible feedback; no silent failures.

---

## 7. Tests-first checklist (CLAUDE.md hard rule)

Every primitive in section 6 ships with its `tests/*.rs` BEFORE the production code that backs it. No probing via curl/python/PowerShell one-liners. The S0 research probes themselves are tests (they assert structural invariants on the live game).

The full test list:

- `tests/probe_loc_field_layout.rs` (S0)
- `tests/probe_scene_slot_inventory.rs` (S0)
- `tests/probe_mouse_globals.rs` (S0)
- `tests/input_snapshot_updates.rs` (S1)
- `tests/horse_hovered.rs` (S2)
- `tests/horse_container.rs` (S3)
- `tests/pick_destination_rules.rs` (S4, unit)
- `tests/horse_transfer_truck_pasture.rs` (S5)
- `tests/horse_transfer_track_lane.rs` (S5 extension)
- `tests/hk1_e2e.rs` (S6)
- `tests/hk1_no_destination.rs` (S7, asserts no crash + status note)

---

## 8. Risks and unknowns

1. **Vtable slot stability across builds.** Mitigation: resolvers anchor on the calling site in `FUN_1400d2ab0`, not on a hardcoded slot number. The slot is decoded from the operand of the `call qword ptr [rcx+disp8]` instruction.
2. **Hudhook click swallowing.** If hudhook's WndProc cannot conditionally swallow WM_LBUTTONDOWN, we either subclass the game's HWND ourselves (a second hook) or fall back to "let the click through and undo the game's drag-start state". Detect early in S1.
3. **Truck Location object location.** Section 2.2 has it as "probably attached to GameState or to a per-Location side-struct". If the truck has no stable Location pointer (e.g. it is reconstructed every scene transition), Strategy C breaks for the truck side and we have to fall back to Strategy B for that one container. Detect during S0 / S2.
4. **Multi-frame drag state.** If the game's tick reads `LOC[0x2d]` BEFORE we get a chance to set it, the drag-start animation could glitch. SEH wrap; if we see glitches, add a one-frame delay (set `LOC[0x2d]` from a deferred callback on the next tick).
5. **Save/load.** The transfer mutates game state that the vanilla save format already persists. No mod-side sidecar needed for HK1. Verify in S5: transfer, save, reload, confirm horse is still where we put it.

---

## 9. Out of scope for v1 (carried forward into a future doc)

- Bulk transfer (Shift+Click on a group; "transfer all eligible" hotkey).
- Custom keybinds (the modifier and button are hardcoded to Shift+LMB).
- Per-location toggles in settings (only the global enable/disable exists).
- HK2+ for any other hotkey.
- Transferring horses we do not own (NPC horses, wild horses).
- Cross-scene transfers when the player is not at either endpoint (i.e. teleporting horses). Vanilla doesn't support this; HK1 doesn't either.

---

## 10. References to other docs

- `docs/todo.md` -> "Hotkeys" -> HK1 spec, groundwork list.
- `docs/HORSE-PLACES.md` -> scene-table layout, horse vector locations, slot inventory.
- `docs/ADDRESS-RESOLUTION.md` -> migration tracker; new resolvers from section 5 add to its tables.
- `docs/PRIOR-ART-HorseyLiveTweaks.md` -> what HLT does and does not do (HLT does NOT have horse transfer; HK1 is novel territory).
- `research/decompiled/all_functions_annotated.c:251400-251680` -> the Location click-drag handler this plan is built around.
- `research/decompiled/annotated/BATCH-08.md:19` -> `drop_horse_fail_event`.
- `research/prior-art/HorseyLiveTweaks/src/core/offsets.h` -> mouse-coord RVAs, scene-table offsets.
