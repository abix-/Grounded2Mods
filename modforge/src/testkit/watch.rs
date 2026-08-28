//! Memory watch primitives: region (poll a window of bytes for changes)
//! and single-value (poll one u8/u32/u64 for transitions). Both
//! support an optional cmdlet trigger (fired BEFORE polling starts)
//! and an optional transition assertion.
//!
//! Game-specific base resolution (`horse_ptr`, `gamestate`, etc.) is
//! the caller's job. Configs carry a resolved `u64` base; the env
//! parser exposes the raw `base_spec` string for the caller to resolve.

use crate::harness::RunningGame;
use crate::research;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

// ============================================================================
// Region watch
// ============================================================================

/// Caller-resolved config for `run_region`.
#[derive(Debug, Clone)]
pub struct RegionConfig {
    pub base: u64,
    pub offset: usize,
    pub len: usize,
    pub hz: u64,
    pub timeout_ms: u64,
    pub trigger: Option<Trigger>,
    pub expect: Option<RegionExpect>,
}

/// Cmdlet to fire BEFORE the poll loop starts.
#[derive(Debug, Clone)]
pub struct Trigger {
    pub op: String,
    pub args: Value,
}

/// Assertion that a specific byte transition occurs during the poll
/// window. Offset is relative to `RegionConfig.offset`.
#[derive(Debug, Clone, Copy)]
pub struct RegionExpect {
    pub offset: usize,
    pub before: u8,
    pub after: u8,
}

/// Env-parsed shape for region watch. Caller resolves `base_spec` to
/// a u64 via game-specific code, then calls [`EnvRegionConfig::resolve`].
#[derive(Debug, Clone)]
pub struct EnvRegionConfig {
    pub base_spec: String,
    pub offset: usize,
    pub len: usize,
    pub hz: u64,
    pub timeout_ms: u64,
    pub trigger: Option<Trigger>,
    pub expect: Option<RegionExpect>,
}

impl EnvRegionConfig {
    /// Parse from env vars. With `prefix="HORSEY_WATCH"`:
    /// - `HORSEY_WATCH_BASE` (default `horse_ptr`)
    /// - `HORSEY_WATCH_OFFSET` (default `0x1f0`)
    /// - `HORSEY_WATCH_LEN` (default `64`)
    /// - `HORSEY_WATCH_HZ` (default `10`)
    /// - `HORSEY_WATCH_TIMEOUT_MS` (default `2000`)
    /// - `HORSEY_WATCH_TRIGGER` (cmdlet name; optional)
    /// - `HORSEY_WATCH_TRIGGER_ARGS` (JSON object; default `{}`)
    /// - `HORSEY_WATCH_EXPECT` (`<offset>:<before>-><after>` hex)
    pub fn from_env(prefix: &str) -> Result<Self> {
        let base_spec =
            std::env::var(format!("{prefix}_BASE")).unwrap_or_else(|_| "horse_ptr".into());
        let offset_s = std::env::var(format!("{prefix}_OFFSET")).unwrap_or_else(|_| "0x1f0".into());
        let offset = parse_hex_or_dec(&offset_s).context("OFFSET parse")?;
        let len: usize = parse_env(prefix, "LEN", 64)?;
        if !(1..=4096).contains(&len) {
            return Err(anyhow!("LEN out of range: {len}"));
        }
        let hz: u64 = parse_env(prefix, "HZ", 10)?;
        if !(1..=100).contains(&hz) {
            return Err(anyhow!("HZ out of range: {hz}"));
        }
        let timeout_ms: u64 = parse_env(prefix, "TIMEOUT_MS", 2000)?;

        let trigger = match std::env::var(format!("{prefix}_TRIGGER")) {
            Ok(op) => {
                let args: Value = match std::env::var(format!("{prefix}_TRIGGER_ARGS")) {
                    Ok(s) => serde_json::from_str(&s)
                        .with_context(|| format!("{prefix}_TRIGGER_ARGS not JSON"))?,
                    Err(_) => json!({}),
                };
                Some(Trigger { op, args })
            }
            Err(_) => None,
        };

        let expect = match std::env::var(format!("{prefix}_EXPECT")) {
            Ok(s) => Some(parse_expect(&s).with_context(|| format!("{prefix}_EXPECT parse"))?),
            Err(_) => None,
        };

        Ok(Self {
            base_spec,
            offset,
            len,
            hz,
            timeout_ms,
            trigger,
            expect,
        })
    }

    /// Bind a resolved base address.
    pub fn resolve(self, base: u64) -> RegionConfig {
        RegionConfig {
            base,
            offset: self.offset,
            len: self.len,
            hz: self.hz,
            timeout_ms: self.timeout_ms,
            trigger: self.trigger,
            expect: self.expect,
        }
    }
}

#[derive(Debug)]
pub struct RegionResult {
    /// (ms_since_trigger, byte_offset, before, after) per observed change.
    pub transitions: Vec<(u64, usize, u8, u8)>,
    pub baseline: Vec<u8>,
    pub final_state: Vec<u8>,
    pub observed_expected: bool,
}

/// Run the region watch loop. Returns Ok(result) regardless of whether
/// the expectation was met; caller can read `observed_expected` and
/// decide. For "fail loudly" semantics use [`RegionResult::assert_or_err`].
pub fn run_region(game: &RunningGame, cfg: &RegionConfig) -> Result<RegionResult> {
    let start_addr = cfg.base + cfg.offset as u64;
    let mode = match (&cfg.trigger, &cfg.expect) {
        (Some(_), Some(_)) => "assertion",
        (Some(_), None) => "discovery-trigger",
        (None, _) => "manual",
    };
    eprintln!(
        "watch::run_region mode={mode} base=0x{:x} offset=0x{:x} \
         window=0x{:x}..0x{:x} len={} hz={} timeout_ms={}",
        cfg.base,
        cfg.offset,
        start_addr,
        start_addr + cfg.len as u64,
        cfg.len,
        cfg.hz,
        cfg.timeout_ms
    );
    if let Some(e) = cfg.expect {
        eprintln!(
            "expect: +0x{:x}: {:02x} -> {:02x}",
            e.offset, e.before, e.after
        );
    }

    let baseline =
        research::read_bytes(game, start_addr, cfg.len as u32).context("baseline read")?;
    eprintln!("\nbaseline:");
    print_hex_block(cfg.offset, &baseline);

    let t0 = Instant::now();
    if let Some(t) = &cfg.trigger {
        eprintln!("\ntrigger: {} args={}", t.op, t.args);
        let r = game
            .op_json(&t.op, &t.args)
            .map_err(|e| anyhow!("trigger {}: {e}", t.op))?;
        eprintln!("trigger response: {r}");
    }

    let mut prev = baseline.clone();
    let mut transitions: Vec<(u64, usize, u8, u8)> = Vec::new();
    let mut observed_expected = false;
    let poll_interval = Duration::from_millis(1000 / cfg.hz);
    let deadline = t0 + Duration::from_millis(cfg.timeout_ms);

    eprintln!("\npoll log:");
    while Instant::now() < deadline {
        std::thread::sleep(poll_interval);
        let now = research::read_bytes(game, start_addr, cfg.len as u32).context("poll read")?;
        let elapsed = t0.elapsed().as_millis() as u64;
        for (i, (&p, &n)) in prev.iter().zip(now.iter()).enumerate() {
            if p != n {
                let off_at = cfg.offset + i;
                eprintln!("  T={elapsed:>5}ms  +0x{off_at:03x}  {p:02x} -> {n:02x}");
                transitions.push((elapsed, off_at, p, n));
                if let Some(e) = cfg.expect {
                    if off_at == e.offset && p == e.before && n == e.after {
                        observed_expected = true;
                    }
                }
            }
        }
        prev = now;
    }

    eprintln!("\nfinal hex view:");
    print_hex_block(cfg.offset, &prev);
    let unique: BTreeSet<usize> = transitions.iter().map(|t| t.1).collect();
    eprintln!(
        "\nsummary: {} transition event(s) across {} unique byte offset(s)",
        transitions.len(),
        unique.len()
    );

    Ok(RegionResult {
        transitions,
        baseline,
        final_state: prev,
        observed_expected,
    })
}

impl RegionResult {
    /// Return an error if `expect` was set but not observed.
    pub fn assert_or_err(&self, expect: Option<RegionExpect>, timeout_ms: u64) -> Result<()> {
        if let Some(e) = expect {
            if !self.observed_expected {
                let unique: BTreeSet<usize> = self.transitions.iter().map(|t| t.1).collect();
                return Err(anyhow!(
                    "expected byte +0x{:x}: {:02x} -> {:02x} not observed within {timeout_ms}ms. \
                     Transitions seen: {} unique offsets ({:?})",
                    e.offset,
                    e.before,
                    e.after,
                    unique.len(),
                    unique
                        .iter()
                        .map(|o| format!("+0x{o:x}"))
                        .collect::<Vec<_>>()
                ));
            }
            eprintln!(
                "\nASSERTION PASSED: +0x{:x}: {:02x} -> {:02x} observed",
                e.offset, e.before, e.after
            );
        }
        Ok(())
    }
}

// ============================================================================
// Single-value watch
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub enum Width {
    U8,
    U32,
    U64,
}

impl Width {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "u8" => Ok(Self::U8),
            "u32" => Ok(Self::U32),
            "u64" => Ok(Self::U64),
            other => Err(anyhow!("width must be u8|u32|u64, got {other}")),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U32 => "u32",
            Self::U64 => "u64",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SingleConfig {
    pub base: u64,
    pub offset: usize,
    pub width: Width,
    pub hz: u64,
    pub budget_ms: u64,
    pub trigger: Option<Trigger>,
    /// Target value after transition.
    pub expect: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct EnvSingleConfig {
    pub base_spec: String,
    pub offset: usize,
    pub width: Width,
    pub hz: u64,
    pub timeout_ms: u64,
    pub max_latency_ms: u64,
    pub trigger: Option<Trigger>,
    pub expect: Option<u64>,
}

impl EnvSingleConfig {
    pub fn from_env(prefix: &str) -> Result<Self> {
        let base_spec =
            std::env::var(format!("{prefix}_BASE")).unwrap_or_else(|_| "horse_ptr".into());
        let offset = parse_hex_or_dec(
            &std::env::var(format!("{prefix}_OFFSET"))
                .map_err(|_| anyhow!("{prefix}_OFFSET required"))?,
        )
        .context("OFFSET parse")?;
        let width = Width::parse(
            &std::env::var(format!("{prefix}_WIDTH")).unwrap_or_else(|_| "u8".into()),
        )?;
        let hz: u64 = parse_env(prefix, "HZ", 20)?;
        if !(1..=100).contains(&hz) {
            return Err(anyhow!("HZ out of range: {hz}"));
        }
        let timeout_ms: u64 = parse_env(prefix, "TIMEOUT_MS", 2000)?;
        let max_latency_ms: u64 = parse_env(prefix, "MAX_LATENCY_MS", 1000)?;

        let trigger = match std::env::var(format!("{prefix}_TRIGGER")) {
            Ok(op) => {
                let args: Value = match std::env::var(format!("{prefix}_TRIGGER_ARGS")) {
                    Ok(s) => serde_json::from_str(&s)
                        .with_context(|| format!("{prefix}_TRIGGER_ARGS not JSON"))?,
                    Err(_) => json!({}),
                };
                Some(Trigger { op, args })
            }
            Err(_) => None,
        };

        let expect = std::env::var(format!("{prefix}_EXPECT")).ok().map(|s| {
            let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
            u64::from_str_radix(t, 16).expect("EXPECT parse")
        });

        Ok(Self {
            base_spec,
            offset,
            width,
            hz,
            timeout_ms,
            max_latency_ms,
            trigger,
            expect,
        })
    }

    pub fn resolve(self, base: u64) -> SingleConfig {
        let budget_ms = if self.trigger.is_some() {
            self.max_latency_ms
        } else {
            self.timeout_ms
        };
        SingleConfig {
            base,
            offset: self.offset,
            width: self.width,
            hz: self.hz,
            budget_ms,
            trigger: self.trigger,
            expect: self.expect,
        }
    }
}

#[derive(Debug)]
pub struct SingleResult {
    pub baseline: u64,
    pub transitions: Vec<(u64, u64, u64)>, // (ms, before, after)
    pub final_value: u64,
    pub matched_at: Option<u64>,
}

pub fn run_single(game: &RunningGame, cfg: &SingleConfig) -> Result<SingleResult> {
    let target_addr = cfg.base + cfg.offset as u64;
    let mode = match (&cfg.trigger, &cfg.expect) {
        (Some(_), Some(_)) => "assertion",
        (Some(_), None) => "discovery-trigger",
        (None, _) => "manual",
    };
    eprintln!(
        "watch::run_single mode={mode} base=0x{:x} offset=0x{:x} \
         addr=0x{:x} width={} hz={}",
        cfg.base,
        cfg.offset,
        target_addr,
        cfg.width.name(),
        cfg.hz
    );

    let baseline = read_value(game, target_addr, cfg.width)?;
    eprintln!(
        "baseline: {} @ 0x{:x} = 0x{:x}",
        cfg.width.name(),
        target_addr,
        baseline
    );
    if let Some(e) = cfg.expect {
        eprintln!("expect:   value -> 0x{e:x}");
    }

    let t0 = Instant::now();
    if let Some(t) = &cfg.trigger {
        eprintln!("trigger: {} args={}", t.op, t.args);
        let r = game
            .op_json(&t.op, &t.args)
            .map_err(|e| anyhow!("trigger {}: {e}", t.op))?;
        eprintln!("trigger response: {r}");
    }

    let poll_interval = Duration::from_millis(1000 / cfg.hz);
    let deadline = t0 + Duration::from_millis(cfg.budget_ms);
    let mut prev = baseline;
    let mut matched_at: Option<u64> = None;
    let mut transitions: Vec<(u64, u64, u64)> = Vec::new();

    eprintln!("\npoll log:");
    while Instant::now() < deadline {
        std::thread::sleep(poll_interval);
        let now_v = read_value(game, target_addr, cfg.width)?;
        let elapsed = t0.elapsed().as_millis() as u64;
        if now_v != prev {
            eprintln!("  T={elapsed:>5}ms  0x{prev:x} -> 0x{now_v:x}");
            transitions.push((elapsed, prev, now_v));
            prev = now_v;
        }
        if let Some(e) = cfg.expect {
            if now_v == e && matched_at.is_none() {
                matched_at = Some(elapsed);
            }
        }
    }

    eprintln!(
        "\nsummary: {} transition(s) over {}ms, final value 0x{:x}",
        transitions.len(),
        cfg.budget_ms,
        prev
    );

    Ok(SingleResult {
        baseline,
        transitions,
        final_value: prev,
        matched_at,
    })
}

impl SingleResult {
    pub fn assert_or_err(&self, expect: Option<u64>, budget_ms: u64) -> Result<()> {
        if let Some(e) = expect {
            match self.matched_at {
                Some(ms) => {
                    eprintln!("ASSERTION PASSED: value reached 0x{e:x} at T={ms}ms");
                    Ok(())
                }
                None => Err(anyhow!(
                    "value 0x{e:x} not observed within {budget_ms}ms. \
                     Final value: 0x{:x}. Transitions: {:?}",
                    self.final_value,
                    self.transitions
                )),
            }
        } else {
            Ok(())
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn read_value(game: &RunningGame, addr: u64, width: Width) -> Result<u64> {
    let resp = game
        .op_json(
            "mem.peek",
            &json!({
                "addr": addr,
                "kind": width.name(),
            }),
        )
        .map_err(|e| anyhow!("mem.peek: {e}"))?;
    let v = resp
        .get("result")
        .and_then(|x| x.get("value"))
        .ok_or_else(|| anyhow!("mem.peek no value: {resp}"))?;
    match width {
        Width::U8 | Width::U32 => v.as_u64().ok_or_else(|| anyhow!("not numeric: {v}")),
        Width::U64 => {
            let s = v.as_str().ok_or_else(|| anyhow!("u64 not string: {v}"))?;
            let h = s.trim_start_matches("0x").trim_start_matches("0X");
            u64::from_str_radix(h, 16).context("u64 hex parse")
        }
    }
}

fn print_hex_block(start_offset: usize, bytes: &[u8]) {
    for (row, chunk) in bytes.chunks(16).enumerate() {
        eprint!("  +0x{:03x}: ", start_offset + row * 16);
        for b in chunk {
            eprint!("{b:02x} ");
        }
        eprintln!();
    }
}

fn parse_hex_or_dec(s: &str) -> Result<usize> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return usize::from_str_radix(h, 16).map_err(|e| anyhow!("hex: {e}"));
    }
    s.parse().map_err(|e| anyhow!("dec: {e}"))
}

fn parse_env<T: std::str::FromStr>(prefix: &str, suffix: &str, default: T) -> Result<T>
where
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match std::env::var(format!("{prefix}_{suffix}")) {
        Ok(s) => s
            .parse()
            .map_err(|e| anyhow!("{prefix}_{suffix} parse: {e}")),
        Err(_) => Ok(default),
    }
}

fn parse_expect(s: &str) -> Result<RegionExpect> {
    let (off_part, rest) = s
        .split_once(':')
        .ok_or_else(|| anyhow!("missing ':' in EXPECT '{s}'"))?;
    let (before_part, after_part) = rest
        .split_once("->")
        .ok_or_else(|| anyhow!("missing '->' in EXPECT '{s}'"))?;
    let offset = parse_hex_or_dec(off_part)?;
    let before = parse_u8_hex(before_part)?;
    let after = parse_u8_hex(after_part)?;
    Ok(RegionExpect {
        offset,
        before,
        after,
    })
}

fn parse_u8_hex(s: &str) -> Result<u8> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(s, 16).map_err(|e| anyhow!("u8 hex: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_expect_basic() {
        let e = parse_expect("0x205:01->00").unwrap();
        assert_eq!(e.offset, 0x205);
        assert_eq!(e.before, 0x01);
        assert_eq!(e.after, 0x00);
    }

    #[test]
    fn parse_expect_decimal_offset() {
        let e = parse_expect("517:1->0").unwrap();
        assert_eq!(e.offset, 517);
        assert_eq!(e.before, 1);
        assert_eq!(e.after, 0);
    }
}
