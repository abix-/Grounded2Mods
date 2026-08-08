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
| Research: NPCs + cartel classes | 10/10 | ANSWERED: spawn AND aggro proven in-game (goon spawned, AttackEntity, operator got attacked) |
| Research: combat/death/aggro | 10/10 | ANSWERED: NotifyAttackedByPlayer fires per player hit (attribution proven live); melee-to-0 raises KnockOut not Die, so XP credits both |
| Research: loot + mob spawn paths | 9/10 | ANSWERED for cash: template clone + FishNet spawn recipe proven in-game (operator picked up spawned $100); remaining: item (non-cash) pickup creation when loot tables need it |
| Combat-XP levelling | 6/10 | WORKING: kill hooks + endless curve + auto-spend + per-save persistence proven; Heavy Hands (punch damage) applies exactly. Open: in-game kill-XP line + feel confirmation from the operator; vitality/regeneration on ice (static setters crash 0.4.6f12, generator fix needed); baseline poisoning on hot reload needs the framework fix |
| Loot drops | 0/10 | blocked on research; after levelling |
| Mob farming areas | 0/10 | blocked on research; after loot |
| Faction war | 0/10 | blocked on research |

## Session log

- 2026-08-08 (evening): combat-XP levelling built and live.
  Kill attribution hooks (NotifyAttackedByPlayer + Die +
  KnockOut), endless curve (50 * level^1.3, cap 1024 =
  unreachable), auto-spend to lowest skill, per-save-slot
  persistence via LoadManager's save folder (proven across 4+
  relaunches). Heavy Hands (punch damage x5 at max) applies
  with exact math. THREE game crashes bisected to one cause:
  static field-backed property SETTERS crash 0.4.6f12 (getters
  fine, instance writes fine); presumed fallout of the patched
  generator's skipped metadata init. Vitality + regeneration
  (PlayerHealth statics) on ice until the generator fix.
  Operator design decisions recorded: endless levelling, more
  skills (toughness, gun damage, fleet foot, jump height),
  auto-spend, loot by mob toughness, Diablo-style mob affix
  types, phone-app UI later, and the standing anti-boredom
  principle (rolled/reactive/emergent + spoiler firewall) in
  the plan.
- 2026-08-08 (later still): loot creation ANSWERED live: cash
  spawned at the player and picked up in-game. Recipe: clone the
  inactive "Dynamic Amount Cash Pickup" template, SetActive,
  position, InstanceFinder.ServerManager.Spawn, set Value,
  UpdateCashStackVisuals. Un-spawned clones get destroyed, so
  the FishNet spawn is mandatory. New control-plane pieces:
  invoke_static op (IL2CPP backend implemented), TypeCache no
  longer crashes the game on unknown class names (restart-only
  shim fix, deployed), scripts/restart.ps1 does the full
  build + redeploy + relaunch loop. ALL RESEARCH GATES for the
  goal checklist's first box are now proven; gameplay build
  starts next (combat-XP levelling first).
- 2026-08-08 (later): kill attribution ANSWERED live via the new
  combat_trace ops (recording prefix hooks on NPCHealth):
  NotifyAttackedByPlayer fires on every player hit, same frame
  as TakeDamage; melee-to-0 raises KnockOut, not Die. XP design:
  record player hits per NPC, credit on Die OR KnockOut.
  Remaining research: the pickup creation call.
- 2026-08-08: THE LOOP IS REAL. Goon spawned via GoonPool and
  ordered onto the player via AttackEntity; operator got
  attacked in-game. Control plane finished: handles flow both
  directions ({"$handle": N} args). Game updated twice under us
  (0.4.6f11 -> f12); interop generator needed a fourth
  null-scan patch, deployed and proven by a clean regeneration.
  Remaining research: XP kill attribution, pickup creation call.
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
