# MISERY RPG system

Design doc for the RPG layer added by the mod. The game has no
RPG system; this is entirely mod-invented. Game state (speed,
health, damage, emission time) is written to real UE properties
the same way the Speed and Shining tabs already work.

## XP and leveling

Kill enemies or craft things to earn XP. XP thresholds per level
TBD (flat or scaling). Each level grants:

- 2 stat points
- 2 skill points

Points are spent manually in the mod UI.

## Stats

Stat points go into three stats. Each point adds a flat bonus.

| Stat | Effect | Game value modified |
|------|--------|---------------------|
| Strength | Increases melee damage | TBD (needs research) |
| Agility | Increases movement speed | MovementSpeeds TMap on BP_PlayerInventory_C (proven) |
| Constitution | Increases health | TBD (needs research) |

### Agility scaling

Each agility point adds a flat amount to walk, sprint, and
crouch speeds. Base values from the game:

| State | Base speed |
|-------|-----------|
| Walk | 250 |
| Sprint | 600 |
| Crouch | 100 |

Scaling formula TBD. Example: +10 walk, +20 sprint, +5 crouch
per point.

### Strength scaling

Needs research. We need to find where melee damage lives in
memory. Likely on a weapon component or the character component.

### Constitution scaling

Needs research. We need to find the player's max health property
and write to it the same way we write speed.

## Skills

Skill points unlock or improve abilities. Skills are tiered:
spending more points in a skill increases its effect or unlocks
new tiers.

| Skill | Effect | Implementation |
|-------|--------|----------------|
| Emission time | Adds bonus time per level to the shining timer on each expedition | Write TimeUntilEmmision on BP_GlobalManager_C (proven) |
| Emission freeze | Unlocks the ability to pause the emission timer | Write FreezeTimer on BP_GlobalManager_C (proven) |

More skills TBD as we research what other game values can be
modified.

## Persistence

The game has no RPG data to save. The mod must persist:

- Current level
- Current XP
- Unspent stat points
- Unspent skill points
- Stat allocations (strength, agility, constitution)
- Skill allocations

Saved to a file in the mod folder (JSON or similar). Loaded on
game start. Stat effects applied every time the player loads
into a world.

## XP sources

### Kills

Needs research. Options:

1. Hook a kill event or damage event on the player character.
2. Poll enemy counts and detect when they decrease near the
   player.
3. Hook ProcessEvent for a specific kill notification function.

We have not researched kill detection yet.

### Crafting

Needs research. The game has crafting; we need to find the event
or state change that fires when the player crafts something.

## Open questions

1. XP scaling per level: flat (100 XP per level) or increasing
   (100, 200, 300, ...)?
2. Stat scaling: flat bonus per point confirmed. What are the
   exact numbers per stat?
3. Kill detection: can we hook it, or do we need to poll?
4. Craft detection: same question.
5. Where does melee damage live in memory?
6. Where does player max health live in memory?
7. Should stats have diminishing returns or stay linear?
8. Should emission freeze be a single unlock or have levels
   (level 1 = slow timer, level 2 = pause)?
