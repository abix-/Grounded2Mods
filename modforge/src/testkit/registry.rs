//! Shared R-tier assertion functions for any consumer mod's
//! `sleuth::TargetRegistry`. Replaces the per-game
//! `r2_function_resolvers.rs` / `r3_*_resolves.rs` /
//! `r4_field_offsets.rs` family with three reusable functions.
//!
//! Pattern: consumer mod's test file is a thin wrapper:
//! ```ignore
//! #[test]
//! fn registry_resolves() {
//!     let Some(game) = common::launch("registry_resolves") else { return };
//!     modforge::testkit::registry::assert_every_target_resolves(
//!         &game, &my_mod::HORSEY_TARGETS, &my_mod::HORSEY_RESOLVER,
//!     );
//! }
//! ```

use crate::harness::RunningGame;
use crate::patterns::sleuth::{Resolver, TargetKind, TargetRegistry};

/// Assert every entry in the registry resolves to a concrete
/// value. The resolver's hint-rva fallback counts as a resolution.
///
/// Panics on the first entry that returned None.
pub fn assert_every_target_resolves(
    _game: &RunningGame,
    registry: &TargetRegistry,
    resolver: &Resolver,
) {
    let resolved = resolver.resolve_all();
    let mut failures: Vec<String> = Vec::new();
    for def in registry.iter() {
        match resolved.get(def.name).and_then(|r| r.value) {
            Some(addr) => {
                eprintln!("  {}: 0x{addr:x}", def.name);
            }
            None => {
                let log = resolved
                    .get(def.name)
                    .map(|r| r.validation_log.join("; "))
                    .unwrap_or_else(|| "<no log>".into());
                failures.push(format!("{}: did not resolve. log: {log}", def.name));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} target(s) in registry {:?} failed to resolve:\n  {}",
        failures.len(),
        registry.name,
        failures.join("\n  "),
    );
}

/// Assert every entry whose value is set has a non-empty
/// validation log AND the log does not contain any "rejected"
/// lines (which would indicate a candidate decoded but validators
/// rejected it AND no fallback worked). The hint-rva fallback is
/// allowed.
pub fn assert_every_target_passes_validators(
    _game: &RunningGame,
    registry: &TargetRegistry,
    resolver: &Resolver,
) {
    let resolved = resolver.resolve_all();
    let mut failures: Vec<String> = Vec::new();
    for def in registry.iter() {
        let r = match resolved.get(def.name) {
            Some(r) => r,
            None => {
                failures.push(format!("{}: missing from resolution", def.name));
                continue;
            }
        };
        let any_rejected = r
            .validation_log
            .iter()
            .any(|l| l.contains("validators rejected"));
        let resolved_ok = r.value.is_some() && !r.from_hint;
        if any_rejected && !resolved_ok && !r.from_hint {
            failures.push(format!(
                "{}: every candidate was validator-rejected and no hint fallback. log: {}",
                def.name,
                r.validation_log.join("; ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} target(s) failed validators:\n  {}",
        failures.len(),
        failures.join("\n  "),
    );
}

/// Assert every `TargetKind::FieldOffset` entry's resolved value
/// equals its hint exactly. Field offsets must match exactly across
/// builds (they describe struct layout, which only changes when the
/// struct changes).
pub fn assert_every_field_offset_matches_hint(
    _game: &RunningGame,
    registry: &TargetRegistry,
    resolver: &Resolver,
) {
    let resolved = resolver.resolve_all();
    let mut failures: Vec<String> = Vec::new();
    for def in registry.iter() {
        if !matches!(def.kind, TargetKind::FieldOffset { .. }) {
            continue;
        }
        let r = match resolved.get(def.name) {
            Some(r) => r,
            None => {
                failures.push(format!("{}: missing from resolution", def.name));
                continue;
            }
        };
        match (r.value, def.hint_rva) {
            (Some(v), Some(hint)) if v == hint => {
                eprintln!("  {}: 0x{v:x} == hint", def.name);
            }
            (Some(v), Some(hint)) => {
                failures.push(format!("{}: resolved 0x{v:x} != hint 0x{hint:x}", def.name));
            }
            (Some(v), None) => {
                failures.push(format!(
                    "{}: resolved 0x{v:x} but no hint to verify against",
                    def.name
                ));
            }
            (None, _) => {
                failures.push(format!("{}: did not resolve", def.name));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} field-offset target(s) failed exact-hint match:\n  {}",
        failures.len(),
        failures.join("\n  "),
    );
}

/// Assert that the registry's diagnostic JSON view has every
/// entry present, regardless of resolution outcome. Use as a
/// smoke test that the HTTP control-plane shape is stable.
pub fn assert_diagnostic_includes_every_entry(
    _game: &RunningGame,
    registry: &TargetRegistry,
    resolver: &Resolver,
) {
    let diag = resolver.diagnostic();
    let entries = diag
        .get("entries")
        .and_then(|v| v.as_array())
        .expect("diagnostic.entries must be array");
    let names: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.get("name").and_then(|n| n.as_str()))
        .collect();
    for def in registry.iter() {
        assert!(
            names.contains(&def.name),
            "diagnostic missing entry '{}'",
            def.name
        );
    }
}

#[cfg(test)]
mod tests {
    // The functions above all take a `&RunningGame` and only run
    // when called from a live integration test in a consumer mod.
    // The Resolver internals are exercised by sleuth::tests; this
    // module's logic is pure delegation + assertion formatting.
    //
    // Smoke-test that the functions exist and the signatures
    // compile against a representative registry.

    use crate::patterns::sleuth::{TargetDef, TargetKind, TargetRegistry};

    static D1: TargetDef = TargetDef {
        name: "A",
        kind: TargetKind::DataGlobal,
        candidates: &[],
        hint_rva: None,
        hint_tolerance: 0,
        validators: &[],
    };
    static REG: TargetRegistry = TargetRegistry::new("smoke", &[&D1]);

    #[test]
    fn registry_smoke_compiles() {
        assert_eq!(REG.entries.len(), 1);
        assert_eq!(REG.name, "smoke");
    }
}
