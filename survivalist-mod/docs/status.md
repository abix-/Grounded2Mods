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
horde and the storyteller raise the stakes on whoever is winning,
but their target is tension and drama, not annihilation: an alpha
camp under a heavy siege should have to fight, adapt, or give
ground to survive, not simply be deleted. Brutal but survivable is
the line.

## How we prioritize

Feature first, polish last. What ranks highest is whatever makes
the game more FUN, more lifelike, and less boring, because that is
what decides whether people keep playing or quit: a world that
turns repetitive, predictable, and static gets abandoned. UI work,
cosmetics, and nice-to-haves come AFTER the gameplay that makes the
world feel alive.

The guiding star for "fun" is the RimWorld storyteller model: a
solid base simulation plus a director layer on top that watches the
world's variables and injects events and pressure so the game stays
unpredictable and dramatic, scaling to how well things are going.
This mod chases the same anti-boredom goal from two directions at
once: a bottom-up emergent world that surprises on its own (the
Darwinian simulation running whether or not the player watches) and
a top-down director that paces the drama (the horde today, a fuller
storyteller later). Neither should smother the other.

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

## Scoring discipline

A score moves ONLY on a live-verified change: something watched
happening in a running game, not merely code that landed. Update
the score AND its status cell in the same commit as the
verification. Never move a score on code-landed-only work.

The six original pillars below carry their operator-set scores
unchanged. Rows marked "(proposed)" are new to this file and await
YOUR live-verified score; the evidence is filled in so you can set
them quickly.

## Status

Baseline scored 2026-07-04 against the vanilla game; live
observations dated inline. Ordered by priority (top = most
important for fun), which is independent of the current score.

| Pillar | Class | Score | Where it stands now, and next | What 10/10 looks like |
|---|---|---|---|---|
| Storyteller / director | Gameplay | 4/10 (proposed) | SHIPPED and armed live (2026-07-06). The director is a real module (Randy Random storyteller, config-driven and swappable, tweakable live), reading the world and pacing drama on an irregular cadence with a survivable guard so it never stacks pressure. THREE rules registered: the horde (rule one), the traveling vendor (a real camp's surplus sold by a real caravan), and strangers of unknown intent (real arriving groups, hidden rolled intent: friendly, aggressive, or wary). Confirmed armed live via the status readout: seeded from the world seed, alpha selected (The Well-Regulated Bears, 51), first event scheduled. NOT yet watched: an event actually firing, the horde arming (needs the game restart for the v6 shim), and whether the aggressive-stranger branch attacks rather than flees. Next: watch the vendor and strangers fire, restart to arm the horde, confirm aggressive strangers bite. | A director sits above the simulation, reads how well things are going and how long since anything happened, and paces events and pressure so the game stays unpredictable and dramatic, scaling to success, without smothering the emergent world underneath. |
| The horde (adaptive pressure) | Gameplay | 3/10 (proposed) | Operator-locked as the counterweight to the alpha settlement (the Mario Kart rule: first place should hurt). The biggest camp draws scaled zombie packs, tiered by size, so growth is a real tradeoff and a snowballing winner must keep winning sieges or bleed. SHIPPED, and now the storyteller's FIRST RULE: the director paces it on Randy's cadence (its old five-minute self-timer is gone), and the director is confirmed watching the alpha (The Well-Regulated Bears, 51) live 2026-07-06. The pack draws from the game's own spawner, at most two roam at once, carried by bridge ABI v6. But the pack itself has still NEVER run: it holds back until the first game restart loads the v6 shim, then arms. Next: restart to arm it, then watch a pack actually mass on the alpha and the walls answer. | The alpha camp visibly draws stronger and more frequent zombie packs as it grows; the biggest faction is not automatically the safest; success buys danger, watched over real time. |
| The act repertoire | Gameplay | 7/10 (proposed) | The multidimensional-factions mandate: factions do many things (scavenge, steal, trade, rob, murder, extort, raid), each chosen from who the faction is (genome plus franchise vote) and what its situation calls for, with consequences flowing through the game's own systems. SHIPPED and mostly watched live 2026-07-05: theft (every branch observed, and a caught thief organically ignited a war through the vanilla caught-stealing path), trade (the first real AI-to-AI exchange; launch seen, delivery and return not yet), the ambition war (launches seen, outcomes pending), robbery (a full arc observed), moving extortion into the franchise vote, and suing for peace (observed, including a ceasefire re-broken then holding). Murder shipped; a plot was observed but no successful kill yet. Scavenge is BLOCKED on a game-side change (consumed camps vanish rather than leaving lootable husks, and enumerating town loot needs a shim primitive plus a restart). Next: unblock scavenge, and watch the not-yet-seen halves (trade delivery and payment, a successful murder, ambition-war outcomes feeding learning). | Every act including scavenge runs live, each act's full arc and its learning feedback observed, and a spectator can tell camps apart by which acts they keep choosing. |
| Faction personality / evolution | Gameplay | 7/10 | LIVE 2026-07-05: personality is visible in behavior with zero hardcoded type checks. Each faction has a trait genome (aggression, expansionism, defensiveness, guile), and decisions are a per-survivor franchise vote: Normal camps let everyone vote and drift toward whoever joins; Looter camps let only core looters vote and stay ruthless under conquest. Looter camps vote near-unanimously to raid and steal; Normal camps split and become the traders, all emergent from around 200 individual votes. All four traits now have live learning loops, and genomes persist across restarts (a seed-keyed sidecar restored 253 survivor genomes on one reload). Next: per-survivor heredity blending on absorption, and aggression shifts from real famine (no famine on the map yet). | Normal and Looter settlements are recognizably different actors whose war, growth, and dealing choices fit their identity and shift as their people live, die, learn, and are absorbed, visible to a spectator without reading code. |
| AI-vs-AI war | Gameplay | 8/10 | LIVE 2026-07-05: the full war lifecycle watched in one day with no player involved. Ignition organic (a caught thief tripped the game's own ladder) and by personality (two ambition wars). Sustain: the two-way revenge loop fired both directions. End: a camp bled to 3 of 10 surrendered by unanimous peace vote; the first ceasefire re-broke while the winner's squads still fought (the game's witnessed-attack path re-declared) and the second held once blades stopped. Next: allies dragged into an AI war watched live; ceasefire decay and re-escalation over days. | AI factions declare, wage, and settle wars with each other without the player: raids launched both ways, ceasefires and surrenders happen, allies drag each other in, verifiable by spectating two camps. |
| Fight for control / predation | Gameplay | 7/10 | LIVE 2026-07-05: predation observed twice. A victor beat a camp to a few survivors and consumed it: survivors absorbed, the loser extinct and its genome dropped from the pool; the map consolidated 22 to 20 settlements in one day with no player and no ops. The first war traced all the way back to a caught burglary. Both loot passes ran but found bare stores (honest zeroes). Next: a stockpile-stripping watched with real goods carried home; ground-drop loot from the war dead (needs the rect-enumeration infra). | Wars are ABOUT something: people and goods transfer on victory, territory feeds growth, and left to run the map consolidates until one faction controls it. |
| Factions can be destroyed | Gameplay | 8/10 | LIVE 2026-07-05: extinction observed. A camp was consumed after losing the war its own caught thief started; survivors absorbed, the faction dead, and with the repopulator disabled it stays dead. Darwinian consolidation, no player. Next: the husk seen as a claimable power vacuum (vanilla roamer reclamation exists; watch one happen). | A faction that loses its people or its base is gone or absorbed: no resurrection, its territory claimable, visible on the map as a power vacuum. |
| Town growth | Gameplay | 6/10 | LIVE 2026-07-05: the growth flywheel closed end to end. Beds-full, fed camps plan a fenced annex with gate and shack; their own builders consume the records and build from real hauled materials; shacks completed (a shack adds 2 beds, not 1) and the new beds were filled by real recruits in the same session (press-gangs at five looter camps, refugees welcomed at Normal camps). Next: a fence line watched standing complete around its annex, attrition realism over days, growth tied to war posture. | The settlement itself grows, more structures and more people, from a non-cheating economy: built by real hands from real hauled materials, peopled by real arrivals, its rate bound to food and war posture. |
| No cheating | Gameplay | 5/10 | LIVE 2026-07-05: two of the three cheats are disabled in the running game. The in-place repopulator (people from nothing) is off, and raider spawn-point refills are suppressed by a conditional that stops only refills while first spawns still populate the world; a suppression counter tracks it. Cheat three (spawn-time arrival gear) is accepted under the boundary that the world may feed the map at its edge but a town may not conjure in place. Next: the two operator boundary calls (trader-party and chicken refills, currently stopped as a side effect); watch the suppression counter climb. | Every faction person walked onto the map or was recruited from it; every weapon and meal came from loot, trade, crafting, or harvest; destroying a faction's people or stores actually weakens it. |
| Player joins the ecosystem | Gameplay | 4/10 (proposed) | Operator-locked full symmetry: every act the factions do to each other they can do to the player, with two carve-outs. STEAL stays out for now (not ready yet; eventually), and a beaten player camp is never absorbed because flipping the player's community would likely corrupt the save (wars still cost the player people and goods the normal way; only the extinction-absorb step skips the player). Trade caravans at the gate, road robberies, hunger raids, ambition wars, assassinations of the player's leader, and AI camps suing the player for peace are all ruled in and ride the same engines as the AI-vs-AI acts. But landing ON the player is largely unverified live so far. Next: watch each act happen to the player camp (a caravan at the gate, a robber on the road, an assassin at night, a camp suing for peace). | Each act is observed happening to the player's camp; the player trades with, defends against, and makes peace with the factions on the same terms the factions use with each other. |
| Chronicle / in-game narration | Polish | 6/10 (proposed) | The living world made visible in-game: dramatic beats (wars declared, surrenders, extinctions, caught thieves, robberies, assassinations) post to the game's own status banner, phrased as word spreading, with only PUBLIC events shown (a clean getaway stays secret). SHIPPED and verified live 2026-07-05. This is presentation, so it ranks below the gameplay above it. Known quirk: the vanilla banner still says "You are at war" for the organic caught-thief path; a patch is deferred. Next (low priority): silence that last vanilla-banner case; richer phrasing as more acts ship. | The player can follow the world's story from in-game messages alone: who is at war, who fell, who robbed whom, phrased naturally, with no false or leaking lines. |

## Pending migration (needs your go)

`faction-war.md` still holds the old scorecard at its top, and a
stale "5/10" line lower in that file (in the build-order section)
that contradicts the AI-vs-AI war score above. Until those are
removed, a score still lives in two places. I have NOT touched
`faction-war.md`. Once you confirm the (proposed) scores here, the
cleanup is: delete the scorecard from `faction-war.md`, neutralize
that stale line, and leave that doc as vision and design only, so
this file is genuinely the sole home for every score.
