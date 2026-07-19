# AI-Assisted Factorio: Mod Research

Status: research pass done 2026-07-18. Answers the research questions in factorio-vision.md.

## The short version

Nobody has shipped the partner described in the vision doc. The pieces all exist separately: an active academic benchmark where LLMs play the game, an abandoned proof-of-concept autonomous player, a chat-only Claude Code companion bridge, strong deterministic monitoring mods, and mature blueprint libraries. Every LLM project uses the same bridge shape: Lua mod or scenario inside the game, RCON to an external program, LLM outside the game. The working assumption in the vision doc held up.

## AI players and assistants

- Factorio Learning Environment (FLE): academic benchmark where LLM agents write Python programs against a game API in a REPL loop. Active (v0.4.3, April 2026), MIT, 882 commits. Benchmark-shaped: headless server clusters, not player-attended games. Best prior art for designing a command surface. https://github.com/JackHopkins/factorio-learning-environment and https://arxiv.org/abs/2503.09617
- factorio-ai-companion: closest to our vision on paper. Claude Code controls companions in Factorio 2.x via MCP and RCON. In reality only the bidirectional chat bridge works (Lua mod captures chat, RCON to a Node MCP server, into Claude Code); game state introspection and automated building are unimplemented future work. MIT, 9 stars, last release January 2026. Its chat bridge is a proven wiring diagram. https://github.com/lveillard/factorio-ai-companion
- AI Player mod: autonomous in-game player driven by a local model (LM Studio) through an external Python RCON bridge. 1.17K downloads, Factorio 2.0+. Abandoned; the creator said continuing was too much of a challenge, and multiplayer never worked. Confirms that full autonomous play is the hard road. https://mods.factorio.com/mod/ai-player
- Seen but not dug into: airi-factorio (computer vision plus LLM plays the game), FactorIA (crafting calculations and optimization suggestions), Personal Assistant mod, autopilot-mod.

## Monitoring mods

Deterministic detection is largely solved in-game. None of these act on what they find or talk to an LLM; they display.

- Bottleneck: indicator lights on machines showing working/stalled state. https://mods.factorio.com/mod/Bottleneck
- Bottleneck Analyzer: samples every crafting building, records which ingredient was missing when machines stall, aggregates percentages per item. Factorio 2.0. https://github.com/nickelbob/factorio-bottleneck-analyzer
- Assembly Analyst: select part of the factory, get a breakdown of what machines spend their time on. https://github.com/ClaudeMetz/AssemblyAnalyst
- Utilization Monitor Blargh: per-building utilization percentage over the last 60 seconds. https://mods.factorio.com/mod/UtilizationMonitorBlargh
- Factory Efficiency Tracker: uptime/downtime overlays per machine. https://mods.factorio.com/mod/factory-efficiency-tracker
- Production Statistics Monitor HUD: pinned production stats and ratios. https://mods.factorio.com/mod/production-monitor
- Instrumentorio: exports Factorio metrics to external monitoring. Proof that data can flow out of the game continuously. https://medium.com/expected-behavior/instrumentorio-custom-monitoring-for-factorio-or-anything-4c5d63fecdd0

Takeaway: the mod API can read machine status, stalls, missing ingredients, and utilization. Detectors for the vision doc's monitoring list are proven territory.

## Bridges between the game and external programs

- Verified: Factorio mods are sandboxed Lua with no network access. Every LLM project above runs an external program and talks to the game over RCON.
- Caveat from the companion project: RCON requires running the game in multiplayer mode even for solo play.
- Mods can also export data to files (how Instrumentorio gets metrics out). Exact mechanism and throughput limits: verify during design.

## Blueprint tooling

The blueprint string format is documented (base64 compressed JSON) and mature libraries exist:

- factorio-draftsman (Python): complete, well-tested, mod-compatible blueprint manipulation. https://github.com/redruin1/factorio-draftsman
- factorio-blueprint (Node.js): create, modify, export blueprints including wires and combinators. https://github.com/demipixel/factorio-blueprint
- factorio-blueprint (Rust): read and write blueprint strings. https://github.com/coriolinus/factorio-blueprint
- Format reference: https://wiki.factorio.com/Blueprint

Takeaway: building at scale can ride on blueprint strings, with generation tooling in Python, Node, and Rust.

## What the modding API exposes

Covered only indirectly this pass. The monitoring mods prove deep read access to machine state; FLE and the companion prove script can place entities and run commands. A detailed read of the Lua API reference is design-phase work.

## What this means for us

1. The niche is open. The one project aimed at our exact vision is a chat bridge with everything else unbuilt. The one autonomous player was abandoned.
2. The bridge shape is settled by three independent projects: in-game Lua mod, RCON, external brain. That matches the Timberborn pattern from the vision doc.
3. The scripts-not-LLM principle maps to reality. Detection is proven mod-side work; the LLM projects that aimed for full autonomous play stalled or died. A partner that watches deterministically and reasons only on demand avoids what killed them.
4. Reusable pieces: FLE's command surface design, the companion's MCP wiring, draftsman or the Rust blueprint crate for building at scale, and the monitoring mods as references for detectors.

## Second pass 2026-07-18: cloned repos and what is useful in them

17 repos shallow-cloned into C:/code/factorio-refs. A GitHub search found a second wave of similar projects beyond the first pass. Licenses checked at repo root; "no license file" means design ideas only, no code reuse.

### Most useful

- factorio-sensei (Rust, MIT): the advice half of our vision, already built, in our default language. RCON client, eleven game-state reader tools (assemblers, entities, furnaces, inventory, position, power, production, recipe, research, resources), Claude agent loop, terminal REPL, and in-game /sensei chat via a bundled Lua mod. Reads state, gives coaching, changes nothing in the world.
- ai-player-v3 (no license file, active, updated 2026-07-18): the cleanest architecture split found, and it independently arrived at our core principle. The LLM only picks what to do; the mod handles how (pathfinding, placement, fuelling) via a deterministic skill layer. Skills run with no LLM at all through a console command. Perception flows out through script-output files, actions flow back through RCON. Player directs it with chat prefixes or by placing blueprint ghosts.
- factorioctl (Rust, no license file): a weekend project whose lessons-learned section is the best failure writeup found. Also has belt network analysis code (gaps, graph, reach, source tracing) and a bugs/ directory workflow: document bugs during play sessions, fix only in dedicated dev sessions.
- claude-code-plays-factorio (FLE team, no license file): the Claude Code harness pattern. Observation is read-only resources (fle://inventory, fle://entities, fle://metrics), actions are tools, plus git-like commit/restore of game state and a local workspace as persistent memory. Four specialized subagent definitions (automation, inspector, prototyping, spatial reasoning).
- claude-in-factorio (MIT): in-game chat GUI to Claude, bridge invokes claude -p with MCP tools, JSON schemas for events and responses, per-planet agent configs.
- factorio-agent by lvshrd (license unchecked, in docs/): RAG knowledge base built from the Factorio runtime API JSON plus wiki pages, with embeddings. Also an MQTT mod for pushing game events out. The "give the LLM real Factorio knowledge" piece.
- ai_combinator (MIT): natural language to combinator circuit logic, with a test case system that validates generated logic and auto-fixes failures. The test-the-generated-thing pattern is worth stealing.
- factorio-bottleneck-analyzer (no license file): tracker.lua samples every crafting machine and records which ingredient was missing on stall; remote-interface.lua shows the standard way a mod exposes calls that RCON can reach.
- Blueprint libraries: factorio-draftsman (Python, MIT, large and mod-aware) is the strongest. The Rust factorio-blueprint crate is GPL-3: decide about copyleft before linking it.

### Their failures, for our planning

- Full autonomy died twice. AI Player v1 was abandoned as too hard; v3 survives by shrinking the LLM's job to picking skills and letting deterministic code do everything else. factorioctl's author concluded the same: offload everything computable to tools, do not make the LLM do pathfinding, give it A*.
- Spatial reasoning is the wall. factorioctl: LLMs have no native sense of why anything is where it is, will build on top of ore patches, cannot hold a spatial plan. Zones (reserving map areas for a purpose) helped. FLE's benchmark results say the same: models fail at layout.
- Aspiration outruns shipping. factorio-ai-companion's README promises 51 commands and autonomous skills; only the chat bridge works.
- Benchmark-shaped is not partner-shaped. FLE and its offspring drive headless server clusters; nobody built for a player-attended game plus an assistant, which is our exact niche.
- Speed matters more than smarts for live play (factorioctl): thousands of decisions per session mean fast tool calls beat clever slow ones.
- No tests, no survival: factorioctl was vibe-coded with no tests and, per its author, collapsed under its own weight as it grew.

## Our own prior art: Timberbot, what went good and bad

Reviewed 2026-07-18 from the timberborn repo (C:/code/timberborn), its docs, and project state. Timberbot is the closest thing to what we want to build: a game mod exposing all state and all actions over HTTP, a Python client, and an AI agent loop on top.

### What went good, carry all of it forward

- The control plane shape. One HTTP server in the mod exposing every read and every write. Reads are served from snapshots on a background thread with zero main-thread cost. Writes are queued and drained on the main thread under a per-frame time budget. This threading model is the crown jewel and maps directly onto Factorio (mod-side event handlers, external brain over RCON).
- The debug endpoint. A reflection inspector that lets you examine live game state and call methods without rebuilding. The debug-first workflow (reproduce, inspect, verify assumptions, then change code) saved endless rebuild cycles.
- Errors written for an AI caller. Every error says what went wrong and what to do next: bad value echoed, valid options listed, suggestions included. This saves the LLM whole turns of guessing and was worth every line.
- Never reimplement game validation. Placement uses the game's own validators, planting uses the game's own planting checks. Factorio equivalent: use the game's can_place_entity, built-in pathfinder request, and prototype data, never our own physics.
- Push, not poll. Batched webhooks with a circuit breaker for game events. 68 event types.
- Zero-alloc discipline. Custom JSON writer, allocate at load and reuse forever, benchmark endpoint proving 0 GC collections across 760K calls. We run inside someone else's game and its stutter is our fault.
- Live test harness. Tests run against the running game, a validate endpoint compares snapshot vs live state, and a launch command boots straight into a named save for repeatable verification.
- Token-efficient output. Compact TOON format for the AI, json for programs, human-readable maps for the player.
- The split of labor: mod does mechanics, external client plus AI does judgment. Same conclusion ai-player-v3 reached independently.

### What went bad, design these out from day one

- Security as an afterthought. The 2026-04-06 review found no authentication at all, arbitrary command execution through the agent-start endpoint, SSRF through webhook registration, wildcard CORS, and unbounded request bodies. Factorio version binds localhost only with auth from the first commit.
- The A* stair placement saga. Two failed fix approaches (lookback direction guessing, destination-based orientation) before accepting that the search graph itself must encode which 3D connectors are physically valid. Post-hoc correction of a pathfinding result does not work. Release v0.7.1 sat blocked on five path routing test failures. Factorio v1 is flat per surface, and the game ships its own pathfinder, so we may dodge this entirely until multilevel railways.
- Unbounded queues. Webhook backlog and POST queues could grow without limit under load. Bound every queue at creation.
- Ephemeral entity IDs. Unity instance IDs change every session, which forced re-query crutches into the error messages. Factorio gives stable unit_number, use it from the start.
- Reachability archaeology. Long DLL spelunking to replicate a thing the game already computed and displayed. Find the game's own service first.
- Synchronous logging on hot paths. Lock contention and disk writes per request.
- Doc drift. Keeping five documentation surfaces in sync required a manual pre-release checklist. Fewer surfaces, or generated docs.

### From the lotj bot work, the cross-cutting rules already paid for

- Every command sent to the game needs an expected settle signal. Fire-and-forget actions produced production wedges.
- Silent failure is the number one killer across the whole failure log. Loud errors on every stub, instrumentation from day one, a panic path that writes where it died.
- One canonical writer per piece of state. Duplicate writers drifted and produced bugs that took multi-round sagas to close.
- Do not trust cached state at decision time when the world may have moved. Stale reads caused wrong decisions repeatedly.
- Live verification before claiming done. Tests green in isolation is not working end-to-end.

## Mining catalog 2026-07-18: what we lift from each repo

Decision: brain in Rust, mine the cloned repos for whatever is useful. License-safe code is MIT, everything else is ideas only.

### Code we can lift (MIT)

- factorio-sensei. The key mechanism discovery: state readers need no mod at all. The Rust side builds small Lua functions as strings, sends them over RCON, and gets JSON back via the game's table-to-json helper. src/lua.rs is 381 lines of ready-made Factorio 2.x readers (position, inventory, power, production, research, entities, resources, assemblers, furnaces, recipes), rcon_ext.rs wraps execution, and the 88-line bundled mod is just an in-game chat inbox (/sensei stores player messages, RCON-only poll and respond commands move them). Lift lua.rs nearly wholesale, the chat inbox pattern, and the typed tool outputs.
- Factorio Learning Environment. The most complete action vocabulary in existence: 28 agent tools including can_place_entity, connect_entities (routes belts and pipes between entities), craft_item, harvest_resource, move_to, nearest_buildable, place_entity, place_entity_next_to, rotate_entity, set_entity_recipe, and set_research, with 97 Lua files implementing the game side. connect_entities and nearest_buildable are the two spatial helpers every other project lacks.
- factorio-ai-companion. 1512 lines of working mod-side Lua command modules (building, item, move, research, resource, combat, and a 406-line queueing layer), plus TypeScript MCP server wiring for Claude Code.
- claude-in-factorio. In-game GUI chat as a three-file mod, JSON schemas for events, responses, and sessions, and the headless claude invocation pattern.
- ai_combinator. 313-line control.lua. The pattern worth stealing is its loop: generate logic from natural language, run defined test cases in-game, auto-fix failures before deploying.
- factorio-draftsman (Python). Full blueprint manipulation if we accept a Python sidecar. The blueprint string format is just compressed JSON, so a Rust implementation is realistic instead. The existing Rust crate is GPL-3, excluded.

### Ideas only (no license file)

- ai-player-v3. The skill layer split (LLM picks what, deterministic code does how), perception snapshots flowing out through script-output files, skills runnable with no LLM, and blueprint ghosts as the direction mechanism.
- factorioctl. Zones (reserving map areas for a purpose) as the answer to spatial drift, belt network analysis (gaps, reach, source tracing), and the bugs/ discipline: document during play, fix only in dev sessions.
- claude-code-plays-factorio. Read-only resources vs state-changing tools, git-like commit and restore of game state, a workspace directory as persistent memory, and four specialized subagent roles.

### To verify at design time

- Whether RCON commands count as console commands that disable achievements for the save. If yes, readers and writers should go through mod-registered commands and remote interfaces instead of raw Lua strings.
- Space Age surface handling: every reader in the catalog assumes one surface; ours must be surface-aware from the start (Nauvis, platforms, other planets).
