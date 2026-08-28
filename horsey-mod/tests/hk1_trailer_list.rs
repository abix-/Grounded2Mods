//! HK1 detection (corrected model): the trailer horse list lives on the
//! MapState at `*MAP_STATE_PTR + 0x130/+0x138` (decomp FUN_1400cd5a0,
//! the truck enter/leave handler). This reads `horse.trailer` on the
//! live game and prints which owned horses the game considers to be in
//! the trailer vs the pasture. On a fresh launch (overworld) the list
//! reflects the saved trailer state.

mod common;

use serde_json::{Value, json};

#[test]
fn trailer_list_reads() {
    let Some(game) = common::launch("hk1_trailer_list") else {
        return;
    };

    // Poll horse.trailer until MapState loads (map_state != 0x0), then read.
    let dl = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut v = game
        .op_json("horse.trailer", &json!({}))
        .expect("horse.trailer op");
    loop {
        let ms = v
            .get("result")
            .unwrap_or(&v)
            .get("map_state")
            .and_then(Value::as_str)
            .unwrap_or("0x0");
        if ms != "0x0" || std::time::Instant::now() >= dl {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
        v = game
            .op_json("horse.trailer", &json!({}))
            .expect("horse.trailer op");
    }
    let r = v.get("result").unwrap_or(&v);
    eprintln!("[TRAILER] {}", serde_json::to_string_pretty(r).unwrap());

    assert!(
        r.get("error").is_none(),
        "horse.trailer returned error: {:?}",
        r.get("error")
    );
    let ms = r.get("map_state").and_then(Value::as_str).unwrap_or("0x0");
    assert_ne!(ms, "0x0", "MapState pointer is null");

    let owned = r
        .get("owned")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let in_trailer = owned
        .iter()
        .filter(|o| o.get("container").and_then(Value::as_str) == Some("trailer"))
        .count();
    eprintln!(
        "[TRAILER] {in_trailer} of {} owned horse(s) read as in the trailer",
        owned.len()
    );
}
