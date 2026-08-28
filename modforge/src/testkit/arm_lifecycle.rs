//! Generic "arm a detour subsystem, idle, assert stats, disarm" test
//! flow. Captures the pattern shared by horsey-mod's `arm_combinator`,
//! `arm_render_trampoline`, and similar single-subsystem hook tests.
//!
//! The pattern is six steps:
//!   1. dryrun (precheck the target address; refuse to arm if INT3).
//!   2. arm (assert `armed: true`).
//!   3. idle a configured duration so incidental fires accumulate.
//!   4. stats (assert each named counter is below `max_value`).
//!   5. disarm (assert `armed: false`).
//!   6. ping (confirm the game is still alive).
//!
//! Any native-PE mod whose hook-lifecycle ops follow the same
//! envelope (`{result: {armed: bool}}` for arm/disarm, `{result: {<counters>}}`
//! for stats, `{result: {prologue_bytes: "hex string"}}` for dryrun)
//! can use this directly.

use crate::harness::RunningGame;
use crate::testkit::fn_entry;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::time::Duration;

/// One subsystem's cmdlet names. The four ops MUST follow the modforge
/// dryrun/arm/stats/disarm envelope.
#[derive(Debug, Clone)]
pub struct SubsystemOps {
    pub dryrun: String,
    pub arm: String,
    pub stats: String,
    pub disarm: String,
}

impl SubsystemOps {
    /// Build the four op names from a common prefix. `prefix.dryrun`,
    /// `prefix.arm`, `prefix.stats`, `prefix.disarm`.
    pub fn from_prefix(prefix: &str) -> Self {
        Self {
            dryrun: format!("{prefix}.dryrun"),
            arm: format!("{prefix}.arm"),
            stats: format!("{prefix}.stats"),
            disarm: format!("{prefix}.disarm"),
        }
    }
}

/// One counter expected in the stats response with an upper bound.
/// `max` is the "runaway loop" guard (e.g. 1_000_000 for a per-event
/// counter; 100_000_000 for a per-frame counter).
#[derive(Debug, Clone)]
pub struct CounterBound {
    pub key: String,
    pub max: u64,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub ops: SubsystemOps,
    /// Human label used in log lines / pass messages.
    pub name: String,
    /// Seconds to idle between arm and stats. Default 3.
    pub idle: Duration,
    /// Counter sanity bounds. Each is asserted `value <= max`.
    pub counter_bounds: Vec<CounterBound>,
    /// Dot-paths into the dryrun response holding the prologue hex
    /// string. Default `["prologue_bytes"]` (single-target subsystem).
    /// For multi-target subsystems (e.g. lifecycle's ctor + dtor pair):
    /// `["ctor.prologue_bytes", "dtor.prologue_bytes"]`.
    pub prologue_paths: Vec<String>,
    /// Dot-paths into the arm/disarm response holding the armed bool.
    /// Default `["armed"]` (single-flag). For multi-flag subsystems:
    /// `["armed_ctor", "armed_dtor"]`.
    pub armed_paths: Vec<String>,
}

impl Config {
    /// Sensible defaults for a single-target subsystem.
    pub fn defaults(ops: SubsystemOps, name: impl Into<String>) -> Self {
        Self {
            ops,
            name: name.into(),
            idle: Duration::from_secs(3),
            counter_bounds: Vec::new(),
            prologue_paths: vec!["prologue_bytes".into()],
            armed_paths: vec!["armed".into()],
        }
    }
}

fn walk_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for part in path.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

#[derive(Debug)]
pub struct ArmReport {
    /// All prologues checked, in `cfg.prologue_paths` order.
    pub prologue_bytes: Vec<String>,
    pub stats: Value,
}

/// Run the dryrun/arm/idle/stats/disarm/ping flow.
pub fn run(game: &RunningGame, cfg: &Config) -> Result<ArmReport> {
    // 1. dryrun: walk every prologue_path
    let dr = game
        .op_json(&cfg.ops.dryrun, &json!({}))
        .map_err(|e| anyhow!("{} dryrun: {e}", cfg.name))?;
    let dr_result = dr
        .get("result")
        .ok_or_else(|| anyhow!("{} dryrun: missing result", cfg.name))?;
    let mut prologues = Vec::new();
    for path in &cfg.prologue_paths {
        let prologue = walk_path(dr_result, path)
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{} dryrun: missing {path}", cfg.name))?
            .to_string();
        eprintln!("[PRECHECK] {}.{path}: [{prologue}]", cfg.name);
        let bytes: Vec<u8> = prologue
            .split_whitespace()
            .filter_map(|t| u8::from_str_radix(t, 16).ok())
            .collect();
        if bytes.first() == Some(&0xcc) {
            return Err(anyhow!(
                "{}.{path} dryrun shows INT3 padding; refusing to arm",
                cfg.name
            ));
        }
        if !fn_entry::is_msvc_x64_prologue_loose(&bytes) {
            return Err(anyhow!(
                "{}.{path} prologue not MSVC-shaped: first 4 = {:02x?}",
                cfg.name,
                &bytes[..bytes.len().min(4)]
            ));
        }
        prologues.push(prologue);
    }

    // 2. arm: assert every armed_path is true
    let arm = game
        .op_json(&cfg.ops.arm, &json!({}))
        .map_err(|e| anyhow!("{} arm: {e}", cfg.name))?;
    let arm_result = arm
        .get("result")
        .ok_or_else(|| anyhow!("{} arm: missing result", cfg.name))?;
    for path in &cfg.armed_paths {
        let armed = walk_path(arm_result, path)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !armed {
            return Err(anyhow!(
                "{}.{path} did not report true after arm: {arm}",
                cfg.name
            ));
        }
    }
    eprintln!("[ARM] {} armed", cfg.name);

    // 3. idle
    eprintln!(
        "[WAIT] {} idling {}s for incidental fires",
        cfg.name,
        cfg.idle.as_secs()
    );
    std::thread::sleep(cfg.idle);

    // 4. stats
    let stats = game
        .op_json(&cfg.ops.stats, &json!({}))
        .map_err(|e| anyhow!("{} stats: {e}", cfg.name))?;
    let s = stats
        .get("result")
        .ok_or_else(|| anyhow!("{} stats: missing result", cfg.name))?
        .clone();
    eprintln!("[STATS] {} {s}", cfg.name);
    for bound in &cfg.counter_bounds {
        let v = walk_path(&s, &bound.key)
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("{} stats: missing counter {}", cfg.name, bound.key))?;
        if v > bound.max {
            return Err(anyhow!(
                "{} {} = {v} exceeds bound {}",
                cfg.name,
                bound.key,
                bound.max
            ));
        }
    }

    // 5. disarm: assert every armed_path is false
    let disarm = game
        .op_json(&cfg.ops.disarm, &json!({}))
        .map_err(|e| anyhow!("{} disarm: {e}", cfg.name))?;
    let disarm_result = disarm
        .get("result")
        .ok_or_else(|| anyhow!("{} disarm: missing result", cfg.name))?;
    for path in &cfg.armed_paths {
        let armed = walk_path(disarm_result, path)
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if armed {
            return Err(anyhow!(
                "{}.{path} still true after disarm: {disarm}",
                cfg.name
            ));
        }
    }
    eprintln!("[DISARM] {} disarmed", cfg.name);

    // 6. ping
    game.op_json("ping", &json!({}))
        .context("game must still respond to ping after disarm")?;

    Ok(ArmReport {
        prologue_bytes: prologues,
        stats: s,
    })
}
