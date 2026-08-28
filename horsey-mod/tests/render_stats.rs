//! Snapshot the stats + dryrun for every ext-gene detour. Generic
//! over the op list (env-overridable). Asserts every op returns a
//! result envelope without an error field.
//!
//! Modes (env-driven):
//! - default: walk the canonical stats + dryrun op set, assert each
//!   one's response is well-formed.
//! - `HORSEY_STATS_OPS=<csv>`: override the stats op list.
//! - `HORSEY_DRYRUN_OPS=<csv>`: override the dryrun op list.
//! - `HORSEY_EXPECT_ARMED=<csv>`: each named subsystem's stats must
//!   report `armed: true` (used to verify a full-stack arm has stuck).

mod common;

use serde_json::{Value, json};

const DEFAULT_STATS_OPS: &[&str] = &[
    "genes.ext.stats",
    "genes.ext.render.stats",
    "genes.ext.combinator.stats",
    "genes.ext.lifecycle.stats",
    "genes.ext.save.stats",
];
const DEFAULT_DRYRUN_OPS: &[&str] = &[
    "genes.ext.dryrun",
    "genes.ext.render.dryrun",
    "genes.ext.combinator.dryrun",
    "genes.ext.lifecycle.dryrun",
    "genes.ext.save.dryrun",
];

fn env_csv(key: &str, default: &[&str]) -> Vec<String> {
    match std::env::var(key) {
        Ok(s) => s
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Err(_) => default.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn snapshot_all_stats_and_dryruns() {
    let Some(game) = common::launch("render_stats") else {
        return;
    };
    let stats_ops = env_csv("HORSEY_STATS_OPS", DEFAULT_STATS_OPS);
    let dryrun_ops = env_csv("HORSEY_DRYRUN_OPS", DEFAULT_DRYRUN_OPS);
    let expect_armed: Vec<String> = env_csv("HORSEY_EXPECT_ARMED", &[])
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();

    eprintln!("\n=== STATS ===");
    for op in &stats_ops {
        let r = game.op_json(op, &json!({})).unwrap_or(Value::Null);
        let res = r.get("result").unwrap_or(&r);
        let err = r.get("error").and_then(Value::as_str).unwrap_or("");
        eprintln!("\n--- {op} ---\nresult: {res}\nerror:  {err}");
        assert!(err.is_empty(), "stats op {op} returned error: {err}");
        assert!(
            r.get("result").is_some(),
            "stats op {op} returned no result"
        );

        // Optional armed-check: if the op name is in expect_armed (by
        // full op name OR by subsystem prefix), require armed=true.
        if expect_armed.iter().any(|e| op.contains(e)) {
            let armed = res
                .get("armed")
                .and_then(Value::as_bool)
                .or_else(|| res.get("armed_ctor").and_then(Value::as_bool));
            assert_eq!(
                armed,
                Some(true),
                "expected {op} armed=true, got result {res}"
            );
        }
    }

    eprintln!("\n=== DRYRUN ===");
    for op in &dryrun_ops {
        let r = game.op_json(op, &json!({})).unwrap_or(Value::Null);
        let res = r.get("result").unwrap_or(&r);
        let err = r.get("error").and_then(Value::as_str).unwrap_or("");
        eprintln!("\n--- {op} ---\nresult: {res}\nerror:  {err}");
        assert!(err.is_empty(), "dryrun op {op} returned error: {err}");
        assert!(
            r.get("result").is_some(),
            "dryrun op {op} returned no result"
        );
    }

    game.pass(&format!(
        "snapshot ok: {} stats + {} dryrun ops well-formed",
        stats_ops.len(),
        dryrun_ops.len()
    ));
}
