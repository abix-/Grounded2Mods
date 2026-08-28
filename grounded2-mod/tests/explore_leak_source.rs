// Leak-source investigation.
//
// 1. Snapshot T0
// 2. Sleep 30s while game runs
// 3. Snapshot T1
// 4. Print top growing classes (existing)
// 5. Print top packages by hosted-object count (NEW)
// 6. Print loaded levels (NEW)
// 7. For top 5 growing classes, dump K outer-chain samples (NEW)
//   . This names the actual asset paths that are leaking.
//
// Output answers: "+144 SoundWave per 30s" -> WHICH 144 sounds,
// living under WHICH content directories.
//
// All output is teed to stdout AND a timestamped file under
// `perf-runs/` (gitignored). The committed evidence is the summary
// in `../docs/ongoing.md`; the per-run logs stay local.
//
// Run pattern:
//
//   set BBP_DEBUG_PORT=17171
//   cargo test --release --test explore_leak_source. --nocapture

mod common;

use common::{Api, open_perf_log};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::time::Duration;

const WINDOW: Duration = Duration::from_secs(30);
const TOP_N: usize = 20;
const SAMPLES_PER_CLASS: usize = 8;
const ALWAYS_SAMPLE: &[&str] = &["SoundWave", "SoundNodeWavePlayer", "Package"];

fn read_class_counts(snap_field: &Value) -> HashMap<String, u64> {
    let mut m = HashMap::new();
    if let Some(arr) = snap_field.get("top_classes").and_then(Value::as_array) {
        for entry in arr {
            if let (Some(c), Some(n)) = (
                entry.get("class").and_then(Value::as_str),
                entry.get("count").and_then(Value::as_u64),
            ) {
                m.insert(c.to_string(), n);
            }
        }
    }
    m
}

fn read_package_counts(snap_field: &Value) -> HashMap<String, u64> {
    let mut m = HashMap::new();
    if let Some(arr) = snap_field.get("top_packages").and_then(Value::as_array) {
        for entry in arr {
            if let (Some(p), Some(n)) = (
                entry.get("package").and_then(Value::as_str),
                entry.get("hosted_count").and_then(Value::as_u64),
            ) {
                m.insert(p.to_string(), n);
            }
        }
    }
    m
}

fn u64_field(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}

#[test]
fn investigate_leak_source() {
    let api = Api::require();
    let mut out = open_perf_log("leak_source");

    writeln!(out, "=== Snapshot T0 ===").unwrap();
    let t0 = api.snapshot();
    let t0_ws = u64_field(&t0.process_memory, "working_set_size");
    let t0_pf = u64_field(&t0.process_memory, "page_fault_count");
    let t0_gobj = u64_field(&t0.game_population, "gobjects_total");
    writeln!(
        out,
        "  working_set: {:.1} MB   page_faults: {}   gobjects: {}",
        t0_ws as f64 / (1024.0 * 1024.0),
        t0_pf,
        t0_gobj
    )
    .unwrap();

    writeln!(
        out,
        "Sleeping {}s while you play in-game...",
        WINDOW.as_secs()
    )
    .unwrap();
    std::thread::sleep(WINDOW);

    writeln!(out, "=== Snapshot T1 ===").unwrap();
    let t1 = api.snapshot();
    let t1_ws = u64_field(&t1.process_memory, "working_set_size");
    let t1_pf = u64_field(&t1.process_memory, "page_fault_count");
    let t1_gobj = u64_field(&t1.game_population, "gobjects_total");
    writeln!(
        out,
        "  working_set: {:.1} MB   page_faults: {}   gobjects: {}",
        t1_ws as f64 / (1024.0 * 1024.0),
        t1_pf,
        t1_gobj
    )
    .unwrap();

    writeln!(out, "\n=== Process delta over {}s ===", WINDOW.as_secs()).unwrap();
    writeln!(
        out,
        "  working_set:  {:+.1} MB   ({:.1} -> {:.1} MB)",
        (t1_ws as i64 - t0_ws as i64) as f64 / (1024.0 * 1024.0),
        t0_ws as f64 / (1024.0 * 1024.0),
        t1_ws as f64 / (1024.0 * 1024.0)
    )
    .unwrap();
    writeln!(out, "  page_faults:  {:+}", t1_pf as i64 - t0_pf as i64).unwrap();
    writeln!(
        out,
        "  gobjects:     {:+}   ({} -> {})",
        t1_gobj as i64 - t0_gobj as i64,
        t0_gobj,
        t1_gobj
    )
    .unwrap();

    let t0_classes = read_class_counts(&t0.game_population);
    let t1_classes = read_class_counts(&t1.game_population);
    let t0_pkgs = read_package_counts(&t0.game_population);
    let t1_pkgs = read_package_counts(&t1.game_population);

    let mut class_deltas: Vec<(String, i64, u64)> = t1_classes
        .iter()
        .map(|(c, n1)| {
            let n0 = *t0_classes.get(c).unwrap_or(&0);
            (c.clone(), *n1 as i64 - n0 as i64, *n1)
        })
        .collect();
    class_deltas.sort_by_key(|(_, d, _)| std::cmp::Reverse(*d));

    writeln!(out, "\n=== Top {} growing CLASSES ===", TOP_N).unwrap();
    writeln!(out, "{:>40} {:>10} {:>10}", "class", "delta", "now").unwrap();
    writeln!(out, "{}", "-".repeat(64)).unwrap();
    for (c, d, n) in class_deltas.iter().take(TOP_N) {
        writeln!(out, "{:>40} {:>+10} {:>10}", c, d, n).unwrap();
    }

    let mut pkg_deltas: Vec<(String, i64, u64)> = t1_pkgs
        .iter()
        .map(|(p, n1)| {
            let n0 = *t0_pkgs.get(p).unwrap_or(&0);
            (p.clone(), *n1 as i64 - n0 as i64, *n1)
        })
        .collect();
    pkg_deltas.sort_by_key(|(_, d, _)| std::cmp::Reverse(*d));

    writeln!(out, "\n=== Top {} growing PACKAGES ===", TOP_N).unwrap();
    writeln!(out, "{:>60} {:>10} {:>10}", "package", "delta", "now").unwrap();
    writeln!(out, "{}", "-".repeat(84)).unwrap();
    for (p, d, n) in pkg_deltas.iter().take(TOP_N) {
        writeln!(out, "{:>60} {:>+10} {:>10}", p, d, n).unwrap();
    }

    writeln!(out, "\n=== Loaded LEVELS at T1 ===").unwrap();
    if let Some(arr) = t1
        .game_population
        .get("loaded_levels")
        .and_then(Value::as_array)
    {
        writeln!(out, "count: {}", arr.len()).unwrap();
        for entry in arr.iter().take(50) {
            let class = entry.get("class").and_then(Value::as_str).unwrap_or("?");
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("?");
            let pkg = entry.get("package").and_then(Value::as_str).unwrap_or("?");
            writeln!(out, "  [{}] {} (pkg={})", class, name, pkg).unwrap();
        }
    } else {
        writeln!(
            out,
            "(no loaded_levels field present -- mod may need rebuild)"
        )
        .unwrap();
    }

    let mut to_sample: Vec<String> = class_deltas
        .iter()
        .take(5)
        .filter(|(_, d, _)| *d > 0)
        .map(|(c, _, _)| c.clone())
        .collect();
    for s in ALWAYS_SAMPLE {
        if !to_sample.iter().any(|x| x == s) {
            to_sample.push((*s).to_string());
        }
    }

    writeln!(
        out,
        "\n=== Address-space breakdown delta (VirtualQuery) ==="
    )
    .unwrap();
    writeln!(
        out,
        "{:>30} {:>14} {:>14} {:>14}",
        "category", "T0 (MB)", "T1 (MB)", "delta (MB)"
    )
    .unwrap();
    writeln!(out, "{}", "-".repeat(76)).unwrap();
    let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
    let mb_i = |b: i64| b as f64 / (1024.0 * 1024.0);
    let cats = [
        "total_committed_bytes",
        "by_type_image_bytes",
        "by_type_mapped_bytes",
        "by_type_private_bytes",
        "by_prot_rwx_bytes",
        "by_prot_rx_bytes",
        "by_prot_rw_bytes",
        "by_prot_r_bytes",
        "by_prot_other_bytes",
        "total_reserved_bytes",
        "total_free_bytes",
    ];
    for cat in cats {
        let v0 = u64_field(&t0.process_regions, cat);
        let v1 = u64_field(&t1.process_regions, cat);
        writeln!(
            out,
            "{:>30} {:>14.1} {:>14.1} {:>+14.1}",
            cat,
            mb(v0),
            mb(v1),
            mb_i(v1 as i64 - v0 as i64)
        )
        .unwrap();
    }
    let r0 = u64_field(&t0.process_regions, "committed_region_count");
    let r1 = u64_field(&t1.process_regions, "committed_region_count");
    writeln!(
        out,
        "  committed regions: {} -> {} (delta {:+})",
        r0,
        r1,
        r1 as i64 - r0 as i64
    )
    .unwrap();

    writeln!(
        out,
        "\n=== Private-region histogram delta ===  (where the {:.0} MB went)",
        mb_i(
            u64_field(&t1.process_regions, "by_type_private_bytes") as i64
                - u64_field(&t0.process_regions, "by_type_private_bytes") as i64
        )
    )
    .unwrap();
    writeln!(
        out,
        "{:>14} {:>10} {:>10} {:>+10} {:>14} {:>14} {:>+14}",
        "size bucket", "T0 count", "T1 count", "d count", "T0 MB", "T1 MB", "d MB"
    )
    .unwrap();
    writeln!(out, "{}", "-".repeat(90)).unwrap();
    let buckets = [
        ("< 64 KB", "lt_64k_count", "lt_64k_bytes"),
        ("< 128 KB", "lt_128k_count", "lt_128k_bytes"),
        ("< 256 KB", "lt_256k_count", "lt_256k_bytes"),
        ("< 1 MB", "lt_1m_count", "lt_1m_bytes"),
        ("< 4 MB", "lt_4m_count", "lt_4m_bytes"),
        ("< 16 MB", "lt_16m_count", "lt_16m_bytes"),
        ("< 64 MB", "lt_64m_count", "lt_64m_bytes"),
        (">= 64 MB", "ge_64m_count", "ge_64m_bytes"),
    ];
    let h0 = t0
        .process_regions
        .get("private_hist")
        .cloned()
        .unwrap_or_default();
    let h1 = t1
        .process_regions
        .get("private_hist")
        .cloned()
        .unwrap_or_default();
    for (label, count_key, bytes_key) in buckets {
        let c0 = u64_field(&h0, count_key);
        let c1 = u64_field(&h1, count_key);
        let b0 = u64_field(&h0, bytes_key);
        let b1 = u64_field(&h1, bytes_key);
        writeln!(
            out,
            "{:>14} {:>10} {:>10} {:>+10} {:>14.1} {:>14.1} {:>+14.1}",
            label,
            c0,
            c1,
            c1 as i64 - c0 as i64,
            mb(b0),
            mb(b1),
            mb_i(b1 as i64 - b0 as i64)
        )
        .unwrap();
    }
    let bases0 = u64_field(&t0.process_regions, "private_alloc_bases");
    let bases1 = u64_field(&t1.process_regions, "private_alloc_bases");
    writeln!(
        out,
        "\n  unique private VirtualAlloc bases: {} -> {} (delta {:+})",
        bases0,
        bases1,
        bases1 as i64 - bases0 as i64
    )
    .unwrap();
    writeln!(
        out,
        "  (stable bases + growing committed = pages added inside existing zones; \n   growing bases = new VirtualAlloc calls; tells us heap-growth vs new-allocator-zone)"
    )
    .unwrap();

    writeln!(out, "\n=== Top 20 largest committed regions at T1 ===").unwrap();
    if let Some(arr) = t1
        .process_regions
        .get("top_committed_regions")
        .and_then(Value::as_array)
    {
        writeln!(
            out,
            "{:>10} {:>9} {:>20} {}",
            "size MB", "type", "base", "mapped"
        )
        .unwrap();
        for entry in arr.iter().take(20) {
            let size_mb = entry.get("size_mb").and_then(Value::as_f64).unwrap_or(0.0);
            let typ = entry.get("type").and_then(Value::as_str).unwrap_or("?");
            let base = entry.get("base").and_then(Value::as_str).unwrap_or("?");
            let name = entry
                .get("mapped_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            writeln!(out, "{:>10.1} {:>9} {:>20} {}", size_mb, typ, base, name).unwrap();
        }
    }

    writeln!(out, "\n=== Outer-chain samples for suspect classes ===").unwrap();
    for class in &to_sample {
        let r = api.op(
            "class_outer_samples",
            serde_json::json!({"class": class, "k": SAMPLES_PER_CLASS}),
        );
        if !r.ok {
            writeln!(out, "\n[{}] op error: {:?}", class, r.error).unwrap();
            continue;
        }
        let total = r
            .result
            .get("matched_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        writeln!(out, "\n[{}] live count = {}", class, total).unwrap();
        if let Some(arr) = r.result.get("samples").and_then(Value::as_array) {
            for s in arr {
                if let Some(p) = s.get("full_path").and_then(Value::as_str) {
                    writeln!(out, "  {}", p).unwrap();
                }
            }
        }
    }
}
