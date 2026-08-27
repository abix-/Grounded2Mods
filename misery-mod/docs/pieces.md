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

### What the registry carries per asset (read live, 2026-08-27)

`FAssetData` is 0x68 bytes. Read off the running game with
`asset_data_bytes`, four static meshes side by side:

```text
+0x00  PackageName   FName    differs per asset
+0x08  PackagePath   FName    differs per asset
+0x10  AssetName     FName    differs per asset
+0x18  zero
+0x20  AssetClassPath package FName   0x5986   same for all
+0x28  AssetClassPath asset   FName   0x2A5A2  same for all ("StaticMesh")
+0x38  a heap pointer, DIFFERENT per asset   <- TagsAndValues
+0x50  an image address, same for all
+0x64  2
```

**The tags are there.** The pointer at +0x38 differs per asset,
which is the cooked tag map.

**And it does not have to be decoded.** Reading a shared
`TMap<FName, ...>` out of raw memory is real work;
`AssetRegistryHelpers::GetTagValue` is a function, 4 parms in 129
bytes, so it is callable through ProcessEvent the same way
`GetAssetsByClass` already is:

```text
GetTagValue(FAssetData, FName TagName, FString& OutValue) -> bool
```

Finding that needed a new control. `class_functions` reads a
class off a LIVE INSTANCE, and `AssetRegistryHelpers` is a static
Blueprint library that only ever has a CDO, so it could not see
it. `class_functions_by_name` looks the class up by name instead.
It is also the safe way to ask what a native engine class can do,
because `discover_class_detail` CRASHES on one (worldgen.md 10).

### Reading the tags is a dead end for now

Three routes tried, 2026-08-27, all blocked:

**Call `GetTagValue`.** It needs an `FName` for the tag name, and
nothing in the framework builds an `FName` from a string. Every
`FName` we have was read off an object that already existed.
Making one means either the engine's own constructor or a walk of
the name pool, and the pool is around half a million entries with
one leaked buffer per unique name resolved (`fname.rs`).

**Read the map's keys instead.** They are `FName`s we could
resolve without constructing any, so this would answer "which
tags exist" directly. But `FAssetDataTagMapSharedView` is a union
of a fixed-map handle and a heap map pointer, and our TMap reader
takes a UObject and a field offset, not a raw address. Decoding
it means chasing a pointer found in memory, which took the game
down three times last night.

**Guess the tag names.** Not evidence, and a wrong guess is
indistinguishable from a missing tag.

### Unblocked the same day

The `FName` problem was the only real block, and patternsleuth
already shipped the resolver for the engine's own constructor.
`ue::fname::from_str` now turns a string into an `FName`
(research.md 28), so `GetTagValue` can be asked for a tag by
name.

Which tag names this build has, read live 2026-08-27:

```text
ApproxSize    yes     Triangles   yes     Vertices     yes
Bounds        yes     Materials   yes     LODs         yes
MinLOD        yes     UVChannels  yes     PhysicsAsset yes
CollisionPrims yes    NaniteEnabled yes   BoundsExtent no
```

**`ApproxSize` and `Bounds` are both there.** Those are the two
that would carry a mesh's dimensions.

### ANSWERED: `ApproxSize` carries the dimensions

Read live 2026-08-27 with `asset_tags`, loading nothing:

```text
Sphere                     ApproxSize 320x320x320          Tris 528   Verts 323  LODs 1
SM_MediaPlateScreen        ApproxSize 0x100x100            Tris 2     Verts 4
SM_Derbis_B                ApproxSize 7x8x7                Tris 40    Verts 66
SM_mountain_background_02  ApproxSize 200000x200000x45159  Tris 9829  Verts 4920 LODs 2
```

**So the parts list is ONE REGISTRY PASS, not 1,500 blocking
loads.** Every mesh's size comes back as a `WxDxH` string, with
triangle and vertex counts, materials and LODs alongside, for
free and without touching the game thread for long.

Two things to carry forward:

- `Bounds` is null on every asset. `ApproxSize` is the one.
- `SM_MediaPlateScreen` reports `0x100x100`. A flat mesh has a
  ZERO dimension, so the classifier must treat zero as "flat",
  not as "missing".

`ApproxSize` is what its name says: approximate, and rounded to
whole units. Good enough to sort a 4 m wall from a 2 m one, which
is what the parts list needs. Anything needing exact bounds still
has to load the mesh, and now only that mesh.

### The parts list exists (2026-08-27)

`parts_list` writes every shipped mesh to a file, with nothing
loaded. Live:

```text
count 2407, half-extent in metres, y up
Block 885, Slab 380, Panel 320, Clutter 520, Post 144, Beam 158
no size: 0
```

Every one of 2,407 meshes has a size, and the whole pass takes
under a second.

**The classifier agrees with the names without being told any**,
which is the point of judging by proportion:

```text
SM_Wall_400x401      Panel   [0.10, 2.00, 2.00]
SM_WallDoor_400x300  Panel   [0.10, 1.50, 2.00]
SM_Floor_400x400     Slab    [2.59, 0.44, 2.87]
```

A 400x401 wall reads as a Panel 0.1 m thick, 4 m wide and 4 m
tall. A floor reads as a Slab.

**Careful with the numbers though.** `SM_Floor_400x400` measures
5.18 by 5.73 m, not 4 by 4. `ApproxSize` is the mesh's BOUNDING
BOX, so a tile with a lip or a skirt reads larger than its module
size. Fine for sorting parts; NOT the same as the module grid,
and the studs must not be placed from the bounding box alone.

Conversion, matching what `ue::pieces` already does: Unreal is
centimetres with z up, this crate is metres with y up, and
`PieceDef::extent` is a HALF-extent while `ApproxSize` is a full
size. So `mf(x,y,z) = ue(y,z,x) / 2 / 100`.

### How the tag is read

`AssetRegistryHelpers::GetTagValue(FAssetData, FName TagName,
FString& OutValue) -> bool`, 4 parms in 129 bytes:

```text
0x00  FAssetData   the whole 0x68-byte entry, BY VALUE
0x68  FName        the tag to ask for
0x70  FString      the answer, written by the engine
0x80  bool         whether the tag was there
```

The tag name must be an `FName`, which is why none of this was
reachable until `ue::fname::from_str` landed (research.md 28).

**2. The studs, and they come from the VANILLA BUILDINGS.**

The first plan was to derive attachment points from a piece's
measured bounds. That is wrong, and the parts list proved it:
`SM_Floor_400x400` measures 5.18 by 5.73 m because `ApproxSize`
is a bounding box and that tile has a lip. Measuring the mesh
more precisely does not help, because the lip is really there.
**A mesh has no field that says "I am a 4 metre module".** That
is not a property of its geometry.

It is a property of how the game PLACES them. Two adjacent floor
tiles sit 4 m apart while each is 5.18 m across, because they
interlock. So the module size is the SPACING BETWEEN NEIGHBOURS,
and the only place that exists is in real placements.

**The vanilla buildings give us the attachment points.** Read a
level's placed pieces with their transforms, group by mesh, and
the common distances between neighbours ARE the module. A tile
that always sits 400 cm from the next one is a 4 m module
whatever its box says and whatever it is called. The offsets
between DIFFERENT meshes are the attachment points, and the pairs
that occur are the instructions.

We cannot trust the names and we cannot trust the boxes. We can
trust where the designers put things.

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
