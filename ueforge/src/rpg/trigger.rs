//! UE-specific Engine + Event enum + concrete event-driven
//! triggers.
//!
//! The trait shapes (`Trigger`, `TriggerDef`, `TriggerCtx<E>`,
//! `OnSlotChange*`) live in [`modforge::rpg::trigger`]. This
//! file plugs in the UE engine:
//!
//! - [`UeEngine`]: the zero-sized type carrying the
//!   `Event<'a> = UeEvent<'a>` association.
//! - [`UeEvent`]: enum of UE event payloads (damage, kill, fall).
//! - [`TriggerCtx`]: type alias for
//!   `modforge::rpg::TriggerCtx<'_, UeEngine>`.
//! - [`OnDamageDealtTrigger`], [`OnDamageTakenTrigger`],
//!   [`OnKillTrigger`], [`OnFallTrigger`] + their static
//!   `TriggerDef`s.
//!
//! `OnSlotChange` is engine-agnostic; it's defined in modforge
//! and re-exported via [`crate::rpg::ON_SLOT_CHANGE`].

pub use modforge::rpg::trigger::{ON_SLOT_CHANGE, OnSlotChangeTrigger, Trigger, TriggerDef};

use modforge::rpg::Engine;

/// UE engine plug-in. Zero-sized; only carries the
/// `Event<'a>` association.
pub struct UeEngine;

impl Engine for UeEngine {
    type Event<'a> = UeEvent<'a>;

    fn event_kind<'a>(event: &Self::Event<'a>) -> &'static str {
        match event {
            UeEvent::DamageDealt(_) => "OnDamageDealt",
            UeEvent::DamageTaken(_) => "OnDamageTaken",
            UeEvent::Kill(_) => "OnKill",
            UeEvent::Fall(_) => "OnFall",
        }
    }
}

/// UE event payload. Wraps borrowed references to the engine
/// event structs so effects can read damage / kill / fall data
/// without copying.
pub enum UeEvent<'a> {
    /// Fired by [`OnDamageDealtTrigger`] per player-instigator
    /// damage hit, BEFORE the engine applies the damage.
    DamageDealt(&'a crate::damage::DamageEvent<'a>),
    /// Fired by [`OnDamageTakenTrigger`] per player-target
    /// damage hit, AFTER the engine applies the damage.
    DamageTaken(&'a crate::damage::DamageEvent<'a>),
    /// Fired by [`OnKillTrigger`] per confirmed creature kill
    /// (killing-blow flag set).
    Kill(&'a KillEvent<'a>),
    /// Fired by [`OnFallTrigger`] when the player's `OnLanded`
    /// UFunction fires.
    Fall(&'a FallEvent<'a>),
}

/// Type alias for ergonomic ueforge call sites. The full type
/// is [`modforge::rpg::TriggerCtx`] specialized to [`UeEngine`].
pub type TriggerCtx<'a> = modforge::rpg::TriggerCtx<'a, UeEngine>;

/// Confirmed kill event. The killing-blow flag has been checked
/// upstream so subscribers fire unconditionally.
pub struct KillEvent<'a> {
    /// The actor that died (typically the victim component's
    /// outer).
    pub victim: &'a crate::ue::UObject,
    /// Resolved name of `victim`'s UClass, pre-allocated by the
    /// trigger dispatcher so subscribers don't pay the FName
    /// lookup per fire.
    pub victim_class_name: &'a str,
    /// Damage instigator resolved from `FDamageInfo`. None if
    /// the weak pointer was unset.
    pub attacker: Option<&'static crate::ue::UObject>,
    /// Convenience: `attacker.class_chain_contains(player_controller_filter)`.
    pub attacker_is_player: bool,
    /// Damage value of the killing blow (after engine mutation,
    /// if any).
    pub damage: f32,
}

/// Player-fall landing event. Fires from inside the OnLanded
/// trampoline BEFORE the engine reads the live Velocity.Z, so
/// effects that mutate `Velocity.Z` (Fall Resistance) and
/// effects that subtract post-impact damage (Impact Resistance)
/// both work off the same fire.
pub struct FallEvent<'a> {
    /// The player pawn whose `OnLanded` fired.
    pub player: &'a crate::ue::UObject,
    /// Resolved `CharacterMovementComponent` (the pointer at
    /// `config.char_movement_component_offset` on `player`).
    /// `None` if the pointer was null. Effects writing
    /// `Velocity.Z` use this directly to avoid re-resolving the
    /// offset; the framework already paid the deref cost.
    pub cmc: Option<&'a crate::ue::UObject>,
    /// Live `Velocity.Z` snapshot taken from `cmc` just before
    /// the engine processes the landing. Zero when `cmc` is
    /// `None`.
    pub velocity_z_before: f64,
}

// =====================================================================
// UE event-driven triggers.
// =====================================================================

/// Fires per player-instigator damage hit, BEFORE the engine
/// applies the damage. Used by Lifesteal, Critical, etc. The
/// underlying hook is `ueforge::damage::DamageHook`; the game
/// crate installs the hook and the framework dispatcher fans
/// out `TriggerCtx::Engine(UeEvent::DamageDealt(&event))` to
/// subscribed skills.
pub struct OnDamageDealtTrigger;
impl Trigger for OnDamageDealtTrigger {
    fn kind(&self) -> &'static str {
        "OnDamageDealt"
    }
}
static ON_DAMAGE_DEALT_IMP: OnDamageDealtTrigger = OnDamageDealtTrigger;
pub static ON_DAMAGE_DEALT: TriggerDef = TriggerDef::new("OnDamageDealt", &ON_DAMAGE_DEALT_IMP);

/// Fires per player-target damage hit, AFTER the engine
/// applies the damage. Used by Evasion (pre-mutate via binder)
/// and Thorns (post-react). Same underlying hook as
/// [`OnDamageDealtTrigger`].
pub struct OnDamageTakenTrigger;
impl Trigger for OnDamageTakenTrigger {
    fn kind(&self) -> &'static str {
        "OnDamageTaken"
    }
}
static ON_DAMAGE_TAKEN_IMP: OnDamageTakenTrigger = OnDamageTakenTrigger;
pub static ON_DAMAGE_TAKEN: TriggerDef = TriggerDef::new("OnDamageTaken", &ON_DAMAGE_TAKEN_IMP);

/// Fires per confirmed creature kill (killing-blow flag set on
/// the damage multicast). Used by kill-credit driven skills.
/// Reuses the [`OnDamageDealtTrigger`] hook plumbing; the
/// dispatcher classifies + fires
/// `TriggerCtx::Engine(UeEvent::Kill(&KillEvent))`.
pub struct OnKillTrigger;
impl Trigger for OnKillTrigger {
    fn kind(&self) -> &'static str {
        "OnKill"
    }
}
static ON_KILL_IMP: OnKillTrigger = OnKillTrigger;
pub static ON_KILL: TriggerDef = TriggerDef::new("OnKill", &ON_KILL_IMP);

/// Fires when the player's `OnLanded` UFunction fires. Used by
/// Fall Resistance + Impact Resistance.
pub struct OnFallTrigger;
impl Trigger for OnFallTrigger {
    fn kind(&self) -> &'static str {
        "OnFall"
    }
}
static ON_FALL_IMP: OnFallTrigger = OnFallTrigger;
pub static ON_FALL: TriggerDef = TriggerDef::new("OnFall", &ON_FALL_IMP);
