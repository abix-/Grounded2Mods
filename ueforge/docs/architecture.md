# Architecture

## The composition model: Effects + Triggers + Skills

Before the Def/Registry mechanics, the architectural rule that
makes this whole framework worth its weight:

> **Each thing we research and figure out how to do in the game is
> an Effect. A Skill is one or more Effects applied with some
> parameters. Hooks let us detect WHEN to fire an Effect.**
>
> If we do this generically we only have to do each one of the
> things ONCE.

Three concerns. Three vocabularies.

### Effects. The verbs

An **Effect** is one reusable operation we figured out how to
perform in the game: write an `f32` at an offset on a class CDO,
scale a set of CharacterMovementComponent speed fields, mutate a
status-effect row + invoke `CreateAndAddEffect`, heal a
HealthComponent via PE-call, etc. Each Effect is:

- A **type** that implements the `Effect` trait (the operation
  shape. "scale these fields by `1 + max_bonus * progress`").
- **Parameterized** per use (instances carry the offsets, the
  base, the max_bonus, the class refs).
- **Researched once, used everywhere.** A new game adopts the
  framework's Effect library; it only needs new Effect types
  when the game has a unique operation no other game shares.

### Skills. The products

A **Skill** is a player-facing upgrade slot: id, display name,
max level, and an Effect. Same Skill type across every game.
Adding a Skill is one catalog row that picks an Effect and
supplies its parameters. The Skill never touches the game world
directly. It owns leveling + persistence + UI; it delegates the
actual writes to its Effect.

```rust
SkillDef {
    id: "attack_damage",
    display_name: "Attack Damage",
    max_level: 100,
    effect: EffectDef::new("PlayerFloat", &ATTACK_DAMAGE_FLOAT),
}
```

`ATTACK_DAMAGE_FLOAT` is a `static PlayerFloatEffect` with
`(offset: 0x12B8, base: 1.0, max_bonus: 3.0, format: ...)`. The
Effect type is shared with every other PlayerFloat skill (Armor,
etc.); only the parameters differ.

### Triggers. The WHEN

A skill needs to know WHEN to fire its Effect. That's the
Trigger. Some triggers are **passive** (`OnSlotChange`. The
effect fires when the player levels up / spends / refunds /
toggles); others are **event-driven** (`OnDamageDealt` fires per
player-dealt hit, `OnKill` per kill, `OnFall` per fall, `Periodic`
per tick).

`TriggerDef { kind, imp: &'static dyn Trigger }` is the Def.
symmetric with `EffectDef`. Each trigger TYPE is a struct that
owns its own wiring: filter, decode, subscriber list. CDO-write
skills use `&ueforge::rpg::trigger::ON_SLOT_CHANGE`; event-driven
skills use the corresponding event trigger.

### Hooks vs Triggers. Two layers

`HookDef` and `TriggerDef` are NOT the same:

- **`HookDef`** = the *mechanism*. A vtable patch on a UE5
  class. Low-level. Plumbing. `ProcessEventHook::install(...)`
  produces one. Each `HookDef` is one patched class.
- **`TriggerDef`** = the *semantic event source*. Built ON TOP
  of zero or more `HookDef`s plus filter + decode + typed event
  dispatch. One trigger may install hooks on multiple classes
  (a damage trigger needs HC + concrete BP subclasses). One
  hook may serve multiple triggers (`OnDamageDealt` and
  `OnDamageTaken` share the damage hook). Some triggers
  (`Periodic`, `OnSlotChange`) install no hooks at all.

Skill authors compose at the **Trigger** layer. `HookDef` is
visible only to framework-level wiring code (the trigger types'
`install` methods). The N:M relationship between hooks and
triggers is exactly why both deserve their own Defs. Folding
trigger logic into HookDef would force duplicated filter/decode
across the N hooks one trigger wraps.

### The "do it once" rule

The whole point of this layering: when we figure out how to
mutate, say, a `UStatusEffectComponent` by walking a data table
+ calling `CreateAndAddEffect`, that work lives in **one**
`StatusEffectApply` Effect type. Every game's status-effect
skills then differ only in `(table_path, row_fname, value_at_max)`
-- the catalog row data. We don't research the operation again,
and we don't write the apply path twice.

Same logic for Hooks: the damage-event decode + Player/Other
classification + before/after dispatch lives once in
`damage::DamageHook`. Every game's damage-driven skills plug
into it.

If you find yourself re-writing the same operation in two
places, the operation should have been an Effect. If two
games' skills differ only in parameters, the Effect type
already covers both. Only the catalog rows change.

---

# Def -> Registry -> Instance -> Controller

> **Authoritative on:** the universal layering pattern every
> ueforge subsystem follows (or should). Borrowed from
> Kubernetes Custom Resource Definitions and adapted for an
> in-process UE5 mod framework.
>
> Read this before adding a new module. New work should land
> compliant; existing drift is tracked in the compliance table.

The motivating problem: ueforge has many subjects. Skills,
buildings, status effects, debug ops, selectors, shutdown
handlers, hooks, tabs. And each one independently invented
its own "schema + storage + instance + behavior" shape. Some
landed on a clean Def + Registry pattern (skills, tabs);
others ended up as hardcoded match arms across multiple files
(debug ops, selectors). The drift makes "how do I add a new
X" answer differently for every X.

This document defines the contract once. New subjects pick
the same shape. Existing subjects get scored against it; the
gap is the cleanup punch list.

## The four roles

| Role | What it is | Example |
|---|---|---|
| **Def** (CRD) | The schema. A `struct` declaring the fields a kind has. Static, immutable, no runtime state. | `Skill<E>`, `BuildingDef<S, C>`, `OpDef`, `SelectorDef` |
| **Registry** (etcd) | The collection of Defs. A `&'static [<Subject>Def]` const + a `<subject>_def(kind)` lookup. One entry per kind. | `CATALOG: &[Skill<E>]`, planned `OP_REGISTRY` |
| **Instance** (CR) | One runtime object created from a Def. Lives in whatever storage fits (an ECS would use components; we use `Mutex<HashMap<Handle, ...>>` or per-subject state structs). | `SkillsState.skill_levels[id]`, `BuildingsTracker::instances[handle]` |
| **Controller** | The function or system that reads the Def + inputs and writes/updates the Instance. Reconcile = re-derive when inputs change. | `RpgApplier::apply_skill`, `BuildingsTracker::tick`, `place_building` |

**Key rule:** the Def is the source of truth. Controllers read
fresh values from the Def at every reconcile. Instances never
cache Def fields. If the Def changes (a new module ships
with a different base value, a hot-reload swaps catalog rows),
the next reconcile picks it up.

## The naming contract

Pick one shape for each role and stick with it across every
subject. Endless's `k8s.md` flags drift between `job` /
`activity` / `kind`; we avoid that here.

| Role | Naming |
|---|---|
| Schema struct | `<Subject>Def` (`SkillDef`, `CreatureDef`, `HookDef`, `OpDef`, `BuildingDef`, `SelectorDef`, `ShutdownHandlerDef`, `TabDef`). Always Def-suffixed. No exceptions. |
| Discriminator | `<Subject>Kind` enum where the set is closed; `&'static str` id where it's open-ended. The field that holds it is always called `kind` / `id` (NOT `name`, `job`, `activity`). |
| Registry wrapper struct | `<Subject>Registry` holding `entries: &'static [<Subject>Def]` plus any extra registry-level config (e.g. `BestiaryRegistry` carries `default_xp: u32` alongside its entries). The wrapper exists so future fields don't reshape every consumer; bare slices are reserved for the carve-out exceptions below. |
| Registry instance | `<SUBJECT>_REGISTRY: <Subject>Registry` (static const) for closed-set framework-owned registries, or per-mod `static CATALOG: <Subject>Registry<E>` when the registry is parameterized over a consumer-defined effect enum. |
| Lookup method | `<Subject>Registry::def(&self, key) -> Option<&'static <Subject>Def>` is the canonical lookup. Free `<subject>_def(...)` functions are kept as thin wrappers for back-compat where prior call sites used them. |
| Tracker (stateful subjects) | `<Subject>Tracker`. Holds the inner state, persists, reconciles. |
| Module-level controller (stateless subjects) | bare functions: `apply_skill`, `place_building`, `dispatch_op`. |

**Bare-slice exception.** Subjects with no extra registry-level
config AND no foreseeable use for one MAY skip the
`<Subject>Registry` wrapper and use a bare
`&'static [<Subject>Def]` directly. Tabs (`MOD_INFO.tabs`) is
the canonical example. The only registry-level fact is
"these are the tabs", a wrapper would be ceremony tax. Mark
the carve-out explicitly in the module's K8s slot header.

**Slice-of-refs registries.** When `<Subject>Def` carries
non-Copy state with a non-trivial `Drop` impl (mutexes,
atomics-in-caches, owned heap data), Rust's const-eval rejects
a temporary array of values in the registry literal:

```rust
// E0493: destructor of [StackDef; N] cannot be evaluated at compile-time
pub static STACKS: StackRegistry =
    StackRegistry::new(&[StackDef::new(...)]);
```

The clean pattern. Not a workaround. Is to declare each Def
as its own named `static` and store `&[&'static <Subject>Def]`
in the registry. References are `Copy` + `Drop`-free, so the
slice literal is const-evaluable:

```rust
static MATERIALS_DEF: StackDef = StackDef::new("materials", ...);

pub static STACKS: StackRegistry =
    StackRegistry::new(&[&MATERIALS_DEF]);
```

This is the canonical pattern for **all** Drop-having registries
in the workspace: `StackRegistry`, `DifficultyRegistry`,
`StatusRegistry`, `HookRegistry` (runtime-populated variant).
Each Def is a named static (better debug + introspection +
direct ref access from other modules) and the registry literal
stays inline.

Drop-free Defs (Skills, Creatures, Tabs) MAY use the simpler
`&[<Subject>Def]` (slice of values) shape. The array literal
const-evaluates fine. Either pattern is architecturally
correct; pick based on whether the Def has Drop-having state.

Header line on every subject's module doc: a single line
declaring its slot.

```
// K8s slot: Def=SkillDef, Registry=CATALOG, Instance=SkillsState.skill_levels, Controller=RpgApplier::apply_skill
```

A reviewer can grep that line across the codebase to confirm
no subject lost its place.

## What does NOT fit (and why)

Not every piece of ueforge wants to be a CRD. Forcing the
pattern where it doesn't fit adds ceremony with no payoff.

| Subject | Why it doesn't fit |
|---|---|
| **Hooks** (`ProcessEventHook`) | Partial fit. `HookDef` exists as the per-install runtime record (class_name, slot, vtable, handler, active_calls, panic_count) and `HookRegistry` wraps the global handle list with the same `register` / `shutdown_all` / `defs` / `panic_count_total` surface as other registries. The carve-out is only that handlers are closures populated imperatively. There's no compile-time `static HOOK_REGISTRY: HookRegistry = ...` const declaration the way `SkillRegistry` works. The shape is consistent; the mechanic is different. |
| **Counters** (`counter!` macro) | One static `AtomicU64` per call site. The "schema" is just `AtomicU64`; the variation is the call site itself. A registry would be a copy of the source. |
| **PE queue jobs** | Anonymous `Box<dyn FnOnce>`. Every job is unique; no shared schema. The Queue itself is the shared piece, and it already has a Def shape (the `Queue` struct + bounded depth + cancel flag). |
| **`Settings<T>`** | Generic over the consumer's struct; the consumer's struct IS the Def. `Settings` is a controller / persistence wrapper. Already correctly placed. |
| **Logger** | Process-singleton sink. No kinds. |

The carve-out is not a free pass: subjects in this list still
follow the rest of the framework's conventions (Drop ordering,
`shutdown_all` registry, etc.). They just don't have a CRD.

## Compliance scorecard

Score = how cleanly the subject splits into Def + Registry +
Instance + Controller, and whether adding a new variant is a
1-line registry append.

| Subject | Def | Registry | Instance | Controller | Score | Gap |
|---|---|---|---|---|---|---|
| **Skills** | `SkillDef { id, display_name, max_level, effect: EffectDef, trigger: &'static TriggerDef }` (`ueforge/src/rpg/skill.rs`) | `SkillRegistry` wrapper; per-mod `static CATALOG: SkillRegistry` | `SkillsState.skill_levels[id]` | `Tracker::apply_one_unlocked` (OnSlotChange) + `Tracker::fire(ctx)` (event-driven) | **100%** | A skill composes WHAT (`EffectDef`) and WHEN (`TriggerDef`). g2rpg's `SKILL_LIFESTEAL` uses `&ON_DAMAGE_DEALT`, `SKILL_IMPACT_RESISTANCE` uses `&ON_DAMAGE_TAKEN`; CDO-write skills use `&ON_SLOT_CHANGE`. |
| **Triggers** | `TriggerDef { kind, imp: &'static dyn Trigger }` (`ueforge/src/rpg/trigger.rs`) + `TriggerCtx` enum carrying typed event payloads | per-trigger static instances declared by the framework (and games for custom triggers); each trigger TYPE is its own struct + impl | one `&'static dyn Trigger` per registered trigger source; `Effect::apply` receives `&TriggerCtx` per fire | `Tracker::apply_one_unlocked` fires SlotChange on activate/spend/refund; `Tracker::fire(ctx)` fans events out to subscribed effects per kind. Framework hooks (`damage::DamageHook`, `fall::FallHook`) decode events; game-side binders call `TRACKER.fire`. | **100%** | Phase 5c (2026-05-11) shipped the full set: `ON_SLOT_CHANGE`, `ON_DAMAGE_DEALT`, `ON_DAMAGE_TAKEN`, `ON_KILL`, `ON_FALL`. `Tracker::fire` walks the catalog under one lock, snapshots subscribers to a 32-slot stack array, drops the lock, then calls `Effect::apply` outside the lock so re-entry is safe (zero heap allocs). `TriggerCtx::Tick { dt }` reserved variant has no firer yet. |
| **Effects** | `EffectDef { kind, imp: &'static dyn Effect }` (`ueforge/src/rpg/effect.rs`) | Per-mod static Effect instances; framework ships standard Effect types (PlayerFloatEffect, SubcomponentMultiplyEffect, SubcomponentAdditiveEffect, ClassFieldsMultiplyEffect, RuntimeEffect, StatusEffectApply, etc.); game-specific Effects implement `Effect` trait | one `&'static dyn Effect` per registered operation | `Effect::apply(level, max_level)` + `Effect::format(level, max_level)` | **100%** | StandardEffect enum + per-game SkillEffect enum collapsed into one Def shape. New game-specific operation = one type + one impl + one static + one catalog row. g2rpg's `apply_skill` match arm is gone; tracker iterates the catalog and calls `effect.apply(...)` directly. |
| **Creatures** | `CreatureDef { class_name, base_xp }` (`ueforge/src/rpg/xp.rs`) | `CreatureRegistry { entries: &[CreatureDef], default_xp: u32 }`; per-mod `static CREATURES: CreatureRegistry` | per-kill XP value | `CreatureRegistry::lookup` | **100%** | Lookup via `CREATURES.def(class_name)` (full row) or `CREATURES.lookup(class_name)` (XP value with default fallback). Future fields (drop tables, level, faction) extend the row. |
| **Tabs** | `TabDef { name, render }` (`ueforge/src/mod_main.rs`) | `MOD_INFO.tabs: &'static [TabDef]` (bare slice. Documented carve-out per the contract) | imgui draw call per render | `ueforge_mod_render_tab` | **100%** | Reference implementation for the bare-slice exception. Renamed to `TabDef` for the workspace Def-suffix mandate. |
| **Mod (root)** | `ModDef { name, version, log_file, console, on_unreal_init, on_shutdown, tabs }` (`ueforge/src/mod_main.rs`) | per-mod singleton `static MOD_INFO: ModDef = ...` | one ModDef per loaded DLL | `ue4ss_mod!(MOD_INFO)` macro emits the C-ABI hooks UE4SS calls | **100%** | Renamed from `ModInfo` to align with the Def-suffix mandate. The root Def for the entire mod. |
| **Buildings** (planned, not built) | `BuildingDef<S, C>` design in [todo.md](../../docs/todo.md) | `BuildingRegistry<S, C>` | `Instance<S>` per placed | `BuildingsTracker::tick` + `BuildingSpawner` trait | **designed-100%** | Naming aligned to the Def-suffix mandate. Status-effect-style: each placed instance carries per-instance state `S` while the BuildingDef captures the type-level config `C`. |
| **Data tables** | `DataTableDef { id, table_name, row_struct: RowSchema }` (`ueforge/src/data_table.rs`) | `DataTableRegistry` wrapping `&'static [&'static DataTableDef]`; per-mod `static DATA_TABLES: DataTableRegistry` | live `UDataTable` + its RowMap rows | `DataTableSnapshot::capture` + `probe::discover_data_tables` (planned) | **100% (Phase 1a)** | Def + Registry shipped. Discovery + snapshot + ImGui browser are Phase 1b-d. The leverage point: discovery walks every live UDataTable's FProperty chain, so a new game's bootstrap is "launch + curl `discover_data_tables` + pick fields". No SDK header dive per field. |
| **Statuses** | `StatusDef { id, table_finder, row_fname, value_offset, vanilla }` (`ueforge/src/rpg/status.rs`) | `StatusRegistry` wrapping `&'static [&'static StatusDef]`; per-mod `static STATUSES: StatusRegistry` | per-row mutated `Value` field in a `UDataTable`; `UStatusEffectComponent` instance on the player | `StatusEffectApply` Effect (and future `StatusEffectClear`, `StatusEffectMutate`) consuming `&'static StatusDef` | **100%** | Row identity (table + fname + value offset + vanilla cache) decoupled from the Effect operation. Multiple Effects can target the same status. Game-side data table is still the runtime authority for the row's `Type` / cooldown / etc.; StatusDef captures only the bits we mutate from Rust. |
| **Debug ops** | `OpDef { name, summary, args, handler }` (`ueforge/src/ops.rs`) | `OpRegistry` static singleton (`OP_REGISTRY`); populated at worker init via `register_builtins()` + `register_with_resolver(...)` + `rpg::ops::register(...)` + `debug::register_pe_call(...)` + per-game closures | per-call args + return JSON | `OpRegistry::dispatch(name, args)`. One call replaces three match statements | **100%** | Auto-generated `list_ops` op for client discovery. New op = one `OP_REGISTRY.register(OpDef::new(...))` line; no dispatch edits. Same closure-populated-at-runtime pattern as hooks. |
| **Selectors** | `SelectorDef { prefix, summary, resolver: fn }` (`ueforge/src/selector.rs`) | `SelectorRegistry` static singleton (`SELECTOR_REGISTRY`); populated via `selector::register_builtins()` (framework's `addr:` / `class:` / `first_class:` / `singleton:`) + per-game `register(SelectorDef { ... })` | resolved `&'static UObject` | `selector::resolve(s)` walks the registry | **100%** | Auto-generated `list_selectors` op for client discovery. New selector kind = one `SELECTOR_REGISTRY.register(...)` line; game crates extend without touching framework code. |
| **Shutdown handlers** | `ShutdownHandlerDef { name, order, run: fn() }` (`ueforge/src/shutdown.rs`) | `ShutdownRegistry` static singleton (`SHUTDOWN_REGISTRY`); framework `register_builtins()` registers hooks=100 / http=200 / settings=300 / scanner=400; game crates interleave at `50` (pre-framework) or `500+` (post-framework) | per-call invocation | `SHUTDOWN_REGISTRY::run_all` (sorts by order, runs each, logs) | **100%** | The `ueforge_mod_shutdown` macro is now: `on_shutdown` -> `register_builtins()` -> `run_all()` -> `finalize_hot_reload_swap`. New ueforge subsystem that spawns threads adds its `register_builtins()` line; game-specific cleanup goes via `SHUTDOWN_REGISTRY.register(...)` from `worker()`. |
| **Modules** (rpg / stacks / difficulty / inventory / damage / planned buildings) | nothing | implicit; each module has its own catalog + tracker + ops + tab without a shared shape |. |. | **n/a** | Possible meta-pattern: a `ModuleDef` that names which subsystems each module participates in. Defer until we feel the pain. |
| **Hooks** | `HookDef` (per-install runtime record; `class_name`, `slot`, `vtable`, `handler`, `active_calls`, `panic_count`) | `HookRegistry` static singleton (`HOOK_REGISTRY`) holding the owned `ProcessEventHook` handles + snapshot accessors | `ProcessEventHook` RAII guard | trampoline (per-fire) + Drop (uninstall + drain) + `HookRegistry::shutdown_all` (hot-reload teardown) | **90%** | Defs are populated imperatively (closures, no compile-time const). Documented exception. Naming + surface matches the other registries. |
| **Stacks** | `StackDef { id, table_name, field_offset, default_multiplier, skip_predicate, multiplier atomic, vanilla cache, counters }` (`ueforge/src/stacks.rs`) | `StackRegistry` wrapping `&'static [StackDef]`; per-mod `static STACKS: StackRegistry` | per-row vanilla cache + applied-row counters in the `StackDef` | `StackDef::apply_now` / `StackRegistry::apply_all_now` (returns `Vec<(id, Result)>` for per-table telemetry) | **100%** | Lookup `STACKS.def("materials")`. Iterate via `for d in &STACKS`. ows-tweaks declares one Def today; multi-table mods scale by adding entries. |
| **Difficulty** | `DifficultyDef { id, class_name, field_offset, multiplier_bits atomic, vanilla cache }` (`ueforge/src/difficulty.rs`) | `DifficultyRegistry` wrapping `&'static [DifficultyDef]`; per-mod `static SURVIVAL: DifficultyRegistry` | per-CDO vanilla cache in the `DifficultyDef` | `DifficultyDef::apply_to_cdos` / `apply_to_all` / `apply_with_filter`; `DifficultyRegistry::apply_all_to_cdos` | **100%** | g2rpg's `SURVIVAL` registry holds two Defs (hunger + thirst). Lookup `SURVIVAL.def("hunger")`. Skip-if-unity short-circuit preserved at the Def level. |
| **Counters** | n/a (carve-out) | per-call-site statics | `AtomicU64` value | `bump` / `time_scope` |. | Doesn't fit. |
| **PE queue jobs** | n/a (carve-out) | per-call closures | `Pending` | `Queue::drain` |. | The Queue itself fits; jobs don't. |

## Highest-leverage refactor targets

In rough priority order. Each one collapses 2-3 hardcoded
match statements into a single registry append.

1. ~~**Debug ops registry.**~~. DONE 2026-05-10. `OpDef` +
   `OpRegistry` shipped; `OP_REGISTRY` singleton; auto-generated
   `list_ops` handler. The three old dispatchers
   (`handle_builtin`, `dispatch_standard_op`, `dispatch_pe_ops`)
   are gone. Game crates collapse to one
   `OP_REGISTRY.dispatch(op, args)` call.
2. ~~**Selectors registry.**~~. DONE 2026-05-10. `SelectorDef`
   + `SelectorRegistry` shipped; `SELECTOR_REGISTRY` singleton;
   `selector::resolve(s)` is the one entry point. Auto-generated
   `list_selectors` op. `resolve_generic` is gone.
3. ~~**Shutdown handlers registry.**~~. DONE 2026-05-10.
   `ShutdownHandlerDef` + `ShutdownRegistry` shipped;
   `SHUTDOWN_REGISTRY` singleton; the macro now runs
   `register_builtins()` then `run_all()`. New ueforge
   subsystem registers itself in `register_builtins`; game
   crates interleave via `SHUTDOWN_REGISTRY.register(...)`.
4. **Buildings + status effects.** Both still in design /
   in-flight. Land them compliant from day one (no rework
   later).
5. **Skills cleanup.** Rename `CATALOG` -> `SKILL_REGISTRY`;
   add framework-level `skill_def(catalog, id)` (already
   `find_skill`. Alias and update naming). Cosmetic; cheap.
6. **`BestiaryRow` struct.** Replace the `(&str, u32)` tuple
   with a named struct so future fields don't reshape every
   consumer.

## Lift candidates: game-specific ops with universal shape

When a game crate registers a debug op into `OP_REGISTRY` with
captured-game-state, look at the **shape**:

- If the op's algorithm is **universal** to UE5 RPG / survival
  games but the **constants** (class names, field offsets,
  UFunction names) are game-specific, it belongs in the
  framework as a binding-driven module.
- Pattern: ueforge ships the op handlers; game declares a
  `<Module>Binding` struct of constants; game's `worker()` calls
  `<module>::register(&BINDING, &PE_QUEUE)`.
- Examples already in this shape: `damage::DamageBinder`,
  `inventory::ViewportBinder`.

Today's known lift candidates (see [todo.md](../../docs/todo.md)
"Lift game-specific ops to ueforge"):

| Op family | Game-side | Universal shape | Status |
|---|---|---|---|
| `simulate_add_health` / `simulate_set_current_health` | (was g2rpg) | `ueforge::rpg::health::HealthBinding { hc_class, hc_selector, current_damage_offset, max_health_offset, add_health_function, set_current_health_function }` -> `register(binding, pe_queue, hint)` | **DONE 2026-05-10** |
| `simulate_apply_damage` | g2rpg stub returns "disabled" | (gated on Wave E1 safe drain site. ApplyDamageFromInfo from a PE trampoline re-enters ProcessEvent and crashes) | open |

When the second consumer asks for the same shape, lift before
duplicating.

## Effects refactor (DONE 2026-05-10. Collapsed StandardEffect + per-game SkillEffect)

Today there are two parallel "effect" types:

1. `ueforge::rpg::StandardEffect`. Sealed framework enum, ~9
   variants of common UE5 RPG effect shapes.
2. Each game's `SkillEffect` enum -- `Standard(StandardEffect)`
   plus game-specific composites (`BackpackSlots`,
   `CustomFallReduction`, ...).
3. `SkillDef<E>` parameterized so the framework's catalog
   scaffolding doesn't see the game's enum.

Per the composition model above (Effects = the verbs), this is
two enums playing the same role + a `<E>` parameter that exists
only to hide game enums from the framework.

The refactor:

- Define `trait Effect` in `ueforge::rpg::effect`.
- Replace each `StandardEffect` variant with a struct + impl
  (`PlayerFloatEffect`, `SubcomponentMultiplyEffect`, ...).
- `EffectDef { kind: &'static str, imp: &'static dyn Effect }`.
- `SkillDef` carries `effect: EffectDef` (no `<E>`).
- `SkillRegistry` and `Tracker` lose their type parameters.
- `RpgApplier` trait collapses (its only role. Providing
  `type Effect` + dispatching match arms. Is now native to
  the trait-object).
- Game-specific operations become game-side `impl Effect for ...`
  types. Game's `SkillEffect` enum disappears.

Cost: `dyn` indirect call per apply (cold path. Invisible).
Benefit: one Def shape across framework + game; adding a new
operation is one type + one static + one catalog row. Game-side
match dispatch goes away.

Status: **shipped 2026-05-10.** All five phases landed in one
commit. Trait + per-variant struct impls + dropped `<E>` from
SkillDef/SkillRegistry/Tracker + deleted RpgApplier + g2rpg
migrated. Workspace check + 66 tests green.

What changed concretely:

- `ueforge::rpg::effect`. New module with `Effect` trait,
  `EffectDef` struct, and 8 standard Effect types
  (`PlayerFloatEffect`, `SubcomponentFloatEffect`,
  `SubcomponentAdditiveEffect`, `SubcomponentU32MaskEffect`,
  `SubcomponentMultiplyEffect`, `ClassFieldsMultiplyEffect`,
  `RuntimeEffect`, `StatusEffectApply`).
- `ueforge::rpg::std_effect` (the old enum). DELETED.
- `ueforge::rpg::applier` (the old `RpgApplier` trait). DELETED.
- `SkillDef` / `SkillRegistry` / `Tracker`. Type parameters
  dropped.
- `Tracker` now owns `DisabledSkills` internally; toggle ops
  drive it directly.
- g2rpg's `SkillEffect` enum. DELETED. One game-specific Effect
  (`BackpackSlotsEffect`) lives in
  `grounded2-mod::rpg::effects`; the catalog row references it
  the same way they reference framework Effects. The reusable
  survival-drain and fall-damage operations now live in Ueforge.
- g2rpg's `applier.rs` (the GameApplier). DELETED.
- g2rpg's `apply.rs`. The giant `apply_skill` dispatch match
  is GONE. The file now holds only shared helpers
  (capture_vanilla, class refs, vanilla caches, CDO walkers).

## How to add a new compliant subject

Checklist for the next module:

1. Define `<Subject>Def { ... }` in a single source file. All
   fields `'static`-friendly (`&'static str`, primitives,
   `fn` pointers. No `String`, no `Vec`, no closures).
2. Define `<Subject>Kind` (closed enum) OR document the id
   convention (open `&'static str` or `String`).
3. Author `<SUBJECT>_REGISTRY: &'static [<Subject>Def]`. One
   entry per kind. Compile-time const where possible.
4. Provide `<subject>_def(kind) -> &'static <Subject>Def`.
5. Decide: stateful (`<Subject>Tracker`) or stateless (free
   functions). Persist via `SlotStore<S>` if the instance
   state survives across saves.
6. Write the K8s-slot one-liner at the top of the module doc.
7. Add a row to the compliance table above.
8. If the subject spawns threads or owns external resources,
   register with the appropriate `shutdown_all` registry so
   hot-reload teardown works.

## Wave 2 durability adoptions

The kovarex review's second wave traded hand-rolled primitives
for vetted crates wherever the migration paid for itself. The
keepers:

### patternsleuth (offset resolution)

Workspace dep pinned to `trumank/patternsleuth` master @9573c52
(`MIT OR Apache-2.0`). `ueforge::ue::resolvers` wraps the three
UE resolvers used: `GUObjectArray`, `FNamePool`, `FNameToString`
(UE's `AppendString`). `resolve_image_offsets()` reads the host
image via `patternsleuth::process::internal::read_image()`, calls
`exe.resolve(UeResolution::resolver())`, returns image-relative
offsets.

Debug op `resolve_offsets` returns a side-by-side comparison
against the configured hardcoded STEAM/XBOX offsets so drift on a
future game patch is visible without rebuilding. The hand-rolled
`ue::sigscan` was deleted (466 LoC; patternsleuth ships the
equivalent plus a UE pattern library plus UE-version-aware
ranking). The `ProcessEvent` vtable index stays hardcoded: it's a
vtable slot, not an image offset, and patternsleuth doesn't ship a
resolver for it.

### zerocopy (POD-byte safety)

Workspace dep + derives on the POD types most often read from raw
bytes (`FGuid`, `FWeakObjectPtr`). `ueforge::parms::as_bytes` /
`from_bytes` are SAFE fns gated on
`T: IntoBytes + Immutable` / `T: FromBytes + KnownLayout`;
`Api::call_ufunction_typed` lost its `unsafe fn` signature.
Test-side parm structs (`AddHealthParms`, `GetValueForStatParms`)
carry the four zerocopy derives; the `unsafe { ... }` wrappers at
call sites are gone. Net: 4 fewer unsafe blocks plus compile-time
layout verification on `FGuid` / `FWeakObjectPtr` / every test
parm struct.

Sites still on raw byte reads (dynamic-offset / dynamic-type) are
candidates for later waves; see todo.md for the open list.

### Generic schema versioning (`Versioned<S>` + `SchemaMigrate`)

`rpg::store::Versioned<S>` is the envelope:
`{schema_version, payload}`. `SchemaMigrate` trait holds a
`CURRENT_VERSION` const and a `migrate(from, raw)` fn.

```rust
pub fn load_versioned_with_migrate<S: SchemaMigrate>(path: &Path)
    -> S;
```

Tries the envelope first, falls back to legacy non-enveloped
state (so existing g2rpg `SkillsState` files still load when the
type eventually adopts the envelope), then falls back to
`S::default()` rather than panicking on corrupted data.
`save_versioned(store, slot, payload)` routes through
`SlotStore::save`'s atomic-write semantics. Existing `SkillsState`
is untouched; new consumers adopt the envelope from day one.

Six unit tests cover round-trip / migrate / failed-migrate
fallback / legacy-non-enveloped / missing-file / save-on-disk
shape.

### SAFETY-comment grind status

Down from 271 to 50 unsafe-block warnings across the wave-2
sweep. Files fully annotated with per-block `// SAFETY:` comments:
`ue/tmap.rs`, `ue/tarray.rs`, `ue/sigscan.rs` (with one fn-level
allow on `text_section`'s PE walk), `damage/mod.rs`,
`inventory/viewport.rs`, `hook/process_event.rs`, `data_table.rs`,
`ops.rs`, `ue/datatable.rs`, `log.rs`, `scanner.rs`.

Files with a module-level
`#![allow(clippy::undocumented_unsafe_blocks)]` justified by a
universal-contract module doc: `ui.rs` (FFI uniform shape),
`ue/field.rs` (untyped `read_*` / `write_*` on `obj.field_ptr`),
`ue/class_ref.rs` (`GObjectsView::from_image` + `&'static` lifetime
extension), `ue/uobject.rs` (the four primitive shapes),
`winproc.rs` (windows-sys FFI uniform shape: Toolhelp32 /
VirtualQuery / Thread sampling).

The remaining 50 are spread thinly across `ue/probe.rs`,
`ue/status_effect.rs`, `ue/class_tweak.rs`, `ue/core_types.rs`,
`ue/typed_field.rs`, `ue/fname.rs`, `ue/platform.rs`,
`ue/player.rs`, `discovery.rs`, `selector.rs`, `damage_info.rs`,
`pe_call.rs`, `parms.rs`, `fstring.rs`, and a handful of
single-block files. Chip away during normal edits; the lint can
go to `deny` when the count hits zero.

### Smaller grooming wins

These shipped alongside the durability work:

- **`hex` crate**. `ueforge/src/hex.rs` deleted; `hex = "0.4"`
  added to workspace deps; consumers map the new `FromHexError`
  to `String` via `.map_err`. ~30 LoC saved.
- **Boundary tests on walkers**. `tarray.rs` gets 5 unit tests
  (empty default, negative num, null data, slice round-trip,
  `repr(C)` layout). `tmap.rs` gets 7 (empty map, single slot,
  find-by-key hit / miss / null-value, null data ptr, negative
  num). Engine-supplied garbage states all short-circuit cleanly.
- **`ue/offsets.rs` regeneration path**. The header now
  documents how to regenerate offsets (run Dumper-7, read
  `Basic.hpp` + UE4SS `Signatures.cpp`) rather than referencing
  an author-machine path. Vendoring the actual `Basic.hpp` is
  deferred since it's a per-game artifact tied to Grounded 2's
  specific UE 5.4 build, not a framework asset.

### Crate-shopping verdicts (don't re-investigate)

After patternsleuth + zerocopy + the four ranked-open adoptions
(see todo.md), the remaining crate options were evaluated and are
not worth doing:

- **`tracing`**. Huge migration touching every `log!` site for
  per-call timing + structured fields. Current custom logger is
  fine.
- **`dashmap`**. Marginal until profiling shows real contention
  on the discovery cache or tweak registries (both write-rare).
- **`bytemuck`**. Covered by `zerocopy`.
- **`once_cell`**. `std::sync::OnceLock` is stable.
- **`windows` crate** (vs `windows-sys`). Touches too much for a
  documentation-style win.
- **`cxx` / `bindgen`**. Overkill for 4 UE4SS symbols + a handful
  of `windows-sys` items per file.
- **`object` / `goblin`**. PE parser. Was useful when we
  hand-rolled `text_section`; patternsleuth absorbed that.
- **`tokio` / `rayon`**. No async runtimes or parallelism needed.
  patternsleuth's internal futures are driven sync.
- **`serde_with`**. Marginal; op JSON shapes are simple.
- **`retour-rs` / `detour-rs`**. Inline-detour libraries.
  ueforge uses vtable patching (`hook::vtable::write_slot`); the
  detour crates would compete with the existing hook framework
  rather than complement it.
- **`iced-x86`**. Auto-compute `disp_offset` / `next_instr_offset`
  from the matched opcode. Marginal: patternsleuth handles
  RIP-relative resolution.
- **Phantom types on `ProcessEventHook<Installed>` /
  `<Uninstalled>`**. Type-state pattern. Marginal because
  `OnceLock` + Drop already enforce install-once.
- **`NonNull<UObject>`**. `&UObject` (non-null by Rust rules)
  covers 95% of the code; the remaining 5% are tagged with their
  own null-check path. Cosmetic change.
- **`pdb` crate**. Only works on debug builds; shipping UE5 games
  strip symbols.
- **Continue grinding SAFETY comments to zero**. Documentation
  discipline, not real safety. The runtime defensive primitives
  (`is_addr_readable`, `catch_unwind` per trampoline, SEH wraps on
  `AppendString`, the DRAINING guard, `ReadProcessMemory`
  fault-safe reads) are what actually prevent crashes.

## Cross-references

- [README.md](README.md). Module index
- [hooks.md](hooks.md). The hook subsystem (carve-out
  example)
- [pe-queue.md](pe-queue.md). The PE queue (carve-out
  example for jobs; the Queue itself is shape-compliant)
- [rpg.md](rpg.md). Skills (95% compliant; the closest to
  the reference shape)
- [lifecycle.md](lifecycle.md). Shutdown sequence (0%
  compliant today; refactor target)
- [settings.md](settings.md). Generic-over-consumer pattern
  (shape-compliant via composition)
- [`endless/docs/k8s.md`](https://github.com/abix-/endless/blob/dev/docs/k8s.md)
 . The source pattern
