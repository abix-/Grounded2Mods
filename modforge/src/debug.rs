//! Engine-agnostic debug-endpoint scaffolding.
//!
//! Half of `ueforge::debug` lifted here during Phase 0b row 19.
//! The pieces that don't touch UObject / pe_queue:
//!
//! - [`PlayerStateView`]: serde-friendly mapping of
//!   [`crate::rpg::SkillsState`].
//! - [`CatalogEntry`] + [`catalog_view`]: per-row catalog view
//!   for `list_ops`-style introspection. Generic over the
//!   engine type so both frameworks reuse it.
//!
//! Engine-specific debug helpers (ProcessSnapshot using UE
//! probes, DamageRing, enqueue_pe / dispatch_call on
//! GameThread) stay in `ueforge::debug` because their payloads
//! and dispatch surfaces are UE-shaped.

use serde::Serialize;

use crate::rpg::engine::Engine;
use crate::rpg::skill::SkillDef;
use crate::rpg::state::SkillsState;

/// Universal player-state view for the debug snapshot.
/// `skill_levels` is a JSON object so per-skill-id lookups in
/// tests don't require a linear scan.
#[derive(Serialize)]
pub struct PlayerStateView {
    pub xp: u64,
    pub level: u32,
    pub skill_points: u32,
    pub skill_levels: serde_json::Map<String, serde_json::Value>,
}

impl PlayerStateView {
    /// Build a view from a [`SkillsState`]. Allocates one
    /// `serde_json::Map` for the skill-levels field.
    pub fn from_state(s: &SkillsState) -> Self {
        let mut skill_levels = serde_json::Map::new();
        for (id, lv) in &s.skill_levels {
            skill_levels.insert(id.clone(), serde_json::Value::Number((*lv).into()));
        }
        Self {
            xp: s.xp,
            level: s.level,
            skill_points: s.skill_points,
            skill_levels,
        }
    }
}

/// One row in a serializable catalog view. `effect_kind` is the
/// stable name of the operation
/// ([`crate::rpg::EffectDef::kind`]) so test assertions key off
/// catalog rows without coupling to the live struct layout.
#[derive(Serialize)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub max_level: u32,
    pub effect_kind: &'static str,
}

/// Map a `[SkillDef<E>]` slice into a serializable
/// `Vec<CatalogEntry>`. Generic over the engine type.
///
/// ```ignore
/// let view = modforge::debug::catalog_view(CATALOG.entries());
/// ```
pub fn catalog_view<E: Engine>(catalog: &'static [SkillDef<E>]) -> Vec<CatalogEntry> {
    catalog
        .iter()
        .map(|s| CatalogEntry {
            id: s.id,
            display_name: s.display_name,
            max_level: s.max_level,
            effect_kind: s.effect.kind,
        })
        .collect()
}
