//! R2 contract: the 4 save-related target addresses must be
//! resolvable via patternsleuth signatures.
//!
//! Drives the D4 stale-address fix:
//!   - SAVE_WRITER (top-level)
//!   - LOAD_GAME (top-level)
//!   - HORSE_SAVE_WRITER (per-horse)
//!   - HORSE_SAVE_LOADER (per-horse)
//!
//! Current state (2026-05-15): the hardcoded RVAs (0x14006dc80,
//! 0x14006e480, 0x14006ee10, 0x14006f150) all land mid-function in
//! the shipping build. The harness test
//! `dryrun_d3_d4::save_dryrun_prologues_ok` is red because of this.
//!
//! Failing-test contract: for each target, the catalog declares a
//! list of candidate signatures; patternsleuth must resolve each to
//! an address whose first byte passes the MSVC function-entry
//! classifier. When the contract goes green, the addresses go into
//! `targets::fn_addr` as resolver-backed accessors and the
//! dryrun_d3_d4 save test flips green too.
//!
//! Initial state: signatures list is empty for each target ->
//! sleuth returns None for each -> test fails. Implementation:
//! derive signatures from decomp body bytes + live `patterns.read_bytes`
//! observations, populate the catalog, watch the test go green.

mod common;

use modforge::harness::{GameHarness, RunningGame};
use serde_json::{Value, json};

/// Catalog entry: a target the resolver should find.
struct SaveTarget {
    name: &'static str,
    /// Hand-authored byte signatures. First match wins per
    /// patternsleuth semantics. Empty = test fails (contract).
    sigs: &'static [&'static str],
}

// Signatures derived from `tests/r2_save_find_entries.rs` probe
// run 2026-05-15. The stale Ghidra RVAs (0x14006dc80,
// 0x14006e480, 0x14006ee10, 0x14006f150) all landed mid-function;
// the true entries are -1548, -304, -277, -287 bytes from those.
//
// Each signature picks bytes that combine prologue (push chain +
// sub rsp) with a body landmark distinctive to the save target
// (e.g. HORSE_SAVE_LOADER's `add rcx, 0x2b8` taking the genome
// pointer). Wildcards on call-rel32 displacements + stack disps
// where compiler-shifted.
const SAVE_TARGETS: &[SaveTarget] = &[
    SaveTarget {
        name: "SAVE_WRITER",
        // -1548 from stale. push rdi; push r14; sub rsp,0x140;
        // mov rbp,rcx; lea rdx, [rcx+...]. Large stack frame is
        // distinctive for the multi-local save driver.
        sigs: &["57 41 56 48 81 ec 40 01 00 00 48 8b e9 48 8d 51"],
    },
    SaveTarget {
        name: "LOAD_GAME",
        // -304 from stale. Shadow-save rbx + push 7 regs +
        // mov rbp,rsp + sub rsp,0x70. The 7-reg push chain is
        // less common than the 5-reg variant.
        sigs: &["48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8b ec 48 83 ec 70"],
    },
    SaveTarget {
        name: "HORSE_SAVE_WRITER",
        // -277 from stale. push rdi; push r14; sub rsp,0x40;
        // mov rbp,rcx; call rel32. rcx is the horse pointer
        // (1st arg). Call displacement wildcarded.
        sigs: &["57 41 56 48 83 ec 40 48 8b e9 e8 ?? ?? ?? ?? 48"],
    },
    SaveTarget {
        name: "HORSE_SAVE_LOADER",
        // -287 from stale. push rbx; push rdi; sub rsp,0x58;
        // mov rdi,rcx; add rcx,0x2b8. The 0x2b8 add is the
        // genome-offset reference; highly distinctive.
        sigs: &["53 57 48 83 ec 58 48 8b f9 48 81 c1 b8 02 00 00"],
    },
];

use modforge::testkit::fn_entry::is_msvc_x64_prologue_loose as looks_like_msvc_entry;

fn parse_prologue_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .filter_map(|tok| u8::from_str_radix(tok, 16).ok())
        .collect()
}

fn resolve(game: &RunningGame, targets: &[SaveTarget]) -> Value {
    let patterns_json: Vec<_> = targets
        .iter()
        .map(|t| json!({"name": t.name, "sigs": t.sigs}))
        .collect();
    let resp = game
        .op_json(
            "patterns.sleuth.resolve",
            &json!({"patterns": patterns_json}),
        )
        .expect("patterns.sleuth.resolve must succeed");
    resp.get("result")
        .and_then(|r| r.get("results"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn fetch_prologue(game: &RunningGame, addr: &str) -> String {
    let resp = game
        .op_json("patterns.read_bytes", &json!({"addr": addr, "n": 16}))
        .expect("patterns.read_bytes must succeed");
    resp.get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

#[test]
fn catalog_resolves_all_save_targets_to_msvc_entries() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r2_save_signatures").expect("harness launch failed");

    let results = resolve(&game, SAVE_TARGETS);
    game.log()
        .event("SAVE_SIGS", &format!("resolved: {results}"));

    let mut failures: Vec<String> = Vec::new();

    for target in SAVE_TARGETS {
        if target.sigs.is_empty() {
            failures.push(format!(
                "{}: catalog has no signatures (TODO: derive from decomp + live image)",
                target.name
            ));
            continue;
        }
        let addr = results
            .get(target.name)
            .and_then(|v| v.as_str())
            .map(str::to_string);
        match addr {
            None => failures.push(format!(
                "{}: resolver returned null (no signature matched)",
                target.name
            )),
            Some(a) => {
                let prologue_hex = fetch_prologue(&game, &a);
                let bytes = parse_prologue_hex(&prologue_hex);
                game.log().event(
                    "SAVE_SIGS",
                    &format!("{} resolved={a} prologue=[{prologue_hex}]", target.name),
                );
                if !looks_like_msvc_entry(&bytes) {
                    failures.push(format!(
                        "{}: address {a} prologue [{prologue_hex}] is not an MSVC entry; signature is too loose / wrong target",
                        target.name
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        let joined = failures.join("\n  - ");
        panic!("save-signature catalog has unresolved targets:\n  - {joined}");
    }
    game.pass("all 4 save targets resolve to MSVC entries");
}
