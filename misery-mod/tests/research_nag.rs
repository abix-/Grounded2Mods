//! What does the playtest notice actually DO when dismissed?
//!
//! Collapsing the widget hides it but leaves whatever it was
//! blocking still blocked (a black screen waiting for a
//! keypress). So the dismissal must call something. This reads
//! the widget's own functions and state to find out what.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_nag -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

const NAG: &str = "WD_PlaytestNote01_C";

/// The notice's functions: what could a keypress be calling?
///
/// Read from the LIVE class, not the discovery cache.
/// `discover_class_detail` reads the startup GObjects walk, and
/// this class is absent from it (research.md 26.5). `nag_stats`
/// walks the live object's `UClass::iter_functions` instead.
#[test]
fn nag_class_detail() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("nag_stats", json!({}));
    assert!(r.ok, "nag_stats failed: {:?}", r.error);
    println!("present: {}", r.result["present"]);
    println!("hooked:  {}", r.result["hooked"]);
    let fns = r.result["functions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("{} function(s) on {NAG}:", fns.len());
    for f in &fns {
        println!("  {}", f.as_str().unwrap_or("?"));
    }
}

/// What else is on screen with it? The thing showing the black
/// screen is probably a sibling, not the notice.
#[test]
fn widgets_on_screen() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    // Widgets that exist as class templates live under
    // /Game/....WidgetTree; the ones actually created and on
    // screen live under /Engine/Transient. Only the latter
    // matter for what the player is looking at.
    let r = api.op(
        "walk_class_chain",
        json!({"needle": "UserWidget", "max": 400}),
    );
    assert!(r.ok, "walk failed: {:?}", r.error);
    let all = r.result["instances"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let live: Vec<&serde_json::Value> = all
        .iter()
        .filter(|w| {
            w["full_name"]
                .as_str()
                .map(|n| n.contains("/Engine/Transient"))
                .unwrap_or(false)
        })
        .collect();
    println!(
        "{} widget object(s), {} of them actually instantiated:",
        all.len(),
        live.len()
    );
    for w in &live {
        println!("  {}", w["full_name"].as_str().unwrap_or("?"));
    }
}

/// The notice's own state: is there a flag it sets on dismissal?
#[test]
fn nag_state() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let live = client::walk_class_chain_instances(&api, NAG, 4);
    let Some(w) = live.first() else {
        println!("notice is not live right now");
        return;
    };
    println!("notice at {}", w.addr_selector);
    let r = api.op("inspect_address", json!({"addr": w.addr}));
    if r.ok {
        println!(
            "{}",
            serde_json::to_string_pretty(&r.result).unwrap_or_default()
        );
    } else {
        println!("inspect failed: {:?}", r.error);
    }
}
