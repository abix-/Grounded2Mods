# traveler

The traveler exists outside any single modded game. Their identity,
history, and stats live in modforge and travel with them. When they
load a modforged game, the mod reads the traveler profile and shapes
the world around it: harder opposition, different faction attitudes,
unlocked or locked content, narrative callbacks to things that happened
in other games entirely.

Progress in game A persists into game B into game C. Each game is
modded differently on top of its base, but the traveler is the thread
that connects them.

## inspiration

Tad Williams' Otherland series (1996-2001). A vast network of virtual
worlds, each with its own rules, aesthetics, and internal logic. The
characters carry their identity and consequences across all of them.
What happens in one world matters in the next. The worlds are not
theme park levels; each is its own complete reality that the characters
experience differently because of who they are and what they have been
through.

The design principle drawn from it: each modforged game keeps its own
rules and feels native. The traveler is not a tourist. They
are a continuous identity whose history accumulates and whose presence
changes the world they walk into.

## profile schema

### decided

The traveler has standard RPG base stats, one cross-game
level, and a nemesis record. Skills are game-specific and stay in each
game's per-slot save (the existing ueforge rpg Tracker/SlotStore).

```json
{
  "schema_version": 1,
  "name": "string",
  "level": 0,
  "xp": 0,
  "strength": 0,
  "dexterity": 0,
  "constitution": 0,
  "intelligence": 0,
  "wisdom": 0,
  "charisma": 0,
  "nemesis": {
    "wins": 0,
    "losses": 0,
    "escalation": 0.0,
    "tactics": []
  },
  "game_history": []
}
```

**Stats**: the classic six. Each scales 0 to 9999. Start at 0 (or a
small seed on cold start). Every game can award stat growth through
its own triggers (kills raise strength, crafting raises intelligence,
trading raises charisma, etc.). Growth is written back to the profile
on save/exit.

**Level**: one number across all games. Cumulative XP from every game
feeds into one Curve. The existing modforge rpg Curve type works
here; just needs a cross-game instance with a high max_level and a
tuned exponent for the 0-9999 range.

**Skills**: NOT in the profile. Skills are game-specific catalog
entries with game-specific effects. They stay in each game's per-slot
save via the existing SlotStore/Tracker. The traveler profile
provides the base stats that skills build on top of.

**Nemesis**: wins, losses, an escalation float that ratchets up with
each encounter, and a tactics array (initially empty, grows as the
nemesis adapts to the player's patterns across games).

**Inventory**: unlimited storage. Items exist in two forms:

- **In-game**: the real item inside a specific game, with all
  game-specific properties (durability, enchantments, stack size,
  exact item ID). Each game owns its own item representation.
- **Abstract**: the cross-game form, stored in the traveler profile.
  A generic schema that any game can read.

Abstract item schema:

```json
{
  "name": "iron sword",
  "type": "weapon",
  "subtype": "melee",
  "quality_tier": 3,
  "value": 500,
  "weight": 4.2,
  "properties": {},
  "origin_game": "misery",
  "origin_data": {}
}
```

`type` and `subtype` are from a fixed vocabulary (weapon/armor/food/
material/tool/consumable, with subtypes per type). `properties` holds
generic key-value pairs any game can read (damage, defense, healing,
etc.). `origin_data` is an opaque blob the originating game wrote,
carrying everything needed to reconstruct the exact in-game item.

**Conversion flow**:

1. Player leaves game A. Game A converts each carried item into an
   abstract item: fills the generic fields AND writes its own
   game-specific reconstruction data into `origin_data`.
2. Abstract items are stored in the traveler profile.
3. Player enters game B. Game B reads the abstract items and converts
   each into its own in-game equivalent using the generic fields
   (type, quality tier, value). It finds the closest match in its
   own item system. `origin_data` from game A is ignored but
   preserved in the profile.
4. Player returns to game A with the same items. Game A sees
   `origin_game: "misery"` matches itself, reads `origin_data`, and
   reconstructs the exact original item. No loss on round-trip.
5. Player returns to game A with items from game B. Game A sees
   `origin_game: "grounded2"`, ignores the foreign `origin_data`,
   and maps from generic properties to the closest match in its own
   items.

Each game mod implements two functions: `to_abstract(game_item) ->
AbstractItem` and `from_abstract(abstract_item) -> game_item`. The
round-trip through the same game is lossless because `origin_data`
preserves everything. Cross-game conversion is lossy by nature (a
misery sword is not a grounded weapon) but best-effort via the
generic properties.

**The transition space**: the moment between games, when the traveler
exists only as a profile with abstract items, could be its own
experience. A hub, an inventory screen, a meta-game where you sort
and prepare what you are carrying into the next world. Not required
for the system to work, but a natural place to put it.

**Game history**: a ledger of past sessions. Each entry records the
game, hours played, and a short outcome. Games can read history from
other games to react ("you played 200 hours of the survivalist game
and your settlements kept dying" means something to the next game's
difficulty seeding). Kept lean: no raw stats dumps, just the
meaningful outcomes.

### alignment with existing rpg system

The existing rpg system in ueforge has:

- `SkillsState`: per-slot, per-game. Has `xp`, `level`,
  `skill_points`, `skill_levels` map. Persisted via `SlotStore`.
- `Curve`: XP math (`base * level^exponent`). Currently one instance
  per game mod.
- `Tracker`: manages slot binding, spending, persistence. One static
  per game mod.
- `SkillRegistry` / `SkillDef`: game-specific skill catalogs with
  Effects and Triggers.

The traveler profile does NOT replace any of this. It sits
alongside it. The relationship:

- The profile's `level` and stats are the player's CROSS-GAME
  identity. They feed into each game as parameters (difficulty
  scaling, faction seeding, loot odds, spawn rates).
- The per-game `SkillsState` is the player's IN-GAME progression.
  Skills, skill points, and per-game XP stay local to each save slot.
- XP earned in a game feeds BOTH the per-game Tracker (for skill
  points) AND the traveler profile (for cross-game level). Two
  separate Curves, two separate level numbers, two separate purposes.
- A game mod reads the profile on load and adjusts its parameters.
  It writes stat growth and game history back on save/exit.

### storage

Local JSON file in a known location (the modforge data directory,
shared across all game mods). Same atomic temp-file-plus-rename
pattern that SlotStore already uses. One file per player identity.

Cloud sync is a future upgrade, not a launch requirement.

## open questions

### directionality

Forward-only (game writes profile on exit, next game reads on load)
is the starting point. Bidirectional (going back to game A picks up
changes from game B) is more interesting but harder. Does the world
shift under you mid-save? Or only on new-game start?

### does the player know?

Transparent ("your history precedes you"), hidden (the world just IS
different), or revealed gradually (patterns emerge by the third game)?

### cold start

First game with no profile. Options: vanilla baseline (no changes
until a profile exists), or seed a neutral profile and let the first
session calibrate it.

### stat growth rates

How fast do stats grow? If a single game can take you from 0 to 9999
in one playthrough, the cross-game progression is meaningless. Stats
need to grow slowly enough that multiple games are needed to reach
high levels, but fast enough that each session feels like it
contributed.

## how it works: parameters, not features

The traveler is not a new system. It is a new input to
systems that already exist in modforge. Each game mod already reads
from these; the profile just gives them more to work with.

**Storyteller**: the director already picks events by difficulty and
pacing. A high-level player profile shifts the difficulty curve: harder
events fire earlier, escalation ramps faster, quiet periods shorten.
The storyteller does not know about "cross-game persistence." It just
sees a higher difficulty parameter.

**Quality**: the tier roll system already takes per-mille odds tables.
A seasoned player gets shifted odds: better loot drops because the
table changes, not because a new loot system was bolted on. The game's
own item system does the rest.

**Dread loop**: the unknown module already controls escalation timing
(min/max delay, foreshadow vs real). A veteran player's profile
compresses the delays and raises the real-event ratio. More threat,
less breathing room. Same system, different parameters.

**Genome / war**: faction genomes already seed aggression,
expansionism, guile, defensiveness. A feared player profile biases the
initial seed: factions start more aggressive or more defensive
depending on the player's history. The Darwinian system takes over
from there. The world reacts to who you are from the first tick.

**RPG / progression**: if the game has levels, skills, or stats, the
profile carries a player level that the mod reads as a baseline. A
level 30 player loading a new game does not start at level 1 in the
mod's eyes, even if the base game thinks they did. The mod adjusts
spawn rates, enemy counts, resource scarcity.

Each game mod maps the profile to its own parameters. No game needs to
understand another game's specifics. A survivalist mod reads
"player.aggression" and feeds it to its faction seeding. A wild west
mod reads the same field and adjusts bounty hunter frequency. Same
data, different interpretation, native to each game.

**Persistent nemesis**: a threat that follows the player between games.
The profile carries a nemesis entry: a win/loss record, an escalation
counter, and trait weights that describe how it fights. Each game
manifests the nemesis through its own enemy/faction systems. In a
survivalist game, it is a faction that always spawns hostile, seeds
with traits tuned to counter the player's known weaknesses, and grows
from the player's failures. In a western, it is a bounty hunter with
better gear and more backup each encounter. In a miner, it is a rival
claim-jumper who shows up at the worst moment.

The dread loop drives the timing: the nemesis fires in every game, and
the profile's encounter history sets the delay range and real-event
ratio. Beat it and it comes back harder next game. Lose to it and it
comes back confident (shorter delays, bolder tactics). The escalation
is continuous across games, not reset per title. The player can never
fully escape it, only push it back.

Each game decides what the nemesis looks like, what it commands, how it
fights. The profile only says: it exists, it is this strong, it has
beaten or lost to the player this many times, and it favors these
tactics. The game's own AI, spawn, and combat systems do the rest.

## world travel: games as destinations

Games are not separate titles the player happens to own. They are
worlds the player travels to during play. One game can send the
player to another game as part of its own gameplay loop.

Example: a misery expedition opens a door in the bunker. Instead of
loading a normal misery scene, it launches Grounded 2 with the
traveler. The misery context shapes the visit: what the
expedition was for, what loot to look for, how dangerous it is. The
player plays Grounded 2 as a world they are visiting. When the visit
ends (objective complete, timer expired, death), Grounded 2 writes the
result back to the profile, closes, and misery picks up where it left
off with whatever the player carried home.

Each world has its own entrance, exit, and rules.

### the travel protocol

**Launch context**: the origin game writes a travel request to the
profile before launching the destination. The request carries:

- origin game (who sent you)
- purpose (loot run, bounty hunt, exile, escape)
- entry parameters (difficulty modifier, time limit, objective)
- return address (what save state to resume on return)

**Entry rules**: the destination game reads the travel request on
load. If present, it sets up the session accordingly: spawn location,
difficulty, available resources, and the exit condition. If no travel
request exists, the game runs normally (standalone play is always
valid).

**Exit trigger**: the destination game knows when the visit is over.
This could be: objective complete, timer expired, player dies, player
reaches an exit point, or the player manually chooses to leave. The
trigger is defined by the entry parameters, not hardcoded per game.

**Return payload**: on exit, the destination game writes what happened
back to the profile: loot acquired, stats gained, nemesis encounter
result, time spent, how the visit ended (success, failure, fled). The
origin game reads this on resume and incorporates the result into its
own world.

**The launcher**: modforge needs a process that can start a game
(Steam launch, direct executable, etc.), pass the travel context in
via the shared profile file, and detect when the destination game
exits. The origin game saves its state and closes before the
destination launches. On return, the player loads the origin game's
save normally. Save/close is the safest option: no memory pressure
from two games, no process management, no hook interference. The
return delay (a save load) is acceptable and can be evaluated once
the system exists.

### what this means for each game mod

Every modforged game needs:

- A way to read travel context on startup and adjust accordingly.
- At least one exit condition that writes a return payload.
- The ability to run as a standalone game with no travel context
  (graceful fallback).

Games do NOT need to know about each other specifically. The travel
protocol is generic: "someone sent you a player with this purpose and
these parameters." The game interprets purpose and parameters through
its own systems. A "loot run" purpose in Grounded 2 means better item
spawns. The same purpose in the wild west means a richer claim site.
Each game decides what any given purpose means in its own world.

### the experience

The player does not think "I am switching games." They think "I am
going on an expedition." The destination game IS the expedition. Its
entire world, its own rules, its own dangers. But the context of why
you are there comes from the origin, and what you bring back matters
when you return. The games become places, not products.

## design constraints

- The traveler profile must be small enough that reading/writing it
  is instant (no load screens, no network waits on game start).
- Each game mod must be playable WITHOUT a traveler profile (graceful
  fallback to vanilla-mod behavior). The system enhances; it never
  gates.
- The profile format must be forward-compatible: adding new traits or
  ledger entries in a future game must not break older games that do
  not understand them.
- No game should be able to corrupt or invalidate the profile. Bad
  data from a buggy mod must not poison the player's history across
  all games.
