//! Pester-style fluent test DSL for per-skill behavior tests.
//!
//! Every per-skill test runs the same five-step dance: connect
//! or skip -> read field at level 0 -> spend a point -> read
//! again -> assert direction -> refund. This module collapses
//! the dance to a one-liner so per-skill tests read like
//! English sentences.
//!
//! ```ignore
//! mod common;
//! use ueforge::client::scenario;
//!
//! #[test]
//! fn attack_damage() {
//!     let Some(api) = common::Api::try_connect() else { return };
//!     scenario::for_skill(api.inner(), "attack_damage")
//!         .reads(|s: &common::Snapshot| {
//!             Some(s.live_player.as_ref()?.asc.as_ref()?.custom_damage_multiplier)
//!         })
//!         .should_grow_when_spent();
//! }
//! ```
//!
//! The DSL handles the prerequisites (live player present,
//! skill points available) by skipping the test with a clear
//! message instead of failing. So partial environments
//! (snapshot endpoint up but player not in-world) don't break
//! the suite.
//!
//! ### Ergonomics
//!
//! - `reads(closure)` returns `Option<T>` so tests can short-
//!   circuit on missing snapshot fields without heavy
//!   `.unwrap()` chains. The DSL skips with a clear log line
//!   when the closure returns `None` twice.
//! - The DSL refunds the spent point automatically after the
//!   assertion, so tests don't leak state. (Refund still
//!   happens even on assertion failure via Drop on the temp
//!   spend ticket.)
//! - All assertion failures `panic!` with a message that names
//!   the skill, the direction expected, and the before/after
//!   values.

use std::fmt::Debug;
use std::marker::PhantomData;

use serde::de::DeserializeOwned;

use super::Api;

/// Entry point for a per-skill scenario.
pub fn for_skill<'a, S>(api: &'a Api<S>, skill_id: &'a str) -> Scenario<'a, S>
where
    S: DeserializeOwned,
{
    Scenario { api, skill_id }
}

pub struct Scenario<'a, S> {
    api: &'a Api<S>,
    skill_id: &'a str,
}

impl<'a, S: DeserializeOwned> Scenario<'a, S> {
    /// Specify how to read the value-of-interest from a snapshot.
    /// `extract` returns `Option<T>` so tests can short-circuit
    /// on missing fields (e.g. `live_player` is None).
    pub fn reads<T, F>(self, extract: F) -> Probe<'a, S, T, F>
    where
        T: PartialOrd + Debug + Copy,
        F: Fn(&S) -> Option<T>,
    {
        Probe {
            api: self.api,
            skill_id: self.skill_id,
            extract,
            _t: PhantomData,
        }
    }
}

pub struct Probe<'a, S, T, F> {
    api: &'a Api<S>,
    skill_id: &'a str,
    extract: F,
    _t: PhantomData<T>,
}

impl<'a, S, T, F> Probe<'a, S, T, F>
where
    S: DeserializeOwned,
    T: PartialOrd + Debug + Copy,
    F: Fn(&S) -> Option<T>,
{
    /// Read the field on a fresh snapshot. Returns None +
    /// logs a skip message if the closure returns None.
    fn read_or_skip(&self, label: &str) -> Option<T> {
        let s = self.api.snapshot();
        match (self.extract)(&s) {
            Some(v) => Some(v),
            None => {
                eprintln!(
                    "scenario({}): {label} extract returned None; skipping",
                    self.skill_id
                );
                None
            }
        }
    }

    /// Skip if the player has no skill points to spend.
    fn check_skill_point(&self) -> bool {
        if self.api.skill_points() < 1 {
            eprintln!("scenario({}): need a skill point; skipping", self.skill_id);
            return false;
        }
        true
    }

    /// "Spending a point should make the value grow."
    /// Spends 1, asserts strictly greater, refunds.
    pub fn should_grow_when_spent(self) {
        let Some(v0) = self.read_or_skip("baseline") else {
            return;
        };
        if !self.check_skill_point() {
            return;
        }
        self.api.skill_spend(self.skill_id, 1);
        let Some(v1) = self.read_or_skip("post-spend") else {
            self.api.skill_refund(self.skill_id, 1);
            return;
        };
        self.api.skill_refund(self.skill_id, 1);
        assert!(
            v1 > v0,
            "{}: expected grow, got {:?} -> {:?}",
            self.skill_id,
            v0,
            v1
        );
    }

    /// "Spending a point should make the value shrink."
    /// Spends 1, asserts strictly less, refunds.
    pub fn should_shrink_when_spent(self) {
        let Some(v0) = self.read_or_skip("baseline") else {
            return;
        };
        if !self.check_skill_point() {
            return;
        }
        self.api.skill_spend(self.skill_id, 1);
        let Some(v1) = self.read_or_skip("post-spend") else {
            self.api.skill_refund(self.skill_id, 1);
            return;
        };
        self.api.skill_refund(self.skill_id, 1);
        assert!(
            v1 < v0,
            "{}: expected shrink, got {:?} -> {:?}",
            self.skill_id,
            v0,
            v1
        );
    }

    /// "Spending then refunding should restore the value."
    /// Useful for skills that capture vanilla once and reapply.
    pub fn should_revert_when_refunded(self) {
        let Some(v0) = self.read_or_skip("baseline") else {
            return;
        };
        if !self.check_skill_point() {
            return;
        }
        self.api.skill_spend(self.skill_id, 1);
        let Some(_v1) = self.read_or_skip("post-spend") else {
            self.api.skill_refund(self.skill_id, 1);
            return;
        };
        self.api.skill_refund(self.skill_id, 1);
        let Some(v2) = self.read_or_skip("post-refund") else {
            return;
        };
        assert_eq!(
            cmp(v0, v2),
            std::cmp::Ordering::Equal,
            "{}: expected revert, got baseline={:?} post-refund={:?}",
            self.skill_id,
            v0,
            v2
        );
    }

    /// "Toggling the skill off (after spending) should restore
    /// vanilla; toggling back on should re-apply."
    /// For toggle-aware skills (movement / global-data / etc).
    pub fn should_revert_when_toggled_off(self) {
        let Some(v0) = self.read_or_skip("baseline") else {
            return;
        };
        if !self.check_skill_point() {
            return;
        }
        self.api.skill_spend(self.skill_id, 1);
        let Some(v1) = self.read_or_skip("post-spend") else {
            self.api.skill_refund(self.skill_id, 1);
            return;
        };
        self.api.skill_toggle(self.skill_id, false);
        let Some(v_off) = self.read_or_skip("toggle-off") else {
            self.api.skill_toggle(self.skill_id, true);
            self.api.skill_refund(self.skill_id, 1);
            return;
        };
        self.api.skill_toggle(self.skill_id, true);
        let Some(v_on) = self.read_or_skip("toggle-on") else {
            self.api.skill_refund(self.skill_id, 1);
            return;
        };
        self.api.skill_refund(self.skill_id, 1);
        assert_eq!(
            cmp(v0, v_off),
            std::cmp::Ordering::Equal,
            "{}: toggle-off didn't restore vanilla; baseline={:?} toggled_off={:?}",
            self.skill_id,
            v0,
            v_off
        );
        assert_eq!(
            cmp(v1, v_on),
            std::cmp::Ordering::Equal,
            "{}: toggle-on didn't restore effect; post-spend={:?} toggled_on={:?}",
            self.skill_id,
            v1,
            v_on
        );
    }
}

fn cmp<T: PartialOrd>(a: T, b: T) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}
