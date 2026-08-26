# MISERY world generation

> **Authoritative on:** how expedition areas are generated:
> the four generators, grids and tile sizes, the level pools
> (squares), square selection, and the world remix work.
> `research.md` covers everything else about the game's
> internals and points here for worldgen; do not duplicate
> worldgen findings there.

## 1. The four generators are four areas

`walk_class_chain(BP_WorldGeneration_Base_C)` returns four
subclassed instances, one per area. The active area is
whichever generator's `EmissionsPast` counter climbs; it
accumulates for the life of the save and never resets (it
survived a forced shining and a world refresh). Observed
2026-08-25: Factory stopped at 42 when the active area
switched to Meadows (43).

## 2. What a generator is

`BP_WorldGeneration_Base_C` is a grid-based level streamer:

| Field | Offset |
|---|---|
| `GridFirstIndex_X` / `GridLastIndex_X` | 0x2A8 / 0x2AC |
| `GridFirstIndex_Y` / `GridLastIndex_Y` | 0x2B0 / 0x2B4 |
| `EmissionCountForRefresh` | 0x2B8 |
| `TileSize` | 0x2C0 |
| `Levels` (array of FSoftObjectPtr) | 0x2C8 |
| `LevelsRefreshed` (array of FSoftObjectPtr) | 0x2D8 |
| `StreamingLevels` (array of ULevelStreaming*) | 0x2E8 |
| `EmissionsPast` | 0x2F8 |
| `Random Stream` | 0x2FC |

Functions: `GenerateNewRandomLevels`, `RunGenerationFromSeed`,
`UnloadLevels`, `UnloadStreamingLevels`,
`CheckIfAllLevelsArevisible`, `BeginCheckIfAllLevelsAreLoad`.

An area is a grid of streamed level squares, generated from a
seed, refreshed against the shining count
(`EmissionCountForRefresh`; 5 for Meadows).

## 3. Grids and tile sizes (live, 2026-08-25)

`research_spawners::dump_generator_grids`:

| Area | Grid | Squares | TileSize | Pool size |
|---|---|---|---|---|
| Factory | x -2..-1, y 6..7 | 4 (2x2) | 16500 | 9 |
| Bunker | x 0..2, y 0..2 | 9 (3x3) | 4800 | 9 |
| Meadows | x 3..5, y 3..5 | 9 (3x3) | 12000 | 18 (max 19) |
| Paneli | x 3..5, y 7..9 | 9 (3x3) | 12000 | 9 |

Squares are NOT interchangeable across all areas: a foreign
square only fits a grid with the same tile size. **Meadows and
Paneli match exactly (12000)**, so cross-area mixing between
those two has no geometry problem.

## 4. The level pools: every square in the game

`Levels` (+0x2C8) is each area's pool of preset squares. Pool
entry layout, measured live (`research_spawners::
dump_level_pools`): FSoftObjectPtr, stride 0x28:

| Offset | Size | Content |
|---|---|---|
| 0x00 | 8 | WeakPtr (zero until resolved) |
| 0x08 | 8 | PackageName FName (full /Game/... path) |
| 0x10 | 8 | AssetName FName (the level's short name) |
| 0x18 | 0x10 | SubPathString FString (empty) |

**Swapping a square is two 8-byte FName writes** (package +
asset). Meadows has one spare slot (18 of 19); the other pools
are full and would need `tarray_grow` (vendors research 24.12)
to append.

The complete catalog, dumped live 2026-08-25:

**Factory (16500):** L_LF_ElectricFactory, L_LF_CoolingTower_B,
L_LF_CoolingTower_A, L_Factory_Depo, L_Factory_Molokozavod,
L_Factory_TraktorniyZavod, L_CementFactory_Art,
L_Factory_Kotelnya_Railways, L_Gradirni_Art.

**Bunker (4800):** L_NuclearBunker01..07,
L_Bunker_ServerRoom_Electric, L_Bunker_Conservatory. The
bunker interior itself is rolled from a pool.

**Meadows (12000):** L_BombCrater, L_DandelionField,
L_Forest01..03, L_Kolhoz01, L_Meadows01,
L_Meadows_CurveRoad_Drainage, L_Meadows03, L_Meadows04,
L_Meadows_Drainage_Electric, L_River_LoggingCamp,
L_Village_Drenazh, L_Village_Dwarf_Hole, L_Village06,
L_VehCemetry01, L_VehCemetry_Bridge,
L_Garages_ElectricBuilding.

**Paneli (12000):** L_BombCraterTown01, L_Town01..03,
L_Town_Anomaly01, L_TownSwamp01, L_Garages02, L_Road01,
L_Anomaly_House.

### 4.1 LevelsRefreshed: the late-game pool (live, 2026-08-25)

`LevelsRefreshed` (+0x2D8) is a SECOND pool, used once the save
has been through world refreshes. **Only Meadows has one** (28
entries); Factory, Bunker, and Paneli have it empty.

12 squares exist ONLY in the refreshed pool, so they cannot
appear in an early-game world at all:

L_BigAntenna01, L_Camp01, L_Forest04, L_Forest05,
L_ForestDeadEnd, L_Garages01, L_GarbageAnomaly,
L_MilitaryAnomaly, L_Swamp01, L_Swamp02, L_SwampVillage01,
L_SwampVillage02.

Two squares are in `Levels` only: L_Forest01, L_Forest02.

So Meadows has 30 unique squares, and the game has 57 in total,
not the 45 in section 4. This is why a late-game Meadows world
contains swamps that are absent from the `Levels` catalog
(observed at emissions 62).

Consequence for mixing: pool writes must target the pool the
generator will actually read. A save past the refresh threshold
reads `LevelsRefreshed`; writes to `Levels` are then ignored.
(The 2026-08-25 Paneli experiments worked because Paneli's
refreshed pool is empty, so it always reads `Levels`.)

### 4.2 Square world coordinates (live, 2026-08-25)

`research_worldgen::square_world_bounds` compared NPC positions
against the grid cell in each square's name:

**A square's centre is (cell_x * TileSize, cell_y * TileSize),
and it extends TileSize/2 in each direction.**

Verified both ways: Meadows `4462_3_5.L_Swamp01` NPCs span
x 31043..39653, y 58436..61672 around cell (3,5) x 12000 =
(36000, 60000); Factory `4458_-1_7.L_LF_ElectricFactory` NPCs
span +/-7500 around cell (-1,7) x 16500 = (-16500, 115500).

Square names are `<worldid>_<cellx>_<celly>.L_<Preset>`, so any
live actor's owning square and world position are derivable
from its full name plus the generator's TileSize. Decorations
can be placed anywhere inside a square by arithmetic; no anchor
actor is needed.

## 5. Area selection (from research.md's original section 19)

Selection lives on `BP_GlobalManager_C`:
`CurrentGeneratedLevel` (+0x2C8, byte), `CustomBiomSelected`
(+0x2F8), `CurrentWorldSeed` (+0x2BC). `SelectRandomBiom`
rolls the area; `GenerateCustomBiom` takes the area number as
a parameter; `GenerateBiom` dispatches to the matching
generator. The number-to-area mapping is unmapped (2 was live
while Meadows was active, one data point).

With the game-thread call op working (research.md 26),
`GenerateCustomBiom` / `GenerateBiom` are callable on demand:
the mapping experiment and forced regenerations no longer wait
for shinings.

## 6. World remix paths (design notes, not yet attempted)

1. Cross-area square mixing: write foreign package/asset FNames
   into a pool entry before generation. First target: Meadows
   <-> Paneli. Unknowns: does GenerateBiom read the pool live
   or a copy; do roads/edges connect across areas (cosmetic
   risk).
2. New combinations: grid bounds are plain ints; extend them or
   duplicate pool entries for worlds the author never generated.
3. New content on existing squares: the spawn machinery
   (research.md 26.3) can spawn ANY actor class; a decoration
   plan overlays structures, containers, or anomalies onto a
   square.
4. A truly new level asset requires pak-level authoring; a
   different magnitude of work.

## 7. Forced regeneration works (confirmed 2026-08-25)

`research_worldgen::force_regenerate`, live:
`BP_GlobalManager_C:GenerateCustomBiom` (one byte parm, the
area number) called through the game-thread call op rebuilt the
expedition world on demand. Evidence: the scaling spawner saw
brand-new squares stream in under a new grid id (4444_ from
4442_) with a different layout, seconds after the call. The
global manager survived. No shining required.

Findings from the first run (area 2 -> 2, Meadows):

- **A forced regeneration increments EmissionsPast** (43 -> 44):
  it counts as a shining for the difficulty curve. Experiment
  sweeps climb the emission level one per run.
- `CurrentWorldSeed` (+0x2BC) did NOT change while the layout
  did, and the new world contained presets absent from the
  dumped `Levels` pool (L_Swamp01, L_SwampVillage01). The seed
  field is not the whole story; suspicion: `LevelsRefreshed`
  takes over after the refresh threshold.
- `current_level=2` while Meadows streams and accumulates:
  **2 = Meadows** (second data point).
- `research_worldgen::dump_worldgen_state` is the read-only
  snapshot: manager byte + seed, per-generator streaming level
  count and EmissionsPast.

### 7.1 The area number mapping (swept live, 2026-08-25)

`GenerateCustomBiom(n)` for n in 0..=4:

| n | Result |
|---|---|
| 0 | Bunker generates (9 levels streamed) |
| 1 | NOTHING generates (level byte set, no generator runs) |
| 2 | Meadows generates |
| 3 | Paneli generates |
| 4 | NOTHING generates |

Factory generated normally earlier the same day (a shining put
it at emissions 42), so Factory is almost certainly number 1
with a broken or different custom-generation path; open
question below.

Also learned: **EmissionsPast is the save's GLOBAL shining
count**, stamped onto whichever generator is active when it
ticks (Bunker jumped 0 to 45, Paneli 0 to 47). Reading the max
across generators (what the scaling spawner does) gives the
global count. The sweep itself advanced the save from 43 to 49:
every forced regeneration costs one tick of the difficulty
curve.

## 8. Cross-area square mixing works (confirmed 2026-08-25)

`research_worldgen::pool_swap_meadows_into_paneli`, live: the
Meadows square `L_VehCemetry_Bridge` was copied over Paneli's
`L_Town01` pool slot (one 0x28-byte element write), a Paneli
world was forced, and after two rerolls the foreign square
generated at grid cell 4,7 inside the Paneli grid with its NPCs
streamed: `4452_4_7.L_VehCemetry_Bridge` in the census.

Facts established:

- **Generators read the pool live**: a runtime pool write is
  honored by the next generation. No copy defeats it.
- **The pool write persists across regenerations** within a
  session; only a save reload resets it (soft object paths are
  plain data, nothing re-fetches them).
- Generation rolls each grid cell independently WITH
  REPETITION from the pool (duplicate squares in one world are
  normal), so any single entry misses a 9-cell world roughly a
  third of the time; reroll until placed.
- The mixed world generates and streams without errors with
  matching tile sizes (both 12000).

This is the whole mechanism for the mixed-pool area row: fill
a pool with entries from any same-size areas and generate.

### 8.1 The mixed-pool area works (confirmed 2026-08-25)

`research_worldgen::mixed_pool_area`, live: all nine Paneli
slots overwritten with a curated blend (6 Meadows squares:
Kolhoz01, VehCemetry_Bridge, River_LoggingCamp,
Village_Dwarf_Hole, BombCrater, Forest02; 3 Town squares:
TownSwamp01, Anomaly_House, Town_Anomaly01), then one forced
generation. The board came out mixed: L_Village_Dwarf_Hole
(Meadows) at cell 3,8 among TownSwamp01 and Anomaly_House
cells. Rolls repeat entries (Anomaly_House landed three
times), and the NPC census only proves squares that contain
NPCs; empty squares (Forest, BombCrater) do not show there.

A whole area assembled from squares of multiple areas is
therefore a solved problem for same-size pools: write nine
elements, regenerate.

### 8.2 Size-mismatch verdict: generates, unplayable (2026-08-25)

`research_worldgen::size_mismatch_probe`: L_CementFactory_Art
(16500) written into the Paneli (12000) grid streamed on the
first roll with its NPCs, no crash, no engine complaint. But
the operator's walk-through found the path out of the
expedition physically blocked: the square's extra 4500 units
of geometry plow into the neighboring cells. **Verdict:
cross-size mixing generates but produces untraversable worlds;
mixing stays restricted to matching tile sizes (Meadows <->
Paneli) unless a future idea handles the overlap.** The probe
restores the pool slot automatically after placement.

## 9. Open questions

- Why GenerateCustomBiom(1) does nothing when Factory
  generates fine under normal shinings: broken custom path,
  or Factory needs a different entry point (GenerateBiom
  after writing the byte, or an expedition-door flow).
- Whether GenerateBiom reads the pool live or copies it.
- Whether cross-area squares connect roads/edges sanely.
- What exactly switches a generator from `Levels` to
  `LevelsRefreshed` (EmissionCountForRefresh threshold?), and
  whether writes to the inactive pool are simply ignored.
- Why some freshly rolled squares report "spawned 0 extra
  NPCs" despite a non-zero plan (intermittent SpawnAIFromClass
  failure; also seen once on the Factory world).
