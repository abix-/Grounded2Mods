//! Record and replay a condition-gated action against the live player.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 k3sc cargo-lock test -p misery-mod \
//!   --test live_action_journal -- --ignored --test-threads=1 --nocapture
//! ```

mod common;

use std::path::PathBuf;

use common::{Api, api_or_skip, offsets_live};
use modforge::client;
use modforge::client::live_journal::{LiveJournal, Observation, OpExecutor, RecordedOp, Recorder};
use serde_json::json;

const CHAR_COMP: &str = "BP_CharacterComponent_C";
const MOVEMENT_SPEED: u64 = 0x200;

struct Restore<'a> {
    api: &'a Api,
    operation: RecordedOp,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        match self.api.execute(&self.operation) {
            Ok(observed) if observed.ok => {}
            Ok(observed) => eprintln!(
                "restore movement speed failed: {}",
                observed.error.as_deref().unwrap_or("no error returned")
            ),
            Err(error) => eprintln!("restore movement speed could not run: {error}"),
        }
    }
}

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn read(selector: &str) -> RecordedOp {
    RecordedOp::new(
        "read_bytes",
        json!({
            "instance_selector": selector,
            "offset": MOVEMENT_SPEED,
            "length": 8,
        }),
    )
}

fn write(selector: &str, bytes_hex: &str) -> RecordedOp {
    RecordedOp::new(
        "write_bytes",
        json!({
            "instance_selector": selector,
            "offset": MOVEMENT_SPEED,
            "bytes_hex": bytes_hex,
        }),
    )
}

fn speed_equals(selector: &str, bytes_hex: &str) -> Observation {
    Observation::new(read(selector), "/result/bytes_hex", json!(bytes_hex))
}

#[test]
#[ignore = "writes and restores the live player movement speed"]
fn misery_records_saves_and_replays_a_condition_gated_action() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::walk_class_instances(&api, CHAR_COMP, 100)
        .into_iter()
        .find(|instance| instance.full_name.contains("PersistentLevel"))
        .expect("load a save so the live player character component exists");
    let original = client::read_bytes(&api, player.addr, MOVEMENT_SPEED, 8);
    assert_eq!(
        original.len(),
        8,
        "movement speed read returned short bytes"
    );
    let original_hex = hex::encode(&original);
    let original_speed = f64::from_le_bytes(original.as_slice().try_into().unwrap());
    let target_speed: f64 = if (original_speed - 777.0).abs() < 0.01 {
        888.0
    } else {
        777.0
    };
    let target_hex = hex::encode(target_speed.to_le_bytes());

    let restore_op = write(&player.addr_selector, &original_hex);
    let _restore = Restore {
        api: &api,
        operation: restore_op.clone(),
    };

    let mut recorder = Recorder::new("misery movement speed", &api);
    recorder
        .action(
            "set player movement speed",
            write(&player.addr_selector, &target_hex),
        )
        .unwrap();
    recorder
        .wait(
            "player movement speed becomes visible",
            speed_equals(&player.addr_selector, &target_hex),
            2_000,
            25,
        )
        .unwrap();
    recorder
        .assertion(
            "player movement speed remains changed",
            speed_equals(&player.addr_selector, &target_hex),
        )
        .unwrap();
    recorder
        .action("restore player movement speed", restore_op)
        .unwrap();
    recorder
        .wait(
            "original movement speed becomes visible",
            speed_equals(&player.addr_selector, &original_hex),
            2_000,
            25,
        )
        .unwrap();
    recorder
        .assertion(
            "player movement speed remains restored",
            speed_equals(&player.addr_selector, &original_hex),
        )
        .unwrap();
    let recorded = recorder.finish();

    let path = std::env::temp_dir().join(format!(
        "modforge-misery-live-journal-{}.json",
        std::process::id()
    ));
    let _remove_file = RemoveFile(path.clone());
    std::fs::write(&path, recorded.to_json().unwrap()).unwrap();
    let replay = LiveJournal::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(replay, recorded);

    let report = replay.replay(&api).unwrap();
    assert_eq!(report.actions, 2);
    assert_eq!(report.assertions, 2);
    assert!(report.wait_polls >= 2);
    assert_eq!(
        client::read_bytes(&api, player.addr, MOVEMENT_SPEED, 8),
        original,
        "journal did not restore the original movement speed"
    );
}
