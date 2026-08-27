//! Record and replay actions against a running game's control plane.
//!
//! Topside owns its fixed simulation step, so [`crate::actions::Journal`]
//! can reproduce a session by tick. An injected game keeps its own clock
//! and may load or stream asynchronously. This journal therefore advances
//! only when a control-plane observation reaches the recorded value.

use std::fmt;
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::Api;
use crate::envelope::OpResponse;

pub const SCHEMA: &str = "modforge.live-journal@v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordedOp {
    pub op: String,
    pub args: Value,
}

impl RecordedOp {
    pub fn new(op: impl Into<String>, args: Value) -> Self {
        Self {
            op: op.into(),
            args,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub operation: RecordedOp,
    pub pointer: String,
    pub equals: Value,
}

impl Observation {
    pub fn new(operation: RecordedOp, pointer: impl Into<String>, equals: Value) -> Self {
        Self {
            operation,
            pointer: pointer.into(),
            equals,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    Action {
        label: String,
        operation: RecordedOp,
    },
    Wait {
        label: String,
        observation: Observation,
        timeout_ms: u64,
        poll_ms: u64,
    },
    Assert {
        label: String,
        observation: Observation,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveJournal {
    schema: String,
    pub name: String,
    entries: Vec<Entry>,
}

impl LiveJournal {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA.to_string(),
            name: name.into(),
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn record_action(&mut self, label: impl Into<String>, operation: RecordedOp) {
        self.entries.push(Entry::Action {
            label: label.into(),
            operation,
        });
    }

    pub fn record_wait(
        &mut self,
        label: impl Into<String>,
        observation: Observation,
        timeout_ms: u64,
        poll_ms: u64,
    ) {
        self.entries.push(Entry::Wait {
            label: label.into(),
            observation,
            timeout_ms,
            poll_ms,
        });
    }

    pub fn record_assertion(&mut self, label: impl Into<String>, observation: Observation) {
        self.entries.push(Entry::Assert {
            label: label.into(),
            observation,
        });
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("serialize live journal: {e}"))
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let journal: Self =
            serde_json::from_str(text).map_err(|e| format!("parse live journal: {e}"))?;
        if journal.schema != SCHEMA {
            return Err(format!(
                "unsupported live journal schema '{}' (expected '{SCHEMA}')",
                journal.schema
            ));
        }
        Ok(journal)
    }

    pub fn replay(&self, executor: &impl OpExecutor) -> Result<ReplayReport, ReplayError> {
        let mut report = ReplayReport::default();
        for (step, entry) in self.entries.iter().enumerate() {
            match entry {
                Entry::Action { label, operation } => {
                    execute_checked(executor, step, label, operation)?;
                    report.actions += 1;
                }
                Entry::Wait {
                    label,
                    observation,
                    timeout_ms,
                    poll_ms,
                } => {
                    report.wait_polls +=
                        wait_for(executor, step, label, observation, *timeout_ms, *poll_ms)?;
                }
                Entry::Assert { label, observation } => {
                    assert_observation(executor, step, label, observation)?;
                    report.assertions += 1;
                }
            }
        }
        Ok(report)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    pub actions: usize,
    pub wait_polls: usize,
    pub assertions: usize,
}

pub trait OpExecutor {
    fn execute(&self, operation: &RecordedOp) -> Result<OpResponse<Value>, String>;
}

impl<S> OpExecutor for Api<S>
where
    S: DeserializeOwned + Serialize,
{
    fn execute(&self, operation: &RecordedOp) -> Result<OpResponse<Value>, String> {
        let response = self.try_op(&operation.op, operation.args.clone())?;
        let state = serde_json::to_value(response.state)
            .map_err(|e| format!("serialize {} state: {e}", operation.op))?;
        Ok(OpResponse {
            ok: response.ok,
            op: response.op,
            error: response.error,
            result: response.result,
            state,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayError {
    pub step: usize,
    pub label: String,
    pub actual: Option<Value>,
    message: String,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "live journal step {} '{}': {}",
            self.step, self.label, self.message
        )
    }
}

impl std::error::Error for ReplayError {}

pub struct Recorder<'a, E> {
    executor: &'a E,
    journal: LiveJournal,
}

impl<'a, E: OpExecutor> Recorder<'a, E> {
    pub fn new(name: impl Into<String>, executor: &'a E) -> Self {
        Self {
            executor,
            journal: LiveJournal::new(name),
        }
    }

    pub fn action(
        &mut self,
        label: impl Into<String>,
        operation: RecordedOp,
    ) -> Result<(), ReplayError> {
        let label = label.into();
        execute_checked(
            self.executor,
            self.journal.entries.len(),
            &label,
            &operation,
        )?;
        self.journal.record_action(label, operation);
        Ok(())
    }

    pub fn wait(
        &mut self,
        label: impl Into<String>,
        observation: Observation,
        timeout_ms: u64,
        poll_ms: u64,
    ) -> Result<(), ReplayError> {
        let label = label.into();
        wait_for(
            self.executor,
            self.journal.entries.len(),
            &label,
            &observation,
            timeout_ms,
            poll_ms,
        )?;
        self.journal
            .record_wait(label, observation, timeout_ms, poll_ms);
        Ok(())
    }

    pub fn assertion(
        &mut self,
        label: impl Into<String>,
        observation: Observation,
    ) -> Result<(), ReplayError> {
        let label = label.into();
        assert_observation(
            self.executor,
            self.journal.entries.len(),
            &label,
            &observation,
        )?;
        self.journal.record_assertion(label, observation);
        Ok(())
    }

    pub fn finish(self) -> LiveJournal {
        self.journal
    }
}

fn execute_checked(
    executor: &impl OpExecutor,
    step: usize,
    label: &str,
    operation: &RecordedOp,
) -> Result<OpResponse<Value>, ReplayError> {
    let observed = executor.execute(operation).map_err(|message| ReplayError {
        step,
        label: label.to_string(),
        actual: None,
        message: format!("operation '{}' could not run: {message}", operation.op),
    })?;
    if !observed.ok {
        return Err(ReplayError {
            step,
            label: label.to_string(),
            actual: None,
            message: format!(
                "operation '{}' failed: {}",
                operation.op,
                observed.error.as_deref().unwrap_or("no error returned")
            ),
        });
    }
    Ok(observed)
}

fn wait_for(
    executor: &impl OpExecutor,
    step: usize,
    label: &str,
    observation: &Observation,
    timeout_ms: u64,
    poll_ms: u64,
) -> Result<usize, ReplayError> {
    let started = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let poll = Duration::from_millis(poll_ms.max(1));
    let mut polls = 0;
    loop {
        polls += 1;
        let observed = execute_checked(executor, step, label, &observation.operation)?;
        let actual = observed_value(&observed, &observation.pointer).cloned();
        if actual.as_ref() == Some(&observation.equals) {
            return Ok(polls);
        }
        if started.elapsed() >= timeout {
            return Err(mismatch(step, label, observation, actual, true));
        }
        thread::sleep(poll.min(timeout.saturating_sub(started.elapsed())));
    }
}

fn assert_observation(
    executor: &impl OpExecutor,
    step: usize,
    label: &str,
    observation: &Observation,
) -> Result<(), ReplayError> {
    let observed = execute_checked(executor, step, label, &observation.operation)?;
    let actual = observed_value(&observed, &observation.pointer).cloned();
    if actual.as_ref() == Some(&observation.equals) {
        return Ok(());
    }
    Err(mismatch(step, label, observation, actual, false))
}

fn mismatch(
    step: usize,
    label: &str,
    observation: &Observation,
    actual: Option<Value>,
    timed_out: bool,
) -> ReplayError {
    let prefix = if timed_out { "timed out waiting: " } else { "" };
    let actual_text = actual
        .as_ref()
        .map(Value::to_string)
        .unwrap_or_else(|| "<missing>".to_string());
    ReplayError {
        step,
        label: label.to_string(),
        actual,
        message: format!(
            "{prefix}expected {} at {}, observed {actual_text}",
            json!(observation.equals),
            observation.pointer
        ),
    }
}

fn observed_value<'a>(response: &'a OpResponse<Value>, pointer: &str) -> Option<&'a Value> {
    match pointer {
        "/result" => Some(&response.result),
        "/state" => Some(&response.state),
        _ => {
            if let Some(rest) = pointer.strip_prefix("/result") {
                return response.result.pointer(rest);
            }
            if let Some(rest) = pointer.strip_prefix("/state") {
                return response.state.pointer(rest);
            }
            None
        }
    }
}
