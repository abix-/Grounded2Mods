// TArray<T> = { data: *mut T, num: i32, max: i32 }. 16 bytes.

use std::slice;

#[repr(C)]
#[derive(Debug)]
pub struct TArray<T> {
    pub data: *mut T,
    pub num: i32,
    pub max: i32,
}

impl<T> Default for TArray<T> {
    fn default() -> Self {
        Self {
            data: std::ptr::null_mut(),
            num: 0,
            max: 0,
        }
    }
}

impl<T> TArray<T> {
    pub fn len(&self) -> i32 {
        self.num
    }

    pub fn is_empty(&self) -> bool {
        self.num <= 0 || self.data.is_null()
    }

    /// Grow this TArray to `new_max` capacity using `size_of::<T>()`
    /// as the element stride. See [`grow_raw`] for the untyped variant.
    ///
    /// # Safety
    /// Caller must ensure `self` points at a valid, live TArray
    /// header in game memory and that no other thread is mutating
    /// the same array concurrently.
    pub unsafe fn grow(&mut self, new_max: i32) -> Result<(), String> {
        let header = self as *mut TArray<T> as *mut u8;
        unsafe { grow_raw(header, std::mem::size_of::<T>(), new_max) }
    }

    pub unsafe fn as_slice(&self) -> &[T] {
        if self.is_empty() {
            return &[];
        }
        unsafe { slice::from_raw_parts(self.data, self.num as usize) }
    }
}

/// Grow a TArray at `header_ptr` (16 bytes: pointer + num + max)
/// to `new_max` capacity with the given element `stride`.
///
/// Allocates a new zeroed buffer FROM THE ENGINE'S ALLOCATOR,
/// copies existing entries, and updates pointer and max.
///
/// The old buffer is leaked rather than freed: the engine may
/// still hold a pointer into it, and freeing memory the engine
/// thinks it owns is the same class of bug as allocating memory
/// it does not. The leak is bounded by how often a mod grows an
/// array, which is a handful of times per load.
///
/// Fails when the engine allocator cannot be reached. It does NOT
/// fall back to Rust's heap; see `ue::gmalloc`.
///
/// # Safety
/// `header_ptr` must point at a valid TArray header (16 bytes).
pub unsafe fn grow_raw(header_ptr: *mut u8, stride: usize, new_max: i32) -> Result<(), String> {
    let old_ptr = unsafe { *(header_ptr as *const *mut u8) };
    let old_num = unsafe { *((header_ptr as usize + 8) as *const i32) };
    let old_max = unsafe { *((header_ptr as usize + 12) as *const i32) };

    if new_max <= old_max {
        return Err(format!(
            "new_max ({new_max}) must exceed current max ({old_max})"
        ));
    }
    if old_num < 0 || old_num > old_max {
        return Err(format!("corrupt TArray: num={old_num} max={old_max}"));
    }

    let new_size = (new_max as usize) * stride;
    // The ENGINE's allocator, never Rust's. This buffer is handed
    // to an engine TArray, so the engine will realloc and free it
    // later. A Rust-heap pointer survives until that moment and
    // then kills the process:
    //
    //   FMallocBinned2 Attempt to realloc an unrecognized block
    //   canary == 0x65 != 0xe3
    //
    // which surfaced as a crash on disconnect, long after the
    // growth that caused it. There is deliberately no fallback to
    // std::alloc: growing with the wrong allocator is worse than
    // not growing.
    let new_ptr = crate::ue::gmalloc::alloc_zeroed(
        new_size,
        crate::ue::gmalloc::DEFAULT_ALIGNMENT,
    )
    .ok_or("TArray grow: engine allocator (GMalloc) unavailable")?;

    let old_bytes = (old_num as usize) * stride;
    if !old_ptr.is_null() && old_bytes > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(old_ptr, new_ptr, old_bytes);
        }
    }

    unsafe {
        *(header_ptr as *mut *mut u8) = new_ptr;
        *((header_ptr as usize + 12) as *mut i32) = new_max;
    }
    Ok(())
}

/// Iterate a TArray of fixed-stride structs at `header_ptr`.
/// Yields `(index, element_ptr)` for each entry. The element
/// pointer is valid for `stride` bytes.
///
/// # Safety
/// `header_ptr` must point at a valid TArray header (16 bytes).
/// Each element must be at least `stride` bytes.
pub unsafe fn iter_stride(
    header_ptr: *const u8,
    stride: usize,
) -> impl Iterator<Item = (usize, *const u8)> {
    let data_ptr = unsafe { *(header_ptr as *const *const u8) };
    let num = unsafe { *((header_ptr as usize + 8) as *const i32) };
    let count = if data_ptr.is_null() || num <= 0 { 0 } else { num as usize };
    (0..count).map(move |i| (i, unsafe { data_ptr.add(i * stride) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_default() {
        let a: TArray<i32> = TArray::default();
        assert_eq!(a.len(), 0);
        assert!(a.is_empty());
        let s = unsafe { a.as_slice() };
        assert!(s.is_empty());
    }

    #[test]
    fn negative_num_is_empty() {
        let a: TArray<u8> = TArray {
            data: 1 as *mut u8,
            num: -5,
            max: 0,
        };
        assert!(a.is_empty());
        let s = unsafe { a.as_slice() };
        assert!(s.is_empty());
    }

    #[test]
    fn null_data_is_empty() {
        let a: TArray<u32> = TArray {
            data: std::ptr::null_mut(),
            num: 7,
            max: 7,
        };
        assert!(a.is_empty());
    }

    #[test]
    fn slice_round_trip() {
        let mut buf: [u32; 4] = [11, 22, 33, 44];
        let a: TArray<u32> = TArray {
            data: buf.as_mut_ptr(),
            num: buf.len() as i32,
            max: buf.len() as i32,
        };
        assert_eq!(a.len(), 4);
        assert!(!a.is_empty());
        let s = unsafe { a.as_slice() };
        assert_eq!(s, &[11, 22, 33, 44]);
    }

    proptest::proptest! {
        #[test]
        fn is_empty_holds_for_garbage_headers(
            num in proptest::prelude::any::<i32>(),
            max in proptest::prelude::any::<i32>(),
            data_addr in proptest::prelude::any::<usize>(),
        ) {
            let a: TArray<u8> = TArray {
                data: data_addr as *mut u8,
                num,
                max,
            };
            assert_eq!(a.len(), num);
            let empty = a.is_empty();
            if num <= 0 || data_addr == 0 {
                assert!(empty);
            }
        }
    }

    #[test]
    fn repr_c_layout_matches_engine() {
        assert_eq!(std::mem::size_of::<TArray<u8>>(), 16);
        assert_eq!(std::mem::size_of::<TArray<u64>>(), 16);
        assert_eq!(std::mem::size_of::<TArray<*const u8>>(), 16);
    }
}
