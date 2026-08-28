//! Engine-generic trigger context + marker trait + standard
//! engine-agnostic triggers.
//!
//! A skill subscribes to a [`TriggerDef`] (semantic event source)
//! and exposes an [`crate::rpg::Effect`] (the work to do). The
//! framework dispatches by `kind` string so each engine plugs in
//! its own event payload via [`crate::rpg::Engine::Event`].
//!
//! Standard kinds shipped by modforge:
//!
//! - `"OnSlotChange"` ([`OnSlotChangeTrigger`]). Fired by
//!   `Tracker` on slot activate / spend / refund / toggle.
//! - `"Tick"` (no shipped Trigger struct; the
//!   [`TriggerCtx::Tick`] variant fires from a periodic poller
//!   when one consumer needs it).
//!
//! Engine-specific triggers (OnHarmonyPatch, OnKill, OnFall,
//! OnDamageDealt) live in the engine crate and dispatch through
//! `Tracker::fire` with an `E::Event<'_>` payload.

use std::time::Duration;

use crate::rpg::engine::Engine;

/// Typed event context passed to [`crate::rpg::Effect::apply`].
///
/// `SlotChange` and `Tick` are engine-agnostic. `Engine` wraps
/// the engine's own event enum (see [`Engine::Event`]).
pub enum TriggerCtx<'a, E: Engine> {
    /// Fired by [`crate::rpg::Tracker`] on slot
    /// activate / spend / refund / toggle. Field-writing effects
    /// use this; the level has changed, write the new value.
    SlotChange,
    /// Periodic poller tick. Reserved; no in-tree consumer yet.
    Tick { dt: Duration },
    /// Engine-specific event. The engine crate's `Event<'a>` is
    /// the concrete payload; the effect downcasts via pattern
    /// match.
    Engine(E::Event<'a>),
}

impl<'a, E: Engine> TriggerCtx<'a, E> {
    /// Kind string for catalog-row matching. Generic over the
    /// engine because `Engine::event_kind` reports per-payload
    /// names.
    pub fn kind(&self) -> &'static str {
        match self {
            TriggerCtx::SlotChange => "OnSlotChange",
            TriggerCtx::Tick { .. } => "Tick",
            TriggerCtx::Engine(e) => E::event_kind(e),
        }
    }
}

/// Marker trait + kind tag for client discovery.
pub trait Trigger: Send + Sync + 'static {
    fn kind(&self) -> &'static str;
}

/// Catalog-row reference to a [`Trigger`] implementation.
pub struct TriggerDef {
    pub kind: &'static str,
    pub imp: &'static dyn Trigger,
}

impl TriggerDef {
    pub const fn new(kind: &'static str, imp: &'static dyn Trigger) -> Self {
        Self { kind, imp }
    }
}

// =====================================================================
// Standard engine-agnostic triggers.
// =====================================================================

/// Fires when the player's skill state changes (slot activate,
/// spend, refund, toggle). Used by every field-writing skill.
/// No underlying engine hook; the Tracker drives dispatch.
pub struct OnSlotChangeTrigger;

impl Trigger for OnSlotChangeTrigger {
    fn kind(&self) -> &'static str {
        "OnSlotChange"
    }
}

static ON_SLOT_CHANGE_IMP: OnSlotChangeTrigger = OnSlotChangeTrigger;

pub static ON_SLOT_CHANGE: TriggerDef = TriggerDef::new("OnSlotChange", &ON_SLOT_CHANGE_IMP);
