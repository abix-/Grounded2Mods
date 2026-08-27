# modforge workspace open issues

Survivalist extraction means moving existing engine-independent code out of the mod. Do not invent new planner, queue, registry, definition, or state types just to generalize it. Prefer direct code moves and small pure functions over new framework APIs. Survivalist keeps its existing game-specific control flow, Unity access, content, and behavior. An extraction row is complete only when the reusable code has moved without adding a speculative system.

| Priority | System | Todo | Done when |
|---:|---|---|---|
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
| 8 | `modforge` | [ ] I-6: Replay format: JSON event stream [(t_ms, event, args)]; record, save, replay | input.replay op plays back recorded event streams. |
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
| 25 | `ueforge` | [ ] tiny_http / ureq 2 migration window (both on 2-5 year support horizon) | HTTP dependencies evaluated and migrated if needed. |
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
