//! The ONE player-facing information layer, made of panels.
//! Engine-agnostic data and decisions: what to show, when, which
//! panel is open, moving stacks between slots. The consumer
//! writes HudState fields directly and reads them to paint.

use crate::item::{EquipSlot, Equipment, Inventory, ItemKind, ItemRegistry, ItemStack, move_between};
use crate::survival::{SurvivalError, SurvivalStats};

/// What kind of thing can be interacted with.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum InteractKind {
    Door { open: bool },
    /// A locked door into a gated room (Rust's card rooms); opens
    /// only at `level` of progression, which nothing grants yet.
    Gate { level: u32 },
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
        InteractKind::Gate { level } => format!("locked (level {level})"),
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
    /// A gate at `level` stays shut.
    Locked(u32),
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
        InteractKind::Gate { level } => InteractResult::Locked(level),
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
    /// The slot a drag started on, until it is dropped (drag and
    /// drop, as in Atlas).
    pub dragging: Option<SlotRef>,
}

impl HudState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Which slot-indexed holder a slot belongs to: the actor's own
/// inventory or hotbar, or the container open in front of them.
/// Equipment is not slot-indexed and is not draggable yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Holder {
    Inventory,
    Hotbar,
    Container,
}

/// One slot of one holder.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotRef {
    pub holder: Holder,
    pub index: usize,
}

/// The slot-indexed holders on screen, borrowed together for a
/// move. `container` is the open crate, if any.
pub struct Holders<'a> {
    pub inventory: &'a mut Inventory,
    pub hotbar: &'a mut Inventory,
    pub container: Option<&'a mut Inventory>,
}

impl Holders<'_> {
    fn get(&self, holder: Holder) -> Option<&Inventory> {
        match holder {
            Holder::Inventory => Some(self.inventory),
            Holder::Hotbar => Some(self.hotbar),
            Holder::Container => self.container.as_deref(),
        }
    }

    fn get_mut(&mut self, holder: Holder) -> Option<&mut Inventory> {
        match holder {
            Holder::Inventory => Some(self.inventory),
            Holder::Hotbar => Some(self.hotbar),
            Holder::Container => self.container.as_deref_mut(),
        }
    }

    /// Two different holders borrowed at once.
    fn pair_mut(&mut self, a: Holder, b: Holder) -> Option<(&mut Inventory, &mut Inventory)> {
        let Holders {
            inventory,
            hotbar,
            container,
        } = self;
        let container = container.as_deref_mut();
        match (a, b) {
            (Holder::Inventory, Holder::Hotbar) => Some((inventory, hotbar)),
            (Holder::Hotbar, Holder::Inventory) => Some((hotbar, inventory)),
            (Holder::Inventory, Holder::Container) => container.map(|c| (&mut **inventory, c)),
            (Holder::Container, Holder::Inventory) => container.map(|c| (c, &mut **inventory)),
            (Holder::Hotbar, Holder::Container) => container.map(|c| (&mut **hotbar, c)),
            (Holder::Container, Holder::Hotbar) => container.map(|c| (c, &mut **hotbar)),
            _ => None,
        }
    }

    fn max_stack_of(&self, slot: SlotRef, registry: &ItemRegistry) -> u32 {
        self.get(slot.holder)
            .and_then(|inv| inv.slots.get(slot.index))
            .and_then(|s| s.as_ref())
            .and_then(|s| registry.def(&s.item))
            .map(|d| d.max_stack)
            .unwrap_or(1)
    }
}

/// A drag starts on `slot`. Empty slots start nothing.
pub fn start_drag(state: &mut HudState, holders: &Holders<'_>, slot: SlotRef) {
    if holders
        .get(slot.holder)
        .and_then(|inv| inv.slots.get(slot.index))
        .is_some_and(Option::is_some)
    {
        state.dragging = Some(slot);
    }
}

/// A drag ends on `slot`: the dragged stack moves there, within one
/// holder ([`move_stack`]) or across two ([`move_between`]), merging
/// or swapping. Dropping back on its own slot, or with no drag in
/// progress, changes nothing.
pub fn end_drag(
    state: &mut HudState,
    holders: &mut Holders<'_>,
    slot: SlotRef,
    registry: &ItemRegistry,
) {
    let Some(from) = state.dragging.take() else {
        return;
    };
    if from == slot {
        return;
    }
    let max_stack = holders.max_stack_of(from, registry);
    if from.holder == slot.holder {
        if let Some(inv) = holders.get_mut(from.holder) {
            move_stack(inv, from.index, slot.index, max_stack);
        }
    } else if let Some((src, dst)) = holders.pair_mut(from.holder, slot.holder) {
        move_between(src, from.index, dst, slot.index, max_stack);
    }
}

/// Hover plus T (Atlas): the stack under the cursor goes to the
/// other side, container to inventory or inventory/hotbar to
/// container, whatever fits; `half` is Shift+T. Without an open
/// container nothing moves.
pub fn transfer_slot(holders: &mut Holders<'_>, slot: SlotRef, half: bool, registry: &ItemRegistry) {
    let other = match slot.holder {
        Holder::Container => Holder::Inventory,
        Holder::Inventory | Holder::Hotbar => Holder::Container,
    };
    let max_stack = holders.max_stack_of(slot, registry);
    let Some((src, dst)) = holders.pair_mut(slot.holder, other) else {
        return;
    };
    let count = match (half, src.slots.get(slot.index).and_then(|s| s.as_ref())) {
        (_, None) => return,
        (true, Some(stack)) => stack.count.div_ceil(2),
        (false, Some(stack)) => stack.count,
    };
    let Some(taken) = src.remove(slot.index, count) else {
        return;
    };
    if let Some(rest) = dst.add(taken, max_stack) {
        // Put back what did not fit; the slot may have been emptied.
        if let Some(left) = src.add(rest.clone(), max_stack) {
            src.slots[slot.index] = Some(left);
        }
    }
}

/// Slots in the hotbar: its own small [`Inventory`] on the actor,
/// separate from the main inventory and the equipment, always on
/// screen (Atlas: items live in the bar, it is not a list of
/// shortcuts).
pub const HOTBAR_SLOTS: usize = 10;

/// The inventory slot a hotbar digit key uses: 1 to 9 are slots 0
/// to 8, 0 is slot 9 (Atlas key order). Other digits are nothing.
pub fn hotbar_slot(digit: u8) -> Option<usize> {
    match digit {
        1..=9 => Some(digit as usize - 1),
        0 => Some(9),
        _ => None,
    }
}

/// Hover plus O: take the whole stack out of `slot` to drop it in
/// the world. The consumer spawns the returned stack at the feet.
pub fn drop_slot(inv: &mut Inventory, slot: usize) -> Option<ItemStack> {
    inv.remove(slot, u32::MAX)
}

/// Double click or the hotbar key: use what is in `slot`. Food is
/// eaten through [`SurvivalStats::eat_from_slot`]; a weapon or tool
/// goes into the hands (the equipment's weapon slot), worn gear goes
/// into its own slot, and whatever was there takes its place in the
/// slot (Atlas: the number key equips). Other kinds have no use yet.
pub fn use_slot(
    stats: &mut SurvivalStats,
    inv: &mut Inventory,
    slot: usize,
    registry: &ItemRegistry,
    equipment: &mut Equipment,
) -> Result<(), SurvivalError> {
    let Some(stack) = inv.slots.get(slot).and_then(|s| s.as_ref()) else {
        return Err(SurvivalError::EmptySlot(slot));
    };
    let Some(def) = registry.def(&stack.item) else {
        return Err(SurvivalError::Unregistered(stack.item.clone()));
    };
    if let Some(armor) = def.armor {
        let held = inv.slots[slot].take();
        inv.slots[slot] = equipment.set(armor.slot, held);
        return Ok(());
    }
    match def.kind {
        ItemKind::Weapon | ItemKind::Tool => {
            let held = inv.slots[slot].take();
            inv.slots[slot] = equipment.set(EquipSlot::Weapon, held);
            Ok(())
        }
        _ => stats.eat_from_slot(inv, slot, registry),
    }
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
                storage: None,
                armor: None,
            })
            .unwrap();
        }
        reg
    }

    fn at(holder: Holder, index: usize) -> SlotRef {
        SlotRef { holder, index }
    }

    #[test]
    fn drag_and_drop_moves_stacks_within_and_across_holders() {
        let reg = registry();
        let mut state = HudState::new();
        let mut inv = Inventory::new(3);
        let mut bar = Inventory::new(2);
        inv.slots[0] = Some(stack("scrap", 5));
        inv.slots[1] = Some(stack("cloth", 2));
        bar.slots[1] = Some(stack("scrap", 18));
        let mut holders = Holders {
            inventory: &mut inv,
            hotbar: &mut bar,
            container: None,
        };
        let inv_ = |i| at(Holder::Inventory, i);
        let bar_ = |i| at(Holder::Hotbar, i);

        // No container open: a drag onto a container slot is lost
        // nowhere, the stack stays put.
        start_drag(&mut state, &holders, inv_(0));
        end_drag(&mut state, &mut holders, at(Holder::Container, 0), &reg);
        assert_eq!(holders.inventory.slots[0].as_ref().unwrap().item, "scrap");

        start_drag(&mut state, &holders, inv_(2));
        assert_eq!(state.dragging, None, "empty slot starts no drag");
        end_drag(&mut state, &mut holders, inv_(0), &reg);
        assert_eq!(holders.inventory.slots[0].as_ref().unwrap().item, "scrap", "no drag, no move");

        start_drag(&mut state, &holders, inv_(0));
        assert_eq!(state.dragging, Some(inv_(0)));
        end_drag(&mut state, &mut holders, inv_(0), &reg);
        assert_eq!(state.dragging, None, "dropping on itself ends the drag");

        start_drag(&mut state, &holders, inv_(0));
        end_drag(&mut state, &mut holders, inv_(1), &reg);
        assert_eq!(holders.inventory.slots[0].as_ref().unwrap().item, "cloth", "swapped");
        assert_eq!(holders.inventory.slots[1].as_ref().unwrap().item, "scrap");

        // Inventory to hotbar: the scrap merges up to the max stack
        // of 20, the rest stays behind.
        start_drag(&mut state, &holders, inv_(1));
        end_drag(&mut state, &mut holders, bar_(1), &reg);
        assert_eq!(holders.hotbar.slots[1].as_ref().unwrap().count, 20);
        assert_eq!(holders.inventory.slots[1].as_ref().unwrap().count, 3);

        // Hotbar to an empty inventory slot: moves whole.
        start_drag(&mut state, &holders, bar_(1));
        end_drag(&mut state, &mut holders, inv_(2), &reg);
        assert!(holders.hotbar.slots[1].is_none());
        assert_eq!(holders.inventory.slots[2].as_ref().unwrap().count, 20);
    }

    #[test]
    fn t_transfers_to_the_other_side_and_shift_t_half() {
        let reg = registry();
        let mut inv = Inventory::new(2);
        let mut bar = Inventory::new(1);
        let mut crate_ = Inventory::new(2);
        inv.slots[0] = Some(stack("scrap", 6));
        crate_.slots[0] = Some(stack("cloth", 4));
        crate_.slots[1] = Some(stack("scrap", 19));
        let mut holders = Holders {
            inventory: &mut inv,
            hotbar: &mut bar,
            container: Some(&mut crate_),
        };

        // Container to inventory, whole stack.
        transfer_slot(&mut holders, at(Holder::Container, 0), false, &reg);
        assert!(holders.container.as_ref().unwrap().slots[0].is_none());
        assert_eq!(holders.inventory.count_of("cloth"), 4);

        // Inventory to container, half of 6 is 3: it tops up the 19
        // to 20, the other 2 take the slot the cloth left.
        transfer_slot(&mut holders, at(Holder::Inventory, 0), true, &reg);
        assert_eq!(holders.container.as_ref().unwrap().slots[1].as_ref().unwrap().count, 20);
        assert_eq!(holders.container.as_ref().unwrap().slots[0].as_ref().unwrap().count, 2);
        assert_eq!(holders.inventory.count_of("scrap"), 3);

        // Whole stack: the last 3 join the 2.
        transfer_slot(&mut holders, at(Holder::Inventory, 0), false, &reg);
        assert_eq!(holders.container.as_ref().unwrap().slots[0].as_ref().unwrap().count, 5);
        assert_eq!(holders.inventory.count_of("scrap"), 0);

        // Container to a full inventory: the stack comes back.
        holders.inventory.slots[0] = Some(stack("cloth", 20));
        holders.inventory.slots[1] = Some(stack("cloth", 20));
        transfer_slot(&mut holders, at(Holder::Container, 0), false, &reg);
        assert_eq!(holders.container.as_ref().unwrap().slots[0].as_ref().unwrap().count, 5);

        // Empty slot: nothing happens.
        transfer_slot(&mut holders, at(Holder::Hotbar, 0), false, &reg);
        assert_eq!(holders.container.as_ref().unwrap().count_of("scrap"), 25);

        // No container open: nothing moves.
        let mut closed = Holders {
            inventory: &mut inv,
            hotbar: &mut bar,
            container: None,
        };
        transfer_slot(&mut closed, at(Holder::Inventory, 0), false, &reg);
        assert_eq!(closed.inventory.count_of("cloth"), 40);
    }

    #[test]
    fn using_a_weapon_slot_puts_it_in_the_hands_and_swaps_the_held_one() {
        let mut reg = registry();
        reg.register(crate::item::ItemDef {
            name: "pipe".to_string(),
            kind: crate::item::ItemKind::Weapon,
            max_stack: 1,
            quality_siblings: 1,
            combat: None,
            food: None,
            storage: None,
            armor: None,
        })
        .unwrap();
        reg.register(crate::item::ItemDef {
            name: "hatchet".to_string(),
            kind: crate::item::ItemKind::Tool,
            max_stack: 1,
            quality_siblings: 1,
            combat: None,
            food: None,
            storage: None,
            armor: None,
        })
        .unwrap();
        let mut stats = crate::survival::SurvivalStats::default();
        let mut gear = Equipment::default();
        let mut bar = Inventory::new(2);
        bar.slots[0] = Some(stack("pipe", 1));
        bar.slots[1] = Some(stack("hatchet", 1));

        use_slot(&mut stats, &mut bar, 0, &reg, &mut gear).unwrap();
        assert_eq!(gear.get(EquipSlot::Weapon).unwrap().item, "pipe");
        assert!(bar.slots[0].is_none(), "the pipe left the bar");

        use_slot(&mut stats, &mut bar, 1, &reg, &mut gear).unwrap();
        assert_eq!(gear.get(EquipSlot::Weapon).unwrap().item, "hatchet");
        assert_eq!(bar.slots[1].as_ref().unwrap().item, "pipe", "the pipe came back");

        assert_eq!(
            use_slot(&mut stats, &mut bar, 0, &reg, &mut gear),
            Err(SurvivalError::EmptySlot(0))
        );
    }

    #[test]
    fn drop_slot_takes_the_whole_stack() {
        let mut inv = Inventory::new(2);
        inv.slots[0] = Some(stack("scrap", 5));
        let dropped = drop_slot(&mut inv, 0).unwrap();
        assert_eq!(dropped.count, 5);
        assert!(inv.slots[0].is_none());
        assert!(drop_slot(&mut inv, 1).is_none(), "empty slot drops nothing");
    }

    #[test]
    fn hotbar_keys_map_in_atlas_order() {
        assert_eq!(hotbar_slot(1), Some(0));
        assert_eq!(hotbar_slot(9), Some(8));
        assert_eq!(hotbar_slot(0), Some(9));
        assert_eq!(hotbar_slot(10), None);
        assert!(HOTBAR_SLOTS == 10);
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
