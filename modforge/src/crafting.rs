//! The crafting vocabulary: recipes as DATA. A RecipeDef is one
//! recipe (inputs, output, station, time); the RecipeRegistry holds
//! the checked-in defs. [`craft`] is the ONE function that executes
//! a recipe: checks inputs, removes them, calls item::create for the
//! output with quality rolled at that moment.

use parking_lot::Mutex;

use crate::item::{self, Inventory, ItemRegistry, ItemStack};

struct PendingCraftResult<T> {
    payload: T,
    ready_at: f32,
    deadline: f32,
}

/// Engine-independent lifecycle for craft results that appear after
/// the originating craft call returns.
///
/// Consumers provide the game-specific payload, result lookup, error
/// handling, and cleanup. The queue owns readiness delays, retries,
/// deadline expiry, removal, and exactly-once cleanup.
pub struct CraftResultQueue<T> {
    jobs: Mutex<Vec<PendingCraftResult<T>>>,
}

impl<T> CraftResultQueue<T> {
    pub const fn new() -> Self {
        Self {
            jobs: Mutex::new(Vec::new()),
        }
    }

    /// Queue a result lookup. Both delays are measured from `now`.
    pub fn push(&self, payload: T, now: f32, ready_delay: f32, timeout: f32) {
        self.jobs.lock().push(PendingCraftResult {
            payload,
            ready_at: now + ready_delay,
            deadline: now + timeout,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.lock().is_empty()
    }

    pub fn len(&self) -> usize {
        self.jobs.lock().len()
    }

    /// Try every ready result, retaining unresolved work until its
    /// deadline. Completed, expired, and failed jobs are removed and
    /// passed to `cleanup` exactly once.
    pub fn advance(
        &self,
        now: f32,
        mut try_resolve: impl FnMut(&T, f32) -> Result<bool, String>,
        mut on_error: impl FnMut(&T, &str),
        mut cleanup: impl FnMut(T),
    ) {
        let mut jobs = self.jobs.lock();
        let mut index = 0;
        while index < jobs.len() {
            let job = &jobs[index];
            if now < job.ready_at {
                index += 1;
                continue;
            }
            let done = match try_resolve(&job.payload, now) {
                Ok(resolved) => resolved || now >= job.deadline,
                Err(error) => {
                    on_error(&job.payload, &error);
                    true
                }
            };
            if done {
                cleanup(jobs.remove(index).payload);
            } else {
                index += 1;
            }
        }
    }
}

impl<T> Default for CraftResultQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StationKind {
    CraftingBench,
    ChemistryTable,
    ElectronicsWorkbench,
    ArmoryBench,
    ResearchStation,
}

/// One recipe as data. Inputs are (item id, count) pairs consumed.
/// Output is the item id and count produced.
#[derive(Clone)]
pub struct RecipeDef {
    pub name: String,
    pub inputs: Vec<(String, u32)>,
    pub output: (String, u32),
    pub station: StationKind,
    pub craft_time: f32,
}

/// The collection of checked-in RecipeDefs. Consumers register their
/// content at startup and look defs up by name or by station.
#[derive(Default)]
pub struct RecipeRegistry {
    defs: Vec<RecipeDef>,
}

impl RecipeRegistry {
    pub fn register(&mut self, def: RecipeDef) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("recipe '{}' registered twice", def.name));
        }
        self.defs.push(def);
        Ok(())
    }

    pub fn def(&self, name: &str) -> Option<&RecipeDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn for_station(&self, station: StationKind) -> Vec<&RecipeDef> {
        self.defs.iter().filter(|d| d.station == station).collect()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// Check whether an inventory has all inputs for a recipe.
pub fn can_craft(recipe: &RecipeDef, inventory: &Inventory) -> bool {
    recipe.inputs.iter().all(|(id, count)| inventory.count_of(id) >= *count)
}

/// Execute a recipe: remove inputs from inventory, create the output
/// item stack with quality rolled at this moment, and return it. The
/// caller decides where the output goes (player inventory, world,
/// container). Returns None if inputs are missing.
pub fn craft(
    recipe: &RecipeDef,
    inventory: &mut Inventory,
    items: &ItemRegistry,
    quality_odds: &[u64],
    now: f32,
    salt: u64,
) -> Option<ItemStack> {
    if !can_craft(recipe, inventory) {
        return None;
    }
    for (id, count) in &recipe.inputs {
        let mut remaining = *count;
        for slot in 0..inventory.slots.len() {
            if remaining == 0 {
                break;
            }
            if let Some(stack) = &inventory.slots[slot] {
                if stack.item == *id {
                    let take = remaining.min(stack.count);
                    inventory.remove(slot, take);
                    remaining -= take;
                }
            }
        }
    }
    let output_def = items.def(&recipe.output.0)?;
    Some(item::create(output_def, recipe.output.1, quality_odds, now, salt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemDef, ItemKind, ItemRegistry, Inventory};

    fn setup() -> (ItemRegistry, RecipeRegistry, Inventory) {
        let mut items = ItemRegistry::default();
        items.register(ItemDef {
            name: "scrap".to_string(),
            unique: false,
            kind: ItemKind::Material,
            max_stack: 20,
            quality_siblings: 1,
            combat: None,
            food: None,
            storage: None,
            armor: None,
            good_for: Default::default(),
            model: None,
        }).unwrap();
        items.register(ItemDef {
            name: "knife".to_string(),
            unique: false,
            kind: ItemKind::Weapon,
            max_stack: 1,
            quality_siblings: 3,
            combat: Some(crate::item::CombatStats {
                damage: "knife slash".to_string(),
                delay: 0.8,
                reach: 1.5,
                pellets: 1,
                spread_degrees: 0.0,
                ammo: None,
            }),
            food: None,
            storage: None,
            armor: None,
            good_for: Default::default(),
            model: None,
        }).unwrap();

        let mut recipes = RecipeRegistry::default();
        recipes.register(RecipeDef {
            name: "craft knife".to_string(),
            inputs: vec![("scrap".to_string(), 3)],
            output: ("knife".to_string(), 1),
            station: StationKind::CraftingBench,
            craft_time: 2.0,
        }).unwrap();

        let mut inv = Inventory::new(5);
        inv.add(
            ItemStack { item: "scrap".to_string(), count: 10, quality: None, note: None },
            20,
        );

        (items, recipes, inv)
    }

    #[test]
    fn registry_rejects_duplicates() {
        let mut reg = RecipeRegistry::default();
        let def = RecipeDef {
            name: "test".to_string(),
            inputs: vec![],
            output: ("x".to_string(), 1),
            station: StationKind::CraftingBench,
            craft_time: 1.0,
        };
        reg.register(def.clone()).unwrap();
        assert!(reg.register(def).is_err());
    }

    #[test]
    fn can_craft_checks_inputs() {
        let (_, recipes, inv) = setup();
        let recipe = recipes.def("craft knife").unwrap();
        assert!(can_craft(recipe, &inv));

        let empty = Inventory::new(5);
        assert!(!can_craft(recipe, &empty));
    }

    #[test]
    fn craft_consumes_inputs_and_produces_output() {
        let (items, recipes, mut inv) = setup();
        let recipe = recipes.def("craft knife").unwrap();
        let result = craft(recipe, &mut inv, &items, &[], 1.0, 42).unwrap();
        assert_eq!(result.item, "knife");
        assert_eq!(result.count, 1);
        assert_eq!(inv.count_of("scrap"), 7);
    }

    #[test]
    fn craft_fails_when_inputs_missing() {
        let (items, recipes, _) = setup();
        let recipe = recipes.def("craft knife").unwrap();
        let mut empty = Inventory::new(5);
        assert!(craft(recipe, &mut empty, &items, &[], 1.0, 42).is_none());
    }

    #[test]
    fn craft_rolls_quality_on_output() {
        let (items, recipes, mut inv) = setup();
        let recipe = recipes.def("craft knife").unwrap();
        let result = craft(recipe, &mut inv, &items, &[1000], 1.0, 42).unwrap();
        assert!(result.quality.is_some());
    }

    #[test]
    fn for_station_filters_correctly() {
        let (_, recipes, _) = setup();
        assert_eq!(recipes.for_station(StationKind::CraftingBench).len(), 1);
        assert_eq!(recipes.for_station(StationKind::ChemistryTable).len(), 0);
    }
}
