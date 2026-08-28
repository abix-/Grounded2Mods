# modforge workspace open issues

Survivalist extraction means moving existing engine-independent code out of the mod. Do not invent new planner, queue, registry, definition, or state types just to generalize it. Prefer direct code moves and small pure functions over new framework APIs. Survivalist keeps its existing game-specific control flow, Unity access, content, and behavior. An extraction row is complete only when the reusable code has moved without adding a speculative system.

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | MISERY player input | [x] Write the Rust executable-analysis research test for keyboard input | `misery-mod/tests/research_player_input.rs` finds the live player's controller, PlayerInput object, input component, key-to-action mappings, and key state data, and prints them for analysis. Pure discovery, no bot input. |
| 1 | MISERY player input | [x] Find InputKey and call it to inject a virtual W press | SOLVED via prior art (bl-sdk): InputKey is EnhancedPlayerInput vtable slot 88 (RVA + 0x42f5970), called as `vtable[88](epi, &FInputKeyParams)` with the correct struct layout and a real FKey. Proven by `find_inputkey_slot_via_iskeydown`: after the call IsInputKeyDown(W)=true, after release=false. BUT `test_inputkey_movement` showed it does NOT move the character: ForwardInput action stays 0. MISERY movement runs on Enhanced Input actions; use InjectInputForAction (next row). research.md section 31. |
| 1 | MISERY player input | [x] Move the character via Enhanced Input action injection | SOLVED and verified live: `StartContinuousInputInjectionForAction(ForwardInput, FInputActionValue(1.0, Axis1D), [], [])` on EnhancedInputLocalPlayerSubsystem walked the character 677 units; `StopContinuousInputInjectionForAction(ForwardInput)` stopped it. BlueprintCallable UFunctions on EnhancedInputSubsystemInterface, called via the `call` op (ProcessEvent, no crash). `test_inject_action_movement`. research.md section 31. |
| 1 | modforge input research | [x] Build the write/exec hardware watchpoint ("find out what writes/runs at this address") | `modforge::winproc::capture_write_watchpoint` + the `watch_writes` op arm a CPU debug register across all threads and record each hit's instruction and call chain as image-relative offsets. Verified live: arms 123/123 threads, last_err 0. Committed. |
| 1 | MISERY player input | [x] Disprove the prologue-scan InputKey candidate | An exec breakpoint on the candidate (RVA + 0x320d680) fired zero times while W was tapped; a control fired 14 times. The candidate is never run on input. Recorded in research.md section 31. |
| 1 | MISERY player input | [~] Decode the KeyStateMap on EnhancedPlayerInput to find the W FKey entry's address | In ONE session a per-key FKey->FKeyState TSet block at EPI + 0x788 received W's FKeyState write on a press (24 writes), FKeyState 0x18 past the FName. CAVEAT: the block is found by a runtime heuristic scan and a later run found DIFFERENT blocks (+ 0x1D0, + 0x538, + 0x548) with no writes, so the + 0x788 offset is NOT confirmed reproducible. Store RVA + 0x42f25d0 is stable. |
| 1 | MISERY player input | [x] Write-watch the W KeyStateMap byte to capture the store and its call chain | Tapping W caught 24 writes from the game input thread. Store instruction + 0x42f25d0 (a shared setter); input-thread caller functions + 0x11aee60, + 0x3cb9360, + 0x39f1210, + 0xf87b70, + 0x10dc360, + 0x3213800. `find_inputkey_by_param` then showed NONE of these carries W's FKey, so InputKey is NOT among them (it is higher up the stack). research.md section 31. |
| 1 | MISERY player input | [ ] Identify InputKey by the parameter it receives, not its chain position | `name_inputkey_by_rcx` narrowed to input-thread functions + 0x39f1210 (this = controller) and + 0x3cb9360 (this = EPI), but `decode_inputkeyparams` showed + 0x39f1210's arg is a UObject container, NOT FInputKeyParams, so the earlier "InputKey = + 0x39f1210" naming is RETRACTED. Find the frame whose incoming arg holds W's FName index + W's FKeyDetails pointer (from the KeyStateMap W entry); that frame is InputKey. research.md section 31. |
| 1 | modforge input research | [x] Guard the read_bytes and write_bytes ops against bad/code pointers | Both ops `VirtualQuery`-check every page before the copy. Proven by `read_op_guards_bad_pointer`: reads and writes of 0x1, 0xDEAD00000000, 0x7FFFFFFF0000 return a clean error and the game survives. Committed. |
| 1 | MISERY player input | [ ] Hook the real InputKey to capture a byte-exact FInputKeyParams from a physical W press | Once InputKey is identified by its parameter, the real block is recorded from a live press so the bot replays real bytes instead of a guessed struct. |
| 1 | Ueforge input | [x] Send key press, held state, and release exactly as MISERY sends player keyboard input | `UnrealInputSurface::key` maps W/A/S/D/E to Enhanced Input actions and holds them via Start/StopContinuousInputInjectionForAction. Verified live through `input.player.commands`: W/A/S/D each walk the character ~200-243 units (`test_bot_all_keys`). |
| 1 | MISERY input proof | [x] Run a permanent live test of the exact player W input mechanism | `test_bot_input_wired`: W via input.player.commands walks the character 270 units, release stops it, positions recorded. |
| 1 | MISERY player input | [x] Investigate exactly how MISERY handles real player mouse movement | Mouse look is the TurnInput (yaw) and LookupDownInput (pitch) Enhanced Input actions, driven by relative mouse delta. research.md section 31. |
| 1 | Ueforge input | [x] Send relative mouse movement exactly as MISERY sends the player's mouse input | `UnrealInputSurface::move_rel` injects TurnInput/LookupDownInput via InjectInputForAction. Verified live: mouse dx=200 turned the view 180 degrees (yaw 90 -> 270). No rotation write, no cursor move. |
| 1 | MISERY input proof | [ ] Add and run a permanent live test of the exact player mouse-input mechanism | The test observes the retained player's view before and after input and fails unless the requested mouse movement changes it. |
| 1 | MISERY input proof | [ ] Add and run a permanent live test of the exact player E input mechanism at a door | The test aims at an in-range door, sends E through the verified player mechanism, and fails unless MISERY's existing binding changes the observed door state. |
| 1 | Ueforge input | [ ] Release every held key when input stops, fails, is cancelled, or shuts down | Unit tests and a restarted live failure case prove W/A/S/D/E are released and the player does not continue moving or interacting. |
| 1 | Ueforge navigation | [ ] Return a complete Unreal navigation path to Modforge in the shared path format | Ueforge owns the Unreal navigation call and returns path points without making MISERY decode or steer through Unreal navigation itself. |
| 1 | MISERY navigation | [ ] Run the complete spawn-to-metal-door-to-expedition test through real player input | For each selected waypoint, Unreal A* returns a complete path and the bot travels it through the verified W/A/S/D, relative mouse, and E input path. |
| 1 | MISERY performance | [ ] Measure the complete live input and route test for repeated work | Input performs no repeated player, function, or object searches; the existing timing report shows every measured bot operation below 16.7 ms. |
| 1 | Unityforge player input | [ ] Investigate and use exactly how the Unity game handles real player keyboard and mouse input | A Unity test receives the same ordered player-input commands as Unreal through the exact functions and data used by that game's player input. |
| 1 | Unity bot navigation | [ ] Run one live Unity waypoint trip through the shared Modforge bot-navigation system | The bot receives a complete Unity path, travels it through player input, performs one bound interaction, observes success, and releases all input. |
| 1 | MISERY interaction | [ ] Aim from the active player camera at the bunker door's colliding-bounds center and press `E` only when the bounds are within interaction range and the door allows interaction | Three consecutive restarted runs open the bunker door once without looking at the floor or requiring manual input. |
| 1 | MISERY interaction | [ ] Open the traversed crate with player `E` input and observe its retained storage state | The crate reports an active user without searching for a container window or another UObject. |
| 1 | MISERY interaction | [ ] Transfer the crate contents through the real inventory UI action | The retained crate item count falls and the retained player item count rises after player-like input. |
| 1 | MISERY performance | [ ] Extend the existing scoped timing guard across the complete spawn-to-looted-crate run | Timing disables after success or panic; `ue:find_object`, `ue:find_objects_by_chain`, `ue:find_actors_by_chain`, and `ue:objects_read` are absent; every measured bot operation remains below 16.7 ms. |
| 1 | MISERY acceptance | [ ] Run and document the permanent cold spawn-to-looted-crate acceptance | One restarted run enters the expedition, selects and reaches a real crate, opens it, loots it, releases input, and records the verified result in the owning docs and changelog. |
| 3 | `wwm-mod` | [ ] Verify the hot-reload cycle works in-game: launch WWM, observe gen 0 ready, run build_and_deploy.ps1 -Hot, watch BepInEx log for hot reload 0 -> 1, confirm curl ops still answer | Hot-reload verified in-game. |
| 3 | `wwm-mod` | [ ] Fix the janky jump: investigate PlayerController per-frame movement to find what clobbers position/velocity; likely needs a Harmony prefix on the per-frame movement method to inject a jump-velocity boost | Jump feels correct and reliable. |
| 3 | `wwm-mod` | [ ] Ship UnitySkillProxyEffect and repoint Strong Back at SkillsManager.SetSkillLevel("Bag", N) (the game already grows the slot list; verified live 5->12 slots) | Strong Back uses the game's built-in Bag skill API. |
| 3 | `wwm-mod` | [ ] Research Charisma + Resilient mappings: neither has an obvious built-in game-skill match; either find a custom proxy through the existing four or write a raw-field Effect against GameDataSO fields | Charisma and Resilient skill effects documented and implemented. |
| 3 | `wwm-mod` | [ ] Verify Greedy Miner in-save: walk_class MineDataSO to confirm field names (zero instances at main menu; SOs instantiate with a save) | Greedy Miner confirmed working in a loaded save. |
| 3 | `horsey-mod` | [ ] horsey sleep_safe_no_tire patch site discovery: find_patch_site cannot uniquely identify the +0x206 zero-store inside the no_tire loop at FUN_1400ceb60; next idea: walk back from DAT_1403d95c5 read and pick the FIRST +0x206 store that follows it | Patch site uniquely identified and sleep_safe_no_tire NOP applied. |
| 5 | `modforge` | [ ] One additional consumer mod (grounded2-mod or schedule1) adopts at least one testkit primitive to prove the abstraction works cross-game | Second consumer uses modforge::testkit. |
| 5 | `modforge` | [ ] Document modforge::testkit in modforge/docs/ (currently only module-level rustdoc) | testkit documentation in modforge/docs/. |
| 5 | `horsey-mod` | [ ] B4a sig tuning: 5 targets still hint-only (SAVE_VERSION_GLOBAL, RACES_COUNTER, NO_TIRE_TOGGLE, RETIRE_HORSE_HANDLER, BALLOON_CONTROLLER); author candidate signatures for each | All 42 registry entries have real candidate sigs (100%). |
| 5 | `modforge` | [ ] Update modforge/docs/target-registry.md from design-doc to shipped-API doc | target-registry.md reflects shipped API. |
| 5 | `horsey-mod` | [ ] Reduce targets.rs from 1286 LOC to pure TargetDef data (remaining LOC is fn_addr, gs_offset, horse_offset modules) | targets.rs is pure TargetDef data. |
| 5 | `survivalist-mod` | [ ] Exercise the Load/Unload re-init path live: trigger a story SWITCH (not quit-to-menu) and confirm the ReinitAfterUnload path works | Story-switch re-init verified live. |
| 8 | `horsey-mod` | [ ] I-R1: Decomp grep for input read sites in all_functions.c (GetCursorPos, GetAsyncKeyState, SDL_*, etc.) to determine which OS surface Horsey uses | Input surface documented; L1/L2/L3 viability decided. |
| 8 | `horsey-mod` | [ ] I-R2: Locate mouse-state globals (expand from hk1.probe.mouse_globals); pattern-resolve via targets | Mouse-state globals pattern-resolved. |
| 8 | `horsey-mod` | [ ] I-R3: Locate keyboard-state globals (likely a 256-byte VK array next to GetAsyncKeyState call sites) | Keyboard-state globals located. |
| 8 | `horsey-mod` | [ ] I-R4: Identify the click dispatcher (HK1 found FUN_1400de2e0 for drop-commit; need the equivalent for any-click) | Click dispatcher function identified. |
| 8 | `horsey-mod` | [ ] I-R5: PostMessage smoke: does PostMessage(hwnd, WM_LBUTTONDOWN, ...) trigger an in-game click? | L2 viability confirmed or ruled out for Horsey. |
| 8 | `modforge` | [ ] I-3: Coordinate spaces: explicit input.client_to_screen + DPI awareness (L2 already uses client-area px) | Coordinate space conversions handle DPI correctly. |
| 8 | `horsey-mod` | [ ] I-4: L3 game-internal pokes v2: direct mouse-state struct writes via FUN_14018c5c0() accessor + direct keyboard buffer writes via FUN_140183330(0)+0xe1 | L3 input bypasses OS entirely for cursor and keyboard. |
| 8 | `modforge` | [ ] I-2b: input.state.get op that reads game's own mouse/keyboard state for verification (needs per-game InputSurface impl) | input.state.get returns game-side state for test verification. |
| 8 | `horsey-mod` | [ ] I-2d: HK1 Shift+Click migration: once calibration gives screen-to-game-coord ratio, ship input.combo with shift + input.mouse.drag | Shift+Click transfer uses modforge::input instead of custom vtable helpers. |
| 8 | `modforge` | [ ] End-to-end test exercising a click-driven flow (open Horses tab, click row 3, assert detail panel opens) | Click-driven UI test passes. |
| 8 | `modforge` | [ ] At least one second consumer (grounded2-mod or schedule1) wires up InputSurface for cross-game proof | Second consumer uses InputSurface. |
| 10 | `horsey-mod` | [ ] V4 validation: confirm 4 vanilla signatures work against live game (vanilla.list enumerates 4; vanilla.invoke on RNG_NEXT_MODULO returns u32 in [0,100)) | 4 vanilla function signatures verified live. |
| 10 | `horsey-mod` | [ ] V5: Migrate horsey-mod call sites that invoke vanilla code to vanilla::call (FUN_1400b3070 rebuild, FUN_1400c6580 RNG, any new direct invocations) | Production call sites use Invoker::call_safe instead of transmute. |
| 10 | `modforge` | [ ] V6: Cross-game vanilla adoption: when grounded2-mod or schedule1 ships their first TargetRegistry, register one function with a Signature | Second consumer registers a Signature and invokes through Invoker. |
| 10 | `grounded2-mod` | [ ] In-game smoke test: game launches, ImGui tab opens, load save triggers slot activate, kill creature triggers XP, HTTP /debug responds | Full in-game acceptance passes. |
| 15 | `unityforge` | [ ] Handle-table namespace per generation: old gen's still-held handles are stale after swap; high-bit-encode the generation if collision matters | Handle collisions across generations impossible. |
| 15 | `unityforge` | [ ] Periodic GC of quiesced generations: _quiesced list grows forever; free GCHandle once gen's threads have exited | Quiesced generations cleaned up. |
| 15 | `ueforge` | [ ] simulate_apply_damage lift to ueforge::rpg::health (gated on Wave E1; ApplyDamageFromInfo from PE trampoline re-enters ProcessEvent and crashes) | simulate_apply_damage available as framework op. |
| 20 | `ueforge` | [ ] Parm decoders: lift per-UFunction parm block shapes from kill_hook / inv_hook / fall_hook into ParmDecoderDef + per-class registry + generic walk_parms debug op | walk_parms debug op works for any hooked UFunction. |
| 20 | `ueforge` | [ ] ClassRef registry: every declared static ClassRef could feed a workspace-wide class_refs_list op | class_refs_list op enumerates all declared ClassRefs. |
| 20 | `ueforge` | [ ] Annotate remaining ~50 undocumented unsafe blocks across ue/probe.rs, status_effect.rs, class_tweak.rs, core_types.rs, typed_field.rs, fname.rs, platform.rs, player.rs, discovery.rs, selector.rs, damage_info.rs, pe_call.rs, parms.rs, fstring.rs | clippy::undocumented_unsafe_blocks flipped to deny with zero warnings. |
| 20 | `ueforge` | [ ] Hot-reload torture test: 1000x Ctrl+R loop with hooks installed; assert no thread leak, hook leak, slot regression | Torture test passes 1000 cycles. |
| 20 | `ueforge` | [ ] zerocopy remaining sites: damage/mod.rs on_event (DamageHookConfig), data_table.rs decode_field, kill_hook/fall_hook/inv_hook parm decoders (~6 structs), FDataTableRowHandle (pointer) | All feasible sites use zerocopy derives. |
| 20 | `ueforge` | [ ] proptest remaining: FieldTweak decoder + inspect_address byte slabs | Property tests cover FieldTweak and inspect_address. |
| 20 | `ueforge` | [ ] insta per-op snapshots for the standard debug-op set (skill_toggle / spend / refund / etc.); gated on building a stateless test fixture | Per-op snapshot tests pass. |
| 25 | `ueforge` | [ ] UE-version-aware ffield / fproperty / ustruct offsets (hardcoded for UE 5.4; UE 5.5+ silently returns wrong names) | Offsets auto-detect UE version. |
| 25 | `ueforge` | [ ] PE hook trampoline linear search: index by vtable pointer when 6+ hooks are installed | Hook dispatch O(1) by vtable pointer. |
| 25 | `ueforge` | [ ] Wave E: Global ProcessEvent pre-callback (RegisterProcessEventPreCallback wrapper + Queue::install_drain helper) | Guaranteed drain site available for status-effect migration. |
| 25 | `ueforge` | [ ] Wave E: AddUObjectCreateListener integration (~100 LOC) | CDO-revert-replay scenario unblocked. |
| 25 | `ueforge` | [ ] Leak-source helpers: lift top_packages / loaded_levels / process_regions from g2rpg's explore_leak_source.rs as ueforge::ue::probe extensions | Leak-source probes available to all mods. |
| 25 | `grounded2-mod` | [ ] Critical Chance + Critical Damage: before callback on damage hook returns multiplied damage on roll | Critical hit skill in catalog, functional in combat. |
| 25 | `grounded2-mod` | [ ] Evasion / Dodge: before callback returns 0 damage on roll for player-taken hits | Evasion skill in catalog, functional in combat. |
| 25 | `grounded2-mod` | [ ] Thorns: after callback resolves attacker's HC and writes damage fraction to its CurrentDamage | Thorns skill in catalog, functional in combat. |
| 25 | `grounded2-mod` | [ ] Leap Distance: verify the untested PlayerMovementMult over AirControl trio implementation | Leap Distance confirmed working in-game. |
| 25 | `grounded2-mod` | [ ] Auto-pickup (range): per-frame proximity scan picks up loose items | Auto-pickup skill in catalog with configurable range. |
| 25 | `grounded2-mod` | [ ] Stamina Pool + Stamina Regen: find UStaminaComponent offsets via Dumper-7 | Stamina skills in catalog. |
| 25 | `grounded2-mod` | [ ] Gear Hardiness: find durability-loss-per-use field for per-item durability scaling | Gear Hardiness skill in catalog. |
| 25 | `grounded2-mod` | [ ] Climb Speed: check if a separate CharMovement field exists before adding | Climb Speed feasibility documented. |
| 25 | `grounded2-mod` | [ ] Collision / Impact Damage Resistance: reduce/negate lethal self-damage from terrain/plant collision via damage hook | Collision resistance skill in catalog. |
| 30 | `grounded2-mod` | [ ] Optimize explore_status_effect_rows.rs: batch read_bytes so it runs in seconds; capture FName for each row | Discovery test runs in seconds. |
| 30 | `grounded2-mod` | [ ] Pick target row per stat type for status-effect migration | Per-stat target rows documented. |
| 30 | `grounded2-mod` | [ ] Implement SkillEffect::PlayerStatusEffect variant in skills.rs | PlayerStatusEffect variant available. |
| 30 | `grounded2-mod` | [ ] One generic apply arm in apply.rs that resolves table, looks up row by FName, mutates Value, calls CreateAndAddEffect | Generic status-effect apply path works. |
| 30 | `grounded2-mod` | [ ] Migrate impact_resistance first as proof of concept | impact_resistance uses status-effect surface. |
| 30 | `grounded2-mod` | [ ] Validate via regression test: bandages must heal even with impact_resistance enabled | Bandage healing works alongside impact_resistance. |
| 30 | `grounded2-mod` | [ ] Migrate remaining skills row-by-row: health_regen, max_health, lifesteal, attack_damage, armor, fall_resistance | All applicable skills use status-effect surface. |
| 30 | `grounded2-mod` | [ ] dmg-trace ring buffer in snapshot (last N multicast events; currently log-only) | Damage trace visible in snapshot JSON. |
| 30 | `grounded2-mod` | [ ] Player world location + equipped weapon in snapshot (context for fall / item-use simulation) | Location and weapon in snapshot. |
| 30 | `grounded2-mod` | [ ] High-frequency drain site for PE queue: kill_hook trampoline only fires on player HC vtable events; with impact_resistance mask, multicast drops to near zero; land Wave E1 | call ops no longer time out under low-traffic conditions. |
| 30 | `grounded2-mod` | [ ] Per-skill test coverage: refund reverts, toggle off reverts to vanilla, toggle on restores, persist across slot reload, level 0 no effect (only ~half covered) | All 13 skills have full lifecycle test coverage. |
| 30 | `grounded2-mod` | [ ] Interaction matrix tests: armor + heal, max_health + fall, fall_resistance + impact_resistance + rock collision, every HC field pair | Interaction tests pass for all stat combinations. |
| 30 | `grounded2-mod` | [ ] Error path tests: op while no slot, malformed args, port collision / second instance | Error paths tested. |
| 35 | `grounded2-mod` | [ ] Vortex / Nexus packaging: cargo deploy package produces zip layout; need Nexus listing | Mod available on Nexus with description and screenshots. |
| 35 | `grounded2-mod` | [ ] Pkg(0) instigator bug: some kills attribute to /Script/CoreUObject (Package) because LastDamageInfo.InstigatorController is unset; sample more kills, hook ApplyDamageFromInfo, or fall back heuristically | Player always gets XP for their kills. |
| 40 | `ueforge` | [ ] Buildings module Phase A1: Grounded 2 build system research (UProductionBuilding, BP_Building, UBuildingPlacementComponent, spawn UFunction, cost, build-menu, persistence) | G2 building system documented. |
| 40 | `ueforge` | [ ] Buildings module Phase A2: Outworld Station build system audit (same as A1; trait surface must fit both) | OWS building system documented. |
| 40 | `ueforge` | [ ] Buildings module Phase A3: Storage / inventory transfer research (find add-item-to-inventory UFunction) | Storage transfer path documented. |
| 40 | `ueforge` | [ ] Buildings module C1: BuildingDef + BuildingRegistry catalog | BuildingDef and registry available. |
| 40 | `ueforge` | [ ] Buildings module C2: BuildingsTracker with activate_slot / deactivate_slot + persistence via SlotStore | Tracker persists building state per slot. |
| 40 | `ueforge` | [ ] Buildings module C3: BuildingSpawner trait + game-impl skeleton | BuildingSpawner trait defined. |
| 40 | `ueforge` | [ ] Buildings module C4: Tick scheduler (per-instance closure fires every interval) | Tick scheduler advances building state. |
| 40 | `ueforge` | [ ] Buildings module C5: Standard debug ops (buildings_list / place / destroy / get / set_state) | Building ops available via HTTP. |
| 40 | `ueforge` | [ ] Buildings module C6: ImGui tab template (catalog + placed list + per-instance state) | Buildings tab renders in ImGui. |
| 40 | `grounded2-mod` | [ ] Buildings module D1: auto-fiber-harvester building (yields plant fibers at configured rate, transfers to nearest storage) | Auto-fiber-harvester spawns and yields in-game. |
| 40 | `grounded2-mod` | [ ] Buildings module D2: catalog row in buildings/catalog.rs | Harvester in building catalog. |
| 40 | `grounded2-mod` | [ ] Buildings module D3: G2 spawner impl | G2 spawner places and destroys buildings. |
| 40 | `grounded2-mod` | [ ] Buildings module D4: ImGui Buildings tab | Buildings tab renders in G2. |
| 45 | `ueforge` | [ ] Phase B+: programmatic Ctrl+R from cargo deploy install --hot (mod watches sentinel file; on change, synthesizes Ctrl+R via UE4SS register_keydown_event) | Hot reload triggered from build script. |
| 45 | `ueforge` | [ ] Phase B++: HTTP reload op that calls UE4SSProgram::queue_reinstall_mods directly | reload op triggers mod reinstall via HTTP. |
| 50 | `workspace` | [ ] Absorb TimberbornMods C# repo into modforge (subtree-merge, preserves history) | Timberborn source lives in modforge workspace. |
| 50 | `workspace` | [ ] Absorb Schedule1Mods C# repo into modforge (subtree-merge, preserves history) | Schedule 1 C# source lives in modforge workspace. |

## InputKey research: confirmed facts

These are verified facts from live testing. Do not repeat the failed approaches listed below.

### What InputKey IS

- `APlayerController::InputKey(const FInputKeyParams&)` is a native C++ virtual function.
- It is NOT a UFunction. Tested all three classes (PlayerController, PlayerInput, EnhancedPlayerInput) via `class_functions_by_name`. Zero matches. ProcessEvent cannot reach it.
- PlayerController has 162 UFunctions. Six are InputKey-related but read-only queries: `IsInputKeyDown`, `WasInputKeyJustPressed`, `WasInputKeyJustReleased`, `GetInputKeyTimeDown`, `GetInputAnalogKeyState`, `GetInputVectorKeyState`. All take FKey (24 bytes). None of them is InputKey itself.
- InputKey is in the vtable of both the controller and EnhancedPlayerInput objects.

### What the player chain looks like

- `live_player` (+0x2C8) -> Controller (+0x408) -> PlayerInput (EnhancedPlayerInput)
- EnhancedPlayerInput vtable has 120 entries. 16 are from the EnhancedInput plugin (slots 0, 9, 49, 87-99).
- ActionInstanceData at +0x598 on EnhancedPlayerInput is OUTPUT (writing to it does not move the player). Confirmed by `inject_forward_input` test.
- KeyStateMap is NOT in the +0x5E8 region. A write watchpoint on +0x5E8 / +0x5F0 caught zero writes on a key press; the earlier before/after diff there was a net change, not a store site. The real per-key FKey->FKeyState TSet is a separate heap block reached through a header on the object (see research.md section 31; the exact object offset is not confirmed reproducible).

### FInputKeyParams struct (unverified layout)

```
FKey (24 bytes):
  FName KeyName: comparison_index (i32) + number (u32) = 8 bytes
  TSharedPtr<FKeyDetails>: pointer (8 bytes) + ref count pointer (8 bytes) = 16 bytes
FVector2D Delta: likely double (UE5.4 LWC) = 16 bytes
float DeltaTime: 4 bytes
int32 NumSamples: 4 bytes
EInputEvent event: 4 bytes (0=IE_Pressed, 1=IE_Released)
FInputDeviceId: 4 bytes
bool bIsGamepadOverride: 1 byte + 7 padding
Total: 64 bytes (unverified, may be wrong)
```

### Failed attempts (do NOT repeat)

1. **Vtable slot guessing on EnhancedPlayerInput (slots 88, 94, 98).** Slot 94 crashed the game twice. The function at slot 94 reads `rdx+0xA8` (168 bytes into the params), which means it expects a struct much larger than FInputKeyParams (64 bytes). Slot 94 is NOT InputKey. Slots 88 and 98 were never tested because the approach was abandoned.

2. **Patternsleuth string xref for "InputKey".** Found the UTF-16 string "InputKey" at 17 locations in .rdata and the UTF-8 version at several more. Zero lea xrefs to any of them in .text. The function does not reference the string "InputKey" with a direct `lea reg, [rip+disp32]`. Also tried "APlayerController::InputKey", "IE_Pressed", "FInputKeyParams" as search strings. "APlayerController::InputKey" and "FInputKeyParams" not found in rdata at all. "IE_Pressed" found once but also zero xrefs.

3. **Prologue byte scan for controller vtable slot 130.** Found a unique match at `0x7ff6a4b0d680` for prologue `40 55 57 41 57 48 8d 6c 24 b9 48 81 ec f0 00 00 00 f6 41 58 10`. Called it on the controller with FInputKeyParams for W. Returned true, no crash, but player did not move. This function might be InputKey (it reads controller+0x1A0 which is PlayerInput, and delegates to PlayerInput vtable), but the call produced no movement. Possible cause: null TSharedPtr<FKeyDetails> in the FKey struct, or wrong FInputKeyParams layout.

4. **Calling controller vtable slot 130 directly.** Returned true but no movement. Nothing proves this function IS InputKey. The prologue scan found A function, not THE function.

### What is needed next

The address of `APlayerController::InputKey` must be found definitively, not guessed. Options that have not been tried:
- Check if MISERY ships with a PDB or has debug symbols
- Use RTTI (vtable[-1] has the MSVC Complete Object Locator with the class name) to map the controller vtable to known class hierarchies, then cross-reference with UE5.4 source for the InputKey slot index
- Hook a candidate function and verify it fires when a real physical W key press happens (requires knowing a candidate address, which we have from the prologue scan)
- Find what populates KeyStateMap (+0x5E8 on EnhancedPlayerInput) by scanning for instructions that write to offset 0x5E8, then trace the call chain backward to InputKey

### Write watchpoint: the method that works (2026-08-28)

Stop guessing addresses and calling them. Watch the memory a real
key press changes and read back which instruction wrote it. The
`watch_writes` op (`modforge::winproc::capture_write_watchpoint`)
arms a CPU debug register on the address across all threads, and
records each write's instruction plus stack call chain as
image-relative offsets. Verified live: arms 123/123 threads.

Tapping W caught 24 writes to the ForwardInput trigger byte
(ActionInstanceData entry + 0x10) from one instruction and one
call chain. The value at entry + 0x40 and the + 0x5E8 region
caught nothing, so those are not the key-press write target.
Captured writer (RVA into MISERY-Win64-Shipping.exe): instruction
+ 0x42f14d2; callers + 0xf590f8, + 0xf4dd41, + 0xf6682d,
+ 0x11af012, + 0x7bf8140, + 0x7be5918, + 0x3cb9443. This is the
Enhanced Input TRIGGER-evaluation path (it writes ForwardInput's
trigger byte), which runs AFTER raw key input, so it is NOT
InputKey. Full detail in `misery-mod/docs/research.md` section 31.
InputKey itself is still not located; see that section for the
open state.
