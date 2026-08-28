//! UE5 core struct mirrors.
//!
//! Plain-old-data `#[repr(C)]` mirrors of stable UE5 engine types
//! every mod ends up redefining locally. Layouts are stable across
//! UE 5.0-5.7; offset constants for game-specific containers (where
//! these fields live on a UObject) stay game-side.
//!
//! ## Zero-copy layout verification
//!
//! Every public POD struct in this module derives `zerocopy`'s
//! `FromBytes`, `IntoBytes`, `KnownLayout`, and `Immutable`. The
//! derive macros verify at **compile time** that the layout is
//! actually POD: no padding-with-pointers, no `Drop`, alignment
//! known. If a future field break that invariant the build fails
//! at the derive site instead of producing UB at the read site.
//! Consumers (FDataTableRowHandle parm decoders, save-slot key
//! reads, FDamageInfo.InstigatorController reads, etc.) can use
//! `T::ref_from_bytes(slice)` for verified-safe transmute.

#![allow(clippy::unused_unit)]

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::ue::UObject;

/// `FGuid`. Four u32s. Used everywhere UE5 needs a stable
/// identifier (save GUIDs, asset GUIDs, replication identifiers).
#[repr(C)]
#[derive(
    Clone, Copy, Default, PartialEq, Eq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable,
)]
pub struct FGuid {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
}

impl FGuid {
    pub const fn zero() -> Self {
        Self {
            a: 0,
            b: 0,
            c: 0,
            d: 0,
        }
    }

    pub const fn is_zero(&self) -> bool {
        self.a == 0 && self.b == 0 && self.c == 0 && self.d == 0
    }

    /// Format as a 32-char lowercase hex string suitable as a
    /// filename component. Order is Microsoft "GuidStruct" memory
    /// order (a, b, c, d), not the canonical "8-4-4-4-12" string.
    /// For stable on-disk filenames this is what matches UE's
    /// internal GUID stringification.
    pub fn to_filename(self) -> String {
        format!("{:08x}{:08x}{:08x}{:08x}", self.a, self.b, self.c, self.d)
    }
}

/// `FWeakObjectPtr`. Two i32s. Index into the global UObject
/// array + serial number. UE invalidates the pointer when the
/// serial doesn't match the live object at that index; we don't
/// resolve here (resolution requires `runtime()` and a GObjects
/// view). Use this struct as a layout mirror in parm structs and
/// at known offsets on host objects (e.g. `FDamageInfo.InstigatorController`).
#[repr(C)]
#[derive(
    Clone, Copy, Default, PartialEq, Eq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable,
)]
pub struct FWeakObjectPtr {
    pub object_index: i32,
    pub object_serial_number: i32,
}

impl FWeakObjectPtr {
    pub const fn is_null(&self) -> bool {
        self.object_index == 0 && self.object_serial_number == 0
    }

    /// Read an `FWeakObjectPtr` from `parent` at byte `offset`.
    pub fn read(parent: &super::UObject, offset: usize) -> Self {
        // SAFETY: caller's contract is that `offset` is the byte
        // offset of an FWeakObjectPtr field on `parent`. The struct
        // is 8 bytes #[repr(C)]; read_unaligned tolerates any
        // alignment.
        unsafe {
            parent
                .field_ptr(offset)
                .cast::<FWeakObjectPtr>()
                .read_unaligned()
        }
    }

    /// Resolve this weak pointer to a live `&'static UObject` via
    /// the live `Runtime` GObjects view. Returns `None` if the
    /// index is invalid or the runtime hasn't been initialized
    /// yet.
    pub fn resolve(&self) -> Option<&'static super::UObject> {
        if self.object_index <= 0 {
            return None;
        }
        let rt = super::try_runtime()?;
        let view = unsafe { super::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
        view.get(self.object_index)
            .map(|o| unsafe { &*(o as *const super::UObject) })
    }
}

/// `FDataTableRowHandle`. Pointer to a UDataTable + FName key
/// of the row. UFunctions that take a "row handle" parm receive
/// this struct by value. Many UE5 status-effect / loot / spawner
/// systems route through this shape.
///
/// The `row_name` field is an `FName`-shaped u64 because UE5's
/// FName layout (`u32 ComparisonIndex; u32 Number;`) is bit-equivalent
/// to a u64 read at the same address. Use [`crate::ue::FName`]
/// when constructing/parsing.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FDataTableRowHandle {
    pub data_table: *const UObject,
    pub row_name: u64,
}

impl FDataTableRowHandle {
    pub const fn null() -> Self {
        Self {
            data_table: std::ptr::null(),
            row_name: 0,
        }
    }

    pub fn is_null(&self) -> bool {
        self.data_table.is_null()
    }
}

// SAFETY: FDataTableRowHandle is a POD layout used in parm structs
// crossing into the engine and back. The pointer is to a UObject
// the engine owns; we don't free it.
unsafe impl Send for FDataTableRowHandle {}
unsafe impl Sync for FDataTableRowHandle {}

/// `EStatusEffectValueType`. The canonical UE5 status-effect
/// value combine semantics enum. Stat values combine across
/// active effects either by summation (`Add`) or by multiplication
/// (`Multiply`); `None` means the stat is presence-only (the row's
/// `Value` field is unused).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EStatusEffectValueType {
    None = 0,
    Add = 1,
    Multiply = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fguid_zero_round_trip() {
        let g = FGuid::zero();
        assert!(g.is_zero());
        assert_eq!(g.to_filename(), "00000000000000000000000000000000");
    }

    #[test]
    fn fguid_format() {
        let g = FGuid {
            a: 0x12345678,
            b: 0xABCDEF01,
            c: 0xDEADBEEF,
            d: 0xFEEDFACE,
        };
        assert_eq!(g.to_filename(), "12345678abcdef01deadbeeffeedface");
        assert!(!g.is_zero());
    }

    #[test]
    fn fweak_null() {
        let w = FWeakObjectPtr::default();
        assert!(w.is_null());
    }

    #[test]
    fn fdatatable_row_handle_null() {
        let h = FDataTableRowHandle::null();
        assert!(h.is_null());
    }

    #[test]
    fn fguid_size_invariant() {
        // FGuid must be exactly 16 bytes; UE relies on this.
        assert_eq!(std::mem::size_of::<FGuid>(), 16);
    }

    #[test]
    fn fweak_size_invariant() {
        assert_eq!(std::mem::size_of::<FWeakObjectPtr>(), 8);
    }
}
