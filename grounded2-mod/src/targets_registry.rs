//! grounded2-mod's declarative target registry. Cross-game adoption
//! proof for the modforge target-registry pattern (workspace todo
//! P0 task B6).
//!
//! Grounded 2 is a UE5-Augusta build (`STEAM` / `XBOX` platforms).
//! The five canonical UE5 globals (g_objects, g_names, g_world,
//! append_string, process_event) have known per-platform RVAs in
//! [`ueforge::ue::offsets::STEAM`] and `XBOX`. Today
//! grounded2-mod consumes those via the `PlatformOffsets` struct
//! directly; this registry mirrors the same data in the modforge
//! shape so a future modforge release can centralize per-platform
//! address swapping through the same Resolver controller used by
//! horsey-mod.
//!
//! What this proves: a SECOND consumer mod successfully declares
//! a `TargetRegistry`, gets the four shared R-tier assertion
//! functions for free, and exposes `targets.resolver.diagnostic`
//! over HTTP without any per-game boilerplate. The cross-game
//! abstraction holds.

use modforge::patterns::sleuth::{
    InImage, Resolver, TargetDef, TargetKind, TargetRegistry, Validator,
};

static V_IN_IMAGE: &[&dyn Validator] = &[&InImage];

// ============================================================================
// UE5-Augusta data globals (Steam build)
// ============================================================================
//
// Hardcoded per ueforge::ue::offsets::STEAM. Hint-only for now;
// real sig candidates land per-target as UE5 builds drift. The
// hint_tolerance is intentionally large because UE5 builds shift
// these globals routinely between game patches.

static G_OBJECTS: TargetDef = TargetDef {
    name: "G_OBJECTS",
    kind: TargetKind::DataGlobal,
    candidates: &[],
    hint_rva: Some(0x09F6_7028),
    hint_tolerance: 0x0010_0000,
    validators: V_IN_IMAGE,
};

static G_NAMES: TargetDef = TargetDef {
    name: "G_NAMES",
    kind: TargetKind::DataGlobal,
    candidates: &[],
    hint_rva: Some(0x09E4_A7B8),
    hint_tolerance: 0x0010_0000,
    validators: V_IN_IMAGE,
};

static G_WORLD: TargetDef = TargetDef {
    name: "G_WORLD",
    kind: TargetKind::DataGlobal,
    candidates: &[],
    hint_rva: Some(0x09C7_A2E0),
    hint_tolerance: 0x0010_0000,
    validators: V_IN_IMAGE,
};

static APPEND_STRING: TargetDef = TargetDef {
    name: "APPEND_STRING",
    kind: TargetKind::FunctionEntry { signature: None },
    candidates: &[],
    hint_rva: Some(0x0125_2060),
    hint_tolerance: 0x0010_0000,
    validators: V_IN_IMAGE,
};

static PROCESS_EVENT: TargetDef = TargetDef {
    name: "PROCESS_EVENT",
    kind: TargetKind::FunctionEntry { signature: None },
    candidates: &[],
    hint_rva: Some(0x0146_E7B0),
    hint_tolerance: 0x0010_0000,
    validators: V_IN_IMAGE,
};

pub static GROUNDED2_TARGETS: TargetRegistry = TargetRegistry::new(
    "grounded2-mod",
    &[&G_OBJECTS, &G_NAMES, &G_WORLD, &APPEND_STRING, &PROCESS_EVENT],
);

/// Per-process Resolver singleton bound to `GROUNDED2_TARGETS`.
pub static GROUNDED2_RESOLVER: Resolver = Resolver::new(&GROUNDED2_TARGETS);

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that Grounded 2 declares every Unreal global needed by its runtime binding.
    /// Stays here because the expected target names describe this game's executable.
    #[test]
    fn registry_declares_five_ue5_globals() {
        for name in ["G_OBJECTS", "G_NAMES", "G_WORLD", "APPEND_STRING", "PROCESS_EVENT"] {
            assert!(
                GROUNDED2_TARGETS.def(name).is_some(),
                "GROUNDED2_TARGETS missing {name}"
            );
        }
        assert_eq!(GROUNDED2_TARGETS.entries.len(), 5);
    }

    /// Verifies that target diagnostics identify Grounded 2 as their owning consumer.
    /// Stays here because the expected registry name belongs to this mod.
    #[test]
    fn registry_name_identifies_consumer() {
        assert_eq!(GROUNDED2_TARGETS.name, "grounded2-mod");
    }

    /// Verifies that Grounded 2's target hints match its active Ueforge platform offsets.
    /// Stays here because both expected address sets describe this game's Steam executable.
    #[test]
    fn registry_hint_rvas_match_ueforge_steam_offsets() {
        // Lock the data agreement: the registry's hints must equal
        // the ueforge::ue::offsets::STEAM constants. Future drift
        // (Grounded2 game update) requires updating BOTH places.
        use ueforge::ue::offsets::STEAM;
        assert_eq!(
            GROUNDED2_TARGETS.def("G_OBJECTS").unwrap().hint_rva,
            Some(STEAM.g_objects as u64),
        );
        assert_eq!(
            GROUNDED2_TARGETS.def("G_NAMES").unwrap().hint_rva,
            Some(STEAM.g_names as u64),
        );
        assert_eq!(
            GROUNDED2_TARGETS.def("APPEND_STRING").unwrap().hint_rva,
            Some(STEAM.append_string as u64),
        );
    }
}
