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
| Combat-XP levelling | 7/10 | EXIT GATE PASSED: operator kill logged "+25 XP ... LEVEL UP -> 9", auto-spend applied, punches confirmed stronger, persistence proven. Open to 10: vitality/regeneration on ice (generator fix), more skills (toughness, gun damage, fleet foot, jump height), baseline-poisoning framework fix, long-play soak |
| Loot drops | 6/10 | WORKING in-game: kills drop rolled, toughness-scaled cash at the body; operator picked it up. Open: unclaimed-drop behavior across save/reload, item (non-cash) drops, affix-count scaling when mob types land |
| Mob farming areas | 2/10 | unblocked: garrison spawner + rolled mob types (tough/armed/veteran) built; minted-NPC re-base sits UNCOMMITTED in the tree, compiles, NOT run in-game; player-level scaling and despawn-on-leave not started |
| Faction war | 3/10 | garrisons + influence bleed + takeover trigger proven in-game (vanilla-goon version); minted re-base unverified; ownership map op, player takeover, NPC-vs-NPC region contests not built |

## Session log

- 2026-08-08 (uncommitted, in the tree): farming re-based onto
  minted NPCs, BUILT BUT NOT RUN IN-GAME. war_pass now spawns
  through NpcFactory (the vanilla 5-goon supply check deleted;
  TOTAL_LIVE_CAP is the guard), mob types re-rolled as tough
  (SetToughness) / armed (baton, knife, or M1911 roll) /
  veteran, applied 8s after minting once the S1API pipeline
  settles; hold-at-post orders dropped entirely on the untested
  assumption that minted BaseEmployee NPCs idle at their posts
  (now a hypothesis row in certainty-tracking.md); farm_state
  reports posts, not live positions. Shim additions:
  NpcFactory.SetToughness, per-mint unique game-side ID (kills
  S1API's duplicate-ID warning spam), game-side NPC ptr
  returned at mint (the kill-hook identity). Evidence so far:
  cargo check and the shim dotnet build both pass 2026-08-08.
  NO in-game evidence: the exit-gate run (10+ minted goons
  fight, die, pay XP/loot/influence) has not happened.
  tests/research_farming.rs added (vanilla-goon
  MaxHealth/Health/movement write probe), never run live.
- 2026-08-08 (late night): THE SUPPLY CAP IS DEAD. Custom NPC
  minting proven in-game: the shim's S1API-backed NpcFactory
  (GoonNpc / PoliceNpc / PlayerNpc + invoke_static statics)
  spawned five visible NPCs on demand at exact positions. The
  key that made them real: S1API's internal
  RegisterCustomNpcForNetworking via reflection (a bare
  constructor leaves them invisible). Open: cosmetics (looks +
  names are defaults), combat/despawn/save behavior of minted
  NPCs, then re-base the war's forces on them.
- 2026-08-08 (night): the influence war is LIVE: persistent
  garrisons across all cartel regions (sized by influence,
  strongest zones fed first from the 5-goon vanilla pool),
  kills bleed cartel influence via ChangeInfluence, takeover
  trigger armed. Supply research finished: live-goon cloning is
  a dead end (three failure stacks captured); custom NPCs
  (goons, police, player NPCs, per the operator) ride S1API's
  registered-prefab recipe next session. Shim gained: Vector3[]
  invoke args, inner-exception unwrapping (opaque invoke errors
  now carry full IL2CPP stacks). Mob hold-at-post via
  SetDestination is unreliable (throws for exit-walking goons);
  the ambush machinery (SpawnAmbush, arms its goons) is the
  candidate replacement, untested live.

- 2026-08-08 (evening, cont.): loot drops live and confirmed:
  player kills drop rolled cash (scaled by the mob's max
  health) at the body via the template-clone + FishNet spawn
  recipe, queued a frame after the kill hook. Heavy Hands
  baseline poisoning killed for good by seeding the proven
  vanilla values at install. Both grind-loop gates (XP + loot)
  passed in-game the same evening.
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
