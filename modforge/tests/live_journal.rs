use std::collections::VecDeque;
use std::sync::Mutex;

use modforge::client::live_journal::{
    Entry, LiveJournal, Observation, Observed, OpExecutor, RecordedOp, Recorder,
};
use serde_json::json;

struct FakeExecutor {
    responses: Mutex<VecDeque<Result<Observed, String>>>,
    calls: Mutex<Vec<RecordedOp>>,
}

impl FakeExecutor {
    fn new(responses: impl IntoIterator<Item = Result<Observed, String>>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<RecordedOp> {
        self.calls.lock().unwrap().clone()
    }
}

impl OpExecutor for FakeExecutor {
    fn execute(&self, op: &RecordedOp) -> Result<Observed, String> {
        self.calls.lock().unwrap().push(op.clone());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("test supplied one response per operation")
    }
}

fn ok(result: serde_json::Value) -> Result<Observed, String> {
    Ok(Observed::ok(result, json!({"runtime_ready": true})))
}

fn read_equals(value: &str) -> Observation {
    Observation::new(
        RecordedOp::new("read_bytes", json!({"length": 8})),
        "/result/bytes_hex",
        json!(value),
    )
}

#[test]
fn live_journal_round_trips_its_versioned_steps() {
    let mut journal = LiveJournal::new("misery movement speed");
    journal.record_action(
        "set movement speed",
        RecordedOp::new("write_bytes", json!({"bytes_hex": "0000000000008940"})),
    );
    journal.record_wait("movement speed applied", read_equals("0000000000008940"), 250, 10);
    journal.record_assertion("movement speed remains applied", read_equals("0000000000008940"));

    let json = journal.to_json().unwrap();
    assert!(json.contains("modforge.live-journal@v1"));
    assert_eq!(LiveJournal::from_json(&json).unwrap(), journal);
}

#[test]
fn replay_executes_actions_then_waits_for_an_observed_value() {
    let mut journal = LiveJournal::new("condition gated");
    journal.record_action("write", RecordedOp::new("write_bytes", json!({})));
    journal.record_wait("visible", read_equals("target"), 100, 1);
    journal.record_assertion("still visible", read_equals("target"));

    let executor = FakeExecutor::new([
        ok(json!({})),
        ok(json!({"bytes_hex": "old"})),
        ok(json!({"bytes_hex": "target"})),
        ok(json!({"bytes_hex": "target"})),
    ]);
    let report = journal.replay(&executor).unwrap();

    assert_eq!(report.actions, 1);
    assert_eq!(report.wait_polls, 2);
    assert_eq!(report.assertions, 1);
    assert_eq!(
        executor.calls().iter().map(|call| call.op.as_str()).collect::<Vec<_>>(),
        ["write_bytes", "read_bytes", "read_bytes", "read_bytes"]
    );
}

#[test]
fn recorder_executes_and_keeps_the_steps_that_can_be_replayed() {
    let recording_executor = FakeExecutor::new([
        ok(json!({})),
        ok(json!({"bytes_hex": "target"})),
        ok(json!({"bytes_hex": "target"})),
    ]);
    let mut recorder = Recorder::new("recorded in misery", &recording_executor);
    recorder
        .action("write", RecordedOp::new("write_bytes", json!({})))
        .unwrap();
    recorder
        .wait("visible", read_equals("target"), 100, 1)
        .unwrap();
    recorder
        .assertion("still visible", read_equals("target"))
        .unwrap();
    let journal = recorder.finish();

    assert!(matches!(journal.entries()[0], Entry::Action { .. }));
    assert!(matches!(journal.entries()[1], Entry::Wait { .. }));
    assert!(matches!(journal.entries()[2], Entry::Assert { .. }));

    let replay_executor = FakeExecutor::new([
        ok(json!({})),
        ok(json!({"bytes_hex": "target"})),
        ok(json!({"bytes_hex": "target"})),
    ]);
    journal.replay(&replay_executor).unwrap();
}

#[test]
fn replay_reports_the_failed_action_and_host_error() {
    let mut journal = LiveJournal::new("failure evidence");
    journal.record_action("dangerous write", RecordedOp::new("write_bytes", json!({})));
    let executor = FakeExecutor::new([Ok(Observed::error("address is not writable", json!(true)))]);

    let error = journal.replay(&executor).unwrap_err();
    assert_eq!(error.step, 0);
    assert_eq!(error.label, "dangerous write");
    assert!(error.to_string().contains("address is not writable"));
}

#[test]
fn assertion_failure_reports_the_pointer_expected_and_actual_values() {
    let mut journal = LiveJournal::new("assertion evidence");
    journal.record_assertion("speed changed", read_equals("target"));
    let executor = FakeExecutor::new([ok(json!({"bytes_hex": "actual"}))]);

    let error = journal.replay(&executor).unwrap_err();
    assert_eq!(error.step, 0);
    assert_eq!(error.label, "speed changed");
    assert_eq!(error.actual, Some(json!("actual")));
    assert!(error.to_string().contains("/result/bytes_hex"));
    assert!(error.to_string().contains("target"));
}
