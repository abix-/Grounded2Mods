//! Generic save-slot key resolver.
//!
//! Most UE5 RPG / survival games store a stable per-playthrough
//! identifier on a singleton class. Typically the in-world
//! GameState carries an `FGuid`. The pattern is:
//!
//! 1. Find the first live instance of class `X`.
//! 2. Read an `FGuid` (or other 16-byte stable id) at offset `Y`.
//! 3. Format as a filename-safe string.
//!
//! Game crates configure once with `(class_name, guid_offset)` and
//! plug `SlotKeyResolver::resolve` into `SlotPoller::spawn`.
//!
//! ```ignore
//! static SLOT_KEY: ueforge::rpg::SlotKeyResolver =
//!     ueforge::rpg::SlotKeyResolver::new("InGameGameState", 0x032C);
//!
//! ueforge::rpg::SlotPoller::spawn(
//!     std::time::Duration::from_secs(1),
//!     || SLOT_KEY.resolve(),
//!     |slot| activate(slot),
//!     || deactivate(),
//! );
//! ```

use crate::ue::{ClassRef, FGuid, UObject};

pub struct SlotKeyResolver {
    class: ClassRef,
    guid_offset: usize,
}

impl SlotKeyResolver {
    pub const fn new(class_name: &'static str, guid_offset: usize) -> Self {
        Self {
            class: ClassRef::new(class_name),
            guid_offset,
        }
    }

    /// Walk GObjects for the first live instance of the configured
    /// class, read the FGuid at `guid_offset`, format as filename.
    /// Returns None when the class isn't loaded, no instance is
    /// active (main menu / between saves), or the GUID is zero
    /// (not yet populated).
    pub fn resolve(&self) -> Option<String> {
        self.class
            .with_first_instance(|obj| read_guid(obj, self.guid_offset))?
    }
}

fn read_guid(obj: &UObject, offset: usize) -> Option<String> {
    let guid: FGuid = unsafe { obj.field_ptr(offset).cast::<FGuid>().read_unaligned() };
    if guid.is_zero() {
        None
    } else {
        Some(guid.to_filename())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_constructible() {
        const _R: SlotKeyResolver = SlotKeyResolver::new("Foo", 0x100);
    }

    #[test]
    fn unresolved_returns_none() {
        let r = SlotKeyResolver::new("DoesNotExist", 0x100);
        assert!(r.resolve().is_none());
    }
}
