# UE5 status-effect system reference

> **Authoritative on:** the universal UE5 design pattern for
> stat modifiers in RPG-style games. Every UE5 RPG /
> survival / action game uses some variant of this shape; the
> class names + enum values + data-table paths differ per game,
> but the **pattern is the same**. ueforge's
> `StatusEffectApply` variant + `ue::status_effect`
> module bind to it.
>
> For Grounded 2's specific layout (Maine offsets, enum
> values, the validated `Table_StatusEffects` data table) see
> [`../../grounded2-mod/docs/damage.md`](../../grounded2-mod/docs/damage.md)
> "Status Effect system". For Outworld Station's surface (when
> we wire it) see the ows-tweaks docs.

## The pattern

In most UE5 RPG games every gameplay modifier you see. Gear
bonuses, food buffs, perks, debuffs, status effects, sizzle /
chill / fire. Is implemented as a **status effect** under
the hood. The UI calls them different things; the engine
wires them all through one shared mechanism.

Three pieces:

1. **`U<X>StatusEffectComponent`**. An actor component
   attached to the player / characters. Holds the live list
   (`TArray<UStatusEffect*>`) of every effect currently active
   on this owner.
2. **`U<X>StatusEffect`**. One live effect instance. Does
   NOT store the stat type or value as instance fields;
   instead it holds an `FDataTableRowHandle` pointing at a
   row in a global data table. **Row-driven.**
3. **A `UDataTable`** holding `F<X>StatusEffectData` rows.
   one row per effect template (e.g. "bonemeal helmet +10
   HP", "spicy meal +10% damage", "sizzle 5 damage/sec").
   Columns are typically `Type` (`E<X>StatusEffectType`
   enum), `Value` (f32), `Duration`, etc.

When native damage code asks "what's this player's current
fall-damage modifier?" it walks the player's status-effect
list, sums (or multiplies, depending on the stat's combine
semantics) every entry whose `Type == FallDamage`, and
applies the result to incoming damage. The list IS the public
API; the engine consults it on every damage event.

## Why mods should plug into this

The natural temptation is to write scalar fields directly on
the HealthComponent / movement component / etc. That works,
but those writes are coarse and don't compose:

- Status-effect-backed values stack correctly across multiple
  sources (gear + perk + skill all contribute to the same
  stat).
- The engine reads the status-effect list on every damage
  event. Proportional scaling along a `sqrt(level/100)`
  curve falls out naturally if the mod writes a row value
  proportional to skill level.
- Vanilla heal multipliers / damage gates / status effects
  the game already implements compose with the mod's effects
  via the same code path.
- Replication is handled by the engine: status effects are
  typically `Net + RepNotify` so clients see consistent state.

Writing `RequiredDamageTypeFlags = 0xFFFFFFFF` to block
environmental damage *works*, but it also blocks bandages
(any damage with `type_flags = 0`, which includes negative
damage = healing). A status effect with `Type =
DamageReductionMultiplier` filtered by the environmental
`DamageTypeFlags` bitmask reduces only environmental damage
without touching heals. **Same outcome, no collateral.**

## The native query path

Component public surface (UFunctions, callable via
ProcessEvent):

```cpp
void AddEffect(UStatusEffect* StatusEffect);
UStatusEffect* CreateAndAddEffect(const FDataTableRowHandle& Row);
void RemoveEffect(UStatusEffect* StatusEffect, bool bBroadcastChangedEvent);
bool CanAddEffect(const FDataTableRowHandle& Row) const;

float GetValueForStat(EStatusEffectType StatType, bool bTemporaryEffectsOnly) const;
float GetValueForStatForApplicationTags(EStatusEffectType StatType, AActor* TargetToCheck) const;
float GetValueForStatForDamageTypeFlags(EStatusEffectType StatType, int32 Flags) const;
bool HasStatusEffectOfType(EStatusEffectType StatType) const;
int32 GetNumStatusEffectOfType(EStatusEffectType StatType) const;
```

`GetValueForStatForDamageTypeFlags(StatType, Flags)` is the
"proportional, type-filtered" getter. Native damage code passes
the incoming damage's type-flag bitmask and gets back the
summed (or multiplied) modifier across all matching effects on
the actor.

## Combine semantics

`EStatusEffectValueType` is universally two-state plus None:

```cpp
enum class EStatusEffectValueType : uint8 {
    None     = 0,
    Add      = 1,
    Multiply = 2,
};
```

Per stat, the engine either sums all active effects' values
or multiplies them. Vanilla baselines differ by combine type:

- **`mul` stats** start at `1.0` (so no effect = no change).
  `FallDamage`, `AttackDamage`, `DamageReduction` typically.
- **`add` stats** start at `0.0` (so no effect = no
  contribution). `MaxHealth`, `LifeSteal`,
  `CriticalHitChance`, `DamageReductionMultiplier` typically.

The mod's row Value for a "shrink fall damage" effect at
`Type=FallDamage (mul)` should be `< 1.0`; for a "boost crit"
effect at `Type=CriticalHitChance (add)` it should be `> 0.0`.

## Implementation paths (cheapest to most work)

1. **Mutate an existing data-table row at runtime.** Find a
   row whose `Type` matches what you want (e.g.
   `Type = FallDamage`), write your skill-scaled `Value` to
   its `+offset_of_value` float, then `CreateAndAddEffect`
   that row handle on the live component. Cheapest. Risk:
   the row is shared; mutating it affects any other system
   using the same row.
2. **Pick a benign / unused row.** Same as (1) but pick a
   row no other system reads. Often a `Type = Generic = 0`
   row or one whose `ApplicationTags` we can flip so vanilla
   code skips it. Lower risk than (1) if such a row exists.
3. **Inject a new row at runtime.** The data table is a
   `UDataTable` UObject; rows live in a `TMap<FName,
   uint8*>` internal field (typically at `+0x30`). Append a
   new entry at runtime keyed by a mod-specific name,
   populate with our row-struct bytes, then reference it.
   Most invasive, also most stable. No collision with
   vanilla rows.
4. **Manual `UStatusEffect` subclass with overridden
   getters.** Create a subclass UClass at runtime that
   overrides `GetStatusEffectType()` / `GetValueForStat()`.
   Engine sees the subclass return our values. Heaviest
   path; UE class manipulation at runtime is nontrivial.

For (1) and (2) ueforge ships
[`StatusEffectApply`](../src/rpg/effect.rs).
declare a catalog row with the table-finder, component class,
row FName, value-at-max, and a `VanillaCache<u64, f32>`; the
framework handles mutate-row + invoke `CreateAndAddEffect`.

For inspection, [`ue::status_effect::read_active`](../src/ue/status_effect.rs)
walks a live actor's component and effect array, resolves each row handle,
and reads its Type and Value. The game supplies all layout offsets and the
maximum number of entries to inspect.

## Probing a new game

To wire this against a game ueforge hasn't been tested on:

1. **Find the table.** Grep the SDK for `Table_StatusEffects`
   or similar; alternatively `walk_class("DataTable")` and
   look at full names.
2. **Find the component class.** Grep the SDK for
   `StatusEffectComponent` or look for a component on the
   player pawn whose class name contains "StatusEffect".
   Record the offset of the component pointer on the pawn.
3. **Find the row struct layout.** From the SDK header, look
   for the struct extending `FTableRowBase` that the data
   table references. Record offsets for `Type` (typically a
   `uint8`), `Value` (typically `f32`), and any combine /
   filter fields.
4. **Find the enum.** The `E<X>StatusEffectType` enum lists
   every stat the engine knows about. The values for
   `FallDamage`, `AttackDamage`, `MaxHealth`, `LifeSteal`,
   etc. are the integers your mod will pass to
   `GetValueForStat`-style queries.
5. **Validate via a probe.** Use ueforge's debug HTTP +
   `client` to walk the table, dump a few rows,
   confirm row-struct offsets match. The probe pattern lives
   in `grounded2-mod/tests/explore_status_effect_rows.rs` as
   a reference.

## See also

- [`rpg.md`](rpg.md) "StatusEffectApply". The
  ueforge catalog variant that binds to this pattern.
- [`testing.md`](testing.md) -- `client` helpers
  for probing tables + rows.
- [`../../grounded2-mod/docs/damage.md`](../../grounded2-mod/docs/damage.md)
  "Status Effect system". The Grounded 2 / Maine
  implementation: specific offsets, enum values, validated
  probe output, the live `Table_StatusEffects` instance.
