// Exploratory: rapid-poll the player's StatusEffects array
// during a bandage use, looking for a HoT row that gets added
// briefly and removed. The earlier single-snapshot observation
// could have missed the row because the snapshot fired after
// the HoT completed.
//
// Pure test code. Uses snapshot only, no mod changes.
//
// Manual run (interactive):
//   set BBP_DEBUG_PORT=17171
//   cargo test --release --target x86_64-pc-windows-msvc \
//     --test explore_bandage_status_effects. --nocapture --test-threads=1 --ignored
//
// Procedure:
//   1. Test starts polling at 200ms intervals for 12 seconds (60 ticks).
//   2. As soon as you see "POLLING START" in the test output, use a bandage.
//   3. Test prints the row diff on every tick where the StatusEffects
//      array changes (added rows, removed rows, value changes).
//   4. CurrentDamage is printed each tick so you see the heal trajectory.
//
// Expected outcomes:
//   - If a HoT row appears mid-poll: log shows it. Capture row+type+value.
//   - If no row ever appears but CurrentDamage drops: HoT is via a
//     non-status-effect mechanism (native C++ path or different
//     component).
//   - If CurrentDamage doesn't drop: bandage didn't fire (suppressed
//     upstream of the heal mechanism).

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

#[test]
#[ignore] // run with --ignored
fn poll_status_effects_during_bandage() {
    let api = common::Api::require();

    let baseline = api.snapshot();
    let baseline_rows: HashSet<String> = baseline
        .status_effects
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|e| e.row.clone())
        .collect();

    eprintln!("\n========================================");
    eprintln!("=== POLLING START -- USE A BANDAGE NOW ===");
    eprintln!("========================================");
    eprintln!(
        "baseline CurrentDamage = {:.2} / MaxHealth {:.2}",
        baseline.live_hc().current_damage,
        baseline.live_hc().max_health
    );
    eprintln!("baseline status_effects ({} rows):", baseline_rows.len());
    for e in baseline.status_effects.as_deref().unwrap_or(&[]) {
        let highlight = if e.row.contains("Bandage") || e.row.contains("HoT") {
            " <-- candidate"
        } else {
            ""
        };
        eprintln!(
            "  row={:<35} type={:?} value={:?}  table={}{}",
            e.row,
            e.stat_type,
            e.value,
            e.table.split('.').next_back().unwrap_or(&e.table),
            highlight,
        );
    }
    eprintln!();

    let start = Instant::now();
    let interval = Duration::from_millis(200);
    let total_duration = Duration::from_secs(12);

    let mut last_rows: HashSet<String> = baseline_rows.clone();
    let mut last_cd = baseline.live_hc().current_damage;
    let mut tick = 0u32;

    let mut all_seen_extra: HashSet<String> = HashSet::new();

    while start.elapsed() < total_duration {
        std::thread::sleep(interval);
        tick += 1;
        let s = api.snapshot();
        let rows: HashSet<String> = s
            .status_effects
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|e| e.row.clone())
            .collect();
        let cd = s.live_hc().current_damage;

        let added: Vec<&String> = rows.difference(&last_rows).collect();
        let removed: Vec<&String> = last_rows.difference(&rows).collect();
        let cd_delta = cd - last_cd;

        if !added.is_empty() || !removed.is_empty() || cd_delta.abs() > 0.05 {
            eprintln!(
                "[tick {tick:3} t={:5.1}s] CD={:.2} (Δ={:+.2})  +rows={:?}  -rows={:?}",
                start.elapsed().as_secs_f32(),
                cd,
                cd_delta,
                added,
                removed
            );
            for r in &added {
                let entry = s
                    .status_effects
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .find(|e| &e.row == *r)
                    .unwrap();
                eprintln!(
                    "    [+] row={} type={:?} value={:?} table={}",
                    entry.row, entry.stat_type, entry.value, entry.table
                );
                all_seen_extra.insert((*r).clone());
            }
        }
        last_rows = rows;
        last_cd = cd;
    }

    eprintln!("\n========================================");
    eprintln!("=== POLLING END ===");
    eprintln!("========================================");
    eprintln!(
        "Total status_effects rows observed beyond baseline during the window: {}",
        all_seen_extra.len()
    );
    for r in &all_seen_extra {
        eprintln!("  candidate bandage row: {r}");
    }
    eprintln!(
        "CurrentDamage trajectory: {:.2} -> {:.2} (delta = {:+.2})",
        baseline.live_hc().current_damage,
        last_cd,
        last_cd - baseline.live_hc().current_damage
    );

    if all_seen_extra.is_empty() {
        eprintln!("\nNO NEW STATUS EFFECTS observed during the bandage window.");
        eprintln!("If CurrentDamage dropped, the heal goes through a non-status-effect mechanism.");
        eprintln!("If CurrentDamage didn't drop, the bandage was suppressed upstream of the heal.");
    }
}
