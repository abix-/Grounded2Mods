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

/// Hunger and thirst, each 0 (empty) to FULL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurvivalStats {
    pub hunger: f32,
    pub thirst: f32,
}

impl Default for SurvivalStats {
    fn default() -> Self {
        Self {
            hunger: FULL,
            thirst: FULL,
        }
    }
}

/// How fast each stat drains, per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurvivalRates {
    pub hunger_per_sec: f32,
    pub thirst_per_sec: f32,
}

impl SurvivalRates {
    /// Rates where a full stat lasts `hunger_secs` / `thirst_secs`.
    pub fn lasting(hunger_secs: f32, thirst_secs: f32) -> Self {
        Self {
            hunger_per_sec: FULL / hunger_secs,
            thirst_per_sec: FULL / thirst_secs,
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
    /// Drain both stats for `dt` seconds, never below zero.
    pub fn drain(&mut self, rates: SurvivalRates, dt: f32) {
        self.hunger = (self.hunger - rates.hunger_per_sec * dt).max(0.0);
        self.thirst = (self.thirst - rates.thirst_per_sec * dt).max(0.0);
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
        }
    }

    #[test]
    fn a_full_stat_lasts_exactly_the_rate_span() {
        let mut stats = SurvivalStats::default();
        let rates = SurvivalRates::lasting(1200.0, 600.0);
        stats.drain(rates, 600.0);
        assert!((stats.hunger - 50.0).abs() < 1e-3);
        assert!(stats.dehydrated());
        stats.drain(rates, 600.0);
        assert!(stats.starving());
        assert_eq!(stats.hunger, 0.0, "never below zero");
    }

    #[test]
    fn eating_restores_and_caps_at_full() {
        let mut stats = SurvivalStats {
            hunger: 20.0,
            thirst: 90.0,
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
        });
        let mut stats = SurvivalStats {
            hunger: 10.0,
            thirst: FULL,
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
