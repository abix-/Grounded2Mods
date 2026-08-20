//! The ONE player-facing information layer, made of panels.
//! Engine-agnostic data and decisions: what to show, when, which
//! panel is open, moving stacks between slots. The consumer
//! implements [`HudBinder`] to read its world and a renderer to
//! paint the result.

use crate::item::Inventory;

/// What the player is looking at right now (fed by the consumer's
/// binder every tick).
#[derive(Clone, Default)]
pub struct Prompt {
    pub text: String,
    pub can_interact: bool,
}

/// Which panel is open. Only one panel open at a time (or none).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum OpenPanel {
    #[default]
    None,
    Inventory,
}

/// The vital bars the HUD always shows.
#[derive(Clone, Default)]
pub struct Vitals {
    pub health: f32,
    pub health_max: f32,
    pub food: f32,
    pub food_max: f32,
}

/// The whole HUD state. The consumer reads this to paint.
pub struct HudState {
    pub vitals: Vitals,
    pub prompt: Prompt,
    pub open_panel: OpenPanel,
    pub inventory: Inventory,
}

impl HudState {
    pub fn new(inventory_slots: usize) -> Self {
        Self {
            vitals: Vitals::default(),
            prompt: Prompt::default(),
            open_panel: OpenPanel::None,
            inventory: Inventory::new(inventory_slots),
        }
    }
}

/// The binder a consumer implements so the HUD can read the world.
/// Called once per tick to refresh the HUD state.
pub trait HudBinder {
    fn vitals(&self) -> Vitals;
    fn prompt(&self) -> Prompt;
    fn inventory(&self) -> &Inventory;
}

/// Refresh the HUD state from the world through the binder.
pub fn tick(state: &mut HudState, binder: &dyn HudBinder) {
    state.vitals = binder.vitals();
    state.prompt = binder.prompt();
    let inv = binder.inventory();
    state.inventory.slots.clear();
    state.inventory.slots.extend_from_slice(&inv.slots);
}

/// Toggle a panel open or closed.
pub fn toggle_panel(state: &mut HudState, panel: OpenPanel) {
    if state.open_panel == panel {
        state.open_panel = OpenPanel::None;
    } else {
        state.open_panel = panel;
    }
}

/// Move a stack from one inventory slot to another. If the target
/// slot has a matching stack, they merge up to max_stack; if it has
/// a different item, the slots swap.
pub fn move_stack(inv: &mut Inventory, from: usize, to: usize, max_stack: u32) {
    if from == to || from >= inv.slots.len() || to >= inv.slots.len() {
        return;
    }
    let (src, dst) = if from < to {
        let (a, b) = inv.slots.split_at_mut(to);
        (&mut a[from], &mut b[0])
    } else {
        let (a, b) = inv.slots.split_at_mut(from);
        (&mut b[0], &mut a[to])
    };
    match (src.as_mut(), dst.as_mut()) {
        (None, _) => {}
        (Some(_), None) => {
            *dst = src.take();
        }
        (Some(s), Some(d)) if s.item == d.item && s.quality == d.quality => {
            let moved = s.count.min(max_stack.saturating_sub(d.count));
            d.count += moved;
            s.count -= moved;
            if s.count == 0 {
                *src = None;
            }
        }
        _ => {
            std::mem::swap(src, dst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemStack;

    fn stack(name: &str, count: u32) -> ItemStack {
        ItemStack {
            item: name.to_string(),
            count,
            quality: None,
        }
    }

    struct FakeBinder {
        vitals: Vitals,
        inv: Inventory,
    }

    impl HudBinder for FakeBinder {
        fn vitals(&self) -> Vitals {
            self.vitals.clone()
        }
        fn prompt(&self) -> Prompt {
            Prompt {
                text: "open door".to_string(),
                can_interact: true,
            }
        }
        fn inventory(&self) -> &Inventory {
            &self.inv
        }
    }

    #[test]
    fn tick_refreshes_from_binder() {
        let mut state = HudState::new(5);
        let mut inv = Inventory::new(5);
        inv.add(stack("scrap", 3), 20);
        let binder = FakeBinder {
            vitals: Vitals {
                health: 80.0,
                health_max: 100.0,
                food: 2.5,
                food_max: 3.0,
            },
            inv,
        };
        tick(&mut state, &binder);
        assert_eq!(state.vitals.health, 80.0);
        assert_eq!(state.prompt.text, "open door");
        assert_eq!(state.inventory.count_of("scrap"), 3);
    }

    #[test]
    fn toggle_panel_opens_and_closes() {
        let mut state = HudState::new(5);
        assert_eq!(state.open_panel, OpenPanel::None);
        toggle_panel(&mut state, OpenPanel::Inventory);
        assert_eq!(state.open_panel, OpenPanel::Inventory);
        toggle_panel(&mut state, OpenPanel::Inventory);
        assert_eq!(state.open_panel, OpenPanel::None);
    }

    #[test]
    fn move_stack_to_empty_slot() {
        let mut inv = Inventory::new(3);
        inv.slots[0] = Some(stack("scrap", 5));
        move_stack(&mut inv, 0, 2, 20);
        assert!(inv.slots[0].is_none());
        assert_eq!(inv.slots[2].as_ref().unwrap().count, 5);
    }

    #[test]
    fn move_stack_merges_matching() {
        let mut inv = Inventory::new(2);
        inv.slots[0] = Some(stack("scrap", 8));
        inv.slots[1] = Some(stack("scrap", 6));
        move_stack(&mut inv, 0, 1, 10);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 4);
        assert_eq!(inv.slots[1].as_ref().unwrap().count, 10);
    }

    #[test]
    fn move_stack_swaps_different_items() {
        let mut inv = Inventory::new(2);
        inv.slots[0] = Some(stack("scrap", 5));
        inv.slots[1] = Some(stack("cloth", 3));
        move_stack(&mut inv, 0, 1, 20);
        assert_eq!(inv.slots[0].as_ref().unwrap().item, "cloth");
        assert_eq!(inv.slots[1].as_ref().unwrap().item, "scrap");
    }
}
