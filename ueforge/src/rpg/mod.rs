//! Generic RPG / level-up framework. Game crates supply their own
//! `SkillEffect` enum + catalog + apply-dispatcher; ueforge ships
//! the universally-shaped pieces:
//!
//! - [`xp::Curve`]. Cumulative XP and `level_for_xp` math driven by
//!   `(base, exponent, max_level)`.
//! - [`progress::sqrt_progress`]. Diminishing-returns helper.
//! - [`SkillsState`]. On-disk schema (xp / level / skill_points /
//!   `skill_levels: BTreeMap<String, u32>`). Open-shape so games can
//!   add skills without breaking existing save files.
//! - [`SlotStore`]. Per-slot JSON persistence under
//!   `<DLL_dir>/<subdir>/<slot>.json`, atomic save (temp + rename).
//! - [`DisabledSkills`]. Thread-safe "disable a skill without
//!   refunding its points" toggle set.
//! - [`SlotPoller`]. 1Hz worker that watches a game-specific
//!   slot-key resolver closure and drives activate / deactivate
//!   transitions on the consumer's behalf.
//!
//! Game crates wire those into their own SkillEffect dispatcher and
//! ImGui tab. The `grounded2-mod` crate is the reference consumer.
//!
//! Persistence shape is intentionally generic over `S: Serialize +
//! DeserializeOwned + Default`: g2rpg uses an extended struct with
//! diagnostic counters (`kill_count`, `last_killed`) but other games
//! may attach completely different fields. ueforge owns the bones,
//! not the meat.

pub mod disabled;
pub mod effect;
pub mod format;
pub mod health;
pub mod ops;
pub mod poller;
pub mod progress;
pub mod skill;
pub mod slot_key;
pub mod state;
pub mod status;
pub mod store;
pub mod tab;
pub mod tracker;
pub mod trigger;
pub mod vanilla;
pub mod xp;

pub use disabled::DisabledSkills;
pub use effect::{
    ClassFieldsMultiplyEffect, Effect, EffectDef, ImpactReversalEffect, LifestealEffect,
    PlayerFloatEffect, RuntimeEffect, StatusEffectApply, SubcomponentAdditiveEffect,
    SubcomponentFloatEffect, SubcomponentMultiplyEffect, SubcomponentU32MaskEffect,
};
pub use format::PercentFormat;
pub use poller::{PollerHandle, SlotPoller, spawn_interval};
pub use skill::{SkillDef, SkillRegistry};
pub use slot_key::SlotKeyResolver;
pub use state::SkillsState;
pub use status::{StatusDef, StatusRegistry};
pub use store::SlotStore;
pub use tracker::{Tracker, XpResult};
pub use trigger::{
    FallEvent, KillEvent, OnDamageDealtTrigger, OnDamageTakenTrigger, OnFallTrigger,
    OnKillTrigger, OnSlotChangeTrigger, Trigger, TriggerCtx, TriggerDef, UeEngine,
    UeEvent, ON_DAMAGE_DEALT, ON_DAMAGE_TAKEN, ON_FALL, ON_KILL, ON_SLOT_CHANGE,
};
pub use vanilla::VanillaCache;
pub use xp::{CreatureDef, CreatureRegistry, Curve};
