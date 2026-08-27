# Terminology

> **Authoritative on:** the approved words for this repo. One
> concept gets ONE word, used everywhere: docs, code, ops, tests,
> commits, conversation. A concept not listed here has no settled
> word yet; settle it with the operator before coining one.

| Word | Means | Never say |
|---|---|---|
| part | One building mesh the game ships: a wall, a floor, a post. `PartDef`, `parts.json`, `ue::parts`. | piece |
| stud | The place where two placed parts share coordinates on a border; recorded on BOTH parts, in each part's own frame. | edge, point, connection point, attachment point, join |
| pivot | The point the game places a part at, wherever the artist put it. Measured and recorded per part; nothing in the design uses it. | marker, origin offset |
| extent | Half-size of a part's own geometry, metres, y up. | bounds, half-extent (in prose) |
| shape | What a part is judged by its proportions: `Slab`, `Panel`, `Post`, `Beam`, `Block`, `Clutter`. | role, category |
| parts.json | The one file. Everything the mod learns about a part is metadata on that part in this file; nothing else is written. | catalog file, model file, sightings file |
| level | A streamed map unit as the engine names it. | |
| square | A map square in MISERY worldgen (worldgen.md). | tile, cell |
