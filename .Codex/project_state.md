# Modforge project state

## Current focus

Extract engine-independent Survivalist systems into their existing Modforge authorities without changing game behavior.

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
- Survivalist retains voter eligibility, conscript exclusion, trait projections, thresholds, actions, and reinforcement outcomes.

## Last session summary

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

## Next steps

- Review the next unchecked Modforge extraction candidate in `docs/todo.md`.

## Open questions

- None for the completed item, upgrade, and mission extractions.
