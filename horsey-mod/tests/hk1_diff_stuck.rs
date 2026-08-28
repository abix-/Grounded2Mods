//! Diff the stuck horse (Coupe DeVille, name_id 251. Can't be dragged)
//! against a movable horse (tomtato, name_id 250) over the horse-object
//! state region. The drag handler (FUN_1400d2ab0) only lets you grab a
//! horse when certain per-horse flags are clear, so the field that
//! differs between stuck and movable is the lock. (Position +0x1d4/+0x1d8
//! and name_id +0x1f8 will differ as expected; look for a boolean-ish
//! flag.)

mod common;

use serde_json::{Value, json};

fn read(game: &modforge::harness::RunningGame, abs: u64, n: usize) -> Vec<u8> {
    let v = game
        .op_json(
            "patterns.read_bytes",
            &json!({"addr": format!("0x{abs:x}"), "n": n}),
        )
        .unwrap_or(Value::Null);
    v.get("result")
        .unwrap_or(&v)
        .get("bytes")
        .and_then(Value::as_str)
        .unwrap_or("")
        .split_whitespace()
        .filter_map(|x| u8::from_str_radix(x, 16).ok())
        .collect()
}

fn ptr_for_nid(horses: &[Value], nid: u64) -> Option<u64> {
    horses
        .iter()
        .find(|h| h.get("name_id").and_then(Value::as_u64) == Some(nid))
        .and_then(|h| h.get("ptr").and_then(Value::as_str))
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
}

#[test]
fn diff_stuck_vs_movable() {
    let Some(game) = common::launch("hk1_diff_stuck") else {
        return;
    };

    let dl = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut horses: Vec<Value> = vec![];
    loop {
        let v = game
            .op_json("gamestate.owned_horses", &json!({}))
            .unwrap_or(Value::Null);
        let r = v.get("result").unwrap_or(&v);
        horses = r
            .get("horses")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if (ptr_for_nid(&horses, 251).is_some() && horses.len() >= 2)
            || std::time::Instant::now() >= dl
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    let coupe = ptr_for_nid(&horses, 251); // stuck (Coupe DeVille)
    // Any other owned horse as the movable comparison.
    let other = horses
        .iter()
        .find(|h| h.get("name_id").and_then(Value::as_u64) != Some(251))
        .map(|h| {
            let nid = h.get("name_id").and_then(Value::as_u64).unwrap_or(0);
            let p = h.get("ptr").and_then(Value::as_str).unwrap_or("0x0");
            (
                nid,
                u64::from_str_radix(p.trim_start_matches("0x"), 16).unwrap_or(0),
            )
        });
    eprintln!("[PTRS] coupe={coupe:?} other={other:?}");
    let (Some(coupe), Some((other_nid, tom))) = (coupe, other) else {
        eprintln!("could not find two horses");
        return;
    };
    eprintln!("[DIFF baseline] Coupe DeVille (251, stuck) vs name_id {other_nid} (movable)");

    // Walk the full horse object below the gene banks (genes start at
    // +0x2b8), in 8-byte rows, flagging rows that differ.
    const START: usize = 0x1c0;
    const END: usize = 0x2b8;
    let a = read(&game, coupe + START as u64, END - START);
    let b = read(&game, tom + START as u64, END - START);
    eprintln!("[DIFF] coupe(stuck) vs tomtato(movable)  horse+0x{START:x}..0x{END:x}");
    for off in (0..a.len().min(b.len())).step_by(4) {
        let av: Vec<u8> = a[off..(off + 4).min(a.len())].to_vec();
        let bv: Vec<u8> = b[off..(off + 4).min(b.len())].to_vec();
        if av != bv {
            let ah: String = av
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            let bh: String = bv
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("  +0x{:03x}: coupe[{ah}]  tomtato[{bh}]", START + off);
        }
    }
}
