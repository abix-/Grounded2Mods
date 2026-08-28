//! Locate where the trailer horse (alpha, name_id 344) lives on the
//! overworld. On the overworld alpha drops out of slot 0 (the home /
//! pasture vector). This walks every scene-table slot's Horse pointer
//! vector and reads each horse's name_id + age, so we can see which
//! container holds the player's trailer horse when not in My House.
//!
//! Attach mode against the running game; reads only valid vector contents
//! (the Horse pointers come from real begin..end vectors), so it is safe.

mod common;

use serde_json::{Value, json};

fn hex(v: Option<&Value>) -> u64 {
    v.and_then(Value::as_str)
        .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
        .unwrap_or(0)
}

#[test]
fn locate_trailer_horse() {
    let Some(game) = common::launch("locate_trailer_horse") else {
        return;
    };

    eprintln!("active_scene_id = {:?}", common::active_scene_id(&game));

    let scan = game
        .op_json("gamestate.scan_438_slots", &json!({}))
        .expect("scan op");
    let r = scan.get("result").unwrap_or(&scan).clone();
    eprintln!(
        "gs_ptr = {:?}, arr_ptr = {:?}",
        r.get("gs_ptr"),
        r.get("arr_ptr")
    );
    let slots = r
        .get("slots_with_horse_vec")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    eprintln!("\n=== scene-table slots: horses as name_id(age)@ptr ===");
    let mut found_344: Vec<String> = vec![];
    let mut found_345: Vec<String> = vec![];

    for s in &slots {
        let slot = s
            .get("slot")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let begin = hex(s.get("begin"));
        let end = hex(s.get("end"));
        if begin == 0 || end <= begin {
            continue;
        }
        let n = ((end - begin) / 8).min(64);
        let mut ids: Vec<String> = vec![];
        for i in 0..n {
            let pv = game
                .op_json("mem.peek", &json!({ "addr": begin + i * 8, "kind": "u64" }))
                .ok();
            let hp = pv
                .as_ref()
                .map(|v| hex(v.get("result").unwrap_or(v).get("value")))
                .unwrap_or(0);
            if hp <= 0x10000 {
                ids.push("null".into());
                continue;
            }
            let hr = game
                .op_json("horse.read", &json!({ "addr": format!("0x{hp:x}") }))
                .ok();
            let (nid, age) = hr
                .as_ref()
                .map(|v| {
                    let rr = v.get("result").unwrap_or(v);
                    (
                        rr.get("name_id").and_then(Value::as_u64).unwrap_or(9999),
                        rr.get("age").and_then(Value::as_i64).unwrap_or(-1),
                    )
                })
                .unwrap_or((9999, -1));
            if nid == 344 {
                found_344.push(format!("slot {slot} @ 0x{hp:x}"));
            }
            if nid == 345 {
                found_345.push(format!("slot {slot} @ 0x{hp:x}"));
            }
            ids.push(format!("{nid}(a{age})@0x{hp:x}"));
        }
        eprintln!("slot {slot}: {ids:?}");
    }

    eprintln!("\nalpha (name_id 344) found in: {found_344:?}");
    eprintln!("bravo (name_id 345) found in: {found_345:?}");
}
