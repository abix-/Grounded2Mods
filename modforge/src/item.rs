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

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use crate::quality;

/// Which stored goods a transfer may select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoodsFilter {
    Any,
    Food,
    NonFood,
}

impl GoodsFilter {
    pub fn matches(self, is_food: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Food => is_food,
            Self::NonFood => !is_food,
        }
    }
}

/// One observed stack available to a goods transfer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GoodsCandidate {
    pub value: f64,
    pub is_food: bool,
}

/// The next stack and carrier selected by a goods transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoodsTransfer {
    pub candidate: usize,
    pub carrier: usize,
}

/// Engine-independent valuable-goods transfer policy.
///
/// Stores may be blocked by the caller. Within an allowed store,
/// the highest-value matching stack is selected, ties keep source
/// order, successful transfers rotate across carriers, and the
/// configured stack cap ends the transfer.
pub struct GoodsTransferPlanner {
    filter: GoodsFilter,
    max_stacks: usize,
    carrier_count: usize,
    transferred: usize,
}

impl GoodsTransferPlanner {
    pub fn new(filter: GoodsFilter, max_stacks: usize, carrier_count: usize) -> Self {
        Self {
            filter,
            max_stacks,
            carrier_count,
            transferred: 0,
        }
    }

    pub fn can_take_from(&self, blocked: bool) -> bool {
        !blocked && !self.complete()
    }

    pub fn next(&self, candidates: &[GoodsCandidate]) -> Option<GoodsTransfer> {
        if self.complete() {
            return None;
        }
        let mut selected = None;
        let mut selected_value = -1.0f64;
        for (index, candidate) in candidates.iter().enumerate() {
            if self.filter.matches(candidate.is_food) && candidate.value > selected_value {
                selected = Some(index);
                selected_value = candidate.value;
            }
        }
        selected.map(|candidate| GoodsTransfer {
            candidate,
            carrier: self.transferred % self.carrier_count,
        })
    }

    pub fn record_success(&mut self) {
        self.transferred += 1;
    }

    pub fn transferred(&self) -> usize {
        self.transferred
    }

    pub fn complete(&self) -> bool {
        self.transferred >= self.max_stacks
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ItemKind {
    Food,
    Material,
    Tool,
    Weapon,
    /// Placed in the world and used there: a storage box, a crafting
    /// bench. A deployable with `storage` slots holds an inventory.
    Deployable,
    /// Words someone left: read where it lies (`write` makes one).
    Note,
}

/// How a weapon fires (topside design.md "How a hit works"). Present
/// on weapons and tools that can deal damage; None on food,
/// materials, and non-combat items. What a hit does is the damage
/// def it names (`crate::combat::DamageDef`); this is the firing.
#[derive(Clone, Debug, PartialEq)]
pub struct CombatStats {
    /// The damage def this weapon lands.
    pub damage: String,
    /// Seconds between shots (Quake 3's per-weapon `addTime`).
    pub delay: f32,
    /// How far a hit can land. Melee: the swing's reach. Ranged: the
    /// Rust hybrid's instant part; past it the shot is a projectile.
    pub reach: f32,
    /// Rays per shot: 1 for most, more for shotguns.
    pub pellets: u32,
    /// Spread half-angle in degrees; 0 for melee.
    pub spread_degrees: f32,
    /// Ammo item consumed per shot; None for melee.
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
    /// Only one of this item may enter a save.
    pub unique: bool,
    pub kind: ItemKind,
    pub max_stack: u32,
    pub quality_siblings: u64,
    pub combat: Option<CombatStats>,
    pub food: Option<FoodStats>,
    /// Inventory slots this item holds once deployed in the world
    /// (a storage box). None for everything that holds nothing.
    pub storage: Option<usize>,
    /// Worn gear: the slot it is worn in and the armor it gives that
    /// slot's body area (design.md "protection per body area from
    /// worn gear"). None for everything that is not worn.
    pub armor: Option<Armor>,
    /// What this is good for once it stands in the world (life.md
    /// "What things are good for"): a storage box is good for hunger
    /// (whoever remembers it may come to eat from it). Empty for
    /// things nobody seeks out.
    pub good_for: crate::memory::GoodFor,
    /// The low poly model file for this item (`pipe.glb`), the same
    /// one on the ground, in the hand, and as the hotbar icon. None
    /// until one exists; the consumer draws a box then.
    pub model: Option<String>,
}

#[derive(Default)]
struct ItemLedgerState {
    entered: Vec<String>,
    holders: serde_json::Map<String, Json>,
}

/// Per-save lifecycle state for unique items.
pub struct ItemLedger {
    schema_version: i64,
    state: Mutex<Option<ItemLedgerState>>,
}

impl ItemLedger {
    pub const fn new(schema_version: i64) -> Self {
        Self {
            schema_version,
            state: Mutex::new(None),
        }
    }

    pub fn entered(&self, path: Option<PathBuf>) -> Vec<String> {
        self.ensure_loaded(path.as_deref());
        self.state
            .lock()
            .as_ref()
            .map(|state| state.entered.clone())
            .unwrap_or_default()
    }

    pub fn has_entered(&self, path: Option<PathBuf>, item_name: &str) -> bool {
        self.entered(path).iter().any(|name| name == item_name)
    }

    pub fn holder(&self, item_name: &str) -> Option<String> {
        self.state
            .lock()
            .as_ref()
            .and_then(|state| state.holders.get(item_name))
            .and_then(Json::as_str)
            .map(str::to_string)
    }

    pub fn mark_entered(&self, path: Option<PathBuf>, item_name: &str) -> bool {
        self.ensure_loaded(path.as_deref());
        {
            let mut slot = self.state.lock();
            let state = slot.get_or_insert_with(ItemLedgerState::default);
            if !state.entered.iter().any(|name| name == item_name) {
                state.entered.push(item_name.to_string());
            }
        }
        self.persist(path.as_deref())
    }

    pub fn set_holder(
        &self,
        path: Option<PathBuf>,
        item_name: &str,
        holder: Option<String>,
    ) -> bool {
        self.ensure_loaded(path.as_deref());
        {
            let mut slot = self.state.lock();
            let state = slot.get_or_insert_with(ItemLedgerState::default);
            match holder {
                Some(holder) => {
                    state.holders.insert(item_name.to_string(), json!(holder));
                }
                None => {
                    state.holders.remove(item_name);
                }
            }
        }
        self.persist(path.as_deref())
    }

    fn ensure_loaded(&self, path: Option<&Path>) {
        let mut slot = self.state.lock();
        if slot.is_some() {
            return;
        }
        let store: Option<Json> = path
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok());
        let entered = store
            .as_ref()
            .and_then(|value| value.get("entered"))
            .and_then(Json::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let holders = store
            .as_ref()
            .and_then(|value| value.get("holders"))
            .and_then(Json::as_object)
            .cloned()
            .unwrap_or_default();
        *slot = Some(ItemLedgerState { entered, holders });
    }

    fn persist(&self, path: Option<&Path>) -> bool {
        let Some(path) = path else { return true };
        let slot = self.state.lock();
        let state = slot.as_ref();
        let text = json!({
            "schema_version": self.schema_version,
            "entered": state.map(|state| state.entered.clone()).unwrap_or_default(),
            "holders": state.map(|state| state.holders.clone()).unwrap_or_default(),
        })
        .to_string();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &text).is_ok() && std::fs::rename(&tmp, path).is_ok()
    }
}

/// What a worn item does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Armor {
    pub slot: EquipSlot,
    pub amount: f32,
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

/// What a note says (topside design.md "The first hour": the note
/// that tells you not to go outside). Written once, read by whoever
/// finds it.
#[derive(Clone, PartialEq, Debug)]
pub struct Note {
    pub text: String,
    pub signed: String,
}

/// A stack of one item. Stacks merge only when item name AND quality
/// match; a Rare rifle never stacks on a Normal one. A `Note` item
/// carries its words on the stack.
#[derive(Clone, PartialEq, Debug)]
pub struct ItemStack {
    pub item: String,
    pub count: u32,
    pub quality: Option<ItemQuality>,
    pub note: Option<Note>,
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
        note: None,
    }
}

/// The one way a note comes to exist: a single `Note` item through
/// `create`, with its words. Refuses a def that is not a note.
pub fn write(def: &ItemDef, note: Note, now: f32, salt: u64) -> Result<ItemStack, String> {
    if def.kind != ItemKind::Note {
        return Err(format!("'{}' is not a note", def.name));
    }
    let mut stack = create(def, 1, &[], now, salt);
    stack.note = Some(note);
    Ok(stack)
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

    /// Take one of `item` from the first slot holding it (a round of
    /// ammo). False when there is none.
    pub fn take_one(&mut self, item: &str) -> bool {
        let Some(slot) = self
            .slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.item == item))
        else {
            return false;
        };
        self.remove(slot, 1).is_some()
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

/// Where worn and held gear goes: one named slot each (the Atlas
/// set). The weapon slot is what the hands hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EquipSlot {
    Head,
    Chest,
    Hands,
    Legs,
    Feet,
    Weapon,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 6] = [
        EquipSlot::Head,
        EquipSlot::Chest,
        EquipSlot::Hands,
        EquipSlot::Legs,
        EquipSlot::Feet,
        EquipSlot::Weapon,
    ];

    fn index(self) -> usize {
        match self {
            EquipSlot::Head => 0,
            EquipSlot::Chest => 1,
            EquipSlot::Hands => 2,
            EquipSlot::Legs => 3,
            EquipSlot::Feet => 4,
            EquipSlot::Weapon => 5,
        }
    }
}

/// An actor's equipment: the third item holder next to the
/// inventory and the hotbar, each its own thing. One stack at most
/// per slot. What may go where is decided when equipping lands
/// with combat.
#[derive(Clone, Default)]
pub struct Equipment {
    slots: [Option<ItemStack>; 6],
}

impl Equipment {
    pub fn get(&self, slot: EquipSlot) -> Option<&ItemStack> {
        self.slots[slot.index()].as_ref()
    }

    /// Put a stack in a slot, returning what was there.
    pub fn set(&mut self, slot: EquipSlot, stack: Option<ItemStack>) -> Option<ItemStack> {
        std::mem::replace(&mut self.slots[slot.index()], stack)
    }

    /// Take everything worn, leaving every slot empty.
    pub fn drain_all(&mut self) -> Vec<ItemStack> {
        self.slots.iter_mut().filter_map(|s| s.take()).collect()
    }
}

impl Inventory {
    /// Take every stack, leaving every slot empty.
    pub fn drain_all(&mut self) -> Vec<ItemStack> {
        self.slots.iter_mut().filter_map(|s| s.take()).collect()
    }
}

/// Everything a dead actor leaves behind: its inventory, its hotbar,
/// and what it wore, in that order, all holders left empty. The
/// consumer puts the list in a box where the body fell.
pub fn loot_all(inv: &mut Inventory, hotbar: &mut Inventory, worn: &mut Equipment) -> Vec<ItemStack> {
    let mut loot = inv.drain_all();
    loot.extend(hotbar.drain_all());
    loot.extend(worn.drain_all());
    loot
}

/// Move the stack in `from.slots[from_slot]` onto `to.slots[to_slot]`
/// across two different inventories: merge into a matching stack up
/// to `max_stack`, swap with a different one, or fill an empty slot.
/// The two-inventory twin of [`crate::hud::move_stack`].
pub fn move_between(
    from: &mut Inventory,
    from_slot: usize,
    to: &mut Inventory,
    to_slot: usize,
    max_stack: u32,
) {
    let (Some(src), Some(dst)) = (from.slots.get_mut(from_slot), to.slots.get_mut(to_slot))
    else {
        return;
    };
    match (src.as_mut(), dst.as_mut()) {
        (None, _) => {}
        (Some(_), None) => *dst = src.take(),
        (Some(s), Some(d)) if s.item == d.item && s.quality == d.quality => {
            let moved = s.count.min(max_stack.saturating_sub(d.count));
            d.count += moved;
            s.count -= moved;
            if s.count == 0 {
                *src = None;
            }
        }
        _ => std::mem::swap(src, dst),
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
            unique: false,
            kind: ItemKind::Material,
            max_stack: 10,
            quality_siblings: 3,
            combat: None,
            food: None,
            storage: None,
            armor: None,
            good_for: Default::default(),
            model: None,
        }
    }

    fn plain(name: &str, count: u32) -> ItemStack {
        ItemStack {
            item: name.to_string(),
            count,
            quality: None,
            note: None,
        }
    }

    #[test]
    fn a_note_is_written_once_and_only_on_a_note_def() {
        let mut paper = def("note");
        paper.kind = ItemKind::Note;
        let note = Note {
            text: "do not go outside".to_string(),
            signed: "M.".to_string(),
        };
        let stack = write(&paper, note.clone(), 0.0, 1).unwrap();
        assert_eq!(stack.count, 1);
        assert_eq!(stack.note, Some(note.clone()));
        assert!(write(&def("scrap"), note, 0.0, 1).is_err(), "scrap is not a note");
    }

    #[test]
    fn loot_all_empties_every_holder_in_order() {
        let mut inv = Inventory::new(3);
        inv.slots[2] = Some(plain("scrap", 4));
        let mut bar = Inventory::new(2);
        bar.slots[0] = Some(plain("pipe", 1));
        let mut worn = Equipment::default();
        worn.set(EquipSlot::Chest, Some(plain("vest", 1)));
        let loot = loot_all(&mut inv, &mut bar, &mut worn);
        let names: Vec<&str> = loot.iter().map(|s| s.item.as_str()).collect();
        assert_eq!(names, ["scrap", "pipe", "vest"]);
        assert_eq!(inv.count_of("scrap"), 0);
        assert_eq!(bar.count_of("pipe"), 0);
        assert!(worn.get(EquipSlot::Chest).is_none());
        assert!(loot_all(&mut inv, &mut bar, &mut worn).is_empty(), "nothing twice");
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
    fn equipment_holds_one_stack_per_named_slot() {
        let mut gear = Equipment::default();
        for slot in EquipSlot::ALL {
            assert!(gear.get(slot).is_none());
        }
        assert!(gear.set(EquipSlot::Weapon, Some(plain("pipe", 1))).is_none());
        let swapped = gear.set(EquipSlot::Weapon, Some(plain("hatchet", 1)));
        assert_eq!(swapped.unwrap().item, "pipe");
        assert_eq!(gear.get(EquipSlot::Weapon).unwrap().item, "hatchet");
        assert!(gear.get(EquipSlot::Head).is_none(), "slots are independent");
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
