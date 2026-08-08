# schedule1-mod: status

THE one place for scores, the goal, the priority order, and what
is up next. Procedure lives in [research.md](research.md); the
plan lives in the repo's docs/schedule1-plan.md.

## Goal

Rust gameplay mod for Schedule 1 (IL2CPP default branch,
MelonLoader): the FF7 grind loop plus conquest. Run around a
region farming hostile mobs, kills drop loot and grant XP,
level combat stats, then take region control from factions in
a faction war that makes the town feel alive (factions hold
regions, fight the player and each other), harder overall.
10/10 for a row means it could ship and run for years for any
player with no fixes.

## Priority order

Research gates everything; then levelling; then loot drops;
then mob farming areas; then faction war in slices (ownership
map, NPC-vs-player contests, player takeover, territory
pressure, NPC-vs-NPC, director split). The goal checklist lives
in the repo's docs/schedule1-todo.md.

## Scores

| Row | Score | Up next |
| --- | --- | --- |
| Control plane (research surface) | 8/10 | crate deployed and answering; smoke-level ops proven via il2cpp-smoke; needs its own first live run + list_methods coverage on game classes |
| Research: map regions | 3/10 | candidates cited from metadata (ScheduleOne.Map.Map, CartelInfluence); run tests/research_map.rs against the live game |
| Research: NPCs + cartel classes | 0/10 | walk the NPC classes live |
| Research: combat/death/aggro | 0/10 | find the death path |
| Research: loot + mob spawn paths | 0/10 | find pickup/dead-drop creation and the NPC spawn path |
| Combat-XP levelling | 0/10 | blocked on research |
| Loot drops | 0/10 | blocked on research; after levelling |
| Mob farming areas | 0/10 | blocked on research; after loot |
| Faction war | 0/10 | blocked on research |

## Session log

- 2026-08-07: crate created on the proven MelonLoader shim path
  (smoke checklist PASSED same day). Game is 0.4.6f11 with the
  operator's patched interop generator; several third-party mods
  broken by the game update, not by us.
