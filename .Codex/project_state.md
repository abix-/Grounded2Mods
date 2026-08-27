# Modforge project state

## Current focus

Audit every Survivalist source function and extract every engine-independent or Unity-specific mechanism into Modforge or Unityforge without changing game behavior.

## Design goals

- Item definitions have one authority: `ItemDef` and `ItemRegistry`.
- Uniqueness is an `ItemDef` property, not a parallel definition type or registry.
- Per-save entered and holder state belongs to the item ledger in `modforge::item`.
- Survivalist retains entry rules, Unity bindings, prototype and inventory access, delivery, logging, and chronicle text.
- Upgrade levels, policy math, schema, and persistence have one engine-independent authority in `modforge::upgrade`.
- Survivalist retains upgrade applicability, menus, material consumption, Harmony effects, save-path selection, and game-object access.
- Mission lifecycle advancement, explicit stage transitions, timeout dispatch, collection removal, error routing, and guaranteed cleanup belong to `modforge::mission`.
- Mission stage data and all Unity checks, intents, outcomes, voting, judgment, chronicle text, logging, and concrete handle cleanup remain in Survivalist.
- Franchise tallying, strict-majority decisions, mean scores, and voter identity collection belong to `modforge::genome`.
- Trait, voter, and collective reinforcement fan-out belongs to `modforge::genome`.
- Survivalist retains voter eligibility, conscript exclusion, trait projections, thresholds, actions, outcome judgment, trait selection, reinforcement direction, and magnitude.
- Settlement survival rungs and classification from nutrition, population loss, and threat pressure belong to `modforge::survival`.
- Survivalist retains settlement threshold values, Unity reads, voting, and every response to the classified rung.
- Adaptive-pressure target selection, tier resolution, deterministic ring placement, global caps, per-target exclusion, tracking, and pruning belong to `modforge::storyteller`.
- Survivalist retains Unity target observations, threat checks, game handles, zombie spawning, liveness reads, movement commands, content values, logging, and chronicle text.
- Adaptive encounter place tracking, budget rolls, encounter composition, anchor and class selection, scatter placement, and successful-spawn caps belong to `modforge::storyteller`.
- MISERY retains Unreal progression and creature observation, enemy exclusions, tuning values, spawn execution, logging, and controls.
- Phenomenon region tracking, weighted selection, reward guarding, count and cluster rolls, placement requests, successful-spawn caps, and counters belong to `modforge::storyteller`.
- MISERY retains its phenomenon catalog, live-square and progression observation, Blueprint lookup, ground traces, Unreal spawning, logging, and controls.
- Vendor percentage pricing, provisional global assignment, commit-on-success, inventory mirroring, and special-offer rejection belong to `modforge::vendor`.
- MISERY retains vendor and item discovery, category and economy values, offer precedence, sewing-kit policy, and Unreal list mutation.
- Raw TArray capacity checks, engine-allocator growth, template cloning, appended slot writes, and count updates belong to `ueforge::ue::tarray`.
- MISERY retains vendor list offsets, item identifiers, price-array construction, stock and price byte patches, labels, and logging.
- Live non-CDO and transient-object lookup, checked raw UFunction parameter calls, zeroed parameter allocation, and game-thread live-object hook installation and completed-hook teardown belong to Ueforge.
- MISERY retains its autoload save checks, Blueprint names, parameter layouts, notice filtering, re-entry guard, dismissal function, diagnostics, and logs.
- The root README is a concise workspace map with capability tables; detailed framework and decompilation material stays in the owning crate documentation.
- Managed handle ownership, bridge-value decoding, managed-list traversal, Unity coordinate decoding, synchronous main-thread dispatch, and Rust/C# pointer and string plumbing belong to Unityforge.
- Survivalist retains its game class and field names, object selection, gameplay policy, Unity actions, content, logging, and presentation.
- Rectangular annex geometry and deterministic identity-based selection belong to Modforge; Survivalist retains construction and encounter policy plus game observation and execution.

## Last session summary

- Documented every function in `survivalist-mod/src` with a concise purpose and an explicit ownership boundary.
- Marked eleven functions as concrete extraction evidence instead of defending engine-independent or Unity-specific mechanics as game code.
- Recorded five coherent lifts in `docs/todo.md`: managed-object and collection helpers, existing main-thread dispatch adoption, Rust/C# boundary helpers, annex planning, and deterministic identity selection.
- Re-exported the shared mission stage from the courier module so existing bounty and threat status code can name it after the earlier mission extraction.
- Added no tests and made no behavior changes.
- Verified `k3sc cargo-lock check -p survivalist-mod` and `k3sc cargo-lock test -p survivalist-mod --lib`; both pass with the two existing unused genome-helper warnings and the library target has zero tests.

- Added the `unique` property to `ItemDef`.
- Extracted the existing entered set, holder ledger, schema version, lazy JSON restore, and temporary-file-then-rename persistence into `modforge::item::ItemLedger`.
- Migrated `survivalist-mod/src/unique.rs` to the item ledger while preserving its seed-keyed path, JSON field names, entry checks, holder announcements, and write-failure warning.
- Added no tests. Updated existing `ItemDef` test literals with `unique: false` so their behavior remains unchanged.
- Verified `k3sc cargo-lock test -p survivalist-mod --lib`: build passed and its zero-test library target passed.
- Verified `k3sc cargo-lock test -p modforge --lib`: build passed, 274 tests passed, and the existing item tests passed. The full target remains red on unrelated `input::tests::backend_parse_rejects_garbage`, where current input code accepts `l3` but the test expects rejection.
- Added `modforge::upgrade` for scoped entity levels, status aggregation, cost and skill requirements, diminishing returns, schema, and JSON persistence.
- Migrated the Survivalist C# upgrade effects to query the Modforge authority through native exports. Unityforge now notifies consumers whenever a Rust generation becomes active so the delegates rebind after initial load, story re-entry, hot reload, or rollback.
- Removed the duplicate C# level dictionaries and JSON persistence. Existing track applicability, menus, material consumption, effects, save path, JSON fields, and status shape remain in Survivalist.
- Added no tests and did not build, following the direct instruction for this extraction.
- Expanded `modforge::mission` with caller-defined multi-stage missions, explicit transitions, timeout callbacks, owned advancement, and one-stage observation missions.
- Migrated Survivalist courier to the existing go-and-return lifecycle and migrated settler, stranger, and robbery to the one-stage collection driver without moving game behavior into Modforge.
- Added no tests and did not build for the mission extraction, following the direct instruction.
- Expanded `modforge::genome` with a shared ballot accumulator and migrated the duplicated tallies in Survivalist survival, steal, trade, scavenge, murder, and robbery.
- Added no tests and did not build for the ballot extraction, following the direct instruction.
- Expanded `modforge::genome` with shared trait, voter, and collective reinforcement dispatch and migrated the duplicated reinforcement loops in Survivalist survival, steal, trade, scavenge, murder, and robbery.
- Preserved Survivalist's outcome judgment, trait choices, directions, magnitudes, Unity effects, logging, and chronicle text. Added no tests and did not build.
- Moved Survivalist's settlement survival rungs and pressure classification into `modforge::survival` while preserving its threshold values and all Unity-driven responses.
- Added no tests and did not build for the settlement survival extraction, following the direct instruction.
- Expanded `modforge::storyteller` with the complete engine-independent adaptive-pressure selection, tier, placement, and active-event lifecycle used by Survivalist's horde.
- Migrated the horde to the Modforge authority while preserving Unity observation and execution, Survivalist's content values, logging, and chronicle text. Added no tests and did not build.
- Replaced the stale root framework prose with concise capability tables for Modforge, Ueforge, and Unityforge, and moved readers to the existing dedicated decompilation documentation for implementation detail.
- Linked every named framework capability directly to its owning source and normalized the Modforge table so each public system module appears exactly once.
- Reviewed the Modforge capability inventory against every exported module and both binaries, corrected actor knowledge that had been mislabeled as native memory access, and replaced the broad simulation bucket with narrower system groups.
- Compacted each framework inventory into logical categories while retaining exactly one one-word source link per unique capability file.
- Shortened the root README's AI disclaimer to one sentence.
- Replaced the root game table's feature inventories with one-sentence purpose summaries.
- Moved reverse-engineering navigation into the workspace docs index and moved detailed credits into `docs/credits.md`.
- Moved workspace build prerequisites from the root README into `docs/building.md`.
- Reduced the root framework explanation to one sentence covering UE5, Unity, and native games.
- Reduced the root ownership rule to one sentence covering shared, engine-specific, and game-specific code.
- Replaced the root architecture-heavy introduction with a one-sentence description of the toolkit.
- Moved both introductory sentences above a Mermaid dependency graph of Modforge, its engine layers, and current consumers.
- Removed the redundant framework wrapper and promoted each framework capability table to its own root section.
- Replaced private `k3sc` build examples in all nine game-mod READMEs with public `cargo`, `dotnet`, script, or no-build guidance.
- Corrected the game-mod feature inventories after the first README pass summarized systems too aggressively. Expanded Grounded 2, MISERY, Schedule 1, Survivalist, WWM, Horsey, and Scrap Mechanic against their current source while preserving each existing table format.
- Corrected Scrap Mechanic's README to describe its loadable `BetterSurvival` custom game instead of incorrectly calling the directory research-only.
- Reworked every mod feature table so each concise feature name links to its primary implementing source file and a separate description column explains the behavior.
- Documented every function in `misery-mod/src` with a concise player-readable purpose and a specific reason the function remains game-specific instead of moving into Modforge, Ueforge, or Unityforge.
- Audited those function boundaries and recorded the remaining MISERY lifts in `docs/todo.md`: standard asset and game-thread operations, reusable field editing, typed TMap mutation, encounter and phenomenon planning, vendor offer planning, raw TArray append support, checked UFunction calls, transient-object lookup, and live-object hook installation.
- Moved the unchanged `asset_inventory` and `load_asset` handlers into `ueforge::assets`, routed both through `ueforge::game_thread::run`, migrated MISERY registration, and removed its game-local asset wrapper.
- Moved the unchanged `call`, `pe_ping`, and `pe_stats` registration and response construction into `ueforge::game_thread::register_ops`; MISERY now owns only its queue, timeout hint, and installation call.
- Added `FieldEditor` beside Ueforge's existing struct-field access authority and migrated MISERY's cached refresh, numeric sliders, boolean controls, and writes into it; MISERY retains only its catalog, object accessor, ranges, text, and tab wiring.
- Added typed scalar key/value entries and live value mutation to `ueforge::ue::tmap`, then removed MISERY's raw map header, stride, slot lookup, read, and write helpers while preserving its movement keys, baseline speeds, multiplier, and UI.
- Expanded `modforge::storyteller` with adaptive encounter configuration and state, including place lifecycle, copy, escalation, and pack rolls, anchor and class selection, scatter placement, session caps, and successful-spawn accounting.
- Migrated MISERY spawning to supply live Unreal snapshots and game-specific policy, then execute Modforge's requests through Ueforge while preserving its emissions curve, random roll order, caps, exclusions, logs, controls, and spawn behavior.
- Expanded `modforge::storyteller` with phenomenon definitions and planning state for region re-entry, weighted distinct selection, reward-danger pairing, ordered guard placement, count and cluster rolls, class-variant choices, placement requests, caps, and counters.
- Migrated MISERY phenomena to supply its catalog and live Unreal facts, resolve ground and Blueprint classes, and execute Modforge's requests while preserving random roll order, placement, counters, logs, controls, and spawn behavior.
- Verified the MISERY library compiles. Modforge built and 312 tests passed; the full library target remains red only on the existing `input::tests::backend_parse_rejects_garbage` mismatch where `l3` is accepted.
- Added `modforge::vendor::OfferPlanner` for percentage pricing, globally unique provisional assignments, commit-on-success behavior, inventory mirroring, and caller-supplied special offers.
- Migrated MISERY's vendor mirror, ammo, food, and sewing-kit decisions to Modforge while preserving vendor order, item and currency policy, prices, append-failure fallback, and raw Unreal mutation.
- Verified the MISERY library compiles. Modforge built and 312 tests passed; the full library target remains red only on the existing `input::tests::backend_parse_rejects_garbage` mismatch where `l3` is accepted.
- Added raw clone-and-append support to `ueforge::ue::tarray`, including engine-allocator growth, template cloning, slot writes, and count updates.
- Migrated MISERY vendors to provide only item, price, and stock byte patches while preserving its offsets, spare-capacity policy, logging, and failure behavior.
- Verified all 53 Ueforge library tests pass and the MISERY library compiles with its existing unused `STACK_TWEAK` warning.
- Added Ueforge live-object lookup for exact classes and transient class-chain matches, then reused the transient lookup in the engine-tick installer and MISERY autoload.
- Added checked byte-buffer and zeroed-parameter UFunction calls to `ueforge::ue::pe_call`, then removed MISERY's local call helper and raw dismissal call.
- Added game-thread live-object hook installation and completed-hook teardown to Ueforge, then removed MISERY's notice poller, installed flag, raw object cast, hook installation, registration, and post-dismissal lifecycle boilerplate.
- Preserved MISERY's save decisions, exact Blueprint names and parameter layouts, notice class filter, re-entry guard, dismissal behavior, diagnostics, and logs. Added no tests.
- Verified `k3sc cargo-lock check -p ueforge -p misery-mod`; both crates compile with only MISERY's existing unused `STACK_TWEAK` warning.
- Verified `k3sc cargo-lock test -p ueforge --lib`: all 53 existing tests pass. Rechecked `k3sc cargo-lock check -p misery-mod` after the hook-lifecycle completion; it still compiles with only the existing warning.
- Verified the changed Modforge poller through `k3sc cargo-lock test -p modforge --lib`: its existing tests pass within the 312 passing tests. The full target remains red only on the existing `input::tests::backend_parse_rejects_garbage` mismatch where `l3` is accepted.

## Next steps

- Start with the Unityforge managed-object and collection helpers because they remove repeated bridge boilerplate across nearly every Survivalist system.

## Open questions

- None for the Survivalist source audit. The extraction order is recorded in `docs/todo.md`.
