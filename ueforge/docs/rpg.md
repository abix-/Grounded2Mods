# RPG framework

> **Authoritative on:** ueforge's level-up / skill-tree / save-
> persistence framework. The complete RPG system every UE5
> RPG / survival mod needs.

ueforge ships the bones of an RPG mod. Catalog row + lookup,
state schema, persistence layer, slot detection, level math,
skill-spend mechanics, ImGui tab template, vanilla baseline
cache, and a library of standard `Effect` types covering ~90% of
common operations. The game crate supplies only:

- The `CATALOG` content (which skills exist, with what
  parameters, picking framework `Effect` types where possible).
- Per-game `Effect` impls for the 10% of operations the
  framework doesn't already cover.
- The kill / damage hook that calls `record_xp`.
- Game-specific selectors (`live_player`, etc.) registered with
  `ueforge::selector`.

That's the irreducibly game-specific surface. Everything else
flows from one of the types in this doc.

## The composition model

Per [architecture.md](architecture.md), a Skill is one Effect
applied with parameters at a level, fired by a Trigger:

```text
SkillDef = id + display_name + max_level + EffectDef + TriggerDef
EffectDef = kind + &dyn Effect              -- WHAT to do
TriggerDef = kind + &dyn Trigger            -- WHEN to do it
```

Effects are types implementing the `Effect` trait. Each
`<X>Effect` struct captures one game operation we figured out
how to perform; instances of that struct are individual
parameterizations (one per skill). New operation = one new type
+ one impl. New skill that uses an existing operation = one new
catalog row + one new static instance.

## Effect trait

```rust
pub trait Effect: Send + Sync + 'static {
    fn apply(&self, level: u32, max_level: u32, ctx: &TriggerCtx);
    fn format(&self, level: u32, max_level: u32) -> String;
}

pub struct EffectDef {
    pub kind: &'static str,
    pub imp: &'static dyn Effect,
}
```

`apply` does the work. `format` produces the ImGui-row text.
The `&TriggerCtx` argument carries typed event data when the
trigger is event-driven; CDO-write effects receive
`TriggerCtx::SlotChange` and ignore the payload.

## Standard Effect types (framework-shipped)

Ten types in `ueforge::rpg::effect` cover the common UE5 RPG
operations. Game catalogs reference them via static instances
+ `EffectDef::new(kind, &INSTANCE)`:

| Type | Pattern |
|---|---|
| `PlayerFloatEffect` | Direct f32 write at offset on player CDO + live |
| `SubcomponentFloatEffect` | f32 on player subcomponent (HC, CMC, etc) |
| `SubcomponentAdditiveEffect` | Captured-vanilla + bonus (Max Health pattern) |
| `SubcomponentU32MaskEffect` | Binary u32 mask (impact-resistance pattern) |
| `SubcomponentMultiplyEffect` | Captured-vanilla * (1 + bonus * progress) over offsets |
| `ClassFieldsMultiplyEffect` | Same shape on every instance of a class (regen / global-data) |
| `RuntimeEffect` | No CDO write; hot-path callback owns the effect |
| `LifestealEffect` | Heal the player for a scaled fraction of outgoing player damage |
| `ImpactReversalEffect` | Reverse matching player damage after engine application |
| `StatusEffectApply` | UE5 row-driven status effect via UFunction |

Adding a skill that uses one of these is two declarations:

```rust
use ueforge::rpg::{EffectDef, PlayerFloatEffect, SkillDef};

static EFFECT_ATTACK_DAMAGE: PlayerFloatEffect = PlayerFloatEffect {
    player: &PLAYER,
    offset: TypedField::at(0x12B8),
    base: 1.0,
    max_bonus: 3.00,
    format: PercentFormat::PlusPercentMult { word: "damage" },
};

// In CATALOG_ENTRIES:
SkillDef {
    id: "attack_damage",
    display_name: "Attack Damage",
    max_level: 100,
    effect: EffectDef::new("PlayerFloat", &EFFECT_ATTACK_DAMAGE),
    trigger: &ueforge::rpg::trigger::ON_SLOT_CHANGE,
},
```

## Game-specific Effects

When the operation isn't in the framework menu, the game crate
declares its own type + impl + static instance:

```rust
// grounded2-mod/src/rpg/effects.rs
use ueforge::rpg::Effect;

pub struct BackpackSlotsEffect {
    pub max_bonus_slots: i32,
}

impl Effect for BackpackSlotsEffect {
    fn apply(&self, level: u32, _max_level: u32, _ctx: &ueforge::rpg::TriggerCtx) {
        // ... game-specific logic accessing crate::settings,
        //     crate::patch::run, crate::inv_hook, etc. ...
    }
    fn format(&self, level: u32, _max_level: u32) -> String { ... }
}

pub static BACKPACK: BackpackSlotsEffect = BackpackSlotsEffect {
    max_bonus_slots: 460,
};
```

Then the catalog row references it:

```rust
SkillDef {
    id: "backpack",
    display_name: "Backpack",
    max_level: 100,
    effect: EffectDef::new("BackpackSlots", &crate::rpg::effects::BACKPACK),
    trigger: &ueforge::rpg::trigger::ON_SLOT_CHANGE,
},
```

There's no central `apply_skill` match anymore -- `Tracker`
iterates the catalog and calls `effect.apply(...)` per row. The
trait dispatches to the right impl.

## HealthBinding. Universal HC simulation ops

`ueforge::rpg::health::HealthBinding` lets a game declare its
HealthComponent constants once and get framework-shipped
`simulate_add_health` + `simulate_set_current_health` ops
registered into `OP_REGISTRY`:

```rust
static HEALTH_CLASS: ueforge::ue::ClassRef =
    ueforge::ue::ClassRef::new("HealthComponent");

static HEALTH_BINDING: ueforge::rpg::health::HealthBinding =
    ueforge::rpg::health::HealthBinding {
        hc_class: &HEALTH_CLASS,
        hc_selector: "live_player_hc",
        current_damage_offset: 0x032C,
        max_health_offset: 0x0328,
        add_health_function: Some("AddHealth"),
        set_current_health_function: Some("SetCurrentHealth"),
    };

// From worker():
ueforge::rpg::health::register(&HEALTH_BINDING, &PE_QUEUE, PE_TIMEOUT_HINT);
```

Same pattern as `damage::DamageBinder` and
`inventory::ViewportBinder`: framework ships the op handlers +
parm shapes (Maine-canonical: `AddHealth(amount, causer)`,
`SetCurrentHealth(desired_health)`); game declares the binding.
Each op in the binding is conditional -- `Some(name)` registers
it; `None` skips. `simulate_apply_damage` is intentionally NOT
shipped (PE re-entry hazard; gated on Wave E1 safe drain site).

## Triggers

Each catalog row carries `trigger: &'static TriggerDef`. The
framework ships five firers as of Phase 5c (2026-05-11):

| TriggerDef | Variant fired | Source |
|---|---|---|
| `ON_SLOT_CHANGE` | `TriggerCtx::SlotChange` | `Tracker::activate_slot` / spend / refund / reapply_* |
| `ON_DAMAGE_DEALT` | `TriggerCtx::DamageDealt(&damage::DamageEvent)` | Game-side `DamageBinder::before` calls `TRACKER.fire` |
| `ON_DAMAGE_TAKEN` | `TriggerCtx::DamageTaken(&damage::DamageEvent)` | Game-side `DamageBinder::after` calls `TRACKER.fire` |
| `ON_KILL` | `TriggerCtx::Kill(&KillEvent)` | Game-side binder on confirmed kill |
| `ON_FALL` | `TriggerCtx::Fall(&FallEvent)` | `ueforge::fall::FallBinder` calls `TRACKER.fire` |

`Tracker::fire(ctx)` walks the catalog under a single
`inner.lock()`, snapshots `(skill, level)` pairs for subscribed
skills (filter: `skill.trigger.kind` matches the `ctx` variant)
into a 32-slot stack array, drops the lock, then calls
`Effect::apply` outside the lock so an Effect that re-enters
Tracker (e.g. an OnKill effect calling `record_xp`) cannot
deadlock. Zero heap allocs per fire.

`Tracker::apply_one_unlocked` (drives activate / spend / refund /
reapply_*) filters to OnSlotChange-kinded skills only.
Event-driven Effects only fire on their event source, never via
SlotChange.

`TriggerCtx::Tick { dt }` is a reserved variant with no firer
yet (periodic poller defers until a periodic skill needs it).

See [architecture.md](architecture.md) "Composition model" for
the full Hook vs Trigger layering rationale.

## SkillDef. Catalog row

```rust
pub struct SkillDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub max_level: u32,
    pub effect: EffectDef,
    pub trigger: &'static TriggerDef,
}

pub struct SkillRegistry {
    entries: &'static [SkillDef],
}
impl SkillRegistry {
    pub const fn new(entries: &'static [SkillDef]) -> Self;
    pub fn def(&self, id: &str) -> Option<&'static SkillDef>;
    pub fn entries(&self) -> &'static [SkillDef];
    pub fn iter(&self) -> std::slice::Iter<'_, SkillDef>;
}
```

No type parameters -- `SkillDef`/`SkillRegistry`/`Tracker` are
all concrete types. Catalog declaration:

```rust
pub const CATALOG_ENTRIES: &[SkillDef] = &[
    SkillDef {
        id: "backpack",
        display_name: "Backpack",
        max_level: 100,
        effect: EffectDef::new("BackpackSlots", &crate::rpg::effects::BACKPACK),
        trigger: &ueforge::rpg::trigger::ON_SLOT_CHANGE,
    },
    SkillDef {
        id: "attack_damage",
        display_name: "Attack Damage",
        max_level: 100,
        effect: EffectDef::new("PlayerFloat", &EFFECT_ATTACK_DAMAGE),
        trigger: &ueforge::rpg::trigger::ON_SLOT_CHANGE,
    },
    // ...
];

pub static CATALOG: SkillRegistry = SkillRegistry::new(CATALOG_ENTRIES);
```

`CATALOG.def(id)` is O(N) linear scan; catalogs are tiny
(~25 rows) and lookups are cold-path (button click, snapshot,
ImGui label). No hashmap.

## SkillsState. On-disk schema

```rust
pub struct SkillsState {
    pub schema_version: u32,
    pub xp: u64,
    pub level: u32,
    pub skill_points: u32,
    pub skill_levels: BTreeMap<String, u32>,
}

impl SkillsState {
    pub fn level_of(&self, skill_id: &str) -> u32;
}

pub const SCHEMA_VERSION: u32 = 1;
```

Open shape (`#[serde(default)]` on every field) so adding new
skills doesn't break existing save files. `skill_levels` is a
`BTreeMap<String, u32>`. String keys are the catalog `id`s,
stable forever.

`schema_version: u32` is bumped whenever the on-disk shape
changes in a non-additive way. Older save files lacking the
field deserialize as `1` via `default_schema_version`. Future
migrations branch on this value at load.

**There is no public `spend` / `refund` on `SkillsState`.** They
were removed deliberately: an in-memory mutator without a save
hook is a trapdoor. A sloppy caller mutates the copy and a
crash before the next persist loses the change. The only
mutation path is `Tracker::spend_skill_points` /
`refund_skill_points`, which mutate state and persist under the
same lock with explicit rollback.

## SlotStore<S>. Per-slot JSON persistence

```rust
pub struct SlotStore<S> { /* ... */ }

impl<S: Serialize + DeserializeOwned + Default> SlotStore<S> {
    pub const fn new(subdir: &'static str) -> Self;
    pub fn path(&self, slot: &str) -> PathBuf;
    pub fn load(&self, slot: &str) -> S;
    pub fn save(&self, slot: &str, state: &S) -> io::Result<()>;
    pub fn last_error(&self) -> Option<String>;
}
```

### Durable atomic save

`save()` writes `<DLL_dir>/<subdir>/<slot>.json.tmp`, calls
`sync_all()` on the temp file, then `fs::rename` to
`<slot>.json`. The fsync before the rename is what turns
"rename is atomic" into "rename is also durable across power
loss". Without it a crash between the rename and the kernel's
own flush leaves a torn or zero-byte file. The very failure
mode atomic-rename was supposed to prevent.

`save()` returns `io::Result<()>`. The store also caches the
last failed save's message in `last_error()` so debug snapshots
can surface it without threading the Result through callers
that don't know what to do with it. The cache clears on the
next successful save.

Loads return `S::default()` on missing / unparseable file
(logged). Soft fall-through is the right behavior for save
data we couldn't read.

### Slot path validation

Slot keys are validated to reject empty strings, leading dots,
and path separators (`/ \ : \0`). Invalid keys route to a
sentinel filename `__invalid__.json` so malformed input is
visible to the next reader instead of silently writing somewhere
unexpected. This closes the trapdoor where a user-controlled
string could escape the configured subdir.

### Generic over S

`SlotStore<S>` lets games persist extended state alongside the
framework `SkillsState` (achievements, kill counters, run
meta). g2rpg uses `SlotStore<SkillsState>` directly; an extended
mod could use `SlotStore<MyExtendedState>` where
`MyExtendedState` includes a `pub skills: SkillsState` field.

## Curve. XP math

```rust
pub struct Curve {
    pub base: f64,
    pub exponent: f64,
    pub max_level: u32,
}

impl Curve {
    pub const fn new(base: f64, exponent: f64, max_level: u32) -> Self;
    pub fn cumulative_xp_for_level(&self, level: u32) -> u64;
    pub fn level_for_xp(&self, xp: u64) -> u32;
}

pub const CURVE: Curve = Curve::new(100.0, 1.8, 50);
```

`cumulative_xp_for_level(level) = round(base * level^exponent)`.
`level_for_xp(xp)` is linear scan from `max_level` downward (cheap
for typical caps 50-100; bounded).

Defaults match g2rpg: 100 / 1.8 / 50. Generous early, flat at the
end. Adjust `(base, exponent, max_level)` to retune.

## Bestiary. Per-creature XP table

```rust
pub struct Bestiary {
    pub table: &'static [(&'static str, u32)],
    pub default_xp: u32,
}

impl Bestiary {
    pub const fn new(
        table: &'static [(&'static str, u32)],
        default_xp: u32,
    ) -> Self;
    pub fn lookup(&self, class_name: &str) -> u32;
}
```

UE5 RPG mods award XP based on the dying actor's BP class. Hold
the `(class_name, base_xp)` pairs in a `static Bestiary`, look up
once per kill from the kill hook. Linear scan; realistic
bestiaries are <100 rows so the cost is invisible.

```rust
pub static BESTIARY: Bestiary = Bestiary::new(
    &[
        ("BP_Aphid_C", 5),
        ("BP_Roly_Poly_C", 25),
        ("BP_Spider_C", 75),
    ],
    5, // default for unknown classes
);
```

## sqrt_progress. Diminishing-returns curve

```rust
pub fn sqrt_progress(level: u32, max: u32) -> f32;
```

`sqrt(level / max)`. Level 1 = ~10% of max, level 25 = 50%,
level 100 = 100%. The default skill scaling. Game `apply` arms
multiply `max_bonus * sqrt_progress(level, skill.max_level)` to
get the at-level bonus.

## Tracker

The runtime entry point. Owns slot binding, in-memory state,
the canonical `DisabledSkills` toggle set, persistence. Drives
every state change transactionally with disk save. No type
parameters. The catalog is concrete `&'static SkillRegistry`,
and apply dispatches via `SkillDef.effect.imp` (trait object).

```rust
use ueforge::rpg::{Tracker, Curve};

pub static TRACKER: Tracker = Tracker::new(
    &CATALOG,
    Curve::new(100.0, 1.8, 50),
    "saves",
);
```

### Slot lifecycle

```rust
TRACKER.activate_slot(slot_key);
// ... gameplay ...
TRACKER.deactivate_slot();
```

`activate_slot` loads `<DLL_dir>/saves/<slot_key>.json` (or
default if missing), reconciles `level` from `xp` if needed,
iterates the catalog and calls `effect.apply(level, max,
TriggerCtx::SlotChange)` per row so the world reflects current
skill levels.

`deactivate_slot` drops the in-memory state.

`slot_key` is whatever stable identifier your game's saves use.
For UE5 games with a `PlaythroughGuid` on the GameState, see
[`SlotKeyResolver`](#slotkeyresolver) below.

### Spending / refunding

```rust
let n = TRACKER.spend_skill_points("attack_damage", 1);     // returns spent count
let n = TRACKER.refund_skill_points("attack_damage", 10);   // returns refunded count
```

Both follow the **stage-save-commit** pattern under the inner
lock:

1. Validate (skill exists, points available, max-level cap).
2. Snapshot rollback values (the previous `skill_points` + the
   previous `skill_levels` entry for this id, including the
   "absent" case).
3. Mutate `SkillsState` in-memory.
4. Persist via `store.save(slot, state)`.
5. **On save success:** call `skill.effect.apply(level, max,
   TriggerCtx::SlotChange)` to push the change into the live
   world.
6. **On save failure:** restore the snapshotted values, log the
   error, return `0`. The world is NOT touched. The session
   reflects what's actually on disk.

This is the durability story. A full disk, EACCES, antivirus
quarantine. Any `io::Error` from the atomic save. Rolls
back instead of letting the in-memory state and the disk drift.
Callers can read `Tracker::last_save_error()` for the cause.

The save runs BEFORE the world apply on purpose: applying first
and then failing the save would leave the buff active for the
session but ungrabable on next restart. Save first, apply only
on success.

### Recording XP

```rust
let result: Option<XpResult> = TRACKER.record_xp(awarded);
```

Awards `awarded` XP, recomputes level via the curve, increments
`skill_points` on level-up, persists. Returns `Option<XpResult>`
-- `None` if no slot is active OR if the disk save failed (in
which case the in-memory `xp` / `level` / `skill_points` are
rolled back to their pre-call values so the session matches
disk). Otherwise:

```rust
pub struct XpResult {
    pub awarded: u64,
    pub total_xp: u64,
    pub old_level: u32,
    pub new_level: u32,
    pub points_gained: u32,
}

impl XpResult { pub fn levelled(&self) -> bool; }
```

Use `result.levelled()` to drive "LEVEL UP!" feedback (sound,
log, UI flash).

### Reading state

```rust
let snap = TRACKER.with_state(|s| (s.level, s.xp));   // Option<R>
let slot = TRACKER.current_slot();                      // Option<String>
```

`with_state` runs your closure under the inner lock. Don't do
heavy work inside; copy the fields you need and release.

### Reapplying

```rust
TRACKER.reapply_one("move_speed");      // for the on/off toggle
TRACKER.reapply_all();                  // for /debug reload_slot
```

### Debug grants

```rust
TRACKER.debug_grant_skill_points(50);
```

Adds points without earning them. Logs the grant. For dev only.
Returns `false` (and rolls back the in-memory `skill_points`)
if the disk save fails. Same stage-save-commit contract as
spend/refund.

## SlotKeyResolver. Save-slot detection

```rust
use ueforge::rpg::SlotKeyResolver;

pub static RESOLVER: SlotKeyResolver = SlotKeyResolver::new(
    "InGameGameState",   // class on which the GUID lives
    0x032C,              // FGuid offset
);

let slot: Option<String> = RESOLVER.resolve();
```

Walks GObjects for the first live instance of the configured
class, reads an `FGuid` at the offset, formats as a 32-char hex
filename. Returns None when not in-world (main menu, between
saves) or when the GUID is zero.

Plug into `SlotPoller`:

```rust
SlotPoller::spawn(
    Duration::from_secs(1),
    || RESOLVER.resolve(),
    |slot| TRACKER.activate_slot(slot),
    || TRACKER.deactivate_slot(),
);
```

## SlotPoller. 1Hz transition watcher

```rust
SlotPoller::spawn(interval, resolve, on_activate, on_deactivate);
```

Spawns a Win32 worker thread that polls `resolve()` every
`interval`. State machine on the resolver's return value:

| Previous | Current | Action |
|---|---|---|
| `None` | `Some(s)` | `on_activate(s)` |
| `Some(a)` | `Some(b)` where `a != b` | `on_deactivate()` then `on_activate(b)` |
| `Some(_)` | `None` | `on_deactivate()` |
| else | else | nothing |

The worker runs for life of process today (kovarex P0. Need
`shutdown_handle()`). Panics in the resolver are caught
silently (kovarex P1. Need logged payload).

## DisabledSkills. Per-skill on/off toggle

```rust
let disabled = TRACKER.disabled_skills();
disabled.set("attack_damage", true);          // disabled
disabled.is_disabled("attack_damage");        // true
disabled.snapshot();                          // Vec<&'static str>
```

Owned by `Tracker`. One canonical store per RPG mod. Disabling
treats a skill as level 0 in the apply path without refunding
the player's points. "drop a buff on demand" toggles in the
ImGui tab. Not persisted.

`Tracker::apply_one_unlocked` checks `disabled.is_disabled(id)`
before calling `effect.apply` and substitutes level=0 when the
skill is disabled. CDO-write effects naturally restore vanilla
at level=0; `RuntimeEffect`s no-op so their hot-path callback
sees the disabled state via the same accessor.

## VanillaCache<K, V>. Baseline capture

```rust
use ueforge::rpg::VanillaCache;

pub static MOVEMENT_VANILLA: VanillaCache<usize, f32> = VanillaCache::new();

let baseline = MOVEMENT_VANILLA.get_or_init(offset, current_value);
let new_value = baseline * (1.0 + bonus);
write_field(obj, offset, new_value);
```

Per-key first-write-wins cache. The first apply call captures
the engine's vanilla; subsequent calls return the captured
value (so we always scale from vanilla, not from whatever the
mod has already overwritten).

Methods:

```rust
pub fn get_or_init(&self, k: K, v: V) -> V;     // returns captured baseline
pub fn set_if_unset(&self, k: K, v: V) -> bool; // returns insert flag
pub fn get(&self, k: K) -> Option<V>;
pub fn snapshot(&self) -> Vec<(K, V)>;
pub fn clear(&self);
```

Use one cache per logical-baseline-set (movement, global combat
data, HC masks). Single-baseline values (one f32, one offset)
are simpler with `OnceLock<f32>`.

## tab::render. ImGui tab template

```rust
use ueforge::rpg::tab::{render as render_rpg_tab, ToggleFns};

const TOGGLES: ToggleFns = ToggleFns {
    is_enabled: |id| !DISABLED_SKILLS.is_disabled(id),
    set_enabled: |id, on| { DISABLED_SKILLS.set(id, !on); },
};

pub fn render() {
    render_rpg_tab(&TRACKER, Some(&TOGGLES));
}
```

`render(tracker, toggle)` draws:

1. **Header**. "Level N", XP progress bar (current / next),
   "Unspent skill points: K".
2. **Per-skill row** for every catalog entry:
   - `+1` / `+10` buttons (gated on points + below max)
   - `-1` / `-10` buttons (gated on level > 0)
   - Optional `on` checkbox (if `ToggleFns` supplied)
   - Effect text from `skill.effect.format(level, skill.max_level)`
   - Next-level preview when below max
3. **Debug footer** -- `+5 / +50 skill points` for testing.

Pass `None` for the `toggle` arg to omit the per-skill on/off
column (some games don't want it).

The render runs on UE4SS's render thread, once per frame while
the tab is visible. It calls `Tracker::with_state` for each
read; the inner lock is held briefly.

## Putting it all together

The full g2rpg-shaped wiring:

```rust
// Game-specific Effect impls (the 10% the framework menu doesn't cover):
pub struct BackpackSlotsEffect { pub max_bonus_slots: i32 }
impl Effect for BackpackSlotsEffect { /* ... */ }
pub static BACKPACK: BackpackSlotsEffect = BackpackSlotsEffect { max_bonus_slots: 460 };

// Framework Effect static instances per skill:
static EFFECT_ATTACK_DAMAGE: PlayerFloatEffect = PlayerFloatEffect { /* ... */ };

// catalog (game-specific):
pub const CATALOG_ENTRIES: &[SkillDef] = &[
    SkillDef {
        id: "backpack", display_name: "Backpack", max_level: 100,
        effect: EffectDef::new("BackpackSlots", &BACKPACK),
        trigger: &ueforge::rpg::trigger::ON_SLOT_CHANGE,
    },
    SkillDef {
        id: "attack_damage", display_name: "Attack Damage", max_level: 100,
        effect: EffectDef::new("PlayerFloat", &EFFECT_ATTACK_DAMAGE),
        trigger: &ueforge::rpg::trigger::ON_SLOT_CHANGE,
    },
];
pub static CATALOG: SkillRegistry = SkillRegistry::new(CATALOG_ENTRIES);

// tracker (one static, no type parameters):
pub static TRACKER: Tracker = Tracker::new(
    &CATALOG,
    Curve::new(100.0, 1.8, 50),
    "saves",
);

// slot detection (game-specific class + offset):
pub static RESOLVER: SlotKeyResolver =
    SlotKeyResolver::new("InGameGameState", 0x032C);

// world loader: wire SlotPoller to RESOLVER + TRACKER:
SlotPoller::spawn(
    Duration::from_secs(1),
    || RESOLVER.resolve(),
    |slot| TRACKER.activate_slot(slot),
    || TRACKER.deactivate_slot(),
);

// kill hook: call TRACKER.record_xp on confirmed kills:
fn on_kill(creature_class: &str) {
    let xp = xp_for_creature(creature_class);    // game's bestiary
    let _ = TRACKER.record_xp(xp as u64);
}

// ImGui tab:
ueforge::TabDef {
    name: "RPG",
    render: || ueforge::rpg::tab::render(&TRACKER, Some(&TOGGLES)),
}
```

That's a complete RPG mod. The game crate ships ~200-300 LoC
of catalog content + applier dispatch arms + format_effect text
+ kill_hook trampoline. ueforge ships everything else.

## Cross-references

- [lifecycle.md](lifecycle.md). How ModInfo wires the tab in
- [hooks.md](hooks.md). The kill_hook surface that drives
  `record_xp`
- [ue-sdk.md](ue-sdk.md). The ClassRef/TypedField/PlayerRef
  helpers your applier uses for actual writes
- [PERFORMANCE.md](PERFORMANCE.md). VanillaCache rationale,
  hot-path discipline for trampolines
