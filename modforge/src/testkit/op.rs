//! Generic HTTP cmdlet invoker with response-contract assertion.
//!
//! Wraps `game.op_json(name, args)` with a uniform "did this cmdlet
//! satisfy the modforge response contract" check: every response must
//! carry either a `result` field (success) or an `error` field
//! (op-defined failure). Anything else is a contract violation.

use crate::harness::RunningGame;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Config {
    pub op: String,
    pub args: Value,
    /// Dot-separated field path (e.g. `result.ptr`) to assert non-null.
    pub expect_field: Option<String>,
    /// String value to assert at `expect_field`.
    pub expect_value: Option<String>,
}

impl Config {
    pub fn new(op: impl Into<String>) -> Self {
        Self {
            op: op.into(),
            args: json!({}),
            expect_field: None,
            expect_value: None,
        }
    }

    pub fn with_args(mut self, args: Value) -> Self {
        self.args = args;
        self
    }

    /// Parse from env vars. With `prefix="HORSEY_DUMP"`:
    /// - `HORSEY_DUMP_OP` (defaults to `default_op`)
    /// - `HORSEY_DUMP_ARGS` (JSON object)
    /// - `HORSEY_DUMP_EXPECT_FIELD`
    /// - `HORSEY_DUMP_EXPECT_VALUE`
    pub fn from_env(prefix: &str, default_op: &str) -> Result<Self> {
        let op = std::env::var(format!("{prefix}_OP")).unwrap_or_else(|_| default_op.into());
        let args: Value = match std::env::var(format!("{prefix}_ARGS")) {
            Ok(s) => serde_json::from_str(&s).with_context(|| format!("{prefix}_ARGS not JSON"))?,
            Err(_) => json!({}),
        };
        let expect_field = std::env::var(format!("{prefix}_EXPECT_FIELD")).ok();
        let expect_value = std::env::var(format!("{prefix}_EXPECT_VALUE")).ok();
        Ok(Self {
            op,
            args,
            expect_field,
            expect_value,
        })
    }
}

/// Invoke the cmdlet, print response, assert contract + optional
/// field expectations. Returns the parsed response.
pub fn run(game: &RunningGame, cfg: &Config) -> Result<Value> {
    eprintln!("op: {} args: {}", cfg.op, cfg.args);
    let resp = game
        .op_json(&cfg.op, &cfg.args)
        .map_err(|e| anyhow!("op {}: {e}", cfg.op))?;
    eprintln!("\nFULL response: {resp}");

    let has_result = resp.get("result").is_some();
    let has_error = resp.get("error").is_some();
    if !has_result && !has_error {
        return Err(anyhow!(
            "response missing both `result` and `error`; cmdlet contract violation: {resp}"
        ));
    }

    if let Some(path) = &cfg.expect_field {
        let mut cur = &resp;
        for part in path.split('.') {
            cur = cur.get(part).ok_or_else(|| {
                anyhow!("expect_field={path}: missing segment '{part}'. resp={resp}")
            })?;
        }
        if cur.is_null() {
            return Err(anyhow!(
                "expect_field={path}: present but null. resp={resp}"
            ));
        }
        if let Some(want) = &cfg.expect_value {
            let got = cur
                .as_str()
                .ok_or_else(|| anyhow!("expect_field={path} not a string: {cur}"))?;
            if got != want {
                return Err(anyhow!(
                    "expect_value mismatch at {path}: got '{got}' want '{want}'"
                ));
            }
        }
    }

    Ok(resp)
}
