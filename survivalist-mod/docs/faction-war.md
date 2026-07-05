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

## Scorecard: the authoritative progress tracker

THE place to see where faction war stands and what to work on
next. One row per vision pillar; scores move ONLY on
live-verified changes (watched happening in a running game, not
code-landed). Baseline scored 2026-07-04 against the vanilla
game.

| Pillar | Score | Why this score today | What 10/10 looks like |
|---|---|---|---|
| AI-vs-AI war | 2/10 | Machinery is pair-agnostic and extortion/ambient hostility touch AI pairs, but organized invasions only ever target the player; no systemic AI-AI war exists. | AI factions declare, wage, and settle wars with each other without player involvement: raids launched both ways, ceasefires/surrenders happen, allies drag each other in. Verifiable by spectating two camps. |
| Town growth | 1/10 | Settlements only repair/rebuild their worldgen footprint; the only population growth is the conjuring repopulator. | Settlements visibly expand: new buildings beyond the worldgen footprint, population grown through recruitment, growth rate tied to their food/resource situation and war posture. |
| Fight for control | 2/10 | Research-corrected baseline: `OccupyBase` fully transfers a base (buildings, crops, animals) but only DEAD settlements can be occupied (reclamation by roamers or scripts), never conquest of a living one. | Wars are ABOUT something: bases/areas change hands on victory, controlling more territory feeds growth, and the map's power balance shifts over time. |
| No cheating | 2/10 | Repopulator conjures people + gear in place; raider camps respawn via spawn points; small credit: squads stock real food/water before traveling. | Every faction person walked onto the map or was recruited from it; every weapon/meal came from loot, trade, crafting, or harvest; destroying a faction's people/stores actually weakens it. |
| Factions can be destroyed | 3/10 | Extermination already ends a community and clears invasions against it, but the repopulator resurrects nearly-dead camps and capture-as-destruction does not exist. | A faction that loses its people or its base is GONE (or absorbed): no resurrection, its territory claimable, visible on the map as a power vacuum. |

Update discipline: when work ships, update the row's score AND
its "why" cell in the same commit as the live verification; never
move a score on code-landed-only work.

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

## Research pass findings (2026-07-04)

### What a raid actually does (StartPillaging, ANSWERED)

`StartPillaging` (Community.cs:5638) ->
`FindRandomEnemyBuildingToAttack` (generic over
`squad.EnemyCommunityId`: works against ANY community, so
AI-vs-AI raids need no pathing work):

- Squads can demolish a building ONLY with carried gear: RPG /
  pipe bomb vs non-explosion-proof buildings, molotov vs
  flammable ones. No demolition gear = they target PEOPLE
  instead (members inside or within ~15 tiles of a building).
  Psycho communities always hunt the nearest person.
- The leader gets an `Attack` command per target; members follow;
  once at the base the squad re-scans an 8-tile radius per pass
  and keeps attacking pillageable enemy objects
  (Community.cs:3956-3998).
- Raids are REAL-GEAR-DRIVEN (no conjured bombs), which fits the
  no-cheat pillar; food/water stock-up before travel is also
  computed from real carried nutrition vs journey length
  (Community.cs:5694).
- Quirk: while a squad's BEHAVIOR is Occupy, the pillage scan
  hard-codes the player community as the enemy
  (Community.cs:3980).

### Occupation (ANSWERED): full base transfer EXISTS, for dead bases only

- `Community.OccupyBase(target)` (Community.cs:4820) is a
  complete property-transfer routine: crop patches and plants,
  every base object (`SetCommunity`), animals, other squads'
  Occupy references repointed, the base rect adopted; small
  cheat inside: missing seeds are conjured
  (`SpawnEquipmentIfSpaceIsAvailable`).
- Gate: `CanBeOccupiedBy` (Community.cs:4772) requires an AI
  settlement with NO active members and a real base rect. So
  today occupation = RECLAMATION of dead/abandoned bases, never
  conquest of a living one.
- Who occupies today: roaming groups (RovingRefugee /
  HunterLooter templates with `CanOccupyEmptyBases`,
  Template.cs:1172) pick an occupiable base while traveling
  (`GoToNextTradeDestination(canOccupyBases: true)`,
  Community.cs:5792) and convert to an Occupy squad; story
  events can order it directly (`StoryEventType.OccupyBase`,
  StoryEvent.cs:3720).
- There is already a settlement LIFECYCLE: camp dies -> roaming
  group claims the base. Conquest is "lower the CanBeOccupiedBy
  bar for wartime victors + reuse OccupyBase", not new
  machinery.

### Ambient inflow (ANSWERED): script templates feed the map

- Travellers (RovingRefugee, RovingTrader, HunterLooter, quest
  groups) are spawned by TEMPLATES in the story-script system
  (`Template : BaseScriptObject`), i.e. the Sandbox story's
  DATA, editable in the in-game Script editor; densities
  (RefugeeDensity etc.) are difficulty knobs consumed at
  worldgen/UI.
- Spawned travellers arrive with GENERATED gear scaled by
  LootDensity (Template.cs:1012). Under the no-cheat boundary
  (the world may feed the map at its edge; towns may not conjure
  in place) this is the acceptable inflow, and it is the natural
  recruitment pool to replace the in-place repopulator.

## Open questions (remaining)

- The surrender/ceasefire negotiation flow (speech nodes,
  extortion demands, what payment does, `OnCeaseFire` effects).
- ANSWERED (2026-07-04): organized invasions are PLAYER-CENTRIC.
  Every `SetInvasionTarget` site with a target: the revenge
  trigger (Community.cs:1430) fires only when the killer is the
  player community or a player ally; the ambush-item chain
  (Community.cs:4746, 21 days) can set an AI-vs-AI invasion when
  a hostile community carries a wanted item; story scripts can
  command ANY community to invade ANY other (StoryEvent.cs:2068).
  There is NO systemic revenge loop between two AI settlements,
  so no self-sustaining AI-vs-AI war: AI-AI conflict is ambient
  (hostile-on-contact fights, looter extortion of weaker AI
  settlements, worldgen hostility) rather than organized raids.
- How a ceasefire decays or re-escalates.
- `Exfil`, `Ambush` (AmbushForEquipmentType suggests item-driven
  ambushes), `Funeral` interactions with war.

## The vision (operator, 2026-07-04)

War between ALL factions: AI factions fight EACH OTHER with
organized warfare; AI factions GROW their towns as they fight (a
fight for control requires growing); growth must be legitimate
(no cheating: no conjured people or gear); and AI factions can be
DESTROYED.

## Gap analysis (vision vs what exists)

### AI factions fight each other

- HAVE: the relationship machinery is pair-agnostic; squads work
  for any community; `SetInvasionTarget` accepts any community
  (the script path proves AI-to-AI works mechanically); looters
  already extort weaker AI settlements; hostile members fight on
  contact.
- GAP: war START and SUSTAIN are player-centric. The revenge
  trigger requires a player/ally killer (Community.cs:1425), and
  `UpdateInvasionTarget` never selects an AI target except by
  adopting an ally's (player-rooted) war.
- CHANGE SHAPE: generalize the revenge trigger to any hostile
  killer community + add AI-vs-AI war causes in
  `UpdateInvasionTarget` (extortion refused, resource pressure,
  nemesis logic aimed at rivals, not just the player). One or two
  Harmony patches. MUST first verify Hunt/pillage works against
  an AI base (StartPillaging research pass).

### Towns grow

- HAVE: members do real work (BuildGoal, FarmingGoal, CraftGoal);
  settlements REPAIR damage and REBUILD their recorded structures
  (`NeedsRepair` + `ConstructionRecords`, which is a rebuild
  list, not expansion); crafting is capped by real infrastructure
  (wood limit per campfire, Community.cs:4945).
- GAP: no expansion exists. No new buildings beyond the worldgen
  footprint, and the ONLY population growth is the repopulator.
- CHANGE SHAPE: the biggest pillar; a new per-community
  development decision layer that drives EXISTING goals (build,
  farm, craft) toward expansion, fed by legitimate recruitment.

### No cheating (inventory of today's cheats)

- Repopulation CONJURES people: on the SurvivorRepopulationDays
  countdown, `GameTerrain.GenerateCharacter` materializes a new
  member with GENERATED equipment inside an existing settlement
  (CommunityManager.cs:740-793).
- Ambient enemy spawn points respawn raider camps
  (`AmbientEnemySpawnPoint`, `OnSpawnedEnemyDied`).
- Zombie respawn buildup on ZombieRespawnDays
  (CommunityManager.cs:1378).
- GOOD precedent: squads stock up REAL food and water before
  traveling (Community.cs:5179).
- CHANGE SHAPE: define the legitimacy boundary (ambient inflow at
  the map edge, e.g. refugees, is the world feeding the map;
  in-place conjuring is not) and replace faction repopulation
  with RECRUITMENT of real wandering survivors, gear from stores
  or crafting.

### Factions can be destroyed

- HAVE: communities already end when all members die
  (`OnCommunityHasNoActiveMembers`, Community.cs:1443); invasion
  targets clear when the target has no active members.
- GAP: destruction only by extermination; occupation/capture as a
  war outcome is unverified (Occupy research pass); and the
  repopulator can resurrect a nearly-dead faction (cheat
  interplay: no-cheat work also makes destruction stick).

## Proposed build order

1. Three research passes to ground the design: `StartPillaging`
   (what a raid actually does), Occupy/CaptureGoal/area ownership
   (what capture can transfer), ambient inflow (spawn points,
   refugees: the legitimate population source).
2. AI-vs-AI wars ignite and sustain (generalized revenge +
   extortion escalation), live-verified by watching two AI camps
   go to war without player involvement.
3. No-cheat substrate: recruitment replaces conjuring; growth
   consumes real resources.
4. Growth brain: settlements expand buildings and population
   through the existing work goals.
5. Control: capture/occupation as a war outcome; destruction
   rules that stick.

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
