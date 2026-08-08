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
| Control plane (research surface) | 10/10 | generic and live-proven end to end: handle chaining walks any structure, main-thread safe, harmony_probe proven on a game class (NPCHealth.Die) |
| Research: map regions | 9/10 | ANSWERED: Map.Regions (6 regions, names + enum + ranks proven), CartelInfluence live values read; remaining: exercise ChangeInfluence |
| Research: NPCs + cartel classes | 9/10 | SPAWN PROVEN IN-GAME (goon visible, interactable); open: tasking a goon so it stays and fights (aggro/behaviour) |
| Research: combat/death/aggro | 6/10 | NPCHealth fully mapped (SyncVar health, Die/KnockOut/TakeDamage, death events); kill hook PROVEN patchable; open: damage-source attribution + aggro |
| Research: loot + mob spawn paths | 6/10 | ItemPickup + DeadDrop mapped live (spawn path done via GoonPool); open: the pickup creation call |
| Combat-XP levelling | 0/10 | blocked on research |
| Loot drops | 0/10 | blocked on research; after levelling |
| Mob farming areas | 0/10 | blocked on research; after loot |
| Faction war | 0/10 | blocked on research |

## Session log

- 2026-08-07 (evening): map regions research ANSWERED live.
  Along the way three control-plane defects found and fixed:
  IL2CPP native fields are proxy properties (reads always
  missed), game-touching ops ran off the main thread (one
  0xc0000005 game crash during an inspect), and complex values
  were dead ends (now carry live handles, so every structure
  chains through the existing ops with no new code). One
  MelonLoader crash fixed: a Vortex sweep removed the patched
  Il2CppInterop.Common.dll; restored from the staging package.
- 2026-08-07: crate created on the proven MelonLoader shim path
  (smoke checklist PASSED same day). Game is 0.4.6f11 with the
  operator's patched interop generator; several third-party mods
  broken by the game update, not by us.
