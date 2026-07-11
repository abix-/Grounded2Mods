# Settlement upgrades implementation plan

> Execute inline, task by task, operator watching. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** The board's rank-1 pillar (docs/status.md "Settlement
upgrades"): the player sinks hours and materials into making
their settlement stronger, and ALWAYS has more upgrades to do.
Model: State of Decay's facility system (levels, prerequisites,
per-facility bonuses), but MORE complex in the TYPES of upgrades
and the LEVELS of each type (operator-locked 2026-07-11). No
complicated UI: everything runs through the game's own
right-click action menu.

**Architecture:** MULTI-TRACK PER-STRUCTURE UPGRADES. Every
structure carries several independent upgrade TRACKS (reinforce,
spikes, expand, precision, ...), each with its own level, stored
PER PLACED STRUCTURE in a seed-keyed sidecar owned by the C#
shim. C# Harmony patches apply the effects at the game's own
stat reads and add the menu entries; the Rust side keeps
observability ops and the tie-ins to the mod's own systems
(quality, theft). This supersedes the earlier type-ladder design:
tracks compose (a wall at Reinforce 4 + Spikes 2), which a type
per combination cannot, structures do not stack and have stable
saved ids, and levels become UNBOUNDED with no generated XML at
all.

**Why more complex than State of Decay, concretely:** SoD caps
most facilities at 3 levels with one mod slot. Ours: many tracks
per structure, each track 10+ levels deep (knob; diminishing
benefits and rising costs decide the practical stop), skill
prerequisites per track and per level band, and costs that
diversify at higher levels. The always-more-to-do math: ~30
upgradeable structures in a camp x 2-4 tracks x 10+ levels =
hundreds of purchases per settlement, each a real material sink.

## Research findings

- (2026-07-11, decompile) A structure's hit points: per-instance
  Prop.Damage accumulates against GetMaxDamage() (reads
  PropPrototype.MaxDamage). Storage capacity:
  PropPrototype.MaxInventoryWeight. Both reads are patchable
  from the C# shim: a postfix multiplying by the instance's
  track bonus is the whole effect path.
- Structures persist with stable ids (BaseObject Id, serialized)
  and do not stack: per-instance state is safe, unlike items.
- THE ACTION MENU CHAIN (all patchable from the C# shim with
  direct type access): population in
  GameCursor.GetAvailableActions (the Repair entry at
  GameCursor.cs:4036 is the exact pattern: own-community check,
  Construction skill gate, disabled reasons); click dispatch in
  Hud's `case CursorAction.X:` switch (Repair at Hud.cs:2373);
  sentinel CursorAction values above the vanilla range for our
  entries; one label patch.
- Upgrade cost base: each structure's own
  RepairResourceType/RepairResourceNeeded (wood walls take wood,
  concrete takes cement), scaled by target level; higher level
  bands add secondary materials (knobs).
- (web, 2026-07-11) State of Decay 2 facilities: build in slots,
  upgrade levels gated by knowledge (skills), facility mods slot
  in for extra passives/actions. Sources: the SoD2 wiki
  (Facilities, Facility Mods), Windows Central base-building
  guide.

## The track catalog

First build (four tracks, effects verified patchable or already
ours):

| Track | Applies to | Effect | Effect path |
|---|---|---|---|
| Reinforce | every structure | max hit points x(1 + curve) | C# postfix on Prop.GetMaxDamage |
| Expand | storage (chests etc) | capacity x(1 + curve) | C# postfix on the max-inventory-weight read |
| Spikes | walls, fences, gates | melee attackers take damage per hit | C# postfix on the structure-melee-hit path |
| Precision | work props (WorkBench, Forge, Kiln, Still) | quality tier odds bonus for items crafted there | Rust: quality.rs craft_odds already computes odds; add the prop's track level to the surplus |

Staged next (each needs its own effect-path research first):
Comfort (housing: rest quality), Insulate (housing: warmth),
Watch (towers: guard detection radius), Efficiency (work props:
crafting time), Secure (storage: resistance against the mod's
own theft/robbery acts), Yield (well/garden output). The catalog
is designed to keep growing; each new track is one effect patch
plus a menu entry.

## Design decisions (locked)

- LEVELS: no hard cap (sidecar state, not types). Benefit per
  level diminishes (config curve), cost per level rises
  (config curve), secondary materials join at level bands
  (5+, 8+). The player stops when the price stops being worth
  it, and a richer player stops later: the sink scales.
- PREREQUISITES: per track skill gates that rise with level
  bands (Reinforce/Spikes: Construction; Precision/Efficiency:
  the recipe skill), shown with the vanilla disabled reasons.
- UI: the action menu only. Hovering an own structure lists one
  entry per available track: "Reinforce (+3): 6 Wood". No new
  screens, no new panels.
- STATE: seed-keyed sidecar owned by the C# shim (the patches
  need synchronous access in hot paths), atomic tmp-then-rename
  writes (the genome store's shape), prop-id keyed:
  `survivalist-mod.upgrades.seed<seed>.json`. Rust reads the
  same file for ops and tie-ins (Precision, later Secure).
- COSTS are REAL: consumed on click from the character's carried
  stack first, then camp stores (real Take calls). V1 is
  click-to-upgrade; the walk-over-and-work crew flow is a later
  polish pass.
- Chronicle line per upgrade (public, visible progress).
- PLAYER-ONLY first; AI camps can climb the same tracks later
  (symmetry), which also makes rich AI camps harder to crack.

## File structure

- Create: `unityforge/cs-shim-survivalist/Upgrades.cs` (the C#
  side: sidecar store, menu population postfix, click dispatch
  prefix, label patch, effect postfixes for Reinforce/Expand/
  Spikes)
- Modify: `survivalist-mod/src/quality.rs` (Precision: prop
  track level joins craft_odds surplus; reads the sidecar)
- Create: `survivalist-mod/src/upgrade.rs` (upgrade_status +
  upgrade_probe ops reading the sidecar + live prop stats)
- Modify: `survivalist-mod/src/lib.rs` (ops)

---

### Task 1: the risky mechanics, verified live first: PASSED 2026-07-11

- [x] C# skeleton: sidecar store + Prop.GetMaxDamage postfix +
  probe entries (Upgrades.cs; shim now references
  Assembly-CSharp: the game-typed layer compiles clean).
- [x] upgrade_probe live: Reinforce 3 on the player's Well took
  20.0 -> 45.725 hit points through the game's own getter
  (exact: x2.28625); Outhouse 5 -> 11.43, RabbitTrap 2 -> 4.57;
  the indestructible guard held on the Campfire (float.MaxValue
  untouched).
- [x] Save round-trip PASSED: after save + reload the Well
  (same id 190942) read 45.725 BEFORE any new write: prop ids
  are stable across saves, the sidecar reattaches, THE
  ARCHITECTURE IS SEALED.
- [x] Noted: shim changes land across operator restarts.
- [x] Committed (6e95a055).

### Task 2: the menu (population, dispatch, label)

- [x] Sentinel CursorAction values (9000+) + population postfix
  on GameCursor.GetAvailableActions: the hovered own-community
  structure gets "Reinforce +N: <cost> <resource>" with the
  Construction-skill gate greyed via the vanilla reason and the
  carried count shown when short. BUILT 2026-07-11, compiles
  clean against the game types.
- [x] Click dispatch prefix on Hud.OnSelectedAction: sentinel
  actions consume the repair resource for real from the
  character's carried stacks (Take + Delete), bump the track,
  persist the sidecar, status-bar line with the new hit points;
  short materials post the shortfall instead. V1 consumes from
  the CARRIED stacks only (camp stores later).
- [x] Label prefix on AvailableAction.GetCaption: sentinel
  entries render their SpeechText (and never reach the vanilla
  caption array index).
- [ ] Live verify (operator restart + eyes): hover shows the
  entry, click upgrades and eats the wood, gates grey out.
  Deploy pending the next game-closed window (shim DLL locks
  while the game runs).
- [x] Committed.

### Task 3: the remaining first-build effects

- [ ] Expand: postfix on the max-inventory-weight read (pin the
  exact method during the build).
- [ ] Spikes: postfix on the structure-melee-hit path (pin the
  method; attackers take track-scaled damage per swing, capped).
- [ ] Precision: quality.rs craft_odds gains the crafting prop's
  track level (the craft hooks already know the recipe; pin how
  the crafting prop reaches the odds call: likely via the craft
  job's carrier position or the recipe's RequiredPropToWorkOn).
- [ ] upgrade_status op: per-structure track levels, totals,
  last upgrade.
- [ ] Commit.

### Task 4: live verify + re-rate

- [ ] Operator session: climb several levels on a wall, a chest,
  and a workbench; watch a zombie bleed on spiked walls; craft
  at the upgraded bench and watch the tier odds move
  (quality_status); save round-trip keeps everything.
- [ ] Re-rate the status row honestly; commit.
