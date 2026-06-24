//! HK1 step 1: read each owned horse's two candidate position fields on
//! the overworld. Scene placement (+0x1d4/+0x1d8, what the trailer
//! rectangle test uses per the decomp) and main actor position
//! (+0x28/+0x2c, which moves around). Printing both so we can correlate
//! against ground truth (which horses are actually in the trailer) and
//! lock which field is the trailer-determining one + roughly where the
//! trailer region sits. Read-only, uses existing ops.

mod common;

use serde_json::{json, Value};

fn f32_at(game: &modforge::harness::RunningGame, addr: u64) -> Option<f32> {
    let v = game
        .op_json("patterns.read_bytes", &json!({"addr": format!("0x{addr:x}"), "n": 4}))
        .ok()?;
    let hex = v.get("result").unwrap_or(&v).get("bytes").and_then(Value::as_str)?;
    let bytes: Vec<u8> = hex
        .split_whitespace()
        .filter_map(|b| u8::from_str_radix(b, 16).ok())
        .collect();
    if bytes.len() < 4 {
        return None;
    }
    Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[test]
fn read_horse_positions() {
    let Some(game) = common::launch("hk1_horse_positions") else { return };

    // Wait out the load race.
    let dl = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut horses: Vec<Value> = vec![];
    loop {
        let v = game.op_json("gamestate.owned_horses", &json!({})).unwrap_or(Value::Null);
        let r = v.get("result").unwrap_or(&v);
        horses = r.get("horses").and_then(Value::as_array).cloned().unwrap_or_default();
        if !horses.is_empty() || std::time::Instant::now() >= dl {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    eprintln!("[POS] {} owned horse(s):", horses.len());
    for h in &horses {
        let idx = h.get("idx").and_then(Value::as_u64).unwrap_or(0);
        let p = usize::from_str_radix(
            h.get("ptr").and_then(Value::as_str).unwrap_or("0x0").trim_start_matches("0x"),
            16,
        )
        .unwrap_or(0) as u64;
        let scene_x = f32_at(&game, p + 0x1d4);
        let scene_y = f32_at(&game, p + 0x1d8);
        let actor_x = f32_at(&game, p + 0x28);
        let actor_y = f32_at(&game, p + 0x2c);
        eprintln!(
            "  horse[{idx}] ptr=0x{p:x}  scene(+0x1d4/+0x1d8)=({scene_x:?}, {scene_y:?})  actor(+0x28/+0x2c)=({actor_x:?}, {actor_y:?})"
        );
    }
    assert!(!horses.is_empty(), "no owned horses");
}
