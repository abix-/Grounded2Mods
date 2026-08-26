//! Engine-agnostic mission system. Four lifecycle shapes:
//!
//! **Multi-stage missions**: [`MultiStageMission`] trait,
//! [`advance_multi_stage`], [`advance_multi_stage_all`]. Callers
//! define the stage type and explicit transitions.
//!
//! **Go-and-return missions**: [`Mission`] trait, [`advance`],
//! [`advance_all`]. This is the existing two-stage compatibility
//! shape used by vendor, steal, trade, scavenge, murder, and courier.
//!
//! **One-stage missions**: [`OneStageMission`] trait,
//! [`advance_one_stage`], [`advance_one_stage_all`]. Used when the
//! game owns the activity and the mod only observes its outcome.
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

/// What a multi-stage callback tells the runner to do next.
pub enum MultiStageStep<S> {
    Continue,
    Transition(S),
    Complete,
}

/// A mission with a caller-defined stage type and any number of
/// explicit transitions.
pub trait MultiStageMission: Sized {
    type Stage: Copy;

    fn stage(&self) -> Self::Stage;
    fn set_stage(&mut self, stage: Self::Stage);
    fn deadline(&self) -> f32;
    fn is_agent_alive(&self) -> Result<bool, String> {
        Ok(true)
    }
    fn on_stage(
        &mut self,
        stage: Self::Stage,
        now: f32,
    ) -> Result<MultiStageStep<Self::Stage>, String>;

    fn on_timeout(&mut self, _now: f32) -> Result<(), String> {
        Ok(())
    }

    fn cleanup(self);
    fn label(&self) -> String;
}

pub fn advance_multi_stage<M: MultiStageMission>(
    mission: &mut M,
    now: f32,
) -> Result<bool, String> {
    if !mission.is_agent_alive()? {
        return Ok(true);
    }
    if now >= mission.deadline() {
        mission.on_timeout(now)?;
        return Ok(true);
    }
    match mission.on_stage(mission.stage(), now)? {
        MultiStageStep::Continue => Ok(false),
        MultiStageStep::Transition(next) => {
            mission.set_stage(next);
            Ok(false)
        }
        MultiStageStep::Complete => Ok(true),
    }
}

pub fn advance_multi_stage_owned<M: MultiStageMission>(
    mut mission: M,
    now: f32,
    on_error: impl Fn(&M, &str),
) -> Option<M> {
    let done = match advance_multi_stage(&mut mission, now) {
        Ok(done) => done,
        Err(error) => {
            on_error(&mission, &error);
            true
        }
    };
    if done {
        MultiStageMission::cleanup(mission);
        None
    } else {
        Some(mission)
    }
}

pub fn advance_multi_stage_all<M: MultiStageMission>(
    missions: &Mutex<Vec<M>>,
    now: f32,
    on_error: impl Fn(&M, &str),
) {
    let mut guard = missions.lock();
    let mut index = 0;
    while index < guard.len() {
        let done = match advance_multi_stage(&mut guard[index], now) {
            Ok(done) => done,
            Err(error) => {
                on_error(&guard[index], &error);
                true
            }
        };
        if done {
            guard.remove(index).cleanup();
        } else {
            index += 1;
        }
    }
}

/// Generate the three field-accessor methods every Mission impl
/// repeats identically. Call inside `impl Mission for MyStruct`.
#[macro_export]
macro_rules! mission_accessors {
    () => {
        fn stage(&self) -> $crate::mission::Stage {
            self.stage
        }
        fn set_stage(&mut self, s: $crate::mission::Stage) {
            self.stage = s;
        }
        fn deadline(&self) -> f32 {
            self.deadline
        }
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

    /// Called once when the mission reaches its deadline.
    fn on_timeout(&mut self, _now: f32) -> Result<(), String> {
        Ok(())
    }

    /// Release handles, squads, and any other resources. Called on
    /// normal completion, timeout, agent death, and error abort.
    fn cleanup(self);

    /// Short display label for error/abort log messages.
    fn label(&self) -> String;
}

impl<M: Mission> MultiStageMission for M {
    type Stage = Stage;

    fn stage(&self) -> Self::Stage {
        Mission::stage(self)
    }

    fn set_stage(&mut self, stage: Self::Stage) {
        Mission::set_stage(self, stage);
    }

    fn deadline(&self) -> f32 {
        Mission::deadline(self)
    }

    fn is_agent_alive(&self) -> Result<bool, String> {
        Mission::is_agent_alive(self)
    }

    fn on_stage(
        &mut self,
        stage: Self::Stage,
        now: f32,
    ) -> Result<MultiStageStep<Self::Stage>, String> {
        match stage {
            Stage::Going => match self.on_going(now)? {
                Step::Continue => Ok(MultiStageStep::Continue),
                Step::Transition => Ok(MultiStageStep::Transition(Stage::Returning)),
                Step::Complete => Ok(MultiStageStep::Complete),
            },
            Stage::Returning => match self.on_returning(now)? {
                Step::Continue => Ok(MultiStageStep::Continue),
                Step::Transition | Step::Complete => Ok(MultiStageStep::Complete),
            },
        }
    }

    fn on_timeout(&mut self, now: f32) -> Result<(), String> {
        Mission::on_timeout(self, now)
    }

    fn cleanup(self) {
        Mission::cleanup(self);
    }

    fn label(&self) -> String {
        Mission::label(self)
    }
}

/// Advance one mission. Returns `Ok(true)` when the mission is
/// done and should be removed.
pub fn advance<M: Mission>(m: &mut M, now: f32) -> Result<bool, String> {
    advance_multi_stage(m, now)
}

/// Advance a mission stored as an owned value. Completed and
/// failed missions are cleaned up; pending missions are returned.
pub fn advance_owned<M: Mission>(mission: M, now: f32, on_error: impl Fn(&M, &str)) -> Option<M> {
    advance_multi_stage_owned(mission, now, on_error)
}

/// Advance all missions in a locked vec. Completed or errored
/// missions are removed and cleaned up. `on_error` receives the
/// mission and error string for game-specific logging.
pub fn advance_all<M: Mission>(missions: &Mutex<Vec<M>>, now: f32, on_error: impl Fn(&M, &str)) {
    advance_multi_stage_all(missions, now, on_error);
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

// ---- one-stage missions ----------------------------------------

pub enum OneStageStep {
    Continue,
    Complete,
    TimedOut,
}

/// A mission observed or resolved in one stage. The consumer owns
/// its game checks and outcome; the framework owns removal, timeout
/// dispatch, error routing, and cleanup.
pub trait OneStageMission: Sized {
    fn advance(&mut self, now: f32) -> Result<OneStageStep, String>;

    fn on_timeout(&mut self, _now: f32) -> Result<(), String> {
        Ok(())
    }

    fn cleanup(self);
    fn label(&self) -> String;
}

pub fn advance_one_stage<M: OneStageMission>(mission: &mut M, now: f32) -> Result<bool, String> {
    match mission.advance(now)? {
        OneStageStep::Continue => Ok(false),
        OneStageStep::Complete => Ok(true),
        OneStageStep::TimedOut => {
            mission.on_timeout(now)?;
            Ok(true)
        }
    }
}

pub fn advance_one_stage_all<M: OneStageMission>(
    missions: &Mutex<Vec<M>>,
    now: f32,
    on_error: impl Fn(&M, &str),
) {
    let mut guard = missions.lock();
    let mut index = 0;
    while index < guard.len() {
        let done = match advance_one_stage(&mut guard[index], now) {
            Ok(done) => done,
            Err(error) => {
                on_error(&guard[index], &error);
                true
            }
        };
        if done {
            guard.remove(index).cleanup();
        } else {
            index += 1;
        }
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
pub fn advance_contract<C: Contract>(slot: &Mutex<Option<C>>, now: f32, on_error: impl Fn(String)) {
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
