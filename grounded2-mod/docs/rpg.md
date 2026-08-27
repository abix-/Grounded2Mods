# RPG system

> **Authoritative on:** the RPG / level-up subsystem. XP, levels,
> skills, kill detection, persistence, the ImGui tab, and how the
> apply step writes skill values into the running game.

For the project's open work see [`todo.md`](todo.md). For the
distribution / loader story see [`ue4ss-port.md`](ue4ss-port.md).
For build instructions see [`building.md`](building.md).

## Vision

Inspired by Factorio's [RPG System](https://mods.factorio.com/mod/RPGsystem)
and the [War3CS / War3FT](https://war3cs2.wiki.gg/) line of
Counter-Strike Warcraft mods. **Classless, free-form skill spec.**
The catalog is a flat menu; the player decides what kind of player
they want to be by spending skill points on whatever combination
they like. There are no archetypes the mod enforces: a tank build
is "max armor + max health + max regen", a glass cannon is "max
attack damage + max crit + max move speed", a survivalist is "max
backpack + min hunger/thirst + max gather". The mod just provides
the levers.

## Player loop

```
   kill creature -> earn XP -> hit level threshold -> get skill point
   spend skill point -> skill level goes up -> stat changes -> kill better
```

XP comes from killing creatures (player kill or Buggy mount kill,
configurable scaling for Buggy). The player levels up via a
quadratic XP curve; each level grants one skill point. Skill points
are spent via the ImGui tab in UE4SS's debug overlay (default
`Insert` to summon).

## Vocabulary

Consistent across code, UI, logs, and docs:

| Term         | Meaning                                                    |
| ------------ | ---------------------------------------------------------- |
| skill        | An upgrade slot (Backpack, Lifesteal, Move Speed, etc).    |
| skill point  | The currency a player earns by leveling up.                |
| skill level  | How many points the player has spent on a given skill.    |
| level        | The player's character level (separate from skill levels).|
| XP           | Cumulative experience earned from kills.                   |

Match the Factorio RPG System / Diablo conventions.

## Catalog

The catalog lives in
[`grounded2-mod/src/rpg/skills.rs`](../grounded2-mod/src/rpg/skills.rs)
as the **single source of truth**. Each skill is one row in
`CATALOG` with:

| Field         | Notes                                                     |
| ------------- | --------------------------------------------------------- |
| id            | Stable string used as the persistence key.                |
| display_name  | Shown in the ImGui tab.                                   |
| max_level     | Caps at 100 for every skill.                              |
| effect        | An `EffectDef` referencing a static Effect impl (framework type or game-specific). Drives apply + format. |

Adding a new skill of an existing shape is one row. Adding a new
shape adds one Effect impl + static instance and references it from one CATALOG row in
`apply_skill` and `format_effect`.

### Current catalog (13 skills)

| Skill            | Domain     | Final value at level 100         |
| ---------------- | ---------- | -------------------------------- |
| Backpack         | survival   | +460 slots (40 vanilla -> 500)   |
| Hunger Resistance| survival   | -75% drain (1.0x -> 0.25x)       |
| Thirst Resistance| survival   | -75% drain                       |
| Attack Damage    | combat     | +300% damage (1.0x -> 4.0x)      |
| Armor            | combat     | -50% damage taken                |
| Move Speed       | movement   | +300% walk + sprint + swim       |
| Jump Height      | movement   | +300% jump-Z velocity            |
| Leap Distance    | movement   | +500% AirControl + boost mult + boost threshold |
| Glide Speed      | movement   | +300% MaxFlySpeed                |
| Fall Damage Resistance | survival | targets fall mitigation; field writes are confirmed, but full immunity is not verified yet |
| Impact Damage Resistance | survival | -100% environmental damage at L100. Per-level sqrt scaling. Catalog row: `trigger: &ON_DAMAGE_TAKEN`, `effect: &effects::IMPACT_REVERSAL` (`ImpactReversalEffect`). Per damage taken event the framework fires `TriggerCtx::DamageTaken`; the Effect short-circuits unless `victim_is_player` and `FDamageInfo.DamageType` name contains "Environmental", then subtracts `damage * sqrt(level/100)` from `event.victim_component`'s CurrentDamage. Does NOT touch fall damage, creature combat, or heals. Bandages work normally. |
| Max Health       | survival   | +200 HP via `Standard(PlayerSubcomponentAdditive)`. Captures vanilla MaxHealth on first sight, writes `vanilla + bonus * progress` on player CDOs + live pawn. |
| Health Regen     | survival   | +500% out-of-combat regen tick % + 6x tick rate (UGlobalCombatData) via `Standard(ClassFieldsMultiply)` with `(offset, exponent)` pairs. Exponent +1 for tick-pct (grow), -1 for tick-rate (shrink). |
| Lifesteal        | combat     | +90% of damage dealt healed back. Catalog row: `trigger: &ON_DAMAGE_DEALT`, `effect: &effects::LIFESTEAL` (`LifestealEffect`). The framework fires `TriggerCtx::DamageDealt` on every player-instigator hit; the Effect short-circuits unless `attacker_is_player && !victim_is_player && damage > 0`, then walks the live player HC and decrements `CurrentDamage` by `damage * 0.90 * sqrt(level/100)`. DisabledSkills toggle handled by `Tracker::fire` (skipped at dispatch). |

### Per-level scaling

Every skill scales with diminishing returns. The progress curve is
`level_progress(level) = sqrt(level / 100)`, so:

| Level | Progress |
| ----- | -------- |
| 1     | 10%      |
| 10    | 32%      |
| 25    | 50%      |
| 50    | 71%      |
| 100   | 100%     |

Sqrt feels generous early (every level matters) and naturally
flattens near the end (99 -> 100 is a small gain). The "final"
value above is multiplied by `level_progress(level)` to get the
in-game value at any given level.

### Future catalog additions

Domains we expect to populate as time permits. Each row is a
catalog entry plus (when needed) a new Effect impl in effects.rs.

- Survival: Gear Hardiness, Stamina Pool, Stamina Regen.
- Combat: Critical Chance, Critical Damage, Evasion / Dodge,
  Thorns.
- Movement: Walk Speed (separate from Move Speed if we want),
  Climb Speed.
- Utility: Gather Yield, Crafting Discount, Repair Discount,
  Hauling Capacity.

Targeting ~25 skills total when populated, similar to War3FT's
catalog scope (collapsed to a single flat menu).

## XP curve

Cumulative XP required to reach level N is `100 * N^1.8`, capped
at level 50. Per-creature XP is a small static lookup keyed on the
dying actor's BP class (Aphid 5, Weevil 15, Spider 75, Boss 750,
unknown 5). Lookup table in
[`xp.rs`](../grounded2-mod/src/rpg/xp.rs).

## Kill detection

We hook `Maine.HealthComponent`'s ProcessEvent slot. The kill
signal is `MulticastHandleEffectsWithDamageFlags`, a NetMulticast
RPC fired on every damage hit, whose `DamageFlags` int parameter
carries the `EDamageInfoFlags::KillingBlow = 2` bit when the hit
is lethal.

When the bit is set, we read `LastDamageInfo.InstigatorController`
(at +0x3B0 on the HealthComponent, +0x20 inside FDamageInfo) to
identify the killer, then classify into three buckets:

| Bucket | Match                                                         | Action            |
| ------ | ------------------------------------------------------------- | ----------------- |
| Player | Instigator class chain contains "PlayerController".           | Award full XP.    |
| Buggy  | Instigator (or its possessed Pawn at +0x308) class contains "Buggy". | Award `settings.rpg.buggy_kill_xp_multiplier * XP` (default 1.0). |
| Other  | Any other AIController; enemy-vs-enemy fights.                | No XP, no record. |

Filter: the dying actor must be an `ASurvivalCreature` subclass.
Buildings, props, and the player's own death go through different
code paths; this filter ensures we only award XP for creature
kills.

### Why not `HealthComponent.Kill`?

`Kill` is `Final|Native`. The engine's internal damage path calls
the C++ method directly, bypassing ProcessEvent. Hooking it
catches nothing. The multicast path was the right answer; see
[`changelog.md`](changelog.md) for the full investigation.

### Known issue: pkg(0) instigator

Some legitimate player kills resolve `InstigatorController` to
`/Script/CoreUObject (Package)` (object index 0, the
default-constructed FWeakObjectPtr). Player loses XP for those
kills. Tracked in [`todo.md`](todo.md). Investigation plan:
hook `ApplyDamageFromInfo` upstream and cache the killer per
HealthComponent before the multicast fires.

## Persistence

PlayerState is stored in
`<DLL_dir>/saves/<playthrough-guid>.json`. The guid comes from
`AInGameGameState.PlaythroughGuid` at +0x32C; it's stable across
save renames so the file follows the save reliably.

Schema (open shape via `#[serde(default)]` so older files load
into newer code without losing the player's progress):

```json
{
  "xp": 0,
  "level": 1,
  "skill_points": 0,
  "skill_levels": { "backpack": 5, "lifesteal": 3 },
  "kill_count": 0,
  "last_killed": ""
}
```

The save loader is `rpg/world_loader.rs`: a 1Hz poller that
watches `save_slot::current_slot_key()` and drives the
`tracker::activate_slot` / `deactivate_slot` transitions. When a
save activates, `apply::apply` runs immediately so skill levels
are reflected in CDOs from the moment the player is in-world.
Save-swap (Some(a) -> Some(b)) is handled too.

Why eager (not lazy) load: skill levels affect player stats from
spawn, not from the first kill. A lazy "load on first kill"
shortcut was used during Spike B but is gone.

## Apply step

The apply step writes skill-leveled values into the game. Lives
in `rpg/apply.rs`. Two public entry points:

| Function                                  | Use                                                      |
| ----------------------------------------- | -------------------------------------------------------- |
| `apply(state, settings)`                  | Walks every skill in the catalog. Called on `activate_slot`. |
| `apply_one(state, settings, skill_id)`    | Re-applies just one skill. Called from `spend_skill_point`. |

### Catalog: Effect trait + per-type structs

After Phase 3 wave 2 (2026-05-10), 9 of 13 catalog skills route
through the Effect library in `ueforge::rpg::effect`. Ueforge's
canonical 8-variant menu. The framework owns the dispatch + the
vanilla cache + the CDO + live-pawn walks; g2rpg's `apply_skill`
shrunk from 11 arms to 4.

Effect declaration shape:

```rust
// One Effect impl per game-specific operation (effects.rs):
    /// 9 of 13 catalog skills route through ueforge.
    // Framework Effects (PlayerFloat, Subcomponent*, Runtime, ClassFieldsMultiply, StatusEffectApply) used directly via static instances.

    // Game-specific operations live as types implementing Effect:
    BackpackSlots { max_bonus_slots: i32 },
    SurvivalDrain { /* settings-multiplier composite */ },
    PlayerFallDamageReduction { /* HC + GMS + SMMC + UFunction */ },
}
```

The four arms in `apply_skill`:

| Arm | Skills | Path |
|---|---|---|
| `Standard(e)` | Attack Damage, Armor, Move Speed, Jump Height, Glide Speed, Leap Distance, Health Regen, Max Health, Lifesteal, Impact Damage Resistance | `e.apply(level, max_level, &PLAYER)`. Ueforge owns it. |
| `BackpackSlots` | Backpack | `patch::run(settings.slot_count + skill_bonus)` (player-only InventoryComponent CDO). |
| `SurvivalDrain` | Hunger, Thirst | Walk SurvivalComponent CDO, write `vanilla * settings_mult * (1 - skill_red * progress)`. |
| `PlayerFallDamageReduction` | Fall Damage Resistance | Multi-component composite: HC ratio + GMS CDO + SMMC live + `UpdateCustomSettings` UFunction. |

Live-damage skills (Lifesteal + Impact Resistance today;
Critical / Evasion / Thorns to come) ride on the
`damage::DamageHook` framework via the event-driven trigger
system. Each is a catalog row with `trigger: &ON_DAMAGE_DEALT`
or `&ON_DAMAGE_TAKEN`; the framework `Tracker::fire(ctx)` fans
events out to subscribed Effects. `G2DamageBinder` in
`kill_hook.rs` calls `TRACKER.fire(DamageDealt)` in `before` and
`TRACKER.fire(DamageTaken)` in `after`; the Effect impl
(`ueforge::rpg::LifestealEffect`,
`ueforge::rpg::ImpactReversalEffect`) does the actual heal /
mutate / reflect via TypedField writes on the resolved component.

CDO writes propagate to newly-spawned instances. Movement skills
also mirror the same writes onto the *current* player pawn (the
framework's `PlayerSubcomponentMultiply` variant does this in
both directions). Lifesteal (post-application heal) lives via
the `ON_DAMAGE_DEALT` trigger + a direct `CurrentDamage`
decrement on the live HC from inside `LifestealEffect::apply`.

### Fall damage and environmental damage

Grounded 2 has two distinct self-damage paths. Fall Damage Resistance
mitigates the first; Collision / Impact Damage Resistance (planned)
mitigates the second. The full pipeline reference. HealthComponent
layout, FDamageInfo discriminator, multicast surfaces, native call
order, every approach tried that did not work, and the velocity-stomp
fix. Lives in [`damage.md`](damage.md). Read that doc before adding
any skill that touches damage.

### How damage skills attach to the engine (plain language)

Most of our skills modify some stat on the player. Damage taken,
damage dealt, lifesteal, max HP, etc. Grounded 2 already has a
built-in mechanism for "this player has +X to stat Y": **status
effects**. Every gear bonus, perk, food buff, and environmental
debuff in the game is implemented as one of these. The bonemeal
helmet's +HP, a perk's +crit chance, a sour candy's lifesteal.
all status effects under the hood.

Each effect is a row in a single big spreadsheet (a "data table")
the game ships called `Table_StatusEffects`. Rows have columns
like `Type` (which stat), `Value` (how much), and `Duration`. The
engine continuously sums up active effects per stat type and
applies the result to gameplay. E.g. fall damage is multiplied
by the player's current `FallDamage` modifier, which is the sum
of every active status effect of that type.

When we want a skill like Fall Damage Resistance to do something,
the cleanest path is to add a status effect to the player whose
`Type` and `Value` match what we want. The engine's existing
designer-facing knobs do the rest.

[`damage.md`](damage.md) "Status Effect system" has the full
reference: which stat types exist, which combine semantic each
one uses (additive vs multiplicative), the data table layout,
and the implementation plan for adding effects from our mod.
That doc is the load-bearing one for any future skill that
modifies a player stat.

Skill-side wiring for the two damage-mitigation skills (different
mechanisms because the underlying damage paths differ. See
[`damage.md`](damage.md)):

- **Fall Damage Resistance** uses
  `PlayerFallDamageReductionEffect` (in effects.rs) plus a PE hook in
  `fall_hook.rs`. On player `OnLanded`, scales
  `CharMovementComponent.Velocity.Z` by `(1 - reduction)` before
  forwarding to the original BP event. Native `ApplyFallDamage`
  reads the mutated velocity live and produces scaled / zero damage.
  Validated: -3431 cm/s landing at level 100 -> zero damage.
- **Impact Damage Resistance** uses
  `ueforge::rpg::ImpactReversalEffect`
  subscribed to `&ON_DAMAGE_TAKEN`. No CDO write. The
  `damage::DamageHook` binder fires `TriggerCtx::DamageTaken`
  via `TRACKER.fire`; the Effect identifies environmental events
  by `FDamageInfo.DamageType` class name containing
  "Environmental" and subtracts `damage * sqrt(level/100)` from
  the player HC's `CurrentDamage` post-application. Bandages
  work normally because the reversal only fires on environmental
  damage, not heals or creature combat. This replaced the older
  binary-mask approach (`RequiredDamageTypeFlags = 0xFFFFFFFF`)
  which blocked bandage heal-ticks; see changelog 2026-05-09
  for the migration. Full sqrt-curve scaling now works (level
  1 ~10%, level 100 = 100%).

The `UStatusEffectComponent` at +0x1378 is still the
**canonical long-term backing** for stat-shaped skills (Fall,
Impact, Lifesteal, Crit, Thorns, Max Health, Armor, Attack
Damage all map to existing `EStatusEffectType` enum values).
The `StatusEffectApply` Effect type in ueforge
covers that surface; migration of individual skills from
CDO-write to status-effect is tracked in
[`todo.md`](todo.md) and [`damage.md`](damage.md).

## ImGui tab

UE4SS exposes an ImGui debug overlay; we register an "RPG" tab via
`CppUserModBase::register_tab`. The tab shows level, XP progress
bar, unspent skill points, and a row per skill with left-anchored
`+1` / `+10` / `-1` / `-10` buttons, an `on` checkbox, and effect
text ("level 5 / 100  +30 slots  (next: +33 slots)").
Always-visible HUD overlay (e.g. XP bar in the corner during
gameplay) would need a custom UMG/HUD path; deferred.

`-1` / `-10` refund spent points: the level decrements, the
points are credited back to the unspent pool, the apply step runs
again, and the new state is saved. The buttons disable at level 0.

**Caveat**: refunding back to level 0 on `PlayerHealthCompU32Mask`
(Impact Damage Resistance) leaves the engine value stale until the
next world load. The apply step early-returns at level 0 and
there is no captured vanilla mask to restore. Refunds to level >= 1
take effect immediately. All other skill shapes recompute cleanly
at level 0 because their formulas naturally produce the vanilla
value.

The `on` checkbox toggles a per-skill enable flag (process-global,
not persisted). Disabling treats the skill as level 0 (vanilla
values) without refunding the spent points. Handy for dropping a
buff like Leap Distance on demand without losing progress. Toggling
fires the apply step so the change is immediate.

The tab also has a debug footer with `+5 skill points` and
`+50 skill points` buttons so combat / movement skills can be
tested without grinding XP.

## Code map

| File                                    | Authority                                            |
| --------------------------------------- | ---------------------------------------------------- |
| `rpg/skills.rs`                         | Catalog + per-level math + format_effect dispatch.   |
| `rpg/apply.rs`                          | Apply step: Tracker iterates the catalog and calls `effect.apply` per row; the 3 game-specific Effect impls live in effects.rs. |
| `rpg/tracker.rs`                        | Thin shim over `ueforge::rpg::Tracker<A>`. |
| `rpg/world_loader.rs`                   | Thin shim over `ueforge::rpg::SlotPoller`. |
| `rpg/save_slot.rs`                      | Resolves the playthrough GUID for the active save.   |
| `rpg/kill_hook.rs`                      | `damage::DamageBinder` impl: kill credit, damage trace, and trigger dispatch. |
| `rpg/fall_hook.rs`                      | OnLanded velocity-stomp + status-effect row reads.   |
| `rpg/xp.rs`                             | `xp::CURVE` + `BESTIARY` (table data only; framework owns the lookup). |
| `rpg/applier.rs`                        | `impl RpgApplier for GameApplier`. |
| `rpg/tab.rs`                            | One-line forward to `ueforge::rpg::tab::render`. |
| `inv_hook.rs`                           | `inventory::viewport::ViewportBinder` impl: Maine UFunction wiring (GetInventoryItems, BPF helpers, InitializeItemSlot, KismetInput wheel delta). Algorithm + state owned by ueforge. |

## Settings

`rpg.buggy_kill_xp_multiplier` (default 1.0) scales XP awarded
when the player's Buggy mount makes the killing blow.

The two non-RPG settings (`inventory.slot_count`,
`survival.hunger_multiplier`, `survival.thirst_multiplier`) are
the **base** values that skills layer on top of. Defaults are
vanilla (40 / 1.0 / 1.0); user can override for a starter buff.
Skill levels then add (backpack) or further multiply (survival)
on top of the user's base.

## What's next

Open work tracked in [`todo.md`](todo.md). Highlights:

- Live-instance writes for remaining combat-side skills.
- Collision / Impact Damage Resistance so high-mobility builds stop
  dying when slamming into terrain and plants.
- Health Regen as the next survivability skill after the movement
  safety pass.
- pkg(0) instigator bug fix.
- Catalog expansion (Crit, Evasion, Max Health, Stamina, Gear
  Hardiness, etc.).
- XP curve / per-creature table tuning through real play.
- Optional: global ProcessEvent hook for richer event reach
  (level-up confirms, dialog events, etc.).
