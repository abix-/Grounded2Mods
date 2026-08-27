# MISERY building parts

> **Authoritative on:** the goal, and every part the game can
> build with: what it measures and where its pivot sits.
> `worldgen.md` covers how areas and squares are generated;
> this file is the plan and the parts list.

## The goal: super configurable Lego

An area in MISERY is made of parts. We want the mod to discover
every one of them, understand how they can attach, learn how the
game's own designers put them together, and then build new things
the same way.

A real Lego system has three parts, and so does this. In order,
because each one needs the one before it.

**1. The parts list.** Every mesh the game ships, measured: its
size, its pivot, and what it is. The game's own asset registry is
the source: it reports 2,398 static meshes while a memory walk
sees about a third of them. A part whose size is unknown cannot
be attached to anything, so the other two steps wait on this.

**It is written to disk, and it is a file to READ**, not just a
cache. Every asset with its package, measured size, pivot and
shape, openable without the game running.

The file is checked against the game by the list of asset names,
not by the exe. The meshes live in the pak files, so a patch can
change them without touching the exe. Same names: use the file as
it stands. New names: measure and add them. Missing names: drop
them. A patch means measuring the twenty new meshes, not
starting over.

### Where the size comes from

The registry cooks searchable tags into each asset, readable with
nothing loaded. This build carries `ApproxSize`, `Triangles`,
`Vertices`, `Materials`, `LODs`, `MinLOD`, `UVChannels`,
`PhysicsAsset`, `CollisionPrims` and `NaniteEnabled`; `Bounds`
exists but is null on every asset (read live 2026-08-27).
`ApproxSize` is the one that carries the dimensions:

```text
Sphere                     ApproxSize 320x320x320          Tris 528   Verts 323  LODs 1
SM_MediaPlateScreen        ApproxSize 0x100x100            Tris 2     Verts 4
SM_Derbis_B                ApproxSize 7x8x7                Tris 40    Verts 66
SM_mountain_background_02  ApproxSize 200000x200000x45159  Tris 9829  Verts 4920 LODs 2
```

Every size is one registry pass with nothing loaded, as a
`WxDxH` string, with triangle and vertex counts, materials and
LODs alongside. Three cautions:

- `ApproxSize` is approximate and rounded to whole units. Enough
  to sort a 4 m wall from a 2 m one; anything needing exact
  bounds loads that one mesh.
- `SM_MediaPlateScreen` reports `0x100x100`. A zero dimension
  means flat, not missing.
- It is the mesh's BOUNDING BOX. `SM_Floor_400x400` measures
  5.18 by 5.73 m yet sits on a 4 m spacing, because its lip is
  inside the box. Never place a part's faces from the box alone;
  that is what the pivot and the studs are for.

Conversion, matching `ue::parts`: Unreal is centimetres with z
up, this crate is metres with y up, and `PartDef::extent` is a
HALF-size while `ApproxSize` is a full one. So
`mf(x,y,z) = ue(y,z,x) / 2 / 100`.

The tag is read with `AssetRegistryHelpers::GetTagValue(
FAssetData, FName TagName, FString& OutValue) -> bool`, 4 parms
in 129 bytes:

```text
0x00  FAssetData   the whole 0x68-byte entry, BY VALUE
0x68  FName        the tag to ask for
0x70  FString      the answer, written by the engine
0x80  bool         whether the tag was there
```

The tag name must be an `FName`; `ue::fname::from_str` builds one
from a string (research.md 28).

### The pivot

A mesh's pivot (the point the game places it at) sits wherever
the artist put it: a wall's at the bottom of its starting edge, a
floor's at a corner. Recorded per part, measured off every loaded
mesh (`UStaticMesh::ExtendedBounds.Origin`, all 2,407 in 2.41 s
live 2026-08-27, agreeing with the tables at the bottom of this
file).

On a STREAMED level it is not needed: the engine knows where the
placed geometry is and can be asked. On an ASSET-LOADED level the
engine knows nothing (measured: zero for every part), and the
pivot plus the extent is the only source of where a part's
geometry sits around its placement. The extraction runs on
asset-loaded levels, so it computes with the pivot (see "Where a
part's geometry sits is COMPUTED").

### Where this stands, 2026-08-27

| Step | State |
|---|---|
| the parts list | DONE. 2,407 meshes on disk with size, shape and pivot, every one of them. |
| the stud reading | DONE and proven live: `modforge::studs::studs_in` finds shared borders, four unit tests, and one square's studs landed on both parts in `parts.json` (54 wall studs partnered with floors, 64 the mirror). The superseded distance design (`joins_in`, `Join`, the `joins` op) is deleted. |
| the catalog | DONE, first full run 2026-08-27: all 123 level assets loaded and read in 71 s, zero failures, 2,818 studs confirmed 4+ times across 94 parts, merged into `parts.json`. The 4 m wall's commonest studs read as a kit: a wall stacks on top seen 190, the next wall along seen 157, and a door wall substitutes at the same stud. |
| the noise | A floor tile still carries 888 studs after the cull: road paving follows terrain, and irregular seams recur enough to survive min_seen 4. Open row. |

### The shape

Each part gets a shape judged from its proportions alone: thin
and tall is a `Panel`, flat and wide a `Slab`, thin in both
horizontal axes a `Post` or `Beam`. Live, all 2,407 in one pass:

```text
Block 885, Slab 380, Panel 320, Clutter 520, Post 144, Beam 158
no size: 0
```

The shape agrees with the names without being told any:

```text
SM_Wall_400x401      Panel   [0.10, 2.00, 2.00]
SM_WallDoor_400x300  Panel   [0.10, 1.50, 2.00]
SM_Floor_400x400     Slab    [2.59, 0.44, 2.87]
```

The shape is for sorting and reading, not for deciding what is a
building part. Names and proportions both lie: `SM_WallClock` is
decoration, a road slab measures like a floor. What IS a building
part is decided by the folder the designers filed the mesh under
(see "Not building parts" at the bottom).

**2. The studs, and they come from the VANILLA BUILDINGS.**

The designers spent hundreds of hours building with these parts,
and the buildings already work. A room is a floor, walls, a door,
and each part is CONNECTED to another part where the two meshes
share coordinates: the bottom of the wall occupies the same
coordinates as the rim of the floor. They share a border. **That
shared border is a STUD**, and stud is the only word for it.

So this is a CATALOG, not a search. Walk the vanilla buildings
part by part and record, for every part, which studs it uses to
connect to which other parts.

### ALL of it, no streaming (proven live 2026-08-27)

The vanilla buildings are data in the pak files, and the whole
route is the same move as the parts list:

- The registry lists **121 level assets** under class `World`:
  every pool square from worldgen.md 4 (`L_Town01`,
  `L_Kolhoz01`, the factories, the bunkers), plus levels the
  pools never name (`L_SafeHub`, `L_Store01`,
  `L_TutorialLevel`).
- `LoadAsset_Blocking` pulls one in as a live `World` object,
  confirmed by walking the object list to the same address.
- `level_parts` then reads its placed actors:
  `3727_4_7.L_Anomaly_House` gave **2,751 parts with names and
  positions**, and the player was never in that square.

So extraction is a loop over the 121, no square ever streamed
into play. (`research_assets::read_a_level_asset_without_
streaming`.)

Two things the loop must settle:

- **Memory.** 121 levels at a few thousand actors each, all
  loaded at once, is unmeasured. Whether a read level can be let
  go afterwards is unknown.
- **Duplicates.** `L_Town01` exists in several packages: the
  preset itself and per-world squares (`3727_5_7.L_Town01`).
  Which to read, or whether reading all of them just confirms
  the same studs more times, is undecided.

And a find on the way: **individual buildings ship as levels
too.** The panel houses are level instances
(`LI_HouseVar1_01` under `/Game/Meshes/Structures/PanelHouses/`),
so a single house is itself a level of placed parts, and those
are among the 121.

### Where a part's geometry sits is COMPUTED (2026-08-27)

On a streamed level the engine knows where every part's geometry
is. On an asset-loaded level it does not: `GetComponentBounds`
answered zero for all 2,306 parts of `L_Anomaly_House`. The
asset holds each part's placement and its mesh, nothing more.

So `level_boxes` computes it: the mesh's own box (extent and
pivot, the same numbers `parts.json` carries), scaled, turned by
the part's yaw, moved to its placement. Proven against the same
square:

```text
2306 boxed parts, 230 SM_Floor, 54 SM_Wall, 23 skipped
SM_Wall_200x400  bottom z 320.00 on SM_Floor_400x400  top z 322.00  gap -2.00 cm
```

Floors come out exactly 22 cm thick (z 300 to 322), matching the
tables below, and walls stand on them. This is also where the
pivot earns its place after all: on asset-loaded levels it is the
only source of where geometry sits around the placement point.

Two facts the stud reading must respect, both measured:

- **Connected parts OVERLAP slightly; they do not meet exactly.**
  A wall's bottom sits 2 cm below the floor's walking surface,
  sunk into it. That is the interlock. Shared coordinates means
  within a tolerance of a few centimetres, not exact equality.
- **Parts in a level are placed at an angle**, so world-axis
  boxes inflate (a 4 m floor spans 4.9 m). Borders must be
  compared in the parts' own turned frame. Up is unaffected.

The GC answers the memory question by itself: a loaded level
nothing references is let go within seconds, so the extraction
loop must load and read a level in one breath, and 121 levels
never sit in memory together.

### Two meshes, same coordinates, shared border

Where each placed mesh's geometry sits in the world is read off
the placed mesh directly; the engine put it there. Two parts are
connected where their geometry has coordinates in common: that
shared border is the stud. Nothing statistical about it, and no
distances, sizes or pivots enter into it. One placed room is
already a complete, correct catalog entry; the same stud in ten
more rooms confirms it.

Each stud is recorded on BOTH parts, in each part's own frame:
the floor's stud says a wall connects here, and the wall's stud
says the mirror. Per part, never per pair, because that is what
makes substitution work: any part with a matching stud can take
the wall's place. A list of "this wall meets this floor" could
never say that.

- **`parts.json` is the catalog, and studs are per PART**, next
  to its size and shape. To place a part you need only its own
  studs and the studs of what is already there.
- **ONE FILE.** A connection found in a level becomes studs on
  both parts and goes into `parts.json`. Nothing else is written.
  If the connections were also written to their own file there
  would be two files saying the same thing, and sooner or later
  one of them is wrong.
- **`seen` is part of the stud**: how many placements confirmed
  it. 231 is a rule, four is a maybe.

### Rules for the pass that reads the levels

- **Only building parts enter.** Which meshes those are is
  decided by their folder in `parts.json` (see "Not building
  parts"), not by name and not by shape.
- **A write past a size cap stops and says so** rather than
  filling the disk. Pairing every actor within 9 m, the old
  wrong test, wrote a 900 MB file on 2026-08-27 (research.md 30).
- **A broken actor is skipped and counted.** Some actors in a
  streamed level have a mesh pointer that does not resolve;
  dereferencing one kills the process (research.md 29), and
  measuring through one reports sizes like 6.8e36. Both kinds are
  skipped and the skip count reported. A level that skips half
  its actors is telling you something.

### The format

A part in `parts.json`, with the real measured numbers:

```json
{
  "name": "SM_Floor_300x400",
  "package": "/Game/.../SM_Floor_300x400",
  "extent": [1.5, 0.1, 2.0],
  "pivot": [1.5, -0.09, 2.0],
  "shape": "Slab",
  "triangles": 44, "vertices": 24, "materials": 1, "lods": 1,

  "studs": [
    {
      "at": [0.0, 3.5, 0.0],
      "turn": 90,
      "seen": 231,
      "with": { "SM_Wall_400x400BrokenCrouch": 231 }
    }
  ]
}
```

- **`pivot`** is where the middle of the geometry sits relative
  to the point the game places the part at, metres, y up.
  Measured and recorded; nothing in the design uses it (see "The
  pivot does not matter").
- **`at`** is where the stud sits on this part, in the part's
  OWN local frame, metres, y up: the same units and axes as
  `extent` directly above it. The measurement is in centimetres
  and converts on the way in, because two units in one file is
  how mistakes happen. Today it is one position; whether a stud
  also needs its span recorded is open.
- **`turn`** is how far the attached part is turned relative to
  this one, degrees. Position alone is not enough: a wall laid
  across a floor and one stood along it sit at the same spot.
- **`seen`** is the confidence, left visible rather than
  collapsed into a boolean. 231 is a rule; 4 is a maybe.
- **`with`** is which parts were actually seen there, counted.
  It answers "what does the game put here", and it is what biases
  generation toward looking like this game rather than merely
  being legal.

The document header says how far to trust the whole thing:

```json
{
  "count": 2407,
  "units": "half-extent, pivot and stud positions in metres, y up",
  "observed": { "sightings": 50328, "squares": 11 },
  "derived_with": { "round_cm": 1.0, "min_sightings": 4 }
}
```

`round_cm` is how close two coordinates must be to count as the
same (a hand-nudged placement must not invent a new stud), and
`min_sightings` is how many placements must confirm a stud.
Recording them means a reader can tell whether a stud list was
built with the loose settings or the strict ones, instead of
guessing.

**Open, and the data will answer it:** whether a stud needs a
TYPE beyond its position and facing. In real Lego a stud is a
stud and anything fits anything. Here a wall's base and a wall's
end might land on similar local positions while being different
kinds of stud, and position alone would then let us build
connections the game never makes.

Then the rule that says when two studs may join. Kinds that are
allowed to meet, facings that must oppose, sizes that must match.
Two parts connect ONLY through a legal pair of studs. That is
what stops a generated building from being a pile of meshes
sharing a coordinate.

**3. The instructions.** Read the vanilla buildings back and
record which parts the designers actually place against which,
at what offset and facing. Two uses: it tells us what looks like
this game rather than like a grid, and it CHECKS the rule. Every
pairing the game itself makes must be one the rule allows.
Where the game does something the rule forbids, the rule is
wrong, not the game.

**Then assembly.** Build a structure by choosing parts whose
studs fit, biased toward the pairings the designers actually
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

For walls specifically the registry lists **55 parts** where a
memory walk saw 12, including parts we had not seen at all:
eight corner variants (`SM_WallRoundedCorner_100x300_45d`,
`_100x400_90d`, `_200x300_90d`, `_200x400_90d`, `_300x300_90d`,
`_300x400_90d`, `_400x300_90d`, `_400x400_90d`, so corners exist
at 45 degrees as well as 90, meaning rooms need not be
rectangular), `SM_WallDoorGarage_400x300`,
`SM_WallWindow_200x300`, `SM_WallWindow_400x400`,
`SM_WallWindowSmall_200x300`, `SM_Wall_100x301`,
`SM_Wall_200x101`, `SM_Wall_400x300_1`, `SM_Wall_01Half02`.

`KismetSystemLibrary:LoadAsset_Blocking` pulls an unloaded part
into memory, so generation is not limited to what an area
happens to have. Confirmed working 2026-08-27: the pivot pass
loads all 2,407 meshes through it. Both calls run on the game
thread (`asset_inventory` and `load_asset` ops,
`ueforge/src/assets.rs`).

The tables below came from measuring what was loaded, so they
carry sizes and pivots. Names the registry knows but that were
not loaded at measuring time have no measurements yet.

Two sources, because neither alone is complete:

- **The object dump** (`UE4SS_ObjectDump.txt`) lists every asset
  loaded when it was written, organised by folder. The game files
  the building parts under
  `/Game/Meshes/Blockout/Meshes/Architecture/` and
  `/Game/Meshes/Structures/Constructor/`, so the folder listing
  is the complete parts list.
- **A live probe** (`mesh_info` op, `research_inventory` test)
  measures what is loaded RIGHT NOW. Sizes and pivots below come
  from there.

**Caution: what is loaded changes with the area.** A live probe
in one world reported 887 meshes with 5 wall sizes; another world
had 12. Never conclude a part does not exist because a probe did
not see it; check this list.

All measurements are full sizes in centimetres, `width x depth x
height`. "Pivot" is where the middle of the geometry sits
relative to the part's PIVOT, which is the point the game places
it at. So a part's faces run from `pivot - extent` to `pivot +
extent`, and that is what says whether two placed parts touch.

## Walls

Named `<width>x<height>` in centimetres, and those numbers are
the real size. The pivot sits at the bottom of the starting
edge, so a wall is placed at the corner it starts from, not at
its middle. All are 20 cm thick.

| Part | Size | Pivot |
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

| Part | Size | Pivot |
|---|---|---|
| `SM_WallDoor_200x300` | 200 x 20 x 300 | 100, 0, 150 |
| `SM_WallDoor_200x400` | 200 x 20 x 400 | 100, 0, 200 |
| `SM_WallDoor_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallDoorDouble_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallDoorDouble_400x400` | 400 x 20 x 400 | 200, 0, 200 |
| `SM_WallDoorGarage_400x400` | 400 x 20 x 400 | 200, 0, 200 |

**Avoid `SM_WallDoor_400x400`.** It measures 458 x 56 x 460 with
its pivot at 171, 18, 227: it does not follow the rule and will
not line up. `SM_WallDoor_400x400Long` also exists, unmeasured.

## Walls with a window

| Part | Size | Pivot |
|---|---|---|
| `SM_WallWindow_200x400` | 200 x 20 x 400 | 100, 0, 200 |
| `SM_WallWindow_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallWindowDouble_400x300` | 400 x 20 x 300 | 200, 0, 150 |
| `SM_WallWindowDouble_400x400` | 400 x 20 x 400 | 200, 0, 200 |
| `SM_WallWindowSmall_200x400` | 200 x 20 x 400 | 100, 0, 200 |
| `SM_WallWindowSmall02_200x400` | 200 x 20 x 400 | 100, 0, 200 |

## Corners

Pivot at the OUTER corner, geometry running back from it.

| Part | Size | Pivot |
|---|---|---|
| `SM_WallRoundedCorner_200x300_90d` | 210 x 210 x 300 | -105, -105, 150 |
| `SM_WallRoundedCorner_400x400_90d` | 410 x 410 x 400 | -205, -205, 200 |

## Floors

22 cm thick, pivot at a corner with the walking surface at
pivot height, so a floor is placed at floor level.

| Part | Size | Pivot |
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

| Part | Size | Pivot |
|---|---|---|
| `SM_Pillar` | 56 x 56 x 330 | 0, 0, 165 |
| `SM_Beam_300` | 40 x 40 x 300 | 0, 0, 150 |
| `SM_Beam_400` | 40 x 40 x 400 | 0, 0, 200 |
| `SM_Concrete_Post_1_C` | 11 x 11 x 107 | 0, 0, 54 |
| `SM_MetalPost` | 17 x 15 x 102 | 0, 0, 51 |
| `SM_Concrete_SmallLthing` | (unmeasured) | |

Beams and pillars are centred in both horizontal axes with the
pivot at their base, unlike walls.

## Stairs and walkways

| Part | Size | Pivot |
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
road area, 108 placed parts used only five distinct parts:
`SM_Floor_400x400` (66), `SM_Wall_400x401` (27),
`SM_WallRoundedCorner_200x300_90d` (8),
`SM_WallDoorGarage_400x400` (6), `SM_Floor_1000x1000` (1).

So the designers work mostly at 4 m, and much of what looks like
building parts is road paving. Buildings are assembled square-on
and then turned to an arbitrary angle when placed (the dominant
angle in that sample was 5 degrees), which is why placed parts
never line up with the world axes.

Two caveats on that sample: it is small, and it came from a road
square rather than a building. Village and garage squares showed
richer use (the six-part `SM_BrikGarage_*` family, house windows,
12 wall sizes) and are the better place to study how rooms are
put together.

## Whole buildings

Separately from the parts, the game has entire buildings as one
mesh, which can only be placed, never assembled:
`SM_WoodenCabit_02` (6.3 x 6.6 x 8.3 m),
`SM_WatchTower_SM_ContainerHouse1`, `SM_House01_Var_A_Stairs`,
and the `SM_BrikGarage_*` family (garage, closed garage, left and
right doors, two tarps) which is a kit for one building type.

## Not building parts

The name prefixes overlap with props, so filter by folder rather
than by name where it matters: `SM_WallClock`, `SM_WallLamp`,
`SM_WallpaperRoll1` to `7`, `SM_WallZaslavFlag` all begin with
`SM_Wall` but are decoration.
