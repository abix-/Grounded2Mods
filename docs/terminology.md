# Terminology

> **Authoritative on:** the approved words for this repo. One
> concept gets ONE word, used everywhere: docs, code, ops, tests,
> commits, conversation. A concept not listed here has no settled
> word yet; settle it with the operator before coining one.

| Word | Means | Never say |
|---|---|---|
| part | One building mesh the game ships: a wall, a floor, a post. `PartDef`, `parts.json`, `ue::parts`. | piece |
| building part | A part the designers filed under the building folders, the ones buildings are made of. The folder decides, not the name or the shape. NOT a filter: every placed mesh enters the catalog regardless. | kit, kit part, building kit |
| stud | The place where two placed parts share coordinates on a border; recorded on BOTH parts, in each part's own frame. | edge, point, connection point, attachment point, join |
| pivot | The point the game places a part at, wherever the artist put it. Measured and recorded per part; with the extent it says where an asset-loaded part's geometry sits. | marker, origin offset |
| extent | Half-size of a part's own geometry, metres, y up. | bounds, half-extent (in prose) |
| shape | What a part is judged by its proportions: `Slab`, `Panel`, `Post`, `Beam`, `Block`, `Clutter`. | role, category |
| parts.json | The one file. Everything the mod learns about a part is metadata on that part in this file; nothing else is written. | catalog file, model file, sightings file |
| level | A streamed map unit as the engine names it. | |
| square | A map square in MISERY worldgen (worldgen.md). | tile, cell |
| bot | The automation that reads the chosen path, observes the player, and sends virtual player input. | follower |
| route | The ordered waypoints and actions for a trip, such as `spawn -> metal-door -> expedition-door`. A route selects the next waypoint but does not calculate the walkable path. | waypoint graph, A* graph |
| waypoint | A meaningful place where the bot stops to arrive, observe, act, or choose the next goal. | A* node, path point |
| path | The detailed walkable path that A* calculates from the player's current position to the selected waypoint using the game's navigation data. | route, edge |
| path point | One intermediate position on the path returned by A*. The bot passes through it without treating it as a stop. | waypoint |
| player input | Virtual keys and mouse movement injected into the game's normal input processing. The game performs movement, aiming, and interaction through its existing bindings. | direct movement call, direct look call, direct interaction call |
