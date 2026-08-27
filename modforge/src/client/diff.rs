//! Snapshot-delta helpers. Every UE5 mod that exposes the
//! standard `ProcessSnapshot` block (counters + process_memory +
//! process_cpu + process_threads + game_population) gets
//! turn-key perf / leak diff analysis: snapshot at T0, snapshot
//! at T1, format the table.
//!
//! ```ignore
//! use ueforge::client::diff::MetricsSnapshot;
//!
//! let s0 = MetricsSnapshot::from_api(&api);
//! std::thread::sleep(Duration::from_secs(30));
//! let s1 = MetricsSnapshot::from_api(&api);
//!
//! eprintln!("{}", s0.diff_all(&s1));
//! ```
//!
//! Replaces ~700 lines of hand-rolled JSON-poking + delta math
//! across the `explore_perf_counters` / `explore_perf_timeseries` /
//! `explore_leak_source` / `explore_thread_attribution` tests.

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::time::Instant;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::Api;

/// A single point-in-time capture of the standard mod snapshot.
/// Holds the raw JSON so we don't need a typed `Snapshot` shape
/// here. Every consumer's snapshot embeds `ProcessSnapshot`
/// (via `#[serde(flatten)]`), so the standard fields are at the
/// top level of any concrete `S`.
pub struct MetricsSnapshot {
    pub at: Instant,
    pub raw: Value,
}

impl MetricsSnapshot {
    /// Build from a typed snapshot value. Most mods' snapshots
    /// derive `Serialize`; pass `&api.snapshot()`.
    pub fn from_snapshot<S: Serialize>(s: &S) -> Self {
        let raw = serde_json::to_value(s).unwrap_or(Value::Null);
        Self {
            at: Instant::now(),
            raw,
        }
    }

    /// Convenience: snapshot the api right now and build. Uses
    /// `Api::snapshot_value` so no `Serialize` bound is required
    /// on the per-mod `Snapshot` type.
    pub fn from_api<S: DeserializeOwned>(api: &Api<S>) -> Self {
        Self {
            at: Instant::now(),
            raw: api.snapshot_value(),
        }
    }

    /// Build from a raw `Value` (e.g. from a manually-constructed
    /// op response).
    pub fn from_value(raw: Value) -> Self {
        Self {
            at: Instant::now(),
            raw,
        }
    }

    fn obj(&self, key: &str) -> Option<&serde_json::Map<String, Value>> {
        self.raw.get(key)?.as_object()
    }

    fn u64_at(map: &serde_json::Map<String, Value>, k: &str) -> u64 {
        map.get(k).and_then(|v| v.as_u64()).unwrap_or(0)
    }

    /// Diff every section. The `Display` impl prints all
    /// sections in the order the existing tests use.
    pub fn diff_all(&self, later: &MetricsSnapshot) -> SystemDiff {
        SystemDiff {
            elapsed_secs: later.at.duration_since(self.at).as_secs_f64(),
            counters: self.diff_counters(later),
            memory: self.diff_memory(later),
            cpu: self.diff_cpu(later),
            threads: self.diff_threads(later),
            population: self.diff_population(later),
        }
    }

    pub fn diff_counters(&self, later: &MetricsSnapshot) -> CounterDiff {
        let elapsed_secs = later.at.duration_since(self.at).as_secs_f64().max(1e-9);
        let c0 = self.obj("counters").cloned().unwrap_or_default();
        let c1 = later.obj("counters").cloned().unwrap_or_default();

        let mut rates: Vec<RateRow> = c0
            .keys()
            .filter(|k| !k.starts_with("time_ns_") && !k.ends_with("_peak"))
            .map(|k| {
                let v0 = Self::u64_at(&c0, k);
                let v1 = Self::u64_at(&c1, k);
                let delta = v1.saturating_sub(v0);
                RateRow {
                    name: k.clone(),
                    delta,
                    per_sec: delta as f64 / elapsed_secs,
                }
            })
            .collect();
        rates.sort_by(|a, b| {
            b.per_sec
                .partial_cmp(&a.per_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut times: Vec<TimeRow> = c0
            .keys()
            .filter(|k| k.starts_with("time_ns_"))
            .map(|k| {
                let v0 = Self::u64_at(&c0, k);
                let v1 = Self::u64_at(&c1, k);
                let ns = v1.saturating_sub(v0);
                TimeRow {
                    name: k.clone(),
                    ns,
                    pct_wall: (ns as f64) / (elapsed_secs * 1_000_000_000.0) * 100.0,
                }
            })
            .collect();
        times.sort_by(|a, b| b.ns.cmp(&a.ns));
        let our_total_ns = times.iter().map(|t| t.ns).sum();

        CounterDiff {
            elapsed_secs,
            rates,
            times,
            our_total_ns,
        }
    }

    pub fn diff_memory(&self, later: &MetricsSnapshot) -> MemoryDiff {
        let m0 = self.obj("process_memory").cloned().unwrap_or_default();
        let m1 = later.obj("process_memory").cloned().unwrap_or_default();
        let elapsed_secs = later.at.duration_since(self.at).as_secs_f64().max(1e-9);
        MemoryDiff {
            elapsed_secs,
            working_set_before: Self::u64_at(&m0, "working_set_size"),
            working_set_after: Self::u64_at(&m1, "working_set_size"),
            private_before: Self::u64_at(&m0, "private_usage"),
            private_after: Self::u64_at(&m1, "private_usage"),
            page_faults_before: Self::u64_at(&m0, "page_fault_count"),
            page_faults_after: Self::u64_at(&m1, "page_fault_count"),
        }
    }

    pub fn diff_cpu(&self, later: &MetricsSnapshot) -> CpuDiff {
        let c0 = self.obj("process_cpu").cloned().unwrap_or_default();
        let c1 = later.obj("process_cpu").cloned().unwrap_or_default();
        let elapsed_secs = later.at.duration_since(self.at).as_secs_f64().max(1e-9);
        CpuDiff {
            elapsed_secs,
            user_ns: Self::u64_at(&c1, "process_cpu_user_ns")
                .saturating_sub(Self::u64_at(&c0, "process_cpu_user_ns")),
            kernel_ns: Self::u64_at(&c1, "process_cpu_kernel_ns")
                .saturating_sub(Self::u64_at(&c0, "process_cpu_kernel_ns")),
            total_ns: Self::u64_at(&c1, "process_cpu_total_ns")
                .saturating_sub(Self::u64_at(&c0, "process_cpu_total_ns")),
        }
    }

    pub fn diff_threads(&self, later: &MetricsSnapshot) -> ThreadsDiff {
        let elapsed_secs = later.at.duration_since(self.at).as_secs_f64().max(1e-9);
        let t0 = self
            .raw
            .get("process_threads")
            .and_then(|v| v.get("threads"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let t1 = later
            .raw
            .get("process_threads")
            .and_then(|v| v.get("threads"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut by_tid_0: HashMap<u64, u64> = HashMap::new();
        for t in &t0 {
            if let (Some(tid), Some(cpu)) = (
                t.get("tid").and_then(|v| v.as_u64()),
                t.get("cpu_total_ns").and_then(|v| v.as_u64()),
            ) {
                by_tid_0.insert(tid, cpu);
            }
        }
        let mut deltas: Vec<ThreadRow> = Vec::new();
        for t in &t1 {
            let tid = t.get("tid").and_then(|v| v.as_u64()).unwrap_or(0);
            let name = t
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let cpu1 = t.get("cpu_total_ns").and_then(|v| v.as_u64()).unwrap_or(0);
            let cpu0 = by_tid_0.get(&tid).copied().unwrap_or(0);
            let delta = cpu1.saturating_sub(cpu0);
            if delta > 0 {
                deltas.push(ThreadRow { name, ns: delta });
            }
        }
        deltas.sort_by(|a, b| b.ns.cmp(&a.ns));
        ThreadsDiff {
            elapsed_secs,
            rows: deltas,
        }
    }

    pub fn diff_population(&self, later: &MetricsSnapshot) -> PopulationDiff {
        let p0 = self
            .raw
            .get("game_population")
            .cloned()
            .unwrap_or(Value::Null);
        let p1 = later
            .raw
            .get("game_population")
            .cloned()
            .unwrap_or(Value::Null);
        let total_before = p0
            .get("gobjects_total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total_after = p1
            .get("gobjects_total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut count_0: HashMap<String, u64> = HashMap::new();
        if let Some(arr) = p0.get("top_classes").and_then(|v| v.as_array()) {
            for c in arr {
                if let (Some(name), Some(count)) = (
                    c.get("class").and_then(|v| v.as_str()),
                    c.get("count").and_then(|v| v.as_u64()),
                ) {
                    count_0.insert(name.to_string(), count);
                }
            }
        }
        let mut classes: Vec<ClassDeltaRow> = Vec::new();
        if let Some(arr) = p1.get("top_classes").and_then(|v| v.as_array()) {
            for c in arr {
                let name = c
                    .get("class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string();
                let count1 = c.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
                let count0 = count_0.get(&name).copied().unwrap_or(0);
                let delta = (count1 as i64) - (count0 as i64);
                classes.push(ClassDeltaRow {
                    name,
                    delta,
                    now: count1,
                });
            }
        }
        classes.sort_by(|a, b| b.delta.cmp(&a.delta));
        PopulationDiff {
            total_before,
            total_after,
            classes,
        }
    }
}

// ----------------------------------------------------------------
// Diff types + Display impls. Tests just `writeln!(out, "{}", diff)`.
// ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RateRow {
    pub name: String,
    pub delta: u64,
    pub per_sec: f64,
}

#[derive(Debug, Clone)]
pub struct TimeRow {
    pub name: String,
    pub ns: u64,
    pub pct_wall: f64,
}

#[derive(Debug, Clone)]
pub struct CounterDiff {
    pub elapsed_secs: f64,
    pub rates: Vec<RateRow>,
    pub times: Vec<TimeRow>,
    pub our_total_ns: u64,
}

impl Display for CounterDiff {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "\n=== Event rate counters over {:.1}s ===",
            self.elapsed_secs
        )?;
        writeln!(f, "{:<40} {:>12} {:>12}", "counter", "delta", "per second")?;
        writeln!(f, "{}", "-".repeat(70))?;
        for r in &self.rates {
            writeln!(f, "{:<40} {:>12} {:>12.1}", r.name, r.delta, r.per_sec)?;
        }
        writeln!(
            f,
            "\n=== CPU time spent in OUR mod over {:.1}s ===",
            self.elapsed_secs
        )?;
        writeln!(f, "{:<40} {:>14} {:>10}", "hot path", "ns total", "% wall")?;
        writeln!(f, "{}", "-".repeat(70))?;
        for t in &self.times {
            writeln!(f, "{:<40} {:>14} {:>9.3}%", t.name, t.ns, t.pct_wall)?;
        }
        writeln!(
            f,
            "{:<40} {:>14} {:>9.3}%",
            "  (sum)",
            self.our_total_ns,
            (self.our_total_ns as f64) / (self.elapsed_secs * 1_000_000_000.0) * 100.0
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryDiff {
    pub elapsed_secs: f64,
    pub working_set_before: u64,
    pub working_set_after: u64,
    pub private_before: u64,
    pub private_after: u64,
    pub page_faults_before: u64,
    pub page_faults_after: u64,
}

impl MemoryDiff {
    pub fn working_set_delta_mb(&self) -> f64 {
        ((self.working_set_after as i128) - (self.working_set_before as i128)) as f64
            / (1024.0 * 1024.0)
    }
    pub fn private_delta_mb(&self) -> f64 {
        ((self.private_after as i128) - (self.private_before as i128)) as f64 / (1024.0 * 1024.0)
    }
}

impl Display for MemoryDiff {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "\n=== Process memory delta over {:.1}s ===",
            self.elapsed_secs
        )?;
        writeln!(
            f,
            "  working_set_size:   {:>13} -> {:>13}  delta {:+} ({:+.2} MB)",
            self.working_set_before,
            self.working_set_after,
            (self.working_set_after as i128) - (self.working_set_before as i128),
            self.working_set_delta_mb()
        )?;
        writeln!(
            f,
            "  private_usage:      {:>13} -> {:>13}  delta {:+} ({:+.2} MB)",
            self.private_before,
            self.private_after,
            (self.private_after as i128) - (self.private_before as i128),
            self.private_delta_mb()
        )?;
        let pf_delta = (self.page_faults_after as i128) - (self.page_faults_before as i128);
        writeln!(
            f,
            "  page_fault_count:   {:>13} -> {:>13}  delta {:+} ({:+.1}/sec)",
            self.page_faults_before,
            self.page_faults_after,
            pf_delta,
            pf_delta as f64 / self.elapsed_secs.max(1e-9)
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CpuDiff {
    pub elapsed_secs: f64,
    pub user_ns: u64,
    pub kernel_ns: u64,
    pub total_ns: u64,
}

impl CpuDiff {
    pub fn pct_wall(&self) -> f64 {
        (self.total_ns as f64) / (self.elapsed_secs * 1_000_000_000.0) * 100.0
    }
}

impl Display for CpuDiff {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n=== Process CPU over {:.1}s ===", self.elapsed_secs)?;
        let scale = self.elapsed_secs * 1_000_000_000.0;
        writeln!(
            f,
            "  process_cpu_user_ns:    {} ns ({:.3}% of wall)",
            self.user_ns,
            (self.user_ns as f64) / scale * 100.0
        )?;
        writeln!(
            f,
            "  process_cpu_kernel_ns:  {} ns ({:.3}% of wall)",
            self.kernel_ns,
            (self.kernel_ns as f64) / scale * 100.0
        )?;
        writeln!(
            f,
            "  process_cpu_total_ns:   {} ns ({:.3}% of wall)",
            self.total_ns,
            self.pct_wall()
        )
    }
}

#[derive(Debug, Clone)]
pub struct ThreadRow {
    pub name: String,
    pub ns: u64,
}

#[derive(Debug, Clone)]
pub struct ThreadsDiff {
    pub elapsed_secs: f64,
    pub rows: Vec<ThreadRow>,
}

impl ThreadsDiff {
    /// Top N rows by CPU delta.
    pub fn top(&self, n: usize) -> impl Iterator<Item = &ThreadRow> {
        self.rows.iter().take(n)
    }
}

impl Display for ThreadsDiff {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n=== Top threads by CPU time delta ===")?;
        writeln!(f, "{:<48} {:>14} {:>9}", "thread", "ns", "% wall")?;
        writeln!(f, "{}", "-".repeat(74))?;
        let scale = self.elapsed_secs * 1_000_000_000.0;
        for r in self.rows.iter().take(15) {
            let pct = (r.ns as f64) / scale * 100.0;
            writeln!(f, "{:<48} {:>14} {:>8.2}%", r.name, r.ns, pct)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ClassDeltaRow {
    pub name: String,
    pub delta: i64,
    pub now: u64,
}

#[derive(Debug, Clone)]
pub struct PopulationDiff {
    pub total_before: u64,
    pub total_after: u64,
    pub classes: Vec<ClassDeltaRow>,
}

impl PopulationDiff {
    /// Top N classes by growth delta.
    pub fn top_growers(&self, n: usize) -> impl Iterator<Item = &ClassDeltaRow> {
        self.classes.iter().take(n)
    }
}

impl Display for PopulationDiff {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n=== UE5 GObjects population ===")?;
        writeln!(
            f,
            "  total: {} -> {}  delta {:+}",
            self.total_before,
            self.total_after,
            (self.total_after as i64) - (self.total_before as i64)
        )?;
        writeln!(
            f,
            "\n=== Top growing UE classes (count delta over window) ==="
        )?;
        writeln!(f, "{:<48} {:>10} {:>10}", "class", "delta", "now")?;
        writeln!(f, "{}", "-".repeat(70))?;
        for r in self.classes.iter().take(20) {
            writeln!(f, "{:<48} {:>+10} {:>10}", r.name, r.delta, r.now)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SystemDiff {
    pub elapsed_secs: f64,
    pub counters: CounterDiff,
    pub memory: MemoryDiff,
    pub cpu: CpuDiff,
    pub threads: ThreadsDiff,
    pub population: PopulationDiff,
}

impl Display for SystemDiff {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.counters)?;
        write!(f, "{}", self.cpu)?;
        write!(f, "{}", self.memory)?;
        write!(f, "{}", self.threads)?;
        write!(f, "{}", self.population)?;
        Ok(())
    }
}

// ----------------------------------------------------------------
// Time-series sampler. Repeatedly snapshots and accumulates
// per-tick deltas. Used by `explore_perf_timeseries`-shaped
// tests.
// ----------------------------------------------------------------

/// One sample row in a time-series capture.
#[derive(Debug, Clone)]
pub struct SampleRow {
    pub t_secs: f32,
    pub working_set: u64,
    pub working_set_delta: i64,
    pub page_faults: u64,
    pub page_faults_delta: i64,
    pub gobjects_total: u64,
    pub gobjects_delta: i64,
}

/// A series of `SampleRow`s. Build via [`MetricsSnapshot::sample_series`]
/// or by manually pushing rows.
#[derive(Debug, Default, Clone)]
pub struct SampleSeries {
    pub rows: Vec<SampleRow>,
}

impl Display for SampleSeries {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:>4}  {:>10}  {:>14}  {:>12}  {:>11}  {:>11}",
            "t", "ws_delta", "ws_total_mb", "pf_delta", "gobj_delta", "gobj_total"
        )?;
        writeln!(f, "{}", "-".repeat(72))?;
        for r in &self.rows {
            writeln!(
                f,
                "{:>4.0}  {:>+10}  {:>14.2}  {:>+12}  {:>+11}  {:>11}",
                r.t_secs,
                r.working_set_delta,
                (r.working_set as f64) / (1024.0 * 1024.0),
                r.page_faults_delta,
                r.gobjects_delta,
                r.gobjects_total
            )?;
        }
        Ok(())
    }
}

impl MetricsSnapshot {
    /// Sample the api at a fixed interval for `count` samples,
    /// returning a [`SampleSeries`] of per-tick deltas. Sleeps
    /// `interval` between calls. The first row's delta fields
    /// are zero (no prior sample to diff against).
    pub fn sample_series<S: DeserializeOwned>(
        api: &Api<S>,
        count: usize,
        interval: std::time::Duration,
    ) -> SampleSeries {
        let mut series = SampleSeries::default();
        let start = Instant::now();
        let mut prev_ws: u64 = 0;
        let mut prev_pf: u64 = 0;
        let mut prev_gobj: u64 = 0;
        let mut first = true;
        for _ in 0..count {
            let snap = MetricsSnapshot::from_api(api);
            let m = snap.obj("process_memory").cloned().unwrap_or_default();
            let ws = MetricsSnapshot::u64_at(&m, "working_set_size");
            let pf = MetricsSnapshot::u64_at(&m, "page_fault_count");
            let gobj = snap
                .raw
                .get("game_population")
                .and_then(|v| v.get("gobjects_total"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let row = SampleRow {
                t_secs: start.elapsed().as_secs_f32(),
                working_set: ws,
                working_set_delta: if first {
                    0
                } else {
                    (ws as i64) - (prev_ws as i64)
                },
                page_faults: pf,
                page_faults_delta: if first {
                    0
                } else {
                    (pf as i64) - (prev_pf as i64)
                },
                gobjects_total: gobj,
                gobjects_delta: if first {
                    0
                } else {
                    (gobj as i64) - (prev_gobj as i64)
                },
            };
            series.rows.push(row);
            prev_ws = ws;
            prev_pf = pf;
            prev_gobj = gobj;
            first = false;
            std::thread::sleep(interval);
        }
        series
    }
}
