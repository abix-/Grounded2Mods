# MISERY building pieces

> **Authoritative on:** the goal, and every piece the game can
> build with: what it measures and where its position marker
> sits. `worldgen.md` covers how areas and squares are generated;
> this file is the plan and the parts list.

## The goal: super configurable Lego

An area in MISERY is made of pieces. We want the mod to discover
every one of them, understand how they can attach, learn how the
game's own designers put them together, and then build new things
the same way.

A real Lego system has three parts, and so does this. In order,
because each one needs the one before it.

**1. The parts list.** Every mesh the game ships, measured: its
size, where its position marker sits, and what it is. The game's
own asset registry is the source, and it reports 2,398 static
meshes while a memory walk sees about a third of them. A piece
whose size is unknown cannot be attached to anything, so nothing
downstream works until this is complete.

**It is written to disk, and it is a file to READ**, not just a
cache. Every asset with its package, class, registry tags,
measured size, marker offset and role, openable without the game
running.

Keyed on the ASSET NAME SET, not the game binary. The meshes live
in the pak files, so a content patch can change them without
touching the exe, and a cache keyed on the exe would then be
silently wrong. The registry query is cheap and loads nothing, so
comparing names gives three cases for free: the same set means
use the file as it stands, new names get measured and added, and
missing names get dropped. A patch becomes "measure the twenty
new meshes" rather than "throw it all away".

Prior art: Cargo's fingerprints and Unreal's own Derived Data
Cache both key on the inputs, not on the executable.

Open question worth settling before any of it: the registry cooks
searchable TAGS into each asset, and for a static mesh those can
already include bounds and vertex counts. If the size is in the
tags, nothing needs loading at all.

**2. The studs.** Where a piece can attach. Derived from its
measured bounds and its marker: which face, where on it, which
way it faces. A 4 m wall has an end at each side and a top; a
floor has four edges.

Then the rule that says when two of those points may join. Kinds
that are allowed to meet, facings that must oppose, sizes that
must match. Two pieces connect ONLY through a legal pair. That is
what stops a generated building from being a pile of meshes
sharing a coordinate.

**3. The instructions.** Read the vanilla buildings back and
record which pieces the designers actually place against which,
at what offset and facing. Two uses: it tells us what looks like
this game rather than like a grid, and it CHECKS the rule. Every
join the game itself makes should be a join our rule allows.
Where the game does something the rule forbids, the rule is
wrong, not the game.

**Then assembly.** Build a structure by choosing pieces whose
points fit, biased toward the pairings the designers actually
use.

## Everything that happens goes through the storyteller

Generated buildings, NPC spawning, and anything added later are
`Rule`s that `modforge::storyteller`'s `Director` picks and
paces. They are NOT features with their own timers.

This is a correction, not a preference. Two features written with
their own watchers, `strange` and `spawning`, each ended up
searching the whole object list on a clock and between them held
the game thread for 126 ms of every second
(`performance.md`). One director deciding when things happen is
also the only way the pacing of the whole world can be reasoned
about at once.

## What is written down where

| Question | Doc |
|---|---|
| What can we build with, and how does it attach? | this file |
| How are areas, squares and levels generated? | `worldgen.md` |
| How does anything in the engine work? | `research.md` |
| What does the mod cost per frame? | `performance.md` |
| What is next? | `todo.md` |

**The game's own asset index is the real inventory.** Unreal
keeps a registry of every shipped asset, queryable without
loading anything: `AssetRegistryHelpers:GetAssetRegistry` then
`AssetRegistry:GetAssetsByClass`. Live 2026-08-26 it reported
**2,398 static meshes in the game while only 869 were loaded**,
so any memory walk sees roughly a third of what exists.

For walls specifically the registry lists **55 pieces** where a
memory walk saw 12, including parts we had not seen at all:
eight corner variants (`SM_WallRoundedCorner_100x300_45d`,
`_100x400_90d`, `_200x300_90d`, `_200x400_90d`, `_300x300_90d`,
`_300x400_90d`, `_400x300_90d`, `_400x400_90d`, so corners exist
at 45 degrees as well as 90, meaning rooms need not be
rectangular), `SM_WallDoorGarage_400x300`,
`SM_WallWindow_200x300`, `SM_WallWindow_400x400`,
`SM_WallWindowSmall_200x300`, `SM_Wall_100x301`,
`SM_Wall_200x101`, `SM_Wall_400x300_1`, `SM_Wall_01Half02`.

`KismetSystemLibrary:LoadAsset_Blocking` pulls an unloaded piece
into memory, so generation is not limited to what an area
happens to have. Both go through the game-thread drain
(`asset_inventory` and `load_asset` ops, `src/assets.rs`); the
load path is not yet confirmed working.

The tables below came from measuring what was loaded, so they
carry sizes and markers. Names the registry knows but that were
not loaded at measuring time have no measurements yet.

Two sources, because neither alone is complete:

- **The object dump** (`UE4SS_ObjectDump.txt`) lists every asset
  loaded when it was written, organised by folder. The game files
  the building pieces under
  `/Game/Meshes/Blockout/Meshes/Architecture/` and
  `/Game/Meshes/Structures/Constructor/`, so the folder listing
  is the complete parts list.
- **A live probe** (`mesh_info` op, `research_inventory` test)
  measures what is loaded RIGHT NOW. Sizes and markers below come
  from there.

**Caution: what is loaded changes with the area.** A live probe
in one world reported 887 meshes with 5 wall sizes; another world
had 12. Never conclude a piece does not exist because a probe did
not see it; check this list.

All measurements are full sizes in centimetres, `width x depth x
height`. "Marker" is where the piece's position handle sits
relative to the middle of its geometry, which is what placement
maths needs.

## Walls

Named `<width>x<height>` in centimetres, and those numbers are
the real size. The marker sits at the bottom of the starting
edge, so a wall is placed at the corner it starts from, not at
its middle. All are 20 cm thick.

| Piece | Size | Marker |
|---|---|---|
| `SM_Wall_100x100` | 100 x 20 x 100 | 50, 0, 50 |
| `SM_Wall_100x300` | 100 x 20 x 300 | 50, 0, 150 |
| `SM_Wall_100x400` | 100 x 20 x 400 | 50, 0, 200 |
| `SM_Wall_200x100` | 200 x 20 x 100 | 100, 0, 50 |
| `SM_Wall_200x300` | 200 x 20 x 300 | 100, 0, 150 |
| `SM_Wall_200x400` | 200 x 20 x 400 | 100, 0, 200 |
| `SM_Wall_400x100` | 400 x 20 x 100 | 200, 0, 50 |
| `SM_Wall_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_Wall_400x401` | 400 x 20 x 400 | 200, 0, 200 |

`SM_Wall_400x401` is the 4 x 4 m wall; there is no `400x400`.

Ruined variants: `SM_Wall_400x400Broken`,
`SM_Wall_400x400BrokenCrouch` (crouch-height gap).

Older, thinner walls that do NOT follow the naming rule and are
pivoted at their middle: `SM_Wall_01` (330 x 1 x 330),
`SM_Wall_01Half` (170 x 1 x 330), `SM_Wall_DF` (330 x 16 x 330),
`SM_Chunck`.

## Walls with a door

| Piece | Size | Marker |
|---|---|---|
| `SM_WallDoor_200x300` | 200 x 20 x 300 | 100, 0, 150 |
| `SM_WallDoor_200x400` | 200 x 20 x 400 | 100, 0, 200 |
| `SM_WallDoor_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallDoorDouble_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallDoorDouble_400x400` | 400 x 20 x 400 | 200, 0, 200 |
| `SM_WallDoorGarage_400x400` | 400 x 20 x 400 | 200, 0, 200 |

**Avoid `SM_WallDoor_400x400`.** It measures 458 x 56 x 460 with
its marker at 171, 18, 227: it does not follow the rule and will
not line up. `SM_WallDoor_400x400Long` also exists, unmeasured.

## Walls with a window

| Piece | Size | Marker |
|---|---|---|
| `SM_WallWindow_200x400` | 200 x 20 x 400 | 100, 0, 200 |
| `SM_WallWindow_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallWindowDouble_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallWindowDouble_400x400` | 400 x 20 x 400 | 200, 0, 200 |
| `SM_WallWindowSmall_200x400` | 200 x 20 x 400 | 100, 0, 200 |
| `SM_WallWindowSmall02_200x400` | 200 x 20 x 400 | 100, 0, 200 |

## Corners

Marker at the OUTER corner, geometry running back from it.

| Piece | Size | Marker |
|---|---|---|
| `SM_WallRoundedCorner_200x300_90d` | 210 x 210 x 300 | -105, -105, 150 |
| `SM_WallRoundedCorner_400x400_90d` | 410 x 410 x 400 | -205, -205, 200 |

## Floors

22 cm thick, marker at a corner with the walking surface at
marker height, so a floor is placed at floor level.

| Piece | Size | Marker |
|---|---|---|
| `SM_Floor_100x100` | 100 x 100 x 22 | 50, 50, -9 |
| `SM_Floor_100x200` | 100 x 200 x 22 | 50, 100, -9 |
| `SM_Floor_100x400` | 100 x 400 x 22 | 50, 200, -9 |
| `SM_Floor_200x200` | 200 x 200 x 22 | 100, 100, -9 |
| `SM_Floor_200x400` | 200 x 400 x 22 | 100, 200, -9 |
| `SM_Floor_400x400` | 400 x 400 x 22 | 200, 200, -9 |
| `SM_Floor_1000x1000` | 1000 x 1000 x 22 | 500, 500, -9 |

## Ceilings

`SM_Concrete_LongCeiling`, `SM_Concrete_BrokenLongCeiling`,
`SM_Concrete_BrokenWideCeiling`. Not on the wall grid; sized to
the concrete panel buildings.

## Pillars and beams

| Piece | Size | Marker |
|---|---|---|
| `SM_Pillar` | 56 x 56 x 330 | 0, 0, 165 |
| `SM_Beam_300` | 40 x 40 x 300 | 0, 0, 150 |
| `SM_Beam_400` | 40 x 40 x 400 | 0, 0, 200 |
| `SM_Concrete_Post_1_C` | 11 x 11 x 107 | 0, 0, 54 |
| `SM_MetalPost` | 17 x 15 x 102 | 0, 0, 51 |
| `SM_Concrete_SmallLthing` | (unmeasured) | |

Beams and pillars are centred in both horizontal axes with the
marker at their base, unlike walls.

## Stairs and walkways

| Piece | Size | Marker |
|---|---|---|
| `SM_Stair_100` | 200 x 150 x 102 | 100, 75, 51 |
| `SM_StairPlane_200` | 200 x 300 x 222 | 100, 150, 91 |
| `SM_Ladder_01_Unit` | 52 x 10 x 50 | 0, 5, 25 |
| `SM_Ladder01` | 90 x 400 x 18 | 0, 0, 9 |
| `SM_FoldingLadder` | 143 x 78 x 240 | 16, 2, 120 |

Also `SM_CatwalkStair_01`, `SM_CatwalkStairRail_01`,
`SM_Catwalk_01`, `SM_Catwalk_02`, `SM_CatwalkRail_01`,
`SM_CatwalkRail_02`, `SM_Barrels_Stairs_02`.

## Frames

Door and window frames that stand alone rather than filling a
wall segment: `SM_DoorFrame` (132 x 24 x 232),
`SM_DoorFrameDouble` (232 x 24 x 232), `SM_DoorFrameGarage`,
`SM_FakeDoor` (132 x 12 x 232), `SM_WindowsFrame` (140 x 22 x
164), `SM_WindowsFrameUp`, `SM_EnteranceDoorframe01`.

## What the designers actually use

Read from live squares (`research_vanilla_rooms`): in a town and
road area, 108 placed pieces used only five distinct parts:
`SM_Floor_400x400` (66), `SM_Wall_400x401` (27),
`SM_WallRoundedCorner_200x300_90d` (8),
`SM_WallDoorGarage_400x400` (6), `SM_Floor_1000x1000` (1).

So the designers work mostly at 4 m, and much of what looks like
building pieces is road paving. Buildings are assembled square-on
and then turned to an arbitrary angle when placed (the dominant
angle in that sample was 5 degrees), which is why placed pieces
never line up with the world axes.

Two caveats on that sample: it is small, and it came from a road
square rather than a building. Village and garage squares showed
richer use (the six-part `SM_BrikGarage_*` family, house windows,
12 wall sizes) and are the better place to study how rooms are
put together.

## Whole buildings

Separately from the pieces, the game has entire buildings as one
mesh, which can only be placed, never assembled:
`SM_WoodenCabit_02` (6.3 x 6.6 x 8.3 m),
`SM_WatchTower_SM_ContainerHouse1`, `SM_House01_Var_A_Stairs`,
and the `SM_BrikGarage_*` family (garage, closed garage, left and
right doors, two tarps) which is a kit for one building type.

## Not building pieces

The name prefixes overlap with props, so filter by folder rather
than by name where it matters: `SM_WallClock`, `SM_WallLamp`,
`SM_WallpaperRoll1` to `7`, `SM_WallZaslavFlag` all begin with
`SM_Wall` but are decoration.
