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
| AI-vs-AI war | 8/10 | LIVE 2026-07-05: the FULL WAR LIFECYCLE observed in one day. Ignition: organic (a caught thief; the game's own ladder declared it) and by personality (two ambition wars: The Dirty Killers 9 of 10 on The Silent Bears; The Well-Regulated Bears 36 of 50 on Silent Valley Command). Sustain: the revenge loop two-way. END: Smiley Crow Militia, bled to 3 of 10, SURRENDERED to The Dirty Killers by unanimous peace ballot; the first ceasefire was re-broken by the still-fighting squads (the game's witnessed-attack path re-declares) and the second stuck once blades stopped. Remaining: allies dragged into an AI war watched live; ceasefire decay/re-escalation over days. | AI factions declare, wage, and settle wars with each other without player involvement: raids launched both ways, ceasefires/surrenders happen, allies drag each other in. Verifiable by spectating two camps. |
| Town growth | 6/10 | LIVE 2026-07-05: the growth flywheel closed end to end. Two beds-full camps (Mercado's Army 10/10, Kirby's Co-operative 8/8) each planned a wire-fence annex with gate + shack; their builders consume the records (Kirby's queue 40 down to 25 mid-build); the shacks COMPLETED (beds 10 to 12 and 8 to 10; the 2026-07-04 probe shack at Crazy Hill Team finished too, 7 to 9: a shack adds 2 beds, not 1); the new beds were then filled by real recruits in the same session (press-gangs at five camps, refugees welcomed at The Dirty Punks). Remaining: a fence line watched to completion, attrition realism over days, growth tied to war posture. | REAL growth (operator, 2026-07-04): the settlement itself grows, MORE STRUCTURES and MORE PEOPLE, fed by a non-cheating economy: structures built by real hands from real hauled materials, people recruited from real arrivals, growth rate bound to their food/resource situation and war posture. |
| Fight for control / PREDATION | 7/10 | LIVE 2026-07-05: predation OBSERVED. Jenna's Council beat The Dirty Punks down and CONSUMED them: 5 survivors absorbed (members 20 to 25), the loser extinct, its genome dropped from the pool; the absorbed appear as silenced conscripts in the vote (franchise rule under conquest verified: effective aggression unchanged at 0.63 while the camp swelled). The whole war traced back to a caught burglary: acts compose into consolidation. The loot pass ran but found 0 stored goods (honest zero, their stores were bare). Remaining: a stockpile-stripping observed with actual goods carried; ground-drop loot from the war dead. | Wars are ABOUT something: bases change hands on victory, territory feeds growth, and the wars CONVERGE: left to run, the map consolidates until ONE faction controls it. |
| No cheating | 3/10 | LIVE 2026-07-04: cheat 1 of 3, the repopulator, is DISABLED in the running game (UpdateRepopulation prefix skip; install log confirmed; it had just conjured back a war casualty minutes earlier, and that healing is now impossible). Remaining cheats: raider spawn-point respawns; spawn-time arrival gear is ACCEPTED per the boundary. Trader-party + chicken refills also stopped as a side effect, pending the two operator boundary calls. | Every faction person walked onto the map or was recruited from it; every weapon/meal came from loot, trade, crafting, or harvest; destroying a faction's people/stores actually weakens it. |
| Factions can be destroyed | 8/10 | LIVE 2026-07-05: EXTINCTION OBSERVED. The Dirty Punks are GONE, consumed by Jenna's Council after losing the war their catching of a thief started: survivors absorbed, faction dead, and with the conjurer disabled it stays dead. The map went from 22 settlements to 21 by Darwinian consolidation, no player involved. Remaining: the husk visible as a claimable power vacuum (vanilla roamer reclamation exists; watch one happen). | A faction that loses its people or its base is GONE (or absorbed): no resurrection, its territory claimable, visible on the map as a power vacuum. |
| Faction personality / EVOLUTION | 7/10 | LIVE 2026-07-05: personality is now VISIBLE IN BEHAVIOR. Every organic theft came from a Looter camp and the trade act belongs to the careful (Normal-leaning) franchise: the types act differently with zero hardcoded type checks, purely from seeded genomes + votes. Trait learning WATCHED shifting live: guile took -1.5/-2.0 lessons at Jenna's (caught thief, dead thief) and +1.0 at The Golden Dudes (clean haul); defensiveness has its loop armed via trade. Franchise-under-conquest verified (5 absorbed conscripts silenced, victor's will unchanged). Pending live: aggression shift (famine-gated raids), expansionism's loop, per-survivor heredity blending on absorption. | Normal and Looter settlements are recognizably different actors: each type's war declarations, target picks, growth priorities, and dealings fit its identity, visible to a spectator without reading code. |

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

## THE VISION: a Darwinian world (operator, 2026-07-05)

The real vision, above everything below: encapsulate DARWINISM,
survival of the fittest, and EVOLUTION. A static world is boring.
A world that reacts and evolves not just to the player but TO
ITSELF, that is a world worth playing in. The map is an ecosystem
under natural selection, running whether or not the player is
watching.

Natural selection needs four things; the mod supplies each:

| Darwinian element | Mechanism in the mod |
|---|---|
| VARIATION | Factions differ: Normal vs Looter personalities, and (to deepen) heritable strategic traits: aggression, defensiveness, expansion appetite, alliance tendency. No two factions play the same. |
| SELECTION PRESSURE | Survival is genuinely hard and cheat-free (repopulator killed): finite food, finite people, zombie pressure, competition for the same land. Being unfit HURTS. |
| DIFFERENTIAL SURVIVAL | The desperation ladder: unfit factions get desperate, take bigger risks, and often die for it; fit factions grow (annex), conquer, and absorb. Weakness is selected AGAINST, in real time. |
| HEREDITY / PROPAGATION | Successful strategies must PROPAGATE, not just persist: a faction that conquers/absorbs another spreads its own traits into the survivors; a faction that dies takes its traits with it. Over time the map's surviving factions embody whatever worked. (Trait model is the deep TODO; see below.) |

The emergent payoff: no scripted ending. The map self-organizes
toward whatever configuration is fittest for the current
conditions, a lone aggressive empire, a defensive coalition, a
fragile multipolar balance, and it can tip when conditions change
(a hard winter, a zombie surge, a war of attrition). The operator
plays inside a world that is competing with itself.

How the existing pillars serve selection:

- Survival/desperation ladder = the selection ENGINE (fitness
  gets tested continuously; the unfit act rashly and die).
- Growth (people + annexes) = fitness EXPRESSION (the fit
  visibly expand).
- Fight for control / conquest = the SELECTION EVENT (the fit
  consume the unfit; territory and people transfer).
- Faction personality = VARIATION (the raw material selection
  acts on).
- Factions destroyed + "eventually one faction" = the
  EQUILIBRIUM selection drives toward (and can be re-disrupted).

### A settlement is a COLLECTIVE, not an entity (operator, 2026-07-05)

The deepest reframe: a settlement is not one actor with one
genome. It is a COLLECTION OF COOPERATING SURVIVORS, each a
Darwinian individual, and what the settlement DOES emerges from
them voting. This is more lifelike and more truly Darwinian:
selection acts on INDIVIDUALS; the faction's "personality" is an
emergent aggregate that shifts as its people live, die, learn, and
change.

Model:

- INDIVIDUAL GENOME: every survivor carries their own trait
  genome (aggression / expansionism / defensiveness / guile / ...)
  that VARIES at birth, LEARNS from what that person lived through,
  and dies with them. The faction-level genome shipped so far is a
  v1 APPROXIMATION of this; the target is per-survivor.
- ROLES + RELATIONSHIPS already exist in the game and are the raw
  social structure: `Rank` (Leader / Captive / None), `Role`
  (Farmer, Guard, Builder, Trader, Enforcer, Cook, Medic, ...),
  and `Relationship` (parent/child/sibling/married/in-love, i.e.
  families and friends). Build the collective on these, do not
  invent parallel structures.
- COLLECTIVE DECISION BY VOTE: when the settlement must choose
  (raid vs endure, expand vs turtle, ally vs go it alone, flee vs
  stand), its VOTING MEMBERS each "vote" their own genome +
  situation, and the aggregate is the settlement's choice. So the
  same hunger produces a raid in a camp of bold survivors and
  endurance in a camp of cautious ones, and it can shift as the
  population changes. Even more emergent variation: the survivors
  collectively figure out what keeps them alive.

THE FRANCHISE RULE (the killer differentiator):

- NORMAL settlements: EVERYONE votes. The collective personality
  is FLUID: absorb survivors and the newcomers get a voice, so the
  camp's character drifts toward whoever joins. A Normal camp is
  what its current people are.
- LOOTER settlements: ONLY THE CORE LOOTERS vote. Press-ganged /
  absorbed members (the conquered) are voiceless, a natural fit
  for `Rank.Captive` or a "conscript, not core" flag. So Looter
  identity is STABLE under conquest: a Looter faction can swell
  with absorbed pacifists and stay ruthless, because the original
  predators still decide. It is what its CONQUERORS are, not its
  conquered.

Why this is powerful:

- Two axes of personality now: the genome MIX of a camp's people,
  AND who is allowed to vote. A Normal and a Looter camp with
  identical populations behave differently because of the
  franchise alone.
- Conquest gains new meaning: a Normal victor is CHANGED by whom
  it absorbs (diluted or radicalized); a Looter victor is only
  FED by it (bigger, unchanged in will). Selection and heredity
  now play out at the level of who holds the vote.
- Learning is per-person: a survivor who survived a disastrous
  raid carries that caution into every future vote, and spreads it
  only if they live and (in a Normal camp) get to vote.

LIVE STATUS (2026-07-05): the collective model is SHIPPED and
verified live. Per-survivor genomes exist; the raid decision is a
FRANCHISE VOTE. Observed on the live map via survival_status:
Looter camps vote UNANIMOUSLY to raid (e.g. 50/50, 22/22) at ~0.6
to 0.7 effective aggression; Normal camps split, only a bold
minority in favor (e.g. 1/5, 9/15) at ~0.3 to 0.38. The
type difference EMERGED from ~200 individual votes, not a
hardcoded faction trait. `silenced` shows conscripts, and
disenfranchisement is now OBSERVED live (2026-07-05): Kirby's
Co-operative press-ganged 3 refugees and survival_status shows
exactly those 3 as silenced (franchise 6 of 9 members). Learning
is per-voter; a dead survivor's genome + vote leave the pool
(OnMemberDied).

ROLES (operator 2026-07-05, expand later): every survivor already
has a `Role` in the settlement (Farmer, Guard, Builder, Trader,
Enforcer, Cook, Medic, Gatherer, Lumberjack, Miner, Trapper,
Organizer, ...). Future expansion of the collective: weight votes
by role (a Guard's opinion on raiding counts more; a Trader
favors extortion/trade over war), let roles gate franchise
further, and let individuals evolve toward the role their genome
+ experience suits. The vote engine is built to extend this way.

Implementation path (later phases, big): promote the genome from
faction-level to per-survivor (keyed by character), tally votes
among the franchise for each settlement decision, and gate the
franchise by type + rank. The current faction-genome + desperation
engine is the scaffolding; the collective/vote model is the deep
version of the same idea.

### Learning: personality shaped by experience (operator, 2026-07-05)

Evolution here runs on TWO timescales, and both matter:

1. WITHIN a faction's life (LEARNING / plasticity): a settlement
   remembers what worked and what did not, and its personality
   GROWS from its own experience. It is not born fixed by type;
   it becomes who its history makes it.
2. ACROSS factions (SELECTION + HEREDITY): fit factions survive
   and propagate their (now-learned) traits through
   conquest/absorption; unfit ones die with their traits.

The learning loop, per faction:

- It makes a choice driven by a trait (raid vs turtle vs expand
  vs ally), because it was desperate or ambitious.
- It observes the OUTCOME: raid won food and people, or lost
  members for nothing; the annex filled, or got burned; the
  alliance saved it, or dragged it into a losing war.
- It REINFORCES the trait that led to a good outcome and weakens
  the one that led to a bad one. Success makes a raider bolder;
  a costly defeat makes it cautious.

Over a playthrough this makes each faction's personality a
DYNAMIC record of its life: two Looter camps that started
identical diverge because one's raids paid off and the other's
got its people killed. Then selection acts on those learned
personalities, and conquest spreads the winners' learned traits
into the survivors they absorb. The map does not just select
among fixed types; it LEARNS and EVOLVES.

Design consequence: the faction trait genome (below) is not just
inherited, it is UPDATED by outcomes. Each faction carries
trait values (aggression, expansionism, defensiveness, guile)
that drift toward what its own experience rewarded, feed its
next desperation-response choice, and propagate on conquest.

The deep gap for true EVOLUTION (not just selection): factions
have no HERITABLE TRAITS that vary, propagate through conquest,
and mutate. Today "personality" is just the Normal/Looter type.
The evolutionary layer (later phase): give each faction a small
trait genome (aggression, expansionism, defensiveness, guile),
let conquest/absorption blend the victor's traits into survivors,
add small drift, and let selection do the rest. Then the map does
not just SELECT, it EVOLVES: the trait mix of the surviving
factions shifts over a playthrough toward what the world rewards.

## Multidimensional factions: the act repertoire (operator, 2026-07-05)

Operator mandate: factions must DO THINGS THAT MAKE SENSE, and be
multidimensional: scavenge, steal, murder, trade, extort, raid.
Not one lever (war) but a repertoire of acts, each chosen because
it fits who the faction is (genome + franchise vote) and what its
situation calls for, with outcomes feeding the per-voter learning
loop. Consequences flow through the GAME'S OWN systems, so acts
generate organic drama (a caught thief ignites a war through the
vanilla caught-stealing path, no mod ignition needed).

Research pass 2026-07-05 (fresh ilspycmd dump; every seam below
read in the decompile, none guessed):

| Act | Trait affinity | What the game already gives (verified) | What the mod adds |
|---|---|---|---|
| Trade | cautious, defensive | Trade squads travel to any non-hostile settlement with beds (GoToNextTradeDestination, Community.cs:5792) and HANG OUT there (SquadAction.Trade = SetHangoutLocation, Community.cs:3529). Goods exchange exists ONLY in the player trade UI; AI-to-AI trade is cosmetic today. | The actual exchange: when a trade squad idles at a friendly camp, surplus moves for need (real items via the proven Take/Add carry pattern, both sides gain). The peaceful acquisition dimension. |
| Scavenge | expansionist | Loot locations exist as map data (LootLocationDef); predation leaves dead-camp husks with stockpiles nobody owns; the carry-home pattern is proven (predation looting). | Scavenge parties: a small squad walks to a husk or ruin, collects, walks home. The baseline low-risk acquisition act; vultures on the map's corpses. |
| Steal | guile | Picking up another community's property IS theft (IsStealingToPickUp, Character.cs:19043); OnStoleSomething (Character.cs:7327) runs the whole consequence ladder when seen, up to war. | A high-guile camp sends a thief into a neighbor's stores for food/gear. Unseen: free wealth. Seen: the VANILLA path ignites the fallout. This is the organic-war-ignition vector. |
| Extort | aggressive, guile | Looters already extort weaker AI settlements on a hardcoded cadence (ExtortAISettlements). | Move the choice into the franchise vote; personality picks the targets and the cadence, not the community type alone. |
| Rob (ambush) | aggression, guile | Community.Ambush (Community.cs:8080) sends a squad after a character CARRYING a wanted item type: demand it (SpeechSituation.Ambush), fight on refusal; SearchForAmbushItem (4720) scans who holds it. | Aim it AI-vs-AI: item-hungry camps rob travelers and rich neighbors of the thing they need. |
| Murder | high aggression, guile | A real stealth-assassination attack path with secrecy handling exists (assassinate + stealthy melee, Character.cs:10798; witness/sound attribution decides if it is pinned). | A lone operative kills a rival camp's member quietly: weaken before a war, or revenge without one. Risky: attribution through the game's own witness systems means it can ignite. |
| Raid / war | aggression, expansionism | SHIPPED: the hunger raid (famine-gated) + the two-way revenge loop. | Ambition ignition: comfortable but aggressive + expansionist camps vote to prey on a weaker, richer neighbor. Wars that start because of WHO a faction is, not only how hungry it is. |

Decision shape (extends what is already live): on the survival
scan, enumerate the acts the situation makes eligible (surplus to
trade, a husk to scavenge, a rich weak neighbor to steal from /
rob / raid), put the choice to the franchise vote with weights
from each voter's genome, run the winner as a squad mission, and
judge the outcome later into the voters' traits exactly like the
raid learning loop. One engine, many acts, personalities visible
in which acts each camp keeps choosing.

Proposed build order (one act per increment, live-verified before
the next): steal (new dimension AND organic ignition), trade
exchange, ambition raid, rob, murder, extortion into the vote.

STEAL SHIPPED (commit `3a10136e`, survivalist-mod/src/steal.rs),
live status 2026-07-05:

- Launch scan every 2 minutes: camps that can spare a body (3+
  members, no invasion, no threats) hold a guile franchise vote
  (per-voter floor 0.5, majority carries; conscripts voiceless
  under Looter rule as everywhere). The most guileful yes-camp
  sends its highest-guile free non-leader member at the nearest
  richer (nutrition + 0.15) non-hostile non-allied neighbor. One
  launch per scan, four thefts in flight map-wide at most.
- The thief travels as a REAL 1-member Trade-behaviour squad
  through the game's own AddSquad / AddToSquad / SetSquadAction
  path (how roving traders move), so pathing, gates, and combat
  reactions are all vanilla.
- At the stores: up to 2 stacks move by the predation-proven
  Take/Add transfer (shared `carry_off_stored_goods`, now capped),
  then the game's own `OnStoleSomething` runs its REAL
  line-of-sight check: seen means StopThief plus the game itself
  setting the pair Hostile (organic ignition through the vanilla
  caught-stealing path); unseen means a clean getaway.
- Learning is per-voter on guile, like the raid loop on
  aggression: clean haul home +1.0, caught -1.5, thief died -2.0.
- Observability: survival_status gains a `stealing` field
  (target, thief, going/returning); every beat logs as
  "survivalist-mod: steal".

LIVE-VERIFIED (2026-07-05, all within hours): EVERY branch of the
act observed, all launches from Looter camps (the guile seeding
makes them the burglars with zero type checks):

- Caught, survived, WAR: Ellie Carey (Kirby's Co-operative) took
  2 stacks from Crazy Hill Team, was seen by the game's real
  line-of-sight check, and the vanilla ladder declared the war:
  the first ORGANIC ignition. She got home alive with the loot;
  her franchise took the -1.5 guile lesson.
- Clean getaway: Rachel Hyde (The Golden Dudes) slipped out of
  Samantha's Gang's stores unseen and carried 2 stacks home; the
  franchise grew bolder (+1.0 guile).
- Caught and KILLED, then the CASCADE: Kennedy Grant (Jenna's
  Council) was caught at The Dirty Punks, war declared (second
  organic ignition), the thief killed (-2.0 guile on top of
  -1.5). The war then ran the whole Darwinian chain: revenge
  invasions BOTH ways, the Punks beaten down, and PREDATION
  consumed them to EXTINCTION. One burglary rewrote the map.
- Timeout recall: Colby Grant's long trek to The Smiley Cobras
  fizzled and he was recalled cleanly.

Hot-reload hardening (same day): a reload empties the Rust-side
mission lists while the squads survive in the game, so init now
sweeps Trade-behaviour squads off AI settlements (provably ours;
vanilla gives Trade squads only to roving/temporary communities).
Live: reclaimed 3 orphans.

TRADE SHIPPED (commit `159fcdcb`, survivalist-mod/src/trade.rs),
same day: the peaceful act. A camp whose franchise votes caution
(defensiveness floor 0.5, majority: Normal camps seed careful, so
they become the traders as naturally as Looters became the
thieves) loads its most careful free member with 2 real food
stacks from the home stores and walks them, as a 1-member Trade
squad, to the nearest camp hungrier by 0.2+ nutrition (allies
welcome, enemies excluded). At the host: food into their storage,
1 non-food stack home as barter payment, all by the game's own
Take/Add (vanilla AI-to-AI trade was cosmetic: squads only hung
out; this is the first real exchange). Defensiveness gets the
third live learning loop: deal done +1.0 per voter, trader lost
on the road -2.0. survival_status shows a `trading` field.

LOADING MODEL (fixed live 2026-07-05): camps keep food PLANTED
and CARRIED, not warehoused, so building-only loading found
nothing (the engine voted and paired for 15+ minutes with zero
launches, invisible until no-load log lines shipped). The caravan
now loads from building stores first, then campmates' carried
surplus: real hand-offs at home, each donor keeping their last
stack. First caravan then launched immediately: "Smiley Skull
Army (Normal, fed 1.36, 7 of 8 voters careful) sends Abraham
Bowers with 2 food stack(s) to hungry The Golden Dudes (1.14)",
while Maxwell's Posse honestly declined for lack of spare food.
The type split is now complete on the live map: Normal camps
trade, Looter camps steal, from the same genomes with zero type
checks. NOT yet observed: the delivery + payment + return half.

AMBITION WAR SHIPPED (commit `9a03efe7`, survival.rs), same day:
wars because of WHO a faction is. A comfortable, unthreatened
camp of 8+ whose franchise votes appetite (per-voter aggression/
expansionism blend, floor 0.55, majority) preys on the nearest
non-hostile non-allied neighbor at most HALF its size, through
the same hostile+invasion ignition as the hunger raid. Hunger
outranks ambition (one ignition per scan) and a 600-second
map-wide cooldown keeps consolidation paced. The learning
experiment now carries the traits that drove the choice, so an
ambition war's outcome teaches BOTH aggression and expansionism
per voter: expansionism's learning loop is armed. LIVE-VERIFIED
launches: "The Dirty Killers (Looter, comfortable, 10 strong)
VOTES to prey on The Silent Bears (5 members): 9 of 10 voters
hungry for more (effective ambition 0.61)", then one cooldown
later "The Well-Regulated Bears (Looter, comfortable, 50 strong)
VOTES to prey on Silent Valley Command (16 members): 36 of 50".
The map's apex predator has picked its next meal; the pacing
cooldown is doing its job. Outcomes (and the learning they feed)
pending observation.

ROB SHIPPED (commit `b910d815`, survivalist-mod/src/rob.rs),
same day: armed robbery, riding the game's own ambush mission
WHOLE. A peaceful camp of 5+ whose franchise votes menace
(aggression/guile blend, floor 0.55, majority) picks a roving
trader or refugee party carrying a stack worth taking and calls
`Community.Ambush`: the game itself staffs a real party
(Enforcers and Guards first), walks them over (the teleport
stagecraft story ambushes use is disabled by an effectively
infinite delay, so they WALK), demands the item in a real
conversation, takes it by force from whoever holds it if
refused, retreats if the fight turns, and walks home. The mod
only makes the choice and judges the outcome later: loot in camp
hands teaches aggression AND guile up per voter; a dead lead
teaches both down hard; empty hands sting a little.
survival_status gains a `robbing` field. NOT yet observed: the
first robbery.

Timeout correction (same day): steal/trade missions now get 30
minutes, not 15; real cross-map walks were fizzling (Abraham
Bowers' caravan, Colby Grant's trek).

SURRENDER SHIPPED (commit `2b39069c`, survival.rs), same day:
wars can now END short of extinction. A camp bled below half its
worldgen size (or terminal), still 3+ strong (at 2 or fewer
predation decides its fate, not diplomacy), facing a hostile at
least twice its strength, holds a peace ballot: a voter wants
out when their own aggression is at or below 0.5. A majority
surrenders through the game's own ceasefire, loser as initiator
(exactly how the game records who capitulated); the winner's
invasion drops on its own since the pair is no longer hostile.
Proud franchises fight on and risk being consumed: selection
acting on aggression itself.

OBSERVED (2026-07-05, within the hour): "Smiley Crow Militia
(bled to 3 of 10) SURRENDERS to The Dirty Killers: 3 of 3 voters
wanted peace; ceasefire." It fired twice, 90 seconds apart: the
first ceasefire was RE-BROKEN because the winner's squads were
still mid-fight and the game's own witnessed-attack path
re-declared hostility; the second stuck once the fighting
stopped, and no re-fires since. Peace talks failing while blood
still flows, then holding: emergent, and honest.

EXTORTION INTO THE VOTE SHIPPED (commit `bfef8cf9`, survival.rs),
same day: vanilla gates the shakedown racket on the Looter type
plus the per-camp `ExtortAISettlements` knob (a public field,
default true). The knob now follows the franchise's menace ballot
(aggression/guile blend, floor 0.5): a Looter camp whose people
have learned caution calls off the shakedowns; an unrepentant one
keeps squeezing. Flips are logged; steady states are silent.
Personality expressed through the game's own lever.

ROB: FIRST FULL ARC OBSERVED (2026-07-05): "Kirby's
Co-operative's robbery of Trading Party: the loot is in camp
hands; menace paid." Four looter franchises voted robbery within
the hour, all picking gold-rich trading parties. Three fixes from
live evidence: (1) hot-reload double-launch closed by reading the
game's own squads list (one Ambush party per camp); (2) no
robbery while the camp has a thief or caravan out (the vanilla
ambush party-fill yanks members from OTHER squads and pulled
Kirby's thief off his mission); (3) the verdict now measures GAIN
(item count before vs after), so prior wealth cannot fake a
successful heist.

MURDER SHIPPED (commit `14b8530a`, survivalist-mod/src/murder.rs),
same day: the darkest act. A camp AT WAR whose franchise votes
for the knife (per-voter guile floor 0.6, the highest bar) sends
its most guileful free member to assassinate the enemy LEADER: a
decapitation strike instead of another raid. The walk out is the
standard 1-member squad; the kill is the game's own assassination
command (`Character.CommandChokeHold` with HoldType.SlitThroat,
the exact entry the player UI issues), so the sneak, the grab,
the throat-slit, the victim's struggle, stealth skill, witnesses,
and secrecy are all vanilla; the squad is shed just before the
strike so the kill goal owns the operative, and a fresh squad
walks them home. One murder in flight map-wide. Learning: clean
kill +1.5 guile per voter; dead operative -2.0 guile -1.0
aggression; blown attempt -1.0. survival_status gains a
`murdering` field. NOT yet observed: the first plot (needs a
warring camp with a dark franchise and a living enemy leader).

## Predation phase (2026-07-05): the selection event

Operator-corrected model: Darwinian predation, NOT territorial
takeover. Vanilla `OccupyBase` is a MOVE-IN (occupier inherits the
dead base's rect/perimeter/buildings/identity), which is wrong for
this vision. Predation instead: strip the loser of everything
portable, bring it home, leave a husk that dies. No real estate
changes hands; only life and material.

Shipped (commit `876e8b2c`, survivalist-mod/src/predation.rs), on
the survival tick:

- TRIGGER: a faction whose invasion target is beaten to <= 2
  living members (or nobody conscious) consumes it. One conquest
  per scan, dramatic and paced.
- CONSUME THE PEOPLE: survivors absorbed into the winner via the
  game's own SetCommunity; they walk to the winner carrying their
  inventories (portable wealth home for free).
- EXTINCTION: emptied of people, the loser hits 0 and the game's
  own death fires; with the conjurer dead it STAYS gone.
- SELECTION + HEREDITY: `genome::remove` drops the loser's trait
  set from the pool; the winner's genome lives and spreads (more
  bodies carry it). The evolution loop closes: variation ->
  selection -> heredity, all turning.

REAL LOOTING (operator-locked "carried not cheated", shipped
commit `7a93dc01`): the loser's building-stored goods are LOOTED,
not ownership-flipped. Absorbed survivors move each stored item
into their OWN inventory via the game's own
`EquipmentContainer.Take` (removes from the building) + `Add`
(gives to the carrier, honoring REAL carry capacity), then
physically walk the loot to the winner's base. Nothing is
duplicated or teleported; wealth is conserved and hands do the
carrying. If nobody survives to carry, the stockpile stays in the
husk. APIs verified live before wiring (Buildings -> Prop.Inventory
-> GetItem -> GetAmount -> Take -> carrier.Inventory.Add; a real
camp had ~2 items per storage building). Ground-drop loot from the
war dead is a later refinement (needs the rect-enumeration
infra).

OBSERVED LIVE (2026-07-05): "PREDATION. Jenna's Council
(aggression 0.55) consumed The Dirty Punks: absorbed 5
survivor(s) ... The Dirty Punks is EXTINCT." The war had grown
out of a caught burglary (steal act), escalated through two-way
revenge, and ended in consumption: the full selection event, no
player, no ops. The absorbed five show as silenced conscripts in
Jenna's franchise (will unchanged at 0.63 effective aggression).
The loot pass ran but their stores were bare (0 goods, honest
zero); a stockpile-stripping with actual goods is still to be
watched.

## Evolution engine status (2026-07-05)

Shipped + live (commits `efa02bcc` survival, `05af2bbb` genome):

- SURVIVAL ASSESSMENT: every AI settlement on a desperation ladder
  (comfortable/strained/desperate/terminal) from food + population
  trend + threat. Live map read 16/5/1 at first snapshot.
  survival_status makes it legible (desperate first).
- TRAIT GENOME (the evolution spine): per-faction
  aggression/expansionism/defensiveness/guile, seeded by type +
  deterministic Id jitter. VERIFIED varied live: 22 distinct
  genomes, clean Looter/Normal aggression split, none identical.
  genome_status shows them.
- CHOICE reads the genome: the desperate-hunger raid picks the
  MOST aggressive eligible camp; timid camps endure. Vanilla only
  ever begged the PLAYER when hungry; this is AI-vs-AI, driven by
  personality.
- LEARNING loop (plasticity): each raid is an experiment judged
  ~200s later; a raid that gained people/food raises aggression,
  one that cost people lowers it, faction death is max-negative.
  "learn. <faction> raid PAID OFF/COST THEM ... aggression -> X"
  in the log.
- HEREDITY seam: `genome::blend_into` ready for the conquest phase
  to pour a victor's genome into absorbed survivors.

v1 limits (honest): genome lives for the SESSION, not across
save/load (deterministic re-seed on reload; mid-session learning
lost). Only aggression's learning loop is live; the other three
traits are seeded/shown/propagate but their learning waits for
the behaviors they drive to be survival-wired. The whole engine is
gated on real famine (nutrition <= 0.5) to fire raids; the map is
currently well-fed, so learning kicks in as food gets tight.

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

COMPLETION OBSERVED (2026-07-05): the probe shack finished and
Crazy Hill Team's beds rose 7 to 9 (a shack adds 2 beds, not the
1 guessed here earlier). The append -> real-build primitive is
fully proven: order placed, built by their own member from real
materials, accommodation actually grew. The annex planner built
on it shipped and is live (see the annex model section below).

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
beds required, food not checked; log verb "press-gangs").

PRESS-GANGING OBSERVED LIVE (2026-07-05): seizures at five
looter camps in one session (Mercado's Army twice, Kirby's
Co-operative twice, The Dirty Killers, Cherry's Army, Jenna's
Council), while The Dirty Punks welcomed 2 refugees on the
Normal path. Kirby's three conscripts then appear as silenced
in the franchise vote, tying the growth and collective models
together live.

STILL PENDING: population attrition realism over days with the
repopulator off (war losses now stay lost until someone walks
in).

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

SHIPPED (commit `49d915eb`, survivalist-mod/src/development.rs):
beds-full + fed settlements with an idle builder plan one annex
per 2-minute scan, map-wide: fence line first (wood for Normal,
wire for Looter camps), gate at the outer-edge midpoint, shack
infill, BaseRect adopted at plan time; terrain gated by the
game's own IsImpassable.

LIVE-VERIFIED (2026-07-05): Mercado's Army (beds full 10/10)
planned a 45-post wire annex east; Kirby's Co-operative (8/8)
planned a 38-post wire annex east. Builders consume the records
(Kirby's queue read 25 of 40 mid-build) and both annex shacks
COMPLETED: beds 10 to 12 and 8 to 10, promptly refilled by
press-ganged conscripts. Full loop observed: beds full -> annex
planned -> shack built by real hands -> beds free -> recruits
fill them -> beds full again. Not yet watched: a fence line
standing complete around its annex.

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

VERIFIED LATER (2026-07-05): the counter-invasion, on a
different pair. Cherry's Army lost a member to Jenna's Council's
Hunt squad and set a revenge invasion on Jenna's Council; Jenna's
Council then lost a member to Cherry's Army and set a revenge
invasion straight back. Both directions of the loop, one session,
zero player involvement, no war_ignite that mod generation.

VERIFIED LATER (2026-07-05, steal act): ORGANIC IGNITION. A
caught thief (steal.rs) tripped the game's own caught-stealing
ladder and the game itself declared Kirby's Co-operative vs
Crazy Hill Team. No ignite op, no mod relationship write.

NOT YET VERIFIED (the pillar is not done until these are watched
happening):

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
