//! Actors as data, and the AI that drives one. An `ActorDef` says what
//! kind of person this is: health, protection, what they hold, which
//! faction, and how they behave. The consumer's one spawn path reads
//! a def and builds the actor with exactly the parts the player has
//! (inventory, hotbar, equipment, survival, health, protection, an
//! action queue). `decide` is the AI: given what the actor can see
//! this tick, it returns the actions to push. An NPC and the player
//! are then the same thing with a different hand on the queue.

use crate::actions::Action;
use crate::combat::{Health, Protection};

/// How an actor behaves when left alone and when it sees a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behaviour {
    /// Stands where spawned. Turns toward a target in sight and
    /// attacks it when in reach.
    Guard,
    /// Walks toward a target in sight and attacks it when in reach;
    /// stands still otherwise.
    Hunter,
}

/// One kind of actor, as data.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorDef {
    pub name: String,
    pub faction: String,
    pub behaviour: Behaviour,
    pub max_health: f32,
    pub protection: Protection,
    /// Item in the weapon slot at spawn, if any.
    pub weapon: Option<String>,
    /// How far the actor notices a target.
    pub sight: f32,
    /// How close the actor wants to be before attacking.
    pub reach: f32,
}

impl ActorDef {
    pub fn health(&self) -> Health {
        Health::new(self.max_health)
    }
}

#[derive(Debug, Default)]
pub struct ActorRegistry {
    defs: Vec<ActorDef>,
}

impl ActorRegistry {
    pub fn register(&mut self, def: ActorDef) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("actor '{}' registered twice", def.name));
        }
        self.defs.push(def);
        Ok(())
    }

    pub fn def(&self, name: &str) -> Option<&ActorDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.iter().map(|d| d.name.as_str())
    }
}

/// What the AI can see this tick, gathered by the consumer from its
/// world. Positions are flat (x, z on the ground); the consumer
/// decides what counts as a target (a hostile faction, in line of
/// sight).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sight {
    /// The actor's own facing, radians, same convention as `Look`.
    pub yaw: f32,
    /// Flat offset from the actor to the nearest target, if any.
    pub target: Option<[f32; 2]>,
}

/// Turn rate cap per tick, radians, so an NPC turns like a body and
/// not a teleport.
pub const TURN_RATE: f32 = 0.15;

/// The AI. Returns the actions for this tick: a look to face the
/// target, a move toward it (Hunter) and an attack once in reach.
/// Nothing in sight means nothing to do.
pub fn decide(def: &ActorDef, sight: &Sight) -> Vec<Action> {
    let Some([dx, dz]) = sight.target else {
        return Vec::new();
    };
    let distance = (dx * dx + dz * dz).sqrt();
    if distance > def.sight {
        return Vec::new();
    }
    let mut actions = Vec::new();
    // Facing: the world's forward is -z, yaw turns about y, so a
    // target at -z is yaw 0 and a target at -x is yaw +pi/2.
    let wanted = (-dx).atan2(-dz);
    let mut turn = wanted - sight.yaw;
    while turn > std::f32::consts::PI {
        turn -= std::f32::consts::TAU;
    }
    while turn < -std::f32::consts::PI {
        turn += std::f32::consts::TAU;
    }
    let turn = turn.clamp(-TURN_RATE, TURN_RATE);
    if turn != 0.0 {
        actions.push(Action::Look {
            yaw: turn,
            pitch: 0.0,
        });
    }
    if distance <= def.reach {
        actions.push(Action::Attack);
    } else if def.behaviour == Behaviour::Hunter {
        actions.push(Action::Move { x: 0.0, y: 1.0 });
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raider() -> ActorDef {
        ActorDef {
            name: "raider".into(),
            faction: "raiders".into(),
            behaviour: Behaviour::Hunter,
            max_health: 80.0,
            protection: Protection::default(),
            weapon: Some("pipe".into()),
            sight: 20.0,
            reach: 1.5,
        }
    }

    #[test]
    fn registry_rejects_a_duplicate_and_finds_by_name() {
        let mut registry = ActorRegistry::default();
        registry.register(raider()).unwrap();
        assert!(registry.register(raider()).is_err());
        assert_eq!(registry.def("raider").unwrap().max_health, 80.0);
        assert_eq!(registry.def("raider").unwrap().health().current, 80.0);
        assert!(registry.def("nobody").is_none());
    }

    #[test]
    fn nothing_in_sight_means_nothing_to_do() {
        let def = raider();
        assert!(decide(&def, &Sight { yaw: 0.0, target: None }).is_empty());
        assert!(
            decide(
                &def,
                &Sight {
                    yaw: 0.0,
                    target: Some([0.0, -50.0])
                }
            )
            .is_empty(),
            "beyond sight range"
        );
    }

    #[test]
    fn a_hunter_turns_toward_and_walks_at_a_target_then_attacks_in_reach() {
        let def = raider();
        // Target dead ahead (-z): no turn, walk.
        let far = decide(
            &def,
            &Sight {
                yaw: 0.0,
                target: Some([0.0, -10.0]),
            },
        );
        assert_eq!(far, vec![Action::Move { x: 0.0, y: 1.0 }]);
        // Target to the left (-x) wants yaw +pi/2: turn capped at TURN_RATE.
        let left = decide(
            &def,
            &Sight {
                yaw: 0.0,
                target: Some([-10.0, 0.0]),
            },
        );
        assert_eq!(
            left,
            vec![
                Action::Look {
                    yaw: TURN_RATE,
                    pitch: 0.0
                },
                Action::Move { x: 0.0, y: 1.0 }
            ]
        );
        // In reach: attack, no walk.
        let close = decide(
            &def,
            &Sight {
                yaw: 0.0,
                target: Some([0.0, -1.0]),
            },
        );
        assert_eq!(close, vec![Action::Attack]);
    }

    #[test]
    fn a_guard_turns_and_attacks_but_never_walks() {
        let mut def = raider();
        def.behaviour = Behaviour::Guard;
        let far = decide(
            &def,
            &Sight {
                yaw: 0.0,
                target: Some([0.0, -10.0]),
            },
        );
        assert!(far.is_empty());
        let close = decide(
            &def,
            &Sight {
                yaw: 0.0,
                target: Some([0.0, -1.0]),
            },
        );
        assert_eq!(close, vec![Action::Attack]);
    }

    #[test]
    fn the_turn_takes_the_short_way_round() {
        let def = raider();
        // Facing -3.1 (almost fully round), target behind and a hair
        // left wants +3.13: the long way is +6.2, the short way is
        // -0.05.
        let actions = decide(
            &def,
            &Sight {
                yaw: -3.1,
                target: Some([-0.1, 10.0]),
            },
        );
        match actions[0] {
            Action::Look { yaw, .. } => assert!(yaw < 0.0 && yaw >= -TURN_RATE, "{yaw}"),
            _ => panic!("expected a look first"),
        }
    }
}
