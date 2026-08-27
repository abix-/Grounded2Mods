// FName: 8 bytes ({ comparison_index: i32, number: u32 }). To resolve it to
// a string we call the game's AppendString export at module_base +
// offsets.append_string. Matches Basic.hpp:341.
//
// `to_string` caches by FName u64 to avoid re-running AppendString for the
// same name on every call. Each AppendString invocation leaks its FString
// buffer (we don't bind UE's FMemory::Free); the cache caps the leak at
// one buffer per unique FName. Bounded by the game's name pool size
// (~50K) instead of unbounded per call. See `fstring.rs`.
//
// The cache stores `Arc<str>` so cache hits return one ref-bump
// instead of a heap-allocating `String::clone`. Callers that need
// an owned `String` call `.to_string()` once at the boundary.
// same cost as before; callers that just compare or substring
// (e.g. `name.starts_with("Default__")`) save the allocation
// entirely.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::ue::fstring::FString;
use crate::ue::offsets::PlatformOffsets;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FName {
    pub comparison_index: i32,
    pub number: u32,
}

// Compile-time guard: every `transmute_copy::<u64, FName>` /
// `as_u64()` site assumes FName is exactly 8 bytes. If UE ever
// changes the layout (e.g. UE5.x bumps to a 12-byte
// `WIDE_NAME_LENGTH` scheme), this fires at build time instead
// of silently corrupting names at runtime.
const _: () = {
    assert!(
        std::mem::size_of::<FName>() == 8,
        "FName must be 8 bytes for u64 transmute round-trips"
    );
    assert!(
        std::mem::align_of::<FName>() <= 8,
        "FName alignment must fit in u64"
    );
};

/// Highest plausible FName comparison index. UE games in the
/// wild peak around ~500K entries; 16M is comfortably above
/// anything real and catches the "raw memory interpreted as
/// FName" garbage cases that crash `AppendString`.
const FNAME_INDEX_SANITY_MAX: i32 = 16 * 1024 * 1024;

/// Read an `FName` field and resolve it to its text.
///
/// An `FName` in memory is two 32-bit halves, a comparison index
/// then a number. Reading one and resolving it is five lines, and
/// those five lines were written out longhand at every call site
/// that touches a struct holding a name. MISERY's vendor code
/// alone had it eight times.
///
/// `None` when the runtime is not up or the value is not a
/// plausible name, so a garbage read returns nothing rather than
/// reaching `AppendString` with rubbish, which crashes.
///
/// # Safety
///
/// `base + offset` must be inside a live object, and the caller
/// must be on the game thread.
pub unsafe fn read_at(base: *const u8, offset: usize) -> Option<String> {
    // SAFETY: forwarded from the caller.
    unsafe { read_with_id(base, offset) }.map(|(_, name)| name)
}

/// The same, keeping the comparison index as well as the text.
///
/// Some structures store that index as the item's id, so a caller
/// that is going to write one back needs both halves.
///
/// # Safety
///
/// As [`read_at`].
pub unsafe fn read_with_id(base: *const u8, offset: usize) -> Option<(u32, String)> {
    let rt = crate::ue::try_runtime()?;
    // SAFETY: the caller's promise about base + offset.
    let (index, number) = unsafe {
        (
            crate::ue::read_at::<u32>(base, offset),
            crate::ue::read_at::<u32>(base, offset + 4),
        )
    };
    let name = FName::from_u64((index as u64) | ((number as u64) << 32));
    if !name.is_plausible() {
        return None;
    }
    // SAFETY: the name passed the plausibility check the resolver
    // needs.
    Some((index, unsafe { rt.name_resolver.to_string(name) }))
}

impl FName {
    pub fn from_u64(raw: u64) -> Self {
        Self {
            comparison_index: (raw & 0xFFFF_FFFF) as i32,
            number: (raw >> 32) as u32,
        }
    }

    pub fn is_none(self) -> bool {
        self.comparison_index == 0 && self.number == 0
    }

    /// Cheap plausibility filter applied before calling
    /// `AppendString`. Rejects negative indices and absurdly large
    /// ones so the engine isn't asked to dereference past the
    /// FName pool. This is a leaf-level guard; SEH wrapping in
    /// `NameResolver::to_arc` is the second line of defense.
    pub fn is_plausible(self) -> bool {
        self.comparison_index >= 0 && self.comparison_index < FNAME_INDEX_SANITY_MAX
    }

    /// Pack the 8-byte FName into a single u64. Usable as a hash
    /// key, useful for transmute round-trips with the engine
    /// (`std::mem::transmute_copy` from a u64 reproduces the same
    /// FName).
    pub fn as_u64(self) -> u64 {
        ((self.number as u64) << 32) | (self.comparison_index as u32 as u64)
    }
}

type AppendStringFn = unsafe extern "system" fn(*const FName, *mut FString);

// SEH-protected call to AppendString. Defined in
// `ueforge/cpp/ueforge_seh.cpp`; wraps `fn(name, out)` in
// `__try`/`__except`. On AV inside the engine the helper
// returns 0 and the out FString is left at its default
// (we leave Rust callers responsible for the default init).
unsafe extern "C" {
    fn ueforge_seh_call_append_string(
        f: AppendStringFn,
        name: *const FName,
        out: *mut FString,
    ) -> i32;
}

pub struct NameResolver {
    append_string: AppendStringFn,
    cache: RwLock<HashMap<u64, Arc<str>>>,
}

impl NameResolver {
    pub unsafe fn new(image_base: usize, offsets: &PlatformOffsets) -> Self {
        let addr = image_base + offsets.append_string;
        let append_string: AppendStringFn = unsafe { std::mem::transmute(addr) };
        Self {
            append_string,
            // 8K starting capacity covers the average game's
            // hot-path name set without growing.
            cache: RwLock::new(HashMap::with_capacity(8192)),
        }
    }

    /// Cached resolve. Cache hit returns an `Arc<str>` ref-bump;
    /// miss runs AppendString once (leaks one FString buffer per
    /// unique FName) and stores the result.
    ///
    /// Defends against engine-side AVs at two levels:
    /// - `FName::is_plausible` rejects garbage indices before the
    ///   engine call (cheap).
    /// - `ueforge_seh_call_append_string` wraps the engine call in
    ///   `__try`/`__except`; an AV inside `AppendString` returns
    ///   a sentinel instead of taking the host.
    pub unsafe fn to_arc(&self, name: FName) -> Arc<str> {
        let key = name.as_u64();
        if let Some(s) = self.cache.read().get(&key) {
            return s.clone();
        }
        if !name.is_plausible() {
            let arc: Arc<str> = Arc::from("<bogus-fname>");
            self.cache.write().insert(key, arc.clone());
            return arc;
        }
        let mut out = FString::default();
        let ok = unsafe {
            ueforge_seh_call_append_string(
                self.append_string,
                &name as *const FName,
                &mut out as *mut FString,
            )
        };
        let arc: Arc<str> = if ok != 0 {
            Arc::from(out.as_string().into_boxed_str())
        } else {
            Arc::from("<seh-fail>")
        };
        self.cache.write().insert(key, arc.clone());
        arc
    }

    /// Owned `String` form. One alloc at the boundary; identical
    /// cost to the old API. Prefer [`Self::to_arc`] when the caller
    /// only needs a borrowed `&str`.
    pub unsafe fn to_string(&self, name: FName) -> String {
        unsafe { self.to_arc(name) }.to_string()
    }
}
