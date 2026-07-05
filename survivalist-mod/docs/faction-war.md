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
| AI-vs-AI war | 5/10 | LIVE 2026-07-04: ignited war produced a real assault (attacker lost a member at the defender base, Funeral squads burying the dead) AND the generalized revenge trigger fired live ("Almighty Rock Family sets a revenge invasion on The Golden Dudes (member killed)"), re-arming the war without player involvement. NOT yet verified: organic ignition without war_ignite, counter-invasions in BOTH directions, how an AI war ends. | AI factions declare, wage, and settle wars with each other without player involvement: raids launched both ways, ceasefires/surrenders happen, allies drag each other in. Verifiable by spectating two camps. |
| Town growth | 4/10 | LIVE 2026-07-04, both halves moving: first organic recruitment fired and Crazy Hill Team EXCEEDS its worldgen size (7 members, initial 6, conjurer off); the structures primitive is verified through site creation (dev_place order -> their Builder consumed it -> a real construction site stands, building_now=1). Remaining: watching a completion (beds rise), the annex planner, attrition realism over days, growth tied to war posture. | REAL growth (operator, 2026-07-04): the settlement itself grows, MORE STRUCTURES and MORE PEOPLE, fed by a non-cheating economy: structures built by real hands from real hauled materials, people recruited from real arrivals, growth rate bound to their food/resource situation and war posture. |
| Fight for control | 2/10 | Research-corrected baseline: `OccupyBase` fully transfers a base (buildings, crops, animals) but only DEAD settlements can be occupied (reclamation by roamers or scripts), never conquest of a living one. | Wars are ABOUT something: bases change hands on victory, territory feeds growth, and the wars CONVERGE: left to run, the map consolidates until ONE faction controls it. |
| No cheating | 3/10 | LIVE 2026-07-04: cheat 1 of 3, the repopulator, is DISABLED in the running game (UpdateRepopulation prefix skip; install log confirmed; it had just conjured back a war casualty minutes earlier, and that healing is now impossible). Remaining cheats: raider spawn-point respawns; spawn-time arrival gear is ACCEPTED per the boundary. Trader-party + chicken refills also stopped as a side effect, pending the two operator boundary calls. | Every faction person walked onto the map or was recruited from it; every weapon/meal came from loot, trade, crafting, or harvest; destroying a faction's people/stores actually weakens it. |
| Factions can be destroyed | 3/10 | Extermination already ends a community and clears invasions against it, but the repopulator resurrects nearly-dead camps and capture-as-destruction does not exist. | A faction that loses its people or its base is GONE (or absorbed): no resurrection, its territory claimable, visible on the map as a power vacuum. |
| Faction personality | 3/10 | LIVE 2026-07-04: the growth doctrine differentiates the types in the running game: a Normal camp welcomed a refugee at its gate while Looter camps took nobody through the same window. Press-ganging (looters seize refugees near base or roaming squads) is shipped; the first observed seizure is pending. War/peace/target-pick doctrines still undifferentiated. | Normal and Looter settlements are recognizably different actors: each type's war declarations, target picks, growth priorities, and dealings fit its identity, visible to a spectator without reading code. |

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

## NORTH STAR: settlements fighting to survive (operator, 2026-07-04)

This is post-apocalyptic zombie survival. Every faction is
FIGHTING TO SURVIVE and does whatever it takes, taking BIGGER
RISKS as it gets more desperate. The settlements must FEEL like
they want to live, not exist statically.

This reframes the whole effort. War, growth, and conquest are not
separate features to check off: they are what a settlement DOES
when survival pressure pushes it. A fed, safe, populous camp
plays conservative. A starving, dwindling, zombie-pressed camp
escalates: forage harder, extort neighbors, then raid them for
food, then stake everything on a desperate attack, then abandon a
doomed base and flee. Desperation is the engine; war and conquest
are its OUTPUTS. That is why the early war work felt shallow:
wars had no MOTIVE. Now the motive is survival.

### The desperation ladder (design)

Each settlement continuously reads its survival state and acts at
the matching intensity. Rungs (draft):

| State | Signal (all readable live) | Behavior |
|---|---|---|
| Comfortable | fed (nutrition high), full/growing, safe | grow (annex), defend, trade |
| Strained | nutrition dipping, losses, some zombie pressure | forage/hunt harder, extort NEIGHBORS (not just player) |
| Desperate | nutrition < ~0.5, shrinking, threatened | raid neighbors FOR FOOD, attack camps they'd normally avoid, worse odds accepted |
| Terminal | starving + few left + base failing | all-in attack, or abandon the base and migrate/merge elsewhere |

### What vanilla already seeds (build on, don't reinvent)

- Hungry settlements (nutrition < 0.5) send Beg squads, and
  Looters extort, BUT only ever aimed at the PLAYER, and it is
  the only desperate act (no escalation, no neighbor targets).
- `UpdateInvasionTarget` (Community.cs:4948) is the per-settlement
  decision brain: THE hook point to add survival-driven target
  selection.
- `StartJourneyToExitMap` (Community.cs:5737) already lets a
  squad/community leave the map: the migration/abandon lever
  exists.
- Signals readable now over the control plane:
  `CalcCommunityNutritionLevel`, `GetLivingNonZombieMemberCount`
  vs `InitialMemberCount`/beds, the `Threats` list (zombie/enemy
  pressure), relationship + invasion state.

### Live status (2026-07-04): assessment proven, responses to expand

Survival engine shipped (survivalist-mod/src/survival.rs) and
LIVE. `survival_status` reads the whole map onto the ladder:
first snapshot was 16 comfortable / 5 strained / 1 desperate.
The rung logic works: The Dirty Punks read DESPERATE from
population collapse (3 of 10 left after the earlier war), not
hunger.

KEY REFINEMENT the live map forced: desperation has different
CAUSES and "whatever it takes" means the RESPONSE must fit the
cause, not just the intensity.

| Cause | Desperate response (design) |
|---|---|
| Hunger (nutrition <= 0.5) | RAID the nearest well-fed neighbor for food. SHIPPED (armed; no famine on the map yet, so unfired, correctly). |
| Population collapse (< half worldgen) | do NOT raid: recruit frantically, turtle, or flee/merge into a stronger faction. TODO. |
| Threat pressure (zombies/enemies at the base) | fortify hard, or relocate the base. TODO. |
| Terminal (starving + gutted + failing) | all-in attack, or abandon-and-flee via StartJourneyToExitMap. TODO. |

So the hunger raid is one branch of one rung; the next build is
the cause-specific responses (esp. the population-collapse branch,
which is what The Dirty Punks need right now).

### Gap

Vanilla desperation is a single mild rung aimed only at the
player. The build: a per-settlement survival assessment + a
graded response that escalates with desperation and targets
NEIGHBORS, so the map's factions visibly struggle, take chances,
raid each other for survival, and sometimes break and flee.
Conquest/absorption become the natural TOP of the ladder, not a
bolted-on mechanic.

## The original vision (operator, 2026-07-04)

War between ALL factions: AI factions fight EACH OTHER with
organized warfare; AI factions GROW their towns as they fight (a
fight for control requires growing); growth must be legitimate
(no cheating: no conjured people or gear); and AI factions can be
DESTROYED.

Sharpened (operator, same day):

- The settlement respawn timer is "very NOT lifelike and cheaty":
  the mod DISABLES the repopulator for settlements and replaces
  it with something that gives settlements realism. Locked; no
  longer a boundary question for settlements (trader parties and
  chickens remain the two open boundary calls).
- The factions fight for CONTROL OF THE MAP, and the wars
  converge: EVENTUALLY THERE WOULD BE ONLY ONE FACTION. Wars are
  not flavor; they are the engine of consolidation.
- Normal and Looter settlements must each have an APPROPRIATE
  PERSONALITY in their strategic choices; the two types should
  wage war, grow, and deal differently.

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

### Faction personality (Normal vs Looter)

GROWTH DOCTRINE (operator-locked 2026-07-04): Normal camps grow
by WELCOME, Looters grow by FORCE AND FEAR.

- Normal: refugees choose them; join at the gate when there are
  beds and food (nutrition >= 0.5). Passive, steady, fragile in
  war. SHIPPED.
- Looter, three mechanisms:
  1. PRESS-GANGING (SHIPPED): refugees encountered near a looter
     base OR near any of their roaming squads are seized as
     conscripts (full members, not captives; the captive rank is
     for hostages and does not work or fight). Beds required,
     food NOT checked: looters take people hungry and raid for
     the rest. Looters must be out in the field to grow.
  2. ABSORBING THE DEFEATED (lands with the control pillar):
     survivors of a settlement a looter faction defeats are taken,
     converting conquest directly into population.
  3. Protection defections (later, optional): extorted
     settlements occasionally lose a member to the extorter.

The texture this buys: Normal factions are resilient accretors,
Looter factions are violent expanders whose population spikes
after victories and starves during peace; a spectator can tell
the types apart from behavior alone.

- HAVE: `CommunityType.Normal` vs `Looter` exists on every
  settlement; Looters get extortion behavior
  (`UpdateInvasionTarget` step 5), looter gear loadouts
  (`PersonalityGroup.LooterFaction`), and a worldgen split
  (`SurvivorCampLooterPercentage`); one camp gets the `Nemesis`
  flag with player-aimed pressure moves.
- GAP: nothing else differs. Normal settlements have NO strategic
  identity; neither type differs in war ignition, target
  selection, growth posture, alliance-making, or when to sue for
  peace. The nemesis moves aim only at the player.
- CHANGE SHAPE: every decision layer we add (war causes, target
  picks, growth priorities, recruitment, peace) branches on
  community type with a doctrine per type; the type table lives
  in data so personalities are tunable without code.

### Factions can be destroyed

- HAVE: communities already end when all members die
  (`OnCommunityHasNoActiveMembers`, Community.cs:1443); invasion
  targets clear when the target has no active members.
- GAP: destruction only by extermination; occupation/capture as a
  war outcome is unverified (Occupy research pass); and the
  repopulator can resurrect a nearly-dead faction (cheat
  interplay: no-cheat work also makes destruction stick).

## Build order (status as of 2026-07-04)

1. DONE: three research passes (raids, occupation, inflow;
   findings above) + the economy audit (mostly real; three
   concentrated cheats).
2. IN FLIGHT (scorecard 5/10): AI-vs-AI wars ignite and sustain.
   Shipped + live-verified: generalized revenge trigger, ignited
   war, real assault, self-re-arming invasion. Remaining: organic
   ignition (extortion escalation or provocations), two-way
   counter-invasion observed, war end behavior.
3. NEXT (serves BOTH no-cheat and growth-people): `growth_status`
   observability op (population, accommodation, nutrition,
   construction activity, arrivals in transit), then suppress the
   in-place repopulator for AI settlements (log every suppressed
   conjure), then recruitment: roving arrivals absorbed by
   settlements with room + food, via the same community-transfer
   primitive OccupyBase uses.
4. Growth-structures: expansion construction orders beyond the
   worldgen footprint riding the EXISTING real construction flow
   (ConstructionRecords + BuildGoal + Recipe ingredients).
   Pre-req: live-verify that AI rebuilds consume ingredients like
   player builds do.
5. Control: capture/occupation as a war outcome (relax the
   CanBeOccupiedBy dead-settlement gate for wartime victors,
   reuse OccupyBase); destruction rules that stick (rides on the
   repopulator suppression in 3).

## The economy: real vs cheated, per resource (audit 2026-07-04)

Growth implies a non-cheating economy (operator). Audit verdict:
the game's economy is MOSTLY REAL already; the cheating is
concentrated in three places. Growth must be built ONLY on the
real flows, and the three cheats are the no-cheat work list.

| Resource | Verdict | Evidence |
|---|---|---|
| Food | REAL | Settlement nutrition = harvested stores + planted crops vs winter need per member (`CalcCommunityNutritionLevel`, Community.cs:6871); crops are real tile objects in real patches; crop reserves managed (`IsAllowedToEatCrops`); members eat from real inventories; squads carry real nutrition for journeys. |
| Water | REAL | Real containers filled at wells/rivers; squads stock up before travel. |
| Structures / materials | REAL system | Construction is Recipe + Ingredients with per-item consumption tracking (`UnderConstructionInfo.IngredientsUsed`, `HasUsedEnoughOfIngredient`); builders use real tools and chop real wood (BuildGoal); wood crafting capped by campfires. AI REBUILDS presumed on the same flow; verify live once expansion work starts. |
| Gear | MIXED | Crafting (CraftGoal + recipes) and looting are real; extortion/trade transfer real items between communities. BUT all spawn-time gear is conjured (worldgen, template travellers, and the repopulator's kit). |
| Gold | REAL transfers | Extortion moves gold between real holders (`OnShakedown`). |
| People | CHEATED | The repopulator conjures members with conjured kit in place (CommunityManager.cs:740-793). The legitimate inflow exists: template-spawned travellers (refugees) arriving from outside. |

### The in-place repopulator, exactly (CommunityManager.UpdateRepopulation, 629-793)

1. ONE world timer: `SurvivorRepopulationDays` of game time
   (1 day on Medium). On expiry: ONE repopulation attempt, then
   the timer resets; a fruitless attempt retries in 10 seconds.
2. Candidates: Normal/Looter settlements (plus roving trader
   parties with a living trader leader) that have AT LEAST ONE
   living non-zombie member AND sit below
   min(InitialMemberCount, accommodation). Consequences:
   - A fully dead camp NEVER resurrects, even in vanilla.
   - Vanilla can NEVER grow a camp past worldgen size, even by
     cheating; the repopulator only heals losses.
   Chickens have a parallel candidate list (below initial flock,
   coop space), ON TOP of real egg breeding that already exists.
3. Pick: chickens vs people proportionally, then one random
   candidate settlement; one new body per firing, globally.
4. Placement stagecraft: up to 100 tries for a tile on the outer
   edge of a random sleeping hut, rejecting tiles within 64 units
   of the player camera, impassable tiles, and the prison area
   (traders: within 8 tiles of the leader). The conjure is
   guaranteed off-screen.
5. The conjure: `GameTerrain.GenerateCharacter` creates the
   person from nothing with a full generated kit (faction
   loadout), bandages, an invisible-strain dice roll, and a
   randomized relationship to an existing member (sibling /
   parent / lover, reciprocation by dice) so they appear to have
   always lived there.

Suppression plan notes: skip the conjure for Normal/Looter
settlements and log every suppressed one; recruitment of real
arrivals replaces the role. TWO BOUNDARY DECISIONS FOR THE
OPERATOR before building:

- Trader-party refills are also in-place conjures: world inflow
  or cheating?
- Chicken conjuring: does livestock count, given real egg
  breeding exists underneath it?

The three cheats (the complete no-cheat work list):

1. The in-place repopulator (people + kit from nothing).
2. Ambient enemy spawn-point respawns (raider camps refill).
3. Spawn-time gear conjuring for arrivals (ACCEPTED under the
   boundary: the world may feed the map at its edge; a town may
   not conjure in place).

Growth design consequence: the "more structures" half rides the
EXISTING real construction flow (new ConstructionRecords beyond
the worldgen footprint + members hauling ingredients through
BuildGoal); the "more people" half rides recruitment of real
arrivals after cheat 1 is suppressed.

## Structure growth phase 1 (2026-07-04): planner + shim primitive

Shipped (commit `10e1f502`, needs a game RESTART; the shim
changed):

- Shim game-struct marshalling: TerrainCoord / TerrainRect now
  cross the bridge as `{"x":..,"y":..}` objects BOTH ways
  (generic value-type serialize + reconstruct in MonoBridge.cs),
  plus null args. This unblocks passing tiles/rects to game
  methods; needed by every geometry op from here on.
- `common.rs`: shared bridge helpers, deduped out of war + growth
  at the third consumer.
- `development.rs`: the planner foundation.
  - `dev_status`: per-settlement base rect + centre, buildable
    prototype availability (does WoodFence/Shack/... have a
    recipe in this story?), construction queues.
  - `dev_place {community, prototype, dx, dy, orientation?}`: the
    LIVE PROBE. Appends ONE real `ConstructionRecord` at a tile
    offset from the base centre via the game's own
    `AddConstructionRecord`. If the execution layer is as the
    research says, a Builder member picks it up and builds it
    from hauled materials with NO further code.

VERIFIED LIVE (2026-07-04, same session):

- dev_status: ALL SEVEN prototypes (WoodFence, WireFence,
  ConcreteWall, WoodGate, WireGate, Shack, Tent) exist AND have
  recipes in the operator's story. Base rects/centres read
  correctly through the new struct marshalling.
- dev_place Shack for Crazy Hill Team: queued (rebuild_queue 1)
  -> record CONSUMED by their Builder (queue back to 0; the only
  consumption path is site creation + BuildGoal assignment) ->
  `building_now` = 1: a REAL construction site stands in their
  camp, being built by their own member.

PENDING: completion (beds 7 -> 8 when the shack finishes, or an
honest stall if their wood runs short). Once observed, the
append -> real-build primitive is fully proven and the annex
planner (fence line + gate + infill + BaseRect adoption +
per-type wall doctrine) is the next build.

## Growth phase status (people half, 2026-07-04)

Shipped (commit `c5ad12fa`), all live in the running game:

- REPOPULATOR DISABLED (operator-locked): prefix skip on
  `CommunityManager.UpdateRepopulation`; the conjurer can never
  run. Side effect pending the two boundary calls: roving trader
  and chicken refills are also stopped (same method).
- RECRUITMENT of real arrivals (survivalist-mod/src/growth.rs,
  scans every 15s on the tick): a roving refugee group whose
  leader stands within 48 world units of a settlement's building
  joins it through the game's own path (`Character.SetCommunity`
  -> `AddMember`, then `UpdateRoles`, the vanilla repopulator's
  own wiring). Doctrine v1 per type: Normal settlements welcome
  refugees when they have bed headroom AND nutrition >= 0.5;
  Looter settlements take nobody (their recruitment personality
  is an open doctrine question). Population is now capped by REAL
  BEDS, not worldgen headcount: the first mechanism that can grow
  a camp past its starting size.
- `growth_status` op: per-settlement members/beds/initial/
  nutrition/rebuild/repair + refugee groups in transit.

First live snapshot (operator's session): 22 settlements mapped;
The Dirty Punks at 3/11 beds after war losses (the prime
recruitment candidate); one refugee pair in transit; Almighty
Rock Family had been healed 14 -> 15 by the repopulator BEFORE
the disable landed, confirming the cheat was active until the
moment it stopped.

VERIFIED 2026-07-04, minutes after shipping: "Crazy Hill Team
takes in 1 refugee(s) who arrived at their gate (0 bed(s)
left)" in the player log, and growth_status then showed 7
members vs initial 6: a settlement exceeded its worldgen size
through a real arrival. Press-ganging shipped in the same hour
(looters seize refugees near base or any roaming squad leader;
beds required, food not checked; log verb "press-gangs");
first observed seizure pending.

STILL PENDING: population attrition realism over days with the
repopulator off (war losses now stay lost until someone walks
in); the structures half of growth.

## Structure growth design: the annex model (operator-locked 2026-07-04)

Settlements use walls to surround the SAFE area; all structures
live inside the fenced area, with gates to the outside. Expansion
therefore happens ONE FENCED AREA AT A TIME:

1. Plan an annex: a rectangle adjacent to the existing perimeter,
   on legal terrain.
2. Fence it FIRST: order the fence segments and gate(s) along the
   annex's new outer edge (members build them from real
   materials; the annex is not safe until enclosed).
3. THEN infill: order interior structures (sleeping huts, crop
   patches, per-type doctrine) inside the enclosed annex.
4. Adopt it: the settlement's base area (BaseRect / Perimeter)
   grows to include the annex, so defense, diplomacy-at-the-gate,
   and prison logic all treat it as home ground.

This mirrors worldgen exactly (camps generate as a perimeter of
fence posts + gates with structures inside; wall material varies
by camp: concrete for the nemesis, wire/wood elsewhere), so the
expansion uses the same prototypes worldgen uses, and per-type
wall doctrine falls out naturally (Looters favor harder
perimeters; Normal camps wood).

Pre-build research checks: ALL SIX ANSWERED (2026-07-04).

1. CONFIRMED, fully real: a Builder-role member picks a
   ConstructionRecord, the settlement resolves the RECIPE
   (`GameImpl.FindRecipeByProduct`), and the build runs through
   `Session.PlaceBuildingDuringGameplay` -> a real construction
   site (`UnderConstructionInfo(recipe)`) -> BuildGoal with
   hauled ingredients (Community.cs:6326-6352). Bonus: the game
   auto-assigns Builder to the highest-Construction-skill member
   whenever records exist, and its record-picking already
   PRIORITIZES accommodation buildings.
2. CONFIRMED: `Session.PlaceBuildingDuringGameplay(character,
   recipe, tile, orientation, checkCanBuildHere)` is the exact
   programmatic API; the AI rebuild path itself uses it.
3. CONFIRMED: `GameCursor.CanBuildHere(...)` overloads +
   `ConstructionRecord.CanRebuildHere(community)` gate every
   record before building starts (a record on bad terrain simply
   never builds; honest stall).
4. Mechanism confirmed (`FindRecipeByProduct(propProto)`), and
   `AddConstructionRecord` accepts fences/gates/huts (it filters
   only graves/traps/crops). WHICH prototypes have recipes is
   story DATA; verify live with a probe op at implementation.
5. CONFIRMED writable data: `public TerrainRect BaseRect` +
   `public List<TerrainCoord> Perimeter` fields; OccupyBase
   mutates them at runtime. IMPLEMENTATION NOTE: our bridge
   cannot yet marshal game structs (TerrainCoord/TerrainRect) as
   invoke args or field writes; the shim's JSON conversion needs
   a generic value-type path (construct + set fields via
   reflection, same pattern as its Vector2/3/4 special cases).
   One shim change, one game restart.
6. CONFIRMED pattern: worldgen places gates at perimeter segment
   midpoints with orientation from polygon-inside tests
   (GameTerrain.cs:6990-7000); the annex mirrors it on its outer
   edge.

Implementation consequence: the development brain is mostly a
PLANNER. It appends ConstructionRecords (fence line, gate, then
interior structures) and adopts the annex into BaseRect; the
game's own builder assignment, recipe resolution, ingredient
hauling, and legality gating do everything else.

## Phase 1 status (2026-07-04)

Shipped (commit `dba3992e` + framework fix `392bca8a`):

- Generalized revenge trigger: prefix on `Community.OnMemberDied`
  (survivalist-mod/src/war.rs). When an AI settlement's member is
  killed by another AI settlement's member while Hostile, the
  victim invokes the game's own `SetInvasionTarget(killer, 7d)`,
  mirroring the vanilla player-only path. Purely additive
  (player/ally killers stay on the vanilla path).
- `war_status` op: every community with type, members, nemesis,
  invasion target, squads. `war_ignite {attacker, defender}` op:
  forces Hostile + invasion through the game's own methods.
- Framework fix that this work surfaced: unityforge never
  registered modforge's shutdown handlers, so every hot reload
  left the OLD generation's HTTP listener holding the port and
  answering with a stale op registry. Fixed at init
  (unityforge/src/mod_main.rs); needs one more hot-reload cycle
  to be verify-closed.

LIVE-VERIFIED (all on 2026-07-04, one session):

- Patch installs clean on Harmony 2.4.2 (log line at load).
- `war_status` maps the whole world (22 AI settlements + roamers
  in the operator's session).
- `war_ignite` -> Almighty Rock Family formed a Hunt squad of 12
  (of 15 members; matches the staffing clamp) and marched on The
  Golden Dudes with the invasion target set.
- THE ASSAULT: the raid concluded with a Rock Family casualty at
  the Golden Dudes base (15 -> 14 members) and Golden Dudes
  Funeral squads burying the dead.
- THE SUSTAIN LOOP: our revenge trigger fired live. Player log
  "survivalist-mod: war. Almighty Rock Family sets a revenge
  invasion on The Golden Dudes (member killed)". Re-arming the
  invasion with zero player involvement. This is the exact
  capability vanilla lacks between AI factions.

Op usage (the verification surface, run any time):

```
curl http://127.0.0.1:17173/op -d '{"op":"war_status"}'
  -> every community: name, type, members, nemesis, invasion_target, squads
curl http://127.0.0.1:17173/op -d '{"op":"war_ignite","args":{"attacker":"<name>","defender":"<name>","days":7}}'
  -> forces Hostile + invasion via the game's own methods
grep "survivalist-mod: war --" Player.log
  -> every revenge-trigger firing, named both ways
```

NOT YET VERIFIED (the pillar is not done until these are watched
happening):

- Organic ignition: an AI-vs-AI war starting WITHOUT `war_ignite`
  (provocation-driven hostility + a kill).
- A counter-invasion in the OPPOSITE direction (needs a Golden
  Dudes member killed by a Rock Family hand; none died yet).
- War end: what Hostile-with-no-InvasionTarget settles into for
  AI pairs (ceasefire path untested).

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
