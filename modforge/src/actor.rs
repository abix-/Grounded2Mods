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
    /// A registered faction, or None for a neutral who belongs to no
    /// side (design.md).
    pub faction: Option<String>,
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

/// The one persistent identity of an actor: issued once by the
/// `ActorMap`, never reused, kept across save and load. The host's
/// runtime handle and the GPU row are bookkeeping; this is the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActorId(pub u64);

/// A record of one live actor: its host handle and its GPU row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorRecord<H> {
    pub handle: H,
    pub row: usize,
}

/// The one actor registry (Endless's EntityMap and GpuSlotPool folded
/// together). Issues ids and rows at spawn, frees rows at removal,
/// answers lookups by id, by handle, and by row. Generic over the
/// host's runtime handle so any game can use it. Two counts, two
/// methods: `row_count` is the high-water mark for sizing buffers,
/// `live_count` is how many actors exist now. Never one number.
#[derive(Debug)]
pub struct ActorMap<H> {
    next_id: u64,
    free_rows: Vec<usize>,
    next_row: usize,
    max_rows: usize,
    by_id: std::collections::HashMap<ActorId, ActorRecord<H>>,
    by_handle: std::collections::HashMap<H, ActorId>,
    by_row: Vec<Option<ActorId>>,
}

impl<H: Copy + Eq + std::hash::Hash> ActorMap<H> {
    /// `max_rows` bounds every row-indexed buffer.
    pub fn new(max_rows: usize) -> Self {
        Self {
            next_id: 1,
            free_rows: Vec::new(),
            next_row: 0,
            max_rows,
            by_id: Default::default(),
            by_handle: Default::default(),
            by_row: Vec::new(),
        }
    }

    /// Register a new actor: a fresh id and a row (a freed row first,
    /// LIFO). None when every row is taken.
    pub fn spawn(&mut self, handle: H) -> Option<(ActorId, usize)> {
        let row = match self.free_rows.pop() {
            Some(row) => row,
            None if self.next_row < self.max_rows => {
                self.next_row += 1;
                self.next_row - 1
            }
            None => return None,
        };
        let id = ActorId(self.next_id);
        self.next_id += 1;
        self.by_id.insert(id, ActorRecord { handle, row });
        self.by_handle.insert(handle, id);
        if self.by_row.len() <= row {
            self.by_row.resize(row + 1, None);
        }
        self.by_row[row] = Some(id);
        Some((id, row))
    }

    /// Forget an actor and free its row. Returns its record, or None
    /// if it was not live (a second remove is a no-op).
    pub fn remove(&mut self, id: ActorId) -> Option<ActorRecord<H>> {
        let record = self.by_id.remove(&id)?;
        self.by_handle.remove(&record.handle);
        self.by_row[record.row] = None;
        self.free_rows.push(record.row);
        Some(record)
    }

    pub fn handle_of(&self, id: ActorId) -> Option<H> {
        self.by_id.get(&id).map(|r| r.handle)
    }

    pub fn row_of(&self, id: ActorId) -> Option<usize> {
        self.by_id.get(&id).map(|r| r.row)
    }

    pub fn id_of(&self, handle: H) -> Option<ActorId> {
        self.by_handle.get(&handle).copied()
    }

    pub fn id_at_row(&self, row: usize) -> Option<ActorId> {
        self.by_row.get(row).copied().flatten()
    }

    /// High-water row count: the bound for buffer sizing and dispatch.
    /// Includes freed rows.
    pub fn row_count(&self) -> usize {
        self.next_row
    }

    /// How many actors exist right now.
    pub fn live_count(&self) -> usize {
        self.by_id.len()
    }

    pub fn ids(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.by_id.keys().copied()
    }
}

/// The seven signed axes of a person (Endless behavior.md; topside
/// design.md's Diablo 2 enemy traits through the genome). Each is a
/// value in -1.5..=1.5: the sign is the pole (Brave or Coward), the
/// magnitude how strongly. Zero is unremarkable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Brave (+) never flees; Coward (-) flees early.
    Courage,
    /// Efficient (+) works more; Lazy (-) wanders more.
    Diligence,
    /// Hardy (+) has more health; Frail (-) less.
    Vitality,
    /// Strong (+) hits harder; Weak (-) softer.
    Power,
    /// Swift (+) walks faster; Slow (-) slower.
    Agility,
    /// Sharpshot (+) reaches farther; Myopic (-) shorter.
    Precision,
    /// Berserker (+) hits harder when hurt; Timid (-) flees more.
    Ferocity,
}

impl Axis {
    pub const ALL: [Axis; 7] = [
        Axis::Courage,
        Axis::Diligence,
        Axis::Vitality,
        Axis::Power,
        Axis::Agility,
        Axis::Precision,
        Axis::Ferocity,
    ];

    pub const NAMES: [&'static str; 7] = [
        "courage",
        "diligence",
        "vitality",
        "power",
        "agility",
        "precision",
        "ferocity",
    ];
}

/// The genome pool every person's axes live in, so they can learn
/// and persist the way factions do (crate::genome).
pub static PEOPLE: crate::genome::Pool = crate::genome::Pool::new(crate::genome::PoolConfig {
    traits: &Axis::NAMES,
    learn_rate: 0.05,
    min: -1.5,
    max: 1.5,
});

/// One person's personality: the seven axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Personality {
    pub axes: [f32; 7],
}

impl Default for Personality {
    fn default() -> Self {
        Self { axes: [0.0; 7] }
    }
}

impl Personality {
    /// Seed a person from the genome by id and world seed: up to two
    /// axes get a signed magnitude of 0.5 to 1.5 (Endless: "0-2
    /// spectrum traits"), the rest stay zero. The same id and seed
    /// give the same person; the pool remembers it after.
    pub fn seed(id: ActorId, world_seed: u64) -> Personality {
        let key = (id.0 as i64) ^ (world_seed as i64).rotate_left(17);
        let values = PEOPLE.get_or_seed(key, || {
            let mut axes = vec![0.0f64; 7];
            // How many axes: 0, 1, or 2, weighted toward 1.
            let count = match (crate::genome::jitter(key, 1, 1.0) + 1.0) * 1.5 {
                x if x < 0.6 => 0,
                x if x < 2.1 => 1,
                _ => 2,
            };
            let mut picked = Vec::new();
            let mut salt = 10;
            while picked.len() < count {
                let axis = (((crate::genome::jitter(key, salt, 1.0) + 1.0) / 2.0) * 7.0) as usize;
                let axis = axis.min(6);
                salt += 1;
                if picked.contains(&axis) {
                    continue;
                }
                picked.push(axis);
                let magnitude = 0.5 + ((crate::genome::jitter(key, salt, 1.0) + 1.0) / 2.0);
                let sign = if crate::genome::jitter(key, salt + 100, 1.0) < 0.0 {
                    -1.0
                } else {
                    1.0
                };
                axes[axis] = sign * magnitude;
                salt += 1;
            }
            axes
        });
        let mut axes = [0.0f32; 7];
        for (i, v) in values.iter().enumerate().take(7) {
            axes[i] = *v as f32;
        }
        Personality { axes }
    }

    pub fn get(&self, axis: Axis) -> f32 {
        self.axes[axis as usize]
    }

    /// Stat multipliers (Endless: plus or minus 25 percent per unit).
    pub fn health_mult(&self) -> f32 {
        1.0 + 0.25 * self.get(Axis::Vitality)
    }

    /// Damage dealt; a Berserker hits half again as hard under half
    /// health, a Timid one less.
    pub fn damage_mult(&self, health_fraction: f32) -> f32 {
        let base = 1.0 + 0.25 * self.get(Axis::Power);
        let hurt = if health_fraction < 0.5 {
            1.0 + 0.5 * self.get(Axis::Ferocity)
        } else {
            1.0
        };
        base * hurt
    }

    pub fn speed_mult(&self) -> f32 {
        1.0 + 0.25 * self.get(Axis::Agility)
    }

    pub fn range_mult(&self) -> f32 {
        1.0 + 0.25 * self.get(Axis::Precision)
    }

    /// The health fraction below which this person flees: a base of
    /// 0.3, pushed up 20 percent per unit of cowardice, and a Brave
    /// person never flees.
    pub fn flee_line(&self) -> f32 {
        let courage = self.get(Axis::Courage);
        if courage > 0.0 {
            return 0.0;
        }
        let timid = (-self.get(Axis::Ferocity)).max(0.0);
        (0.3 + 0.2 * (-courage) + 0.1 * timid).min(0.9)
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
            faction: Some("raiders".into()),
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
    fn ids_are_never_reused_and_rows_are() {
        let mut map: ActorMap<u32> = ActorMap::new(8);
        let (a, ra) = map.spawn(10).unwrap();
        let (b, rb) = map.spawn(11).unwrap();
        assert_ne!(a, b);
        assert_ne!(ra, rb);
        assert!(map.remove(a).is_some());
        assert!(map.remove(a).is_none(), "a second remove is a no-op");
        let (c, rc) = map.spawn(12).unwrap();
        assert!(c != a && c != b, "ids never come back");
        assert_eq!(rc, ra, "the freed row does");
        assert_eq!(map.live_count(), 2);
        assert_eq!(map.row_count(), 2, "high-water, not live");
    }

    #[test]
    fn every_lookup_resolves_and_stale_ones_do_not() {
        let mut map: ActorMap<u32> = ActorMap::new(8);
        let (a, ra) = map.spawn(10).unwrap();
        assert_eq!(map.handle_of(a), Some(10));
        assert_eq!(map.id_of(10), Some(a));
        assert_eq!(map.row_of(a), Some(ra));
        assert_eq!(map.id_at_row(ra), Some(a));
        map.remove(a);
        assert_eq!(map.handle_of(a), None);
        assert_eq!(map.id_of(10), None);
        assert_eq!(map.id_at_row(ra), None);
        // The row is reused by someone else: the old id still resolves
        // to nothing, the row to the new actor.
        let (b, rb) = map.spawn(20).unwrap();
        assert_eq!(rb, ra);
        assert_eq!(map.id_at_row(ra), Some(b));
        assert_eq!(map.row_of(a), None);
    }

    #[test]
    fn rows_run_out_at_the_bound() {
        let mut map: ActorMap<u32> = ActorMap::new(2);
        assert!(map.spawn(1).is_some());
        assert!(map.spawn(2).is_some());
        assert!(map.spawn(3).is_none());
        assert_eq!(map.live_count(), 2);
    }

    #[test]
    fn personality_seeds_the_same_person_twice_and_people_differ() {
        let a = Personality::seed(ActorId(5), 42);
        let b = Personality::seed(ActorId(5), 42);
        assert_eq!(a, b);
        // Across a hundred people: at most two axes each, magnitudes
        // in 0.5..=1.5, and not everyone the same.
        let mut distinct = std::collections::HashSet::new();
        let mut with_traits = 0;
        for id in 1..=100u64 {
            let p = Personality::seed(ActorId(id), 42);
            let set: Vec<f32> = p.axes.iter().copied().filter(|v| *v != 0.0).collect();
            assert!(set.len() <= 2, "{id}: {:?}", p.axes);
            for v in &set {
                assert!((0.5..=1.5).contains(&v.abs()), "{id}: {v}");
            }
            if !set.is_empty() {
                with_traits += 1;
            }
            distinct.insert(p.axes.map(|v| v.to_bits()));
        }
        assert!(with_traits > 50, "most people have a trait: {with_traits}");
        assert!(distinct.len() > 20, "people differ: {}", distinct.len());
        // Another world seed, another person.
        assert_ne!(Personality::seed(ActorId(5), 43).axes, a.axes);
    }

    #[test]
    fn axes_move_the_stats_the_endless_way() {
        let mut p = Personality::default();
        assert_eq!(p.health_mult(), 1.0);
        assert_eq!(p.flee_line(), 0.3);
        p.axes[Axis::Vitality as usize] = 1.0;
        assert_eq!(p.health_mult(), 1.25);
        p.axes[Axis::Power as usize] = -1.0;
        assert_eq!(p.damage_mult(1.0), 0.75);
        p.axes[Axis::Ferocity as usize] = 1.0;
        assert_eq!(p.damage_mult(0.4), 0.75 * 1.5, "berserker under half health");
        p.axes[Axis::Courage as usize] = -1.0;
        assert!((p.flee_line() - 0.5).abs() < 1e-6, "a coward flees at half");
        p.axes[Axis::Courage as usize] = 1.0;
        assert_eq!(p.flee_line(), 0.0, "the brave never flee");
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
