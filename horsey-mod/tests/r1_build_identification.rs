//! R1 build identification. Every attach must log + expose a
//! stable hash of the loaded Horsey.exe image so test logs and bug
//! reports can be tied to a specific build.
//!
//! Failing-test contract:
//!   1. `game.build_info` HTTP op must exist.
//!   2. It returns a JSON object with at minimum:
//!      - `image_sha256`: 64-char lowercase hex.
//!      - `image_base`: hex string of the loaded .exe base address.
//!      - `text_size`: integer byte size of the .text section.
//!   3. Two calls within the same session return the same hash
//!      (deterministic; not a fresh compute each time).
//!   4. The hash is stable across different launches of the SAME
//!      Horsey.exe binary (only changes when Steam ships an update).
//!
//! Property (4) requires comparing two separate launches in one
//! test, which our harness supports via two sequential
//! `GameHarness::launch` calls.

mod common;

use modforge::harness::GameHarness;
use serde_json::json;

fn is_lowercase_hex_64(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
}

#[test]
fn build_info_returns_image_sha256() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r1_build_info_shape").expect("harness launch failed");

    let resp = game
        .op_json("game.build_info", &json!({}))
        .expect("game.build_info op must exist");

    let result = resp.get("result").expect("envelope has result");
    let sha = result
        .get("image_sha256")
        .and_then(|v| v.as_str())
        .expect("image_sha256 must be present as a string");
    let base = result
        .get("image_base")
        .and_then(|v| v.as_str())
        .expect("image_base must be present as a string");
    let text_size = result
        .get("text_size")
        .and_then(|v| v.as_u64())
        .expect("text_size must be present as a u64");

    game.log().event(
        "BUILD",
        &format!("sha256={sha} base={base} text_size={text_size}"),
    );

    assert!(
        is_lowercase_hex_64(sha),
        "image_sha256 must be 64 lowercase hex chars, got {sha:?}"
    );
    assert!(
        base.starts_with("0x") && base.len() > 4,
        "image_base must be `0x...`, got {base:?}"
    );
    assert!(
        text_size > 0x10_000 && text_size < 0x800_0000,
        "text_size {text_size} not in plausible 64KB..128MB range"
    );

    game.pass("build_info has sha256 + base + text_size");
}

#[test]
fn build_info_sha_is_stable_within_session() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r1_build_info_stable").expect("harness launch failed");

    let a = game
        .op_json("game.build_info", &json!({}))
        .expect("build_info call 1");
    let b = game
        .op_json("game.build_info", &json!({}))
        .expect("build_info call 2");

    let sha_a = a
        .get("result")
        .and_then(|r| r.get("image_sha256"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let sha_b = b
        .get("result")
        .and_then(|r| r.get("image_sha256"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    game.log().event("BUILD", &format!("a={sha_a} b={sha_b}"));
    assert_eq!(
        sha_a, sha_b,
        "two build_info calls in the same session must return identical hashes"
    );
    game.pass("build_info sha stable within session");
}
