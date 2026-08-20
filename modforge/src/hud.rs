//! The ONE player-facing information layer, made of panels.
//! Engine-agnostic data and decisions: what to show, when, which
//! panel is open, moving stacks between slots. The consumer
//! writes HudState fields directly and reads them to paint.

use crate::item::{Inventory, ItemRegistry, ItemStack};
use crate::survival::{SurvivalError, SurvivalStats};

/// What kind of thing can be interacted with.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum InteractKind {
    Door { open: bool },
    Pickup,
    Container,
}

/// What the player is looking at right now.
#[derive(Clone, Default)]
pub struct Prompt {
    pub text: String,
    pub can_interact: bool,
}

/// Build the prompt text for an interactable. The consumer feeds the
/// kind; modforge decides what the player reads.
pub fn prompt_for(kind: InteractKind, item_name: Option<&str>, item_count: Option<u32>) -> Prompt {
    let text = match kind {
        InteractKind::Door { open } => {
            if open { "[E] close door" } else { "[E] open door" }.to_string()
        }
        InteractKind::Pickup => match (item_name, item_count) {
            (Some(name), Some(count)) if count > 1 => format!("[E] pick up {name} x{count}"),
            (Some(name), _) => format!("[E] pick up {name}"),
            _ => "[E] pick up".to_string(),
        },
        InteractKind::Container => "[E] open".to_string(),
    };
    Prompt {
        text,
        can_interact: true,
    }
}

/// The result of interacting with something. The consumer reads this
/// and executes the engine side (toggle transform, despawn entity,
/// open panel).
#[derive(Debug, PartialEq)]
pub enum InteractResult {
    ToggleDoor,
    PickedUp,
    InventoryFull,
    OpenContainer,
}

/// Execute an interaction. modforge decides what happens; the
/// consumer carries out the engine side based on the result.
pub fn interact(
    kind: InteractKind,
    player_inv: &mut Inventory,
    pickup_stack: Option<ItemStack>,
    max_stack: u32,
    state: &mut HudState,
) -> InteractResult {
    match kind {
        InteractKind::Door { .. } => InteractResult::ToggleDoor,
        InteractKind::Pickup => {
            if let Some(stack) = pickup_stack {
                if player_inv.add(stack, max_stack).is_some() {
                    InteractResult::InventoryFull
                } else {
                    InteractResult::PickedUp
                }
            } else {
                InteractResult::PickedUp
            }
        }
        InteractKind::Container => {
            toggle_panel(state, OpenPanel::Inventory);
            InteractResult::OpenContainer
        }
    }
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

/// The whole HUD state. The consumer writes fields directly from
/// its game systems and reads them to paint. No binder trait.
/// Inventories are not here: they belong to the actor or container
/// that owns them, and the HUD reads those when it paints.
#[derive(Default)]
pub struct HudState {
    pub vitals: Vitals,
    pub prompt: Prompt,
    pub open_panel: OpenPanel,
    /// The inventory slot picked by the first click, waiting for
    /// the second (pick-up-then-place, as in Minecraft and Rust).
    pub selected_slot: Option<usize>,
}

impl HudState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A click on inventory slot `slot` while the panel is open. The
/// first click selects; a second click on another slot moves the
/// stack there (merge or swap, see [`move_stack`]); a second click
/// on the same slot deselects. Clicking an empty slot with nothing
/// selected does nothing.
pub fn click_slot(state: &mut HudState, inv: &mut Inventory, slot: usize, registry: &ItemRegistry) {
    if slot >= inv.slots.len() {
        return;
    }
    match state.selected_slot {
        None => {
            if inv.slots[slot].is_some() {
                state.selected_slot = Some(slot);
            }
        }
        Some(from) if from == slot => state.selected_slot = None,
        Some(from) => {
            let max_stack = inv.slots[from]
                .as_ref()
                .and_then(|s| registry.def(&s.item))
                .map(|d| d.max_stack)
                .unwrap_or(1);
            move_stack(inv, from, slot, max_stack);
            state.selected_slot = None;
        }
    }
}

/// Use what is in `slot`: food is eaten through
/// [`SurvivalStats::eat_from_slot`]. Other kinds have no use yet.
pub fn use_slot(
    stats: &mut SurvivalStats,
    inv: &mut Inventory,
    slot: usize,
    registry: &ItemRegistry,
) -> Result<(), SurvivalError> {
    stats.eat_from_slot(inv, slot, registry)
}

/// The text one inventory slot shows. Empty slots show nothing.
pub fn slot_label(stack: Option<&ItemStack>) -> String {
    match stack {
        None => String::new(),
        Some(s) if s.count > 1 => format!("{} x{}", s.item, s.count),
        Some(s) => s.item.clone(),
    }
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

    #[test]
    fn toggle_panel_opens_and_closes() {
        let mut state = HudState::new();
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
    fn prompt_for_door() {
        let p = prompt_for(InteractKind::Door { open: false }, None, None);
        assert_eq!(p.text, "[E] open door");
        let p = prompt_for(InteractKind::Door { open: true }, None, None);
        assert_eq!(p.text, "[E] close door");
    }

    #[test]
    fn prompt_for_pickup_with_count() {
        let p = prompt_for(InteractKind::Pickup, Some("scrap"), Some(5));
        assert_eq!(p.text, "[E] pick up scrap x5");
        let p = prompt_for(InteractKind::Pickup, Some("hatchet"), Some(1));
        assert_eq!(p.text, "[E] pick up hatchet");
    }

    #[test]
    fn interact_door_returns_toggle() {
        let mut inv = Inventory::new(5);
        let mut state = HudState::new();
        let r = interact(InteractKind::Door { open: false }, &mut inv, None, 10, &mut state);
        assert_eq!(r, InteractResult::ToggleDoor);
    }

    #[test]
    fn interact_pickup_adds_to_inventory() {
        let mut inv = Inventory::new(5);
        let mut state = HudState::new();
        let s = stack("scrap", 3);
        let r = interact(InteractKind::Pickup, &mut inv, Some(s), 20, &mut state);
        assert_eq!(r, InteractResult::PickedUp);
        assert_eq!(inv.count_of("scrap"), 3);
    }

    #[test]
    fn interact_pickup_full_inventory() {
        let mut inv = Inventory::new(1);
        inv.slots[0] = Some(stack("cloth", 20));
        let mut state = HudState::new();
        let s = stack("scrap", 5);
        let r = interact(InteractKind::Pickup, &mut inv, Some(s), 20, &mut state);
        assert_eq!(r, InteractResult::InventoryFull);
    }

    #[test]
    fn interact_container_opens_inventory_panel() {
        let mut inv = Inventory::new(5);
        let mut state = HudState::new();
        let r = interact(InteractKind::Container, &mut inv, None, 10, &mut state);
        assert_eq!(r, InteractResult::OpenContainer);
        assert_eq!(state.open_panel, OpenPanel::Inventory);
    }

    fn registry() -> ItemRegistry {
        let mut reg = ItemRegistry::default();
        for (name, max_stack) in [("scrap", 20), ("cloth", 20)] {
            reg.register(crate::item::ItemDef {
                name: name.to_string(),
                kind: crate::item::ItemKind::Material,
                max_stack,
                quality_siblings: 1,
                combat: None,
                food: None,
            })
            .unwrap();
        }
        reg
    }

    #[test]
    fn click_select_then_click_moves_and_clears_selection() {
        let reg = registry();
        let mut state = HudState::new();
        let mut inv = Inventory::new(3);
        inv.slots[0] = Some(stack("scrap", 5));
        inv.slots[1] = Some(stack("cloth", 2));

        click_slot(&mut state, &mut inv, 2, &reg);
        assert_eq!(state.selected_slot, None, "empty slot selects nothing");
        click_slot(&mut state, &mut inv, 0, &reg);
        assert_eq!(state.selected_slot, Some(0));
        click_slot(&mut state, &mut inv, 0, &reg);
        assert_eq!(state.selected_slot, None, "same slot deselects");

        click_slot(&mut state, &mut inv, 0, &reg);
        click_slot(&mut state, &mut inv, 1, &reg);
        assert_eq!(state.selected_slot, None);
        assert_eq!(inv.slots[0].as_ref().unwrap().item, "cloth", "swapped");
        assert_eq!(inv.slots[1].as_ref().unwrap().item, "scrap");

        click_slot(&mut state, &mut inv, 1, &reg);
        click_slot(&mut state, &mut inv, 2, &reg);
        assert!(inv.slots[1].is_none(), "moved to the empty slot");
        assert_eq!(inv.slots[2].as_ref().unwrap().count, 5);
    }

    #[test]
    fn slot_labels() {
        assert_eq!(slot_label(None), "");
        assert_eq!(slot_label(Some(&stack("hatchet", 1))), "hatchet");
        assert_eq!(slot_label(Some(&stack("scrap", 6))), "scrap x6");
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
