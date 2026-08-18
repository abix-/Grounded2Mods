//! Engine-agnostic mission system. Two lifecycle shapes:
//!
//! **Missions** (go-and-return): [`Mission`] trait, [`advance`],
//! [`advance_all`]. Used by vendor, steal, trade, scavenge, murder,
//! and courier modules.
//!
//! **Contracts** (offered/owed/paying): [`Contract`] trait,
//! [`advance_contract`]. Used by work-board systems (bounty,
//! clear-the-threat).
//!
//! [`should_tick`] provides the shared cadence-gating pattern.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Going,
    Returning,
}

/// What a per-stage callback tells the runner to do next.
pub enum Step {
    /// Still traveling; check again next tick.
    Continue,
    /// Arrived at the stage destination; advance to the next stage.
    Transition,
    /// Mission finished early (no Returning leg needed).
    Complete,
}

/// Generate the three field-accessor methods every Mission impl
/// repeats identically. Call inside `impl Mission for MyStruct`.
#[macro_export]
macro_rules! mission_accessors {
    () => {
        fn stage(&self) -> $crate::mission::Stage { self.stage }
        fn set_stage(&mut self, s: $crate::mission::Stage) { self.stage = s; }
        fn deadline(&self) -> f32 { self.deadline }
    };
}

/// A go-and-return mission. Games implement this on their mission
/// struct; the framework handles stage transitions, timeout, and
/// the batch advance loop.
pub trait Mission: Sized {
    fn stage(&self) -> Stage;
    fn set_stage(&mut self, s: Stage);
    fn deadline(&self) -> f32;

    /// Is the agent performing the mission still alive/valid?
    fn is_agent_alive(&self) -> Result<bool, String>;

    /// Called each tick while Going. Check distance to the target,
    /// do arrival work when close enough, and return the appropriate
    /// [`Step`]. `now` is the current game time in seconds.
    fn on_going(&mut self, now: f32) -> Result<Step, String>;

    /// Called each tick while Returning. Check distance to home, do
    /// return work when close enough, and return the appropriate
    /// [`Step`]. `now` is the current game time in seconds.
    fn on_returning(&mut self, now: f32) -> Result<Step, String>;

    /// Release handles, squads, and any other resources. Called on
    /// normal completion, timeout, agent death, and error abort.
    fn cleanup(self);

    /// Short display label for error/abort log messages.
    fn label(&self) -> String;
}

/// Advance one mission. Returns `Ok(true)` when the mission is
/// done and should be removed.
pub fn advance<M: Mission>(m: &mut M, now: f32) -> Result<bool, String> {
    if !m.is_agent_alive()? {
        return Ok(true);
    }
    if now >= m.deadline() {
        return Ok(true);
    }
    match m.stage() {
        Stage::Going => match m.on_going(now)? {
            Step::Continue => Ok(false),
            Step::Transition => {
                m.set_stage(Stage::Returning);
                Ok(false)
            }
            Step::Complete => Ok(true),
        },
        Stage::Returning => match m.on_returning(now)? {
            Step::Continue => Ok(false),
            Step::Transition | Step::Complete => Ok(true),
        },
    }
}

/// Advance all missions in a locked vec. Completed or errored
/// missions are removed and cleaned up. `on_error` receives the
/// mission and error string for game-specific logging.
pub fn advance_all<M: Mission>(
    missions: &Mutex<Vec<M>>,
    now: f32,
    on_error: impl Fn(&M, &str),
) {
    let mut guard = missions.lock();
    let mut i = 0;
    while i < guard.len() {
        let done = match advance(&mut guard[i], now) {
            Ok(d) => d,
            Err(e) => {
                on_error(&guard[i], &e);
                true
            }
        };
        if done {
            guard.remove(i).cleanup();
        } else {
            i += 1;
        }
    }
}

/// Returns true (and stores the new timestamp) when at least
/// `cadence` seconds have elapsed since the last true return.
/// Uses the `f32-as-bits-in-AtomicU32` pattern shared across
/// all tick-gated systems.
pub fn should_tick(now: f32, cadence: f32, last: &AtomicU32) -> bool {
    let prev = f32::from_bits(last.load(Ordering::Relaxed));
    if now - prev >= cadence {
        last.store(now.to_bits(), Ordering::Relaxed);
        true
    } else {
        false
    }
}

// ---- contracts (offered/owed/paying lifecycle) ------------------

/// The three phases of a work-board contract.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContractPhase {
    /// Posted on the board; waiting for the player to fulfill.
    Offered,
    /// Condition met; payment delivery pending.
    Owed,
    /// Payment in transit.
    Paying,
}

/// A work-board contract: offered, owed, paying, done.
///
/// Games implement this on their contract enum. The enum's
/// variants carry phase-specific data; [`Contract::advance`]
/// consumes the current variant and returns the next (or None
/// when the contract is finished).
pub trait Contract: Sized {
    fn phase(&self) -> ContractPhase;
    fn advance(self, now: f32) -> Result<Option<Self>, String>;
    fn label(&self) -> String;
}

/// Drive a singleton contract forward one tick. Takes the
/// contract out of the slot, advances it, and puts the result
/// back. On error the contract is consumed (the advance impl
/// is responsible for cleanup before returning Err).
pub fn advance_contract<C: Contract>(
    slot: &Mutex<Option<C>>,
    now: f32,
    on_error: impl Fn(String),
) {
    let mut guard = slot.lock();
    let Some(contract) = guard.take() else { return };
    *guard = match contract.advance(now) {
        Ok(next) => next,
        Err(e) => {
            on_error(e);
            None
        }
    };
}
