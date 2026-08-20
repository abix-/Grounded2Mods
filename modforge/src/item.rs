//! The item vocabulary: items as DATA. An ItemDef is one item kind
//! (name, kind, max stack, quality siblings); the ItemRegistry holds
//! the checked-in defs; an Inventory is slots holding stacks. Slots
//! and stacks only, no weight (topside design.md, operator-locked).
//!
//! [`create`] is the ONE function that brings an item stack into
//! existence, and it rolls quality right there (the ownership rule:
//! quality is rolled at the moment an item enters existence, in one
//! place). Consumers own where created stacks go: a world pickup, a
//! crate, the edge's courier.

use crate::quality;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ItemKind {
    Food,
    Material,
    Tool,
    Weapon,
}

/// Combat stats on an item. Present on weapons and tools that can
/// deal damage; None on food, materials, and non-combat items.
#[derive(Clone, Debug)]
pub struct CombatStats {
    pub damage: f32,
    pub attack_speed: f32,
    pub range: f32,
    pub ammo: Option<String>,
}

/// What eating one of this item restores. Present on food and
/// drink; None on everything else.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodStats {
    pub hunger: f32,
    pub thirst: f32,
}

/// One item kind as data. `name` is the id; one concept, one name.
/// `quality_siblings` is how many statistical siblings each quality
/// tier has (see [`crate::quality`]).
#[derive(Clone)]
pub struct ItemDef {
    pub name: String,
    pub kind: ItemKind,
    pub max_stack: u32,
    pub quality_siblings: u64,
    pub combat: Option<CombatStats>,
    pub food: Option<FoodStats>,
}

/// The collection of checked-in ItemDefs. Consumers register their
/// content at startup and look defs up by name.
#[derive(Default)]
pub struct ItemRegistry {
    defs: Vec<ItemDef>,
}

impl ItemRegistry {
    pub fn register(&mut self, def: ItemDef) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("item '{}' registered twice", def.name));
        }
        self.defs.push(def);
        Ok(())
    }

    pub fn def(&self, name: &str) -> Option<&ItemDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// The quality rolled onto one stack at creation: which tier (index
/// into the game's tier table, best first) and which statistical
/// sibling within it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ItemQuality {
    pub tier: usize,
    pub sibling: u64,
}

/// A stack of one item. Stacks merge only when item name AND quality
/// match; a Rare rifle never stacks on a Normal one.
#[derive(Clone, PartialEq, Debug)]
pub struct ItemStack {
    pub item: String,
    pub count: u32,
    pub quality: Option<ItemQuality>,
}

/// The one item-creation function. Every stack that enters existence
/// (edge arrival, crafting, harvest, loot spot, test setup) comes
/// from here, with quality rolled from `odds` (cumulative per-mille,
/// best tier first; empty odds means always base quality).
pub fn create(def: &ItemDef, count: u32, odds: &[u64], now: f32, salt: u64) -> ItemStack {
    let quality = quality::roll_tier(odds, now, salt).map(|tier| ItemQuality {
        tier,
        sibling: quality::roll_sibling(def.quality_siblings, now, salt),
    });
    ItemStack {
        item: def.name.clone(),
        count,
        quality,
    }
}

/// Slots holding stacks. The ONE inventory type: the player, NPCs,
/// crates, and stores all carry this.
#[derive(Clone)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
}

impl Inventory {
    pub fn new(slot_count: usize) -> Self {
        Self {
            slots: vec![None; slot_count],
        }
    }

    /// Add a stack: top up matching stacks first, then fill empty
    /// slots, splitting at `max_stack`. Returns what did not fit.
    pub fn add(&mut self, mut stack: ItemStack, max_stack: u32) -> Option<ItemStack> {
        for slot in self.slots.iter_mut().flatten() {
            if slot.item == stack.item && slot.quality == stack.quality && slot.count < max_stack {
                let moved = stack.count.min(max_stack - slot.count);
                slot.count += moved;
                stack.count -= moved;
                if stack.count == 0 {
                    return None;
                }
            }
        }
        for slot in self.slots.iter_mut().filter(|s| s.is_none()) {
            let moved = stack.count.min(max_stack);
            *slot = Some(ItemStack {
                count: moved,
                ..stack.clone()
            });
            stack.count -= moved;
            if stack.count == 0 {
                return None;
            }
        }
        Some(stack)
    }

    /// Take up to `count` out of one slot. Returns the taken stack;
    /// the slot keeps any rest.
    pub fn remove(&mut self, slot: usize, count: u32) -> Option<ItemStack> {
        let stack = self.slots.get_mut(slot)?.as_mut()?;
        let taken = ItemStack {
            count: stack.count.min(count),
            ..stack.clone()
        };
        stack.count -= taken.count;
        if stack.count == 0 {
            self.slots[slot] = None;
        }
        Some(taken)
    }

    /// Total count of one item across all slots, any quality.
    pub fn count_of(&self, item: &str) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum()
    }
}

/// Move one slot's stack from one inventory into another; what does
/// not fit stays in the source slot.
pub fn transfer(from: &mut Inventory, to: &mut Inventory, slot: usize, max_stack: u32) {
    let Some(stack) = from.remove(slot, u32::MAX) else {
        return;
    };
    if let Some(rest) = to.add(stack, max_stack) {
        from.slots[slot] = Some(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> ItemDef {
        ItemDef {
            name: name.to_string(),
            kind: ItemKind::Material,
            max_stack: 10,
            quality_siblings: 3,
            combat: None,
            food: None,
        }
    }

    fn plain(name: &str, count: u32) -> ItemStack {
        ItemStack {
            item: name.to_string(),
            count,
            quality: None,
        }
    }

    #[test]
    fn registry_serves_defs_and_rejects_duplicates() {
        let mut reg = ItemRegistry::default();
        reg.register(def("scrap")).unwrap();
        assert!(reg.def("scrap").is_some());
        assert!(reg.def("gold").is_none());
        assert!(reg.register(def("scrap")).is_err());
    }

    #[test]
    fn adding_tops_up_matching_stacks_before_empty_slots() {
        let mut inv = Inventory::new(3);
        assert!(inv.add(plain("scrap", 7), 10).is_none());
        assert!(inv.add(plain("scrap", 7), 10).is_none());
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 10);
        assert_eq!(inv.slots[1].as_ref().unwrap().count, 4);
        assert_eq!(inv.count_of("scrap"), 14);
    }

    #[test]
    fn different_quality_never_stacks() {
        let mut inv = Inventory::new(2);
        let mut rare = plain("rifle", 1);
        rare.quality = Some(ItemQuality { tier: 0, sibling: 1 });
        inv.add(plain("rifle", 1), 10);
        inv.add(rare, 10);
        assert!(inv.slots[0].is_some() && inv.slots[1].is_some());
    }

    #[test]
    fn full_inventory_returns_the_rest() {
        let mut inv = Inventory::new(1);
        let rest = inv.add(plain("scrap", 15), 10).unwrap();
        assert_eq!(rest.count, 5);
        assert_eq!(inv.count_of("scrap"), 10);
    }

    #[test]
    fn remove_splits_and_empties_slots() {
        let mut inv = Inventory::new(1);
        inv.add(plain("scrap", 10), 10);
        assert_eq!(inv.remove(0, 4).unwrap().count, 4);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 6);
        assert_eq!(inv.remove(0, 99).unwrap().count, 6);
        assert!(inv.slots[0].is_none());
        assert!(inv.remove(0, 1).is_none());
    }

    #[test]
    fn transfer_moves_a_stack_and_keeps_what_does_not_fit() {
        let mut crate_inv = Inventory::new(1);
        let mut player = Inventory::new(1);
        crate_inv.add(plain("scrap", 10), 10);
        player.add(plain("scrap", 7), 10);
        transfer(&mut crate_inv, &mut player, 0, 10);
        assert_eq!(player.count_of("scrap"), 10);
        assert_eq!(crate_inv.count_of("scrap"), 7);
    }

    #[test]
    fn creation_rolls_quality_once() {
        let d = def("rifle");
        // 1000 per-mille on the first tier: always that tier.
        let sure = create(&d, 1, &[1000], 1.0, 42);
        let q = sure.quality.unwrap();
        assert_eq!(q.tier, 0);
        assert!((1..=3).contains(&q.sibling));
        // Zero odds everywhere: always base quality.
        assert_eq!(create(&d, 1, &[0, 0], 1.0, 42).quality, None);
        assert_eq!(create(&d, 1, &[], 1.0, 42).quality, None);
    }
}
