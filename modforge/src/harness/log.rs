//! Per-test run log written to `target/test-runs/<name>-<ts>.log`.
//!
//! Every harness step (build, kill, steam launch, process detect,
//! injector, HTTP probe, op POSTs) writes a timestamped line, flushed
//! immediately so you can `tail -f` the file while a long test is
//! running. The log survives the test process; on failure you have a
//! permanent record of the sequence of events.
//!
//! The file path is also printed to stderr at the start of every
//! launch so it shows up in `cargo test --nocapture` output.

use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// One per-test log. Cheap to clone (Arc<Mutex<File>> + PathBuf).
#[derive(Clone)]
pub struct TestLog {
    inner: Arc<Mutex<TestLogInner>>,
    path: PathBuf,
}

struct TestLogInner {
    file: File,
}

impl TestLog {
    /// Open a log under `<workspace>/target/test-runs/`. `name` is
    /// the test name; we append a UTC timestamp so multiple runs
    /// don't overwrite each other.
    pub fn open(workspace_dir: &Path, name: &str) -> anyhow::Result<Self> {
        let dir = workspace_dir.join("target").join("test-runs");
        std::fs::create_dir_all(&dir)?;
        let ts = utc_timestamp();
        let safe_name = sanitize(name);
        let path = dir.join(format!("{safe_name}-{ts}.log"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)?;
        let log = Self {
            inner: Arc::new(Mutex::new(TestLogInner { file })),
            path: path.clone(),
        };
        eprintln!("[harness] test log: {}", path.display());
        log.write_line("HARNESS", &format!("opened log {}", path.display()));
        Ok(log)
    }

    /// Write one timestamped event line. Format:
    /// `2026-05-15T13:22:04.123Z [STAGE] message`.
    pub fn event(&self, stage: &str, message: &str) {
        self.write_line(stage, message);
        eprintln!("[harness][{stage}] {message}");
    }

    /// Write without printing to stderr (use for high-volume events
    /// like per-poll HTTP probe attempts).
    pub fn event_quiet(&self, stage: &str, message: &str) {
        self.write_line(stage, message);
    }

    /// Mark a test result. Conventional stages: `PASS`, `FAIL`.
    pub fn result(&self, status: &str, message: &str) {
        let stage = if status.eq_ignore_ascii_case("pass") {
            "PASS"
        } else {
            "FAIL"
        };
        self.write_line(stage, message);
        eprintln!("[harness][{stage}] {message}");
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn write_line(&self, stage: &str, message: &str) {
        let line = format!("{} [{stage}] {message}\n", utc_iso_now());
        let mut guard = self.inner.lock();
        let _ = guard.file.write_all(line.as_bytes());
        let _ = guard.file.flush();
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn utc_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Compact, sortable, filename-safe: YYYY-MM-DD_HHMMSS.
    format_unix_compact(now)
}

fn utc_iso_now() -> String {
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let ms = d.subsec_millis();
    format!("{}.{ms:03}Z", format_unix_iso(secs))
}

// Plain Gregorian date math; no chrono dep. Good enough for log
// filenames + line stamps; not for anything load-bearing.
fn split_unix(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let mut sod = (secs % 86_400) as u32;
    let hh = sod / 3600;
    sod %= 3600;
    let mm = sod / 60;
    let ss = sod % 60;
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, hh, mm, ss)
}

// Howard Hinnant's algorithm, public domain.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn format_unix_compact(secs: u64) -> String {
    let (y, mo, d, hh, mm, ss) = split_unix(secs);
    format!("{y:04}-{mo:02}-{d:02}_{hh:02}{mm:02}{ss:02}")
}

fn format_unix_iso(secs: u64) -> String {
    let (y, mo, d, hh, mm, ss) = split_unix(secs);
    format!("{y:04}-{mo:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}")
}
