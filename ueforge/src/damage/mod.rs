//! Generic damage-event hook framework. One of ueforge's
//! opinionated UE5 mod modules (rpg / stacks / difficulty /
//! inventory / damage).
//!
//! Every UE5 game with combat has a `UHealthComponent`-shaped
//! class that fires a multicast / RPC for every damage event.
//! That fire is the one universal seam where mods can:
//!
//! - **Observe** (kill credit, damage tracing, telemetry)
//! - **Mutate pre-application** (Critical, Evasion, damage caps)
//! - **React post-application** (Lifesteal, Thorns, status effects)
//!
//! The class names + UFunction names + parm offsets vary per game.
//! The pattern doesn't. ueforge owns the trampoline + parm decoding
//! + classification + before/after dispatch; the game crate provides
//! a [`DamageHookConfig`] of names/offsets + a [`DamageBinder`]
//! impl that decides what to do with each decoded [`DamageEvent`].
//!
//! ```ignore
//! use ueforge::damage::{DamageHook, DamageHookConfig, DamageBinder, DamageEvent};
//! use ueforge::ue::damage_info::DamageInfoLayout;
//!
//! const G2_DAMAGE_INFO: DamageInfoLayout = DamageInfoLayout {
//!     last_damage_info_offset: 0x03B0,
//!     instigator_offset:       0x03B0 + 0x20,
//!     damage_source_offset:    0x03B0 + 0x28,
//!     damage_type_class_offset:0x03B0 + 0x40,
//!     damage_flags_offset:     0x03B0 + 0x70,
//! };
//!
//! const CONFIG: DamageHookConfig = DamageHookConfig {
//!     component_class: "HealthComponent",
//!     damage_fn: "MulticastHandleEffectsWithDamageFlags",
//!     damage_parm_offset: 0x18,
//!     damage_flags_parm_offset: 0x1C,
//!     type_flags_parm_offset: 0x20,
//!     killing_blow_mask: 2, // EDamageInfoFlags::KillingBlow
//!     damage_info: G2_DAMAGE_INFO,
//!     player_outer_filter: "BP_SurvivalPlayerCharacter",
//!     player_controller_filter: "PlayerController",
//! };
//!
//! struct G2Binder { /* tracker, catalog, etc */ }
//!
//! impl DamageBinder for G2Binder {
//!     fn before(&self, event: &DamageEvent) -> Option<f32> {
//!         // Crit roll on player-dealt; Evasion on player-taken.
//!         if event.attacker_is_player && !event.victim_is_player {
//!             // ... critical
//!         }
//!         None
//!     }
//!     fn after(&self, event: &DamageEvent) {
//!         // Kill credit + lifesteal + thorns + damage trace.
//!     }
//! }
//!
//! static HOOK: DamageHook<G2Binder> = DamageHook::new(CONFIG);
//!
//! // From `on_unreal_init`:
//! let hook = HOOK.install(G2Binder::new()?)?;
//! ueforge::hook::register(hook);
//! ```
//!
//! ### When to mutate damage
//!
//! Returning `Some(new_damage)` from `before` writes the new value
//! to the parm struct **before** the engine's original ProcessEvent
//! runs, so the engine's own application path picks it up. This is
//! the right surface for `Critical` (multiply) and `Evasion`
//! (zero on roll). Post-application mutations (e.g. Lifesteal
//! decrementing player HP) belong in `after`. Mutate game state
//! directly there.
//!
//! ### Drain integration
//!
//! Many mods drain a game-thread PE queue from a damage trampoline
//! (the canonical "high-rate game-thread surface"). Either call
//! `Queue::drain` from `before` (it runs every fire) or wire your
//! own per-event hook. The framework doesn't impose either.

use std::ffi::c_void;
use std::sync::OnceLock;

use crate::hook::ProcessEventHook;
use crate::ue::actor::{class_chain_contains, is_outer_named};
use crate::ue::damage_info::DamageInfoLayout;
use crate::ue::{ClassRef, UFunction, UObject};

/// Per-game configuration for a damage hook. All `&'static str` so
/// callers declare it as a `const`.
#[derive(Debug, Clone, Copy)]
pub struct DamageHookConfig {
    /// Class to hook ProcessEvent on (typically the
    /// HealthComponent or equivalent).
    pub component_class: &'static str,
    /// UFunction whose firing represents one damage event. The
    /// hook only invokes the binder for THIS function; other PE
    /// fires on the same class are forwarded straight to the
    /// engine.
    pub damage_fn: &'static str,
    /// Byte offset of the `Damage: f32` parm within the damage
    /// UFunction's parms struct.
    pub damage_parm_offset: usize,
    /// Byte offset of the `DamageFlags: i32` parm.
    pub damage_flags_parm_offset: usize,
    /// Byte offset of the `TypeFlags: u32` parm (0 for fns that
    /// don't carry one).
    pub type_flags_parm_offset: usize,
    /// Bit-mask in DamageFlags that marks a killing blow (G2:
    /// `EDamageInfoFlags::KillingBlow = 2`).
    pub killing_blow_mask: i32,
    /// Per-game `FDamageInfo` layout used to resolve the
    /// instigator from `LastDamageInfo`.
    pub damage_info: DamageInfoLayout,
    /// Substring matched against the victim component's outer's
    /// full name to detect "the victim is the local player"
    /// (G2: `"BP_SurvivalPlayerCharacter"`).
    pub player_outer_filter: &'static str,
    /// Substring matched against the instigator controller's
    /// class chain to detect "the attacker is a player"
    /// (G2: `"PlayerController"`). Walks the UClass +
    /// super_class chain via [`class_chain_contains`].
    pub player_controller_filter: &'static str,
}

/// One damage event decoded by the framework. Passed to
/// [`DamageBinder::before`] and [`DamageBinder::after`].
pub struct DamageEvent<'a> {
    /// The component receiving the damage UFunction call (the HC,
    /// typically). Use `victim.outer()` to get the dying actor.
    pub victim_component: &'a UObject,
    /// The actor outer the component lives on, if resolvable.
    pub victim: Option<&'a UObject>,
    /// `true` if `victim`'s outer's full name contains
    /// `config.player_outer_filter`.
    pub victim_is_player: bool,
    /// The damage instigator (typically a controller) resolved
    /// from `FDamageInfo.InstigatorController`. None if the weak
    /// pointer is unset.
    pub attacker: Option<&'static UObject>,
    /// `true` if `attacker`'s class chain contains
    /// `config.player_controller_filter`.
    pub attacker_is_player: bool,
    /// Damage value as read from the parm struct (or
    /// engine-mutated value on `after`).
    pub damage: f32,
    /// Raw DamageFlags from the parm struct.
    pub damage_flags: i32,
    /// Raw TypeFlags from the parm struct.
    pub type_flags: u32,
    /// `(damage_flags & config.killing_blow_mask) != 0`.
    pub is_killing_blow: bool,
}

/// Game-side knowledge the damage hook needs from the game crate.
/// Default impls let games opt out of either side.
pub trait DamageBinder: Send + Sync + 'static {
    /// Called BEFORE the engine's original ProcessEvent runs.
    /// Return `Some(new_damage)` to override the damage value the
    /// engine applies. Use for Critical (multiply on roll),
    /// Evasion (zero on roll), damage caps, etc.
    fn before(&self, _event: &DamageEvent) -> Option<f32> {
        None
    }

    /// Called AFTER the engine's original ProcessEvent runs. Game
    /// state was just mutated by the engine. Use for Lifesteal
    /// (decrement player HC.CurrentDamage), Thorns (apply
    /// reflected damage to attacker), kill credit, telemetry,
    /// post-application reversal (env-damage subtraction), etc.
    fn after(&self, _event: &DamageEvent) {}
}

/// Static damage-event hook. Construct with [`DamageHook::new`];
/// install once at module init with [`install`](Self::install).
pub struct DamageHook<B: DamageBinder> {
    config: DamageHookConfig,
    state: OnceLock<HookState<B>>,
}

struct HookState<B> {
    binder: B,
    damage_fn_ptr: usize,
}

// SAFETY: HookState holds a B (DamageBinder is Send + Sync by
// trait bound) and a usize-cached UFunction pointer. Nothing
// in HookState is !Send or !Sync.
unsafe impl<B: DamageBinder> Send for HookState<B> {}
// SAFETY: see Send impl. The cached pointer is read-only after
// install; concurrent reads from the trampoline are safe.
unsafe impl<B: DamageBinder> Sync for HookState<B> {}

impl<B: DamageBinder> DamageHook<B> {
    pub const fn new(config: DamageHookConfig) -> Self {
        Self {
            config,
            state: OnceLock::new(),
        }
    }

    pub fn config(&self) -> &DamageHookConfig {
        &self.config
    }

    /// Resolve the damage UFunction handle, stash the binder, and
    /// install a `ProcessEventHook` on the configured class. The
    /// returned handle is what the caller registers via
    /// [`crate::hook::register`] for hot-reload safety.
    pub fn install(&'static self, binder: B) -> Result<ProcessEventHook, &'static str> {
        let component_class = ClassRef::new_dynamic(self.config.component_class)
            .get()
            .ok_or("DamageHook: component class not loaded")?;
        let damage_fn = component_class
            .get_function(self.config.component_class, self.config.damage_fn)
            .ok_or("DamageHook: damage UFunction not found")?;
        let damage_fn_ptr = damage_fn as *const UFunction as usize;

        if self
            .state
            .set(HookState {
                binder,
                damage_fn_ptr,
            })
            .is_err()
        {
            return Err("DamageHook: install called twice");
        }

        crate::log!(
            "damage: hooking {} ({} at {:p})",
            self.config.component_class,
            self.config.damage_fn,
            damage_fn
        );
        ProcessEventHook::install(
            self.config.component_class,
            move |this, function, parms, original| {
                on_event(self, this, function, parms, original);
            },
        )
    }
}

fn on_event<B: DamageBinder>(
    hook: &DamageHook<B>,
    this: &UObject,
    function: &UFunction,
    parms: *mut c_void,
    original: crate::hook::OriginalProcessEvent,
) {
    let Some(state) = hook.state.get() else {
        // SAFETY: `original` was captured from the engine vtable at
        // install time; the trampoline contract guarantees this /
        // function / parms are the engine-supplied call args. We
        // pass through unchanged when the hook state isn't ready.
        unsafe { original.call(this, function, parms) };
        return;
    };

    let fn_ptr = function as *const UFunction as usize;
    if fn_ptr != state.damage_fn_ptr || parms.is_null() {
        // SAFETY: see above; identity-cached fast path for
        // not-our-function fires + null-parms defense.
        unsafe { original.call(this, function, parms) };
        return;
    }

    // Decode parms. SAFETY for each parm read: `parms` points at
    // the UFunction's stack-allocated parm block; the offsets in
    // `cfg` are the configured byte offsets of the matching parm
    // within that block. `read_unaligned` is robust against any
    // odd alignment of the (Hit, Damage, Flags) parm tuple. The
    // parms layout is checked once via discovery against the
    // FProperty chain.
    let cfg = &hook.config;
    // SAFETY: see decode comment above; offset stays within the
    // engine-provided parm block.
    let mut damage: f32 = unsafe {
        (parms as *const u8)
            .add(cfg.damage_parm_offset)
            .cast::<f32>()
            .read_unaligned()
    };
    // SAFETY: see decode comment above.
    let damage_flags: i32 = unsafe {
        (parms as *const u8)
            .add(cfg.damage_flags_parm_offset)
            .cast::<i32>()
            .read_unaligned()
    };
    let type_flags: u32 = if cfg.type_flags_parm_offset == 0 && cfg.damage_flags_parm_offset == 0 {
        0
    } else {
        // SAFETY: see decode comment above.
        unsafe {
            (parms as *const u8)
                .add(cfg.type_flags_parm_offset)
                .cast::<u32>()
                .read_unaligned()
        }
    };

    let is_killing_blow = (damage_flags & cfg.killing_blow_mask) != 0;
    let victim = this.outer();
    let victim_is_player = is_outer_named(this, cfg.player_outer_filter);
    let attacker = cfg.damage_info.instigator(this);
    let attacker_is_player = attacker
        .map(|a| class_chain_contains(a, cfg.player_controller_filter))
        .unwrap_or(false);

    let mut event = DamageEvent {
        victim_component: this,
        victim,
        victim_is_player,
        attacker,
        attacker_is_player,
        damage,
        damage_flags,
        type_flags,
        is_killing_blow,
    };

    // Pre-call: binder may override damage.
    if let Some(new_damage) = state.binder.before(&event) {
        if new_damage != damage {
            // SAFETY: same parm-block + offset as the earlier
            // read; we are still inside the trampoline before
            // the engine reads the value, so writing here lands
            // before the engine's native damage application.
            unsafe {
                (parms as *mut u8)
                    .add(cfg.damage_parm_offset)
                    .cast::<f32>()
                    .write_unaligned(new_damage);
            }
            damage = new_damage;
            event.damage = new_damage;
        }
    }

    // SAFETY: forward to the captured engine ProcessEvent. The
    // parm block may have been mutated by the binder above; the
    // engine reads the new value as part of its native path.
    unsafe { original.call(this, function, parms) };

    // Post-call: re-read damage in case the engine wrote OUT, then
    // notify the binder.
    // SAFETY: same offset as the original decode; the parm block
    // is still mapped (the engine returns out of ProcessEvent
    // without freeing its caller's stack frame).
    let post_damage: f32 = unsafe {
        (parms as *const u8)
            .add(cfg.damage_parm_offset)
            .cast::<f32>()
            .read_unaligned()
    };
    if post_damage != damage {
        event.damage = post_damage;
    }
    state.binder.after(&event);
}
