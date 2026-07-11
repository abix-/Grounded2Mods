# Survivalist mod: status

THE one place to see where the mod stands and what to work on next.
Every x/10 score lives here and nowhere else. The other docs
support this one:

- `research.md`: how to read and change the live game (control
  plane, Harmony, deploy facts). Reference; carries no scores.
- `faction-war.md`: the vision, the design decisions, and the
  game-code research behind each pillar. Design; carries no scores.
- repo `docs/todo.md`: the task backlog; it points here for status.

## The goal

This is a simulation of brutal life in a zombie apocalypse. It is
NOT a difficulty ramp that escalates until everything is dead and
only zombies are left. Pressure exists to make survival hard,
costly, and meaningful, never to guarantee a wipeout. Whatever the
situation, there must ALWAYS be a way for the player to survive,
overcome, or adapt. A hard winter, a zombie surge, a war of
attrition should push the player to the edge and force real
choices, but never close off every road out.

This goal bounds every pressure mechanic in the table below. The
storyteller is the director over the WHOLE story, and it runs two
layers: Randy Random (unpredictable events, the incursions) and
Mario Kart (keeping the leader in check, the horde drawing pressure
to whoever is winning). Both aim at tension and drama, not
annihilation: an alpha camp under a heavy siege should have to
fight, adapt, or give ground to survive, not simply be deleted.
Brutal but survivable is the line.

## How we prioritize

THE POINT OF THIS MOD (operator-locked 2026-07-10): we are making
it to satisfy the OPERATOR, to see if we can make survivalist into
a game that keeps them entertained. Features are added toward THIS
goal as the priority. Everything below serves it.

Feature first, polish last. What ranks highest is whatever makes
the game more FUN, more lifelike, and less boring, because that is
what decides whether people keep playing or quit: a world that
turns repetitive, predictable, and static gets abandoned. UI work,
cosmetics, and nice-to-haves come AFTER the gameplay that makes the
world feel alive.

The guiding star for "fun" is the RimWorld storyteller model: a
solid base simulation plus a director on top. Our storyteller runs
TWO layers, both already decided (do not confuse or merge them):

- RANDY RANDOM: unpredictable events thrown regardless of how the
  player is doing (the incursions and the dread loop). Random by
  design; it delivers variety and not-knowing. It is NOT adaptive
  and must never be made to scale to the player's safety (that
  would be Cassandra, a different storyteller). Randy rolls WHAT
  and WHEN.
- MARIO KART: keeping the leader in check. Adaptive pressure on
  whoever is winning (the alpha, AI or player). Today this is the
  horde: the biggest camp draws scaled zombie packs. Mario Kart is
  the adaptive layer; it targets the leader.

The storyteller is the umbrella over both. This mod chases the
anti-boredom goal from two directions: a bottom-up emergent
Darwinian world that surprises on its own, and this top-down
director. Neither should smother the other.

GAMEPLAY IS WHAT THE PLAYER FEELS, not what the simulation does. The
Darwinian world grinding away AI-vs-AI is the backdrop, not the
game. A change only counts as gameplay if a bored player, safe
behind their walls, would sit up: something coming for them, a
choice that costs them, stakes that rise. Trait genomes drifting in
a camp across the map is simulation the player never sees; a war
band massing on their gate is gameplay. Apply the bored-player test
to every feature: would they notice, and would they care? If not,
it is scope creep, no matter how clever the simulation.

Two things in the table carry the priority:

- ROW ORDER. Rows are ranked by how much closing the gap improves
  fun and lifelikeness, highest at the top. This ranking is a
  proposal; reorder it freely.
- CLASS. "Gameplay" rows are features that change how the world
  plays and feels. "Polish" rows are presentation and niceties that
  wait until the gameplay above them has earned them.

Priority (how much a pillar matters) is a SEPARATE axis from score
(how done it is). A high-priority, low-score row is exactly where to
work next: it matters a lot and is barely built.

FOCUSED COMPLETION (operator-locked 2026-07-07). Drive ONE item to
10/10 at a time. Take the highest-priority row that is not yet
10/10, do the focused work to FINISH it, then move to the next. No
scope creep, no jumping between rows, no inventing features that are
not already decided in this file. The job is to flesh out and
complete what is already documented, one pillar at a time, until
every row reads 10/10. When in doubt about what to build, the
answer is: the next thing that moves the current top item toward
10/10, nothing else.

Priority order (top = do first), and UP NEXT is the top row below
10/10. RERANKED 2026-07-10 (operator call): ranked by what keeps
the player playing hour after hour, so the content that gives more
to do, more to discover, more to learn, and the loot chase ranks
above everything else:

1. Settlement upgrades (incremental, a resource sink)  <- UP NEXT (operator: max priority 2026-07-11; "i like to build")
2. More to do (ecosystem-generated work)
3. Quality system (higher quality, lower chance)
4. More to discover (named uniques)
5. More to learn (rare people and knowledge)
6. Storyteller / director
7. The horde (adaptive pressure) = the storyteller's Mario Kart layer
8. The act repertoire
9. Faction personality / evolution
10. AI-vs-AI war
11. Fight for control / predation
12. Factions can be destroyed
13. Town growth
14. No cheating
15. Player joins the ecosystem
16. Chronicle / in-game narration (Polish)

The storyteller (6) and the horde (7) are one job: the horde IS the
storyteller's Mario Kart layer, so finishing the storyteller means
finishing both its layers to live-verified 10/10, the Randy
incursions AND the Mario Kart horde.

## Scoring discipline

WHAT 10/10 MEANS. 10/10 is the ultimate bar: shipped on the Steam
Workshop and used by 1000 players for 10 years without any bug
fixes, everybody happy, everything working perfectly. That is the
definition for EVERY row.

A score reads how far this pillar's CODE is from that bar right now:
how complete, robust, bug-free, and ready to ship and survive a
decade of zero-fix use. Rate the code as it stands; re-rate when it
changes.

## Status

Baseline scored 2026-07-04 against the vanilla game; live
observations dated inline. Ordered by priority (top = most
important for fun), which is independent of the current score.

| Pillar | Class | Score | Where it stands now, and next | What 10/10 looks like |
|---|---|---|---|---|
| Settlement upgrades (incremental, a resource sink) | Gameplay | 0/10 | DESIGNED 2026-07-11 (operator vision, MAX PRIORITY: "i like to build"; model State of Decay's facility system but MORE complex in upgrade TYPES and LEVELS per type, operator-locked; no complicated UI), plan in docs/plans/2026-07-11-settlement-upgrades.md, nothing built. MULTI-TRACK PER-STRUCTURE UPGRADES: every placed structure carries several independent upgrade tracks (Reinforce = hit points on everything, Expand = storage capacity, Spikes = melee attackers bleed on walls and gates, Precision = better quality-tier odds for items crafted at that work prop; catalog grows: Comfort, Insulate, Watch, Efficiency, Secure, Yield staged behind effect-path research), each track unbounded levels with diminishing benefits and rising, diversifying REAL costs (each structure's own repair resource, consumed on click), skill prerequisites per level band, all through a REAL "upgrade" option in the game's right-click action menu (operator: real modding, not janky modding; the build-menu-marker idea was rejected). Architecture: per-structure state in a seed-keyed sidecar owned by the C# shim, C# Harmony patches on the game's stat reads (hit points, capacity, melee-hit) and on the action menu (population, click dispatch, label), Rust keeping ops and the tie-ins to the mod's own systems (Precision feeds quality's craft odds; Secure will resist the mod's own theft acts). Research done 2026-07-11: hit points and capacity reads pinned, the menu chain pinned (population, dispatch, label), structures have stable saved ids and never stack so per-instance state is safe. Task 1 gate: the hit-point postfix + sidecar surviving a save round-trip, probed live before anything else. | The player ALWAYS has more upgrades to do: dozens of structures x several tracks x deep levels, every purchase a real material sink with visible payoff (walls that hold, chests that swallow more, benches that craft finer things, spikes that bleed the horde), gated by skills, priced to scale with wealth, all through the game's own menu, and eventually the same tracks open to AI camps. |
| More to do (ecosystem-generated work) | Gameplay | 0/10 | DESIGNED 2026-07-10 (operator-picked from the content brainstorm), nothing built. The factions already generate events; this makes them generate WORK for the player: a bounty on the rival leader who keeps raiding them, a ransom for a prisoner, pay to join their war, an escort for their caravan. Nothing scripted: every offer is read off what the factions are already doing (their wars, grudges, prisoners, caravans), so the simulation writes the work and a long save never runs out of it. Offers arrive in-world (a messenger at the gate, a chronicle line) and pay in real goods from the hirer's real stores, per the no-cheating boundary. THE WORK BOARD (operator-locked 2026-07-10): the player must be able to see, in-game, a list of every open offer and what it pays; the chronicle announces, the board lists. Rides the act repertoire (the acts create the situations that create the work) and faction memory. Next: the bounty arc (offer, hunt, kill, collect) plus the board, planned task by task in docs/plans/2026-07-10-bounty-arc.md. | Factions offer the player real work generated by the live ecosystem; the player completes it and is paid from real stores; refusing or failing has consequences; and offers never run out because the world never stops producing grudges. |
| Quality system (higher quality, lower chance) | Gameplay | 0/10 | DESIGNED 2026-07-10, operator-locked ECOSYSTEM-WIDE: the whole world plays the loot game, not just the player. Every item rolls a quality and higher quality is rarer. Three hooks: the EDGE rolls quality (who sends the band sets the odds: refugees carry worn junk, warbands decent gear, military remnants the top of the ladder, and escalation means later arrivals roll better, so the storyteller's incursions are also the loot table); HANDS roll quality (a crafter's skill sets the odds when something is made, so a master crafter is a rare person worth fighting over); and the WORLD values quality (thieves target the good blade, trade prices scale with it, a warlord carries his camp's best rifle, the chronicle can say who holds what). RESEARCH DONE 2026-07-10 (findings + design in faction-war.md "The quality system"): the game has NO per-item quality field and stacks merge by item type, so quality tiers MUST be distinct item types, which is also the vanilla-grain answer (the type definition carries every stat and the price; tier variants ship as story XML exactly like the work board's quests; the game's own item factory is callable for edge-only swaps, inside the no-cheating boundary). FULL TIER SYSTEM BUILT 2026-07-10 (code-landed, gen41; design in faction-war.md "The quality system"): Factorio naming (Normal, Uncommon, Rare, Epic, Legendary, uniques above), covering ALL 38 vanilla weapons and armor pieces via a DRY generator (scripts/generate_quality.ps1, 456 files) whose knobs are per-tier MULTIPLIERS on the base stats (stat 1.1/1.2/1.35/1.5, price 2/4/8/16, recoil down), each tier shipping 3 jittered siblings sharing one display name so two Rare rifles usually differ (true per-item stat ranges are impossible: stats live on the type). Every weapon and armor piece an edge band carries rolls a tier by sender odds (military best, raiders lower), swapped net-zero into the spawned hand; edge-only, no world-loot entries. quality_status reports loads, swaps per tier, traders rolled, last swap. Same day (gen42): the SHOP rolls too (vanilla roving traders are ambient arrivals, so their stock rolls tiers once when first seen; our own vendor act stays honest and sells what real stores hold), and the ACTS VALUE QUALITY (every act that loads loot or payment now grabs the most valuable matching stack first: theft, robbery, predation, trade payment, couriers, vendor wares). HANDS ROLL QUALITY built 2026-07-11 (gen43): every craft in the game funnels through one entry, paired hooks capture the recipe and the crafter, and the output rolls a tier with skill-scaled odds (a novice never crafts Legendary; a master's hands are worth fighting over); AI camps craft through the same entry, so skilled camps field better gear over time; hooks verified installing live. EQUIP-BEST ANSWERED 2026-07-11 (decompile, no build needed): the AI already wields its highest-scoring carried weapon and the scores are exactly the stats our tiers raise (melee = the type's damage; ranged = damage-per-second from type stats), so whoever holds tiered gear uses it automatically: the dangerous-looking enemy IS the loot. UNWATCHED: needs the story restart, then armed arrivals, trader stock, and a skilled crafter's output watched tiered, priced up, lootable. Still open: other gear categories; knob tuning after the first play session. | Every item has a quality the whole ecosystem recognizes: rarer is better, sources differ in their odds, factions hoard, steal, and price by it, and the player can chase a specific good item they know sits in someone else's hands. |
| More to discover (named uniques) | Gameplay | 0/10 | DESIGNED 2026-07-10, nothing built. Named one-of-a-kind items that enter ONLY with incursions, matched to the sender: military remnants carry hardware nothing on the map has, the mysterious stranger carries the oddity, the mega-horde leaves behind what its off-map victims dropped. The scariest events become the best loot sources, so dread and desire are the same emotion and a bored safe player starts WANTING the dangerous thing to cross the edge. Uniques live in the ecosystem, not in a lootbox: the chronicle mentions the warlord carrying the named rifle; you see it, you want it, you go take it. Sits on the quality system (a unique is the near-impossible roll) and the incursion set (already built). FIRST SLICE BUILT 2026-07-10 (code-landed, gen40): The Colonel's Rifle ships as story XML (a storied marksman's rifle above the fine tier, price 400, edge-only), enters with a military remnant band (20 percent per band until it enters, handed to the band's leader), the chronicle announces it, and a seed-keyed sidecar (the genome memory's pattern) guarantees ONCE PER SAVE across reloads; unique_status op reports loaded/entered/carrier. THE LEGEND HAS AN ADDRESS (built 2026-07-11, gen44): once the rifle has entered, a slow whole-map scan tracks who holds it (a named carrier of a camp, or a camp's stores), the chronicle announces every change of hands ("The Colonel's Rifle is with X of Y"; "no one knows where The Colonel's Rifle is" when it drops out of sight), and the last known holder persists in the same sidecar so reloads stay quiet; unique_status reports the holder. UNWATCHED: needs the story restart, then a military arrival watched carrying it in and the chronicle following it. | Uniques enter at the edge with the incursions that fit them, live visibly in faction hands via the chronicle, and can be taken; a spectator can name the storied items on the map and who holds them. |
| More to learn (rare people and knowledge) | Gameplay | 0/10 | DESIGNED 2026-07-10, nothing built. Camp capability gated behind rare people and rare knowledge: a recruit who is a surgeon, a book that teaches a recipe, a specialist a faction guards jealously. People are already this mod's loot (recruiting, absorption, press-gangs); this makes every arriving band worth inspecting for WHO is in it, not just what it carries. Fuses with the quality system through crafting: the master crafter IS the walking loot. Needs research into the game's skill and recipe model (what is teachable, what can gate). Next: that research. | Rare specialists and learnable knowledge exist in the world, enter at the edge, can be recruited, captured, or traded for, and visibly unlock things a camp could not do before; camps, AI and player alike, differ in capability by who they hold. |
| Storyteller / director | Gameplay | 2/10 | RATED 2/10 against the ship-bar (2026-07-07, from a code read): a working prototype, an ocean from a decade of zero-fix flawless use. The dread loop and the signal payoff run live, but there are ZERO automated tests, most payoffs (raiders, military, settlers, mysterious, refugee, mega-horde) have never been watched end-to-end, the edge-spawn generator is hours old and unverified (it can spawn on impassable ground, spawn a 0-member band, or spawn on top of a camp), the manual handle juggling could leak or crash over 10 years, and it disables game systems whose long-run interactions are unknown. SHIPPED and armed live (2026-07-06). The director is a real module (Randy Random, config-driven and swappable, tweakable live), pacing drama on an irregular cadence with a survivable guard. THREE on-map rules: the horde, the traveling vendor (a real camp sells wares to a camp or the player), and strangers of unknown intent (a real arriving group with a hidden rolled outcome: join, share goods, attack, leave, or shake the camp down for tribute). The VISION now reaches past on-map events to OFF-MAP INCURSIONS (backlog below), the real anti-boredom engine and the way a comfortable player stays challenged. The full incursion set is BUILT (code-landed 2026-07-07): the dread loop plus mega-horde, raiders, military remnants, settling faction, mysterious stranger, refugee wave, and signal, all in incursion.rs/stranger.rs. FIRST LIVE FIRING watched 2026-07-07 (deployed via gen29 hot reload): the dread loop runs on its own (dread signs scheduling payoffs, a false alarm, and the off-map SIGNAL completing its full arc, chronicle line and all). So the loop mechanism and the signal payoff are LIVE-VERIFIED. Everything else is still code-landed only, unwatched: the aggressive-stranger/raiders branch (a roll came up but no roving group was near the edge to convert, so the hostile-attack foundation stays unproven), military remnants, the settling faction, the mysterious stranger, the refugee wave, and the mega-horde. Status 2026-07-08: the RANDY layer is now FEATURE-COMPLETE. The edge-spawn build gap is CLOSED: ALL off-map arrivals, raiders, military, strangers, refugees, mysterious, settlers, now spawn a real band at the map edge (the game's own SpawnAmbientLooters) and march in, per the design lock, so the edge is the single fresh people+loot faucet and nothing depends on an ambient group being nearby (incursion::spawn_band_at_edge + common::march_band_to). Scavenge (act repertoire) fixed the same day: it was throwing every scan on the volatile props list and never launched. NOT on the table: making the random incursions adaptive to the player (Cassandra, rejected 2026-07-07); the leader-check is the horde's job and the incursions stay random. |  The storyteller tells the STORY of a pocket of survivors on an island in a collapsing world, and its greatest tool is the UNKNOWN BEYOND THE MAP EDGE. The finite map gets solved and safe; the edge is the infinite unknown, and the director decides what crosses it and when: a traveling mega-horde flattening settlements in its path, off-map warbands and scavengers, military remnants killing everything they pass, whole factions arriving to settle, a mysterious stranger whose meaning is never fully learned. Each is telegraphed with dread (smoke on the horizon, refugees speaking of soldiers to the north) and escalates as the map goes quiet. They come for the PLAYER, so a bored player behind safe walls never coasts: something worse is always out there. |
| The horde (adaptive pressure) | Gameplay | 2/10 | RATED 2/10 against the ship-bar (2026-07-07): the pack has NEVER been watched massing on the alpha, so its whole runtime behavior is unverified; it is code-landed and armed (v6 shim loaded), no tests, a young feature. Operator-locked as the counterweight to the alpha settlement (the Mario Kart rule: first place should hurt). The biggest camp draws scaled zombie packs, tiered by size, so growth is a real tradeoff and a snowballing winner must keep winning sieges or bleed. SHIPPED, and now the storyteller's FIRST RULE: the director paces it on Randy's cadence (its old five-minute self-timer is gone), and the director is confirmed watching the alpha (The Well-Regulated Bears, 51) live 2026-07-06. The pack draws from the game's own spawner, at most two roam at once, carried by bridge ABI v6. The game WAS restarted 2026-07-07 with the v6 shim loaded (deploy verified: gen29 Rust + fresh shim in the mod folder, load confirmed in Player.log), so the spawner is now ARMED. But no pack has been watched massing yet in the running game. Next: watch a pack actually mass on the alpha and the walls answer. | The alpha camp visibly draws stronger and more frequent zombie packs as it grows; the biggest faction is not automatically the safest; success buys danger, watched over real time. |
| The act repertoire | Gameplay | 3/10 | RATED 3/10 vs the ship-bar (2026-07-07, code read): a big surface (six act modules) with lots watched live, but scavenge has NEVER fired, murder has no successful kill, trade delivery/payment is unseen, ambition-war outcomes are pending, zero tests, and every act rides fragile game-reflection. The multidimensional-factions mandate: factions do many things (scavenge, steal, trade, rob, murder, extort, raid), each chosen from who the faction is (genome plus franchise vote) and what its situation calls for, with consequences flowing through the game's own systems. SHIPPED and mostly watched live 2026-07-05: theft (every branch observed, and a caught thief organically ignited a war through the vanilla caught-stealing path), trade (the first real AI-to-AI exchange; launch seen, delivery and return not yet), the ambition war (launches seen, outcomes pending), robbery (a full arc observed), moving extortion into the franchise vote, and suing for peace (observed, including a ceasefire re-broken then holding). Murder shipped; a plot was observed but no successful kill yet. Scavenge is now BUILT too (code-landed 2026-07-07, scavenge.rs): an expansionist franchise vote sends a party to loot the nearest ownerless building (found by walking the game's own public props list, PropManager.AllProps, no shim and no restart), carries the goods home in real packs, and the voters learn expansionism from the haul. The party SIZE is emergent (eager yes-voters, capped by what the camp can spare and by the loot found). Next: all acts are code-landed; what remains is VERIFICATION. Watch the not-yet-seen arcs live: a scavenge party walk-loot-return, trade delivery and payment, a successful murder, ambition-war outcomes feeding learning. | Every act including scavenge runs live, each act's full arc and its learning feedback observed, and a spectator can tell camps apart by which acts they keep choosing. |
| Faction personality / evolution | Gameplay | 4/10 | RATED 4/10 vs the ship-bar (2026-07-07, code read): the best-built pillar and top of this board. Deterministic seeding, learning, crash-safe atomic persistence with a schema_version, and the memory was live-verified again today (247 survivors restored). Held below higher by: ZERO tests despite being the most testable code here, and a corrupt save file silently wipes all memory with no error. LIVE 2026-07-05: personality is visible in behavior with zero hardcoded type checks. Each faction has a trait genome (aggression, expansionism, defensiveness, guile), and decisions are a per-survivor franchise vote: Normal camps let everyone vote and drift toward whoever joins; Looter camps let only core looters vote and stay ruthless under conquest. Looter camps vote near-unanimously to raid and steal; Normal camps split and become the traders, all emergent from around 200 individual votes. All four traits now have live learning loops, and genomes persist across restarts (a seed-keyed sidecar restored 253 survivor genomes on one reload). Next: nothing to build here for its own sake; this engine only shows up through the visible acts it drives (Looters raiding, Normal camps trading), so its progress is the act repertoire's. Deeper genome mechanics (per-survivor heredity blending, trait drift) are simulation the player never sees, and are CUT as scope creep per the bored-player test above. | Normal and Looter settlements are recognizably different actors whose war, growth, and dealing choices fit their identity and shift as their people live, die, learn, and are absorbed, visible to a spectator without reading code. |
| AI-vs-AI war | Gameplay | 3/10 | RATED 3/10 vs the ship-bar (2026-07-07, code read): clean, well-guarded, and the full lifecycle was watched once, but it hangs off the game's OnMemberDied signature and Killer/Community field names (a game patch silently kills it), zero tests, never run at scale. LIVE 2026-07-05: the full war lifecycle watched in one day with no player involved. Ignition organic (a caught thief tripped the game's own ladder) and by personality (two ambition wars). Sustain: the two-way revenge loop fired both directions. End: a camp bled to 3 of 10 surrendered by unanimous peace vote; the first ceasefire re-broke while the winner's squads still fought (the game's witnessed-attack path re-declared) and the second held once blades stopped. Next: allies dragged into an AI war watched live; ceasefire decay and re-escalation over days. | AI factions declare, wage, and settle wars with each other without the player: raids launched both ways, ceasefires and surrenders happen, allies drag each other in, verifiable by spectating two camps. |
| Fight for control / predation | Gameplay | 3/10 | RATED 3/10 vs the ship-bar (2026-07-07, code read): extinction/absorption and map consolidation were watched live, but the loot-carry path has only ever been watched finding EMPTY stores (never verified moving real goods), a magic 500-item cap and hand-tuned threshold, zero tests, fragile reflection. LIVE 2026-07-05: predation observed twice. A victor beat a camp to a few survivors and consumed it: survivors absorbed, the loser extinct and its genome dropped from the pool; the map consolidated 22 to 20 settlements in one day with no player and no ops. The first war traced all the way back to a caught burglary. Both loot passes ran but found bare stores (honest zeroes). Next: a stockpile-stripping watched with real goods carried home; ground-drop loot from the war dead (needs the rect-enumeration infra). | Wars are ABOUT something: people and goods transfer on victory, territory feeds growth, and left to run the map consolidates until one faction controls it. |
| Factions can be destroyed | Gameplay | 3/10 | RATED 3/10 vs the ship-bar (2026-07-07, code read): rides predation's extinction path, which was watched (a camp consumed and stays dead), but shares all of predation's caps (zero tests, fragile reflection, unscaled). LIVE 2026-07-05: extinction observed. A camp was consumed after losing the war its own caught thief started; survivors absorbed, the faction dead, and with the repopulator disabled it stays dead. Darwinian consolidation, no player. Next: the husk seen as a claimable power vacuum (vanilla roamer reclamation exists; watch one happen). | A faction that loses its people or its base is gone or absorbed: no resurrection, its territory claimable, visible on the map as a power vacuum. |
| Town growth | Gameplay | 3/10 | RATED 3/10 vs the ship-bar (2026-07-07, code read): the flywheel was watched end to end, but the recruit scan threw a caught TargetInvocationException in the running game TODAY (a real live bug), the fence-line was never watched standing complete, attrition and war-posture growth are unbuilt, and there are zero tests. LIVE 2026-07-05: the growth flywheel closed end to end. Beds-full, fed camps plan a fenced annex with gate and shack; their own builders consume the records and build from real hauled materials; shacks completed (a shack adds 2 beds, not 1) and the new beds were filled by real recruits in the same session (press-gangs at five looter camps, refugees welcomed at Normal camps). Next: a fence line watched standing complete around its annex, attrition realism over days, growth tied to war posture. | The settlement itself grows, more structures and more people, from a non-cheating economy: built by real hands from real hauled materials, peopled by real arrivals, its rate bound to food and war posture. |
| No cheating | Gameplay | 3/10 | RATED 3/10 vs the ship-bar (2026-07-07, code read): focused and verified (two of three cheats disabled, watched live via a clean AddInjury prefix + spawn suppressors), but two operator boundary calls are still open, disabling core game systems has unknown 10-year interactions, and there are zero tests. LIVE 2026-07-05: two of the three cheats are disabled in the running game. The in-place repopulator (people from nothing) is off, and raider spawn-point refills are suppressed by a conditional that stops only refills while first spawns still populate the world; a suppression counter tracks it. Cheat three (spawn-time arrival gear) is accepted under the boundary that the world may feed the map at its edge but a town may not conjure in place. That edge boundary is now a DELIBERATE FAUCET, not just a tolerated exception: the storyteller's off-map incursions are GENERATED at the edge (see the incursions backlog design lock), and because those spawned bands carry real gear and real people, edge-spawning is the sanctioned way NEW loot and population enter this closed world (repopulator + refills off). So off-map generation and the no-cheat boundary are the SAME mechanism seen from two pillars. Next: the two operator boundary calls (trader-party and chicken refills, currently stopped as a side effect); watch the suppression counter climb. | Every faction person walked onto the map or was recruited from it; every weapon and meal came from loot, trade, crafting, or harvest; destroying a faction's people or stores actually weakens it. |
| Player joins the ecosystem | Gameplay | 2/10 | RATED 2/10 vs the ship-bar (2026-07-07, code read): the acts CAN target the player, but landing on the player camp is almost entirely unverified live, the STEAL-against-player carve-out is unbuilt, and there are zero tests. Mostly promise, little watched. Operator-locked full symmetry: every act the factions do to each other they can do to the player, with two carve-outs. STEAL stays out for now (not ready yet; eventually), and a beaten player camp is never absorbed because flipping the player's community would likely corrupt the save (wars still cost the player people and goods the normal way; only the extinction-absorb step skips the player). Trade caravans at the gate, road robberies, hunger raids, ambition wars, assassinations of the player's leader, and AI camps suing the player for peace are all ruled in and ride the same engines as the AI-vs-AI acts. But landing ON the player is largely unverified live so far. Next: watch each act happen to the player camp (a caravan at the gate, a robber on the road, an assassin at night, a camp suing for peace). | Each act is observed happening to the player's camp; the player trades with, defends against, and makes peace with the factions on the same terms the factions use with each other. |
| Chronicle / in-game narration | Polish | 3/10 | RATED 3/10 vs the ship-bar (2026-07-07, code read): simple (30 lines) and verified posting live, but a known vanilla "You are at war" banner still leaks on the caught-thief path (a deferred bug, so not "everything works perfectly"), and there are zero tests. The living world made visible in-game: dramatic beats (wars declared, surrenders, extinctions, caught thieves, robberies, assassinations) post to the game's own status banner, phrased as word spreading, with only PUBLIC events shown (a clean getaway stays secret). SHIPPED and verified live 2026-07-05. This is presentation, so it ranks below the gameplay above it. Known quirk: the vanilla banner still says "You are at war" for the organic caught-thief path; a patch is deferred. Next (low priority): silence that last vanilla-banner case; richer phrasing as more acts ship. | The player can follow the world's story from in-game messages alone: who is at war, who fell, who robbed whom, phrased naturally, with no false or leaking lines. |

## Storyteller backlog: off-map incursions (2026-07-06 vision)

The map is a finite box a good player eventually solves; the EDGE is
the border of an infinite unknown, and the storyteller's biggest
tool is deciding what crosses it. These are the incursions from
beyond, the real anti-boredom engine: they come FOR the player, they
are telegraphed with dread but not explained, and they ESCALATE as
the map goes quiet and the outside notices this pocket of life.
Ordered by build feasibility; most reuse tech already in the tree.

DESIGN LOCK (operator, 2026-07-07): every off-map incursion is
GENERATED at the map edge from the undefined beyond, spawned fresh
by the mod, NOT repurposed from an ambient local group. The current
raiders/military are WRONG: they hijack a wandering refugee band
that happens to be near a camp and flip it hostile, so they no-op
when none is near and they are thematically backwards. They must be
rebuilt to spawn a real band at a map-edge spawn point (the game's
own AmbientEnemySpawnPoint.SpawnEnemyGroup creates a hostile
Community; a list of these spawn points is reachable over the
bridge, the same way the horde reuses the zombie spawner). This
edge-generation is ALSO a first-class LOOT + PEOPLE FAUCET: a
spawned band arrives carrying real gear and real members, so
edge-spawning is the sanctioned way to inject NEW loot and
population into the world, the "world feeds the map at its edge"
boundary (see the No-cheating pillar). With the repopulator and
raider refills disabled, the off-map edge is HOW this otherwise
closed world stays supplied with fresh goods and people.

- Traveling mega-horde: a wall of the dead crossing the map on a
  line, flattening every settlement in its path (not aimed at the
  biggest camp; a blind force of nature). The dread is WHY it moved:
  something worse off-map drove it here, and the player never learns
  what. Extends the horde's own spawner.
- Off-map raiders and scavengers: a hostile warband that raids and
  leaves or attacks and stays; scavengers who strip what they can
  and vanish. Hostile groups spawned at the edge, pointed inward
  (the aggressive-stranger shape).
- Military remnants: government or special-forces teams crossing on
  a mission, killing everything they see, purpose never explained.
  A themed hostile group.
- The mysterious stranger: a lone figure with a hidden meaning the
  player never fully learns. A special variant of the strangers
  system.
- Off-map signal: a radio broadcast luring factions and the player
  toward the edge. The plague half (a group carrying a worse strain
  that may doom the camp) is DROPPED, operator call 2026-07-07: it
  conflicts with the no-infections directive (2026-07-04,
  infection.rs), which stands.
- A settling faction: an off-map offshoot founds a new camp,
  rewriting the balance and keeping the map from ever being fully
  known. The hard one: needs creating a community and a base from
  nothing (uncertain feasibility).
- Refugees fleeing an off-map catastrophe: a wave of real survivors
  running from something beyond the edge that the player only hears
  about secondhand, which foreshadows what is coming for THEM next.
  Extends the arriving-groups inflow the strangers already read.
- Dread signs: smoke on the horizon, refugees speaking of what is
  coming. Chronicle lines that plant an incursion before it lands.

ALL of these are BUILT (code-landed 2026-07-07): the dread loop
(telegraph, delay, false-alarm or foreshadowed payoff, escalation)
in incursion.rs, with payoff kinds for the mega-horde, off-map
raiders, military remnants, the settling faction (claims a dead base
via the game's own reclamation), the mysterious stranger, the
refugee wave (arms a foreshadow so what they fled arrives on the
next sign), and the off-map signal.

LIVE STATUS (2026-07-07, deployed and watched in a running game):
the dread loop and the SIGNAL payoff are LIVE-VERIFIED. Watched
firing on their own: dread signs scheduling payoffs, a raiders roll
(that found no roving group near the edge, so nothing crossed), a
false alarm, and the signal completing its full arc. Everything else
is still code-landed only, UNWATCHED: raiders/aggressive actually
reaching and attacking a camp (the hostile-attack foundation),
military remnants, the settling faction, the mysterious stranger,
the refugee wave, and the mega-horde. The horde spawner is now armed
(v6 shim loaded on the restart) but no pack has been watched. What
remains is VERIFICATION: one watched aggressive arrival plus more
playtime for the escalation-gated payoffs unlocks the scored credit
for the rest of the set.

## What a fast-bored player enjoys, and what to build first

The design target is a player who gets bored FAST (the operator).
What un-bores them is not any one big threat; it is the LOOP of not
knowing what is coming:

1. DREAD. A quiet stretch breaks with a sign: smoke on the horizon,
   a refugee raving about soldiers to the north. What is coming?
   The anticipation is half the fun, sometimes more than the payoff.
2. UNCERTAINTY. Mega-horde? Raiders? A lone stranger? A plague?
   Nothing? They cannot predict it, so they cannot get comfortable.
3. VARIED PAYOFF. It lands, and it is never the same thing twice.
4. LINGERING MYSTERY. They never fully learn the meaning: why the
   horde moved, what the stranger wanted, who the soldiers were.
5. ESCALATION. It grows bigger and stranger as the map goes quiet,
   so safety never sets in.

Build-order consequence: the SETUP and the MYSTERY are worth as much
as the threats. Telegraphing and the mystery events are not garnish;
they ARE the fun for a fast-bored player, because they create the
not-knowing. One dramatic incursion with no dread before it and no
mystery after is a fireworks show; the LOOP is the game. So the
highest-enjoyment build is the dread loop itself (telegraph, then
payoff) plus two or three varied incursions to fill it, not one big
threat in isolation.

Value ranking (raw player impact, 2026-07-06; build status 2026-07-07):

| Incursion | Value | Build status |
|---|---|---|
| Traveling mega-horde | 9/10 | BUILT (code-landed); spawner armed on the restart, no pack watched yet |
| Off-map raiders / warband | 8/10 | BUILT (code-landed); rolled live but found no group near the edge, hostile-attack path still UNWATCHED |
| Military remnants (special forces) | 8/10 | BUILT (code-landed); UNWATCHED, gated behind 4 resolved payoffs |
| A settling faction | 7/10 | BUILT (code-landed); UNWATCHED |
| The mysterious stranger | 6/10 | BUILT (code-landed); UNWATCHED |
| Refugee wave fleeing something | 5/10 | BUILT (code-landed); UNWATCHED |
| Off-map signal | 5/10 | BUILT + LIVE-VERIFIED 2026-07-07 (fired and completed its full arc); plague half dropped (no-infections directive stands) |
| Dread signs (the telegraph) | 4/10 raw, but core of the loop | BUILT + LIVE-VERIFIED 2026-07-07 (the loop watched scheduling and resolving payoffs on its own) |

The two foundations under the top three (the horde firing, a hostile
group actually attacking rather than fleeing) are both unverified.
One game restart plus watching one aggressive stranger reach a camp
unlocks the 8s and the 9 at once.
