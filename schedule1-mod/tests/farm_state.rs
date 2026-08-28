//! Zone-garrison observability: print farm_state (per-region
//! garrison counts + kill tally). Counts only; rolled specifics
//! stay behind the spoiler firewall.
//!
//! ```text
//! cargo test -p schedule1-mod --test farm_state. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn print_farm_state() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let r = api.op("farm_state", json!({}));
    if !r.ok {
        println!("farm_state FAILED: {:?}", r.error);
        return;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&r.result).unwrap_or_default()
    );
}
