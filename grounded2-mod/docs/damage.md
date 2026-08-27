# Damage system

> **Authoritative on:** Grounded 2's damage pipelines as observed at
> the SDK / runtime level. What goes through `FDamageInfo`, what
> bypasses it, where `CurrentDamage` is written, which surfaces are
> reachable via UE4SS ProcessEvent hooks and which are not. This is
> the reference any skill that mitigates or routes damage should
> read first.
>
> For skills that consume this surface see [`rpg.md`](rpg.md). For
> the build / loader story see [`ue4ss-port.md`](ue4ss-port.md) and
> [`building.md`](building.md).

## Three distinct self-damage paths

High-mobility players in Grounded 2 die to three different things,
and they are *not* the same code path:

| Failure mode    | DamageType class            | src_type | DamageSource example | Pipeline |
| --------------- | --------------------------- | -------- | -------------------- | -------- |
| Fall damage     | `<null>`                    | 0        | none populated        | Native, bypasses `FDamageInfo`. Engine writes `CurrentDamage` directly. |
| Plant / terrain collision | `BP_EnvironmentalDamage_C` | 2 | (foliage / world geo) | Standard `FDamageInfo` pipeline through `ApplyDamageFromInfo`. Triggered by ramming foliage at high speed. |
| Hazard zones | `SurvivalDamageType` (base class) | 2 | `BP_Hazard_*_C` (e.g. `BP_Hazard_PetRestriction_Labs_C`) | Standard `FDamageInfo` pipeline. Triggered by entering / standing in a damage volume (Lab restriction barriers, sizzle/chill zones, etc.). |

Both ultimately surface a
`UHealthComponent::MulticastHandleEffectsWithDamageFlagsAtOwnerLocation`
event for cosmetic effects. The discriminator is
`HealthComponent.LastDamageInfo` (+0x3B0): empty for fall damage,
fully populated for environmental and normal damage. So the two
paths share their final notification surface but diverge entirely
upstream.

## HealthComponent layout (relevant fields)

`UHealthComponent` (Maine_classes.hpp:42186), held by every damageable
actor. Player's pointer at `ASurvivalCharacter +0x1340`.

| Offset  | Type        | Field                                 |
| ------- | ----------- | ------------------------------------- |
| +0x00EC | float       | `BaseDamageReduction` (Armor skill)   |
| +0x0328 | float       | `MaxHealth`                           |
| +0x032C | float       | `CurrentDamage` (Net, RepNotify)      |
| +0x03B0 | FDamageInfo | `LastDamageInfo` (0xE8 bytes)         |

Player health = `MaxHealth - CurrentDamage`. The runtime updates
`CurrentDamage` on every hit, increases it for damage, decreases it
for heals.

## FDamageInfo layout

Standard damage record (Maine_structs.hpp:4810, 0xE8 bytes). When
populated, every field below is filled. When empty (fall damage), the
whole struct stays zeroed and the runtime still writes `CurrentDamage`
without it.

| Offset  | Type                          | Field                       |
| ------- | ----------------------------- | --------------------------- |
| +0x0008 | FVector                       | `HitLocation`               |
| +0x0020 | TWeakObjectPtr<AController>   | `InstigatorController`      |
| +0x0028 | TWeakObjectPtr<UObject>       | `DamageSource`              |
| +0x0030 | TWeakObjectPtr<AActor>        | `Target`                    |
| +0x0038 | EInflictDamageStyle (u8)      | `InflictStyle`              |
| +0x0040 | TSubclassOf<UDamageType>      | `DamageType`. The discriminator |
| +0x004C | TWeakObjectPtr<UAttack>       | `OriginAttack`              |
| +0x006A | EDamageSourceType (u8)        | `DamageSourceType`          |
| +0x0070 | int32                         | `DamageFlags`               |
| +0x0078 | FDamageData                   | `OriginDamageData`          |

For damage that goes through `ApplyDamageFromInfo`, the struct is
populated *before* `CurrentDamage` is written. So reading
`LastDamageInfo` at any post-write PE event tells you what just hit.

## Multicast surfaces (PE-reachable)

`UHealthComponent` exposes three multicast variants for damage
effects, all `Net|NetMulticast|Native|Event|Const`. `Const` means
they cannot modify state. They fire *after* the damage write as
client-side notifications.

| Function | Parm layout |
| -------- | ----------- |
| `MulticastHandleEffectsWithDamageFlags` | `HitLocation` +0x00 (FVector_NetQuantize 0x18), `Damage` +0x18 (f32), `DamageFlags` +0x1C (i32), `DamageTypeFlags` +0x20 (u32), `PlayEffectType` +0x24 (u8) |
| `MulticastHandleEffectsWithDamageFlagsAtOwnerLocation` | `Damage` +0x00 (f32), `DamageFlags` +0x04 (i32), `DamageTypeFlags` +0x08 (u32), `PlayEffectType` +0x0C (u8) |
| `MulticastHandleEffectsWithDamageFlagsAndInflictStyleAtOwnerLocation` | Same as above plus `InflictStyle` +0x0D (u8) |

Skipping `original.call` on any of these only suppresses cosmetic
effects, not damage. The damage is already in `CurrentDamage` by
the time these fire. Confirmed by `before/after CurrentDamage`
snapshot around `original.call`: delta is zero across all three.

`DamageFlags` carries `EDamageInfoFlags::KillingBlow = 2` when the
hit is lethal; this is the bit `kill_hook` uses for kill detection.
`DamageTypeFlags` is the `ESurvivalDamageTypeFlags` bitmask.
empty for fall damage, application-specific for typed damage.

## Fall damage path

### Event order in one engine tick

| Step | Event | PE-reachable? |
| ---- | ----- | ------------- |
| 1 | `UCharacterMovementComponent::PhysFalling` ticks | no (native) |
| 2 | `UCharacterMovementComponent::ProcessLanded` | no (native) |
| 3 | `ACharacter::Landed(Hit)` -> `OnLanded` BP event | **YES** |
| 4 | `ASurvivalCharacter::ApplyFallDamage()` reads `CharMovementComponent.Velocity.Z` and computes damage | no (engine direct C++ dispatch from `Super::Landed` native frame) |
| 5 | `UHealthComponent::ApplyDamage(...)` writes `CurrentDamage` | no (engine direct C++ dispatch) |
| 6 | `MulticastHandleEffectsWithDamageFlagsAtOwnerLocation` fires (post-damage cosmetic) | YES |
| 7 | `MulticastFallEffects(SurfaceType, VelocityZ)` | YES (cosmetic) |
| 8 | `BndEvt__HealthComponent_..._DamagedDelegate` (on the BP class) | YES |

The native fall path runs entirely between step 3 and step 6 with no
PE entry. `ApplyFallDamage` and `ApplyDamage` are both
`Final+Native+BlueprintCallable` UFunctions, but the engine calls
their C++ implementations directly when invoked from a native frame.
The UFunction's `native_func` slot is consulted only for BP /
ProcessEvent calls.

### Observed trace (level 100 fall, pre-fix)

```
OnLanded                                 CD=14.59  (suppressed)
MulticastFallEffects        pre/POST     14.59 / 14.59
ReceiveAnyDamage            pre/POST     14.59 / 14.59
                               <-- native CurrentDamage write happens here
OnHitReact                  pre/POST     98.61 / 98.61
BndEvt__..._DamagedDelegate pre/POST     98.61 / 98.61
multicast (kill_hook)       damage=84.03
ReceiveHit                  pre/POST     98.61 / 98.61
```

Delta `98.61 - 14.59 = 84.02` ~= multicast `damage=84.03`. The damage
write is *between* `ReceiveAnyDamage POST` and `OnHitReact pre` and
no PE-reachable surface lives in that gap.

### LastDamageInfo on fall damage (proof of pipeline bypass)

```
fn=MulticastHandleEffectsWithDamageFlagsAtOwnerLocation damage=92.51 flags=0x0 type_flags=0x0
LastDamageInfo: DamageType=<null> src_type=0 flags=0x0 hit=(0,0,0)
instigator=<none> source=<none> attack=<none>
```

Every slot of `LastDamageInfo` is empty. The native code does not
construct an `FDamageInfo` for fall damage. It just writes
`CurrentDamage` and fires the cosmetic multicast.

### Approaches that do not work (and why)

| Approach | Result |
| -------- | ------ |
| Patch `bTakeFallDamage` (+0x1571), `MinimumFallDamageVelocity` (+0x1574), `FallDamageRatio` (+0x157C) on `ASurvivalCharacter` CDO and live pawn | Writes land per logs, native code does not read these fields. |
| Patch `USurvivalGameModeSettings::FallDamageMultiplier` (+0x008C) on every CDO instance (8 settings classes for various difficulty modes) | Writes land. No effect. |
| Patch the replicated `FCustomGameModeSettings::FallDamageMultiplier` at +0x1C inside `USurvivalModeManagerComponent::CustomSettings` (CustomSettings is at +0x114 on the component, so absolute +0x130) | Writes land. No effect. |
| Call `USurvivalModeManagerComponent::UpdateCustomSettings` UFunction via ProcessEvent (mimics the in-game difficulty UI; runs `OnRep_CustomSettings` server-side) | Dispatch reported by UE4SS, no effect on damage. |
| ProcessEvent hook on `OnLanded` and skip `original.call` | OnLanded is BP-cosmetic. Engine does not gate damage on BP event return. |
| ProcessEvent hook on `ApplyFallDamage` UFunction directly | Hook never fires. Engine bypasses ProcessEvent for `Final+Native+BlueprintCallable` when called from native code. |
| UE4SS Lua `RegisterHook("/Script/Maine.SurvivalCharacter:ApplyFallDamage", ...)` | UE4SS reports `Registered native hook (1, 2)`, callbacks never fire. Hits [issue #626](https://github.com/UE4SS-RE/RE-UE4SS/issues/626). `RegisterPreHook` swaps `UFunction::native_func`, but the engine's direct C++ call from `Super::Landed` bypasses the slot. |
| Skip `original.call` on the post-damage multicast | Multicast is `Const`, fires *after* the damage write. `before/after CurrentDamage` delta around `original.call` is zero. Skipping breaks cosmetic effects only. |

Net: nothing reachable via ProcessEvent or any field write affects
the native fall damage formula directly. The damage is computed
inline from `CharMovementComponent.Velocity.Z` and written to
`CurrentDamage` in one native call frame between `OnLanded` and the
post-damage multicast.

### The fix: velocity stomp in `OnLanded`

The engine reads `CharMovementComponent.Velocity.Z` *live* at the
moment `ApplyFallDamage()` runs (step 4). `OnLanded` (step 3) fires
during `Super::Landed` from the same native call site, *before*
`ApplyFallDamage` is invoked. Mutating `Velocity.Z` inside our
`OnLanded` PE hook means the native formula sees the mutated value.

`fall_hook.rs` does this in one write:

```text
On player OnLanded with fall_resistance level > 0:
  reduction = sqrt(level / 100)
  Velocity.Z *= (1.0 - reduction)

At level 100  -> reduction = 1.0  -> Velocity.Z = 0   -> no damage.
At level 50   -> reduction ~ 0.71 -> Velocity.Z * 0.29 -> partial.
```

Field offsets:

- `ASurvivalCharacter.CharMovementComponent` ptr: `+0x1380`
- `UMovementComponent.Velocity` (FVector, doubles): `+0x00D8`
- `FVector.Z`: `+0x10` inside FVector -> abs `+0x00E8` on the CMC

The hook does *not* suppress `OnLanded`. It forwards to the
original BP event so animations / sound / other landing logic still
runs. Validated in-game at level 100: -3431 cm/s landing -> zero
damage. Log line on each fall:

```
rpg/fall: stomped Velocity.Z -3431.07 -> -0.00 on BP_SurvivalPlayerCharacter_Female02_C_... (reduction=1.000)
```

### If the velocity-stomp regresses

If a future Grounded 2 patch changes `ApplyFallDamage` to read from
a captured local instead of live `Velocity.Z` (e.g. `ImpactVelocityZ`
member field, or a pre-captured value passed to a different seam),
the velocity stomp will silently no-op and fall damage will return.

Two fallbacks documented but not implemented:

1. **Detour the C++ method directly** via PolyHook_2_0 (already a
   UE4SS dep). Recover the C++ method address by decoding the first
   `call rel32` instruction inside the UFunction's `native_func`
   thunk. UHT generates `execApplyFallDamage(UObject*, FFrame&, RESULT_DECL)`
   whose first call is to the actual C++ method.
2. **Pattern-signature scan** via UE4SS's `SinglePassSigScanner`.
   More fragile across patches but does not depend on thunk layout.

## Environmental / collision damage path

Triggered by ramming foliage or world geometry at high speed (very
common at +300% Move Speed).

### Trace evidence

```
fn=MulticastHandleEffectsWithDamageFlagsAtOwnerLocation damage=134.88 flags=0x2 type_flags=0x0
LastDamageInfo: DamageType=BP_EnvironmentalDamage_C src_type=2 flags=0x0 hit=(0,0,0)
```

Same multicast surface as fall damage, but `LastDamageInfo` is fully
populated:

- `DamageType = BP_EnvironmentalDamage_C` (the discriminator)
- `src_type = 2` (`EDamageSourceType::Environmental`)
- `DamageFlags = 0x2` -> KillingBlow set, the hit was lethal here

Unlike fall damage, this path *does* go through the standard
`FDamageInfo` pipeline, which means `ApplyDamageFromInfo` plumbing
is involved upstream. That is a much friendlier surface for
mitigation.

### Mitigation: Impact Damage Resistance skill

`SKILL_IMPACT_RESISTANCE = "impact_resistance"`. Implemented as a
write to `UHealthComponent.RequiredDamageTypeFlags` (+0x00FC,
uint32). At any level > 0 the apply step writes `0xFFFFFFFF` to the
field on every player CDO and on the live player pawn. The native
ApplyDamage gate consults this field as `if ((incoming.type_flags
& Required) == 0) reject`, and fall / environmental damage in
Grounded 2 carries `type_flags = 0`, so no bit matches and the
damage is rejected before `CurrentDamage` is written.

Validated in-game: rock collision and environmental damage both
report `damage=0.00` in the multicast trace; impact-trace POST line
(which fires only on actual `CurrentDamage` change) stays silent.

Creature attacks carry non-zero `type_flags` (`ESurvivalDamageTypeFlags`
bits encode physical / chill / sizzle / etc.) and pass the gate
normally, so the player still takes hostile damage as expected.

#### Research finding: `AddHealth` bypasses the mask (2026-05-09)

Drove via `tests/explore_apply_damage_gate.rs` against the live
debug endpoint. With `impact_resistance` at level 100 and the
mask set to `0xFFFFFFFF` on the player HC, calling
`UHealthComponent::AddHealth(20.0, nullptr)` via process_event:

```
current_damage_before: 37.62
current_damage_after:  17.62
delta: 20.0
```

**The canonical heal entry point is not gated by
`RequiredDamageTypeFlags`.** This rules out the simplest
hypothesis (bandages → AddHealth → mask blocks them). If
bandages used AddHealth, they would not be blocked, but they ARE
blocked in-game.

Conclusion: bandages take a different path. Hypotheses:

1. **Status-effect-driven**: bandage adds a row from
   `Table_StatusEffects` whose tick eventually calls
   `ApplyDamageFromInfo` with negative damage and `type_flags=0`
  . Hits the same mask gate that rejects fall / environmental
   damage.
2. **Direct ApplyDamageFromInfo**: bandage code calls
   `ApplyDamageFromInfo` directly with negative damage.
3. **Separate Heal UFunction**: a heal-specific function that
   internally consults the same mask field for some reason.

Next experiment: while the player uses a bandage, watch the
snapshot's `status_effects` list for an entry that wasn't there
before (the bandage row on the data table). If one appears, we
have hypothesis 1 confirmed and can read the row's `Type` /
`Value` to identify what stat it modifies.

#### Result: hypothesis 1 (status-effect tick) is RULED OUT (2026-05-09)

User used a bandage in-game with `impact_resistance` at level 100
(mask `0xFFFFFFFF` active). Snapshot before vs after:

- **HP unchanged**: CurrentDamage = 134.0 before AND after. The
  bandage did NOT heal, confirming the user-visible bug.
- **`status_effects` count UNCHANGED**: 16 baseline rows -> 15
  after (the only delta is `PetRestriction` dropping off,
  unrelated). No bandage-flavored row was added at any point we
  observed.
- **`damage_ring` EMPTY**: zero `Multicast*` /
  `ApplyDamage*` events fired on the player's HealthComponent
  during the bandage attempt.

This rules out hypothesis 1 (bandage adds a heal-tick status
effect that calls ApplyDamageFromInfo) and hypothesis 2 (bandage
calls ApplyDamageFromInfo directly with negative damage). Both
would have shown up in `damage_ring` because kill_hook's
ProcessEvent trampoline catches every PE call on the player's
UHealthComponent vtable. And we saw none.

Remaining hypotheses:

3. **Bandage routes through `UHealthComponent::AddHealth`
   directly**, and the heal is suppressed somewhere downstream.
   The kill_hook ring's current filter does NOT capture
   `AddHealth` calls (only damage-shaped functions); next iter
   widen the filter and try again. If bandage IS calling
   AddHealth, we'll see it in the ring AND the (independently
   tested) AddHealth path bypasses the mask. Meaning the
   block is somewhere ELSE that intercepts AddHealth's effect.
4. **Bandage consume itself is suppressed.** The use animation
   may play but the actual consume / call to AddHealth never
   fires, suppressed by something tied to `impact_resistance`
   or its side effects (e.g. the mask write triggers a state
   on the consume side). Disconfirmed easily: did the bandage
   item count decrement on use? If yes, suppression is
   downstream of consume; if no, it's at the consume gate.
5. **Heal applies then is instantly reverted.** AddHealth
   succeeds, CurrentDamage drops, then a tick later something
   restores it. Would show as a brief CurrentDamage dip the
   user notices but our snapshot misses (we capture state, not
   transitions). Add a `current_damage_history` ring populated
   on every kill_hook fire to observe this.

Action: extend `kill_hook`'s filter to also capture `AddHealth`,
`ServerAddHealth`, `ApplyHit`, `OnRest`. Re-run the bandage
experiment. Either we see AddHealth fire (rules in hypothesis 3
or 5) or we don't (hypothesis 4: consume is suppressed. Check
inventory count delta).

#### User clarification: bandages are heal-over-time (HoT)

**Bandages don't heal instantly. They tick over several
seconds.** This changes the experiment shape. The post-bandage
snapshot we took caught state AFTER the HoT was supposed to run,
but BEFORE the HoT had necessarily completed. Possibilities now:

- The HoT status effect was ADDED briefly (during the heal
  window) then REMOVED, and our single post-snapshot missed it.
- The HoT status effect was added and is still active right now,
  but its row name doesn't match what we expected. Re-check the
  diff carefully.
- The HoT was BLOCKED entirely. Our impact_resistance mask
  (0xFFFFFFFF) somehow prevents the AddEffect call, so the
  effect never gets added to the StatusEffects array.

The user's instinct: "we're blocking the heal over time
somehow." Most plausible mechanism: when AddEffect is called for
the bandage row, the engine consults something that the mask
write touches. Maybe `RequiredDamageTypeFlags` is read by
StatusEffectComponent on every effect-add for "is this player
damageable", and the all-bits-set value rejects the heal-tagged
effect.

Refined experiment plan:

1. Snapshot baseline (mask on).
2. User uses bandage.
3. Snapshot within ~500ms (catch HoT mid-tick).
4. Snapshot again at ~3s (during HoT).
5. Snapshot at ~10s (post-HoT).

If a new row appears in any of (3)/(4) but not the baseline, we
have the HoT effect captured. Read its Type and Value to learn
the mechanism.

If NO new row appears in any of those windows, the AddEffect
call itself is being suppressed. And the suppression is
specifically tied to the mask being set (toggle off and repeat
to confirm).

#### Result: side-by-side on/off comparison (2026-05-09)

User toggled impact_resistance OFF and used a bandage. Healed
visibly: CurrentDamage 134.0 -> 76.5 (~57.5 HP restored over the
HoT duration). Critical: **the snapshot looked identical to the
mask-on bandage attempt in every respect EXCEPT HP**:

|                          | mask ON (broken)        | mask OFF (working)      |
| ------------------------ | ----------------------- | ----------------------- |
| `required_damage_flags`  | 0xFFFFFFFF              | 0x00000000              |
| `current_damage`         | 134.0 -> 134.0          | 134.0 -> 76.5           |
| status_effects rows      | 15 (no new entry)       | 15 (no new entry)       |
| status_effects names     | identical               | identical               |
| damage_ring events       | 0                       | 0                       |

**Bandages do not add a row to the StatusEffects array AND do
not fire any UFunction kill_hook intercepts.** Yet the heal
works. The path is:

1. Not the StatusEffectComponent.StatusEffects array (no row
   diff).
2. Not any UHealthComponent UFunction we filter
   (Multicast/ApplyDamage*/ApplyHit/ApplyDamage).
3. Either on a different class's ProcessEvent vtable, or a
   native C++ call that bypasses ProcessEvent entirely (similar
   to how native ApplyFallDamage bypassed our kill_hook on the
   fall-damage investigation).

Refined hypothesis: bandages call `UHealthComponent::AddHealth`
or `ServerAddHealth` directly. We proved (via simulate_add_health)
that AddHealth bypasses the mask. So the question becomes: is
the bandage call site even reaching AddHealth when the mask is
on, or is something earlier in the chain rejecting it?

Action: extend kill_hook ring filter to capture EVERY UFunction
call on the player's HealthComponent vtable, not just
damage-shaped ones. Re-run the on-vs-off bandage experiment.

If `AddHealth` shows up in the ring with mask OFF but NOT with
mask ON, the bandage->AddHealth chain is being blocked
upstream by the mask. We then trace upstream.

If `AddHealth` shows up in BOTH (with mask off AND on), but HP
changes only with mask off, something downstream of AddHealth
is reverting the heal. The simplest culprit being a tick that
re-applies CurrentDamage or fights the heal.

If `AddHealth` does NOT show up at all in either run, the heal
goes through a different class entirely. Hook UInventoryComponent
or UConsumableComponent ProcessEvent slots and re-test.

#### Result: AddHealth via the generic `call` primitive (2026-05-09)

Test: `tests/explore_bandage_path.rs::addhealth_with_mask_on_vs_off`.
Drove `UHealthComponent::AddHealth(20.0, nullptr)` through the
generic `call` op (no mod-side rebuild needed. The test wrote
the parm struct itself).

**With mask OFF**: `CurrentDamage` 32.79 -> 12.79 (delta = 20).
Heal worked. `damage_ring` captured one entry:

```
fn=AddHealth damage=0 flags=0x00000000 type_flags=0x00000000
```

This is the first direct evidence that AddHealth, when invoked
explicitly, **does fire through ProcessEvent on the player's
HealthComponent vtable**. Our wider kill_hook capture sees it.
That means: if bandages WERE calling AddHealth, we would see it
in the ring during a real bandage use. We did not. So bandages
do NOT call AddHealth (at least not on the player's local
HealthComponent vtable; ServerAddHealth on a remote replication
path remains a possibility).

**With mask ON**: AddHealth call timed out at the HTTP layer
without completing. Game itself remained responsive after.
Diagnosis: NOT a crash. The PE-queue drain in `kill_hook`'s
trampoline only fires when the engine calls a UFunction on a
player's `UHealthComponent` vtable. And with the mask
rejecting all `type_flags=0` events, the multicast traffic that
normally drives the trampoline drops to ~zero. The queued
AddHealth op sits unread until ureq's 5s timeout. Workaround
during research: drive activity in-game (move, take a hit) to
fire multicasts that drain the queue. Proper fix:
high-frequency drain site (UE4SS's
`RegisterProcessEventPreCallback`, or a `Tick` UFunction hook).
Tracked in `todo.md` "Endpoint parity gap".

#### Updated hypothesis tree

| Path | Status |
| --- | --- |
| Bandages add a status-effect row that ticks ApplyDamageFromInfo | RULED OUT (no row appeared, ring empty) |
| Bandages call ApplyDamageFromInfo directly with negative damage | RULED OUT (would show in ring; ring empty) |
| Bandages call `UHealthComponent::AddHealth` on the player's local vtable | RULED OUT (would show in ring during real bandage; nothing observed) |
| Bandages call `UHealthComponent::ServerAddHealth` (remote replicated path) | OPEN. ServerAddHealth doesn't go through the local vtable; need to install a separate hook to see it |
| Bandages route through a UFunction on a different class entirely (UConsumableComponent, UItem subclass, etc.) | OPEN. Broaden the trampoline to a non-HealthComponent class to see |
| Bandages use a native C++ call that bypasses ProcessEvent entirely (like the native fall-damage path we found earlier) | OPEN. If so, `damage_ring` will never see anything; need a native detour |

Next step: extend the trampoline (or install a new one) on a
class likely to mediate consumable use -- `UConsumableComponent`
or the player's BP class. Re-run the bandage observation test.

#### DEFINITIVE: AddHealth is NOT the bandage path (2026-05-09)

Test `tests/explore_bandage_path.rs::addhealth_with_mask_on_vs_off`,
re-run after the fall_hook drain-site fix:

```
mask OFF: AddHealth(20) -> delta = 20  (CurrentDamage 117.61 -> 97.61)
mask ON : AddHealth(20) -> delta = 20  (CurrentDamage  97.61 -> 77.61)
```

Heal is **identical** with mask on vs off. AddHealth is
unaffected by `RequiredDamageTypeFlags`. If bandages used
AddHealth, they would heal regardless of mask state. But
bandages observably DO NOT heal when the mask is on. Therefore
**bandages do not call AddHealth**.

#### Updated rule-out matrix

| Path | Status |
| ---- | ------ |
| Status-effect tick (heal-over-time row added to player) | RULED OUT (no new row in `status_effects` during real bandage use) |
| `ApplyDamageFromInfo` with negative damage | RULED OUT (would show in `damage_ring`; ring empty during real bandage) |
| `UHealthComponent::AddHealth` | RULED OUT (mask on/off identical heal. Mask wouldn't matter even if it were the path) |
| `UHealthComponent::ServerAddHealth` (replicated) | RULED OUT (would fire through HC vtable; ring empty) |
| ANY UFunction on `UHealthComponent` | RULED OUT (kill_hook covers the entire HC vtable; nothing fires during real bandage) |
| UFunction on a different class (UConsumableComponent, UStatusEffectComponent, UInventoryComponent, BP_Bandage_C, ...) | OPEN |
| Native C++ call that bypasses ProcessEvent entirely | OPEN |

#### Where bandages MUST be going

Two remaining hypotheses:

1. **A different class's UFunction.** The bandage flow likely
   touches `UStatusEffectComponent` (to add a HoT effect) or
   `UConsumableComponent` (to consume the item) before the heal
   actually applies. The HoT mechanism then writes
   `CurrentDamage` directly without going through any UFunction
   we've hooked. Test plan: `walk_class("StatusEffectComponent")`
   then `read_bytes` the `StatusEffects` TArray repeatedly during
   a real bandage to catch the row appearing mid-HoT (our
   single-snapshot might have missed the window).
2. **Native C++ direct write.** UE's `UHealthComponent` has a
   native fast-path that mutates `CurrentDamage` without
   ProcessEvent involvement. The fall-damage investigation
   earlier showed the same shape. Native code wrote
   `CurrentDamage` and no PE call ever fired. If bandage HoT
   uses the same kind of path, the only way to observe it is to
   poll `CurrentDamage` rapidly during a bandage and infer the
   call rate. Or: install a write-watch on the field via x86
   debug registers (much heavier; out of scope today).

The mask-blocks-bandages mechanism, then, must be:
- Either the mask write triggers a side-effect on the
  StatusEffectComponent that prevents the HoT-row addition.
- Or the native C++ heal path consults `RequiredDamageTypeFlags`
  even though it doesn't go through the gated ApplyDamage code.

Next test: rapid-poll `status_effects` during a bandage to catch
the HoT row mid-tick.

#### IDENTIFIED: bandage mechanism is `BandageHoTTier1` (2026-05-09)

`tests/explore_bandage_status_effects.rs` polled `status_effects`
at 200ms intervals for 12 seconds with impact_resistance OFF and
the player using a bandage. Result:

```
baseline (right after bandage use, before HoT completes):
  21 rows including "BandageHoTTier1"
ticks 1-14 (3.5 seconds):
  CurrentDamage dropping ~1.44 every 250ms
  total delta: -23 HP healed
tick 14:
  BandageHoTTier1 row REMOVED, healing complete
```

So bandages **DO** use the status-effect system. The HoT row is
`BandageHoTTier1` (in `/Game/Blueprints/Attacks/Table_StatusEffects.Table_StatusEffects`,
needs verification on table; previous polls showed both
`Table_StatusEffects` and `DT_StatusEffects` as observed tables).
It ticks ~1.44 HP every ~250ms for ~3.5 seconds.

Why the earlier mask-off snapshot missed it: the bandage's HoT
duration (~3.5s) is brief; if the snapshot is taken after the HoT
completes, the row is already gone. We were measuring after the
window.

**Compared to mask-on run** (same test, mask = 0xFFFFFFFF):
`BandageHoTTier1` never appeared in the polled window AND
CurrentDamage didn't change. The mask suppresses the HoT
addition entirely.

#### Suppression mechanism (still open)

Three places suppression could happen:

1. **Bandage's BP code** reads `RequiredDamageTypeFlags` before
   calling `CreateAndAddEffect` and skips the call if the mask
   is non-zero. Test: hook `UStatusEffectComponent` and observe
   whether `CreateAndAddEffect` fires during a mask-on bandage.
   If NO call fires, suppression is upstream of the effect
   system.
2. **`UStatusEffectComponent::CreateAndAddEffect`** internally
   reads `RequiredDamageTypeFlags` and rejects the add call.
   Test: hook the component and observe whether the call
   ENTERS but the row never appears.
3. **The HoT row's stat type** maps to something the mask
   gate consults. If `BandageHoTTier1`'s Type column is a
   "damage-class" stat, native code applying the heal might
   route through the same gate that rejects type_flags=0.

To distinguish, we need visibility on `UStatusEffectComponent`.
That requires a one-time hook in the mod (per the skill: hooks
live in the mod, exposed via a named ring; tests poll the ring).
This is the next mod change.

Other research possibilities that don't require a new hook:
- `walk_class("StatusEffect")` to find UStatusEffect instances
  during a bandage. Read their data-table row handle to see
  what they correspond to. Catches the row even if it's not in
  the player's TArray (e.g. owned by the bandage item).
- Read `BandageHoTTier1`'s row metadata via `read_bytes` on the
  resolved data-table row pointer. Tells us its Type/Value
  shape; we can then trace what stat it modulates.

#### ROOT CAUSE: bandage HoT is `Type=Health` and the mask blocks the tick (2026-05-09)

Caught `BandageHoTTier1`'s row metadata via the polling test
(rapid snapshot caught the row's full Type and Value while it
was active in the player's StatusEffects array):

```
row=BandageHoTTier1   type=24   value=1.25   table=Table_StatusEffects
```

Looking up `EStatusEffectType` value 24 in
`Maine_structs.hpp:162`:

```cpp
enum class EStatusEffectType : uint8 {
    ...
    AttackDamage   = 23,
    Health         = 24,    // <-- bandage HoT type
    HealthPercentage = 25,
    ...
}
```

So bandage HoT is `Type=Health` (raw HP modifier),
`Value=1.25` per tick. Combined with `PlayerUpgradeHealing1`
(`Type=HealingReceived=57`, `Value=1.15`), the observed per-tick
heal of ~1.44 = 1.25 * 1.15 matches.

**Why the mask blocks bandages**: a `Health`-typed effect's
tick applies HP changes through the same native code path that
processes damage events. Per the earlier finding, native
`ApplyDamage` consults `RequiredDamageTypeFlags`; if the
incoming heal-tick carries `type_flags=0` (no damage-class
bits set, because heals aren't damage-typed), the gate rejects
the tick the same way it rejects fall/environmental damage. The
mask `0xFFFFFFFF` requires at least one bit to overlap; zero
overlaps with everything fail.

So the suppression isn't "AddEffect is blocked". It's "the
per-tick heal application is blocked by the same gate that
rejects fall damage". The row may briefly exist (our 200ms
polling resolution can miss a single-tick lifetime), but
without working ticks it produces no observable HP change.

#### REVISED fix path for `impact_resistance` (2026-05-09 evening)

Earlier sections proposed status-effect migration. That's
correct for skills like max_health / attack_damage / lifesteal,
but **not for impact_resistance**, because the status-effect
filter shapes don't cleanly distinguish environmental damage
from fall damage (both carry `type_flags = 0`) without
reading the row's `DamageTypeFlags` or `DamageType` filter
fields, and we don't yet have programmatic write access to
those filter fields.

The cleaner shape mirrors **fall_resistance**'s velocity-stomp
pattern:

| Skill | Surface | Discriminator | Action |
| ----- | ------- | ------------- | ------ |
| fall_resistance | `OnLanded` PE hook | (only path) | scale CMC.Velocity.Z by `(1 - reduction)` before native ApplyFallDamage runs |
| impact_resistance | `ApplyDamageFromInfo` PE hook | `FDamageInfo.DamageType == BP_EnvironmentalDamage_C` | scale `Damage` out-param by `(1 - reduction)` before forwarding |

We already have a PE hook on `UHealthComponent` (kill_hook).
That trampoline already fires for `ApplyDamageFromInfo` calls
on the player HC. To finish impact_resistance:

1. Inside the trampoline's `ApplyDamageFromInfo` branch, read
   `FDamageInfo.DamageType` (a `TSubclassOf<USurvivalDamageType>`
   at +0x40 inside FDamageInfo, which is the 3rd parm at
   +0x18 of the parm struct, so absolute parm offset
   +0x18+0x40 = +0x58).
2. Resolve the class's name. If it contains "Environmental"
   (or matches `BP_EnvironmentalDamage_C` exactly), this is
   the impact path.
3. If the player has `impact_resistance > 0`, compute
   `reduction = 1.0 * progress(level)` and scale the `Damage`
   parm at offset 0x00 by `(1 - reduction)` BEFORE forwarding
   to the original.
4. Forward as normal.

This:
- Reduces only environmental damage (not bandages, not creature
  combat, not fall)
- Scales per-skill-level
- Removes the binary-mask hack entirely
- Removes the bandage regression as a side-effect (the mask is
  the bug; not setting it = no bug)

#### LANDED 2026-05-09: impact_resistance is now Runtime + ApplyDamageFromInfo intercept

Implementation:

- `skills.rs` catalog row: `Runtime { max_bonus: 1.0, format:
  MinusPercent { word: "environmental damage" } }`. No CDO
  writes, no mask, no status-effect addition.
- `kill_hook::on_event`: when `fn_name == "ApplyDamageFromInfo"`
  on a player's `UHealthComponent`, reads the DamageType class
  from `FDamageInfo+0x40` (parm offset `0x18+0x40 = 0x58`).
  If the class name contains "Environmental", scales:
  - `Damage` parm at parm+0x00 (the OUT param the engine reads
    as input AND writes the applied amount to)
  - `OriginDamageData.Damage` at parm+0x18+0x78 = parm+0x90
  Both by `(1 - reduction * progress)` where
  `progress = sqrt(level / 100)` and `reduction = 1.0` at L100.
- Forwards to original.

Result:
- environmental damage is scaled before native code processes it
- bandages don't go through ApplyDamageFromInfo (we proved this:
  when bandages tick, kill_hook ring captures nothing) -> never
  affected
- creature combat damage has DamageType = something like
  `BP_<creature>_attack_C`, not Environmental -> never affected
- fall damage is the native-direct path that bypasses
  ProcessEvent -> handled separately by fall_resistance
- the `RequiredDamageTypeFlags = 0xFFFFFFFF` mask is GONE; the
  bandage regression is fixed by removal of the bug, not by
  reaching for a workaround.

Validation: tests/skill_impact_resistance.rs and
tests/bandage_regression.rs.

**Fix path: status-effect migration of `impact_resistance`** (DEPRECATED; see above).
Move from the binary `RequiredDamageTypeFlags = 0xFFFFFFFF`
write to applying a status effect of
`EStatusEffectType::DamageReductionMultiplier` (=30) or
`FallDamage` (=14) with the right `Value`. Those types
modulate damage *via the status-effect system*, which natively
filters by stat type and does not intercept heals.

After migration, `impact_resistance` will:
- Reduce environmental / fall damage like today
- NOT touch healing (different stat)
- Scale per-skill-level instead of being binary

This is the long-tracked migration in `todo.md` "RPG:
Status-effect-backed skill rewrite". The bandage bug is the
forcing function: the binary mask is unfixable; the migration
is mandatory for the skill to ship usable.

#### Reuse the same pattern for our `health_regen` skill

Bandages reveal the **canonical Grounded 2 healing pattern**:

> Apply a `UStatusEffect` whose `FStatusEffectData` row has
> `Type = EStatusEffectType::Health (24)` and `Value = HP per tick`.
> The engine's status-effect tick pipeline applies the HP gain
> through the standard pipeline; gear / perk multipliers
> (`HealingReceived = 57`) compound automatically.

Our current `health_regen` skill takes a different approach: it
mutates `UGlobalCombatData.CombatRegenTickPercentage` /
`CombatRegenTickRate` directly. That works but it has problems:

- It scales the **out-of-combat** regen system globally, which
  affects every actor that uses it (players, tamed creatures).
  Possible cross-contamination.
- It modifies a singleton-style asset, so multiplayer guests
  may not see the change.
- It bypasses the canonical pattern bandages use, so it doesn't
  benefit from `HealingReceived` multipliers, gear bonuses, etc.

**Migrate `health_regen` to the same status-effect pattern**.
Apply a long-duration (or refreshed-on-tick) `UStatusEffect`
with `Type=Health` and `Value` scaled by skill level, attached
to the player's `UStatusEffectComponent`. The engine then
ticks it the same way it ticks `BandageHoTTier1`, with all the
existing gear/perk modulation. No GlobalCombatData mutation
needed, no asset-level cross-contamination, fully integrated
with vanilla.

Implementation sketch:
1. On apply, find or create a row in `Table_StatusEffects` for
   our regen (e.g. `RpgHealthRegenTier1`) with `Type=24`,
   `Value=skill_level_scaled`. Either mutate an existing
   reusable row or add a new one (see "UStatusEffect is
   row-driven" earlier).
2. Call `UStatusEffectComponent::CreateAndAddEffect` with that
   row handle, on the player's component.
3. Refresh on slot reload / level-up so duration doesn't
   expire mid-session.
4. Remove the GlobalCombatData mutation entirely.

This same pattern will be used by:
- `impact_resistance` -> `DamageReductionMultiplier` / `FallDamage`
- `lifesteal` -> `LifeSteal` (= 38)
- `attack_damage` -> `AttackDamage` (= 23)
- `armor` -> `DamageReduction` (= 29)
- `max_health` -> `MaxHealth` (= 5)
- `health_regen` -> `Health` per tick (= 24)
- crit, thorns, etc. -> matching enum values

This is the right long-term shape for the entire catalog.

#### Discovery: every required row type exists in `Table_StatusEffects` (2026-05-09)

Test `tests/explore_status_effect_rows.rs` walked
`Table_StatusEffects` row map and grouped rows by
`FStatusEffectData.Type`. Output (filtered to migration targets):

```
Type   5 (MaxHealth)                 : 18 rows  first @ 0x25e07bda6c0
Type  14 (FallDamage)                :  7 rows  first @ 0x25e24abe680
Type  23 (AttackDamage)              : 168 rows
Type  24 (Health)                    : 116 rows
Type  29 (DamageReduction)           : 157 rows
Type  30 (DamageReductionMultiplier) :  4 rows  first @ 0x25e271d5d70
Type  38 (LifeSteal)                 : 11 rows
```

**Every stat type our migration plan needs exists in the data
table** with multiple candidate rows. We can either reuse an
existing row (mutate its `Value` to our level-scaled value, then
`CreateAndAddEffect` with that row handle) or pick the lowest-
risk row per stat (one with a name like `RpgX` reserved for our
mod, when we eventually inject one).

Caveats:

- The discovery test was slow (~3.7 min) because it made one
  HTTP `read_bytes` call per row. Optimization: read large
  chunks of element memory in one call, parse client-side. Drop
  runtime to seconds.
- We captured row addresses but not the `FName` keys. The
  `FName` is at element[0..8] of each TMap slot; we already
  read it but didn't surface it. `CreateAndAddEffect` takes a
  `FDataTableRowHandle = { UDataTable*, FName }`, so we need
  the FName to build the handle. One-line fix on the next test
  iteration.

Next step: re-run discovery with the FName captured, pick a
target row per stat type (probably the simplest / smallest-
value one to avoid stomping a vanilla effect tier), then
implement the generic `SkillEffect::PlayerStatusEffect` apply
path that calls `CreateAndAddEffect`. The first migration
(`impact_resistance`) is the proof of concept; once it works,
the other six skills are one-line catalog row changes.

Caveat: `simulate_apply_damage` (calling `ApplyDamageFromInfo`
via process_event) **crashes the game** when invoked from inside
the `kill_hook` PE trampoline. The drain site is itself inside a
ProcessEvent call; re-entering with another ProcessEvent on a
function that triggers replication appears to deadlock or AV.
Tracked in `todo.md` "Endpoint parity gap".

#### Critical regression: bandages / healing items blocked (2026-05-09)

**Confirmed in-game**: with `impact_resistance` enabled at any
level, **bandages do not heal**. Disabling impact_resistance (with
the skill toggle) lets bandages work; re-enabling it blocks them
again. Repro is binary. Enabling only impact_resistance in an
otherwise-clean catalog reproduces the block. Other skills do not
contribute.

Mechanism: bandages and other healing items route through the
same `ApplyDamage` entry as damage, but with negative `Damage` and
`type_flags = 0`. The mask gate `(incoming.type_flags & Required)
== 0 -> reject` rejects the heal event before it can decrement
`CurrentDamage`.

This makes the current implementation of `impact_resistance` a
**hard regression on a core gameplay item**. The skill is unusable
as currently shipped without crippling healing. The path forward is
the status-effect migration tracked in [`todo.md`](todo.md):
move `impact_resistance` to `EStatusEffectType::DamageReductionMultiplier`
(or `FallDamage`), filtered by damage type so healing events are
not affected. Until that lands, players must keep the skill toggled
off (or refunded) when they want to heal.

The toggle now properly clears the mask. The apply step writes
either `mask` (when level>0 AND enabled) or the captured vanilla
value (when level==0 OR disabled). See
[`changelog.md`](changelog.md) 2026-05-09 entry.

#### Observed traces (rock collision and hazard)

Running into a rock at high speed produces *two* consecutive damage
events:

```
# Event 1
MulticastFallEffects           CD=14.59
ReceiveAnyDamage               CD=14.59
                                 <-- native write
OnHitReact                     CD=48.58
BndEvt__..._DamagedDelegate    CD=48.58
multicast: damage=33.99 DamageType=<null> src_type=0          (fall path!)

# Event 2 (same second, same impact)
MulticastFallEffects           CD=48.58
ReceiveAnyDamage               CD=48.58
                                 <-- native write
OnHitReact                     CD=82.56
BndEvt__..._DamagedDelegate    CD=82.56
multicast: damage=33.99 DamageType=BP_EnvironmentalDamage_C   (env path)
```

Two findings:

1. **Rock impact triggers BOTH the fall-damage path AND the
   environmental path** for a single physical event. Each delivers
   the same damage value computed from impact velocity. Net damage
   is roughly double what a fall of equivalent velocity would
   inflict.
2. **`OnLanded` does NOT fire on rock impact.** The fall-damage path
   activates without a Landed event because there was no airtime to
   land from. So our existing `OnLanded` velocity-stomp covered
   actual falls but missed rock collisions entirely.

Why velocity-stomp does NOT carry over from fall: by the time any
PE event fires on a rock / hazard collision, the engine has already
zeroed `CharMovementComponent.Velocity.Z` as part of native
collision response. Confirmed by tracing V.Z at every PE event.
it reads 0.0 throughout. There is no PE-reachable surface with
live velocity for impact damage. The fix has to attack the damage
itself, not the input it was computed from.

#### Diagnostic trace (gated)

When `impact_resistance` level > 0, `fall_hook.rs` logs every
event during which `CurrentDamage` actually changed. The POST
line fires only when delta > 0.001 to keep the log readable when
`ReceiveHit` etc. fire dozens of times per collision. This trace
is what identified the native-side gating field as the only
remaining intercept and stays silent for users who have not put a
point into the skill.

#### Fallback if `RequiredDamageTypeFlags` regresses

If a future game patch changes the native ApplyDamage gate to
ignore `RequiredDamageTypeFlags`, fall back to one of:

1. `ImmunityFlags` (uint32 at +0x00F8 on `UHealthComponent`,
   `Edit, Net`). Different gate, same idea. If the runtime check
   is `(incoming & Immunity) != 0 -> reject`, set Immunity to a
   value that catches `type_flags = 0` damage.
2. Native function detour against `UHealthComponent::ApplyDamage`
   via PolyHook_2_0 (last resort, see "If the velocity-stomp
   regresses" above for the technique).

## Kill detection (already implemented)

`kill_hook.rs` hooks
`UHealthComponent::MulticastHandleEffectsWithDamageFlags` (the
non-AtOwnerLocation variant; the location-included form is what
fires on player damage; player kill detection only needs the
HealthComponent variant for creatures). Reads `DamageFlags & 2` for
the KillingBlow bit. On kill, walks
`HealthComponent.LastDamageInfo.InstigatorController` (FWeakObjectPtr
at `+0x3B0 + 0x20`) and classifies into Player / Buggy / Other for
XP attribution. Filters dying actors to `ASurvivalCreature`
subclasses only. Buildings and props also fire this event with
KillingBlow on destruction.

See [`rpg.md`](rpg.md) for the XP layer that consumes this signal.

## Related player-character offsets

`ASurvivalCharacter` (Maine_classes.hpp around line 5700).

| Offset  | Type             | Field                                                 |
| ------- | ---------------- | ----------------------------------------------------- |
| +0x1340 | UHealthComponent* | `HealthComponent`                                     |
| +0x1378 | UStatusEffectComponent* | `StatusEffectComponent` (the canonical extension surface for damage modifiers; see below) |
| +0x1380 | UCharacterMovementComponent* | `CharMovementComponent`                  |
| +0x12B8 | float            | `CustomDamageMultiplier` (Attack Damage skill writes) |
| +0x1571 | bool             | `bTakeFallDamage` (UNUSED by native fall code)        |
| +0x1574 | float            | `MinimumFallDamageVelocity` (UNUSED)                  |
| +0x157C | float            | `FallDamageRatio` (UNUSED)                            |

The "UNUSED" entries are SDK-exposed but not consulted by Grounded
2's native fall-damage code. Documented here so we do not retry
patching them.

## Status Effect system (the canonical extension surface)

> **Generic reference**: the universal UE5 status-effect design
> pattern (component holds a list, effects are row-driven from
> a UDataTable, native code queries summed stats) lives in
> [`../../ueforge/docs/status-effects.md`](../../ueforge/docs/status-effects.md).
> Read that first if you're new to the pattern; the framework's
> `StandardEffect::StatusEffect` variant binds to it directly.
> This section captures the **Grounded 2 / Maine specifics**:
> exact class-layout offsets, the live `Table_StatusEffects`
> data table, validated `EStatusEffectType` enum values, and
> the probe output from a real player session.

### Plain-language overview (Grounded 2 example)

In Grounded 2, **every gameplay modifier you've ever seen is a
"status effect"** under the hood. You don't think of them that way
because the UI calls them perks, gear bonuses, food buffs, sizzle,
chill, etc.. But the game wires all of those through one shared
mechanism:

- The **bonemeal helmet's +10 Max HP** -> a status effect with
  `Type=MaxHealth, Value=10`.
- A **"Crit Punch" perk's +5% crit chance** -> a status effect
  with `Type=CriticalHitChance, Value=0.05`.
- A **spicy meal's +10% damage for 60s** -> a status effect with
  `Type=AttackDamage, Value=0.10, Duration=60`.
- **Sizzle ticking heat damage** -> a status effect.
- **A bee bringing 30% lifesteal for 10s** -> a status effect.

The icons in the corner of your screen showing buffs / debuffs are
status effects with `bShowInUI = true`. Most status effects (like
gear bonuses) don't have a UI icon. They are invisible passive
modifiers stacked silently on the character.

The character object that tracks "what's currently active on this
player" is a `UStatusEffectComponent`. It is just a list of every
effect currently stuck on you. Equip a helmet -> the helmet adds
its effect to the list. Take it off -> the effect is removed.

The game's data side. The **catalog** of every status-effect
template that exists in the game. Lives in one big spreadsheet
(a "data table") at
`/Game/Blueprints/Attacks/Table_StatusEffects`. Each row is one
effect template, with columns like `Type`, `Value`, `Duration`,
and so on. Every active effect on every character points at one
of these rows.

When the native game code asks "what's this player's current
fall damage modifier?" it walks the player's status-effect list,
sums up every entry whose `Type = FallDamage`, and applies that
to the damage. Same for HP, attack damage, lifesteal, etc. The
list is the public API; the engine consults it on every damage
event.

Our skills should plug into the same system. Setting `FallDamage`
to `0.0` via a status effect is *exactly* the channel the engine
uses to express "this player ignores fall damage". We do not
need to fight the engine, we use its own designer-facing knob.

The rest of this section is the technical detail of how to add a
status effect to the player from our mod.

### The component

`UStatusEffectComponent` lives on `ASurvivalCharacter` at +0x1378
(`Maine_classes.hpp:35888`). This is the *intended* extension
point for any per-stat / per-damage-type modifier the game can
natively support. Skills that should be backed by status effects:
fall damage, impact damage, lifesteal, crit chance, crit damage,
thorns, attack damage, stun resist. Nearly every entry in our
catalog has a matching `EStatusEffectType` enum value.

### Why this is the right surface

Our current implementations write scalar fields directly on
`UHealthComponent` (Armor's `BaseDamageReduction`, Impact
Resistance's `RequiredDamageTypeFlags`) or pre-stomp velocity on
`UCharacterMovementComponent` (Fall Resistance). These work but
they are binary or coarse, and they do not scale per-skill-level
along the `sqrt(level/100)` curve the rest of the catalog uses.
The status effect system is *built* for proportional, type-aware,
stackable scaling. The engine reads it on every damage event.

### Data table 101 (skip if you know UE)

A **data table** in Unreal Engine is a spreadsheet baked into
the game. Each row is one record, each column is a property of
that record. Game programmers use them when they have many
similar things and don't want to hard-code each one separately.
Examples elsewhere in Grounded 2:

- Every weapon has a row in some weapons data table (columns:
  damage, durability, swing speed).
- Every creature has a row in a creatures data table (columns:
  HP, drops, XP value).
- **Every status effect has a row in `Table_StatusEffects`**
  (columns: `Type`, `Value`, `Duration`, etc.).

A row is identified by its **row name** (an `FName`). The same
way a spreadsheet identifies a row by a primary key column.
"Pointing at a row" is done with an `FDataTableRowHandle =
{ DataTable*, FName }`. That is what `UStatusEffect.StatusEffectRowHandle`
holds: a reference to which template in `Table_StatusEffects`
this effect was instantiated from.

To add a new status-effect template, we either (a) write a new
row to `Table_StatusEffects`, or (b) repurpose an existing row.
Then we hand the engine a row handle pointing at it and call
`CreateAndAddEffect`. The engine does the rest.

### The component

`UStatusEffectComponent` (extends `UActorComponent`,
`Maine_classes.hpp:35888`):

| Field | Offset | Notes |
| ----- | ------ | ----- |
| `DefaultStatusEffects` | +0x0108 | `TArray<FDataTableRowHandle>`. Effects added at component init |
| `MountedDefaultStatusEffects` | +0x0118 | Effects added while riding a Buggy mount |
| `UnmountedDefaultStatusEffects` | +0x0128 | Effects added when not mounted |
| `StatusEffects` | +0x01C8 | `TArray<UStatusEffect*>`. The live, replicated list (Net, RepNotify) |
| `HealthComponent` | +0x0248 | back-pointer to the actor's HC |

Public surface (UFunctions, callable via ProcessEvent):

```cpp
void AddEffect(UStatusEffect* StatusEffect);
UStatusEffect* CreateAndAddEffect(const FDataTableRowHandle& StatusEffectRowHandle);
void RemoveEffect(UStatusEffect* StatusEffect, bool bBroadcastChangedEvent);
bool CanAddEffect(const FDataTableRowHandle& StatusEffectRowHandle) const;

// the getters native damage code queries:
float GetValueForStat(EStatusEffectType StatType, bool bTemporaryEffectsOnly) const;
float GetValueForStatForApplicationTags(EStatusEffectType StatType, AActor* TargetToCheck) const;
float GetValueForStatForDamageTypeFlags(EStatusEffectType StatType, int32 Flags) const;
bool HasStatusEffectOfType(EStatusEffectType StatType) const;
int32 GetNumStatusEffectOfType(EStatusEffectType StatType) const;

// useful event taps:
TMulticastInlineDelegate<void(AActor* Owner, UStatusEffect* StatusEffect)> OnStatusEffectAdded;
TMulticastInlineDelegate<void(AActor* Owner, UStatusEffect* StatusEffect)> OnStatusEffectRemoved;
```

`GetValueForStatForDamageTypeFlags(StatType, Flags)` is the
"proportional, type-filtered" getter. Native damage code passes the
incoming damage's type flag bitmask (the same `DamageTypeFlags` we
read off the multicast parm) and gets back the summed modifier
across all matching status effects on the actor.

### `EStatusEffectType` enum (relevant subset)

`Maine_structs.hpp:136`, 90+ entries. Values that map directly to
skill ideas in our catalog:

| Stat | Value | Use |
| ---- | ----- | --- |
| `MoveSpeed` | 1 | (we currently write CMC fields directly; could replace) |
| `MaxHealth` | 5 | future Max Health skill |
| `MaxStun` | 7 | future stun-resist skill |
| `StunImmunity` | 8 | "" |
| `MaxHauling` | 13 | future hauling skill |
| `FallDamage` | **14** | Fall Damage Resistance |
| `WaterBreathing` | 15 | future skill |
| `HungerRate` | 17 | (we currently write SurvivalComponent directly) |
| `ThirstRate` | 18 | "" |
| `StaminaRegenRate` | 19 | future stamina skill |
| `AttackDamage` | 23 | (we currently write `CustomDamageMultiplier`) |
| `DamageReduction` | 29 | Armor (currently writes `BaseDamageReduction`) |
| `DamageReductionMultiplier` | **30** | proportional generic reduction |
| `CriticalHitChance` | 31 | future crit skill |
| `ApplyAttackEffect` | 33 | future on-hit skill |
| `ReflectDamage` | 37 | future thorns skill |
| `LifeSteal` | **38** | Lifesteal (currently a `Runtime` no-op) |
| `Sizzle` | 48 | sizzle (heat) resist |
| `CriticalDamage` | 62 | future crit-damage skill |
| `Luck` | 65 | future luck skill |
| `Chill` | 76 | chill (cold) resist |

The named values used by skills documented elsewhere in this file
are bolded. This is by far the cleanest mapping between our skill
catalog and the engine's native modifier surface.

### `UStatusEffect` is row-driven (important)

`UStatusEffect` (`Maine_classes.hpp:35829`) extends `UObject`,
0x110-byte instance. Visible fields:

| Field | Offset | Notes |
| ----- | ------ | ----- |
| `StatusEffectRowHandle` | +0x58 | `FDataTableRowHandle_NetCrc32`. The data table row this effect was instantiated from. Replicated. |
| `TimeElapsed` | +0x88 | float, replicated |
| `bEnabled` | +0x90 | bool |
| `ResetTime` | +0x94 | int32, replicated |
| `OriginObject` | +0xD8 | UObject* |
| `OriginID` | +0xE8 | FGuid, replicated |
| `OriginType` | +0xF8 | EStatusEffectOriginType enum |

Critically, `UStatusEffect` does **not** store the stat type or
value as direct instance fields. The getters
`GetStatusEffectType()`, `GetDuration()`, etc. read from the data
table row referenced by `StatusEffectRowHandle`. So:

- Two `UStatusEffect` instances with the same row handle have the
  same value forever.
- We cannot just allocate a `UStatusEffect`, set "value = X", and
  AddEffect. The row handle is the source of truth.

The row struct is `FStatusEffectData` (`Maine_structs.hpp:11045`,
extends `FTableRowBase`):

| Field | Offset | Notes |
| ----- | ------ | ----- |
| `Type` | +0x30 | `EStatusEffectType` (uint8) |
| `Value` | +0x34 | float. The stat amount |
| `DurationType` | +0x38 | enum |
| `Duration` | +0x3C | float seconds |
| `Interval` | +0x40 | float seconds |
| `MaxStackCount` | +0x44 | int32 |
| `DamageTypeFlags` | +0x48 | uint32. Bitmask filter |
| `DamageType` | +0x50 | TSubclassOf<USurvivalDamageType> |
| `ApplicationTags` | +0x58 | FGameplayTagContainer (0x20 bytes) |
| `EffectTags` | +0x78 | FGameplayTagContainer |
| `ApplyType` | +0x160 | enum |
| `ClearFlags` | +0x164 | uint32 |

### Implementation paths (cheapest to most work)

1. **Mutate an existing data-table row at runtime**. Find a row in
   one of the game's status-effect data tables whose `Type` matches
   what we want (e.g. `FallDamage = 14`), write our skill-scaled
   `Value` to its +0x34 float, then
   `CreateAndAddEffect(thatRowHandle)` on the live component.
   Cheapest. Risk: the row is shared across the game; mutating it
   affects any other systems using the same row.
2. **Find a benign / unused row to repurpose**. Same as (1) but
   pick a row no other system reads from. E.g. an unused
   `EStatusEffectType::Generic = 0` row, or one whose
   `ApplicationTags` we can flip so vanilla code skips it. Lower
   risk than (1) if such a row exists.
3. **Inject our own row into the data table**. The data table is a
   `UDataTable` UObject; rows live in a `TMap<FName, uint8*>`
   internal field. Append a new entry at runtime keyed by a
   mod-specific name, populate with our `FStatusEffectData` bytes,
   then reference it. Most invasive, also most stable. No
   collision with vanilla rows.
4. **Manual `UStatusEffect` subclass with overridden getters**.
   Create a subclass UClass at runtime that overrides
   `GetStatusEffectType()` / `GetValueForStat()`. Engine then sees
   our subclass return our values. Heaviest path; UE class
   manipulation at runtime is nontrivial.

### The native query path (validated in-game)

`fall_hook.rs` ships a probe (gated on `impact_resistance > 0`)
that calls `UStatusEffectComponent::GetValueForStat(StatType,
false)` and `UUserInterfaceStatics::GetStatusEffectValueType(StatType)`
for a handful of damage-relevant stat types on the player's
component, logging both the runtime value and the combine
semantic. Observed on a mid-game player with equipped gear:

```
rpg/sfx-probe:
  FallDamage(14)=1.000/mul              <- vanilla multiplier
  DamageReduction(29)=1.000/mul         <- vanilla multiplier
  DamageReductionMultiplier(30)=0.000/add
  AttackDamage(23)=1.210/mul            <- gear gives +21% damage
  LifeSteal(38)=0.000/add
  CriticalHitChance(31)=0.060/add       <- gear gives 6% crit
  CriticalDamage(62)=0.000/add
  ReflectDamage(37)=0.000/add
  MaxHealth(5)=30.000/add               <- gear gives +30 HP
```

The `EStatusEffectValueType` enum is two-state plus `None`:

```cpp
enum class EStatusEffectValueType : uint8 {
    None     = 0,
    Add      = 1,
    Multiply = 2,
};
```

So per stat, the engine either sums all active effects' values or
multiplies them. Vanilla baseline differs by combine type: `mul`
stats start at `1.0` (so no effect = no change), `add` stats start
at `0.0` (so no effect = no contribution).

### Stat semantics table (what each skill writes)

For every skill in our catalog, the contribution we add is fully
determined by `(stat_type, combine_semantic, max_bonus, current
level progress)`:

| Skill | Stat type | Combine | Vanilla | Our contribution per skill level |
| ----- | --------- | ------- | ------- | -------------------------------- |
| `fall_resistance` | `FallDamage` (14) | mul | 1.0 | `1 - reduction` (level 100 = 0.0; multiplied total = 0.0 = no fall damage) |
| `impact_resistance` | `FallDamage` (14) or `DamageReduction` (29) | mul | 1.0 | `1 - reduction` (same shape) |
| `armor` | `DamageReduction` (29) | mul | 1.0 | `1 - reduction` |
| `attack_damage` | `AttackDamage` (23) | mul | 1.0 baseline (gear stacks) | `1 + bonus` (level 100 with `max_bonus=3` -> `4.0`) |
| `lifesteal` | `LifeSteal` (38) | add | 0.0 | `bonus * progress` (level 100 with `max_bonus=0.9` -> `0.9`) |
| `critical_chance` | `CriticalHitChance` (31) | add | 0.0 | raw probability fraction |
| `critical_damage` | `CriticalDamage` (62) | add | 0.0 | bonus fraction |
| `thorns` | `ReflectDamage` (37) | add | 0.0 | reflect fraction |
| `max_health` | `MaxHealth` (5) | add | 0.0 baseline (gear stacks) | flat HP bonus |
| `hunger_resistance` | `HungerRate` (17) | add or mul (probe) | TBD | scales drain |
| `thirst_resistance` | `ThirstRate` (18) | add or mul (probe) | TBD | scales drain |
| `move_speed` | `MoveSpeed` (1) | add or mul (probe) | TBD |. Migration of CMC writes |

The "probe" rows have not yet been observed in-game; need to
extend the sfx-probe to those stat types before migration to
confirm semantics.

### What "FallDamage = 1.0/mul" means for our migration

The vanilla baseline for `FallDamage` is `1.0` and the combine
semantic is `mul`. Native fall code reads the stat and multiplies
into the computed damage. Our skill contributes a value that
the engine multiplies *together* with vanilla 1.0:

- Level 100 (`reduction = 1.0`) -> contribute `0.0` -> total =
  `1.0 * 0.0 = 0.0` -> fall damage * 0 = no damage.
- Level 50 (`reduction ~0.71`) -> contribute `0.29` -> total =
  `1.0 * 0.29 = 0.29` -> ~29% damage taken.
- Level 0 -> no effect added -> total = `1.0` -> full damage.

Smooth sqrt-curve scaling drops out for free.

### Vanilla data table identified

Probe of the player's live `StatusEffectComponent.StatusEffects`
array reveals **all status effects in the game flow through one
data table**:

```
DataTable: /Game/Blueprints/Attacks/Table_StatusEffects.Table_StatusEffects
```

Sample active rows on a mid-game player (every entry references
the table above):

| Row name | What it does |
| -------- | ------------ |
| `PlayerUpgradeHealth1` | additive HP from a Player Upgrade |
| `WeaponClub` | equipped club bonus |
| `RogueFinisherCriticals` | perk (stacks; appears multiple times) |
| `AntRedStaminaAttack` | gear / perk |
| `FighterFinisherStun` | perk |
| `MaxHealthSmall` | gear-driven +30 HP |
| `PerkSpearTier1` | perk |
| `PerkSpearThrowAttackUpTier1` | perk |

Naming convention is `<Source><Effect><Tier>`. The table holds
hundreds of rows. One per status effect template the game
ships.

Practical consequence: the canonical injection point is to
write a row in `Table_StatusEffects` (or mutate an unused one)
and reference it from a row handle, then call
`CreateAndAddEffect(handle)` on the player's component.

### Implementation plan

1. **Resolve `Table_StatusEffects` UDataTable** at apply time. Two
   ways: GObjects scan by name, or follow any existing
   `UStatusEffect.StatusEffectRowHandle` on the player (the
   `DataTable*` field of the handle is the table). Latter is
   cheaper if any vanilla effects are active.

2. **Locate or inject a row whose `Type` matches our skill's
   target stat.** Call `UDataTableFunctionLibrary::GetDataTableRowNames`
   to enumerate, then `GetDataTableRowFromName(Table, name, &out)`
   for each name to copy the `FStatusEffectData` bytes into our
   buffer; check the `Type` field at struct offset +0x30. Cache the
   first match per stat.

   - If a row with the right `Type` exists: write our skill-scaled
     value to its `Value` field at struct offset +0x34, then use
     its row handle.
   - If no row exists for the stat we want (e.g. we cannot find a
     `FallDamage` template): inject a new row by walking the
     `UDataTable.RowMap` TMap and adding an entry, or pick a
     low-impact existing row and overwrite its Type+Value. Path
     decision per stat after we see the row catalog.

**In-game probe results (2026-05-05).** First sfx-list run with the
linear `RowMap` walk over an active player's StatusEffects array:

- `Type` at struct offset +0x30 and `Value` at +0x34 are confirmed
  correct. `PlayerUpgradeHealth1` resolved to `Type=5 Value=20.0`
  (Type 5 = MaxHealth in `EStatusEffectType`); `MaxHealthSmall`
  resolved to `Type=5 Value=10.0`. Numbers match the visible
  vanilla effects.
- The `Table_StatusEffects` data table identification is correct;
  every active effect on the player traces back to it.
- The linear walk only finds ~4 of 13 active rows. The other 9
  (e.g. `RogueFinisherCriticals`, `AntRedStaminaAttack`,
  `FighterFinisherStun`, `PerkSpearTier1`,
  `PerkSpearThrowAttackUpTier1`, `MealTier1Regen`,
  `MealTier1WeakPointDamage`) consistently miss. The engine
  resolved them (their `UStatusEffect` instances are live), so the
  rows exist in the map. Our walk's stride or layout assumption
  is wrong.
- Root cause hypothesis: `TMap<K,V>` is backed by
  `TSet<TPair<K,V>>` whose element type is
  `TSetElement<TPair<K,V>>`. Pair plus `HashNextId : int32` plus
  `HashIndex : int32`, stored inside a `TSparseArray` with a
  free-list bitset. Walking with stride = `sizeof(K) + sizeof(V)`
  drifts past valid entries; walking without consulting the
  allocation bitarray also reads free slots (mostly zeros, harmless
  but wasteful). Need to honor the real TSetElement stride and the
  TSparseArray allocation flags.
- Fix path: prefer `UDataTableFunctionLibrary::GetDataTableRowNames`
  + `GetDataTableRowFromName` (engine-provided lookup, no manual
  TMap walk required) for read; only walk the map directly when we
  need to inject a new row.

#### Concrete spike plan (status-effect write+apply)

Five steps. Step 1 gates everything; cheap (~25 LOC) and proves the
read path. Steps 2-4 are the proof-of-concept; Step 5 is the
catalog migration.

1. **Fix the linear `RowMap` walk**
   (now owned by `ueforge::ue::status_effect::read_active`).
   `TMap<K,V>` is backed by `TSet<TPair<K,V>>` whose element is
   `TSetElement<TPair> { pair: 16, HashNextId: i32, HashIndex: i32 }`
   = 24 bytes, stored in a `TSparseArray` with a separate
   `TBitArray AllocationFlags`. Current walk uses stride 16 and
   ignores the bitarray, which is why it finds 4/13 rows. Fix:
   stride = 24, consult the bitarray (or cross-check against
   `GetDataTableRowFromName` once during validation, then trust
   the walk).
   - Validation: re-run `probe_player_status_effects`, expect
     13/13 rows to resolve with `Type=N Value=F.FFF`, no
     `<row-not-found>`.
2. **Mutate one row's `Value`.** Linear walk now returns a *live*
   `uint8*` into the engine's RowMap. Test against
   `MaxHealthSmall` (currently `Type=5 Value=10.000`): write 100.0
   to `+0x34` on slot activation.
3. **Apply via `CreateAndAddEffect`.** Resolve
   `UStatusEffectComponent::CreateAndAddEffect` UFunction (player's
   component is at `+0x1378`). Build
   `FDataTableRowHandle { DataTable*, FName }` (16 bytes), call via
   `process_event`.
4. **Witness.** Existing `probe_status_effect_values` already logs
   `MaxHealth(5)=30.000/add` on every fall (sum of vanilla
   `PlayerUpgradeHealth1=20` + `MaxHealthSmall=10`). After Step 2
   mutates the small row to 100, the line should jump to
   `MaxHealth(5)=120.000/add`. That single observation proves the
   full pipeline.
5. **Generalize.** Add
   `SkillEffect::PlayerStatusEffect { stat_type, max_value, format }`
   in `skills.rs`. `apply.rs` gains an arm: pick row per stat (cache
   once), mutate `Value` to `skill_bonus(max_value, level)`,
   AddEffect. Migrate in this order:
   - `fall_resistance` -> Type=14 (FallDamage), mul stat. Velocity-
     stomp in `fall_hook.rs` stays as fallback until in-game
     validated, then deletes.
   - `impact_resistance` -> Type=30 (DamageReductionMultiplier) or
     reuse Type=14. Replaces the binary
     `RequiredDamageTypeFlags = 0xFFFFFFFF` hack with a real curve.
   - `lifesteal` -> Type=38, removes the `Runtime` no-op.
   - Remaining catalog (armor, attack damage, future crit, thorns,
     max health) follow the same shape, ~5 LOC each.

Touched files for the spike (Steps 1-4): `fall_hook.rs` (~80),
`apply.rs` (~30), `skills.rs` (~5 for a throwaway `maxhp_test`
skill or just hardcoded behind `impact_resistance > 0`).

3. **Build the row handle** as
   `FDataTableRowHandle { DataTable* = ourTable, FName = ourRowName }`
   (16 bytes). Note `UStatusEffect` stores the
   `_NetCrc32` variant which is 0x28 bytes. The first 16 are the
   same handle, the trailing 0x18 is replication metadata that the
   engine populates after AddEffect.

4. **Call `CreateAndAddEffect(handle)`** on the player's live
   `UStatusEffectComponent` via `process_event`. Engine
   instantiates a `UStatusEffect`, points it at the row, replicates
   it. Native `GetValueForStat` queries pick it up.

5. **On level change** (skill spend or save activation): re-look-up
   our row, write a new scaled `Value`, re-AddEffect or rely on
   reactivation.

6. **On slot deactivate** (player exits save): RemoveEffect via
   `UStatusEffectComponent::RemoveEffect(effect, true)`.

### Why mutating a shared row is acceptable here

If we use an existing row and mutate `Value`, vanilla code
referencing the same row also sees our value. Which would be
a bug in most contexts. But in our case the vanilla rows are
templates that the game instantiates new effects from at gear
equip / perk unlock time. Mutating the template after vanilla
has already created its instances does not affect those
instances (each instance carries the row handle but reads the
row's current state on every `GetValueForStat` query, so
mutation propagates to every instance).

Cleanest implementation is therefore:

- **Pick a unique-to-us row** (either inject a new entry into the
  RowMap, or repurpose an existing row that no vanilla path
  references at this moment, e.g. one tied to a gear tier the
  player is not wearing).

The earlier four-path list collapses to one concrete plan above.

### Why this beats the current implementations

- **Sqrt-curve scaling.** Status effect values are floats. Set
  `value = max_bonus * sqrt(level/100)` and the engine handles the
  rest.
- **Type filtering for free.** `GetValueForStatForDamageTypeFlags`
  natively skips non-matching damage. We do not have to write a
  separate gate (which is what `RequiredDamageTypeFlags = 0xFFFFFFFF`
  is doing today as a binary stand-in).
- **Stacks with vanilla mods, items, perks.** Other status-effect
  sources in the game add or remove entries on the same component.
  Our skill becomes one of many sources rather than a one-off
  field write that other systems do not see.
- **Save / load semantics handled by the engine.** Status effects
  are replicated and serialized through the existing UE pipeline.

### Migration plan (tracked in todo.md)

Current direct-field-write implementations are interim. The
migration:

1. Locate `UStatusEffect` field layout (StatType offset, Value
   offset, optional duration / source / tags).
2. Add `SkillEffect::PlayerStatusEffect { stat_type, max_value,
   format }` variant in `skills.rs`.
3. `apply.rs` walks live player `StatusEffectComponent`, finds or
   creates a `UStatusEffect` for the given stat type, sets its
   value to `max_value * sqrt(level/100)`.
4. Migrate skills one at a time: Fall, Impact, Lifesteal first
   (they are the ones we know native code consults). Then Armor,
   Attack Damage, etc. once parity is verified.
5. Remove `RequiredDamageTypeFlags` / direct-field hacks from
   `apply.rs` and the velocity-stomp from `fall_hook.rs` after
   each skill is migrated and validated.

## UE4SS hooking caveats (Grounded 2 observations)

Generic UE4SS hooking caveats (Issue #626 / `Final+Native+BlueprintCallable`
silent failures, concrete-BP vs base-class hook scope, CDO-patches-
don't-reach-pre-spawned-instances) are documented in
[`../../ueforge/docs/ue-engine.md`](../../ueforge/docs/ue-engine.md)
"UE4SS hooking caveats". The same issues bite any UE5 mod built on
UE4SS, not just Grounded 2.

The Grounded 2-specific instances confirmed broken:

- `/Script/Maine.SurvivalCharacter:ApplyFallDamage` (Issue #626)
- `/Script/Maine.HealthComponent:ApplyDamage` (Issue #626)

Concrete player BP classes that hooks must enumerate (vs the base
`SurvivalCharacter`):

- `BP_SurvivalPlayerCharacter_Female02_C`
- `BP_SurvivalPlayerCharacter_Gellarde_C`
- (other gender / appearance variants. See `fall_hook.rs`)
