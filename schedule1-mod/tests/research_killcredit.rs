//! Research: kill attribution (docs/research.md question 3,
//! the open half). TakeDamage(Single, Boolean, Boolean) carries
//! no attacker, so this test proves live which NPCHealth signals
//! fire when the PLAYER attacks: does NotifyAttackedByPlayer
//! fire, and does Die on the same NPC follow it? That pair is
//! the XP-credit mechanism candidate.
//!
//! Run ONLY with the operator in-game, ready to attack an NPC
//! (punching a goon works; killing one is the full answer).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_killcredit. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

/// Read back whatever the trace has recorded so far, without
/// installing hooks or waiting. For when the live run's console
/// output was lost: the events live in the mod until cleared.
///
/// ```text
/// cargo test -p schedule1-mod --test research_killcredit report_only. --test-threads=1 --nocapture
/// ```
#[test]
fn report_only() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let report = api.op("combat_trace_report", json!({}));
    if !report.ok {
        println!("combat_trace_report FAILED: {:?}", report.error);
        return;
    }
    println!("tracing={}", report.result["tracing"]);
    let events = report.result["events"].as_array().cloned().unwrap_or_default();
    println!("{} event(s):", events.len());
    for e in &events {
        println!(
            "  +{ms}ms {event} npc={npc} health={health}",
            ms = e["ms"],
            event = e["event"].as_str().unwrap_or("?"),
            npc = e["npc"],
            health = e["health"],
        );
    }
}

/// Drop the trace hooks without recording anything new. For
/// when a run was interrupted and left the hooks installed.
///
/// ```text
/// cargo test -p schedule1-mod --test research_killcredit stop_trace. --test-threads=1 --nocapture
/// ```
#[test]
fn stop_trace() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let stop = api.op("combat_trace_stop", json!({}));
    println!("combat_trace_stop: ok={} {}", stop.ok, stop.result);
}

#[test]
fn trace_player_attack_signals() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let start = api.op("combat_trace_start", json!({}));
    if !start.ok {
        println!("combat_trace_start FAILED: {:?}", start.error);
        return;
    }
    println!("hooks: {}", start.result["hooks"]);

    println!("OPERATOR: attack an NPC now (kill one if you can). Watching for 120s...");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut seen = 0usize;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let report = api.op("combat_trace_report", json!({}));
        if !report.ok {
            println!("combat_trace_report FAILED: {:?}", report.error);
            break;
        }
        let events = report.result["events"].as_array().cloned().unwrap_or_default();
        for e in events.iter().skip(seen) {
            println!(
                "  +{ms}ms {event} npc={npc} health={health}",
                ms = e["ms"],
                event = e["event"].as_str().unwrap_or("?"),
                npc = e["npc"],
                health = e["health"],
            );
        }
        seen = events.len();
        // A Die event means the full answer is in; stop early.
        if events.iter().any(|e| e["event"] == "Die") {
            println!("Die observed; enough.");
            break;
        }
    }

    let stop = api.op("combat_trace_stop", json!({}));
    println!("combat_trace_stop: ok={} {}", stop.ok, stop.result);
    if seen == 0 {
        println!("NO events recorded; either nothing was attacked or the hooks are on the wrong methods.");
    }
}
