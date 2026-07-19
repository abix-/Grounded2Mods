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
