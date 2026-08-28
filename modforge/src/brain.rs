//! The brain (topside life.md "The brain"): one pure function that
//! turns what a person perceives into what it does next. No engine,
//! no entities: a `Perception` in, a `Decision` out, the state it
//! keeps between thinks (`Activity`, `CombatState`) owned by the
//! consumer and handed back each time. Every random choice comes
//! from a `Roll` the consumer seeds by tick and ActorId, so a replay
//! decides the same.
//!
//! Prior art: Endless's decision system (the ordered hard rules,
//! activity kept through a fight), The Sims' needs (the worst need
//! drives the pick; what is known, not what exists, is considered),
//! Halo 2's lesson that the brain never reads the world directly.

use glam::Vec3;

use crate::actions::Action;
use crate::actor::{ActorId, Behaviour, Personality, TURN_RATE};
use crate::memory::{Known, Memory};
use crate::monument::Roll;
use crate::survival::{Need, SurvivalStats};

/// What a person is doing, kept between thinks.
#[derive(Clone, Debug, PartialEq)]
pub enum Activity {
    Idle,
    /// Walking to a known thing to satisfy a need.
    Going {
        key: u64,
        to: Vec3,
        need: Need,
    },
    /// At a known thing, doing it.
    Doing {
        key: u64,
        what: Doing,
    },
    /// Walking to a known but unchecked thing to see what it holds.
    Looking {
        key: u64,
        to: Vec3,
    },
    /// Strolling to a point near home.
    Wander {
        to: Vec3,
    },
    /// Walking home.
    GoHome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    Eat,
    Sleep,
    Check,
}

/// Whether a person is fighting, kept between thinks and kept apart
/// from the activity so an errand resumes after a fight (Endless).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatState {
    None,
    Fighting { target: ActorId, began_at: Vec3 },
    Fleeing { from: ActorId },
}

/// What the consumer saw this think. Positions are world metres; y
/// is up; facing is the yaw the actions' `Look` turns.
#[derive(Clone)]
pub struct Perception<'a> {
    pub now: u64,
    pub position: Vec3,
    pub yaw: f32,
    pub health_fraction: f32,
    pub needs: SurvivalStats,
    pub home: Option<Vec3>,
    pub behaviour: Behaviour,
    pub at_home: bool,
    pub asleep: bool,
    /// The nearest hostile in sight, if any.
    pub hostile: Option<(ActorId, Vec3)>,
    /// Within reach of the current activity's target.
    pub arrived: bool,
    pub memory: &'a Memory,
    pub personality: &'a Personality,
    /// The registry's answer for a remembered thing: how much its
    /// kind is worth for a need (RimWorld: the def says, memory only
    /// remembers the kind). The consumer builds it from its
    /// registries; the brain never reads the world.
    pub worth: &'a dyn Fn(&Known, Need) -> f32,
}

impl std::fmt::Debug for Perception<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Perception")
            .field("now", &self.now)
            .field("position", &self.position)
            .field("needs", &self.needs)
            .field("hostile", &self.hostile)
            .field("arrived", &self.arrived)
            .finish_non_exhaustive()
    }
}

/// The one thing the consumer must carry out this tick with its own
/// data, because the brain cannot touch the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Do {
    /// Take food from the known thing `key` and eat it.
    Eat {
        key: u64,
    },
    Sleep,
    Wake,
    /// Look inside the known thing `key` and note what it held.
    Check {
        key: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub activity: Activity,
    pub combat: CombatState,
    pub actions: Vec<Action>,
    pub do_now: Option<Do>,
}

/// How far a person chases before breaking off back home.
pub const LEASH: f32 = 60.0;
/// A need below this sends a person to something it knows.
pub const NEED_LINE: f32 = 50.0;
/// A need below this, with nothing known to answer it, sends a person
/// looking.
pub const LOOK_LINE: f32 = 30.0;
/// Within this of a target counts as there.
pub const REACH: f32 = 1.5;
/// Metres of walking that cost one point of satisfaction.
const METRES_PER_POINT: f32 = 4.0;

/// The brain. The first rule that applies wins.
pub fn decide(
    p: &Perception,
    activity: &Activity,
    combat: &CombatState,
    roll: &mut Roll,
) -> Decision {
    if let Some(d) = arrived(p, activity, combat) {
        return d;
    }
    if let Some(d) = flee(p, activity, combat) {
        return d;
    }
    if let Some(d) = leash(p, activity, combat) {
        return d;
    }
    if let Some(d) = fight(p, activity, combat) {
        return d;
    }
    // Out of a fight: the life.
    let combat = CombatState::None;
    if let Some(d) = keep_going(p, activity, combat) {
        return d;
    }
    if let Some(d) = life(p, combat) {
        return d;
    }
    if let Some(d) = look(p, combat) {
        return d;
    }
    wander(p, activity, combat, roll)
}

/// Rule 1: at the target, start doing; done doing, stop.
fn arrived(p: &Perception, activity: &Activity, combat: &CombatState) -> Option<Decision> {
    match activity {
        Activity::Going { key, need, .. } if p.arrived => {
            let (what, do_now) = match need {
                Need::Hunger | Need::Thirst => (Doing::Eat, Some(Do::Eat { key: *key })),
                Need::Rest => (Doing::Sleep, Some(Do::Sleep)),
                Need::Safety => (Doing::Check, None),
            };
            Some(Decision {
                activity: Activity::Doing { key: *key, what },
                combat: *combat,
                actions: vec![],
                do_now,
            })
        }
        Activity::Looking { key, .. } if p.arrived => Some(Decision {
            activity: Activity::Doing {
                key: *key,
                what: Doing::Check,
            },
            combat: *combat,
            actions: vec![],
            do_now: Some(Do::Check { key: *key }),
        }),
        Activity::Doing { key, what } => {
            // Keep eating while hungry and the thing still feeds; keep
            // sleeping until rested; a check is one look.
            let done = match what {
                Doing::Eat => {
                    let fed = p.needs.hunger >= 90.0 && p.needs.thirst >= 90.0;
                    let empty = !p
                        .memory
                        .known
                        .iter()
                        .any(|k| k.key == *key && k.believed_to_hold());
                    fed || empty
                }
                Doing::Sleep => p.needs.rest >= 95.0 || p.hostile.is_some(),
                Doing::Check => true,
            };
            if !done {
                let do_now = match what {
                    Doing::Eat => Some(Do::Eat { key: *key }),
                    _ => None,
                };
                return Some(Decision {
                    activity: activity.clone(),
                    combat: *combat,
                    actions: vec![],
                    do_now,
                });
            }
            Some(Decision {
                activity: Activity::Idle,
                combat: *combat,
                actions: vec![],
                do_now: (*what == Doing::Sleep).then_some(Do::Wake),
            })
        }
        Activity::GoHome if p.at_home => Some(Decision {
            activity: Activity::Idle,
            combat: CombatState::None,
            actions: vec![],
            do_now: None,
        }),
        Activity::Wander { .. } if p.arrived => Some(Decision {
            activity: Activity::Idle,
            combat: *combat,
            actions: vec![],
            do_now: None,
        }),
        _ => None,
    }
}

/// Rule 2: hurt past the flee line, run home.
fn flee(p: &Perception, _activity: &Activity, combat: &CombatState) -> Option<Decision> {
    let from = match combat {
        CombatState::Fighting { target, .. } => *target,
        CombatState::Fleeing { from } => *from,
        CombatState::None => return None,
    };
    if p.health_fraction >= p.personality.flee_line() {
        // A fleeing person who is safe again stops fleeing.
        if matches!(combat, CombatState::Fleeing { .. }) && p.hostile.is_none() {
            return Some(Decision {
                activity: Activity::GoHome,
                combat: CombatState::None,
                actions: vec![],
                do_now: None,
            });
        }
        return None;
    }
    Some(Decision {
        activity: Activity::GoHome,
        combat: CombatState::Fleeing { from },
        actions: walk_toward(p, p.home.unwrap_or(p.position)),
        do_now: None,
    })
}

/// Rule 3: chased too far from where the fight began, break off.
fn leash(p: &Perception, _activity: &Activity, combat: &CombatState) -> Option<Decision> {
    let CombatState::Fighting { began_at, .. } = combat else {
        return None;
    };
    if p.position.distance(*began_at) <= LEASH {
        return None;
    }
    Some(Decision {
        activity: Activity::GoHome,
        combat: CombatState::None,
        actions: walk_toward(p, p.home.unwrap_or(*began_at)),
        do_now: None,
    })
}

/// Rule 4: a hostile in sight: face it, close on it (hunters), hit
/// it in reach. The activity is kept underneath (Endless).
fn fight(p: &Perception, activity: &Activity, combat: &CombatState) -> Option<Decision> {
    let (who, at) = p.hostile?;
    let began_at = match combat {
        CombatState::Fighting { began_at, .. } => *began_at,
        _ => p.position,
    };
    let mut actions = turn_toward(p, at);
    let distance = p.position.distance(at);
    if distance <= reach_of(p) {
        actions.push(Action::Attack);
    } else if p.behaviour == Behaviour::Hunter {
        actions.push(Action::Move { x: 0.0, y: 1.0 });
    }
    Some(Decision {
        activity: activity.clone(),
        combat: CombatState::Fighting {
            target: who,
            began_at,
        },
        actions,
        do_now: (p.asleep).then_some(Do::Wake),
    })
}

/// Rule 5: an errand under way keeps going.
fn keep_going(p: &Perception, activity: &Activity, combat: CombatState) -> Option<Decision> {
    let to = match activity {
        Activity::Going { to, .. } | Activity::Looking { to, .. } | Activity::Wander { to } => *to,
        Activity::GoHome => p.home?,
        _ => return None,
    };
    Some(Decision {
        activity: activity.clone(),
        combat,
        actions: walk_toward(p, to),
        do_now: None,
    })
}

/// Rule 6: the worst need, when it presses, picks the best known
/// thing: what it gives minus the walk, the walk costing a Lazy
/// person more, plus a little seeded chance (inside `Roll`, the
/// consumer seeds it) so two people in one spot differ.
fn life(p: &Perception, combat: CombatState) -> Option<Decision> {
    let (need, value) = p.needs.worst_need();
    if value >= NEED_LINE {
        return None;
    }
    let diligence = p.personality.get(crate::actor::Axis::Diligence);
    let walk_cost = 1.0 - 0.25 * diligence;
    let best = p
        .memory
        .good_for(need, p.worth)
        .map(|(known, gives)| {
            let distance = known.position.distance(p.position);
            let score = gives - distance / METRES_PER_POINT * walk_cost;
            (known, score)
        })
        .filter(|(_, score)| *score > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let known = best.0;
    // Rest at home: sleep where you stand if this is home.
    if need == Need::Rest && p.at_home {
        return Some(Decision {
            activity: Activity::Doing {
                key: known.key,
                what: Doing::Sleep,
            },
            combat,
            actions: vec![],
            do_now: Some(Do::Sleep),
        });
    }
    Some(Decision {
        activity: Activity::Going {
            key: known.key,
            to: known.position,
            need,
        },
        combat,
        actions: walk_toward(p, known.position),
        do_now: None,
    })
}

/// Rule 7: nothing known answers a pressing need: go and look at
/// the nearest thing never checked.
fn look(p: &Perception, combat: CombatState) -> Option<Decision> {
    let (_, value) = p.needs.worst_need();
    if value >= LOOK_LINE {
        return None;
    }
    let known = p.memory.unchecked_nearest(p.position)?;
    Some(Decision {
        activity: Activity::Looking {
            key: known.key,
            to: known.position,
        },
        combat,
        actions: walk_toward(p, known.position),
        do_now: None,
    })
}

/// Rule 8: nothing presses: stroll to a rolled point within the home
/// radius, or stand a while.
fn wander(p: &Perception, activity: &Activity, combat: CombatState, roll: &mut Roll) -> Decision {
    if !matches!(activity, Activity::Idle) {
        // Something else was under way and still is; keep it.
        return Decision {
            activity: activity.clone(),
            combat,
            actions: vec![],
            do_now: None,
        };
    }
    // Most thinks while idle do nothing; one in ten starts a stroll.
    if !roll.chance(100) {
        return Decision {
            activity: Activity::Idle,
            combat,
            actions: vec![],
            do_now: None,
        };
    }
    let centre = p.home.unwrap_or(p.position);
    let radius = p.behaviour.home_radius();
    let angle = roll.measure(0.0, std::f32::consts::TAU);
    let distance = roll.measure(radius * 0.3, radius);
    let to = centre + Vec3::new(angle.cos() * distance, 0.0, angle.sin() * distance);
    Decision {
        activity: Activity::Wander { to },
        combat,
        actions: walk_toward(p, to),
        do_now: None,
    }
}

fn reach_of(p: &Perception) -> f32 {
    1.8 * p.personality.range_mult()
}

/// Face a point: the short way round, capped per tick.
fn turn_toward(p: &Perception, to: Vec3) -> Vec<Action> {
    let dx = to.x - p.position.x;
    let dz = to.z - p.position.z;
    if dx.abs() < 1e-4 && dz.abs() < 1e-4 {
        return vec![];
    }
    // The world's forward is -z, yaw turns about y.
    let wanted = (-dx).atan2(-dz);
    let mut turn = wanted - p.yaw;
    while turn > std::f32::consts::PI {
        turn -= std::f32::consts::TAU;
    }
    while turn < -std::f32::consts::PI {
        turn += std::f32::consts::TAU;
    }
    let turn = turn.clamp(-TURN_RATE, TURN_RATE);
    if turn == 0.0 {
        return vec![];
    }
    vec![Action::Look {
        yaw: turn,
        pitch: 0.0,
    }]
}

/// Face a point and walk forward, unless already there.
fn walk_toward(p: &Perception, to: Vec3) -> Vec<Action> {
    let flat = (to - p.position).with_y(0.0);
    if flat.length() <= REACH {
        return vec![];
    }
    let mut actions = turn_toward(p, to);
    actions.push(Action::Move { x: 0.0, y: 1.0 });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Axis;

    fn calm() -> Personality {
        Personality::default()
    }

    /// The registry's answer in these tests: a storage box is worth
    /// 50 for hunger, home 100 for rest, a wreck nothing.
    fn worth(known: &Known, need: Need) -> f32 {
        match (known.kind.as_str(), need) {
            ("storage box", Need::Hunger) => 50.0,
            ("home", Need::Rest) => 100.0,
            _ => 0.0,
        }
    }

    fn perception<'a>(memory: &'a Memory, personality: &'a Personality) -> Perception<'a> {
        Perception {
            now: 100,
            position: Vec3::ZERO,
            yaw: 0.0,
            health_fraction: 1.0,
            needs: SurvivalStats::default(),
            home: Some(Vec3::ZERO),
            behaviour: Behaviour::Hunter,
            at_home: true,
            asleep: false,
            hostile: None,
            arrived: false,
            memory,
            personality,
            worth: &worth,
        }
    }

    fn has_move(d: &Decision) -> bool {
        d.actions.iter().any(|a| matches!(a, Action::Move { .. }))
    }

    #[test]
    fn a_wounded_coward_flees_home_and_a_brave_one_stands() {
        let memory = Memory::default();
        let mut coward = calm();
        coward.axes[Axis::Courage as usize] = -1.0;
        let mut p = perception(&memory, &coward);
        p.home = Some(Vec3::new(0.0, 0.0, 30.0));
        p.at_home = false;
        p.health_fraction = 0.4;
        p.hostile = Some((ActorId(9), Vec3::new(0.0, 0.0, -3.0)));
        let combat = CombatState::Fighting {
            target: ActorId(9),
            began_at: Vec3::ZERO,
        };
        let mut roll = Roll::new(1);
        let d = decide(&p, &Activity::Idle, &combat, &mut roll);
        assert_eq!(d.combat, CombatState::Fleeing { from: ActorId(9) });
        assert_eq!(d.activity, Activity::GoHome);
        assert!(has_move(&d), "runs");
        assert!(!d.actions.contains(&Action::Attack));

        let brave = {
            let mut b = calm();
            b.axes[Axis::Courage as usize] = 1.0;
            b
        };
        let p2 = Perception {
            personality: &brave,
            ..p.clone()
        };
        let d = decide(&p2, &Activity::Idle, &combat, &mut roll);
        assert!(
            matches!(d.combat, CombatState::Fighting { .. }),
            "the brave stand"
        );
    }

    #[test]
    fn a_hunter_chased_past_its_leash_goes_home() {
        let memory = Memory::default();
        let personality = calm();
        let mut p = perception(&memory, &personality);
        p.position = Vec3::new(0.0, 0.0, -(LEASH + 5.0));
        p.at_home = false;
        p.hostile = Some((ActorId(9), Vec3::new(0.0, 0.0, -(LEASH + 8.0))));
        let combat = CombatState::Fighting {
            target: ActorId(9),
            began_at: Vec3::ZERO,
        };
        let d = decide(&p, &Activity::Idle, &combat, &mut Roll::new(1));
        assert_eq!(d.combat, CombatState::None);
        assert_eq!(d.activity, Activity::GoHome);
        assert!(has_move(&d));
    }

    #[test]
    fn a_hostile_in_sight_is_fought_and_the_errand_is_kept_underneath() {
        let memory = Memory::default();
        let personality = calm();
        let mut p = perception(&memory, &personality);
        p.hostile = Some((ActorId(9), Vec3::new(0.0, 0.0, -1.0)));
        let errand = Activity::Wander {
            to: Vec3::new(10.0, 0.0, 0.0),
        };
        let d = decide(&p, &errand, &CombatState::None, &mut Roll::new(1));
        assert!(d.actions.contains(&Action::Attack), "in reach: swing");
        assert_eq!(d.activity, errand, "the stroll waits");
        assert_eq!(
            d.combat,
            CombatState::Fighting {
                target: ActorId(9),
                began_at: Vec3::ZERO
            }
        );
        // A guard never chases, a hunter does.
        p.hostile = Some((ActorId(9), Vec3::new(0.0, 0.0, -10.0)));
        p.behaviour = Behaviour::Guard;
        let d = decide(&p, &errand, &CombatState::None, &mut Roll::new(1));
        assert!(!has_move(&d));
        p.behaviour = Behaviour::Hunter;
        let d = decide(&p, &errand, &CombatState::None, &mut Roll::new(1));
        assert!(has_move(&d));
    }

    #[test]
    fn a_hungry_person_goes_to_the_remembered_box_and_eats_until_fed() {
        let mut memory = Memory::default();
        memory.see(7, "storage box", Vec3::new(0.0, 0.0, -12.0), 1);
        let personality = calm();
        let mut p = perception(&memory, &personality);
        p.needs.hunger = 30.0;
        let mut roll = Roll::new(1);
        let d = decide(&p, &Activity::Idle, &CombatState::None, &mut roll);
        assert_eq!(
            d.activity,
            Activity::Going {
                key: 7,
                to: Vec3::new(0.0, 0.0, -12.0),
                need: Need::Hunger
            }
        );
        assert!(has_move(&d));
        // Arrived: eat.
        p.arrived = true;
        p.position = Vec3::new(0.0, 0.0, -11.0);
        let d = decide(&p, &d.activity, &CombatState::None, &mut roll);
        assert_eq!(d.do_now, Some(Do::Eat { key: 7 }));
        let eating = d.activity.clone();
        // Still hungry: eat again. Fed: done.
        let d = decide(&p, &eating, &CombatState::None, &mut roll);
        assert_eq!(d.do_now, Some(Do::Eat { key: 7 }));
        p.needs.hunger = 95.0;
        let d = decide(&p, &eating, &CombatState::None, &mut roll);
        assert_eq!(d.activity, Activity::Idle);
        assert_eq!(d.do_now, None);
    }

    #[test]
    fn an_empty_box_sends_a_hungry_person_looking_at_the_nearest_unchecked_thing() {
        let mut memory = Memory::default();
        memory.see(7, "storage box", Vec3::new(0.0, 0.0, -2.0), 1);
        memory.checked(7, vec![], 2);
        memory.see(8, "wreck", Vec3::new(40.0, 0.0, 0.0), 1);
        memory.see(9, "wreck", Vec3::new(-20.0, 0.0, 0.0), 1);
        let personality = calm();
        let mut p = perception(&memory, &personality);
        p.needs.hunger = 20.0;
        let d = decide(&p, &Activity::Idle, &CombatState::None, &mut Roll::new(1));
        assert_eq!(
            d.activity,
            Activity::Looking {
                key: 9,
                to: Vec3::new(-20.0, 0.0, 0.0)
            },
            "the nearer wreck"
        );
        // Arrived at it: check it.
        p.arrived = true;
        let d = decide(&p, &d.activity, &CombatState::None, &mut Roll::new(1));
        assert_eq!(d.do_now, Some(Do::Check { key: 9 }));
    }

    #[test]
    fn a_tired_person_at_home_sleeps_and_wakes_rested() {
        let mut memory = Memory::default();
        memory.see(1, "home", Vec3::ZERO, 1);
        let personality = calm();
        let mut p = perception(&memory, &personality);
        p.needs.rest = 20.0;
        let d = decide(&p, &Activity::Idle, &CombatState::None, &mut Roll::new(1));
        assert_eq!(d.do_now, Some(Do::Sleep));
        let sleeping = d.activity.clone();
        p.asleep = true;
        p.needs.rest = 60.0;
        let d = decide(&p, &sleeping, &CombatState::None, &mut Roll::new(1));
        assert_eq!(d.activity, sleeping, "still asleep");
        p.needs.rest = 96.0;
        let d = decide(&p, &sleeping, &CombatState::None, &mut Roll::new(1));
        assert_eq!(d.do_now, Some(Do::Wake));
        assert_eq!(d.activity, Activity::Idle);
    }

    #[test]
    fn idle_people_wander_within_their_home_radius_and_guards_stay_close() {
        let memory = Memory::default();
        let personality = calm();
        let mut p = perception(&memory, &personality);
        p.home = Some(Vec3::new(100.0, 0.0, 100.0));
        for behaviour in [Behaviour::Guard, Behaviour::Hunter] {
            p.behaviour = behaviour;
            let mut roll = Roll::new(3);
            let mut farthest: f32 = 0.0;
            let mut strolls = 0;
            for _ in 0..400 {
                let d = decide(&p, &Activity::Idle, &CombatState::None, &mut roll);
                if let Activity::Wander { to } = d.activity {
                    strolls += 1;
                    farthest = farthest.max(to.distance(p.home.unwrap()));
                    assert!(has_move(&d));
                }
            }
            assert!(
                strolls > 10 && strolls < 120,
                "{behaviour:?}: {strolls} strolls in 400"
            );
            assert!(
                farthest <= behaviour.home_radius() + 1e-3,
                "{behaviour:?}: {farthest}"
            );
            assert!(farthest > behaviour.home_radius() * 0.5);
        }
    }

    #[test]
    fn the_same_roll_decides_the_same() {
        let memory = Memory::default();
        let personality = calm();
        let p = perception(&memory, &personality);
        let a: Vec<Decision> = (0..50)
            .scan(Roll::new(9), |roll, _| {
                Some(decide(&p, &Activity::Idle, &CombatState::None, roll))
            })
            .collect();
        let b: Vec<Decision> = (0..50)
            .scan(Roll::new(9), |roll, _| {
                Some(decide(&p, &Activity::Idle, &CombatState::None, roll))
            })
            .collect();
        assert_eq!(a, b);
    }
}
