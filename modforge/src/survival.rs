//! Survival stats: hunger and thirst on every actor. The player and
//! every NPC carry the same stats and run the same rules (topside
//! design.md "Survival pressures", operator-locked 2026-08-20).
//! Engine-agnostic: the consumer owns the component and the tick;
//! this module owns the numbers and the decisions.
//!
//! Prior art: the hunger and thirst bars of Atlas, Valheim, and
//! Project Zomboid. A stat runs from full to empty, drains per
//! second, food restores a fixed amount, empty is the consumer's
//! cue for starvation damage.

use crate::item::{Inventory, ItemDef, ItemRegistry};

/// Full value of each stat.
pub const FULL: f32 = 100.0;

/// The needs (topside life.md "The person"), each 0 (empty) to FULL.
/// Hunger and thirst are the survival pressures; rest is what sends
/// a person home at night; safety is what makes one flee or a camp
/// move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurvivalStats {
    pub hunger: f32,
    pub thirst: f32,
    pub rest: f32,
    pub safety: f32,
}

impl Default for SurvivalStats {
    fn default() -> Self {
        Self {
            hunger: FULL,
            thirst: FULL,
            rest: FULL,
            safety: FULL,
        }
    }
}

/// Which need a person has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Hunger,
    Thirst,
    Rest,
    Safety,
}

/// What is true of a person this tick, for the needs that depend on
/// it. The consumer fills it from its world; the brain sets `asleep`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Condition {
    pub asleep: bool,
    pub at_home: bool,
    pub threat_in_sight: bool,
}

/// How fast each stat moves, per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurvivalRates {
    pub hunger_per_sec: f32,
    pub thirst_per_sec: f32,
    /// Rest lost per second awake.
    pub rest_per_sec: f32,
    /// Rest gained per second asleep.
    pub sleep_per_sec: f32,
    /// Safety lost per second with a threat in sight.
    pub fear_per_sec: f32,
    /// Safety gained per second; three times this at home.
    pub calm_per_sec: f32,
}

/// Safety lost when hit, per point of health lost.
pub const FEAR_PER_HIT_POINT: f32 = 1.0;

impl SurvivalRates {
    /// Rates where a full stat lasts `hunger_secs` / `thirst_secs`,
    /// rest lasts `day_secs` awake and refills in a third of it
    /// asleep, and safety refills over a tenth of a day.
    pub fn lasting(hunger_secs: f32, thirst_secs: f32, day_secs: f32) -> Self {
        Self {
            hunger_per_sec: FULL / hunger_secs,
            thirst_per_sec: FULL / thirst_secs,
            rest_per_sec: FULL / day_secs,
            sleep_per_sec: 3.0 * FULL / day_secs,
            fear_per_sec: 10.0 * FULL / day_secs,
            calm_per_sec: 10.0 * FULL / day_secs,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SurvivalError {
    /// The item has no food stats; it cannot be eaten.
    NotFood(String),
    /// Nothing in that inventory slot.
    EmptySlot(usize),
    /// The stack names an item the registry does not know.
    Unregistered(String),
}

impl SurvivalStats {
    /// Move every need for `dt` seconds under `condition`: hunger and
    /// thirst drain; rest drains awake and refills asleep; safety
    /// drains with a threat in sight and refills otherwise, faster at
    /// home. Never below zero, never above FULL.
    pub fn tick(&mut self, rates: SurvivalRates, condition: Condition, dt: f32) {
        self.hunger = (self.hunger - rates.hunger_per_sec * dt).max(0.0);
        self.thirst = (self.thirst - rates.thirst_per_sec * dt).max(0.0);
        self.rest = if condition.asleep {
            (self.rest + rates.sleep_per_sec * dt).min(FULL)
        } else {
            (self.rest - rates.rest_per_sec * dt).max(0.0)
        };
        self.safety = if condition.threat_in_sight {
            (self.safety - rates.fear_per_sec * dt).max(0.0)
        } else {
            let calm = if condition.at_home { 3.0 } else { 1.0 };
            (self.safety + rates.calm_per_sec * calm * dt).min(FULL)
        };
    }

    /// Drain hunger and thirst only (the old name; a tick with no
    /// condition).
    pub fn drain(&mut self, rates: SurvivalRates, dt: f32) {
        self.tick(rates, Condition::default(), dt);
    }

    /// A hit lands: safety falls by the health lost.
    pub fn hit_taken(&mut self, health_lost: f32) {
        self.safety = (self.safety - health_lost * FEAR_PER_HIT_POINT).max(0.0);
    }

    /// The lowest need and its value; the brain's first question.
    pub fn worst_need(&self) -> (Need, f32) {
        let mut worst = (Need::Hunger, self.hunger);
        for (need, value) in [
            (Need::Thirst, self.thirst),
            (Need::Rest, self.rest),
            (Need::Safety, self.safety),
        ] {
            if value < worst.1 {
                worst = (need, value);
            }
        }
        worst
    }

    /// Eat one of `def`: restore what its food stats say, capped at
    /// FULL. The caller removes one from the stack on Ok; the
    /// inventory HUD and the hotbar both enter here.
    pub fn eat(&mut self, def: &ItemDef) -> Result<(), SurvivalError> {
        let Some(food) = def.food else {
            return Err(SurvivalError::NotFood(def.name.clone()));
        };
        self.hunger = (self.hunger + food.hunger).min(FULL);
        self.thirst = (self.thirst + food.thirst).min(FULL);
        Ok(())
    }

    /// Eat one item out of `slot` of `inventory`. The one path the
    /// inventory HUD and the hotbar both take: look the stack's def
    /// up in `registry`, refuse non-food, restore, remove one.
    pub fn eat_from_slot(
        &mut self,
        inventory: &mut Inventory,
        slot: usize,
        registry: &ItemRegistry,
    ) -> Result<(), SurvivalError> {
        let Some(stack) = inventory.slots.get(slot).and_then(|s| s.as_ref()) else {
            return Err(SurvivalError::EmptySlot(slot));
        };
        let Some(def) = registry.def(&stack.item) else {
            return Err(SurvivalError::Unregistered(stack.item.clone()));
        };
        self.eat(def)?;
        inventory.remove(slot, 1);
        Ok(())
    }

    pub fn starving(&self) -> bool {
        self.hunger <= 0.0
    }

    pub fn dehydrated(&self) -> bool {
        self.thirst <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{FoodStats, ItemKind};

    fn def(name: &str, food: Option<FoodStats>) -> ItemDef {
        ItemDef {
            name: name.to_string(),
            kind: if food.is_some() {
                ItemKind::Food
            } else {
                ItemKind::Material
            },
            max_stack: 10,
            quality_siblings: 1,
            combat: None,
            food,
            storage: None,
            armor: None,
            good_for: Default::default(),
            model: None,
        }
    }

    #[test]
    fn a_full_stat_lasts_exactly_the_rate_span() {
        let mut stats = SurvivalStats::default();
        let rates = SurvivalRates::lasting(1200.0, 600.0, 1200.0);
        stats.drain(rates, 600.0);
        assert!((stats.hunger - 50.0).abs() < 1e-3);
        assert!(stats.dehydrated());
        stats.drain(rates, 600.0);
        assert!(stats.starving());
        assert_eq!(stats.hunger, 0.0, "never below zero");
    }

    #[test]
    fn rest_empties_over_a_day_awake_and_refills_in_a_third_asleep() {
        let mut stats = SurvivalStats::default();
        let rates = SurvivalRates::lasting(1200.0, 600.0, 1200.0);
        let awake = Condition::default();
        stats.tick(rates, awake, 1200.0);
        assert_eq!(stats.rest, 0.0);
        let asleep = Condition {
            asleep: true,
            ..Default::default()
        };
        stats.tick(rates, asleep, 400.0);
        assert!((stats.rest - FULL).abs() < 1e-3, "{}", stats.rest);
    }

    #[test]
    fn safety_falls_on_hits_and_in_sight_of_a_threat_and_refills_fastest_at_home() {
        let mut stats = SurvivalStats::default();
        let rates = SurvivalRates::lasting(1200.0, 600.0, 1200.0);
        stats.hit_taken(30.0);
        assert_eq!(stats.safety, 70.0);
        let threatened = Condition {
            threat_in_sight: true,
            ..Default::default()
        };
        stats.tick(rates, threatened, 12.0);
        assert!((stats.safety - 60.0).abs() < 1e-3, "{}", stats.safety);
        assert_eq!(stats.worst_need(), (Need::Safety, stats.safety));
        let mut away = stats;
        away.tick(rates, Condition::default(), 12.0);
        let mut home = stats;
        home.tick(
            rates,
            Condition {
                at_home: true,
                ..Default::default()
            },
            12.0,
        );
        assert!((away.safety - 70.0).abs() < 1e-3);
        assert!((home.safety - 90.0).abs() < 1e-3, "three times as fast at home");
        home.tick(
            rates,
            Condition {
                at_home: true,
                ..Default::default()
            },
            1000.0,
        );
        assert_eq!(home.safety, FULL, "never above full");
    }

    #[test]
    fn eating_restores_and_caps_at_full() {
        let mut stats = SurvivalStats {
            hunger: 20.0,
            thirst: 90.0,
            ..Default::default()
        };
        let can = def(
            "canned food",
            Some(FoodStats {
                hunger: 50.0,
                thirst: 20.0,
            }),
        );
        stats.eat(&can).unwrap();
        assert_eq!(stats.hunger, 70.0);
        assert_eq!(stats.thirst, FULL);
    }

    #[test]
    fn eating_from_a_slot_consumes_one_and_refuses_the_rest() {
        let mut registry = ItemRegistry::default();
        let can = def(
            "canned food",
            Some(FoodStats {
                hunger: 50.0,
                thirst: 0.0,
            }),
        );
        registry.register(can.clone()).unwrap();
        registry.register(def("scrap", None)).unwrap();
        let mut inv = Inventory::new(3);
        inv.slots[0] = Some(crate::item::create(&can, 2, &[], 0.0, 0));
        inv.slots[1] = Some(crate::item::create(&def("scrap", None), 1, &[], 0.0, 0));
        inv.slots[2] = Some(crate::item::ItemStack {
            item: "bogus".to_string(),
            count: 1,
            quality: None,
            note: None,
        });
        let mut stats = SurvivalStats {
            hunger: 10.0,
            thirst: FULL,
            ..Default::default()
        };

        stats.eat_from_slot(&mut inv, 0, &registry).unwrap();
        assert_eq!(stats.hunger, 60.0);
        assert_eq!(inv.count_of("canned food"), 1);
        assert_eq!(
            stats.eat_from_slot(&mut inv, 1, &registry),
            Err(SurvivalError::NotFood("scrap".to_string()))
        );
        assert_eq!(
            stats.eat_from_slot(&mut inv, 2, &registry),
            Err(SurvivalError::Unregistered("bogus".to_string()))
        );
        assert_eq!(
            stats.eat_from_slot(&mut inv, 5, &registry),
            Err(SurvivalError::EmptySlot(5))
        );
        assert_eq!(inv.count_of("scrap"), 1, "refused eats consume nothing");
    }

    #[test]
    fn only_food_can_be_eaten() {
        let mut stats = SurvivalStats::default();
        let scrap = def("scrap", None);
        assert_eq!(
            stats.eat(&scrap),
            Err(SurvivalError::NotFood("scrap".to_string()))
        );
        assert_eq!(stats, SurvivalStats::default());
    }
}
