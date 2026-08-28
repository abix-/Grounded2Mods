//! Verify all 12 H-batch function resolvers match the hardcoded
//! RVA in this build.

mod common;

use modforge::testkit::json::u64_at_key as u64_of;
use serde_json::json;

#[test]
fn h_function_resolvers() {
    let Some(game) = common::launch("r2_h_function_resolvers") else {
        return;
    };

    let targets: &[(&str, u64, &[&str])] = &[
        (
            "EVAL_DIPLOID_BLEND_A",
            0x1400a5d10,
            &[
                "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 48 63 da 48 8d 2d 72 87 34 00 48 8b",
            ],
        ),
        (
            "EVAL_DIPLOID_BLEND_B",
            0x1400a5df0,
            &[
                "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 48 63 da 48 8d 2d 92 86 34 00 48 8b",
            ],
        ),
        (
            "GENE_DEATH_DRIFT",
            0x1400c0650,
            &[
                "48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48 8d a8 b8 fa ff ff 48 81 ec 08 06 00 00 0f 29 70",
            ],
        ),
        (
            "GENE_ALLELE_SWAP",
            0x1400c0390,
            &[
                "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 48 63 fa 48 be c1 d4 1c 42 29 8f a0 3f 4d 63 d9 45",
            ],
        ),
        (
            "GENE_TABLE_XML_WRITER",
            0x1400a4880,
            &[
                "41 54 41 55 41 56 41 57 48 8d 6c 24 90 48 81 ec 70 01 00 00 0f 29 70 c8 0f 29 78 b8 44 0f 29 40",
            ],
        ),
        (
            "GENE_TABLE_LOADER",
            0x1400a3eb0,
            &[
                "41 57 48 8d a8 28 ff ff ff 48 81 ec a0 01 00 00 0f 29 70 b8 0f 29 78 a8 44 0f 29 40 98 44 0f 29 48 88 33 f6 8b ce 48 8d 05 ?? ?? ?? ?? 0f 1f 00",
            ],
        ),
        (
            "POP_XML_LOADER",
            0x1400a4fe0,
            &[
                "48 8d ac 24 c0 cf ff ff b8 40 31 00 00 e8 ?? ?? ?? ?? 48 2b e0 48 8d 4c 24 50 e8 ?? ?? ?? ?? 90",
            ],
        ),
        (
            "GENE_ENGINE_CONSUMER",
            0x1400ab3c0,
            &[
                "48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48 8d a8 88 fd ff ff 48 81 ec 38 03 00 00 0f 29 70",
            ],
        ),
        (
            "TMX_MAP_PARSER",
            0x1400fe2e0,
            &[
                "48 89 5c 24 08 48 89 7c 24 10 55 48 8b ec 48 83 ec 20 8b fa 33 db 83 fa 0c 7f 25 b9 78 04 00 00",
            ],
        ),
        (
            "DAILY_HORSE_EVENT",
            0x14002fe00,
            &[
                "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57 48 8d 6c 24 c9 48 81 ec",
            ],
        ),
        (
            "TRACK_STATE_MACHINE",
            0x14002d7c0,
            &[
                "48 8b c4 48 89 58 10 48 89 70 18 48 89 78 20 55 41 54 41 55 41 56 41 57 48 8d 68 a1 48 81 ec e0",
            ],
        ),
        (
            "CIRCUS_HANDLER",
            0x140039190,
            &[
                "48 8b c4 48 89 48 08 55 53 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 b8 48 81 ec 48 01 00 00 0f",
            ],
        ),
    ];

    let r = game
        .op_json("targets.resolve.gamestate_ptr", &json!({}))
        .expect("ok");
    let ib = u64_of(r.get("result").unwrap(), "image_base").expect("image_base");

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
        .expect("results");

    let mut failures = Vec::new();
    for (name, rva, _) in targets {
        let resolved = u64_of(results, name).unwrap_or(0);
        let expected = ib + (rva - 0x140000000);
        if resolved == 0 {
            failures.push(format!("{name}: sig did not match"));
            continue;
        }
        let delta = resolved.abs_diff(expected);
        if delta != 0 {
            failures.push(format!(
                "{name}: resolved 0x{resolved:x} drifts 0x{delta:x} from expected 0x{expected:x}; \
                 sig is not unique"
            ));
            continue;
        }
        game.log().event(
            "R2-H",
            &format!("{name}: resolved=0x{resolved:x} delta=0x{delta:x}"),
        );
    }
    if !failures.is_empty() {
        panic!(
            "H function resolvers failed:\n  - {}",
            failures.join("\n  - ")
        );
    }
    game.pass("R2 H batch: 12 function entries resolve via patternsleuth");
}
