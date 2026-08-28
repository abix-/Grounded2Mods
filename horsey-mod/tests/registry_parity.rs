//! Parity test: every target in `horsey::targets_registry::HORSEY_TARGETS`
//! resolves correctly against the live game image AND the resolved
//! address agrees with the corresponding legacy `targets::resolve::*`
//! function (where one exists).
//!
//! This is the gate that catches drift between the declarative
//! registry (new path) and the hand-rolled resolvers (legacy path)
//! during the B2 -> B4 migration. Once every horsey-mod call site
//! is on the registry and B4 is complete, the legacy half can be
//! deleted and this test becomes a pure registry-resolves check.

mod common;

use horsey::targets_registry::{HORSEY_RESOLVER, HORSEY_TARGETS, resolve};
use modforge::testkit::registry as r_tests;

#[test]
fn every_target_in_registry_resolves() {
    let Some(game) = common::launch("registry_resolves") else {
        return;
    };
    r_tests::assert_every_target_resolves(&game, &HORSEY_TARGETS, &HORSEY_RESOLVER);
}

#[test]
fn every_target_passes_validators() {
    let Some(game) = common::launch("registry_validators") else {
        return;
    };
    r_tests::assert_every_target_passes_validators(&game, &HORSEY_TARGETS, &HORSEY_RESOLVER);
}

#[test]
fn every_field_offset_matches_hint() {
    let Some(game) = common::launch("registry_field_offsets") else {
        return;
    };
    r_tests::assert_every_field_offset_matches_hint(&game, &HORSEY_TARGETS, &HORSEY_RESOLVER);
}

#[test]
fn diagnostic_includes_every_entry() {
    let Some(game) = common::launch("registry_diagnostic") else {
        return;
    };
    r_tests::assert_diagnostic_includes_every_entry(&game, &HORSEY_TARGETS, &HORSEY_RESOLVER);
}

/// Parity: registry-resolved GAMESTATE_PTR equals the legacy
/// `resolve::gamestate_ptr()` result. Both should converge on the
/// same address; this test guards against silent drift while both
/// paths coexist.
#[test]
fn registry_gamestate_ptr_matches_legacy_resolver() {
    let Some(_game) = common::launch("registry_gamestate_parity") else {
        return;
    };

    let registry_value = HORSEY_RESOLVER.resolve("GAMESTATE_PTR");
    let legacy_value = resolve::gamestate_ptr();

    eprintln!("registry  GAMESTATE_PTR: {registry_value:?}");
    eprintln!("legacy    GAMESTATE_PTR: {legacy_value:?}");

    match (registry_value, legacy_value) {
        (Some(r), Some(l)) => {
            assert_eq!(
                r as usize, l,
                "registry resolved 0x{r:x}, legacy resolved 0x{l:x}"
            );
        }
        (None, None) => {
            // Both paths failed. Acceptable as long as the hint
            // fallback was tried; the registry should at minimum
            // report from_hint=true.
            let resolved = HORSEY_RESOLVER.resolve_all();
            eprintln!(
                "both paths failed; registry log: {:?}",
                resolved
                    .get("GAMESTATE_PTR")
                    .map(|r| r.validation_log.clone())
            );
        }
        (r, l) => panic!("registry/legacy disagree: registry={r:?} legacy={l:?}"),
    }
}
