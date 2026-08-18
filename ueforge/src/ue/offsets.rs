// SDK-version-coupled offset constants. Cross-checked against
// Dumper-7's `SDK/Basic.hpp` for Grounded 2 on the active UE 5.4
// build. To regenerate against a new game patch:
//
//   1. Run Dumper-7 (https://github.com/Encryqed/Dumper-7) against
//      the running Maine-Win64-Shipping.exe.
//   2. Open the generated `SDK/Basic.hpp` and read off
//      `UObject` / `UField` / `UClass` / `UFunction` / `FName` /
//      `FProperty` offsets.
//
// Address offsets (g_objects, g_names, append_string) are resolved
// by patternsleuth at init. The STEAM/XBOX constants here are used
// by grounded2-mod's detect_and_init path which predates the
// patternsleuth migration. Two builds: Steam and Xbox Game Pass.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Steam,
    Xbox,
}

#[derive(Debug, Clone, Copy)]
pub struct PlatformOffsets {
    pub g_objects: usize,
    pub append_string: usize,
    pub g_names: usize,
    pub process_event_idx: usize,
    /// Layout of the GObjects array.
    pub g_objects_layout: GObjectsLayout,
}

/// UE has used several `UObjectArray` layouts over the years.
/// The two we care about:
///
/// - `FlatFixed`: `g_objects` points directly at an
///   `FFixedUObjectArray { FUObjectItem* Objects; int32 MaxElements;
///   int32 NumElements; }`. Used by Grounded 2's UE5-Augusta build.
/// - `WrappedChunked`: `g_objects` points at an `FUObjectArray`
///   whose `ObjObjects` member at +0x10 is an
///   `FChunkedFixedUObjectArray { FUObjectItem** Objects; void*
///   PreAllocatedObjects; int32 MaxElements; int32 NumElements;
///   int32 MaxChunks; int32 NumChunks; }`. Used by stock UE 5.x
///   (including Outworld Station). Each chunk holds 64K
///   `FUObjectItem`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GObjectsLayout {
    FlatFixed,
    WrappedChunked,
}

pub const STEAM: PlatformOffsets = PlatformOffsets {
    g_objects: 0x09F6_7028,
    append_string: 0x0125_2060,
    g_names: 0x09E4_A7B8,
    process_event_idx: 0x4C,
    g_objects_layout: GObjectsLayout::FlatFixed,
};

pub const XBOX: PlatformOffsets = PlatformOffsets {
    g_objects: 0x09F3_6F28,
    append_string: 0x0125_0F80,
    g_names: 0x09E1_A6B8,
    process_event_idx: 0x4C,
    g_objects_layout: GObjectsLayout::FlatFixed,
};

impl Platform {
    pub fn offsets(self) -> &'static PlatformOffsets {
        match self {
            Platform::Steam => &STEAM,
            Platform::Xbox => &XBOX,
        }
    }
}

// Field offsets inside UObject and its derived classes. These are stable
// across the two platforms because they're determined by the C++ class
// layout, not by where the binary places statics.

pub mod uobject {
    pub const VTABLE: usize = 0x00;
    pub const FLAGS: usize = 0x08;
    pub const INDEX: usize = 0x0C;
    pub const CLASS: usize = 0x10;
    pub const NAME: usize = 0x18;
    pub const OUTER: usize = 0x20;
    pub const SIZE: usize = 0x28;
}

pub mod ufield {
    pub const NEXT: usize = 0x28;
    pub const SIZE: usize = 0x30;
}

pub mod ustruct {
    pub const SUPER_STRUCT: usize = 0x40;
    pub const CHILDREN: usize = 0x48;            // UField* (functions)
    pub const CHILD_PROPERTIES: usize = 0x50;    // FField* (native props)
    pub const SIZE: usize = 0xB0;                // PropertiesSize i32
}

/// FField. UE 5.x lightweight property header. Lives off
/// `UStruct::ChildProperties` chain, threaded by `Next`.
/// Verified against UE 5.4 stock; sizes/offsets are stable
/// through UE 5.x.
pub mod ffield {
    pub const CLASS_PRIVATE: usize = 0x08;
    pub const OWNER: usize = 0x10;          // FFieldVariant (2 ptrs = 0x10)
    pub const NEXT: usize = 0x20;
    pub const NAME_PRIVATE: usize = 0x28;   // FName (8 bytes)
    pub const SIZE: usize = 0x30;
}

/// FProperty extends FField. `OFFSET_INTERNAL` is the byte
/// offset of this field within an instance. What
/// inspect_address uses to map an address back to a field name.
pub mod fproperty {
    pub const ARRAY_DIM: usize = 0x30;
    pub const ELEMENT_SIZE: usize = 0x34;
    pub const PROPERTY_FLAGS: usize = 0x38;
    pub const REP_INDEX: usize = 0x40;
    pub const OFFSET_INTERNAL: usize = 0x4C; // verified live on OWS UE 5.4
}

pub mod uclass {
    pub const CAST_FLAGS: usize = 0xD8;
    pub const CLASS_DEFAULT_OBJECT: usize = 0x110;
    pub const SIZE: usize = 0x200;
}

pub mod ufunction {
    pub const FUNCTION_FLAGS: usize = 0xB0;
    pub const SIZE: usize = 0xE0;
}

/// `FFixedUObjectArray`. Used by `GObjectsLayout::FlatFixed`.
pub mod tuobject_array {
    pub const OBJECTS: usize = 0x00;
    pub const MAX_ELEMENTS: usize = 0x08;
    pub const NUM_ELEMENTS: usize = 0x0C;
}

/// `FUObjectArray` wraps a `FChunkedFixedUObjectArray ObjObjects`
/// at offset 0x10. Field offsets are within the inner struct.
/// Used by `GObjectsLayout::WrappedChunked`.
pub mod chunked_uobject_array {
    pub const OBJ_OBJECTS: usize = 0x10;          // FUObjectArray::ObjObjects
    pub const OBJECTS: usize = 0x00;              // FUObjectItem** chunk-ptrs
    pub const PRE_ALLOCATED_OBJECTS: usize = 0x08;
    pub const MAX_ELEMENTS: usize = 0x10;
    pub const NUM_ELEMENTS: usize = 0x14;
    pub const MAX_CHUNKS: usize = 0x18;
    pub const NUM_CHUNKS: usize = 0x1C;
    pub const NUM_ELEMENTS_PER_CHUNK: usize = 64 * 1024;
}

pub mod fuobject_item {
    pub const OBJECT: usize = 0x00;
    pub const SIZE: usize = 0x18;
}

// FUNC_Native flag, set when calling FString-returning BlueprintPure
// statics so the engine doesn't try to allocate result params.
pub const FUNC_NATIVE: u32 = 0x400;

// TMap layout. Stable through UE 5.0-5.4. Verified empirically
// (the stride was 16 in older UE; UE5 bumped it to 24). Override
// these per-game if a future engine bump shifts them. Callers
// pass the offsets explicitly to `tmap::slots(obj, offset)` so
// only the per-element / per-pair shape is constants here.
pub mod tmap {
    /// Stride of one slot in the underlying `TSparseArray<
    /// TSetElement<TPair<K, V>>>`. The slot is a union of the
    /// element (16-byte pair + i32 HashNextId + i32 HashIndex)
    /// and a 2-i32 free-list link, sized to the larger.
    pub const ELEMENT_SIZE: usize = 24;
    /// Offset of `TPair::Value` within an element (after the
    /// 8-byte key).
    pub const PAIR_VALUE: usize = 8;
    /// Offset of `Num` (i32) inside the TMap header.
    pub const DATA_NUM: usize = 8;
    /// Cap on linear scan length to bound runaway when a TMap
    /// header is corrupted or being mutated mid-walk. Bumped to
    /// 64K to cover the largest legitimate DataTables we've seen
    /// in stock UE5 games (status-effect tables on long-tail
    /// titles, item tables on RPG-shaped builds). If you hit the
    /// cap on a real DT, raise this and ship a test.
    pub const MAX_LINEAR_SCAN: usize = 65_536;
}

// UDataTable layout. RowMap (TMap<FName, uint8*>) sits at +0x30
// on UE 5.x. Verified against Dumper-7 output for Grounded 2
// (UE5-Augusta).
pub mod datatable {
    pub const ROW_MAP: usize = 0x0030;
    /// `UScriptStruct* RowStruct`. Describes the row schema
    /// (FProperty chain at `+CHILD_PROPERTIES` on the UScriptStruct,
    /// same as any UStruct). Verified against Dumper-7 output for
    /// Grounded 2 (UE5-Augusta).
    pub const ROW_STRUCT: usize = 0x0028;
}
