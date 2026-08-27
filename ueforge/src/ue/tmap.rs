//! `TMap<K, V>` walker.
//!
//! UE's `TMap` is `TSet<TPair<K, V>>` whose element type is
//! `TSetElement<TPair> { pair: 16, HashNextId: i32, HashIndex: i32 } = 24 bytes`.
//! The set's storage is a `TSparseArray` whose slots are a union of
//! the element + a 2-i32 free-list link, sized to the larger = 24 bytes.
//! Free slots can hold any 8 bytes in the first 8; we walk all slots
//! linearly and key-match (false hits on a real key colliding with
//! free-list links are vanishingly unlikely; if observed, gate via
//! the AllocationFlags `TBitArray` at `+0x10` of the TMap).
//!
//! Layout has been stable through UE 5.0-5.4. Constants live in
//! [`crate::ue::offsets::tmap`] so callers can override per game
//! if a future engine bump shifts them.
//!
//! Pure pointer math + `read_unaligned`. No allocations on the
//! happy path. Safe to call from any thread that's already serving
//! as game-thread (drained via `ueforge::Queue`).

use crate::ue::UObject;
use crate::ue::offsets::tmap as off;

/// A scalar whose every bit pattern is valid when read from a live map slot.
///
/// # Safety
/// Implementors must be `Copy`, fit in eight bytes, and accept every possible
/// bit pattern. This is required because [`slots`] includes free sparse-array
/// entries whose bytes do not satisfy richer Rust type invariants.
pub unsafe trait TMapScalar: Copy {}

macro_rules! impl_tmap_scalar {
    ($($ty:ty),+ $(,)?) => {
        $(unsafe impl TMapScalar for $ty {})+
    };
}

impl_tmap_scalar!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

/// One scalar key/value pair read from a live Unreal `TMap` slot.
pub struct TMapEntry<K, V> {
    element: *mut u8,
    key: K,
    value: V,
}

impl<K: TMapScalar, V: TMapScalar> TMapEntry<K, V> {
    /// Return the key captured when this entry was read.
    pub fn key(&self) -> K {
        self.key
    }

    /// Return the value captured when this entry was read or last written.
    pub fn value(&self) -> V {
        self.value
    }

    /// Write a new value into this entry's live Unreal map slot.
    ///
    /// # Safety
    /// The owning `TMap` must still be live and must not have reallocated
    /// since this entry was returned by [`scalar_entries`].
    pub unsafe fn write_value(&mut self, value: V) {
        // SAFETY: the caller guarantees that the captured slot remains live.
        // `scalar_entries` checked that V fits in the fixed value half.
        unsafe {
            (self.element.add(off::PAIR_VALUE) as *mut V).write_unaligned(value);
        }
        self.value = value;
    }
}

/// Read scalar keys and values from every slot in a `TMap` field.
///
/// Both types must fit in the fixed eight-byte halves of Unreal's pair
/// layout. As with [`slots`], free sparse-array slots are included, so callers
/// must identify the entries they own by key.
///
/// # Safety
/// `obj` must be live, `offset` must identify a valid `TMap<K, V>`, and the
/// supplied Rust types must match the map's actual scalar key and value types.
pub unsafe fn scalar_entries<K: TMapScalar, V: TMapScalar>(
    obj: &UObject,
    offset: usize,
) -> impl Iterator<Item = TMapEntry<K, V>> {
    assert!(std::mem::size_of::<K>() <= off::PAIR_VALUE);
    assert!(std::mem::size_of::<V>() <= off::PAIR_VALUE);

    // SAFETY: the caller supplies the same live object and valid map offset
    // required by `slots`.
    unsafe { slots(obj, offset) }.map(|(_, element)| {
        // SAFETY: each pointer is a full map slot. The size checks above keep
        // both scalar reads inside their respective pair halves.
        unsafe {
            TMapEntry {
                element: element.cast_mut(),
                key: (element as *const K).read_unaligned(),
                value: (element.add(off::PAIR_VALUE) as *const V).read_unaligned(),
            }
        }
    })
}

/// Walk every `TPair<K, V>` slot in a `TMap` field stored at
/// `offset` inside the given object. Yields `(slot_index, slot_ptr)`
/// pairs. Each `slot_ptr` is the start of the 24-byte
/// `TSetElement<TPair>` slot. The first 8 bytes are the key
/// (`K = FName` u64 in DataTable cases), the next 8 are the value.
///
/// Slot bytes for free entries don't satisfy the type invariant.
/// callers must filter by key match or by inspecting the allocation
/// flags TBitArray at `+0x10` of the TMap (not implemented here;
/// the linear scan + key match is the expected pattern).
///
/// # Safety
/// `obj` must be a live UObject and `offset` must point at a valid
/// TMap field. The returned pointer is valid until GObjects /
/// owning UPackage is mutated. Callers typically dereference the
/// 8-byte key + 8-byte value-pointer immediately and don't hold
/// the slot pointer across drains.
pub unsafe fn slots(obj: &UObject, offset: usize) -> impl Iterator<Item = (usize, *const u8)> {
    // SAFETY: forwards `obj` + `offset` to `header`; the caller's
    // contract (live UObject, valid TMap offset) flows through.
    let (data_ptr, count) = unsafe { header(obj, offset) };
    let scan = count.min(off::MAX_LINEAR_SCAN);
    if count > off::MAX_LINEAR_SCAN {
        // One log line, not per-row, so a truncated big DT doesn't
        // spam. Bump MAX_LINEAR_SCAN if you see this on a real DT.
        crate::log::log(format_args!(
            "ueforge tmap: truncating walk at {} of {} slots (raise MAX_LINEAR_SCAN)",
            off::MAX_LINEAR_SCAN,
            count
        ));
    }
    (0..scan)
        .filter(move |_| !data_ptr.is_null())
        .map(move |i| {
            // SAFETY: `data_ptr` is the TSparseArray storage base; `i`
            // is bounded by the array's reported slot count (clamped
            // to MAX_LINEAR_SCAN). Each slot is `ELEMENT_SIZE = 24`
            // bytes per the TSetElement<TPair> layout. Pointer
            // arithmetic stays within the allocated array.
            (i, unsafe { data_ptr.add(i * off::ELEMENT_SIZE) })
        })
}

/// Read the TMap header at `obj.field_ptr(offset)`: returns
/// `(data_ptr, num_slots)`. `num_slots` includes both occupied
/// and free slots (the `TSparseArray` is dense in storage).
///
/// # Safety
/// Same as [`slots`].
pub unsafe fn header(obj: &UObject, offset: usize) -> (*const u8, usize) {
    // SAFETY: the caller's `unsafe fn` contract requires `obj` to be
    // a live UObject and `offset` to point at a valid TMap. Reading
    // the first 8 bytes (data pointer) and the i32 at +DATA_NUM is
    // well-defined for that layout. `read_unaligned` covers the case
    // where the TMap straddles a non-8-aligned offset on the row.
    unsafe {
        let row_map = obj.field_ptr(offset);
        let data_ptr = (row_map as *const *const u8).read_unaligned();
        let data_num = (row_map.add(off::DATA_NUM) as *const i32).read_unaligned();
        if data_ptr.is_null() || data_num <= 0 {
            (std::ptr::null(), 0)
        } else {
            (data_ptr, data_num as usize)
        }
    }
}

/// Find the slot whose `FName` key (u64-encoded) equals
/// `row_name`. Returns the value pointer (second 8 bytes of the
/// pair) when found.
///
/// # Safety
/// Same as [`slots`].
pub unsafe fn find_value_by_fname_key(
    obj: &UObject,
    offset: usize,
    row_name: u64,
) -> Option<*const u8> {
    // SAFETY: `slots` yields valid 24-byte slot pointers within the
    // TSparseArray storage. Reading the first 8 bytes as the FName
    // key matches the TPair<FName, *void> layout; reading
    // +PAIR_VALUE as a *const u8 matches the value half. Free-list
    // slots whose first 8 bytes happen to collide with `row_name`
    // are vanishingly unlikely; callers tolerate the null-check on
    // the value pointer as the second filter.
    unsafe {
        for (_, element) in slots(obj, offset) {
            let key: u64 = (element as *const u64).read_unaligned();
            if key == row_name {
                let value: *const u8 =
                    (element.add(off::PAIR_VALUE) as *const *const u8).read_unaligned();
                if !value.is_null() {
                    return Some(value);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic UObject buffer whose TMap field at `offset`
    /// is `{ data_ptr, num }`. The slot array follows. The buffer
    /// is leaked so its lifetime is `'static` for the test (small
    /// per-test allocation; freed at process exit).
    fn make_tmap(offset: usize, slots: &[(u64, u64)]) -> &'static UObject {
        // UObject header bytes don't matter for the walker; we
        // just need the TMap header to land at `offset` and the
        // slot array to be reachable through its data pointer.
        const HEADER: usize = 0;
        let mut buf = vec![0u8; offset + 64];
        // Pre-allocate the slot data area (24 bytes per slot).
        let mut slot_buf: Vec<u8> = vec![0u8; slots.len() * off::ELEMENT_SIZE];
        for (i, &(key, val)) in slots.iter().enumerate() {
            let base = i * off::ELEMENT_SIZE;
            slot_buf[base..base + 8].copy_from_slice(&key.to_le_bytes());
            slot_buf[base + off::PAIR_VALUE..base + off::PAIR_VALUE + 8]
                .copy_from_slice(&val.to_le_bytes());
        }
        let slot_ptr = slot_buf.as_ptr() as usize;
        // Leak slot_buf so the data pointer stays valid.
        let _: &'static mut [u8] = Box::leak(slot_buf.into_boxed_slice());

        // Write TMap header at `offset`: 8-byte data ptr +
        // 4-byte num at +DATA_NUM.
        buf[offset..offset + 8].copy_from_slice(&slot_ptr.to_le_bytes());
        buf[offset + off::DATA_NUM..offset + off::DATA_NUM + 4]
            .copy_from_slice(&(slots.len() as i32).to_le_bytes());

        let leaked: &'static [u8] = Box::leak(buf.into_boxed_slice());
        // SAFETY: leaked is 'static, properly aligned for u8.
        // The walker only reads through `field_ptr(offset)` which
        // is a byte read at offset. UObject layout details are
        // never touched by the walker.
        let _ = HEADER;
        unsafe { &*(leaked.as_ptr() as *const UObject) }
    }

    #[test]
    fn empty_map_yields_no_slots() {
        let obj = make_tmap(0, &[]);
        // SAFETY: synthetic buffer; offset 0 is valid.
        let count = unsafe { slots(obj, 0).count() };
        assert_eq!(count, 0);
    }

    #[test]
    fn single_slot_walks() {
        let obj = make_tmap(0, &[(0x1234_5678_DEAD_BEEF, 0x1111_2222)]);
        // SAFETY: synthetic buffer.
        let collected: Vec<(usize, *const u8)> =
            unsafe { slots(obj, 0).collect() };
        assert_eq!(collected.len(), 1);
        // SAFETY: element ptr points into the leaked slot buffer.
        let key = unsafe { (collected[0].1 as *const u64).read_unaligned() };
        assert_eq!(key, 0x1234_5678_DEAD_BEEF);
    }

    #[test]
    fn find_value_by_fname_hits_matching_key() {
        let obj = make_tmap(
            16,
            &[
                (0x1, 0x1111_1111),
                (0x2, 0x2222_2222),
                (0xDEAD_BEEF, 0xCAFE_BABE),
            ],
        );
        // SAFETY: synthetic buffer; TMap header at offset 16.
        let hit = unsafe { find_value_by_fname_key(obj, 16, 0xDEAD_BEEF) };
        let hit = hit.expect("key present");
        assert_eq!(hit as usize, 0xCAFE_BABE);
    }

    #[test]
    fn find_value_by_fname_misses_unknown_key() {
        let obj = make_tmap(8, &[(0x10, 0x100), (0x20, 0x200)]);
        // SAFETY: synthetic buffer.
        let hit = unsafe { find_value_by_fname_key(obj, 8, 0x999) };
        assert!(hit.is_none());
    }

    #[test]
    fn find_value_skips_null_value_pointer() {
        // Key matches but value=0; walker treats that as 'not
        // really a hit'. Free-slot defense.
        let obj = make_tmap(8, &[(0x5, 0)]);
        // SAFETY: synthetic buffer.
        let hit = unsafe { find_value_by_fname_key(obj, 8, 0x5) };
        assert!(hit.is_none());
    }

    #[test]
    fn null_data_pointer_yields_no_slots() {
        // Build a TMap header whose data ptr is null but num > 0.
        // The walker MUST NOT deref into a wild address.
        let mut buf = vec![0u8; 32];
        buf[0..8].copy_from_slice(&0u64.to_le_bytes()); // null data
        buf[off::DATA_NUM..off::DATA_NUM + 4]
            .copy_from_slice(&100i32.to_le_bytes()); // num lying about size
        let leaked: &'static [u8] = Box::leak(buf.into_boxed_slice());
        // SAFETY: synthetic.
        let obj = unsafe { &*(leaked.as_ptr() as *const UObject) };
        // SAFETY: walker is documented to bail on null data.
        let n = unsafe { slots(obj, 0).count() };
        assert_eq!(n, 0);
    }

    // Property test: arbitrary (key, value) sets built into a
    // synthetic TMap, then looked up by `find_value_by_fname_key`.
    // The walker must always return the matching value (or None
    // if absent), with no false positives from collisions inside
    // the slot bytes.
    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config {
            cases: 64,
            .. proptest::test_runner::Config::default()
        })]
        #[test]
        fn find_value_matches_or_misses(
            entries in proptest::collection::vec(
                (proptest::prelude::any::<u64>(), 1u64..u64::MAX),
                0..16usize,
            ),
            probe_key in proptest::prelude::any::<u64>(),
        ) {
            // Skip degenerate input: duplicate keys could legitimately
            // resolve to either entry, which isn't a walker bug.
            let mut keys: Vec<u64> = entries.iter().map(|&(k, _)| k).collect();
            keys.sort();
            keys.dedup();
            if keys.len() != entries.len() { return Ok(()); }

            let obj = make_tmap(0, &entries);
            // SAFETY: synthetic buffer.
            let hit = unsafe { find_value_by_fname_key(obj, 0, probe_key) };
            let expected: Option<u64> = entries.iter()
                .find(|&&(k, _)| k == probe_key)
                .map(|&(_, v)| v);
            match (hit, expected) {
                (None, None) => {},
                (Some(p), Some(v)) => assert_eq!(p as usize as u64, v),
                (a, b) => panic!("walker mismatch: got {a:?}, expected {b:?}"),
            }
        }
    }

    #[test]
    fn negative_num_yields_no_slots() {
        let mut buf = vec![0u8; 32];
        // Pretend data is non-null so the null-check doesn't
        // short-circuit; the num check is what must reject this.
        buf[0..8].copy_from_slice(&(1usize).to_le_bytes());
        buf[off::DATA_NUM..off::DATA_NUM + 4]
            .copy_from_slice(&(-50i32).to_le_bytes());
        let leaked: &'static [u8] = Box::leak(buf.into_boxed_slice());
        // SAFETY: synthetic; the walker MUST bail on negative
        // num before any pointer arithmetic.
        let obj = unsafe { &*(leaked.as_ptr() as *const UObject) };
        // SAFETY: walker is documented to bail on num <= 0.
        let n = unsafe { slots(obj, 0).count() };
        assert_eq!(n, 0);
    }
}
