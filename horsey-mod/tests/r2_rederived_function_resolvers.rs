//! Verify all 11 re-derived function-entry resolvers.

mod common;

use modforge::testkit::json::u64_at_key as u64_of;
use serde_json::json;

#[test]
fn rederived_function_resolvers() {
    let Some(game) = common::launch("r2_rederived_function_resolvers") else {
        return;
    };

    let targets: &[(&str, u64, &[&str])] = &[
        (
            "CHECK_HORSE_ELIGIBILITY",
            0x1400de230,
            &[
                "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8b ec 48 81 ec 80 00 00 00 0f 29 74 24",
            ],
        ),
        (
            "RETIRE_HORSE_HANDLER",
            0x1400df675,
            &[
                "53 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 e1 48 81 ec f8 00 00 00 4c 8b e9 45 33 e4 44 89 65",
            ],
        ),
        (
            "COMPUTE_HORSE_PRICE",
            0x1400dc785,
            &[
                "48 89 7c 24 10 55 48 8b ec 48 83 ec 60 0f 29 74 24 50 48 8b f9 4c 63 c2 48 8b 91 30 01 00 00 48",
            ],
        ),
        (
            "CRISPR_LAB",
            0x1400894bc,
            &[
                "57 41 54 41 56 41 57 48 8d a8 58 fe ff ff 48 81 ec 80 02 00 00 0f 29 70 c8 0f 29 78 b8 44 0f 29",
            ],
        ),
        (
            "BREEDING",
            0x1400e0817,
            &[
                "57 41 54 41 55 41 56 41 57 48 8b ec 48 83 ec 68 44 8b f2 4c 8b f9 4c 8b 81 30 01 00 00 48 8b 81",
            ],
        ),
        (
            "DRAW_PAUSE_STATUS",
            0x1400660cc,
            &[
                "57 41 54 41 56 41 57 48 8d 68 a1 48 81 ec c0 00 00 00 0f 29 70 c8 0f 29 78 b8 44 0f 29 40 a8 44",
            ],
        ),
        (
            "POP_GENOME_BUILDER",
            0x1400927e7,
            &[
                "57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec e0 00 00 00 0f 29 b4 24 d0 00 00 00 48 8b f9",
            ],
        ),
        (
            "SUMO_HANDLER",
            0x14007b28c,
            &[
                "57 41 56 48 8d 68 a1 48 81 ec c0 00 00 00 0f 29 70 d8 0f 29 78 c8 44 0f 29 40 b8 44 0f 29 48 a8",
            ],
        ),
        (
            "POWER_PLANT",
            0x140069270,
            &[
                "48 89 5c 24 08 55 48 8b ec 48 83 ec 60 0f 29 74 24 50 48 8b d9 ff 81 50 02 00 00 e8 ?? ?? ?? ??",
            ],
        ),
        (
            "WORLD_ACTION",
            0x1401075a2,
            &[
                "57 41 54 41 55 41 56 41 57 48 83 ec 70 0f 29 70 c8 0f 29 78 b8 44 8b ca 4c 8b f9 0f 57 c0 f3 0f",
            ],
        ),
        (
            "BALLOON_CONTROLLER",
            0x14010a9e0,
            &[
                "48 8b c4 48 89 58 10 48 89 70 18 48 89 78 20 55 41 54 41 55 41 56 41 57 48 8d 68 a1 48 81 ec a0",
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
        .expect("sleuth.resolve");
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
                "{name}: resolved 0x{resolved:x} != expected 0x{expected:x} (delta 0x{delta:x})"
            ));
            continue;
        }
        game.log()
            .event("R2-REDERIVED", &format!("{name}: 0x{resolved:x} (exact)"));
    }
    if !failures.is_empty() {
        panic!(
            "re-derived resolvers failed:\n  - {}",
            failures.join("\n  - ")
        );
    }
    game.pass("R2 re-derived: 11 stale-RVA function entries now resolve");
}
