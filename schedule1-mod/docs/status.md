# schedule1-mod: status

THE one place for scores, the goal, the priority order, and what
is up next. Procedure lives in [research.md](research.md); the
plan lives in the repo's docs/schedule1-plan.md.

## Goal

Rust gameplay mod for Schedule 1 (IL2CPP default branch,
MelonLoader): combat-XP RPG levelling plus a faction war that
makes the town feel alive (factions hold regions, fight the
player and each other), harder overall. 10/10 for a row means it
could ship and run for years for any player with no fixes.

## Priority order

Research gates everything; then levelling; then faction war in
slices (ownership map, NPC-vs-player contests, territory
pressure, NPC-vs-NPC, director split).

## Scores

| Row | Score | Up next |
| --- | --- | --- |
| Control plane (research surface) | 8/10 | crate deployed and answering; smoke-level ops proven via il2cpp-smoke; needs its own first live run + list_methods coverage on game classes |
| Research: map regions | 0/10 | find the region-owning class live |
| Research: NPCs + cartel classes | 0/10 | walk the NPC classes live |
| Research: combat/death/aggro | 0/10 | find the death path |
| Combat-XP levelling | 0/10 | blocked on research |
| Faction war | 0/10 | blocked on research |

## Session log

- 2026-08-07: crate created on the proven MelonLoader shim path
  (smoke checklist PASSED same day). Game is 0.4.6f11 with the
  operator's patched interop generator; several third-party mods
  broken by the game update, not by us.
