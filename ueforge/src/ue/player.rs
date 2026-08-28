//! Canonical "find the player" surface for UE5 mods.
//!
//! Every UE5 RPG / survival / tweak mod needs the same handful of
//! operations:
//!   - "give me every CDO of the player class" (apply CDO writes)
//!   - "give me every live player pawn" (apply live-instance writes)
//!   - "give me the local player's primary live pawn" (read state)
//!
//! The implementation walks GObjects for instances matching a
//! configured base class and an optional Blueprint-class
//! `full_name` filter. UE5-version-specific offsets are not
//! required; this is a name-based discovery surface.
//!
//! Configure once at module init with the base class + (optional)
//! BP-class-name substring; static methods do the rest.
//!
//! ```ignore
//! static PLAYER: ueforge::ue::PlayerRef = ueforge::ue::PlayerRef::new(
//!     "SurvivalCharacter",                  // base class
//!     Some("BP_SurvivalPlayerCharacter"),   // BP filter substring
//! );
//!
//! // Apply CDO writes:
//! PLAYER.for_each_cdo(|cdo| {
//!     // ... write fields on the player CDO ...
//! });
//!
//! // Apply live-instance writes:
//! PLAYER.for_each_live(|pawn| {
//!     // ... write fields on the live player pawn ...
//! });
//!
//! // Read state from the first live pawn:
//! if let Some(stats) = PLAYER.with_first_live(|p| read_stats(p)) {
//!     // ...
//! }
//! ```

use crate::ue::{ClassRef, UObject};

pub struct PlayerRef {
    base_class: ClassRef,
    bp_name_filter: Option<&'static str>,
}

impl PlayerRef {
    /// Configure the player surface. `base_class_name` is the UE
    /// SurvivalCharacter / Pawn class name; `bp_name_filter` is an
    /// optional substring of the Blueprint-generated subclass's
    /// full name (e.g. `BP_SurvivalPlayerCharacter` to exclude NPC
    /// SurvivalCharacter subclasses).
    pub const fn new(base_class_name: &'static str, bp_name_filter: Option<&'static str>) -> Self {
        Self {
            base_class: ClassRef::new(base_class_name),
            bp_name_filter,
        }
    }

    pub fn class(&self) -> &ClassRef {
        &self.base_class
    }

    pub fn bp_filter(&self) -> Option<&'static str> {
        self.bp_name_filter
    }

    /// Walk every CDO matching the configured class + filter.
    /// Returns the count.
    pub fn for_each_cdo(&self, f: impl FnMut(&UObject)) -> usize {
        match self.bp_name_filter {
            Some(filter) => self
                .base_class
                .for_each_cdo_matching(|obj| obj.full_name().contains(filter), f),
            None => self.base_class.for_each_cdo_subclass(f),
        }
    }

    /// Walk every live (non-CDO) instance matching the configured
    /// class + filter. Returns the count.
    pub fn for_each_live(&self, f: impl FnMut(&UObject)) -> usize {
        match self.bp_name_filter {
            Some(filter) => self
                .base_class
                .for_each_matching(|obj| obj.full_name().contains(filter), f),
            None => self.base_class.for_each_instance(f),
        }
    }

    /// Pass the first live pawn to `f` and return the result.
    /// None if no live pawn was found.
    pub fn with_first_live<R>(&self, f: impl FnOnce(&UObject) -> R) -> Option<R> {
        // Capture by raw addr so the closure is FnMut-friendly.
        let mut hit: Option<usize> = None;
        self.for_each_live(|obj| {
            if hit.is_none() {
                hit = Some(obj as *const UObject as usize);
            }
        });
        hit.map(|addr| {
            let obj: &UObject = unsafe { &*(addr as *const UObject) };
            f(obj)
        })
    }

    /// Return the first live pawn as `&'static UObject`. None if
    /// no live pawn was found.
    ///
    /// # Safety
    /// Caller MUST be on the game thread (typically inside a PE
    /// trampoline / drain-site closure). The returned reference's
    /// lifetime is artificially extended to `'static`; it's only
    /// valid until the engine destroys the underlying object,
    /// which can happen at any GC pass off-thread. Use this when
    /// you need to pass the pawn through APIs that require
    /// `'static` (e.g. queued-closure resolvers) and the caller
    /// guarantees the object lives for the duration of use.
    pub unsafe fn first_live_static(&self) -> Option<&'static UObject> {
        let mut hit: Option<usize> = None;
        self.for_each_live(|obj| {
            if hit.is_none() {
                hit = Some(obj as *const UObject as usize);
            }
        });
        hit.map(|addr| unsafe { &*(addr as *const UObject) })
    }

    /// Pass the first CDO to `f` and return the result.
    /// None if no CDO was found.
    pub fn with_first_cdo<R>(&self, f: impl FnOnce(&UObject) -> R) -> Option<R> {
        let mut hit: Option<usize> = None;
        self.for_each_cdo(|obj| {
            if hit.is_none() {
                hit = Some(obj as *const UObject as usize);
            }
        });
        hit.map(|addr| {
            let obj: &UObject = unsafe { &*(addr as *const UObject) };
            f(obj)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_constructible() {
        const _P: PlayerRef = PlayerRef::new("Foo", Some("BP_Bar"));
        const _Q: PlayerRef = PlayerRef::new("Foo", None);
    }

    #[test]
    fn unresolved_returns_none() {
        let p = PlayerRef::new("DoesNotExist", Some("BP_DoesNotExist"));
        assert_eq!(p.for_each_cdo(|_| {}), 0);
        assert_eq!(p.for_each_live(|_| {}), 0);
        assert!(p.with_first_live(|_| ()).is_none());
        assert!(p.with_first_cdo(|_| ()).is_none());
    }

    #[test]
    fn config_round_trip() {
        const P: PlayerRef = PlayerRef::new("Foo", Some("BP_Bar"));
        assert_eq!(P.class().name(), "Foo");
        assert_eq!(P.bp_filter(), Some("BP_Bar"));
    }
}
