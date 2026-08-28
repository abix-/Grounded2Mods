//! Verify all 8 R-parity function-entry resolvers return the
//! expected hardcoded RVA (within the 0x10000 sanity band).
//!
//! Locks the migration from R-parity to R: production call sites
//! call `targets::resolve::*` first; this test proves the live
//! binary's 32-byte body sig matches at the same place the
//! hardcoded RVA does.

mod common;

use modforge::testkit::json::u64_at_key as u64_of;
use serde_json::json;

#[test]
fn r_parity_function_resolvers() {
    let Some(game) = common::launch("r2_function_resolvers") else {
        return;
    };

    // 8 functions; for each, build a target list and resolve via
    // patterns.sleuth.resolve (the existing multi-target op), then
    // compare to the hardcoded rebased RVA.
    let targets: &[(&str, u64, &[&str])] = &[
        (
            "APPLY_GENE_TO_HORSE",
            0x14009f670,
            &[
                "48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48 8d a8 d8 fb ff ff 48 81 ec e8 04 00 00 0f 29 70",
            ],
        ),
        (
            "HORSE_CONSTRUCTOR",
            0x1400aac50,
            &[
                "48 89 5c 24 10 48 89 4c 24 08 57 48 83 ec 20 48 8b d9 48 8d 05 ?? ?? ?? ?? 48 89 01 33 ff 48 89",
            ],
        ),
        (
            "HORSE_DESTRUCTOR",
            0x1400bf1e0,
            &[
                "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8d 05 ?? ?? ?? ?? 8b f2 48 89 01 48 8b d9 ff 0d",
            ],
        ),
        (
            "GENE_COMBINATOR",
            0x1400a2d70,
            &[
                "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 ec 20 4c 8b f9 45",
            ],
        ),
        (
            "SAVE_WRITER",
            0x14006d674,
            &[
                "57 41 56 48 81 ec 40 01 00 00 48 8b e9 48 8d 51 18 48 8d 4c 24 30 e8 ?? ?? ?? ?? 48 8b 54 24 48",
            ],
        ),
        (
            "LOAD_GAME",
            0x14006e350,
            &[
                "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8b ec 48 83 ec 70 48 8b f1 44 8b c2 48 8d 15",
            ],
        ),
        (
            "HORSE_SAVE_WRITER",
            0x14006ecfb,
            &[
                "57 41 56 48 83 ec 40 48 8b e9 e8 ?? ?? ?? ?? 48 8d 8d b8 02 00 00 44 8b f0 e8 ?? ?? ?? ?? 48 8b",
            ],
        ),
        (
            "HORSE_SAVE_LOADER",
            0x14006f031,
            &[
                "53 57 48 83 ec 58 48 8b f9 48 81 c1 b8 02 00 00 e8 ?? ?? ?? ?? 48 8d 8f a8 02 00 00 e8 ?? ?? ??",
            ],
        ),
    ];

    let image_resp = game
        .op_json("targets.resolve.gamestate_ptr", &json!({}))
        .expect("ok");
    let ib = u64_of(image_resp.get("result").unwrap(), "image_base").expect("image_base");

    let patterns: Vec<_> = targets
        .iter()
        .map(|(name, _, sigs)| json!({"name": *name, "sigs": *sigs}))
        .collect();

    let resp = game
        .op_json("patterns.sleuth.resolve", &json!({"patterns": patterns}))
        .expect("sleuth.resolve must succeed");
    let results = resp
        .get("result")
        .and_then(|r| r.get("results"))
        .expect("result.results");

    let mut failures: Vec<String> = Vec::new();
    for (name, rva, _) in targets {
        let resolved = u64_of(results, name).unwrap_or(0);
        let expected = ib + (rva - 0x140000000);
        if resolved == 0 {
            failures.push(format!("{name}: sig did not match"));
            continue;
        }
        let delta = resolved.abs_diff(expected);
        if delta > 0x10000 {
            failures.push(format!(
                "{name}: resolved 0x{resolved:x} drifts 0x{delta:x} from rebased \
                 hardcoded 0x{expected:x}"
            ));
            continue;
        }
        game.log().event(
            "R2-FN",
            &format!("{name}: resolved=0x{resolved:x} expected=0x{expected:x} delta=0x{delta:x}"),
        );
    }
    if !failures.is_empty() {
        let joined = failures.join("\n  - ");
        panic!("function resolvers failed:\n  - {joined}");
    }
    game.pass("R2: all 8 R-parity function entries resolve via patternsleuth");
}
