# AI-Assisted Factorio: Vision

Status: vision only. No code exists. Next step is researching existing Factorio mods and tooling before any design decisions are locked.

## The idea

Factorio is an amazing game, but as a factory grows in complexity it becomes harder for one person to manage. The player should have a partner to help design, build, maintain, troubleshoot, deal with biters, and keep up with the factory at scale. We built something similar for Timberborn (Timberbot) and modforge is home to our other game mods, but we have never modded Factorio.

## What the partner helps with

- Design: layouts, ratios, planning ahead of the next scale-up.
- Building at scale: turning a plan into placed entities without hand-placing everything.
- Maintenance and keeping up: noticing what the factory needs before it becomes a problem.
- Troubleshooting: why did production stall, where is the bottleneck, what starved.
- Defense: biter attacks, wall coverage, ammo supply.

## Core principle: LLM only where judgment is needed

Everything repeatable is scripts or loops, not LLM calls. Monitoring for stalled assemblers, low power, belt backups, biter attacks, and resource depletion is deterministic work. The LLM is for judgment: design decisions, diagnosis, planning, and conversation with the player. If a task can be a script, it must be a script.

## Works on any modded game

Every Factorio game gets modded. The partner must understand any modded game state and act appropriately. Item, recipe, entity, and technology knowledge comes from the game's own prototype data at runtime, never from hardcoded vanilla lists. The player's own games will run mods (relatively light initially), so this is a day-one requirement, not a later feature.

## Working assumption on architecture (to be validated by research)

The Timberborn pattern: an in-game mod exposes factory state and a command surface, and an external brain runs the scripted automation and the LLM assistance.

One Factorio fact points this direction regardless (verify during research): Factorio mods are sandboxed Lua with no network access, so any LLM has to live outside the game and talk to it through a bridge such as RCON or file exchange.

## Research questions (next step)

What already exists, so we do not rebuild it:

- AI players or AI assistants for Factorio, in any form.
- Automation and monitoring mods: anything that already watches the factory and reports problems.
- RCON tooling and other bridges between the game and external programs.
- Blueprint tooling: generators, planners, anything that produces or manipulates blueprint strings.
- What the modding API exposes: what state can be read, what actions can be taken, what the limits are.

Unverified leads from memory, to confirm or kill during research:

- An academic benchmark where LLMs play Factorio.
- RCON bot projects that drive a headless server.
- Blueprint generator tools.
