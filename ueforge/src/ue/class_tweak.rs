//! `ClassFieldTweak<T>`. Live-UObject sibling of
//! [`crate::ue::datatable::FieldTweak`]. Walks every UObject of a
//! named class, snapshots a vanilla value per instance the first
//! time it's seen, and writes a transformed value on every apply.
//!
//! Use this anywhere a mod walks GObjects to mutate a field at a
//! known offset on every instance of a class:
//!
//! ```ignore
//! static SLOTS: ClassFieldTweak<i32> =
//!     ClassFieldTweak::new("InventoryComponent", 0x01E0);
//!
//! // Apply on init, or every time a slider changes:
//! let stats = SLOTS.apply(
//!     // Filter: only player-owned components, skip CDO mounts.
//!     |obj| obj.full_name().contains("BP_SurvivalPlayerCharacter"),
//!     // Transform: vanilla -> target. Return None to skip writing
//!     // this instance (bad value, already at target, etc.).
//!     |vanilla| if vanilla <= 1 { None } else { Some(target) },
//! )?;
//! ```
//!
//! Two closure shape:
//! - `filter(&UObject) -> bool` runs *before* the vanilla read, so
//!   non-target instances cost only the `is_a` + `full_name` check.
//! - `transform(T) -> Option<T>` runs against the captured vanilla
//!   on every apply. `None` means "leave this instance alone";
//!   `Some(t)` writes `t` if it differs from the current value.
//!
//! Like [`crate::ue::datatable::FieldTweak`], the vanilla snapshot
//! is keyed per-instance (by UObject address) so re-applies with a
//! different transform don't compound on each other. Toggle a
//! 4x multiplier to 8x, the second apply rewrites from vanilla, not
//! from 4x-of-vanilla.
//!
//! Threading: the vanilla map is mutex-guarded. Apply runs on
//! whichever thread calls it (typically a worker thread or a tab
//! callback); GObjects walks are off-thread-safe in UE.

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;

use crate::ue::{self, GObjectsView, UObject};

/// Counters returned by [`ClassFieldTweak::apply`]. `scanned` is
/// every instance that passed the class filter (including CDOs);
/// `considered` is the subset the user `filter` accepted;
/// `patched` is the subset the user `transform` chose to write.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassTweakStats {
    pub scanned: usize,
    pub considered: usize,
    pub patched: usize,
}

pub struct ClassFieldTweak<T: Copy + PartialEq + Send + 'static> {
    class_name: &'static str,
    offset: usize,
    vanilla: OnceLock<Mutex<HashMap<usize, T>>>,
}

impl<T: Copy + PartialEq + Send + 'static> ClassFieldTweak<T> {
    pub const fn new(class_name: &'static str, offset: usize) -> Self {
        Self {
            class_name,
            offset,
            vanilla: OnceLock::new(),
        }
    }

    /// Walk the named class. For every instance the `filter`
    /// accepts, snapshot vanilla on first sight, then write
    /// `transform(vanilla)` if the transform returns `Some`.
    pub fn apply<Filter, Transform>(
        &self,
        filter: Filter,
        transform: Transform,
    ) -> Result<ClassTweakStats, String>
    where
        Filter: Fn(&UObject) -> bool,
        Transform: Fn(T) -> Option<T>,
    {
        let rt = ue::try_runtime().ok_or("ueforge: runtime not initialized")?;
        let class = ue::find_class_fast(self.class_name)
            .ok_or_else(|| format!("class '{}' not found", self.class_name))?;
        let view = unsafe { GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
        if !view.is_valid() {
            return Err("gobjects view invalid".into());
        }

        let vanilla = self.vanilla.get_or_init(|| Mutex::new(HashMap::new()));
        let mut van = vanilla.lock();

        let mut stats = ClassTweakStats::default();
        for obj in view.iter() {
            if !obj.is_a(class) {
                continue;
            }
            stats.scanned += 1;
            if !filter(obj) {
                continue;
            }
            stats.considered += 1;

            let key = obj as *const UObject as usize;
            let cur = unsafe { (obj.field_ptr(self.offset) as *const T).read_unaligned() };
            let vanilla_val = *van.entry(key).or_insert(cur);

            let Some(target) = transform(vanilla_val) else {
                continue;
            };
            if cur != target {
                unsafe {
                    (obj.field_ptr(self.offset) as *mut T).write_unaligned(target);
                }
                stats.patched += 1;
            }
        }
        Ok(stats)
    }

    /// Restore every captured instance back to its vanilla value.
    /// Useful for "disable this skill" / "reset to defaults" paths.
    pub fn revert(&self) -> Result<usize, String> {
        let Some(van_lock) = self.vanilla.get() else {
            return Ok(0);
        };
        let van = van_lock.lock();
        let mut reverted = 0;
        for (&addr, &vanilla_val) in van.iter() {
            unsafe {
                let p = addr as *mut u8;
                let cur = (p.add(self.offset) as *const T).read_unaligned();
                if cur != vanilla_val {
                    (p.add(self.offset) as *mut T).write_unaligned(vanilla_val);
                    reverted += 1;
                }
            }
        }
        Ok(reverted)
    }

    pub fn class_name(&self) -> &'static str {
        self.class_name
    }
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// How many instances have a captured vanilla baseline.
    pub fn vanilla_count(&self) -> usize {
        self.vanilla.get().map(|m| m.lock().len()).unwrap_or(0)
    }
}
