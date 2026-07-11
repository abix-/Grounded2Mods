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

OPERATOR-LOCKED 2026-07-11: buildings that DO something get
upgrades for how well they do it, borrowing Factorio's taxonomy:
PRODUCTIVITY, SPEED, EFFICIENCY, and QUALITY, each applying to a
building by what it does; passive structures (walls and the
like) get HEALTH REGEN. Tracks by structure function:

| Track | Applies to (by function) | Effect | Effect path |
|---|---|---|---|
| Reinforce | every destructible structure | max hit points x(1 + curve) | C# postfix on Prop.GetMaxDamage (LIVE) |
| Health Regen | every destructible structure (walls and other passives especially) | the structure heals its damage over time | C# driver tick: Prop.Damage is a public per-instance field; a slow pass heals tracked props by level rate |
| Expand | storage (MaxInventoryWeight > 0) | capacity x(1 + curve) | C# postfix on the max-inventory-weight read |
| Spikes | walls, fences, gates | melee attackers take damage per hit | C# postfix on the structure-melee-hit path |
| Speed | work props (WorkBench, Forge, Kiln, Still, Campfire) | crafting there runs faster | C# patch on the craft-progress rate (pin the method at build) |
| Productivity | work props | chance of extra product per craft | rides the existing craft hook (the product is in hand; roll a duplicate) |
| Efficiency | work props | chance a craft refunds its ingredients | C# patch on Recipe.UseIngredients (chance to skip consumption) |
| Quality | work props | better quality-tier odds for items crafted there | Rust: quality.rs craft_odds already computes odds; add the prop's track level to the surplus |
| Secure | storage (MaxInventoryWeight > 0) | a hostile taking (the mod's theft, predation, and tribute acts) can find the locks holding and leave that building's stores untouched | Rust: the shared stored-goods drain (common.rs carry_off_stored_goods) tests each building's locks on hostile call sites only; the C# shim owns the level and the roll (SecureBlocks, 5 percent per level capped at 50: never fully theft-proof). Willing loads (own wares, payments) never test locks |

Staged after those (each needs its own effect-path research):
Comfort (housing: rest quality), Insulate (housing: warmth),
Watch (towers: guard detection radius), Yield (well/garden
output). The catalog is designed to keep growing; each new
track is one effect patch plus a menu entry. A work prop ends
up with six tracks (Reinforce, Health Regen, Speed,
Productivity, Efficiency, Quality): deep exactly where the
building does the most.

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
- [x] LIVE-VERIFIED 2026-07-11 by the operator's own hands:
  menu entry visible, click upgraded the Well 3 -> 4 consuming 8
  real wood (log: "Well #190942 Reinforce -> 4 (8 Wood
  consumed)"), status-bar message shown, sidecar restored all 4
  upgraded structures on load. Operator: "i think its all
  working holy shit".
- [x] Committed.

### Task 3: the remaining first-build effects (BUILT 2026-07-11, compiles clean; live verify = Task 4)

- [x] Health Regen: driver-tick pass every 15s heals tracked
  structures through the public damage-fraction API (0.2 hp/min
  per level).
- [x] Expand: postfix on Prop.GetMaxInventoryWeight (only when
  the base capacity is nonzero).
- [x] Spikes: postfix on Prop.ApplyDamage: a character source in
  melee reach (radius-0, non-burning hits) takes SharpObject leg
  damage, 0.05/hit per level, capped at level 10, via the game's
  own Character.OnMeleeAttack.
- [x] Speed: postfix on CraftingProp.Craft adds track-scaled
  extra progress per tick.
- [x] Productivity: postfix on Recipe.CreateProduct: prop-carrier
  crafts roll (4 percent per level, capped 50) for an extra
  product spawned into the prop.
- [x] Efficiency: prefix on the innermost Recipe.UseIngredients:
  a roll (4 percent per level, capped 50) skips consumption
  entirely; the same lookup records the Quality handoff.
- [x] Quality: quality.rs adds the work prop's track level to the
  crafter's surplus via the C# TakeCraftQualityBonus handoff
  (recorded at ingredient time near the prop, consumed when the
  Rust craft job resolves).
- [x] Menu: entries per track by the structure's function (work
  props list six tracks; storage: Reinforce, Health Regen,
  Expand; fences and gates: Reinforce, Health Regen, Spikes).
- [x] Committed; deploy rides the next game-closed window.

### Task 4: live verify + re-rate

- [ ] Operator session: climb several levels on a wall, a chest,
  and a workbench; watch a zombie bleed on spiked walls; craft
  at the upgraded bench and watch the tier odds move
  (quality_status); save round-trip keeps everything.
- [ ] Re-rate the status row honestly; commit.

### Task 5: Secure (ninth track; BUILT 2026-07-11, both sides compile clean)

- [x] C#: TrackSecure on storage (same predicate as Expand, so
  chests and other containers list it in the menu), knobs beside
  the other track knobs, and the SecureBlocks(propId) roll (5
  percent per level, capped at 50 so stores are never fully
  theft-proof; a held lock logs to the player log).
- [x] Rust: carry_off_stored_goods takes `hostile`; the three
  takings against the owner's will (steal, predation, stranger
  tribute) test each building's locks and a held lock keeps that
  whole building's stores; the five willing loads (courier pay,
  vendor wares + payment, trade food + payment) never test.
  The stranger shakedown already targets the player camp, so a
  secured chest is felt gameplay on the next shakedown.
- [ ] Live verify (rides the Task 4 session): put Secure levels
  on a chest, force a shakedown or theft, watch the "Secure
  held" line and the chest keep its stores.
