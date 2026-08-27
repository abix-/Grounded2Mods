# Changelog

> **Authoritative on:** chronological history of milestones,
> investigations, and design pivots across every crate in the
> workspace. Things that are DONE.
>
> For what's next see [`todo.md`](todo.md). For per-subject
> deep dives see each crate's `docs/` folder:
> [`../ueforge/docs/`](../ueforge/docs/) for the framework,
> [`../grounded2-rpg/docs/`](../grounded2-rpg/docs/) for the
> Grounded 2 mod.

Newest first.

## Changelog rules

- Each day has exactly one `## YYYY-MM-DD` heading and exactly one table below it.
- Every daily table has exactly three columns: `System`, `Item`, and `Done when`.
- Start every Item with `[x]`.
- Never combine completed items into one changelog row.
- Put new days above older days. Add rows to the existing table when that date already exists.
- Entries before 2026-08-17 keep their existing historical format.

## 2026-08-27

| System | Item | Done when |
|---|---|---|
| `modforge::client` | [x] Extend the action journal to injected games with recorded operation actions, condition waits, and assertions in versioned JSON | Five tests prove round-trip, ordered replay, polling to an observed value, replayable recording, operation failure evidence, and assertion failure evidence. |
| `misery-mod` | [x] Make MISERY the first live action-journal proof | Against the running game, the test recorded a player movement-speed write, waited for the generic memory read, asserted it, restored the exact original bytes, saved and loaded the journal, replayed it, and restored the original value again. |
| `workspace` | [x] Update Rust and direct dependencies to current stable releases | Rust 1.98 is required, the lockfile is refreshed, ureq 3 and the other major-version updates compile across all workspace targets and features, and the local hudhook fork includes upstream 0.9.2 plus its Present1 patch. |
| `ueforge::transform` | [x] Skip an actor whose mesh pointer does not resolve, instead of measuring whatever it lands on | Live: a square that reported two parts at 6.8e36 with NaN now reads 98 clean parts, the log counts 3 skipped, and the test asserts no `<bogus-fname>`, no NaN, nothing past 10 km. |
| `ueforge::parts` | [x] Every placed part read from a level carries its pivot, from the same ExtendedBounds read as its extent, position-converted (handedness flip) unlike a size | Live: 74 of 98 parts in one square carry a pivot; `SM_Floor_400x400` reads extent [2.0, 0.11, 2.0] pivot [2.0, -0.09, -2.0], agreeing with parts.json. |
| `modforge::structure` | [x] Hard cutover from the word "piece" to "part" across the building kit: types, functions, ops, filenames and docs | `PartDef`, `PartShape`, `ueforge/src/ue/parts.rs`, `misery-mod/docs/parts.md`, the `level_parts` and `place_parts` ops, and a clean `cargo check --workspace --all-targets`. |
| `ueforge::assets` | [x] Put every mesh's pivot in the parts list, so a size is no longer the only thing known about a part's faces | `parts_list` reports 2407 of 2407 with a pivot in 2.41 s live, and `SM_Wall_100x100`, `SM_Floor_400x400` and `SM_Pillar` match the tables in parts.md that were measured by the loaded-mesh probe. |
| `unityforge` | [x] Move Schedule 1's guarded main-thread effect wrapper into Unityforge | Schedule 1 keeps its crash-bisection flag, labels, timeout, and effect configurations without implementing Unity queue dispatch or effect delegation. |
| `modforge` | [x] Replace Schedule 1's manual capped combat-trace vector with Modforge's bounded ring | Schedule 1 records, snapshots, and clears combat events with the same drop-oldest ordering and atomic snapshot-and-clear behavior without a local bounded-collection loop. |
| `modforge` | [x] Move Schedule 1's fixed-capacity timestamped recent-key tracking into Modforge's ring system | Schedule 1 configures NPC hit and credit windows without implementing timestamp storage, rotation, or window lookup. |
| `modforge` | [x] Move Schedule 1's lowest-level automatic skill spending into Modforge's RPG tracker | Schedule 1 chooses to auto-spend earned points and invokes the tracker operation without implementing catalog selection or spending loops. |
| `unityforge` | [x] Move Schedule 1's bridge-handle decoding and managed-object wrapping into Unityforge | Loot, farming, kill credit, and combat tracing use Unityforge's owned and borrowed handle helpers while Schedule 1 keeps its game classes, fields, and policy. |
| `modforge` | [x] Move Schedule 1's three-dimensional coordinate decoding into Modforge's shared value parsing | Schedule 1 uses Modforge for engine-independent Vector3 parsing while Unity bridge values and game position usage remain outside Modforge. |
| `cs-shim-il2cpp` | [x] Replace the BepInEx IL2CPP shim's private Rust DLL lifecycle with the shared generation loader | The shim links `cs-shim-common/GenerationLoader.cs`, uses it for initial load, per-frame ticking, shutdown, and generation-file hot reload, and retains only BepInEx and IL2CPP host wiring. |
| `modforge` | [x] Replace the vanilla-invoke design draft with shipped API documentation | The page matches the current signature, invoker, operation, error, and calling-thread behavior. |
| `misery` | [x] Autoload reaches the operator's own save: `LoadLevel` on `BP_SingleplayerNewGameMenu`, not the host-a-server object | A launch lands in the save with no keys and no clicks, emission count 42 rather than 1. |
| `modforge` | [x] Time work by name, off by default, with `timing` and `timing_report` controls every forge registers | A 30 second window prints every named job slowest first, and off records nothing. |
| `ueforge` | [x] Object-reading controls run on the game thread; nine sites, five routed and four found later | A poll through a full level load no longer faults inside `find_objects_by_chain`. |
| `ueforge` | [x] `LevelStreamer` answers which regions are loaded from the game's own array instead of searching every object | A tick with nothing new does no object search, `ue:objects_read` absent from the report. |
| `ueforge` | [x] `NewLevels` reports only the regions that appeared since last time | `spawning` went from 246 ms a pass to 5.97 ms. |
| `ueforge` | [x] A load watcher asks `world_is_up` instead of re-running its finder to learn one bit | The mod held the game thread for 0.04 ms per second, down from 126. |
| `ueforge` | [x] `LiveActor` finds an actor once and keeps it until the world ends | The Speed tab open for 30 seconds does zero object searches. |
| `modforge` | [x] `ReadOnce` keeps anything worked out once, cleared when the world ends | The Speed tab reads the game zero times in 30 seconds of redrawing. |
| `ueforge` | [x] A repeating job can end itself with `PollerHandle::stop_soon`, and a hook can be removed with `hook::remove` | The notice watcher dismisses once, removes its hook and stops, all in the same second. |
| `misery` | [x] The gameplay tab fills itself when a world loads and reloads on a new one | Operator confirmed the tab comes up filled with no Refresh click. |
| `ueforge` | [x] `FMalloc::Malloc`'s vtable slot MEASURED from the running image rather than guessed | Three call sites agree on slot 5, and a vendor pass adds 73 items with no `grow failed`. |
| `misery` | [x] The vendor price array comes from the engine allocator, not Rust's | An audit of every crate found one other site that stored a Rust pointer in engine memory, and it was this one. |
| `ueforge` | [x] Build an `FName` from a string through the engine's own constructor, find-only | `"StaticMesh"` round-trips to the same text and an invented name comes back missing. |
| `ueforge` | [x] Read cooked asset registry tags without loading the asset | `ApproxSize` returns `320x320x320` for a mesh that was never loaded. |
| `misery` | [x] THE PARTS LIST: every shipped mesh with a size and a shape, nothing loaded | 2,407 meshes on disk, none without a size, and the classifier calls a 400x401 wall a Panel. |
| `modforge` | [x] Derive per-piece studs from observed joins, each in its own local frame | A different piece seen in the same place gets the same stud, and turning the assembly invents none. |
| `ueforge` | [x] Guard the per-actor read in `read_level` with `seh::guard` | An actor whose mesh pointer does not resolve is skipped and counted instead of killing the game. |

## 2026-08-26

| System | Item | Done when |
|---|---|---|
| `modforge` | [x] Move Survivalist's incursion geometry calculations into Modforge | Modforge owns centroid, spread, radial edge-position, and nearest-destination calculations while Survivalist keeps observation, constants, spawn policy, band creation, equipment, relationships, movement, logs, and chronicle text. |
| `modforge` | [x] Move Survivalist's existing goods filter into Modforge's item system | Survivalist keeps valuable-first selection, transfer loops, capacity checks, rescans, inventory discovery, item reads, Unity calls, and carrying behavior without speculative transfer-planner types. |
| `modforge` | [x] Move Survivalist's first-reachable-with-capacity calculation into Modforge | Survivalist keeps destination order and eligibility, headroom mutation, Unity transfers, role updates, conscript marking, logs, and chronicle text without speculative population-planner types. |
| `modforge` | [x] Move Survivalist's delayed craft-result collection advancement into a small Modforge function | Survivalist keeps craft jobs, queue authority, readiness and deadline fields, hooks, handles, result lookup, odds, and item swaps without a duplicate planner wrapper. |
| `modforge` | [x] Add deterministic identity-and-salt selection and migrate Survivalist's stranger choices and text variants | Survivalist supplies identities, thresholds, salts, and text catalogs without a local hash or bounded selection implementation. |
| `modforge` | [x] Add rectangular annex planning and migrate Survivalist's development planner | Modforge owns rectangle expansion and perimeter, gate, interior, and blockage planning; Survivalist supplies its live terrain and construction policy and executes the resulting work. |
| `unityforge` | [x] Add checked Rust and C# boundary helpers and migrate Survivalist's upgrade exports | Unityforge owns UTF-8 input, UTF-16 paths, returned strings, panic containment, and result conversion while Survivalist keeps upgrade scopes, calls, reports, and save-path choice. |
| `unityforge` | [x] Replace Survivalist's local blocking main-thread helper with Unityforge's queue authority | Survivalist supplies closures and timeout labels without implementing its own oneshot state, polling loop, sleep, or timeout. |
| `unityforge` | [x] Add reusable managed-object and managed-list helpers and migrate Survivalist's common wrappers | Survivalist names game fields and applies game policy without local JSON handle decoding, unsafe owned or borrowed wrapping, raw Count and Item traversal, or bridge-coordinate parsing. |
| `modforge` | [x] Move Survivalist's horde adaptive-pressure planning and lifecycle into Modforge | Modforge owns target eligibility and ranking, tier and position selection, event caps, duplicate-target exclusion, tracking, and pruning; Survivalist keeps Unity observation and execution, content values, logs, and chronicle text. |
| `modforge` | [x] Move Survivalist's settlement pressure classification into Modforge | Modforge owns the survival rungs and classification from nutrition, population loss, and threat pressure; Survivalist supplies its thresholds and Unity observations and applies every response. |
| `modforge` | [x] Move Survivalist's repeated reinforcement dispatch loops into Modforge | Modforge fans one outcome across traits, voters, and faction aggregates for survival, steal, trade, scavenge, murder, and robbery while Survivalist keeps judgment, trait choice, direction, magnitude, Unity effects, logs, and chronicle text. |
| `modforge` | [x] Move Survivalist's repeated franchise ballot arithmetic into Modforge | Modforge owns ballot accumulation, strict-majority decisions, mean score, and voter identity collection for survival, steal, trade, scavenge, murder, and robbery while Survivalist keeps member enumeration, eligibility, thresholds, actions, and outcomes. |
| `modforge` | [x] Expand the mission system and migrate Survivalist's courier, settler, stranger, and robbery lifecycles | Modforge owns multi-stage transitions, one-stage missions, collection advancement, timeout callbacks, error removal, and cleanup while Survivalist keeps Unity access, game decisions, outcomes, presentation, and concrete handle cleanup. |
| `modforge` | [x] Move Survivalist's upgrade state and policy into Modforge and bind the C# effects to the active Rust generation | Modforge is the only authority for scoped levels, cost and skill math, diminishing returns, schema, and persistence; Survivalist keeps game applicability, patches, menus, effects, save-path selection, and object access without a duplicate level cache. |
| `modforge` | [x] Make uniqueness a property of `ItemDef` and move Survivalist's unique-item ledger into Modforge | `ItemRegistry` remains the only item catalog; Modforge owns the entered set, holder ledger, schema, and persistence keyed by item name while Survivalist keeps game entry rules, inventory access, delivery, and chronicle text. |
| `ueforge` | [x] Add raw TArray clone-and-append support through Unreal's allocator and migrate MISERY's vendor list growth | MISERY owns vendor-entry offsets and byte patches without implementing allocation, growth, cloning, or append mechanics. |
| `modforge` | [x] Add an engine-independent vendor offer planner and migrate MISERY's vendors | Modforge owns percentage pricing, globally unique assignments, buy-list mirroring, and caller-supplied special offers; MISERY supplies its vendor, item, food, currency, price, and sewing-kit policy and applies the plan. |
| `modforge` | [x] Move MISERY's phenomenon planning into Modforge | Modforge owns weighted selection, reward-danger constraints, count and cluster rolls, region lifecycle, caps, and placement requests; MISERY keeps Blueprint catalogs, live-square discovery, ground traces, and Unreal spawning. |
| `modforge` | [x] Move MISERY's adaptive encounter planning into Modforge | Modforge owns budget configuration, copy, escalation and pack selection, caps, anchor selection, and scatter placement; MISERY supplies live progression and creature snapshots and executes spawn requests through Ueforge. |
| `ueforge` | [x] Add typed TMap key and value traversal and mutation, then migrate MISERY's movement-speed map access | MISERY keeps its inventory offsets, movement keys, base speeds, multiplier policy, and UI without implementing Unreal TMap layout. |
| `ueforge` | [x] Add a reusable struct-field editor and migrate MISERY's gameplay field controls | Ueforge owns cached refresh, double sliders, booleans, caller-supplied ranges, and writes; MISERY keeps only its field definitions, object accessor, ranges, text, and tab wiring. |
| `ueforge` | [x] Move MISERY's standard call and queue-operation registration into Ueforge's game-thread control plane | MISERY starts only its queue; Ueforge registers and reports the shared `call`, `pe_ping`, and `pe_stats` operations. |
| `ueforge` | [x] Move MISERY's asset inventory and asset-loading operation handlers into Ueforge | MISERY has no local asset operation wrapper; Ueforge owns filtering and game-thread routing so any Ueforge mod can register the standard operations. |
| `ueforge` | [x] Move Grounded 2's existing parameterized survival-drain effect into Ueforge's standard RPG effects | Ueforge owns the unchanged level-scaled calculation, class-default walk, field write, log, and formatting while Grounded 2 keeps the class, offsets, vanilla values, settings access, tuning, and catalog statics. |
| `ueforge` | [x] Move Grounded 2's existing parameterized fall-damage effect into Ueforge's standard RPG effects | Ueforge owns the unchanged vanilla capture, class walking, synchronized field writes, log, and formatting while Grounded 2 keeps every class reference, offset, cache, tuning value, and catalog static. |
| `ueforge` | [x] Move damage-hook event conversion, timestamping, and ring recording out of Grounded 2 | Grounded 2 supplies its live CurrentDamage observation and keeps its trace logging policy without constructing or timestamping the shared debug event. |
| `ueforge` | [x] Move Grounded 2's existing active status-effect traversal into Ueforge | Ueforge walks the component and effect array, resolves row handles and FNames, looks up data-table rows, and reads Type and Value while Grounded 2 keeps every layout offset, first-player selection, and snapshot presentation. |
| `ueforge` | [x] Move the existing parameterized ImpactReversalEffect out of Grounded 2 and into Ueforge's standard RPG effects | Ueforge owns the unchanged damage-event filtering, level scaling, post-damage health reversal, log, and formatting while Grounded 2 keeps its damage layout, health offset, marker, tuning, and catalog static. |
| `ueforge` | [x] Move the existing parameterized LifestealEffect out of Grounded 2 and into Ueforge's standard RPG effects | Ueforge owns the unchanged damage-event filtering, level scaling, health-component write, log, and formatting while Grounded 2 keeps its player reference, offsets, tuning, and catalog static. |
| `ueforge` | [x] Lift transient live-object lookup, checked byte-buffer UFunction calls, and game-thread live-object hook installation from MISERY | MISERY autoload retains its save checks, Blueprint names, and parameter layouts; notice suppression retains its exact class filter, re-entry guard, dismissal handler, and logs while Ueforge stops its poller and removes its hook after dismissal. |
| `misery` | [x] Nag screen bypassed for real: call the notice's own spacebar handler, `InpActEvt_SpaceBar_K2Node_InputKeyEvent_1`, instead of hiding the widget | Operator confirmed two cold starts reaching the game with no key pressed. Log shows the handler firing and `ItemList` resolving 2s later, where the previous build timed out after 30s and went silent. |
| `misery` | [x] Fix research_nag::nag_class_detail: it read the startup discovery cache, which this class is absent from; read the live class via `nag_stats` instead | Test prints the four live functions on `WD_PlaytestNote01_C`. |
| `ueforge` | [x] Add `UFunction::parms_size` and `UFunction::num_parms` | Callers size a parm block from the function instead of assuming; the notice handler declares 1 parm of 24 bytes (an `FKey`). |
| `ueforge` | [x] Reach the game thread with no world loaded: `hook::engine_tick` patches the `UEngine::Tick` vtable slot | Live at the main menu: 1756 ticks, 0 panics, and every queue drain came from the tick. This is what UE4SS does for itself (`HookEngineTick = 1`); its C++ mod API exposes no way to share that hook. |
| `ueforge` | [x] Wire UE4SS's `on_update` per-frame callback (`frame::on_update`), previously an unused virtual in the shim | Fires every frame regardless of world state. Used for polling only. |
| `misery` | [x] Prove `on_update` is NOT the game thread and stop draining the queue from it | Thread ids measured live: UE4SS 20928, `UEngine::Tick` and ProcessEvent both 21488. Calls made from on_update loaded a save and then crashed the session. research.md 26.6. |
| `misery` | [x] Load a save from the mod with no clicks: `SGK SetLoadSaveGame(true)` then `LoadLevel()` on the live `BP_HostLoadGameServer` | Operator confirmed Save 1 loaded and played. research.md 26.7, `research_load::load_current_slot`. |
| `ueforge` | [x] Find the disconnect crash: `ue::tarray::grow_raw` allocated with `std::alloc` and handed the pointer to an engine `TArray`, so the engine reallocated memory `FMallocBinned2` never issued | `Attempt to realloc an unrecognized block ... canary == 0x65 != 0xe3`. Vendor arrays grown at load, crash at teardown. research.md 26.8. |
| `ueforge` | [x] Add `ue::gmalloc` and stash the `GMalloc` global at init from the scan that already runs; `grow_raw` fails loudly rather than falling back to Rust's heap | Growing with the wrong allocator is worse than not growing. Resolving it lazily instead fired a patternsleuth scan on the game thread mid-vendor-pass and crashed the game. |
| `ueforge` | [x] Leave `MALLOC_SLOT` as `None` until measured: slot 2 was inferred from patternsleuth's pattern bytes (`call [rax+0x10]`) and was wrong live, returning null and then crashing | An unverified vtable slot is a call to an arbitrary engine function with arbitrary arguments. Vendors lose their added items until the slot is measured; nothing else is affected. |
| `misery` | [x] Note that auto-load uses `BP_HostLoadGameServer`, the host-a-server path, not the singleplayer one | Found by the operator asking which had loaded. Recorded in todo, not yet fixed. |
| `modforge` | [x] Rename the 10 `Spec` types to `Def` (`PieceDef`, `RoomDef`, `SolidDef`, `StairDef`, `LightDef`, `PropDef`, `ScatterDef`, `BuildDef`, `GameDef`, `InjectorDef`) | `architecture.md` documents `Def` as the one convention and never mentions `Spec`; the split was drift, not design. |
| `ueforge` | [x] Lift actor transforms out of misery into `ue::transform`: `RootComponent` to `USceneComponent`, location / rotation / scale, static mesh and its half-extent, `set_mobility`, `set_static_mesh` | Engine layouts, not MISERY's, and ueforge had none of it. `harvest.rs` 531 to 471 lines. Verified live: `piece_shapes` still reports sane metre sizes off real actors. |
| `ueforge` | [x] Lift the asset registry into `ueforge::assets`: `assets_of_class`, `load_asset`, `class_path`, `AssetEntry` | Pure engine APIs (`AssetRegistryHelpers`, `AssetRegistry:GetAssetsByClass`, `KismetSystemLibrary:LoadAsset_Blocking`). misery's `assets.rs` 248 to 102 lines, keeping only the two ops and their game-thread routing. Live: 2398 static meshes reported, 936 loaded. |
| `misery` | [x] Auto-load the saved game on launch (`src/autoload.rs`) | Live: nag dismissed at :58, `autoload: loading the saved game` at :00, operator in game with no keys or clicks. Runs once per launch. |
| `misery` | [x] Guard auto-load so a missing save cannot become a new game: abort unless a slot is held, `FindExistingSave` confirms it, and the load flag reads back set | `LoadLevel` is also the New Game path. `FindExistingSave` answers in byte 16, measured live: 1 for "Save 1", 0 for a null slot. |
| `ueforge` | [x] Lift the whole game-thread setup into `ueforge::game_thread::serve(&queue)`: find the engine, resolve Tick, derive the vtable slot, install, drain | misery and grounded2 share one implementation; nothing game-specific left in either. |
| `ueforge` | [x] Derive the `UEngine::Tick` vtable slot instead of hardcoding 95: resolve the address with patternsleuth, then find the slot holding it | Log shows `GameEngine::Tick at 0x7FF6426C4030 (vtable slot 95)`, matching UE4SS's own scan. A failed scan installs nothing rather than guessing. |
| `misery` | [x] Remove the ProcessEvent hook on the player character and its retry poller; the engine tick serves the queue always | Its only job was draining, which now happens every frame regardless of world state. |
| `grounded2` | [x] Remove `drain_pending()` from `kill_hook` and `fall_hook` | Closes the open "high-frequency drain site" item: the queue no longer depends on gameplay events firing. Compile-verified only, not run in-game. |
| `workspace` | [x] Stop three mods sharing one build artifact: `misery-mod`, `grounded2-mod` and `outworld-station-mod` all declared `[lib] name = "main"` | They overwrote each other in target and cargo then treated the loser as fresh, so a deploy shipped whichever built last. The Grounded 2 mod went into MISERY and crashed it. Each now builds under its own name and deploys as `main.dll`. |
| `modforge` | [x] `cargo deploy` reads the crate's cdylib target name from `cargo metadata` instead of assuming `main.dll` | Deploy follows the per-crate artifact name. |
| `misery` | [x] `restart.ps1` refuses to deploy a DLL that does not contain `misery_mod.log` | Size and timestamp cannot tell two mods apart; the compiled-in log name can. |
| `ueforge` | [x] Correct lifecycle.md: per-package `target_dir` does NOT prevent cdylib collisions | It only applies to `cargo deploy`; a plain `cargo build -p` writes to the shared workspace target. That wrong belief is what allowed the crash. |
| `ueforge` | [x] Rename `pe_queue::DrainSite` to `GameThread` | Invented word replaced with Unreal's own (`ENamedThreads::GameThread`, `IsInGameThread`). |
| `ueforge` | [x] Add `ue::actor::find_objects_by_chain`: class-chain search with no PersistentLevel requirement | Widgets and the game instance are not actors; `find_actors_by_chain` could not see them. |

## 2026-08-25

| System | Item | Done when |
|---|---|---|
| `misery` | [x] Add all ammo and magazine items to the GunDealer's sell list, auto-applied on load | Player can sell any Ammo_ or Magazine_ item at the GunDealer, verified live: all 13 types on his sell list at 40% of buy cost. |
| `misery` | [x] Buy-to-sell mirror: every item a vendor sells, he also buys back at 40% of his charge price | vendor_mirror log lines show entries added on each vendor; dump test shows mirrored items with 40% prices. |
| `misery` | [x] Custom pricing for added sell entries: private price array per entry at SELL_PRICE_PCT (40%) of the item's vanilla buy cost | Live dump shows added GunDealer entries paying 52/60/76/100/104 rubles matching 40% of vanilla costs; vanilla entries unchanged. |
| `misery` | [x] One dedup vendor pass: vanilla sell lists, then buy mirror, then GunDealer ammo, then Barman food; each item sellable at exactly one logical vendor | no_duplicate_vendor_entries test passes live: 0 within-list, 0 cross-sell, 0 cross-buy duplicates. |
| `misery` | [x] Sewing kit permanently sold by the ResourseSaler (50 rubles, stock 1) | Live dump shows Resource_SewingKit on his buy list at 50 rubles. |
| `misery` | [x] Duplicates test for vendor lists | no_duplicate_vendor_entries in research_vendors.rs asserts within-list and cross-vendor uniqueness, with the vanilla Weapon_TOZ exception documented. |
| `misery` | [x] Fix vendor hooks never firing: find_actor matches class names exactly, so base-class finders never matched subclass vendor actors | misery_mod.log shows the vendors hook applying after load; previously zero vendor lines ever logged. |
| `misery` | [x] Fix research test client: server state field is now a bool, not a Snapshot struct | research_vendors tests run against the live control plane again. |
| `ueforge` | [x] Add find_actors_by_chain: inheritance-aware actor search matching a class anywhere in the class chain | misery vendor pass finds all 7 vendors via BP_MasterVendorBuildPart_C. |
| `misery` | [x] Game-thread dispatch: pe_queue DrainSite drained from a ProcessEventHook on the player character class | research_dispatch::game_thread_ping passes live: 205k trampoline fires, zero panics, a worker-thread job executed on the game thread. |
| `misery` | [x] Drain site class identified: BP_SGKMasterCharacter_C, installed from the live instance's vtable with retry-backoff | research.md 26.2; fires only while a save is loaded, so main-menu dispatch would need a separate pre-menu site. |
| `misery` | [x] Fix PROCESS_EVENT_IDX: 0x4C was wrong for this game, true index 0x4D measured from the GameInstance vtable | research.md 26.1; explains hooks that never fired and call_ufunction returning Ok with no effect. |
| `misery` | [x] NPC spawning research: enemies are placed in the world preset tiles, not spawned; one BP_DwarfSpawn_C point in the hub | research.md sections 25.0 to 25.4, live census via research_spawners tests. |
| `misery` | [x] Live writes to the hub spawn point (count 1 to 5, class to BP_Swamper_C) verified by read-back | set_spawn_point_more / set_spawn_point_entity; whether the game re-reads them is open. |
| `ueforge` | [x] Add walk_class_chain op and ProcessEventHook::install_for_object (survive Blueprint reinstancing) | Chain walk finds actors walk_class misses; hook patches the live instance's vtable. |
| `misery` | [x] Wire the framework call op through the game-thread drain | UFunction calls from the control plane execute on the game thread (register_pe_call in dispatch.rs). |
| `misery` | [x] Spawn an NPC from the mod: SpawnAIFromClass via the call op, donor class copied from a live hostile | research_spawn::spawn_one_npc passed live: non-null pawn, census 77 to 78, spawned Assembly confirmed in-world. Recipe in research.md 26.3. |
| `misery` | [x] Scaling NPC spawner: per-square random threat budget growing with EmissionsPast, cross-biome escalations, packs, quiet rolls; doubling average at emission 30 | Live at emissions 42: squares rolled 9/8/8/0 extras with a zombie soldier pack; log shows one roll per square. research.md 25.5. |
| `misery` | [x] Fix spawner square keys: class prefix in full names made every NPC class its own square, over-spawning | Post-fix log shows one roll per square with true vanilla counts. |
| `misery` | [x] Fix pe_dispatch install blocking the features chain at main menu: backoff moved to its own thread | Features after pe_dispatch register immediately; log shows all features watching before load. |
| `misery` | [x] Measure area grids: Factory 2x2 (4 squares), Bunker/Meadows/Paneli 3x3 (9), Meadows has an 18-preset pool | research_spawners::dump_generator_grids, live values in research.md 25.4. |
| `misery` | [x] Worldgen doc split: worldgen.md is the authority for generators, grids, pools, square catalog | research.md sections 19 and 25.4 are pointers; skill updated. |
| `misery` | [x] Forced world regeneration on demand via GenerateCustomBiom on the game thread; area mapping swept (0 bunker, 2 meadows, 3 paneli, 1/4 dead) | research_worldgen tests; each forced regen ticks the global shining count. worldgen.md 7. |
| `misery` | [x] Cross-area square mixing: a Meadows square generated inside a Paneli world via one pool element write | pool_swap_meadows_into_paneli passed live: 4452_4_7.L_VehCemetry_Bridge on the Paneli grid. worldgen.md 8. |
| `misery` | [x] Mixed-pool area: all nine Paneli slots filled with a 6 Meadows + 3 Town blend, world generated mixed | mixed_pool_area passed live: Village_Dwarf_Hole among swamp towns and anomaly houses. worldgen.md 8.1. |
| `misery` | [x] Tile sizes measured: Factory 16500, Bunker 4800, Meadows and Paneli 12000; complete 45-square catalog dumped | worldgen.md 3 and 4; Meadows/Paneli squares interchangeable. |
| `misery` | [x] Size-mismatch probe: an oversized Factory square in the town grid streams fine but blocks traversal | Operator-confirmed blocked path; cross-size mixing ruled out. worldgen.md 8.2. |
| `modforge` | [x] Structures can be CAPTURED as well as authored: StructureDef gains pieces (PieceSpec), footprint measures them, arrange is public | modforge builds, 243 lib tests pass (1 pre-existing unrelated input failure). Generic arrangement rules now serve host-game structures. |
| `misery` | [x] Harvest pieces out of a live square with transforms and mesh identity, and rebuild them elsewhere | research_harvest::harvest_and_compose: 564 pieces read, 60 rebuilt, operator confirmed solid and real in-game. worldgen.md 9. |
| `misery` | [x] Capture structures from squares into a library and generate monuments with modforge's arrangement rules | Live: 342-piece Clustered and 194-piece AlongRoad monuments built from structures harvested out of other squares. worldgen.md 9.4. |
| `misery` | [x] Alternate-reality overlays: per-square phenomena (anomaly fields, teleport nests, black holes, camps, caches) with rewards guarded by dangers | Live in the bunker world: rolls logged per square, 28 props placed, no crash. src/strange.rs. |
| `modforge` | [x] Shape classifier: a piece's role read from its box proportions (Slab, Panel, Post, Beam, Block, Clutter) | 9 unit tests; engine-agnostic, works from measured extents alone. |
| `modforge` | [x] shell_slots: decompose a RoomSpec into floor tiles and wall segments with openings assigned, greedy module fill (7 m becomes 4+2+1) | 8 unit tests; 260 modforge lib tests pass (1 pre-existing unrelated input failure). |
| `misery` | [x] Found and characterised the game's modular building kit: walls, doors, windows, corners, floors, stairs on a 1/2/4 m grid | research_kit: 12 walls, 5 doors, 4 windows, 6 floors loaded; pivot rule proven on 9 walls (names ARE dimensions, pivot at bottom start corner). |
| `misery` | [x] Build a generated room from the kit: modforge decides the shell, misery binds slots to meshes | Operator confirmed in-game: four walls closing, doorway, windows, floor. worldgen.md 9.6. |
| `misery` | [x] Fix ground trace reading FHitResult ImpactPoint Y instead of Z | Everything ground-placed was at the wrong altitude; z came back equal to y. worldgen.md 9.6. |
| `misery` | [x] Fix yaw conversion: the modforge to UE map is a reflection, so angles negate rather than offset | Two of four walls ran backwards before the fix. worldgen.md 9.6. |
| `modforge` | [x] Add poller::spawn_interval: a generic stoppable interval worker (stop flag, condvar wake, join, panic count) reusing the existing PollerHandle and registry | SlotPoller only fitted slot tracking; every mod background loop can now be stopped at shutdown. |
| `misery` | [x] Convert all four watchers (dispatch, spawning, strange, places) from raw threads to stoppable workers, stopped at shutdown order 50 before hooks tear down at 100 | Raw threads keep executing freed code after a DLL unload; they also queued into a drain nobody served once hooks died. |
| `misery` | [x] Attempt hot reload, diagnose the remaining crash | Swap now completes cleanly and UE4SS restarts the mod, but the reloaded image dies in patternsleuth's scan (rayon's global pool holds first-image code). restart.ps1 stays the supported path. research.md 26.4. |
| `misery` | [x] Add reload.ps1 with real verification (staged file consumed, main.dll actually changed, control plane back, game alive) | Fails loudly rather than reporting a reload that did not happen. Currently the game does not survive it. |
| `misery` | [x] Query the game's asset registry: every shipped asset, loaded or not, plus load-on-demand | Live: 2398 static meshes exist where 869 were loaded; 55 wall pieces where a memory walk saw 12, including 45-degree corners. src/assets.rs, pieces.md. |
| `misery` | [x] Suppress the playtest notice without synthesising a keypress | Hooks the widget's own class to reach the game thread at the main menu, where the usual drain site does not exist. research.md 26.5. |
| `misery` | [x] Document two crash causes found doing it: Blueprint widget classes share one vtable, and destroying a hooked object calls a pure virtual | Handlers must check what they are handed; never RemoveFromParent a hooked widget. research.md 26.5. |

## 2026-08-20

| System | Item | Done when |
|---|---|---|
| `modforge` | [x] Add InteractKind (Door, Pickup, Container), prompt_for, and interact function to modforge::hud: modforge decides the prompt text and the interaction result, the consumer executes the engine side | modforge builds and 6 new tests pass covering prompt text per kind, door toggle, pickup into inventory, full inventory, and container opens inventory panel. |
| `modforge` | [x] Add modforge::hud module: the one player-facing info layer (HudState, Vitals, Prompt, OpenPanel, HudBinder trait, tick, toggle_panel, move_stack); panels for inventory, health, food, and prompts | modforge builds with the module and its 5 tests pass covering binder tick, panel toggle, stack move to empty, merge matching, and swap different items. |
| `modforge` | [x] Add modforge::biome module: the biome vocabulary (BiomeDef as the generic definition of a type of area: ground, scatter, weather, monuments, npcs, wildlife, harvest; BiomeRegistry) | modforge builds with the module and its registry test passes; topside design.md records what biomes control. |
| `modforge` | [x] Add modforge::item module: the item vocabulary (ItemDef, ItemRegistry, ItemStack, ItemQuality, Inventory with slots and stacks, transfer) with quality rolled by create, the one item-creation function | modforge builds with the module and its 7 tests pass covering stacking, remove, transfer, capacity, and a quality tier rolled at creation. |

## 2026-08-19

| System | Item | Done when |
|---|---|---|
| `modforge` | [x] Add modforge::structure module: the structure vocabulary (StructureDef, RoomSpec, Opening, StairSpec, SolidSpec, LightSpec, Aabb, validate, side_frame) as engine-agnostic data on glam math and plain rgb, moved from the topside game | modforge builds with the module and its 2 tests pass; topside consumes it with no local copy of the defs (its spawn_structure stays as the Bevy binder). |

## 2026-08-18

| System | Item | Done when |
|---|---|---|
| `modforge` | [x] Add modforge::quality module (roll_tier, roll_sibling) for engine-agnostic quality tier rolls; survivalist quality.rs refactored to use it | modforge::quality has roll_tier and roll_sibling; survivalist quality.rs uses them. |
| `modforge` | [x] Add modforge::unknown module (DreadLoop, rng) for the fear-of-the-unknown dread loop; survivalist incursion refactored to use it | modforge::unknown has DreadLoop; incursion.rs uses it instead of manual statics. |
| `modforge` | [x] Add Contract trait (ContractPhase, advance_contract) to modforge::mission for the offered/owed/paying work-board lifecycle; survivalist bounty and threat refactored to implement it | modforge::mission has Contract trait; bounty.rs and threat.rs use it with should_tick + advance_contract. |

## 2026-08-17

| System | Item | Done when |
|---|---|---|
| `modforge` | [x] Add `mission_accessors!` macro to `modforge::mission` and extract 4 Unity helpers (`is_npc_alive`, `dist_sq_to_building`, `send_squad_home`, `remove_squad_and_drop`) into survivalist common.rs; all 5 mission trait impls repointed (net -129 lines) | Each mission trait impl contains only game-specific logic. |
| `modforge` | [x] Lift mission runner into `modforge::mission`: Mission trait with Stage (Going/Returning), Step (Continue/Transition/Complete), advance/advance_all batch loop, should_tick cadence helper | Engine-agnostic mission runner available; survivalist vendor/steal/trade/scavenge/murder repointed at framework trait. |
| `unityforge` | [x] Add `UnityStaticPropAdditiveEffect` and `UnityInstancePropMultiplyEffect` to `unityforge::rpg::std_effect` | Two new framework effect types for static properties and instance properties. |
| `schedule1-mod` | [x] Repoint catalog at framework effect types; local Effect impls replaced by thin `Il2CppGuardedEffect` wrapper for MAIN_QUEUE + bisection guard | schedule1 skills.rs has no local read/write logic; uses framework effect types. |
| `modforge` | [x] Enrich `skill_state` op to include `max_level` and `effect` text per skill | Framework `skill_state` now returns the same rich format schedule1 had locally. |
| `modforge` | [x] Lift storyteller/director into `modforge::storyteller`: Rule, Outcome, Config, Director with tick-driven weighted-random event pacer, splitmix64 RNG, standard `storyteller_config` op | Engine-agnostic Director available; survivalist-mod repointed at framework module. |
| `modforge` | [x] Lift genome/trait system into `modforge::genome`: Pool (keyed trait vectors with reinforcement, blending, snapshot), GenomeStore (dirty-tracked sidecar persistence with atomic save), jitter (deterministic hash) | Engine-agnostic genome pool available; survivalist-mod repointed at framework module. |
| `schedule1-mod` | [x] Remove 4 duplicate RPG ops (`skill_state`, `skill_add_xp`, `skill_levelup`, `skill_grant_points`) from `register_ops`; keep only `effects_enable` | schedule1 uses framework ops; only the game-specific `effects_enable` remains local. |
| `modforge` | [x] Add `skill_state` and `skill_add_xp` to `modforge::rpg::ops::register`; all RPG mods get 7 standard ops for free | Framework registers 7 ops; wwm-mod removed 5 duplicate custom ops (net -65 lines). |
| `modforge` | [x] Add generic research helpers to `modforge::client`: `ping_or_skip`, `handle_of`, `count_of`, `dump_sequence`, `print_declared_methods`, `fields`, `parse_vec3` | Helpers available to all mods via `modforge::client`. |
| `unityforge` | [x] Add `unityforge::client` module with `find_instances` (renamed from walk), `first_handle`, `first_handle_inactive`, `field_exists` | Unity-specific research helpers in `unityforge::client`; re-exports `modforge::client::*`. |
| `schedule1-mod` | [x] DRY tests/common: remove duplicated walk, first_handle, handle_of, count_of, dump_sequence, print_declared_methods, parse_vec3, ping_or_skip | `tests/common/mod.rs` imports from `unityforge::client`. |
| `wwm-mod` | [x] DRY tests/common: remove duplicated walk, first_handle, handle_of, print_declared_methods, fields, field_exists, ping_or_skip | `tests/common/mod.rs` imports from `unityforge::client`. |
| `workspace` | [x] Add README.md to all 8 mod/framework directories with consistent structure: Game, Features (table with ratings), Build, Deploy | All mods have a README with feature table and x/10 ratings. |
| `modforge` | [x] Move `modforge::client::research` contents into `modforge::client` directly; update 17 consumer files across 4 crates | All consumers use `modforge::client::` prefix; `research` submodule deleted. |
| `misery` | [x] Fix all tabs breaking after main menu reload | Tabs re-render after reload without crashing. |
| `misery` | [x] Document stale OWS tweaks DLL removal | `misery-mod/docs/research.md` updated. |
| `misery` | [x] Vendor system research, section 24 | `misery-mod/docs/research.md` section 24 written with offset map and sell list structure. |
| `modforge` | [x] Add `read_f64`, byte-slice decoders, `fname_from_parts`, `TArrayHeader`/`read_tarray_header` to `modforge::client::research` | All helpers available; 14 misery test files use them instead of local copies. |
| `misery` | [x] DRY all research tests: retire duplicated helpers from `common/mod.rs` and per-test locals | `common/mod.rs` has zero shared helpers; all 12 research/set tests use `modforge::client::research`. Net 316 lines removed. |
| `misery` | [x] Vendor food sell list expansion | `vendors.rs` adds all food items to Barman's sell list via TArray grow + template clone. |
| `ueforge` | [x] Add `follow_ptr_chain` and `row_name_map` to DRY misery callers | Both functions in `ueforge::ue`, misery callers collapsed. |
| `ueforge` | [x] Add `struct_fields` module | `ueforge::ue::struct_fields` provides field offset resolution. |
| `misery` | [x] DRY `gameplay.rs` using `find_object` and `read_at`/`write_at` from ueforge | Removed duplicate UE object access wrappers. |
| `misery` | [x] DRY `shining.rs` using `find_actor` and `read_at`/`write_at` from ueforge | Removed duplicate actor lookup and byte access. |
| `ueforge` | [x] DRY `iter_stride`, `handle_request` across misery/outworld/grounded2 | Shared iteration and request handling lifted to ueforge. |
| `ueforge` | [x] Move 4 engine-generic debug ops from misery into `register_builtins` | Ops available to all mods without per-mod registration. |
| `ueforge` | [x] Add `FieldAccessor`, replace misery gameplay wrappers | Generic field read/write accessor replaces per-mod boilerplate. |
| `ueforge` | [x] Add `start_debug_server`, collapse misery and OWS debug boilerplate | One-call debug server setup replaces duplicated init code. |
| `misery` | [x] Merge UI files into their feature modules | Separate UI files folded into `shining.rs`, `speed.rs`, `gameplay.rs`. |
| `ueforge` | [x] Add `resolve_and_init`, collapse misery `on_unreal_init` boilerplate | Single call replaces manual patternsleuth + runtime init sequence. |
| `ueforge` | [x] Add `input::send_key`, replace misery `synthesize_space` | Framework keystroke helper replaces game-specific wrapper. |
| `misery` | [x] Remove scanner, classes, structs tabs | Unused debug tabs deleted from mod metadata. |
| `ueforge` | [x] Remove hardcoded address fallbacks from `PlatformOffsets` | `g_world` and `process_event` fields deleted; patternsleuth is the only source. `resolve_and_init` takes two structural params instead of `&PlatformOffsets`. |
| `ueforge` | [x] Add features builder for mod init | `ueforge::features()` with `.once()`, `.on_each_load()`, `.on_first_table()`, `.install()`. |
| `misery` | [x] Convert to features builder | `on_unreal_init` uses `ueforge::features()` instead of manual thread spawns. |
| `ows` | [x] Convert to features builder | `on_unreal_init` uses `ueforge::features()` for stacks and dynamic tweaks. |
| `wwm` | [x] Research docs update, build script auto-detect, test suite | 6 test files added, `build_and_deploy.ps1` auto-detects `*_Data` folder, `wwm-mod/docs/research.md` updated. |
| `ueforge` | [x] Add `begin_child`/`end_child` ImGui wrappers | C++ and Rust bindings for scrollable child regions. |
| `workspace` | [x] Add misery-mod to workspace members | `Cargo.toml` members list updated. |
| `workspace` | [x] Move mod docs into their mod folders | 6 files moved: misery research/rpg/itemlist, wwm research, schedule1 plan/todo. All references updated across 26 files. |
| `schedule1` | [x] Certainty tracking: NPCResponses_Civilian correction and fix path | Retraction reversed: NPCResponses_Civilian IS the difference on custom goons. Base NPCResponses + Aggression 1.0 hypothesized as fix path. |
| `schedule1` | [x] Restart script: wait for save load after control plane ready | `restart.ps1` waits for save load after control plane answers, not just ping. |
| `schedule1` | [x] Research test suite: 8 behaviour/combat/patrol tests | `research_behaviours.rs`, `research_behaviours_deep.rs`, `research_behaviours_hold.rs`, `research_behaviours_phase1.rs`, `research_combat_config.rs`, `research_patrol.rs`, `research_priority.rs`, `research_retaliation_isolation.rs` added. |
| `schedule1` | [x] New crate schedule1-mod added to workspace (cdylib, unityforge + modforge, port 17175) | Built and deployed to Mods/, replaces smoke dll. |
| `schedule1` | [x] Research docs + certainty-tracking started, harmony_probe op added to unityforge | research.md + certainty-tracking.md created; harmony_probe op proves per-target patchability. |
| `schedule1` | [x] Map regions research: ScheduleOne.Map.Map singleton, 6 regions, CartelInfluence | research.md documents region structure, EMapRegion 0-5, influence per region. |
| `schedule1` | [x] NPC research: GoonPool.SpawnGoon + CartelGoon.AttackEntity proven in-game | Spawn and combat paths documented in research.md. |
| `schedule1` | [x] Combat research: NotifyAttackedByPlayer fires per hit, KnockOut (not Die) at 0 health | Kill/knockout hooks fire clean; XP credits both paths. |
| `schedule1` | [x] Harmony prefixes on Die/KnockOut/NotifyAttackedByPlayer/TakeDamage all fire simultaneously | All four hooks installed and firing clean in combat. |
| `schedule1` | [x] Kill-observation hook point proven for combat XP | Die + KnockOut prefixes fire during real fights (combat_trace). |
| `schedule1` | [x] Loot path: cash template clone + FishNet spawn, operator picked up cash in-game | Recipe proven and documented in research.md 4b. |
| `schedule1` | [x] XP on player kills: killcredit.rs with per-NPC attribution and dedupe | Kill credit tracks player hits, awards XP on Die/KnockOut. |
| `schedule1` | [x] Heavy Hands skill (punch damage, instance props, exact math proven) | SkillDef registered, auto-spend applied, punches confirmed stronger. |
| `schedule1` | [x] Vitality + regeneration ON ICE (static field setters crash 0.4.6f12) | Skills defined but disabled until interop generator fix. |
| `schedule1` | [x] Persistence per save slot (LoadManager save folder name) | RPG state survives 4+ relaunches. Endless curve + auto-spend. |
| `schedule1` | [x] Combat RPG exit gate passed | Operator kill logged XP + level up, auto-spend applied, persistence proven. |
| `schedule1` | [x] Loot table v1: cash drop on NPC kill, amount scaled by mob toughness | Loot drops confirmed in-game. |
| `schedule1` | [x] Loot drop regression on custom NPCs: NOT REPRODUCING | Player killed Tough Armed goon, loot dropped, XP awarded. Earlier regression was transient. |
| `schedule1` | [x] S1ApiNpcs.cs: GoonNpc/PoliceNpc/PlayerNpc + NpcFactory statics in MelonLoader shim | Five custom NPCs spawned and visible in-game. 5-goon supply cap eliminated. |
| `schedule1` | [x] Custom NPC combat proven: two armed goons fought each other with no player involvement | Knife + baton combat via SetAndAttackTarget, index-tracked mints. |
| `schedule1` | [x] Custom NPC despawn: Object.Destroy works, ServerManager.Despawn NREs | Object.Destroy is the working cleanup path. |
| `schedule1` | [x] Custom NPCs do NOT survive save/reload (all S1NPC refs become unresolved) | Mod must respawn from persisted state on every load. |
| `schedule1` | [x] Custom NPC death cleanup: KillNpc transitions to DeadBehaviour, Die hook fires | DespawnNpc after death is the safe cleanup path. |
| `schedule1` | [x] EnableIdleBehaviour makes idle the active behaviour | IdleBehaviour with no IdlePoint holds position (0.00m drift over 30s). |
| `schedule1` | [x] IdleBehaviour WITH IdlePoint causes wandering (36m away), not holding | Do NOT use SetIdlePoint for garrison posts. |
| `schedule1` | [x] Freshly spawned goons have 18 behaviours, all disabled, none active | This is why they drift after spawn. |
| `schedule1` | [x] Custom goons on idle do NOT auto-fight when attacked (CombatBehaviour never activates) | Guard goes Idle to Dead without Combat. Retaliation needs a separate trigger. |
| `schedule1` | [x] EnableCombatBehaviour does not make custom goons fight back | CombatBehaviour enabled but goon goes FaceTarget then CallPolice then Unconscious when punched. |
| `schedule1` | [x] Vanilla goon behaviour stack compared: 17 behaviours all disabled, same as custom | Behaviour stack is not the difference; something else triggers vanilla retaliation. |
| `schedule1` | [x] Read influence per region via CartelInfluence singleton | GetInfluence returns 0-1 float per region. |
| `schedule1` | [x] ChangeInfluence works via 2-param server RPC (tested 4 deltas in a row) | RpcLogic___ChangeInfluence_2792544924 moves influence reliably. |
| `schedule1` | [x] Region ownership research: vanilla has NO faction ownership system | RegionInfluenceData has only Region + Influence. Mod must track ownership itself. |
| `schedule1` | [x] SetToughness (MaxHealth write + Heal) on custom NPCs | Instance property writes proven safe. |
| `schedule1` | [x] Movement speed writes via S1API NPCMovement.SpeedMultiplier | Read/write proven (default 1.0, set to 2.0 reads back 2.0). |
| `schedule1` | [x] Damage output via CombatBehaviour.VirtualPunchWeapon Damage property | Path identical to SetToughness and SetSpeedMultiplier (both work). |
| `schedule1` | [x] MelonLoader shim: HarmonyBridge verified, Il2CppInterop surface confirmed, shim built | MelonLoader 0.7.2 ships HarmonyX 2.10.2 + Il2CppInterop.Runtime 1.5.1; shim compiles clean. |
| `schedule1` | [x] Generation-loader hot reload mirrored into MelonLoader entry | MelonLoader entry drives GenerationLoader same as Mono entry. |
| `schedule1` | [x] Smoke passed on Schedule 1: ping, smoke_state, walk_class, read/write, postfix, hot reload | All smoke checks passed via il2cpp-smoke tests against live game. |
| `schedule1` | [x] Il2CppInterop 0.4.6 crash fix: 4 sites patched (null type refs in Pass11, RewriteTypeRef, JudgeSpecifics, Pass16ScanMethodRefs) | Patched generator emits all 148 interop assemblies; MelonLoader loads all 9 mods on 0.4.6. |
| `schedule1` | [x] Research proven live: region owner, mob classes, death path, kill hook, loot path | All five proven by 2026-08-08 in research.md + certainty-tracking.md. |
| `schedule1` | [x] Kill NPC, gain XP, level combat stat, stat visibly changes, survives save/reload | Operator knocked out NPC, +25 XP, LEVEL UP -> 9, auto-spend raised Heavy Hands, survived 4+ relaunches. |
| `schedule1` | [x] Kill NPC, loot drops (cash), pick it up in-game | Kill dropped rolled cash stack at body (toughness-scaled), operator picked it up. |
| `schedule1` | [x] Spawn custom goon at exact position via S1API | NpcFactory.SpawnGoon, five NPCs spawned and visible. |
| `schedule1` | [x] Spawn custom police and player NPCs | NpcFactory.SpawnPolice + SpawnPlayerNpc proven. |
| `misery` | [x] Add build.rs for ueforge cdylib shim | `misery-mod/build.rs` compiles the UE4SS C++ shim. |
| `misery` | [x] RPG design doc | `misery-mod/docs/rpg.md`: skill tree design for MISERY (stats, XP curve, effect mappings). |
| `misery` | [x] Restart script | `misery-mod/scripts/restart.ps1`: kill game, build, deploy, launch, wait for control plane. |
| `misery` | [x] Test common module | `misery-mod/tests/common/mod.rs`: shared API setup and skip helpers. |
| `misery` | [x] Research test suite: 9 tests | `freeze_timer.rs`, `research_difficulty.rs`, `research_gameplay_scan.rs`, `research_gameplay_settings.rs`, `research_item_stacks.rs`, `research_movement.rs`, `research_movement_map.rs`, `set_countdown.rs`, `set_movement_map.rs`, `set_movement_speed.rs` added. |
| `workspace` | [x] Update README for all new mods and current state | README reflects schedule1, misery, survivalist, scrapmechanic, quasimorph. Diagram, table, layout, status, docs, credits all updated. |
| `misery` | [x] 10x item stack multiplier via FieldTweak on ItemList (175 rows) | All item stacks multiplied 10x on load. |
| `misery` | [x] UE4SS external console enabled | Console available for debugging. |
| `misery` | [x] Shining tab (emission timer control) | ImGui tab controls shining emission timer. |
| `misery` | [x] Speed tab (1x/2x/3x movement speed buttons) | ImGui tab sets movement speed multiplier. |
| `misery` | [x] Gameplay tab (hunger/thirst/stamina/damage/enemy knobs) | ImGui tab exposes gameplay setting knobs. |
| `misery` | [x] Fix speed tab and 2x default after main menu reload | Class name string matching replaces stale UClass pointer comparison. |
| `unityforge` | [x] S1ApiNpcs: fix IL2CPP type checks, add SetAggression and InspectCombatConfig | `GetIl2CppType()` replaces `GetType()` for correct IL2CPP runtime type resolution. `SetAggression` writes aggression via IL2CPP field reflection. `InspectCombatConfig` dumps response types and combat config for research. |
| `unityforge` | [x] Add Assembly-CSharp and FishNet.Runtime references to csproj | Enables direct use of Schedule 1 game types (behaviours, NPCs, patrol routes) and FishNet (NetworkBehaviour base). |
| `scrapmechanic` | [x] Better Survival mod: initial commit | Lua mod with 1000 inventory slots, half fuel consumption, no inventory loss on death, no building restrictions. 40 files (scripts, configs, game databases). |
| `quasimorph` | [x] Initial scaffold: csproj, build script, stub ModMain, research doc | Unity Mono mod using first-party mod API (no BepInEx/MelonLoader). Research doc covers hook types, mod structure, console commands. |

## 2026-07-04

| System | Item | Done when |
|---|---|---|
| `unityforge` | [x] SV1: extract generation loader out of BepInEx plugin into cs-shim-common/GenerationLoader.cs (Generation class, LoadGeneration, CheckHotReload/HotSwap, LocateRustDll, NativeLibrary P/Invoke, ShutdownForUnload/ReinitAfterUnload) | GenerationLoader shared between BepInEx and survivalist hosts. |
| `unityforge` | [x] SV2: logger sink seam via ShimLogger.Sink delegate; BepInEx hosts wire ManualLogSource, survivalist host wires UnityEngine.Debug.Log with [Unityforge] prefix | Logger sink abstracted for all hosts. |
| `survivalist-mod` | [x] SV3: new host project cs-shim-survivalist/ (global-namespace Main, Load()/Unload(), re-entrant guard, driver GameObject, DontDestroyOnLoad, GenerationLoader, net472 target) | Survivalist shim builds green. |
| `unityforge` | [x] SV4: Harmony bridge actually works + Harmony 2.0.4 compat (static PrefixDispatcher + PostfixDispatcher, delegate list per patched method, UnpatchAll per-dispatcher) | Both shims build green with working Harmony. |
| `survivalist-mod` | [x] SV5: survivalist-mod Rust crate (cdylib on unityforge + modforge, http_port 17173, ops::register_builtins + selector::register_builtins) | Crate builds green. |
| `survivalist-mod` | [x] SV6: deploy script (shim to DLLs/, cdylib to mod root as survivalist_mod.unityforge.dll, -Hot generation staging) | Build and deploy pipeline works. |
| `survivalist-mod` | [x] SV7: live smoke verified (shim loads, ping answers on 17173, walk_class + inspect_object work, hot reload gen 0->1, Harmony patch fires live for faction-war revenge trigger) | Full live smoke passed. |
| `survivalist-mod` | [x] SV8: gameplay research doc (research.md with live-access recipe, difficulty knobs, class maps; faction-war.md with mechanics and phase 1 live-verified; status.md tracker) | Research and design docs written. |

## 2026-05-16

| System | Item | Done when |
|---|---|---|
| `modforge` | [x] testkit::fn_entry: is_msvc_x64_prologue (with unit tests), find_fn_bounds_via_int3, verify_fn_entry config+runner | Deduped across dump_apply_gene + find_retire_horse_handler. |
| `modforge` | [x] testkit::msvc: MsvcStdString parser (with unit tests) + is_vtable_at_image_rva plausibility check | MSVC helpers available in testkit. |
| `modforge` | [x] testkit::op: generic op invoker + response contract + dot-path field assertion | Op invocation and assertion available in testkit. |
| `modforge` | [x] testkit::build_info + testkit::xrefs + testkit::assets | Build info, xref, and asset helpers available. |
| `modforge` | [x] testkit::watch: region + single-value watchers, env-parseable, three-mode (assertion / discovery-trigger / manual) | Watch primitives available in testkit. |
| `modforge` | [x] testkit::snapshot: take + diff_against for bytes-at-addr snapshots | Snapshot helpers available in testkit. |
| `modforge` | [x] testkit::recipes: find_fn_by_rdata_string + find_struct_by_field_value | Recipe helpers available in testkit. |
| `modforge` | [x] B1: Extend sleuth with TargetDef, TargetRegistry, Resolver, ResolvedTarget, 4 Recipe variants, 6 built-in validators (8 new unit tests) | Registry types available in modforge::patterns::sleuth. |
| `horsey-mod` | [x] B2: targets_registry.rs declares 41 targets (7 data globals + 4 invocable + 30 hint-only); parity integration test | Registry matches legacy resolver byte-for-byte. |
| `modforge` | [x] B3: shared R-tier tests in modforge (4 shared assertion functions parameterized over RunningGame + TargetRegistry + Resolver) | Shared R-tier tests available to all consumers. |
| `horsey-mod` | [x] B4a: 37 of 42 entries have real candidate sigs (88%) via prologue_fn! macro | 37 targets have real sigs. |
| `horsey-mod` | [x] B4b: all 24 call sites migrated to targets_registry::resolve wrapper | All call sites use registry. |
| `horsey-mod` | [x] B5: delete legacy targets::resolve::* (2507 LOC to 1286 LOC, -1221 LOC) | Legacy resolvers deleted. |
| `grounded2-mod` | [x] B6: cross-game adoption proof (GROUNDED2_TARGETS with 5 UE5-Augusta globals; layout.rs E0432 blocker cleared) | Second consumer uses TargetRegistry. |
| `modforge` | [x] PR-1 through PR-10: input prior-art research (game-mod input libs, UI automation, game-bot ecosystems, per-engine input pokes, raw-input, accessibility, crate audit, injection precedents, replay formats, anti-cheat notes) | input-prior-art.md written with recommendation. |
| `modforge` | [x] I-1: modforge::input primitives (L1 SendInput over windows-sys, L2 PostMessage, Backend enum, Button, Key::parse, InputSurface trait, SYNTHETIC_EXTRA_INFO tag) | Input primitives available, zero new deps. |
| `modforge` | [x] I-2: 7 HTTP input cmdlets (input.mouse.move/click, input.key.down/up/press, input.cursor.get, input.foreground.hwnd) | Input ops available via HTTP. |
| `horsey-mod` | [x] I-5: test harness first slice (L1 cursor round-trip within 1px, L1 keyboard F24 press, L2 PostMessage; sleuth resolver bug fixed) | Input smoke tests pass against live Horsey. |
| `modforge` | [x] I-2a: input.find_hwnd_by_pid + input.self.hwnd ops (EnumWindows-by-PID with visibility filter) | HWND lookup ops available. |
| `modforge` | [x] I-2c: drag/scroll/combo cmdlets (input.mouse.drag L1+L2 interpolated, input.mouse.scroll L1+L2, input.combo modifier hold + dispatch) | Drag, scroll, and combo ops pass live smoke. |
| `horsey-mod` | [x] I-4 L3 routing + HorseyInputSurface v1 (writes LOC+0x174/+0x178 cursor floats directly; buttons + keys delegate to L1; graceful degrade without save) | L3 cursor move tracks target within 1px. |
| `horsey-mod` | [x] I-2d-recon: input_hk1_calibration.rs captures OS-to-game-coord mapping (5-point plus pattern) | Calibration test runs clean (needs in-save run for actual transform). |
| `modforge` | [x] V1: modforge::vanilla primitives (Signature, ArgKind, RetKind, ArgValue, RetValue, Win64 ABI dispatcher; 13 unit tests) | Vanilla invocation primitives available. |
| `modforge` | [x] V2: sleuth TargetDef extended with optional Signature (TargetKind::FunctionEntry) | Signature field on TargetDef. |
| `modforge` | [x] V3: Invoker controller + vanilla.invoke / vanilla.list HTTP cmdlets (SEH-wrapped by default; 5 unit tests) | Vanilla invoke available via HTTP. |
| `horsey-mod` | [x] V4 data: signatures attached to 4 horsey functions (APPLY_GENE_TO_HORSE, HORSE_REBUILD, RNG_NEXT_MODULO, HORSE_COPY_GENE_LANE_PAIRS) | 4 vanilla functions registered with signatures. |

## 2026-05-14 (relocated from todo.md: unityforge / wwm-mod / naming completions)

Material below was tracked as `[x]` checkboxes in [`todo.md`](todo.md) until
2026-05-15. Relocated here as the authoritative record. No new work; only the
provenance changed.

- **Demo-end block SOLVED** (attempt 7). Harmony prefix returning
  false on `TutorialManager.CompleteDemo` +
  `CompleteDemoCoroutine`. Implemented in
  `unityforge/cs-shim-mono/Plugin.cs` `InstallDemoCompleteBlock`.
  Full incident write-up in
  [`wwm-mod/docs/research.md` §7.7](wwm-mod/docs/research.md#77-demo-end-block-2026-05-14-solved-on-attempt-7).
  Methodology lesson: `list_methods` against the most-obvious
  manager class as the FIRST move, not the last.
- **Plot Enlargement / Land Surveyor "buy" feature investigated,
  NOT PURSUING**. Bank's Land Surveyor board
  (`Bank/.../WoodenBoard/PlotOffer{,(1),(2),(3)}`) is a demo-cut
  feature. Visual assets shipped (4 PlotOffer GameObjects with
  labels + prices), zero managed code references across any DLL
  in `Managed/`, not wired into the game's `BuyPopup` /
  `BuySection` system. Implementation would be content
  development, not modding. Evidence in
  [`wwm-mod/docs/research.md` §7.8](wwm-mod/docs/research.md#78-plot-enlargement--land-surveyor-search-2026-05-14-partial).
- **Naming standardization landed**: per-game mod crates now use
  the `<game>-mod` form. `grounded2-rpg` -> `grounded2-mod`,
  `wwm-rpg` -> `wwm-mod`, `outworld-station-tweaks` ->
  `outworld-station-mod`, `horseyforge` -> `horsey-mod`. One
  commit per crate so it bisects clean. `il2cpp-smoke` kept its
  name (smoke target, not a per-game mod). What the rename
  touched per crate: directory rename, workspace `Cargo.toml`
  member list, each crate's `[package].name` + `[lib].name`,
  `[package.metadata.ueforge]` (deploy paths, log filenames,
  `mods.txt` entry, deployed log filename change e.g.
  `grounded2_rpg.log` -> `grounded2_mod.log`), every
  `use grounded2_rpg::` / `use wwm_rpg::` import, workspace
  docs sweep, each crate's own `docs/` + `README.md`,
  `.claude/project_state.md`, skill files
  (`~/.claude/skills/grounded2/SKILL.md` etc.), PowerShell
  scripts (`build_and_deploy.ps1`), horsey-mod deployed
  artifacts (`horseyforge.dll` -> `horsey.dll` and the matching
  `inject.exe` / `.log` / `.injstate`).

- **Unityforge Phase 4 generation-versioned hot reload**.
  Deep-dive in
  [`unityforge-plan.md` §6.5](unityforge-plan.md#65-hot-reload-phase-4).
  Replaced naive-FreeLibrary cycle (which crashed WWM
  2026-05-13). Cdylib `unityforge_shutdown` joins background
  threads (`modforge::server::shutdown_all` calls
  `Server::unblock` + thread join; `modforge::rpg::poller::
  shutdown_all` uses a `Condvar` to wake the sleeping poller
  immediately and joins; both registered in
  `modforge::shutdown::SHUTDOWN_REGISTRY` at order 200/250).
  Shim rewrote `UnityforgeShimPlugin` (cs-shim-mono/Plugin.cs)
  around a `Generation` class (module handle, init/tick/shutdown
  delegates, pinned bridge table) with one `_active` + a
  `_quiesced` list and per-gen tick dispatch. Watcher routes
  per-second scan for `*.unityforge.gen<N>.dll`; highest
  N > active triggers `HotSwap`. `NativeLibrary.Free` removed
  from the helper entirely. HTTP port + Harmony lifecycle on
  swap: old generation's `unityforge_shutdown` joins HTTP
  listener (port released) + unpatches via
  `HOOK_REGISTRY.shutdown_all` before new gen's `init` fires.
  Shared C# `Harmony` instance + per-gen patch sets in
  `_patches` dict; cross-gen patches don't collide because old
  gen removes its own first. `build_and_deploy.ps1 -Hot` scans
  for highest existing `gen<N>.dll` and writes the build as N+1.

- **Modforge extraction Phase 0a + 0b rows 1-16**. ~12k Rust
  lines lifted out of ueforge into the engine-agnostic
  `modforge` crate. Deep-dive in
  [`unityforge-plan.md` §6 Phase 0](unityforge-plan.md#phase-0-modforge-extraction-4-5-days).
  Phase 0a shipped `modforge/spec/*` and `modforge/docs/*`
  (op-envelope, selector-grammar, op-registry,
  generic-primitives, skill-catalog, rpg-persistence, xp-curve,
  effect-kinds, trigger-kinds; methodology, composition-model,
  def-registry, naming). Phase 0b rows 1-15 migrated
  engine-agnostic infrastructure (ring, counters, log, args,
  envelope, settings, shutdown, selector grammar, ops registry,
  server, scanner, winproc, hot_reload, RPG traits, RPG pure
  math, tracker, poller, slot store, vanilla, disabled) + RPG
  traits + RPG pure math + tracker. Row 16: generic
  `std_effect` lifted to `modforge::rpg::std_effect` with
  blanket `impl<E: Engine> Effect<E>`.

- **Modforge extraction Phase 0b rows 17-22** (framework-wide
  subsystem migration). Row 17: `client` -> `modforge/client/`.
  ueforge's HTTP client + scenario DSL + diff/perf/research
  submodules moved wholesale; `crate::parms` coupling inlined
  as direct zerocopy calls in `call_ufunction_typed`.
  Row 18: `bin/ueforge-deploy` -> `modforge-deploy` bin in
  modforge. `.cargo/config.toml` alias repointed.
  Row 19: `debug` glue -> `modforge/src/debug.rs` (pure half:
  `PlayerStateView`, `CatalogEntry`, `catalog_view` generic
  over `E`). UE-specific helpers (ProcessSnapshot, DamageRing,
  enqueue_pe) stay in `ueforge::debug`.
  Row 20: snapshots generics -> `modforge/src/snapshots/`
  (stub declared; concrete ProjectionSnapshot types follow the
  first consumer).
  Row 21: `ui` declarative shape -> `modforge/src/ui.rs`
  (`TabDef { name, render }` moved; ueforge + unityforge both
  re-export; rendering stays per-framework).
  Row 22: `worker` shape -> `modforge/src/worker.rs`
  (`spawn(name, work)` with panic-guard moved wholesale).

- **Unityforge Phase 1: IL2CPP support + Rust SDK unification**.
  Deep-dive in
  [`unityforge-plan.md` §6 Phase 1](unityforge-plan.md#phase-1-unityforge-skeleton--both-c-shims--http-control-plane-10-12-days).
  1a Cargo crate + workspace wiring. 1b C# Mono shim at
  `unityforge/cs-shim-mono/Unityforge.Shim.Mono.csproj`
  (BepInEx 5). 1b shim split: shared `Bridge.cs`,
  `HarmonyBridge.cs`, `Logger.cs` live in `cs-shim-common/` and
  are linked from both backend csprojs. 1b-il2cpp C# IL2CPP
  shim at
  `unityforge/cs-shim-il2cpp/Unityforge.Shim.Il2Cpp.csproj`
  (BepInEx 6 IL2CPP + Il2CppInterop). Same bridge ABI as Mono;
  `Il2CppBridge.cs` implements the surface against
  `Il2CppInterop.Runtime`. 1c Rust `unity::*` SDK:
  backend-agnostic surface at `unityforge::unity::*` (`Type`,
  `Object`, `runtime_kind`); bridge ABI v2 carries
  `RuntimeKind` tag; `mono::*` and `il2cpp::*` are
  backend-specific escape hatches. 1d HTTP control plane.
  1e Hook bridge.

- **Unityforge Phase 2: rpg framework completeness**. Deep-dive
  in [`unityforge-plan.md` §6 Phase 2](unityforge-plan.md#phase-2-unityforgerpg-3-4-days).
  Shipped `slot_key_unity` + vanilla cache + std_effect
  (3 effects) + tracker/skill type aliases. `trigger_harmony` +
  `OnUnityEvent`: `ON_HARMONY_POST`, `ON_HARMONY_PRE`,
  `ON_UNITY_EVENT` TriggerDefs in
  `unityforge::rpg::trigger_harmony` plus `fire_post` /
  `fire_pre` / `fire_event` helpers (game-side trampolines push
  events through `Tracker::fire`). `ops_register` for RPG ops:
  lifted `ueforge::rpg::ops` to `modforge::rpg::ops` generic
  over `E: Engine`; both frameworks call
  `modforge::rpg::ops::register(&TRACKER)` (or the
  `unityforge::rpg::ops::register` /
  `ueforge::rpg::ops::register` re-exports) to add the standard
  five-op set.

- **wwm-mod Phase 3a (Mono proof) milestones**. Deep-dive in
  [`unityforge-plan.md` §6 Phase 3a](unityforge-plan.md#3a-wwm-mod-mono).
  Strong Back + Greedy Miner declarative via
  `UnityFieldAdditiveEffect` and `UnityFieldMultiplyEffect`.
  Slot poller + UnitySlotKey wiring polls
  `GameSerializationSystem._currentLoadedSaveNumber`.
  Save/load via `modforge::rpg::store`: JSON under
  `<DLL_dir>/wwm-mod/<slot>.json` written atomically by the
  Tracker. **Quick Pickaxe VERIFIED IN-GAME 2026-05-13**:
  spending 10 points mutated `DigManager._digRange` from 3.0
  to 4.5 exactly; save persisted to `<WWM>/wwm-mod/0.json`.
  Lucky shipped as a format-only RuntimeEffect (hot-path
  probability scaling is a future Harmony postfix callback
  that reads the level on every fire). Declarative UI:
  `ModDef.tabs` field added on unityforge; `register_ui_ops`
  exposes `list_tabs` + `render_tab` via HTTP;
  `wwm-mod::skills::render_tab` logs a catalog + state snapshot
  through the BepInEx sink. Full in-process ImGui rendering is
  deferred until a bundled imgui binding lands on unityforge
  (plan carve-out: "OnGUI fallback is always available").

- **Unityforge Phase 3b: IL2CPP smoke proof-point**. Deep-dive
  in [`unityforge-plan.md` §6 Phase 3b](unityforge-plan.md#3b-il2cpp-proof-point).
  Default Harmony target is
  `UnityEngine.Time::get_realtimeSinceStartup` (every Unity
  game ships it) so the smoke crate loads against any IL2CPP
  game; overridable via `IL2CPP_SMOKE_TARGET_CLASS` /
  `IL2CPP_SMOKE_TARGET_METHOD` env vars. Smoke cdylib at
  `grounded2mods/il2cpp-smoke/` with crate-type `cdylib` and
  the three `unityforge_*` exports verified via dumpbin. Ops
  `smoke_state` (runtime tag + postfix counter), `smoke_read`
  (read_field through the bridge), `smoke_write` (write_field
  through the bridge); one Harmony postfix wired through
  `patch_postfix`. Curl verification still requires a running
  game (out of scope).

## 2026-05-14 (horseyforge: split-flag fatigue + binary-patch infra wip)

- **Split-flag fatigue suppressor** ([`608f994`](.)).
  Replaces use of the game's built-in `no_tire` cheat
  (`DAT_1403d95c5`), which zeroes BOTH `+0x205`
  (race-eligibility) AND `+0x206` (sleep-prompt counter)
  on every horse every frame. The `+0x206` write breaks
  the sleep-gate check at `FUN_1400e0aa0:131551`: the
  game decides "no horses are tired" and refuses to let
  the player sleep, blocking day advancement.
  Fix: spawn our own 50ms worker
  (`horseyforge/src/fatigue.rs`) that zeros only `+0x205`,
  leaving `+0x206` untouched. Race gate always passes;
  sleep gate works normally. Toggled via
  `fatigue.suppressor.{get,set}` ops. `cheats.no_tire.set`
  is kept but its docstring now warns about the sleep
  break.
- **Binary-patch infrastructure** ([`a31246f`](.)). Wip.
  New `horseyforge::patches` module with `patch_bytes(name,
  addr, new_bytes)` (VirtualProtect + write +
  FlushInstructionCache + originals saved) and
  `revert_all` (called on DLL detach so future hot-reload
  generations don't re-patch already-NOP'd bytes). New
  op `patches.list`. **First use case
  `sleep_safe_no_tire` is NOT YET WORKING**: pattern scan
  for the `+0x206` zero-store inside the no_tire loop at
  `FUN_1400ceb60` finds zero or multiple sites depending
  on the disambiguator. Three iterations tried; current
  proximity heuristic (find a `+0x206` store with a
  `+0x205` sibling within 64 bytes) still failing on the
  user's build. Next idea: walk back from the
  `DAT_1403d95c5` read and pick the FIRST `+0x206` store
  after it. See [`todo.md`](todo.md) "horseyforge:
  sleep_safe_no_tire patch site discovery".

## 2026-05-13 (horseyforge: native-PE binding + hot reload)

- **Native-PE binding of `modforge` for Horsey Game**
  ([`e9d3345`](.)). New crate `horseyforge` (sibling to
  `ueforge` and `unityforge`). Where those rely on a
  managed-runtime plugin loader, horseyforge attaches via
  an injector EXE that `CreateRemoteThread`s a
  `LoadLibraryW(horseyforge.dll)` into the running
  `Horsey.exe`. HTTP control plane on `127.0.0.1:33077`
  with auth (powered by `modforge::server`), exposing
  `game.read`, `game.money.{get,set,add}`,
  `game.year.{get,set}`, `cheats.{no_tire,debug_mode}.
  {get,set}`, `horses.count`, `horses.roster_addr`,
  `horse.{read,set_age,set_max_age,clear_tiredness}`.
  Initially planned to proxy `steam_api64.dll` (1,089
  forwarders); MSVC link.exe's `.DEF` forwarder support
  was too brittle. Injector pattern: simpler, hot-reload
  friendly, game-agnostic.
- **Hot reload via staged DLLs + `_shutdown` op**
  ([`91f79f5`](.)). Solves the file-lock problem: cargo
  cannot rebuild `horseyforge.dll` while Horsey has it
  loaded. `inject.exe` always COPIES `horseyforge.dll` to
  a timestamped staged path
  (`horseyforge-<ts>.dll`) before `LoadLibraryW`; cargo's
  output is never loaded directly so it's never locked.
  State persisted to `horseyforge.injstate` (HMODULE +
  staged path). `--reload` POSTs `_shutdown` to release
  the listener, `CreateRemoteThread`s `FreeLibrary` on
  the old HMODULE, deletes the old staged file
  (best-effort retry), stages the new build, loads it,
  updates state. `--fresh` ignores existing state.
- **`no_tire` enabled by default at DLL attach**
  ([`c37fa54`](.)). `worker_main` flips
  `DAT_1403d95c5` to 1 right after registering ops.
  `NO_TIRE_TOGGLE` lives in `.data` (static address, not
  behind a pointer), so the write is valid before any
  save is loaded. First successful test of the hot-reload
  pipeline: HMODULE swapped from `0x7ffd999b0000` to
  `0x7ffdbf190000`, game state preserved, `no_tire:true`
  visible immediately. Later superseded by the
  split-flag suppressor (above) once the sleep-gate
  break was diagnosed.

## 2026-05-12 (proptest scanner round-trips)

- 8 new property tests on `scanner::Val::from_json` covering
  every numeric `Ty` (u8/i8/u16/i16/u32/i32/u64/i64/f32/f64).
  Each runs across the full domain and asserts the bytes
  match `value.to_le_bytes()`. f32/f64 use bits comparison so
  NaN payloads survive the round trip. Plus 4 unit tests on
  out-of-range rejection + `parse_addr` hex prefix handling.
  scanner.rs previously had zero tests.

## 2026-05-12 (insta adoption seed)

- **`insta` 1** added as a workspace + ueforge dev-dep with
  the `json` feature. Four envelope-shape snapshots seeded
  in `envelope.rs` covering `OpResponse::ok`, `err`, and both
  arms of `from_result`. Snapshots live at
  `ueforge/src/snapshots/*.snap`. Wire-shape changes now
  surface as reviewable diffs (`cargo insta review`) instead
  of silent breakage downstream.
- Per-op snapshots for the standard debug-op set still
  pending. See [`todo.md`](todo.md).

## 2026-05-12 (proptest adoption first wave)

- **`proptest` 1** added as a workspace + ueforge dev-dep.
  Property tests seeded on both walkers:
  - `TArray::is_empty_holds_for_garbage_headers` runs
    `(num, max, data_addr)` triples across the full
    i32/usize space and asserts the walker never claims
    non-empty on a null pointer or negative `num`.
  - `TMap::find_value_matches_or_misses` builds synthetic
    TMaps from up to 16 random `(u64, u64)` entries and
    asserts `find_value_by_fname_key` returns exactly the
    matching value (or None for absent keys). De-dupes
    inputs so duplicate-key non-determinism isn't a false
    positive.
- FieldTweak decoder + `Val::from_json` + `inspect_address`
  byte slabs still open. See [`todo.md`](todo.md).

## 2026-05-12 (fastrand + smallvec quick wins)

Two of the five P1 crate adoptions from
[`todo.md`](todo.md) "Pending crate adoptions":

- **`fastrand` 2** replaces the hand-rolled xorshift PRNG in
  `ueforge::hook::install::jitter`. 18 lines down to a one-line
  `fastrand::i64(-250..=250)` call. Same behavior: PRNG quality
  is non-cryptographic by design; jitter just needs to be
  unsynchronized across concurrent install workers.
- **`smallvec` 1** replaces the `[(Option<&SkillDef>, u32);
  32]` fixed stack array in `ueforge::rpg::Tracker::fire` with
  `SmallVec<[(&'static SkillDef, u32); 32]>`. Same zero-alloc
  happy path. The previous shape silently dropped subscribers
  past 32; SmallVec spills to heap so a future catalog
  expansion can't bite us.

Both adoptions ship workspace-wide deps + per-crate wiring. All
hook + tracker unit tests pass; full workspace `cargo check`
clean.

## 2026-05-12 (zerocopy first wave)

Wave one of zerocopy adoption shipped after the crate-shopping
pass. Compile-time POD layout verification replaces hand-rolled
byte-cast unsafe at the parm-buffer surface.

Framework:
- `ue::core_types::FGuid` + `FWeakObjectPtr` derive
  `FromBytes` + `IntoBytes` + `KnownLayout` + `Immutable`. The
  derive verifies at compile time that no field breaks POD (no
  padding-with-pointers, no `Drop`, alignment known).
- `ueforge::parms::as_bytes` / `from_bytes` rewritten as SAFE
  fns gated on `T: IntoBytes + Immutable` /
  `T: FromBytes + KnownLayout`. The previous `unsafe fn`
  signature is gone; safety contract moves into the trait
  bounds the consumer's derive proves.
- `Api::call_ufunction_typed` loses its `unsafe fn` marker.
  Callers pass any `T` that derives the four zerocopy traits;
  the compiler proves POD-ness for us.

Consumers:
- `grounded2-rpg` dev-deps gain `zerocopy`. Test-side parm
  structs (`AddHealthParms`, `GetValueForStatParms`) get the
  four zerocopy derives.
- 3 `unsafe { common::parms_as_bytes(...) }` wrappers + the
  unsafe call to `call_ufunction_typed` are gone.
- `GetValueForStatParms.temporary_only` is now `u8` instead of
  `bool` (zerocopy correctly rejects `bool` because only 0/1
  are valid byte patterns; 2-255 would be UB).

Workspace clippy: 50 -> 40 unsafe-block warnings.

Still open in the zerocopy migration: dynamic-offset sites
(`damage::on_event` driven by `DamageHookConfig`, `decode_field`
driven by FProperty class string), per-UFunction parm decoder
structs in kill/fall/inv_hooks, and `FDataTableRowHandle`
(contains a raw pointer; would need a split type).

## 2026-05-12 (patternsleuth integration + workspace LICENSE + crate survey)

### patternsleuth integration

The 10-year-bar lift, finally on the right foundation.

`trumank/patternsleuth` is the Rust sig-scan crate UE4SS itself
uses to locate engine functions at runtime. Workspace dep pinned
to master @9573c52, features `process-internal` + `image-pe`.

- New module `ueforge::ue::resolvers`. `UeResolution` is an
  `impl_try_collector!` struct wrapping three UE resolvers:
  `GUObjectArray`, `FNamePool`, `FNameToString` (UE's
  `AppendString`). `resolve_image_offsets()` reads the host
  image via `patternsleuth::process::internal::read_image()`,
  calls `exe.resolve(UeResolution::resolver())`, subtracts
  `host_image_base()` so results are image-relative.
- New debug op `resolve_offsets`. Returns a side-by-side
  comparison against the configured hardcoded STEAM/XBOX
  `PlatformOffsets` so future UE patches are caught from a
  `curl` rather than a code update.
- Deleted `ueforge::ue::sigscan` (was 466 LoC). Our hand-rolled
  `Pattern::parse` + `find` + `resolve_rip32` + `text_section`
  + the `sig_scan` debug op. patternsleuth ships the equivalent
  + a UE-specific pattern library + UE-version-aware
  ranked-candidate multi-pattern support. The 8 sigscan unit
  tests are gone too (covered by patternsleuth's test suite).
- `ProcessEvent` vtable index stays hardcoded. It's a vtable
  slot, not an image offset, and patternsleuth doesn't ship a
  resolver for it. Every UE 5.x build has
  `ProcessEventIdx = 0x4C` regardless of UE-version drift.

### Workspace LICENSE

Added `LICENSE` file at the repo root (GPL-3.0-only, matching
the abix- standard from k3sc + abixio). `Cargo.toml` updated
from `MIT` (cargo new default) to `GPL-3.0-only`.

### Crate survey (rtfm pass)

After patternsleuth landed, surveyed the codebase for other
hand-rolled patterns where a maintained crate exists. Proposed
five P1 adoptions with maintenance status confirmed via gh api:

1. **zerocopy** (Google, 2026-05-12 active). Replace ~50
   `read_unaligned` matchups with derive-based `FromBytes`.
   Compile-time layout verification eliminates the wrong-
   offset / wrong-type bug class at build time. **HIGH gain.**
2. **proptest** (2026-04-30 active). Random-input fuzzing
   of the TArray / TMap / FieldTweak walkers. Catches walker
   bugs that boundary tests miss. **MEDIUM-HIGH gain.**
3. **insta** (Mitsuhiko, 2026-05-02 active). Snapshot
   testing for op JSON responses. Schema regressions surface
   as `.snap` diffs. **MEDIUM gain.**
4. **smallvec** (Servo, mature-stable). Replaces the fixed
   32-slot stack array in `Tracker::fire`. Removes silent-
   overflow risk. **LOW gain.**
5. **fastrand** (smol-rs, 2026-05-03 active). Replaces our
   hand-rolled xorshift PRNG in `hook/install.rs::jitter`.
   18 LoC saved. **LOW gain.**

Plus a documented "crate-shopping verdicts" section in todo.md
listing tracing / dashmap / bytemuck / once_cell / windows
crate / cxx / bindgen / object / goblin / tokio / rayon /
serde_with as evaluated and not worth adopting (with reasons).
Captures the rtfm analysis so future sessions don't redo this
survey.

ueforge 102/102 tests pass; all three crates build clean
release. The patternsleuth integration is unverified in-game
(needs the curl-and-compare against hardcoded STEAM/XBOX);
that's the next P0 acceptance.

## 2026-05-11 (DataTableDef Phase 2 + 1 stragglers)

Five commits across the day land the full Phase 1 finish and
most of Phase 2: read filtering, registered-catalog ops, runtime
write ops, on-disk persistence, ImGui surface, and the unified
static `TweakDef`.

### Phase 1 stragglers (66413c9)

- `ui_data_table_browser`: row-FName filter input inside the
  rows tree of the selected table. Case-insensitive substring
  match on `row_name`; footer shows `filtered: N rows matching
  '...'`.
- `data_table::register(&'static DataTableRegistry)` +
  `registered()` + `list_json()`. Per-process catalog
  registration so the new `list_data_tables` op enumerates
  statically-declared tables separately from the runtime
  discovery cache.
- `list_data_tables` op in `register_builtins`. Returns
  `{registered: false, count: 0, tables: [], note: ...}` when
  no consumer has registered; otherwise the full catalog JSON.

### Phase 2 runtime tweak ops (1acc7db)

The dynamic write primitives (`dynamic_apply_i32 / f32 / u32`)
already existed; this lift wires three new debug ops:

- **`tweak_apply`** `{table, field, kind, op, value}`. Resolves
  offset via discovery cache, captures vanilla per row on first
  apply, writes `set` / `multiply` / `add` using the captured
  baseline. Idempotent on re-apply.
- **`tweak_list`** snapshots every active tweak across the
  three primitive registries with vanilla_count per entry.
- **`tweak_revert`** reverts one specific `(table, field)` or
  all when args are empty.

Supporting surface: `dynamic_revert_one`, `dynamic_list_json`,
`tweak_apply_from_args`.

### Phase 2 persisted-tweak surface (8aa3d79)

Tweaks survive Ctrl+R hot-reload.

- Every successful `tweak_apply` writes
  `<DLL_dir>/tweaks.json` atomically (temp + fsync + rename).
- `tweak_revert` removes the matching entry; the no-args form
  clears the file.
- Schema 1 envelope: `{schema_version, tweaks: [{table, field,
  kind, op, value}, ...]}`. Missing/unparseable files start
  fresh, no error.
- Game crates call `data_table::restore_persisted_at_init()`
  from their `on_unreal_init` worker (after
  `discovery::run_at_load`) to reload + reapply at boot and
  after every Ctrl+R.
- Three new ops: `tweak_persisted_list`,
  `tweak_persisted_load`, `tweak_persisted_reapply`.
- New surface: `PersistedTweak` (serde), `record_persisted`
  (private), `forget_persisted{,_all}_pub`,
  `persisted_list_json`, `load_persisted_from_disk`,
  `reapply_persisted`, `restore_persisted_at_init`.
  `tweak_apply_inner(args, persist)` is the shared body so
  reload-time replay skips the redundant IO storm.

### Phase 2 ImGui tab (531e51c)

New module `ueforge::ui_tweaks::render`. Games include it in
`MOD_INFO.tabs` for a hot-iteration tweak UX without curl.
Three collapsing sections:

- **Apply a tweak**: table/field text inputs, kind toggle
  (i32/f32/u32), op toggle (set/multiply/add), value input.
  Apply button calls `tweak_apply_from_args`; result or error
  displayed below.
- **Active tweaks (in-memory)**: per-row Revert + Revert-all.
- **Persisted on disk**: file path + count + Reload + Reapply
  buttons + entry list.

### Phase 2 static TweakDef unification (a8c3156)

The big architectural piece. New module `ueforge::tweak`.

- `TweakTarget` enum: `DataTable { table, field }` |
  `Class { class, field }`.
- `TweakKind` enum: `I32` / `F32` / `U32`.
- `TweakOp` enum: `Set` / `Multiply` / `Add`.
- `TweakDef` carries `AtomicU64 current_value_bits` (runtime-
  tunable via store_*) + `default_value_bits` (for
  reset_to_default).
- Six const constructors covering every (target × kind):
  `data_table_i32 / f32 / u32`, `class_i32 / f32 / u32`.
- `apply` / `revert` / `reset_to_default` / `resolved` methods;
  `load_*` / `store_*` per kind.
- `TweakRegistry` slice-of-refs wrapper with `apply_all` /
  `revert_all` returning `Vec<(id, Result<rows, err>)>`.
- Backend reuse: DataTable apply delegates to
  `data_table::dynamic_apply_*` so static TweakDefs and the
  runtime tweak surface share captured vanilla on the same
  field. Class apply uses a new symmetric `DYN_CLASS_I32 / F32
  / U32` registry, same shape as the data-table side.
- Unit tests: const-construction across all six constructors,
  load/store round-trip, op math, Multiply-skips-vanilla-zero,
  registry lookup.

Existing `stacks::StackDef` + `difficulty::DifficultyDef` left
intact. Migration to TweakDef is a follow-up; non-breaking
because both shapes share the dynamic vanilla cache when
targeting the same field.

### Consumer-side sweep + module deletion

After the deprecation marker landed, the two consumer sites
migrated and the deprecated modules came out:

- `outworld-station-tweaks/src/stacks.rs` now declares
  `static MATERIALS_TWEAK: TweakDef = TweakDef::data_table_i32(
   "materials", "DT_Materials", "MaxCanStack",
   TweakOp::Multiply, DEFAULT_MULTIPLIER)`. Status counters
  (`last_applied_rows`, `ever_applied`) moved to local
  `AtomicUsize` / `AtomicBool`; `vanilla_count` reads through
  the new `TweakDef::vanilla_count()` accessor.
- `grounded2-rpg/src/survival.rs` now declares two
  `TweakDef::class_f32("hunger" / "thirst", "SurvivalComponent",
  "HungerSettings.AdjustmentPerSecond" / "ThirstSettings...",
  TweakOp::Multiply, 1.0)` statics. Offsets removed (resolved by
  field name from the discovery cache).
- `ueforge/src/stacks.rs` + `ueforge/src/difficulty.rs` deleted;
  their `pub mod` lines removed from `lib.rs`. Workspace README
  + ueforge README pointers updated.
- New surface added in support: `TweakDef::vanilla_count()` +
  `data_table::dynamic_vanilla_count(table, field)`.

### Still open in Phase 2

- Replicated-field respect (defer until concrete case).
- Non-primitive (FString / TArray) writes (FMemory ABI work).

ueforge 71/71 tests pass; both crates build clean release.
In-game smoke test still pending.

## 2026-05-11 (Triggers Phase 5c. event-driven catalog dispatch)

Five commits ship the event-driven trigger system end-to-end
(3516276..2b2c66d) plus tracking updates (60e1ece..9c01f3a).
Lifesteal + impact resistance are now real catalog rows with
their own EffectDef + TriggerDef; fall_hook PE plumbing lives in
ueforge; every g2rpg damage / kill / fall event flows through
`Tracker::fire`.

### Framework

- Real event types replace the three stubs:
  `KillEvent { victim, victim_class_name, attacker,
  attacker_is_player, damage }`, `FallEvent { player, cmc,
  velocity_z_before }`. `TriggerCtx::DamageDealt / DamageTaken`
  reuse `ueforge::damage::DamageEvent`.
- Four new framework `TriggerDef`s: `ON_DAMAGE_DEALT`,
  `ON_DAMAGE_TAKEN`, `ON_KILL`, `ON_FALL`.
- `Tracker::fire(ctx)` event dispatch. Walks the catalog under
  one `inner.lock()`, snapshots `(skill, level)` pairs to a
  32-slot stack array, drops the lock, then calls `Effect::apply`
  outside the lock so effects can re-enter Tracker (record_xp,
  etc.) without deadlocking. Zero heap allocs per fire. Kind is
  matched from the ctx variant.
- New `ueforge::fall::FallHook<B>` module mirroring
  `ueforge::damage::DamageHook` shape. PE-install +
  LazyFunctionPtr OnLanded filter + Velocity.Z snapshot from CMC
  + before/after dispatch. `FallEvent.cmc: Option<&UObject>` lets
  binders/effects write Velocity.Z without re-resolving the
  pointer offset.
- `Tracker::apply_one_unlocked` filters to OnSlotChange-kinded
  skills only. Event-driven Effects no longer get spurious
  SlotChange fires on activate / spend / refund / toggle.

### Grounded 2 migration

- `fall_hook.rs` collapses from ~270 to ~210 LoC; PE-install
  plumbing now in `ueforge::fall::FallHook`. The G2FallBinder
  owns the G2 bits: PE-queue drain, velocity stomp for
  fall_resistance, fan-out via `TRACKER.fire(TriggerCtx::Fall)`.
  Status-effect snapshot helpers stay in the file (used by debug
  endpoint).
- `effects.rs` adds `LifestealEffect` (subscribed to
  `ON_DAMAGE_DEALT`) and `ImpactReversalEffect`
  (`ON_DAMAGE_TAKEN`). Both `let-else` on the TriggerCtx variant
  they want and short-circuit otherwise.
- `kill_hook.rs` `G2DamageBinder` lost ~70 LoC: dropped
  `apply_lifesteal` and `apply_impact_resistance_reversal`
  (moved to Effects). Now fires `TRACKER.fire(DamageDealt)` on
  player-instigator hits in `before()`, `TRACKER.fire(DamageTaken)`
  on player-target hits in `after()`, and `TRACKER.fire(Kill)` on
  confirmed creature kills. Kill credit + KillerKind classifier
  stay in the binder (XP bookkeeping, not a catalog skill).
- `skills.rs` catalog rows: `SKILL_LIFESTEAL` ->
  `trigger=ON_DAMAGE_DEALT` with `LifestealEffect`;
  `SKILL_IMPACT_RESISTANCE` -> `trigger=ON_DAMAGE_TAKEN` with
  `ImpactReversalEffect`. Drops two `RuntimeEffect` placeholders.

### Hygiene

- Every unsafe block added in 5c.1..5c.5 carries a `// SAFETY:`
  comment (clippy::undocumented_unsafe_blocks is workspace-warn).

### Deferred from 5c

- `TriggerCtx::Tick { dt }` reserved variant has no firer.
  Periodic poller defers until a periodic skill needs it.
- Generic ueforge-side `FallVelocityStompEffect` is not in the
  standard effect library. G2's stomp lives in G2FallBinder
  because SKILL_FALL_RESISTANCE already uses ON_SLOT_CHANGE for
  CDO writes (PlayerFallDamageReductionEffect); promoting to a
  second SkillDef would be ugly UX. Revisit if a second game
  wants the stomp.

In-game smoke test pending. ueforge 67/67 unit tests pass; both
crates build clean release. Behavior should be unchanged.

## 2026-05-10 (composition model. Effects + Triggers + Skills)

The architectural shift articulated in
[`../ueforge/docs/architecture.md`](../ueforge/docs/architecture.md)
"Composition model: Effects + Triggers + Skills". TL;DR: each
thing we research and figure out how to do in the game is an
Effect (one type, parameterised per use). A Skill is one Effect
applied with parameters. Hooks (low-level vtable patches) and
Triggers (semantic event sources) are two distinct Defs at
different layers; skills compose at the Trigger layer.

### Effects refactor (collapsed StandardEffect + per-game SkillEffect)

- New `ueforge::rpg::effect` module with `Effect` trait + 8
  standard struct types: `PlayerFloatEffect`,
  `SubcomponentFloatEffect`, `SubcomponentAdditiveEffect`,
  `SubcomponentU32MaskEffect`, `SubcomponentMultiplyEffect`,
  `ClassFieldsMultiplyEffect`, `RuntimeEffect`,
  `StatusEffectApply`.
- Old `StandardEffect` enum + `RpgApplier` trait DELETED.
- `SkillDef<E>` / `SkillRegistry<E>` / `Tracker<A: RpgApplier>`
  lose their type parameters; `DisabledSkills` moves into
  `Tracker` as the canonical store.
- g2rpg's `SkillEffect` enum + `GameApplier` DELETED. Three
  game-specific Effect impls live in `crate::rpg::effects`
  (`BackpackSlotsEffect`, `SurvivalDrainEffect`,
  `PlayerFallDamageReductionEffect`). The giant `apply_skill`
  match in `apply.rs` is GONE.

### Triggers (Phase 1 + 2a)

- New `ueforge::rpg::trigger` module: `Trigger` trait,
  `TriggerDef { kind, imp: &'static dyn Trigger }`, `TriggerCtx`
  enum carrying typed event payloads, `OnSlotChangeTrigger` +
  static `ON_SLOT_CHANGE` reference.
- `Effect::apply` signature now takes `&TriggerCtx`.
- `SkillDef.trigger: &'static TriggerDef` field; every g2rpg
  catalog row declares `trigger: &ON_SLOT_CHANGE`.
- `Tracker` fires `TriggerCtx::SlotChange` on activate / spend /
  refund / toggle. Other variants (Kill, Fall, DamageDealt,
  DamageTaken, Tick) are placeholder stubs; Phase 5c lifts
  kill_hook / fall_hook into framework triggers that fire them.

### Health ops lift

- New `ueforge::rpg::health` module: `HealthBinding { hc_class,
  hc_selector, current_damage_offset, max_health_offset,
  add_health_function, set_current_health_function }` +
  `register(binding, pe_queue, hint)`.
- g2rpg's `simulate_add_health` + `simulate_set_current_health`
  ops + their PE-queue plumbing (`DebugCmd` enum,
  `enqueue_pe(DebugCmd)`, `execute_on_game_thread`,
  `exec_add_health`, `exec_set_current_health`,
  `op_simulate_add_health`, `op_simulate_set_current_health`)
  DELETED. ~70 LoC drop from g2rpg.

### Wave B: cosmetic alignment sweeps

- `StackTweak` -> `StackDef` + `StackRegistry`. ows-tweaks's
  `STACKS = StackRegistry::new(&STACK_DEFS)`.
- `DifficultyKnob` -> `DifficultyDef` + `DifficultyRegistry`.
  g2rpg's `SURVIVAL = DifficultyRegistry` holding hunger +
  thirst Defs.
- `ModInfo` -> `ModDef` + `Tab` -> `TabDef`. Def-suffix mandate
  uniform workspace-wide. Tab is the documented bare-slice
  exception.

### Misc cleanup

- `leak_cstr` (const no-op pretending to do C-string work)
  DELETED from `mod_main.rs`.

## 2026-05-10 (registry alignment wave A: ops + selectors + shutdown)

Per the new
[architecture.md](../ueforge/docs/architecture.md) Def +
Registry contract, every cross-cutting subject in ueforge now
exposes the same `<Subject>Def` + `<Subject>Registry` surface.
Three high-leverage refactors landed:

### Debug ops (was 30%, now 100%)

`ueforge::ops::OpDef` + `OpRegistry` + `OP_REGISTRY` singleton
collapse three hardcoded dispatchers (`handle_builtin`,
`dispatch_standard_op`, `dispatch_pe_ops`) into a single
`OP_REGISTRY.dispatch(op, args)` call. Game crates `register()`
their ops at worker init via captured-state closures
(tracker, pe_queue, selector resolver). Auto-generated
`list_ops` op for client discovery. Per-mod handle() shrank
from ~40 lines of match arms to ~10. See
[architecture.md](../ueforge/docs/architecture.md) "Debug
ops" row.

### Selectors (was 30%, now 100%)

`ueforge::selector::SelectorDef` + `SelectorRegistry` +
`SELECTOR_REGISTRY` singleton. `selector::resolve(s)` is the
one entry point for every op + scanner site; walks the
registry. Framework ships `addr:` / `class:` / `first_class:`
/ `singleton:` via `register_builtins()`; game crates extend
with their own (`live_player`, `live_player_hc` in g2rpg)
without touching framework code. Auto-generated
`list_selectors` op.

### Shutdown handlers (was 0%, now 100%)

`ueforge::shutdown::ShutdownHandlerDef` + `ShutdownRegistry`
+ `SHUTDOWN_REGISTRY` singleton with `run_all()` sorting by
`order: u32`. The `ueforge_mod_shutdown` macro collapses to:
`game.on_shutdown()` -> `register_builtins()` -> `run_all()` ->
`finalize_hot_reload_swap()`. Built-ins use orders
hooks=100 / http=200 / settings=300 / scanner=400; game
crates interleave at `50` (pre-framework) or `500+` (post).
New ueforge subsystem can't forget the wiring: it adds a
line to `register_builtins`. See
[architecture.md](../ueforge/docs/architecture.md) "Shutdown
handlers" row.

## 2026-05-10 (registry alignment wave 0: skills + creatures + hooks)

Hard rename to the `<Subject>Def + <Subject>Registry` contract
(no aliases, hard migration):

- **Skills**: `Skill<E>` -> `SkillDef<E>` + new
  `SkillRegistry<E>` wrapper. Lookup via `CATALOG.def(id)`;
  iteration via `CATALOG.iter()` or `for s in CATALOG`.
  `find_skill` / `skill_def` free fns removed.
- **Creatures** (was Bestiary): tuple `(&str, u32)` -> named
  `CreatureDef { class_name, base_xp }` + `CreatureRegistry`.
  Per-mod `static CREATURES: CreatureRegistry`.
- **Hooks**: private `Entry` -> public `HookDef` with accessors;
  new `HookRegistry` struct + `HOOK_REGISTRY` singleton; existing
  `register` / `register_many` / `shutdown_all` free fns become
  thin wrappers around the registry methods. `installed_defs()`
  snapshot accessor for debug surfaces.

See [architecture.md](../ueforge/docs/architecture.md) for the
full naming contract + per-subject scorecard.

## 2026-05-10 (kovarex review wave 2 P1. Continued)

Four more P1 items landed in a follow-up session. Workspace check
+ 68 ueforge lib tests green.

- **SlotStore failure-injection tests.** 6 new tests cover save
  failures (parent-as-file, target-as-nonempty-dir), `last_error`
  cache populates + clears across recovery, corrupt JSON loads
  as default, missing file loads as default, and an existing
  good file is untouched when an unrelated save fails.
  `save_to_path` / `load_from_path` test seams added on
  `SlotStore` (pub(crate)) so failure injection doesn't depend
  on the process-global `dll_dir()`. See
  [ueforge/src/rpg/store.rs](../ueforge/src/rpg/store.rs).

- **Freeze sweeper + cap.** Replaced per-freeze writer threads
  with a single `ueforge-freeze-sweeper` thread that iterates
  the freeze map at the soonest-due tick (clamped 1-10ms).
  `MAX_FREEZES = 64` cap; `freeze` past the cap returns an
  error. Sweeper is lazy-spawned on first freeze and stopped via
  `scanner::shutdown_sweeper_if_running` from the framework's
  `ueforge_mod_shutdown` (between settings shutdown and
  side-file rename). Re-resolve / failure-counting / drop-on-
  permanent-staleness logic moved into the sweeper's per-job
  loop with the same `MAX_CONSECUTIVE_FAILURES = 30` semantics.

- **Hot-reload entry-leak audit.** `Entry` `Box::leak` per
  install is **intentional and bounded**. Reusing entries
  across DLL unloads is unsafe (the `handler: Box<dyn Fn>`
  carries a vtable into the unloading DLL's code). Per-cycle
  cost: ~250 bytes/hook + closure capture; 1000 reloads x 5
  hooks = ~1 MB. Added `LEAKED_ENTRY_COUNT` instrumentation
  + `hook::leaked_entry_count()` accessor for snapshot
  surfaces so dev sessions can monitor accumulation. Audit
  conclusion documented in
  [ueforge/docs/hooks.md](../ueforge/docs/hooks.md)
  "Entries are leaked, not freed".

- **`clippy::undocumented_unsafe_blocks` enabled workspace-wide
  at `warn`** via `[workspace.lints.clippy]` + per-crate
  `[lints] workspace = true`. 271 existing undocumented unsafe
  blocks surface as warnings; new unsafe must carry
  `// SAFETY:` to merge clean. Flip to `deny` when the count
  reaches zero.

## 2026-05-10 (kovarex review wave 2. Durability + safety)

Brutal-honesty review against the 10/10 bar (10 years daily-driver,
no patches). Five P0 correctness/crash bugs and three P1 grooming
items shipped. All design knowledge captured in the per-subject
docs cited below; this entry is a pointer index.

### P0. Correctness / crash vectors (all shipped)

- **Spend / refund / record_xp / debug_grant transactional with
  disk save.** Stage-save-commit pattern: snapshot rollback values,
  mutate in-memory, save to disk first, apply to live world only
  on save success; restore the snapshot on failure. Disk full or
  EACCES no longer leaves the session and the save file in
  disagreement. See [ueforge/docs/rpg.md](../ueforge/docs/rpg.md)
  "Spending / refunding".

- **PE queue bounded + cancellation on caller timeout.** `Queue`
  carries `max_depth` (default 1024) and per-job
  `Arc<AtomicBool> cancelled`. `enqueue` past the cap returns 503;
  `recv_timeout` flips the cancel flag so the next drain skips the
  job. Closes the double-execute bug where a client retry would
  re-run the original (still-queued) op. See
  [ueforge/docs/pe-queue.md](../ueforge/docs/pe-queue.md)
  "Bounded depth + cancellation".

- **HTTP body cap + constant-time auth.** 1 MiB body cap (413 over),
  byte-by-byte auth header compare with no early exit. See
  [ueforge/docs/http.md](../ueforge/docs/http.md) "Safety / production".

- **Scanner fault-safe page reads.** `scan_memory` reads each
  region in 64 KiB chunks via `ReadProcessMemory`-backed
  `safe_read_chunk`; `scan_rescan` uses `safe_read_bits` per
  survivor. UE allocator freeing a page mid-scan no longer
  crashes the host. See
  [ueforge/docs/memory-tools.md](../ueforge/docs/memory-tools.md)
  "Fault-safe page reads".

- **Trampoline panic must not double-call original.**
  Thread-local `CALLED_ORIGINAL` flag set by
  `OriginalProcessEvent::call`. The trampoline saves/restores
  around the handler invocation (reentrance-safe). On panic, falls
  through to original ONLY when the handler had not already
  called it. See [ueforge/docs/hooks.md](../ueforge/docs/hooks.md)
  "Panic safety".

### P1. Grooming (three of ten shipped)

- **`Arc<str>` FName cache.** `NameResolver` caches `Arc<str>`;
  cache-hit returns a ref-bump, no `String::clone`. `to_arc`
  added for callers that only need `&str`; `to_string` retained
  as a thin wrapper. `is_default_object` migrated. See
  [ueforge/docs/ue-sdk.md](../ueforge/docs/ue-sdk.md) "FName".

- **Trampoline panic counter.** Per-`Entry` `AtomicU64` bumped in
  the catch_unwind err arm; `ProcessEventHook::panic_count` and
  `hook::panic_count_total` for snapshot surfaces. Without this,
  silent handler failures were invisible. See
  [ueforge/docs/hooks.md](../ueforge/docs/hooks.md) "Panic safety".

- **`Settings::watch` teardown via registry.** Auto-registered at
  spawn; `settings::shutdown_all()` stops + joins all watchers,
  wired into `ueforge_mod_shutdown` between server shutdown and
  side-file rename. Closes the watcher-thread leak across
  hot-reload cycles. See
  [ueforge/docs/settings.md](../ueforge/docs/settings.md)
  "Hot-reload (`Settings::watch`)" and
  [ueforge/docs/lifecycle.md](../ueforge/docs/lifecycle.md).

Remaining P1s (sig-scan offsets, SlotStore failure-injection,
generic schema versioning, freeze thread pool, hot-reload entry
leak audit + torture test, `clippy::undocumented_unsafe_blocks`)
and the entire P2 grooming list live in [`todo.md`](todo.md).

## 2026-05-10 (Phase 3 wave 2: g2rpg catalog + framework lifts)

### bbp catalog migrated to `Standard(StandardEffect)`

`grounded2-rpg::SkillEffect` shrunk from 11 variants to 4. Nine
of 13 catalog skills now route through `ueforge::rpg::std_effect::
StandardEffect` (Attack Damage, Armor, Move/Jump/Glide/Leap,
Impact Resistance, Lifesteal, Max Health, Health Regen). The four
that stay game-specific are genuine composites: Backpack
(InventoryComponent CDO write), Hunger / Thirst (settings-multiplier
drain), and Fall Damage (multi-component HC ratio + GMS CDO + SMMC
live + UFunction call).

`apply::apply_skill` collapsed from 11 arms to 4. The Standard arm
forwards `e.apply(level, max_level, &PLAYER)` to the framework with
zero game-specific dispatch. `format_effect` followed: `Standard`
delegates to `StandardEffect::format`, the other three arms keep
their tailored format strings. ~570 lines of duplicated apply +
format dispatch deleted.

### Framework lifts (DRY for the next UE-game RPG)

Five extractions to ueforge so a future ows-rpg ships with only
game-specific logic:

- **`ueforge::rpg::Bestiary`**. Per-creature XP table keyed by BP
  class short name with a default-fallback. Replaces a per-game
  `OnceLock<Vec<(&str, u32)>>` + linear-scan helper.
- **`ueforge::ue::field`**. Generic untyped UObject byte ops:
  `read_f32` / `write_f32` / `read_u32` / `write_u32` / `read_i32` /
  `write_i32` / `read_bool` / `write_bool` / `read_component_ptr`.
  The TypedField counterpart still exists for structured sites;
  these are the escape hatch for runtime-decided offsets.
- **`ueforge::ue::actor`**. Kill-hook universals:
  `class_chain_contains(obj, needle)` walks the UClass + super
  chain looking for a class-name substring (depth-bounded);
  `controller_pawn(controller)` reads the stable `AController.
  Pawn` slot at `+0x0308`; `describe(obj)` formats `name(class)`
  for log lines; `A_CONTROLLER_PAWN_OFFSET` constant.
- **`FWeakObjectPtr::read` + `::resolve`**. Fold the index
  validation + Runtime + GObjectsView walk into one method on the
  existing struct. Two callers in g2rpg's kill_hook collapsed to
  single-line uses.
- **`ueforge::debug`**. First wave of the debug-endpoint scaffold:
  `PlayerStateView::from_state`, `CatalogEntry`, `catalog_view`,
  `STANDARD_OPS`. Game-specific snapshot collectors stay in the
  game crate; the universal view types and op-list metadata are
  shared.

After these lifts: g2rpg's `apply.rs` is 320 lines (was 814);
`skills.rs` is 460 lines (was 544); `xp.rs` is 53 lines (was 63
including bespoke OnceLock); `kill_hook.rs` shed ~50 lines of
duplicated UE-class-chain + weak-ptr walk; `debug.rs` shed ~30
lines of view-struct boilerplate. All ueforge unit tests
green (62 pass).

### Pester-style scenario DSL + RPG-op shortcuts on `Api<S>`

Per-skill tests were the last big duplication pile. Each one
reimplementing connect/skip + read-baseline + check-skill-points
+ spend + read-after + assert + refund. ~30 lines apiece.

Lifted to `ueforge::client::scenario`:

```rust
scenario::for_skill(api.inner(), "attack_damage")
    .reads(|s: &common::Snapshot| {
        Some(s.live_player.as_ref()?.asc.as_ref()?.custom_damage_multiplier)
    })
    .should_grow_when_spent();
```

Four assertion shapes provided:
- `should_grow_when_spent`. Spend 1, value strictly greater, refund.
- `should_shrink_when_spent`. Spend 1, value strictly less, refund.
- `should_revert_when_refunded`. Spend then refund; baseline restored.
- `should_revert_when_toggled_off`. Spend, toggle off (vanilla
  restored), toggle on (boost restored), refund.

The DSL handles environment prerequisites (skips with a clear
log line if no live player / no skill points) so partial setups
don't fail the suite.

Plus standard RPG-op shortcuts on `Api<S>`:
- `skill_spend(id, count)` / `skill_refund(id, count)` /
  `skill_toggle(id, bool)` / `set_skill_points(count)` /
  `reload_slot()`. Each calls `op_ok` + returns post-op state.
- `skill_level(id) -> u32` / `skill_points() -> u32`. Read from
  the snapshot's `player_state` JSON path. Available on any
  `Api<S>` regardless of the per-mod typed Snapshot shape.

Per-skill test migrations:

| Test | Before | After |
|---|---|---|
| skill_attack_damage | 38 | 16 |
| skill_armor | 23 | 15 |
| skill_jump_height | 36 | 15 |
| skill_glide_speed | 34 | 26 (+1 assertion) |
| skill_max_health | 30 | 26 (+1 assertion) |
| skill_lifesteal | 35 | 25 |
| skill_move_speed | 73 | 26 |
| skill_health_regen | 60 | 38 (+1 assertion) |
| skill_hunger | 55 | 27 (+1 assertion) |
| skill_thirst | 46 | 26 (+1 assertion) |
| skill_leap_distance | 46 | 34 (+2 assertions) |

Plus `research_probes.rs` 258 -> 188 (uses the new
`find_live_instance` + `read_component_ptr` +
`call_ufunction_typed` helpers).

**Net: -340 lines of test boilerplate; +9 new test scenarios
gained from the easier shape (now testing toggle-revert + refund
on multiple skills where the original had only spend-grew).**

The tests now read like Pester / English: "for skill X,
reads Y, should grow when spent".

### Research-test helpers wave 2 (`client::diff` + class-CDO + thread report)

Continued the research-helpers extraction. Three more ueforge
surfaces + four test migrations:

- **`ueforge::client::diff`** (608 lines, new module).
  `MetricsSnapshot::from_api` captures the standard snapshot
  block (counters / process_memory / process_cpu /
  process_threads / game_population). `diff_all` / `diff_counters`
  / `diff_memory` / `diff_cpu` / `diff_threads` /
  `diff_population` produce typed diff structs with `Display`
  impls that emit the same table format every test was
  hand-rolling. `MetricsSnapshot::sample_series(api, count,
  interval) -> SampleSeries` for time-series patterns.
- **`Api::snapshot_value`**. Bypasses typed deserialization so
  `client::diff` works with any per-mod `Snapshot` type that's
  `DeserializeOwned` (no `Serialize` bound needed).
- **`research::find_class_cdo`** /
  `research::walk_class_instances_with_cdo`. Find a class's
  default object in one call.
- **`research::sample_thread_modules`** +
  `ThreadModulesReport`. Typed view of the
  `sample_thread_modules` op response with Display impl.

Test migrations:

| Test | Before | After | Saved |
|---|---|---|---|
| `explore_perf_counters` | 258 | 53 | 205 |
| `explore_perf_timeseries` | 106 | 31 | 75 |
| `explore_thread_attribution` | 82 | 30 | 52 |
| `explore_environmental_damage_type` | 138 | 107 | 31 |

Plus the prior wave: `explore_dt_rows` 140->47, `explore_status_effect_rows`
218->105.

**Net across both research-helpers waves: ~570 lines of test
boilerplate deleted; ~1100 lines of reusable framework added.**
Every future UE-game investigation gets the same savings.

`explore_leak_source.rs` (350 lines) was the next candidate but
uses g2-specific `top_packages` / `loaded_levels` /
`process_regions` extensions to `gobjects_population` not in
the framework today. Deferred until a second consumer wants
those probe extensions.

### Research-test helpers (`ueforge::client::research`)

Every `explore_*` test in g2rpg + ows-tweaks reimplemented the
same handful of patterns from scratch: find a DataTable, walk
its rows (TMap header + batch element array + FName
extraction), find class instances, resolve FNames, read row
fields at offsets. ~150-250 lines of boilerplate per test.

Lifted to `ueforge::client::research`:

- `find_data_table_by_name(api, "DT_X") -> Option<(selector, addr)>`
- `find_data_table_by_path(api, "...substring...")`. When
  multiple tables share a short name (CDO + live).
- `read_data_table_rows(api, selector) -> Vec<DtRow { fname, addr }>`.
  reads the TMap header at +0x30 + batch-reads the element
  array in one `read_bytes` call.
- `walk_class_instances(api, "ClassName", max) -> Vec<ClassInstance>`.
- `fname_to_string(api, fname_u64) -> Option<String>`.
- Typed read helpers: `read_i32` / `read_u32` / `read_f32` /
  `read_u8` / `read_u64` / `read_bytes`. "give me the field
  at this offset on this address".

Migrations:

- `outworld-station-tweaks/tests/explore_dt_rows.rs`: 140 -> 47
  lines (-93). All TMap header parsing + slot iteration + row
  field reads collapsed.
- `grounded2-rpg/tests/explore_status_effect_rows.rs`: 218 -> 105
  lines (-113). Same.

Future work: `ueforge::client::diff` (counter diff, GObjects
population diff, perf time series) collapses ~800 more lines of
g2rpg perf / leak research boilerplate the same way. Tracked
in `docs/todo.md`.

### Hardening bundle: HTTP auth + FName size guard

Three kovarex P1/P2 items resolved in one pass:

- **HTTP per-launch auth token** -- `server::Config::auth_token:
  Option<&'static str>`. When `Some(t)`, every request must carry
  `X-Ueforge-Auth: <t>` header or gets a 401. When `None`, no
  auth (back-compat default). `client::Api::with_auth(token)`
  builder method on the test client. Token generation + storage
  stays a consumer concern; one-line at startup.
- **FName size guard** -- `const _: () = assert!(size_of::<FName>()
  == 8 && align_of::<FName>() <= 8)` in `ue::fname.rs`. If UE
  ever changes the layout beneath our `transmute_copy::<u64,
  FName>` sites, this fires at build time instead of silently
  corrupting names at runtime.
- **`process_event_idx`**. Audited; already correctly required
  on `PlatformOffsets` (no Default impl, both STEAM/XBOX consts
  set it explicitly). Todo entry was misadvised; closed.
- **`#[non_exhaustive]` audit**. Correctly skipped. `ModInfo` /
  `Tab` / `PlatformOffsets` / `Config` are literal-constructed
  by every consumer; `#[non_exhaustive]` would break the call
  sites without giving us actual API stability. In a monorepo
  with atomic updates, breaking changes mean "one-line update
  at the call site in the same commit". Which is fine.
  Documented the rationale in todo.md so future audits don't
  re-litigate.

### Damage module + Lifesteal live

`ueforge::damage::DamageHook<B>`. Universal damage-event hook,
the fifth opinionated module. Same shape as the inventory viewport hook:
config struct + binder trait. Owns the multicast UFunction
trampoline + parm decode (Damage / DamageFlags / TypeFlags) +
`FDamageInfo` lookup (instigator resolution via
`DamageInfoLayout`) + Player/Other classification (via
`is_outer_named` + `class_chain_contains`) + `before` /
`after` dispatch (binder mutates damage pre-application or
reacts post-application).

g2rpg-side migration:

- `kill_hook.rs` rewritten as a `DamageBinder` impl. Owns the
  Maine `DamageHookConfig` (component class + UFunction +
  parm offsets), the `KillerKind` classifier (Player /
  Buggy / Other. Buggies are tame G2 mounts), and the
  per-event reactions: damage-trace push, impact-resistance
  reversal, kill credit, Lifesteal heal.
- `damage_trace.rs` (168 lines) and `impact_resistance.rs`
  (79 lines) deleted. Their logic now lives inside the binder
  using the framework's already-decoded `DamageEvent`.
- Total g2rpg damage code: 474 lines (3 files) -> 238 lines
  (1 file).

**Lifesteal landed live**: when an attacker classified as
the player damages a non-player victim, the binder reads the
player's lifesteal level from the live tracker, computes
`heal = damage * 0.90 * sqrt(level/100)`, walks
`PLAYER.first_live_static` to the live HC at +0x1340, and
decrements `CurrentDamage` at +0x32C (clamped to 0). Skill
toggle honored. Catalog row already existed
(`Standard::Runtime` with `max_bonus = 0.90`); previously a
no-op since no live-damage hook was wired. Now functional.

Critical / Evasion / Thorns rows pending; the framework
shape supports them (Critical = `before` returns multiplied
damage on roll; Evasion = `before` returns 0 on roll;
Thorns = `after` walks attacker's HC + applies %).

### Hot-update Phase B complete (B1-B5: safe Ctrl+R with hooks)

Five lifts that close the loop on hot-update. Ctrl+R is now
safe even with PE hooks installed:

- **B1: `ueforge::hook::registry`** -- `register(hook)` /
  `register_many(hooks)` / `shutdown_all()`. Hook handles live
  in a `static Mutex<Vec<ProcessEventHook>>` instead of being
  `mem::forget`-ed. `shutdown_all` drops every handle in
  registration order.
- **B2: `SHUTTING_DOWN` + `active_calls` drain.**
  `process_event::SHUTTING_DOWN: AtomicBool` short-circuits new
  trampoline fires straight to engine's original ProcessEvent.
  Each `Entry` carries `active_calls: AtomicUsize` incremented
  at trampoline entry, decremented at exit. `Drop` restores the
  vtable slot, then spins (with `thread::yield_now`) up to
  500ms for the counter to hit zero. Logs a warning on timeout.
- **B3: `server::SpawnHandle` + `server::shutdown_all`.**
  `spawn` registers (Server Arc + JoinHandle); `SpawnHandle::stop`
  calls `tiny_http::Server::unblock` then joins. `shutdown_all`
  clears the registry, dropping every handle.
- **B4: `ueforge_mod_shutdown` orchestration.** The macro path
  now runs:
  1. game's `MOD_INFO.on_shutdown`
  2. `hook::shutdown_all` (uninstall + drain)
  3. `server::shutdown_all` (stop listeners + join)
  4. `mod_main::finalize_hot_reload_swap` (side-file rename)
- **B5: g2rpg-side adoption.** `mem::forget` calls in
  `lib.rs::worker` replaced with `ueforge::hook::register` /
  `register_many`. `install_immediate_or_log` (already routes
  through registry) keeps its existing call sites.

Dev loop now end-to-end:

```
1. edit Rust
2. cargo deploy install -p grounded2-rpg     # writes main-new.dll
3. alt-tab to game, Ctrl+R                   # ~1-2s reload
4. test
```

State on disk (save slots, settings, catalog) survives; the new
image reads it on slot activation. From close-to-test cycle of
60-180s down to ~5s.

### Hot-update Phase B0 shipped (side-file deploy)

Implemented the side-file pattern (user's design) for hot-deploy
while the game is running. Two pieces:

- `cargo deploy install` (`ueforge/src/bin/ueforge_deploy.rs`):
  writes to `main-new.dll` when `main.dll` already exists; first
  deploy writes `main.dll` directly. The side-file path is never
  locked (no live image at that path) so `fs::copy` succeeds
  while UE4SS has the running DLL mapped.
- ueforge shim's `ueforge_mod_shutdown`
  (`ueforge/src/mod_main.rs`): after the game's `on_shutdown`,
  the framework's `finalize_hot_reload_swap` checks for
  `main-new.dll` and. If present. Renames `main.dll` ->
  `main-old.dll` (`SHARE_DELETE` permits the rename of the
  loaded image) then `main-new.dll` -> `main.dll`. Rolls back
  on step 2 failure to leave the dir consistent. Logs the swap.
- ueforge shim's `ueforge_mod_unreal_init` calls
  `cleanup_old_dll` once on the new image's first init.
  best-effort `remove(main-old.dll)`.

Verified empirically (PowerShell test loading `version.dll`
through every step of the swap; all five succeeded. Rename
loaded DLL, rename side-file into place, FreeLibrary, delete
old, LoadLibrary new).

**Workflow today**: edit Rust -> `cargo deploy install` (no
need to close the game) -> close + reopen the game. The
shutdown swap fires on close so the next launch loads the new
image. Net win: you can deploy in parallel with running
sessions instead of having to close first.

Phase B1-B5 (HookRegistry::shutdown_all + active_calls drain +
SpawnHandle::stop) still pending; until they ship, Ctrl+R is
unsafe with PE hooks installed (old DLL's vtable patches point
into freed memory after FreeLibrary). Close + reopen the game
between iterations until then.

### Hot-update: locked-DLL gotcha verified

Earlier hot-update doc claim ("just `cargo deploy install` while
the game runs") was wrong. Verified empirically (PowerShell test
loading `version.dll` then attempting overwrite + rename):

- **Direct overwrite of a loaded DLL: fails** with sharing
  violation. `LoadLibraryExW` opens the file with
  `FILE_SHARE_READ | FILE_SHARE_DELETE` (no SHARE_WRITE), so
  `fs::copy` over a live DLL gets `os error 32`.
- **Rename old -> `.old`, then write new: works.** `SHARE_DELETE`
  permits the rename; the new file lives at the canonical path
  for next `LoadLibraryExW`.

`cargo deploy install` today uses `fs::copy` and reports "file in
use". The fix (Phase B0 in `docs/todo.md`) is rename-and-replace
on sharing-violation. Lifecycle + building docs updated to
reflect reality + flag this as an open blocker.

### Hot-update research (Phase A complete)

Confirmed that UE4SS supports full hot-**update** of cpp mods
natively, not just hot-reload. The flow:

- `Ctrl+R` (default; `EnableHotReloadSystem` + `HotReloadKey` in
  UE4SS-settings.ini) calls `UE4SSProgram::queue_reinstall_mods`.
- `uninstall_mods()` calls each cpp mod's `uninstall_mod` ->
  `~UespyMod()` -> our `ueforge_mod_shutdown` -> game's
  `MOD_INFO.on_shutdown` callback.
- `~CppMod()` calls `FreeLibrary(main.dll)`. Our DLL detaches.
- `setup_mods()` rescans disk and `LoadLibraryExW`-s a **fresh**
  `main.dll`. **This is the hot-update step**: whatever DLL is
  on disk gets loaded, so a `cargo deploy install` between
  build + Ctrl+R lands the new build.
- `start_cpp_mods()` -> `start_mod()` -> fresh `UespyMod` ->
  `on_unreal_init` -> our worker init runs again.

Dev loop becomes:
```
1. edit Rust
2. cargo deploy install -p grounded2-rpg
3. alt-tab to game, press Ctrl+R
4. new version is live in ~1-2s
```

Static state in the DLL resets (intended. Atomics, OnceLocks
reload from disk via `Tracker<A>::activate_slot`). Cached UE
references survive (they point INTO the game process, not into
our DLL); the new image re-resolves on first use.

**Phase B implementation pending.** Until it lands, mods that
install `ProcessEventHook`s (kill / fall / inv) cannot safely
hot-reload. The old DLL's vtable patches still point into
freed memory after FreeLibrary. The Phase B plan
(`docs/todo.md`):

- B1: `HookRegistry` + `shutdown_all()` (track + drop hook
  handles instead of `mem::forget`).
- B2: `SHUTTING_DOWN` flag + `active_calls` counter per hook;
  Drop waits for trampoline drain.
- B3: `server::SpawnHandle::stop()` for HTTP listener.
- B4: Wire `ueforge_mod_shutdown` to call `shutdown_all`
  before the game callback.
- B5: g2rpg: stop using `mem::forget` on hook handles.

`ueforge/docs/lifecycle.md` updated with full hot-reload section
covering mechanism, dev loop, what survives / resets, and the
Phase B caveat.

### Inventory module: viewport-paging framework

`ueforge::inventory::viewport`. The universal "fixed-size
visible grid over a larger underlying inventory" pattern,
extracted from g2rpg's `inv_hook.rs`. Owns the algorithm + state
+ ProcessEvent hook trampoline + mouse-wheel scroll handling +
per-widget viewport-start map + synthetic-refresh re-entrance
guard + post-refresh rebind + construct-reset.

Game crate writes a [`ViewportConfig`] (class name + UFunction
names + grid offset + page size + scroll step) and a
[`ViewportBinder`] trait impl with two methods (`mouse_wheel_delta`
extraction from the game's `OnMouseWheel` parm bytes, `bind_slot`
calling the game's "initialize visible slot" UFunction) + a
`begin_rebind` associated context for caching per-cycle state
(typically the `GetInventoryItems` TArray result so we don't
re-call it 40x per scroll).

g2rpg's `inv_hook.rs` shrank 396 -> 220 lines (-176). The remaining
220 lines are: Maine-specific class names + UFunction parm `#[repr(C)]`
mirrors + the binder impl that wires the Maine UFunction handles
together. The viewport-start state, mouse-wheel delta dispatch,
re-entrance guard, post-refresh rebind, and the install / log /
class-name lookup machinery are all framework-side.

`grounded2-rpg/src/parms.rs` also shrank: `IntReturnParms`,
`GetChildAtParms`, `SelectedIndexParms`,
`SetSelectedInventorySlotParms` deleted (the framework's
`PanelWidget` helper owns the UMG-side parms; the
selected-slot parms were always unused).

### Heterogeneous-module principle (formalized)

ueforge's `lib.rs` doc-header + `README.md` now formalize the
design rule: **each universal pattern is defined ONCE in
ueforge.** Modules (rpg / stacks / difficulty / inventory) are
independent, opt-in via use sites. A pure stack-size mod only
consumes `stacks`. An RPG-only mod only consumes `rpg`. A mod
that uses all four picks one knob from each menu and ignores
the rest. Game crates carry only game-specific knowledge
(UE class names, field offsets, UFunction parm shapes); the
per-game extension surface is `&'static` config + an opt-in
trait impl.

If you find the same scaffolding in two game crates, that's a
missing module. File under "Open: more ueforge extraction
candidates" in `docs/todo.md`.

### Settings hot-reload + PE-call + OpRouter PE-ops half

Three more lifts that close out most of the "Open: more ueforge
extraction candidates" list:

- **`Settings::watch(interval, on_reload)`**. Spawns an
  mtime-poller thread that reloads `<DLL_dir>/<file>.json` when
  it changes on disk and fires a game-supplied `on_reload(&T)`
  callback. Drop the returned `WatchHandle` to stop. `reload()`
  available standalone for tab-driven manual reloads / debug
  ops. Tests pass; build clean.
- **`ueforge::ue::pe_call::call_ufunction`**. Folds "find
  UFunction or error -> process_event with parm pointer" into
  one safe-typed call. g2rpg's `exec_add_health` /
  `exec_set_current_health` collapsed to use it; covers every
  future health / inventory / status-effect op shape with one
  primitive instead of a per-fn boilerplate copy.
- **`ueforge::debug::dispatch_pe_ops`**. The second half of
  the OpRouter lift. Handles `call` / `read_bytes` /
  `write_bytes` (the three standard ops that need a per-game
  instance resolver). Combined with the already-shipped
  `dispatch_standard_op`, every standard op now routes through
  one of two ueforge dispatchers with one line per group.

g2rpg's `debug.rs`: 770 -> 737 lines. The handler is now three
guard arms (snapshot / dispatch_standard_op / dispatch_pe_ops)
+ four game-specific simulate_* arms + the unknown-op error.

### Three modules: rpg / stacks / difficulty

ueforge now ships opinionated framework modules for the three
most common UE5 mod patterns, each wrapping a low-level primitive
with the universal apply-loop + atomic-knob + status-counter
shape:

- **`ueforge::rpg`** (existing). Skill catalog, XP curve,
  bestiary, per-slot persistence, ImGui tab, the
  `StandardEffect` 8-variant menu.
- **`ueforge::stacks`** (new). Inventory stack-size data-table
  tweak. Wraps `FieldTweak<i32>` with multiplier atomic, last-
  applied / ever-applied counters, on-first-sight worker, and
  apply-now / revert helpers. Game crate writes one
  `StackTweak::new(table, offset, default_mult, skip_predicate)`
  static.
- **`ueforge::difficulty`** (new). CDO field tweak for
  difficulty knobs (drain rates, damage multipliers, regen,
  etc). Wraps `ClassFieldTweak<f32>` with f32 multiplier atomic
  + apply_to_cdos / apply_to_all / apply_with_filter / revert.
  Game crate writes one `DifficultyKnob::new(class, offset)`
  static per knob and calls `set_multiplier` + `apply_to_cdos`.

Migrations:

- `outworld-station-tweaks/src/stacks.rs`: 86 -> 64 lines (-22).
  The vanilla snapshot, multiplier atomic, status counters, and
  apply worker all moved to `StackTweak`.
- `grounded2-rpg/src/survival.rs`: 85 -> 41 lines (-44). The
  hand-rolled "skip if multiplier == 1.0", `ClassFieldTweak`
  invocations, and stat aggregation all collapsed into
  `DifficultyKnob::apply_to_cdos`.

The framework's design rule going forward: a UE5 game implementing
RPG + stacks + difficulty writes ONLY game-specific knobs (table
names, field offsets, multiplier UI, settings persistence). No
re-implementation of the apply loop, the vanilla cache, or the
counter atomics.

### Bootstrap + class-name lookup helpers

Three more universals lifted to ueforge so each new mod's
boilerplate shrinks:

- **`ueforge::ue::platform::detect_and_init`**. One-call
  replacement for the 15-line "detect host exe -> pick offsets ->
  log image_base/GObjects -> init_runtime" boilerplate every
  mod's worker thread runs at startup. Falls back to the first
  platform row with a WARN if the host exe doesn't match.
- **`ueforge::ue::with_first_instance_of(class_name, f)`** /
  **`with_first_cdo_of`**. String-keyed convenience wrappers
  for ad-hoc snapshot / debug call sites. Replaces 4 `apply::*`
  helpers in g2rpg.
- **`PlayerRef::first_live_static`** (unsafe). The
  game-thread-only "first live pawn as `&'static UObject`"
  pattern, used by debug-endpoint resolvers that pass refs
  through queued-closure boundaries. Wraps the unsafe-extend-
  lifetime trick in one place with a clear safety contract.
- **`ClassRef::with_first_cdo`**. Symmetric counterpart to
  the existing `with_first_instance`.
- **`ueforge::hook::install_immediate_or_log`**. The universal
  "install once, log success or failure, leak handle" pattern
  every mod's worker runs per hook. Replaces the hand-rolled
  `match try_install() { Ok(h) => log + forget; Err(e) => log }`
  triplet at every call site.

g2rpg's `lib.rs` shrank 151 -> 136 lines (-15); `debug.rs`
shrank 786 -> 770 (-16); `apply.rs` shrank 398 -> 371 (-27 by
deleting `first_instance_of` / `class_default_object`
wrappers).

### debug.rs scaffolding lift

Promoted four pieces of g2rpg's debug endpoint to ueforge so the
next mod's `debug.rs` doesn't re-implement them:

- **`ueforge::debug::DamageEvent` + `DamageRing`**. Shared event
  shape + EventRing wrapper with the standard accessors. Game
  crates declare `static RING: DamageRing = DamageRing::new(64);`
  and call `record` / `snapshot` / `pushes` / `peak`.
- **`ueforge::debug::ProcessSnapshot`**. Bundles the five
  system-metric JSON fields (counters, process memory, CPU,
  threads, GObjects population, regions) behind a single
  `collect(counters_json, top_classes)` call.
- **`ueforge::debug::dispatch_standard_op`**. Single function
  handling 8 of the 12 standard ops (`skill_*`, `reload_slot`,
  `set_skill_points`, `walk_class`, `class_outer_samples`,
  `sample_thread_modules`). Returns `Option<Result<Json,String>>`;
  game crates fall through to their own ops on `None`.
- **`ueforge::debug::enqueue_pe`**. Generic "queue a closure on
  a DrainSite with timeout + custom hint" wrapper.

g2rpg's `debug.rs` shrank 896 -> 786 lines (-110). The remaining
weight is genuinely game-specific: HcFields/CmcFields/AscFields
view types + Maine class-name CDO collectors + simulate_*
executors with their G2-specific UFunction parm structs.

### kill_hook split + `DamageInfoLayout` lift

Second pass on `kill_hook.rs`. The 604-line file mixed three
concerns. Kill-credit dispatch, diagnostic damage tracing, and
impact-resistance reversal of environmental damage. Split into
three focused modules:

- `rpg/kill_hook.rs` (227 lines). Kill-credit core only:
  install + on_event + KillerKind classifier + award_kill.
- `rpg/damage_trace.rs` (168 lines). Per-fn parm decoder,
  damage_ring observer, `LastDamageInfo` log dump.
- `rpg/impact_resistance.rs` (79 lines). Environmental-damage
  detection + post-application reversal.

Universal pieces lifted to ueforge:

- **`ueforge::ue::damage_info::DamageInfoLayout`**. Per-game
  offset config for the `FDamageInfo` struct (instigator,
  source, damage-type class, damage flags). Methods fold the
  weak-ptr resolve + UClass cast into one call. The shape is
  universal across UE5 RPGs; only the offsets differ.
- **`ueforge::ue::actor::is_outer_named`**. Player-filter
  shorthand (`this.outer().full_name().contains(needle)`).
- **`ueforge::ue::actor::outer_class_name`**. Common log-line
  builder.

g2rpg's kill_hook can now point at the next UE5 game and the
authors clone-and-tweak only the killing-blow parm offset
(`MULTICAST_DAMAGE_FLAGS_OFFSET`), the player-outer string, the
creature-class string, and the per-game `DamageInfoLayout`. Every
other line is universal.

## 2026-05-10 (hardening + ops extraction)

### Kovarex P0 + P1 hardening shipped

All five P0s + four of six P1s closed:

- **`SlotStore::save -> io::Result<()>` + fsync.** Save returns
  `Result`; tracker logs + caches via `last_error()` accessor for
  snapshot surfaces. Atomic save now opens the temp explicitly,
  writes, calls `sync_all()` (durability across power loss),
  then renames. 4 unit tests on save/load/round-trip + parent
  creation.
- **`SlotStore` slot-path validation.** Reject empty / leading
  dot / path separators (`/`, `\`, `:`, `\0`) in slot keys;
  invalid keys route to a sentinel `__invalid__.json` so
  malformed input is visible. Closes the kovarex P1 path-
  injection vector alongside the P0 fixes.
- **`SlotPoller::Handle` + shutdown.** `SlotPoller::spawn`
  returns `PollerHandle` with `stop()` flipping an
  `Arc<AtomicBool>`. Worker is named `ueforge/rpg/slot-poller`
  via `thread::Builder::name` (visible to debuggers).
  `bbp_on_shutdown` now calls `world_loader::shutdown()` so
  the worker doesn't outlive an unloaded DLL on hot-reload.
- **`SlotPoller` panic visibility.** Each tick wraps
  resolve / activate / deactivate in `catch_unwind`. Panics
  are counted (`PollerHandle::panic_count`), the most recent
  payload exposed via `last_panic()`, the worker keeps running.
  bbp surfaces both via `world_loader::panic_count()` /
  `last_panic()`.
- **Spend/refund transactional with persistence.** Removed
  `pub fn spend` / `pub fn refund` from `SkillsState`. The
  only path now is `Tracker<A>::{spend,refund}_skill_points`,
  which mutates state and calls `store.save()` under the
  same lock.
- **`DisabledSkills` -> `ArcSwap`.** Reads are lock-free now;
  writers clone-modify-publish through a writer mutex. ImGui
  per-row + apply per-CDO calls stay on the fast path even
  with frequent toggles. 4 unit tests.
- **`Curve::level_for_xp` upper guard.** Added
  `MAX_LEVEL_LIMIT = 1024` const + `assert!` in `Curve::new`.
  10K-level mistakes become const-eval failures (in `static`
  context) or runtime panics (in dynamic).
- **`schema_version: u32` on `SkillsState`.** Free insurance
  for future migrations. Older save files (which lack the
  field) load as v1 via `#[serde(default)]`.

### `ueforge::rpg::ops` extraction

Five generic op handlers lifted out of bbp's
`grounded2-rpg/src/debug.rs` into the framework:

- `skill_toggle(tracker, disabled, args)`. Flips disabled
  flag + reapplies.
- `skill_spend(tracker, args)`. Spend N points; returns
  `{ id, requested, spent }`.
- `skill_refund(tracker, args)`. Refund N points; returns
  `{ id, requested, refunded }`.
- `reload_slot(tracker)`. Reapply every catalog skill.
- `set_skill_points(tracker, args)`. Debug-grant N points.

Plus `OP_NAMES: &[&str]` for dispatchers that want to
advertise the full op list. bbp's debug.rs lost ~80 LoC of
op boilerplate; any future RPG mod gets the five canonical
ops verbatim for free.

Test count: 56 ueforge unit tests passing (was 45 at the
start of this hardening session).

## 2026-05-10 (post-framework cleanup)

### Repo hygiene + tail-end framework lifts

Final 2026-05-10 cleanups after the major dedup wave. Mostly
file-tree shape + the last few extractions. Each commit is
small but worth a chronological line.

#### Dedup waves not previously listed

Three more waves landed on 2026-05-10 after the "ueforge as full
UE-mod framework" entry below was written:

- **`ueforge::pe_queue::DrainSite`** -- `Queue` paired with the
  standard performance-counter quad (drain calls, drained cmds,
  peak depth, time_ns spent). One static replaces the bbp
  pattern of a static `Queue` + four hand-declared `counter!`
  statics + a 20-LoC `drain_pending` wrapper. Surfaced via
  `drain_calls()` / `drained_cmds()` / `peak()` / `time_ns()`.
  Reference: [`../ueforge/docs/pe-queue.md`](../ueforge/docs/pe-queue.md).
  Commit `0a4aa32`.
- **`ueforge::ue::core_types`**. POD `#[repr(C)]` mirrors of
  stable UE5 layouts: `FGuid` (16 bytes), `FWeakObjectPtr` (8
  bytes), `FDataTableRowHandle` (UDataTable* + FName u64),
  `EStatusEffectValueType` enum (None/Add/Multiply). Six unit
  tests including size invariants. bbp's local duplicates of
  `FGuid` (in save_slot.rs) and `FWeakObjectPtr` (in
  kill_hook.rs) deleted; both consume `ueforge::ue::*`.
  `FDataTableRowHandle` + `EStatusEffectValueType` ready for the
  pending status-effect migration. Commit `c2b6776`.
- **`ueforge::rpg::SlotKeyResolver`**. Generic save-slot key
  extractor. Configure once with `(class_name, guid_offset)`;
  static `resolve()` walks GObjects for first instance of the
  class, reads `FGuid` at offset, formats as filename. Plug
  into `SlotPoller::spawn` directly. bbp's `save_slot.rs`
  shrunk to a 5-line wrapper. 2 unit tests. Commit `0997f25`.

#### Repo hygiene

- **Per-crate docs/ folders.** Bbp-specific deep dives moved to
  `grounded2-rpg/docs/` (damage, inventory, rpg, performance,
  features, engine, testing, ongoing, port history). ueforge
  doctrine + 12 subsystem reference docs land in
  `ueforge/docs/`. Workspace `docs/` retains only cross-cutting
  files (`README.md`, `todo.md`, `changelog.md`). Skill at
  `~/.claude/skills/grounded2-rpg/SKILL.md` updated to match.
  Commits `b1dc6a7`, `e041e52`.
- **`better-backpack` -> `grounded2-rpg`** rename. Crate name,
  workspace dir, `MOD_INFO.name` (`Grounded2RPG`), log file
  (`grounded2_rpg.log`), mod folder, deploy zip prefix all
  changed. ~93 files touched. Commit `e041e52`.
- **`outworld-station/tweaks/` -> `outworld-station-tweaks/`**
  flattened to match `grounded2-rpg`. Crate renamed; mod
  folder is now `OutworldStationTweaks`; log
  `outworld_station_tweaks.log`. Commit `b0cedb6`.
- **`archive/` deleted.** ~3300 lines of dead C++ winhttp
  proxy code + the standalone DLL injector (~700 LoC, dead
  since UE4SS handles loading). `inspection-guide.md` rescued
  to `grounded2-rpg/docs/inspection.md`. Still useful as
  generic UE5 mod-inspection methodology + worked examples.
  Commits `a1e0be6`, `f222941`.
- **`scripts/*.py` deleted.** Two Python uasset inspection
  tools (`dump_strings.py`, `read_property.py`) ported to
  Rust as `ueforge::uasset` lib + `dump-strings` /
  `read-property` `[[bin]]` targets. Repo is now Rust-only
  except for the irreducible UE4SS / ImGui C++ shim. Commit
  `4866a40`.
- **ImGui as git submodule.** `ueforge/cpp/imgui/` (~55K LoC
  of vendored Dear ImGui v1.92.1) replaced with a git
  submodule pinned to upstream `ocornut/imgui` tag `v1.92.1`.
  First-party C++ in the repo drops to 502 LoC (the shim).
  `build.rs` panics with a clear fix message if the submodule
  isn't initialized. Commit `d7c91b0`.
- **`ueforge-deploy` merged into `ueforge`** as a `[[bin]]`
  target alongside `dump-strings` / `read-property`. Workspace
  member count: 5 -> 4. `cargo deploy` alias updated to
  `run -p ueforge --bin ueforge-deploy --release --quiet --`.
  Commit `a79da15`.

#### Native (C++) surface accounting

Post-cleanup totals:
- First-party C++ in the repo: **502 lines** (the shim:
  `ueforge_shim.cpp`, `ueforge_ui.cpp`, `ueforge_cppusermodbase.hpp`,
  `ueforge_imgui_bridge.hpp`).
- Vendored ImGui: not in repo (submodule).
- Doctrine: [`../ueforge/docs/native.md`](../ueforge/docs/native.md)
 . "what C++ is in this repo, why each piece is irreducible,
  and what stays in Rust."

## 2026-05-10 (final)

### ueforge as full UE-mod framework

Continued extraction past the initial dedup wave. ueforge now ships
the complete set of systems every UE4SS Rust mod needs.

#### New ueforge surface (continued)

- **`ueforge::ue::ClassRef::for_each_matching` /
  `for_each_cdo_subclass` / `for_each_cdo_matching` /
  `for_each_any`**. The full walker family. Predicate-filtered
  variants for "live pawns whose full_name contains substring";
  CDO variants for subclass-aware walks; `for_each_any` for
  singleton-style data assets where the CDO IS the real data.
- **`ueforge::ue::ClassRef::new_dynamic(name)`**. Runtime
  constructor for the rare cold-path case where the class name
  is computed (catalog-row apply, debug-op handler).
- **`ueforge::ue::TypedField::deref`** specialized for
  `TypedField<*mut UObject>`. Typed component-pointer follow.
  Closes the `obj.field_ptr(off).cast::<*mut UObject>().read_unaligned()`
  pattern.
- **`ueforge::ring::EventRing<T>`** -- `Ring<T>` paired with
  built-in push counter + peak high-watermark. `record(item)`
  bumps both atomics, then pushes. Eliminates the need to pair
  every event ring with two separately declared `counter!`
  statics.
- **`ueforge::ue::PlayerRef`**. Canonical "find the player"
  surface for any UE5 mod. `(base_class, Option<bp_filter>)`
  config; static methods for `for_each_cdo`, `for_each_live`,
  `with_first_live`, `with_first_cdo`.

#### Doctrine docs

- **`../ueforge/docs/PERFORMANCE.md`**. Hot-path discipline (zero
  allocs, bail early, no mutexes on empty path, bounded
  everything, install-time heavy lifting, counter every hot
  path); memory leak vectors with their named ueforge fixes;
  consume-don't-reinvent table; consumer responsibilities.
- **`../ueforge/docs/RESEARCH.md`**. TDD investigation methodology
  for UE5 games: research-is-code rule, the seven-step TDD loop,
  five probe types in leverage order, doctrines on status
  effects / data tables / damage paths / instigator resolution
  / install-vs-hook, snapshot-not-log validation, failure-
  injection-test rule, research notebook shape.

After this session ueforge is **the** framework crate. g2rpg
contains only the G2-specific content: offsets, SkillEffect
variants, CATALOG content, GameApplier dispatch arms,
format_effect text, kill_hook / fall_hook / inv_hook trampolines,
parm structs, snapshot/op dispatcher. Everything else is ueforge.

## 2026-05-10 (later)

### Major dedup wave: g2rpg -> ueforge

Six commits in one session promoted the bulk of g2rpg's
infrastructure into ueforge. ueforge is now the framework crate
that gives every UE4SS Rust mod its load-bearing systems; g2rpg
owns only the G2-specific content (offsets, skill effect
variants, catalog rows, hooks, parm structs).

#### New ueforge surface

- **`ueforge::ue::ClassRef`**. Typed cached UClass handle.
  `const`-constructible static; lazy resolve on first `get()`;
  `cdo()`, `find_function()`, `with_first_instance()`,
  `for_each_instance()`, `find_instance()` helpers.
- **`ueforge::function_table!` decl-macro**. Struct-of-`usize`
  table of UFunction-pointer-identity slots with `install(&UClass)
  -> Result<Self, &'static str>` and `required` / `optional`
  field kinds.
- **`ueforge::ue::TypedField<T>`** -- `const`-constructible
  `(offset, T)` pair with `read(obj)`, `write(obj, v)`,
  `ptr(obj)`. Couples offset and type at declaration so accidental
  mismatches become `TypedField<f32>` vs `TypedField<u32>` type
  errors, not runtime corruption.
- **`ueforge::rpg::VanillaCache<K, V>`**. Per-key vanilla
  baseline cache. `get_or_init(k, v)` returns the captured
  baseline forever (first-write wins); `set_if_unset`, `get`,
  `clear`, `snapshot`. `parking_lot::Mutex` internally.
- **`ueforge::counter_json!` decl-macro**. Pairs of
  `(static_ident => "json_key")` collapsed into a
  `serde_json::Value::Object` with `load(Relaxed)` per counter.
- **`ueforge::hook::install_with_backoff(name, RetryPolicy, fn)`**
 . Generic exponential-backoff retry around any
  `FnMut() -> Result<H, &'static str>`. `RetryPolicy::default_install()`
  ships with g2rpg's battle-tested 500ms/5s/10min tuning.
- **`ueforge::worker::spawn(name, FnOnce)`**. Named worker
  thread (Win32 `SetThreadDescription` via `Builder::name`) with
  `catch_unwind` + logged panic payload. Closes the kovarex P1
  "unnamed worker thread" + "silent panic swallow" findings.
- **`ueforge::hook::LazyFunctionPtr`**. Lazily-cached
  `&UFunction` for hot-path identity dispatch. Warm path: 1
  atomic load + 1 branch. Cold path: 1 FName resolve, cache,
  never re-taken.
- **`ueforge::hook::ProcessEventHook::install_many`**.
  multi-class install with skip-on-not-loaded log lines.

#### RPG framework (Phase 3 wave 2)

The whole RPG / level-up system became framework code:

- **`ueforge::rpg::Skill<E>`** + **`find_skill`**. Generic
  catalog row parameterized on the game's effect enum.
- **`ueforge::rpg::RpgApplier`** trait. The seam where
  ueforge's state/persistence layer meets the game's apply
  dispatch. Methods: `apply_skill(state, skill)`, `apply_all`,
  `format_effect(skill, level)`.
- **`ueforge::rpg::Tracker<A: RpgApplier>`**. Owns slot
  binding, in-memory state, Applier instance, persistence.
  Drives spend/refund/record_xp/reapply transactionally with
  disk save. Returns `XpResult` from `record_xp` so the caller
  can log "LEVEL UP!" feedback.
- **`ueforge::rpg::tab::render(tracker, ToggleFns)`**. ImGui
  template: header (level + XP bar + skill points), catalog
  rows (+1/+10/-1/-10/optional on-toggle), debug footer
  (+5 / +50 skill points). Game crates supply only the
  `format_effect` closure (via Applier) and the toggle hooks.

#### g2rpg shrinkage

- `tracker.rs` reduced to a thin shim: a static
  `Tracker<GameApplier>` + ~10 wrapper fns that match the
  legacy API surface for kill_hook / world_loader / debug.rs.
- `tab.rs` reduced to a counter-bumping wrapper around
  `ueforge::rpg::tab::render`.
- `inv_hook.rs` lost ~80 LoC of UFunction-resolve boilerplate
  (now `function_table!`).
- `fall_hook.rs` lost ~30 LoC of multi-class install +
  manual UFunction cache (now `install_many` + `LazyFunctionPtr`).
- `lib.rs` lost ~50 LoC of `CreateThread` / retry-loop /
  `HOOK_RETRY_*` consts (now `worker::spawn` +
  `install_with_backoff`).
- `counters.rs::snapshot_json` collapsed to a single
  `counter_json!` invocation.
- `apply.rs` `VanillaTable` struct deleted (now
  `ueforge::rpg::VanillaCache`).

Net: ueforge gained ~2000 LoC of framework surface (+30 unit
tests, all passing); g2rpg lost ~500+ LoC of duplicated
infrastructure. Every crate builds clean release. In-game
smoke test pending.

## 2026-05-10

### Phase 3 (first wave). Ueforge::rpg generic framework

The RPG-mod-shaped pieces every UE game needs land in ueforge:

- `ueforge::rpg::xp::Curve { base, exponent, max_level }`. The
  classic `cumulative_xp_for_level` / `level_for_xp` math
  parameterized so any RPG mod plugs its own numbers.
- `ueforge::rpg::progress::sqrt_progress(level, max)`. Generous-
  early diminishing-returns curve.
- `ueforge::rpg::SkillsState`. On-disk schema (xp / level /
  skill_points / `skill_levels: BTreeMap<String, u32>`) with
  `spend()` / `refund()` / `level_of()` methods.
- `ueforge::rpg::SlotStore<S>`. Per-slot JSON persistence under
  `<DLL_dir>/<subdir>/<slot>.json`. Generic over persisted struct.
  Atomic temp+rename save.
- `ueforge::rpg::DisabledSkills`. Thread-safe toggle set for
  "disable a skill without refunding its points".
- `ueforge::rpg::SlotPoller::spawn(interval, resolve, on_activate,
  on_deactivate)` -- 1Hz worker that drives activate / deactivate
  transitions on a consumer-supplied resolver closure.

5 unit tests on the curve + progress math (xp round-trip, max-level
cap, sqrt endpoints, quarter-is-half). The framework's first unit
tests, closes the kovarex P2 "no unit tests on framework
primitives" item.

g2rpg migrations: `src/rpg/state.rs` deleted entirely; `tracker.rs`
keeps a `static STORE: SlotStore<SkillsState>` and routes load /
save / spend / refund through ueforge; `world_loader.rs` shrinks
from 95 LoC of CreateThread plumbing to 30 LoC routed through
`SlotPoller::spawn`; `xp.rs` shrinks to a 3-line curve constant +
the per-creature XP table (game-specific bestiary); `apply.rs`
DISABLED_SKILLS swap; `skills::level_progress` becomes a 1-liner
forwarder. g2rpg's `PlayerState` is gone -- `kill_count` and
`last_killed` (diagnostic-only) dropped per "free to redesign"
scope.

Phase 3 second wave (catalog generic, apply dispatcher, RpgTab
widget) is next session.

### Phase 2 small promotions

Three Phase 2 items land before the big RPG promotion:

- **`ueforge::ue::class_tweak::ClassFieldTweak<T>`**. Live-UObject
  sibling of `FieldTweak<T>` (which is DataTable-row scoped). Two
  closure shape: `filter(&UObject) -> bool` (cheap, runs before
  the vanilla read) and `transform(T) -> Option<T>` (`None` =
  skip writing this instance). Captures vanilla keyed per-instance
  so re-applies don't compound.
- **g2rpg `patch.rs` + `survival.rs` migrated to `ClassFieldTweak<T>`.**
  Hand-rolled GObjects walks deleted; both files are now driven
  by `static SLOTS / HUNGER / THIRST: ClassFieldTweak<...> = ...;`
  and a single `.apply()` call. Net -150 LoC from g2rpg.
- **`ueforge::hook::function_ptr` / `function_ptr_required`**.
  small helper that returns `*const UFunction as usize`. Pattern
  is: stash in an `AtomicUsize` at hook install, dispatch by
  pointer identity in the trampoline.
- **`Queue::drain` canonical-site doc.** The `g2rpg::kill_hook` PE
  trampoline drain pattern (empty-check, re-entrance guard, peak
  counter, time scope) is now in the rustdoc so the next mod
  doesn't have to reverse-engineer it.

### g2rpg consumes ueforge for ALL infra (Phase 1 dedup)

grounded2-rpg lost ~930 lines of duplicated framework. Every piece
of generic UE4SS / SDK / hook / log / settings / counter / ImGui /
build plumbing now lives in ueforge and is consumed (not mirrored)
by the mod.

Deleted from g2rpg:
- `cpp/shim.cpp` (357 LoC). CppUserModBase mirror + ImGui render lambda
- `src/rpg/ffi.rs` (340 LoC). C-ABI bridges that the shim called
- `src/log.rs`. Duplicate of ueforge::log
- `src/sdk/`. Thin re-export shim of ueforge::ue
- `src/hook/`. Thin re-export shim of ueforge::hook
- `winhttp.def`. Legacy
- Custom `DllMain` + `grounded2_rpg_start` / `grounded2_rpg_stop` exports
- `DLL_HMODULE` static

Replaced with:
- `ueforge::ue4ss_mod!(MOD_INFO)`. One macro emits every extern "C"
  hook the shim invokes (factory, DllMain forwarding, tab dispatch)
- `src/rpg/tab.rs`. Rust ImGui render. Calls `tracker` / `skills`
  / `xp` directly; uses ImGui `##id` label suffix instead of
  PushID/PopID for unique button instances per skill row
- 1-line `build.rs`: `ueforge::build::CppShim::new().compile()`

Settings struct shape unchanged on disk (back-compat preserved); IO
routes through `ueforge::settings::Settings<T>`. The legacy parent-dir
fallback for `settings.json` was dropped per "free to redesign"
scope; if anyone has a pre-Rust install pattern, move the file next
to main.dll.

Cumulative shape after Phase 1: g2rpg consumes ueforge for SDK, hook
framework, log, settings IO, ImGui bindings, counter primitives,
ring buffer, HTTP server, op envelope, generic ops, tab
registration, DllMain, factory exports, C++ shim. Zero duplication.

What stays in g2rpg (correct game-side): `inv_hook`, `kill_hook`,
`fall_hook`, `survival`, `patch`, `parms`, `debug`, the `counters`
domain statics, and the entire `rpg/` subsystem (catalog content,
apply dispatcher, persistence layer). `rpg/` is the Phase 2 promotion
candidate. Shape is generic for any UE game with the right
`slot_key()` resolver, so it lands in `ueforge::rpg` next.

### ueforge hardening (kovarex review landed)

Three P0 + six P1 items shipped. Crash vectors closed, FString /
GObjects walks bounded, dev profile unwinds.

**P0:**
- `parking_lot::Mutex` everywhere. No more poison-then-abort hard
  crashes. `[profile.dev] panic = "unwind"` so research crashes
  leave a backtrace; release stays on abort for size + perf.
- Hot paths use `try_runtime` + soft fallback (empty name, dropped
  PE call + log) instead of `expect()` aborting if a hook fires
  before init_runtime sets the OnceLock.
- Address-validated freezes. `freeze { selector, offset }` resolves
  through `selector::resolve_generic` and re-resolves on staleness;
  `VirtualQuery` validates page protection on every write; 30
  consecutive failed validations stop the writer. Legacy
  `freeze { addr }` still works but has no recovery path.

**P1:**
- `NameResolver` caches FName u64 -> String. Caps the FString leak
  at one buffer per unique FName (was: per call). Bounded by the
  game's name pool (~50K) instead of unbounded.
- `find_class_fast` caches by name. Selector resolution drops from
  O(GObjects walk). ~150K objects on OWS. To O(1) on the warm
  path.
- `UClass::cached_native_properties` returns `Arc<[NativeProperty]>`;
  `inspect_address` allocates zero per click after the first lookup.
- `ops::{read_bytes,write_bytes}` clamp `offset+length` to
  `class.properties_size()` when the class is known. Selectors that
  point at a UObject can no longer read/write past the instance.
- `arc_swap::ArcSwap` replaces hand-rolled `AtomicPtr<&'static [...]>`
  + `Box::leak` snapshot in PE hooks. No leak per install/drop.
- `tmap::MAX_LINEAR_SCAN` 8192 -> 65536 with one-line truncation log.
- `Api::try_op` + `perf::try_open` Result-returning variants
  alongside the panicking convenience wrappers.
- `scan_memory` builds JSON sample before taking sessions lock so
  multi-second scans don't block other ops.

### Three-way feature audit added

`../ueforge/README.md` ships a 62-row matrix mapping every feature
across ueforge, grounded2-rpg, and ows-tweaks. Each cell is
`live here / consumes ueforge / duplicates (delete) / should be
promoted / N/A`. Verdict column names the migration item.
Phase 1 status now marks 30+ rows done.

### Property walker on inspect_address

`ueforge::inspect_address` now resolves field names via the
UClass `ChildProperties` chain. Click an address in the Scanner
tab -> "DataTable::RowMap on DT_Materials" instead of
"DT_Materials @ +0x30 (size 0x60)". Walks the super-class chain.
Adds `ffield`, `fproperty`, and `ustruct::CHILD_PROPERTIES`
constants to `ue::offsets` (UE 5.4 verified; 5.5/5.6 may shift
`OFFSET_INTERNAL`. Track via the offsets-versioning P2 item).

## 2026-05-09 (evening)

### impact_resistance bug fix landed: ApplyDamageFromInfo intercept

Replaced the binary `RequiredDamageTypeFlags = 0xFFFFFFFF` mask
write with a Runtime effect that intercepts
`UHealthComponent::ApplyDamageFromInfo` on the player HC,
identifies environmental damage by
`FDamageInfo.DamageType` class name containing "Environmental",
and scales the `Damage` parm by `(1 - reduction * progress)`
before forwarding.

Why the change: the binary mask blocked bandages (and all
healing) by gating `type_flags=0` events at the native
ApplyDamage level. Bandage HoT applies a `Type=Health (24)`
status effect whose tick goes through that gate and was being
rejected. Research-as-code (`tests/explore_bandage_path.rs`,
`tests/explore_bandage_status_effects.rs`) characterized the
mechanism end-to-end before the fix landed.

The new mechanism uses the same shape as fall_resistance's
velocity-stomp on OnLanded: intercept-and-scale at the PE
surface, gated by a discriminator (DamageType class for
impact_resistance, "is fall" for fall_resistance).

Side-effects:
- Bandages now heal regardless of impact_resistance level
- Creature combat damage is untouched (different DamageType)
- Fall damage is untouched (different surface, handled by
  fall_resistance)
- Per-level sqrt scaling instead of binary all-or-nothing

## 2026-05-09

### Discovered: impact_resistance blocks bandages

Enabling `impact_resistance` at any level prevents bandages and
other healing items from working. Confirmed in-game by binary
isolation (only impact_resistance enabled -> repro; toggled off ->
heals fine). Mechanism: bandages route healing through
`ApplyDamage` with negative damage and `type_flags = 0`. The
`RequiredDamageTypeFlags = 0xFFFFFFFF` mask we set rejects any
event with type_flags=0, so heal events fail the gate before the
HP decrement. Detail in [`damage.md`](damage.md) "Critical
regression: bandages / healing items blocked".

Workaround until the canonical fix lands: keep impact_resistance
toggled off when healing is needed. The status-effect migration
in [`todo.md`](todo.md) replaces the binary mask with a
type-filtered DamageReductionMultiplier that won't touch healing.

### `PlayerHealthCompU32Mask` learned to disable cleanly

Previously the variant only wrote the mask at level > 0 and never
restored vanilla, so refunding the skill or toggling it off left
the mask set (mid-session and beyond). Apply step now captures the
vanilla mask on first sight, runs at level 0 / disabled too, and
writes the captured vanilla back when the skill is inactive. This
is what makes the toggle actually unblock bandages.

### Avoided ProcessEvent deadlock by deleting dead UFunction call

Toggling fall_resistance off used to freeze the game. Root cause:
`apply_skill` for fall_resistance was invoking
`USurvivalModeManagerComponent::UpdateCustomSettings` via
ProcessEvent. That UFunction triggers replication on the Net-flagged
`CustomSettings` field; calling it mid-session from any non-game
thread (ImGui callback, world_loader poller) hangs on the
replication marker.

Reading the SDK + the original commit (`6ad1df2`), the call was
dead code. The changelog explicitly notes "Player still takes
fall damage" even with the UFunction firing, and the actual
mitigation that works is the velocity-stomp on `Velocity.Z` in
`fall_hook.rs` plus the `RequiredDamageTypeFlags` mask. Native
fall-damage code never reads the FallDamageMultiplier fields the
UFunction was meant to invalidate. Deleted the call and its helper
entirely; the GMS / SMMC field writes remain (harmless).

## 2026-05-08

### Catalog +3 skills (untested in-game)

Three new skills shipped, all using two new `SkillEffect` shapes.
Pending in-game witness.

- **Max Health** -- `PlayerHealthCompAdditive` (new variant). Captures
  the vanilla baseline at `UHealthComponent.MaxHealth` (+0x328) on
  first apply, then writes `vanilla + max_bonus * progress` on
  player CDOs and the live pawn. Additive so HP stacks rather than
  scaling, +200 HP at level 100.
- **Health Regen** -- `GlobalDataMult` (new variant). Walks every
  `UGlobalCombatData` instance and writes per-offset multipliers:
  CombatRegenTickPercentage (0x010C, exponent +1.0) scales up,
  CombatRegenTickRate (0x0110, exponent -1.0) scales down (more
  frequent ticks). Combat-regen delay (0x0108) intentionally
  untouched to preserve the post-combat feel. +500% / 6x at level
  100.
- **Leap Distance**. Existing `PlayerMovementMult` over three
  CharMovementComponent fields: AirControl (0x02C0),
  AirControlBoostMultiplier (0x02C4), AirControlBoostVelocityThreshold
  (0x02C8). Lets the player keep accelerating in their input
  direction through the arc. A real "leap" feel rather than a
  taller jump. +500% at level 100.

### ImGui tab: refund + per-skill toggle

- `-1` / `-10` refund buttons next to the `+1` / `+10` spend
  buttons. Decrement the level, credit the points back, run the
  apply step, save. Disabled at level 0. Caveat: refund-to-0 on
  `PlayerHealthCompU32Mask` (Impact Damage Resistance) needs a
  reload to take effect because the apply step early-returns at
  level 0 and there is no vanilla mask captured.
- `on` checkbox per row. Disabling treats the skill as level 0
  without refunding the points. Drops a buff (e.g. Leap Distance)
  on demand without losing progress. Process-global, not persisted
  (cheap to reapply on launch). Toggling fires `apply_one`.

New FFI: `bbp_rpg_refund`, `bbp_rpg_refund_many`,
`bbp_rpg_is_skill_enabled`, `bbp_rpg_set_skill_enabled`.

## 2026-05-05 (single big day)

### Impact Damage Resistance (binary)

Landed via `apply.rs` writing
`UHealthComponent.RequiredDamageTypeFlags = 0xFFFFFFFF` (+0x00FC,
uint32) on player CDOs and the live pawn at any
`impact_resistance` level > 0. Native ApplyDamage gate consults
the field; incoming damage with `type_flags = 0` (fall,
environmental, hazard zones) fails the bitwise check and is
rejected before `CurrentDamage` is written. Creature attacks
carry non-zero flags and pass through. Validated in-game: rock
collision multicasts report `damage=0.00`, impact-trace POST line
silent (no `CurrentDamage` change). Same field-write pattern as
Armor's `BaseDamageReduction` write, just on a different gate.

The gate is **binary** (level 1 = full immunity, identical to
level 100). Status-effect-backed migration tracked below.

### Vanilla status-effect data table identified

`fall_hook.rs` extended with a probe that walks the player's
`UStatusEffectComponent.StatusEffects` array and logs each effect's
row handle. Result: every active status effect in the game flows
through one data table:

```
DataTable: /Game/Blueprints/Attacks/Table_StatusEffects.Table_StatusEffects
```

Sample active rows (mid-game player): `PlayerUpgradeHealth1`,
`WeaponClub`, `RogueFinisherCriticals`, `AntRedStaminaAttack`,
`FighterFinisherStun`, `MaxHealthSmall`, `PerkSpearTier1`,
`PerkSpearThrowAttackUpTier1`. Naming convention is
`<Source><Effect><Tier>`. Detail in `../grounded2-rpg/docs/damage.md` "Vanilla data
table identified".

This finalizes the migration plan: resolve the table (follow any
existing row handle), enumerate rows, mutate or inject one per
skill, AddEffect via `process_event`. Implementation step is
concrete now.

### Status Effect combine semantics decoded

Extended the sfx-probe to also call
`UUserInterfaceStatics::GetStatusEffectValueType(StatType)` for each
probed stat. `EStatusEffectValueType` enum is two-state plus None:
`Add=1`, `Multiply=2`. Per-stat semantics observed in-game:

- `FallDamage`, `DamageReduction`, `AttackDamage` -> `mul` (vanilla
  baseline 1.0; effects multiply).
- `LifeSteal`, `CriticalHitChance`, `CriticalDamage`, `ReflectDamage`,
  `MaxHealth`, `DamageReductionMultiplier` -> `add` (vanilla baseline
  0.0; effects sum).

Per-skill write formulas now fully determined; full table in
`damage.md` "Stat semantics table". This finishes the validation
phase of the A1 migration; next is picking an implementation path
(mutate existing row vs inject new row) and building the apply
step.

### Status Effect system validated (probe shipped, in-game readout)

`fall_hook.rs` now ships a probe (gated on `impact_resistance > 0`)
that calls `UStatusEffectComponent::GetValueForStat(StatType,
false)` on the player's component for damage-relevant stat types.
First in-game readout on a mid-game player:

```
FallDamage(14)=1.000  DamageReduction(29)=1.000  AttackDamage(23)=1.210
CriticalHitChance(31)=0.060  MaxHealth(5)=30.000  LifeSteal(38)=0.000
```

Confirmed:
1. The status-effect system is live and queryable.
2. Vanilla equipped gear / perks already populate it (the `+21%`
   attack and `+30` max health values come from existing items).
3. Stat semantics differ (multiplier vs additive vs probability),
   queryable via `USurvivalGameplayStatics::GetStatusEffectValueType`.

This validates the migration target. Next step is to probe the
combine semantic per stat, then implement the apply step using
one of the four implementation paths in `damage.md`.

### Status Effect system identified (canonical extension surface)

SDK review of `UStatusEffectComponent` on `ASurvivalCharacter` at
+0x1378: this is the proper Grounded 2 surface for proportional,
per-stat / per-damage-type modifiers. Native damage code calls
`GetValueForStatForDamageTypeFlags(StatType, Flags)` on every
damage event and uses the float result as a multiplier.
`EStatusEffectType` enum has matching values for nearly every
skill we plan: `FallDamage=14`, `DamageReductionMultiplier=30`,
`LifeSteal=38`, `ReflectDamage=37`, `AttackDamage=23`,
`CriticalHitChance=31`, `CriticalDamage=62`, `MaxHealth=5`, etc.

Current implementations (Armor's `BaseDamageReduction`, Impact's
`RequiredDamageTypeFlags`, Fall's velocity-stomp, Lifesteal's
`Runtime` no-op) are interim. Migration to status-effect-backed
skills gives sqrt-curve scaling, native type filtering, and
stacking with vanilla items / perks. Plan in [`todo.md`](todo.md);
full pipeline reference in [`damage.md`](damage.md).

### Fall Damage Resistance

Landed via velocity-stomp in `fall_hook.rs`'s `OnLanded` PE hook on
the player BP class. See [`damage.md`](damage.md) for the full
pipeline reference.

### Movement skill fix verified

Move Speed was initially wired only through generic UE movement caps and
player CDO writes, which was not enough for Grounded 2. The fix now
also patches Grounded-specific `UMaineCharMovementComponent` fields:

- `CustomGroundSpeedMultiplier` (+0x1198)
- `CustomFlySpeedMultiplier` (+0x119C)
- `CustomSwimSpeedMultiplier` (+0x11A0)
- `MaxSwimSprintSpeed` (+0x1164)

Movement writes are also mirrored onto live player pawns, not just
CDOs, so the effect can be validated immediately in-session. User
retest confirmed the run-speed skill now works.

### Skill registry refactor

`skills.rs` is now the single source of truth. Each skill is one
row in `CATALOG` carrying id, display_name, max_level, and a
`SkillEffect` variant. `apply.rs` is one function `apply_skill`
that pattern-matches on the effect. Adding a new skill of an
existing shape is one CATALOG row, no other code changes. Adding
a new shape is one variant + match arms in apply and format.
Catalog at 9 skills, ready to grow to 25+.

### Vocabulary and curve

Hard rename `rank -> level` everywhere. Schema field
`skill_ranks -> skill_levels`. Every skill now caps at level 100
with a `sqrt(level/100)` diminishing-returns curve.

### Combat + movement skills

- Attack Damage (max +300%): `ASurvivalCharacter.CustomDamageMultiplier` at +0x12B8.
- Armor (max -50% damage taken): `UHealthComponent.BaseDamageReduction` at +0xEC, via the player CDO's HealthComponent pointer.
- Move Speed (max +300% walk + sprint + swim): `MaxWalkSpeed` (+0x288), `MaxSprintSpeed` (+0x10EC), `MaxSwimSpeed` (+0x290), `MaxSwimSprintSpeed` (+0x1164), `CustomGroundSpeedMultiplier` (+0x1198), `CustomSwimSpeedMultiplier` (+0x11A0).
- Jump Height (max +300%): `JumpZVelocity` at +0x1B8.
- Glide Speed (max +300%): `MaxFlySpeed` (+0x294) plus `CustomFlySpeedMultiplier` (+0x119C).
- Lifesteal (max +90% of damage dealt): catalog entry only;
  runtime kill_hook integration was queued and pivoted to the
  registry refactor.

All movement skills capture vanilla baselines via `OnceLock`s on
first apply so re-apply doesn't compound.

### apply optimization

`apply_one(state, settings, skill_id)` runs just one skill. A
`+1` click in the tab now writes one CDO + produces one log
line, not five.

### ImGui tab

UE4SS `register_tab` for the "RPG" tab. Shows level, XP bar,
skills with rank/effect/+1 buttons. Vendored upstream
ocornut/imgui v1.92.1 (matches UE4SS) as
`grounded2-rpg/cpp/imgui/`. Forward-declared
`UE4SSProgram::get_current_imgui_context` and
`get_current_imgui_allocator_functions` with `RC_UE4SS_API`
(dllimport via UE4SS.lib). Symbol mangling gotcha: `register_tab`
is `protected` on the real CppUserModBase. MSVC mangling encodes
access (Q=public, I=protected). First attempt with public failed
LNK2019; moved to a protected section of the mirror.

C-ABI in `rpg/ffi.rs` (8 functions wrapped in `catch_unwind` so
panics can't take down UE4SS's render thread). Lambda reads
state via getters and posts spends via `bbp_rpg_spend`.

UI also shows current effect + next-level preview per skill, e.g.
`Backpack level 5 / 100  +30 slots  (next: +33 slots)`. Debug
footer with `+5` / `+50` skill-point grant buttons for testing.

### Eager state load on world entry

`rpg/world_loader.rs`: 1Hz poller that watches
`save_slot::current_slot_key()` and drives tracker transitions
(`None -> Some(s)` activate, `Some(a) -> Some(b)` save-swap,
`Some -> None` deactivate). Replaces the lazy "load on first
kill" shortcut from Spike B.

### Skill catalog + apply step + settings layering

settings.json defines the BASE values for each stat. RPG skills
layer on top:

- backpack: target = `settings.slot_count + skill_bonus`.
- survival drain: target = `vanilla * settings_mult * skill_mult`.

Defaults flipped to vanilla (`slot_count=40`,
`hunger/thirst_mult=1.0`); user overrides settings.json for a
starter buff. Skill levels then layer.

apply.rs captures vanilla survival rates at init (before the
init-time survival::run modifies them) so re-apply uses a fixed
baseline.

### Buggy kill attribution

Three buckets in `kill_hook.rs::classify`:

- Player: instigator class chain contains "PlayerController".
- Buggy: instigator (or its possessed Pawn at +0x308) class
  chain contains "Buggy".
- Other: enemy AIC; logged + dropped, no kill_count bump.

Confirmed in-game: PLAYER on Grub kills, BUGGY on Spiderling
killed by `AIC_AntSoldier_Augusta_Buggy_C`. Single-player /
host case works; co-op ownership disambiguation deferred.

### XP / level math

`rpg/xp.rs`: cumulative XP curve `100 * N^1.8`, level cap 50,
per-creature XP lookup table (~20 species placeholders).
PlayerState schema bumped to `(xp, level, skill_points,
skill_levels)`.

### Spike A: kill detection

Hook on `Maine.HealthComponent` ProcessEvent slot. Match on
`MulticastHandleEffectsWithDamageFlags`. Read DamageFlags at
+0x1C in the parms struct, mask with `EDamageInfoFlags::KillingBlow = 2`.
On match, read `LastDamageInfo.InstigatorController` from
HealthComponent +0x3B0 (inside FDamageInfo at +0x20). Filter to
ASurvivalCreature subclasses to exclude buildings / props.

#### Attempt 1 (failed)

First attempt hooked the `Kill` UFunction directly. Did not fire:
`Kill` is `Final|Native`, the engine bypasses ProcessEvent on
the internal damage path. Diagnostic logging of all PE calls on
HealthComponent showed only `OnStatusEffectChanged`,
`MulticastHandleEffectsWithDamageFlags`, `OnCombatChanged` during
a Weevil kill.

#### Investigation

`OnDeath` multicast delegate's BP-bound handlers fire via PE on
each enemy's vtable, not HealthComponent's, so a per-vtable hook
on HealthComponent can't see them. Two paths considered: (A)
read `DamageFlags` from `MulticastHandleEffectsWithDamageFlags`
which DOES fire via PE on HealthComponent and includes a
KillingBlow bit, or (B) global ProcessEvent hook via UE4SS's
`RegisterProcessEventPreCallback`. SDK grep found
`EDamageInfoFlags::KillingBlow = 2`, which validated path A.
Implemented in ~10 lines, no architectural change.

Path B retained on the future-infra TODO; it's the right call
for any future PE-only signal we'll need (level-up confirms,
item crafted, dialog events, etc.).

### Spike B: persistence

PlayerState in `<DLL_dir>/saves/<playthrough-guid>.json`. First
attempt used `USaveLoadManager.SaveInProgressSaveGameHeaderData`
(at +0x90), but that's null except during an active save
operation. Switched to `AInGameGameState.PlaythroughGuid` at
+0x32C; populated for the duration of the active session.

### RPG pivot

Direction shift from "configurable tweaks toolkit" (settings.json
flat sliders) to "Factorio-style RPG / level-up mod" (skills earn
points, points buy ranks, ranks change game state). Inspired by
the Factorio
[RPG System](https://mods.factorio.com/mod/RPGsystem) plus
[War3CS / War3FT](https://war3cs2.wiki.gg/) for Counter-Strike.

The "everything is a slider" model became the *output* of the
level system, not the user-facing knob. Settings becomes base
values; skills layer on top.

### UE4SS C++ mod load works

Loader-side bug fixed: our `CppUserModBase` mirror was missing
six virtuals vs the upstream header (`on_ui_init`, four post-1.0
`on_lua_*` overloads, `on_cpp_mods_loaded`). UE4SS dispatched
through later vtable slots into garbage and crashed the game
during init. Mirror now matches the header virtual list verbatim.
`on_cpp_mods_loaded` is NOT marked `RC_UE4SS_API` because
UE4SS.lib doesn't export it; using local inline body works.

## Earlier

The Rust port of the original C++ Grounded 2 - RPG System mod is
documented in [`rust-port.md`](rust-port.md). The pivot to a
UE4SS C++ mod (CPPMod) shape is documented in
[`ue4ss-port.md`](ue4ss-port.md).

The original C++ implementation is preserved at
[`../archive/winhttp-proxy/`](../archive/winhttp-proxy/) for
reference.
