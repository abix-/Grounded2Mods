# Faction war: how the game does it today

Faction war in Survivalist: Invisible Strain is a per-pair
relationship state machine driving squad missions. Communities
hold a relationship record per other community; provocations or
scripts flip a pair to Hostile ("war"); a hostile AI settlement
then picks an invasion target and sends a Hunt squad to pillage
it; nemesis and looter communities additionally run standing
pressure campaigns (warnings, extortion, alliance shakedowns);
wars end via ceasefire records where the surrendering side is
marked. Everything below is read from the decompiled game
(Assembly-CSharp; line numbers from the ilspycmd dump; re-derive
any file with `ilspycmd -t <Type> Assembly-CSharp.dll`).

## The flow, end to end

1. Every community pair sits in one of six relationship states;
   Hostile = at war.
2. Wars start from provocations (theft, witnessed attacks,
   gunfire), from worldgen, from dialog, or from story scripts.
3. A war declaration cascades to allies and posts notifications.
4. Each AI settlement's update loop picks at most one "invasion
   target" and forms at most one squad at a time for it.
5. Squads are staffed by role (Enforcers, then Guards, then
   everyone else), sized 4-12, and behave per a mission type
   (Hunt = pillage raid; Extortion; warnings; etc.).
6. Losses feed back: killing a hostile community's member makes
   YOU their invasion target for 7 days.
7. Wars end by ceasefire; the initiating (surrendering) side is
   recorded on the relationship.

## Relationship state machine

- `CommunityRelationshipType`: Unknown, Introducing, Known,
  Ceasefire, Hostile, Allied.
- Each `Community` holds `List<CommunityRelationshipRecord>
  CommunityRelationships` (Community.cs:108); the record is a
  STRUCT `{OtherCommunityId, RelationshipType, Initiator}`.
- `Initiator == true` on a Ceasefire record means THAT side
  surrendered (`GetRelationship`, Community.cs:1163;
  `HasSurrenderedTo` Community.cs:1125).
- Always-hostile communities (raiders etc.) report Hostile to
  everyone unless a specific record overrides
  (Community.cs:1155-1168).
- `CommunityManager.SetRelationship(c1, c2, type,
  showWarNotifications)` (CommunityManager.cs:294) is THE single
  writer: logs "War declared between X and Y"
  (CommunityManager.cs:308), reveals community names
  (`DiscoverNameIfAtWar`), posts the DeclaredWar log event, and
  CASCADES to allies: each side's allies get set Hostile to the
  enemy, ceasefires propagate (CommunityManager.cs:430-444).
  Ceasefire fires `OnCeaseFire` on both communities
  (CommunityManager.cs:458).

## How wars start (every SetRelationship(Hostile) site)

| Trigger | Where |
|---|---|
| Caught stealing from a community | Character.cs:7371 |
| Community member witnesses an attack on their own | AlertGoal.cs:223 |
| Hostile gunshot/sound attribution | Character.cs:21157 |
| Worldgen initial relations (some pairs start hostile) | GameTerrain.cs:9308 |
| Always-hostile community types (raiders) + spawn-time player hostility | Community.cs:326 |
| Dialog / speech choices (introductions can go bad; surrender demands) | Conversation.cs:255 area + StoryManager speech options |
| Story scripts: an event can directly send a Hunt squad at the player | StoryEvent.cs:1708 |

## Invasions (the actual war-making)

State on Community (Community.cs:174-180): `InvasionTarget`,
`InvasionTimeoutTime`, `LastInvasionTime`,
`InvasionTargetWasSetFromScript`.

- REVENGE TRIGGER: when a member dies and the killer belongs to
  the player's community or a player ally AND relations are
  Hostile, the victim community calls
  `SetInvasionTarget(killerCommunity, 7 days)`
  (Community.cs:1425-1430). Scripts can also set it
  (`InvasionTargetWasSetFromScript`).
- `UpdateInvasionTarget()` (Community.cs:4948) is the war brain,
  run only when the settlement is calm (no threats, no squads
  out, leader conscious, nobody in combat). Decision cascade:
  1. Drop the target if timed out, dead, or no longer hostile.
  2. Adopt an ALLY's invasion target (allies join wars).
  3. Nemesis pressure: `WarnOffAlliance` squad when the player
     gains allies, `WarnPopulation` when player population grows
     past their count + 10 (skipped if they surrendered to you).
  4. Nemesis out-allying: `RequestAlliance` squads to neutral AI
     settlements when the player has more allies than they do.
  5. Looter extortion: `Extortion` squads on a cadence gated by
     "community aggro" (see below) with cooldown ladder (player:
     7d, then 5d/3d/2d/1d as aggro rises;
     Community.cs:5003-5030); weaker AI settlements are also
     extorted (7d cooldown, `ExtortAISettlements`).
  6. Hungry settlements (nutrition < 0.5) send `Beg` squads to
     the player (led by their leader).
- Community aggro = `clamp(playerMembers - 1 +
  playerTallStructures, 0, MaxAggro)`
  (CommunityManager.cs:1453). Your growth drives pressure.

## Squads (the units of war)

`Squad` (Squad.cs): Id, SquadOwner, Members, Behaviour, Action,
DestTile/GoalTile, ThreatId, EnemyCommunityId, PillageObjectId,
GoalCharacter, GoalAchieved, AmbushForEquipmentType, timers.

- `SquadBehaviour` (14): None, Defend, Hunt, Travel, Trade,
  Funeral, Occupy, Exfil, Extortion, Beg, WarnOffAlliance,
  WarnPopulation, RequestAlliance, Ambush.
- Staffing (Community.cs:5080-5145): members classified
  Enforcer / Guard / Other by role; only healthy, un-squadded,
  non-follower members are available (health gating is WAIVED
  for the first invasion wave). Squad size = `clamp(active/2, 4,
  12)`; the FIRST invasion against a target sends `active`
  (everyone available). One squad formed per pass; invasions
  have a 1-day cooldown (`LastInvasionTime`).
- Mission start (Community.cs:5179): Hunt squads go straight to
  `StartPillaging(squad, canAttackOutsideBase: true)`; all other
  behaviors stock up food/water first, then travel to target.
- `SetSquadAction` (Community.cs:3601) drives per-action leader
  commands (`SquadAction`: StockUpFood, StockUpWater,
  StopForWarmth, HangAround, ExitMap, ...); the leader gets the
  goal, members follow via `ObeyLeaderGoal`.
- Squad map icons: visible unless the squad `isInvader`
  (invaders are hidden until encountered; Community.cs:3620).

## Nemesis

`Community.Nemesis` (Community.cs:128) is assigned at WORLDGEN
(GameTerrain.cs:6986); nemesis camps generate with concrete-wall
perimeters. Looter-vs-normal camp split comes from
`DifficultySettings.SurvivorCampLooterPercentage`
(GameTerrain.cs:6985).

## Defense side

Communities keep a `Threats` list (per-threat members, bounds,
LastEncounteredTime); Defend squads are raised against threats
(Community.cs:3790) and squad-vs-threat assignment is tracked
(`GetNumberOfSquadMembersAssignedToThreat`). Partially mapped.

## Open questions (next research passes)

- `StartPillaging` internals: exactly what a Hunt squad does at
  your base (PillageObjectId targeting, when they attack
  defenses vs steal vs kill, retreat conditions).
- The surrender/ceasefire negotiation flow (speech nodes,
  extortion demands, what payment does, `OnCeaseFire` effects).
- `Occupy` behavior: takeover of bases/areas (ties to
  CaptureGoal + area ownership).
- Whether AI-vs-AI hunts happen (extortion targets AI; the
  revenge trigger requires a player/ally killer; do two AI
  settlements ever fully war?).
- How a ceasefire decays or re-escalates.
- `Exfil`, `Ambush` (AmbushForEquipmentType suggests item-driven
  ambushes), `Funeral` interactions with war.

## Enhancement seams (where changes plug in)

- `UpdateInvasionTarget` (Community.cs:4948) is THE hook point
  for new war-decision behavior (new casus belli, new mission
  types, different target selection).
- Squad size / cooldown / cadence values are code constants:
  Harmony patch territory (all patchable now via the embedded
  Harmony 2.4.2).
- Relationship changes all flow through ONE method
  (`CommunityManager.SetRelationship`): a single choke point to
  observe or veto war state changes.
- `StoryEvent` can already script Hunt squads at the player: the
  in-game Script editor may allow DATA-side war events without
  code (verify in the editor).
- Live tuning: `Session.Instance.CommunityManager` and every
  community's fields are reachable over the control plane for
  experiments (e.g. set `InvasionTarget`, force a war, watch the
  squad form).
