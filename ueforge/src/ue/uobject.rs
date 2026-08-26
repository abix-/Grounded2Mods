// UObject + UClass + UFunction wrappers.
//
// We model these as zero-sized newtypes over raw pointers (well, around the
// actual UObject memory in the game). Field access goes through `field()`
// helpers that read at known offsets. No Rust-level inheritance. Callers
// pass `&UObject` everywhere and downcast via `is_a()`.
//
// ## Universal SAFETY contract (all `unsafe` blocks in this module)
//
// This file implements the framework's primitive UE traversal
// surface. Every `unsafe { ... }` block falls into one of four
// well-defined shapes:
//
// 1. `(self.as_ptr().add(offset) as *const T).read_unaligned()`
//    / write variant: reads/writes a UE struct field at an offset
//    that the caller's `unsafe fn` contract requires to be a
//    valid `T` field on this UObject. read_unaligned is robust to
//    UE-side layouts that aren't pointer-aligned.
//
// 2. `&*(uobject as *const UObject as *const UClass)` and the
//    reverse via `.as_object()`: UClass / UFunction extend UObject
//    in memory layout (UE C++ inheritance), so the casts are
//    well-defined for objects that ARE a UClass / UFunction.
//    Callers gate via `is_uclass_meta` / `is_a` before casting.
//
// 3. `name_resolver.to_string(fname)` / `to_arc(fname)`: the
//    NameResolver is initialized at runtime detect; it handles
//    out-of-range FNames gracefully (returns a fallback string).
//    Marked `unsafe` because internal pointer math reaches into
//    GNames; safe given a valid runtime.
//
// 4. `process_event(func, parms)`: invokes the engine's
//    ProcessEvent on a live UObject with a live UFunction and a
//    parm-block whose layout matches the function's parm tuple.
//    Caller's `unsafe fn` contract owns the parm-layout
//    invariant.
//
// All four shapes are categorically safe given (1)-(4); we allow
// the workspace lint at the module level rather than repeating
// the same SAFETY comments 40 times across the same shapes.
#![allow(clippy::undocumented_unsafe_blocks)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;

use crate::ue::fname::{FName, NameResolver};
use crate::ue::offsets::{
    self, GObjectsLayout, PlatformOffsets, chunked_uobject_array, fuobject_item, tuobject_array,
    uclass,
};

/// One field on a UE class: the property's name, its byte
/// offset within an instance, and the size of one element. For
/// arrays / TArrays the element size is the per-element size,
/// not the total field size.
#[derive(Debug, Clone)]
pub struct NativeProperty {
    pub name: String,
    pub offset: u32,
    pub element_size: u32,
}

#[repr(transparent)]
pub struct UObject {
    _opaque: [u8; 0],
}

impl UObject {
    pub fn as_ptr(&self) -> *const u8 {
        self as *const UObject as *const u8
    }

    pub fn as_mut_ptr(&self) -> *mut u8 {
        self as *const UObject as *mut u8
    }

    pub unsafe fn read_field<T: Copy>(&self, offset: usize) -> T {
        unsafe { (self.as_ptr().add(offset) as *const T).read_unaligned() }
    }

    pub unsafe fn write_field<T: Copy>(&self, offset: usize, value: T) {
        unsafe { (self.as_mut_ptr().add(offset) as *mut T).write_unaligned(value) }
    }

    pub unsafe fn field_ptr(&self, offset: usize) -> *mut u8 {
        unsafe { self.as_mut_ptr().add(offset) }
    }

    pub fn class(&self) -> Option<&UClass> {
        unsafe {
            let p: *mut UClass = self.read_field(offsets::uobject::CLASS);
            p.as_ref()
        }
    }

    pub fn outer(&self) -> Option<&UObject> {
        unsafe {
            let p: *mut UObject = self.read_field(offsets::uobject::OUTER);
            p.as_ref()
        }
    }

    pub fn fname(&self) -> FName {
        unsafe { self.read_field(offsets::uobject::NAME) }
    }

    pub fn index(&self) -> i32 {
        unsafe { self.read_field(offsets::uobject::INDEX) }
    }

    /// "Default__" shows up in the FName for every CDO. Mirrors the C++
    /// IsDefaultObject implementation.
    ///
    /// If the runtime hasn't been initialized yet (caller fired before
    /// `init_runtime`), returns false. We prefer a soft fallback over a
    /// panic on the hot path. Panic = abort in release would crash the
    /// game over a recoverable race.
    pub fn is_default_object(&self) -> bool {
        let Some(rt) = try_runtime() else { return false };
        let name = unsafe { rt.name_resolver.to_arc(self.fname()) };
        name.starts_with("Default__")
    }

    /// Resolved short name. Soft-fallback to `"<unresolved>"` if the
    /// runtime hasn't been initialized. Same rationale as
    /// [`Self::is_default_object`].
    pub fn name(&self) -> String {
        match try_runtime() {
            Some(rt) => unsafe { rt.name_resolver.to_string(self.fname()) },
            None => String::from("<unresolved>"),
        }
    }

    pub fn full_name(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut cur = self.outer();
        while let Some(o) = cur {
            parts.push(o.name());
            cur = o.outer();
        }
        parts.reverse();
        let mut out = String::new();
        if let Some(cls) = self.class() {
            out.push_str(&cls.as_object().name());
            out.push(' ');
        }
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                out.push('.');
            }
            out.push_str(p);
        }
        if !parts.is_empty() {
            out.push('.');
        }
        out.push_str(&self.name());
        out
    }

    pub fn is_a(&self, target: &UClass) -> bool {
        let mut cur: Option<&UClass> = self.class();
        let mut depth = 0;
        while let Some(c) = cur {
            // Bound the walk: corrupt content occasionally has
            // super chains that loop on themselves. Real UE5
            // hierarchies cap at ~10 levels deep, so 64 is
            // generous + an effective sanity guard.
            if depth > 64 {
                return false;
            }
            if std::ptr::eq(c, target) {
                return true;
            }
            // Guard the super-class pointer read: an unmapped
            // page on the next link would AV the host.
            let super_addr =
                c as *const UClass as usize + offsets::ustruct::SUPER_STRUCT;
            if !crate::winproc::is_addr_readable(super_addr) {
                return false;
            }
            cur = c.super_class();
            depth += 1;
        }
        false
    }

    /// Calls vtable[ProcessEventIdx] with (self, function, parms).
    ///
    /// Returns silently if the runtime isn't initialized. We'd rather
    /// drop the engine call than abort the game on a setup race. The
    /// caller's parm buffer is left untouched.
    pub unsafe fn process_event(&self, function: &UFunction, parms: *mut c_void) {
        let Some(rt) = try_runtime() else {
            crate::log::log(format_args!(
                "ueforge: process_event called before init_runtime; dropping call"
            ));
            return;
        };
        unsafe {
            let vtable: *const *const c_void = self.read_field(offsets::uobject::VTABLE);
            let slot = *vtable.add(rt.platform_offsets.process_event_idx);
            let pe: ProcessEventFn = std::mem::transmute(slot);
            pe(self, function, parms);
        }
    }
}

pub type ProcessEventFn =
    unsafe extern "system" fn(this: *const UObject, function: *const UFunction, parms: *mut c_void);

#[repr(transparent)]
pub struct UClass {
    _opaque: [u8; 0],
}

impl UClass {
    pub fn as_object(&self) -> &UObject {
        unsafe { &*(self as *const UClass as *const UObject) }
    }

    pub fn class_default_object(&self) -> Option<&UObject> {
        unsafe {
            let p: *mut UObject = (self as *const UClass as *const u8)
                .add(uclass::CLASS_DEFAULT_OBJECT)
                .cast::<*mut UObject>()
                .read_unaligned();
            p.as_ref()
        }
    }

    /// Size in bytes of one instance of this class (UE
    /// `UStruct::PropertiesSize`). The address range
    /// `[obj_base, obj_base + properties_size)` is the memory
    /// owned by a UObject of this class.
    pub fn properties_size(&self) -> u32 {
        unsafe {
            (self as *const UClass as *const u8)
                .add(offsets::ustruct::SIZE)
                .cast::<u32>()
                .read_unaligned()
        }
    }

    /// Walk this class's ChildProperties chain (FField* threaded
    /// by Next), yielding `(name, offset_within_instance, element_size)`
    /// for every native property. Does NOT recurse into the
    /// super-class. Caller walks the super chain via
    /// [`Self::super_class`] if needed.
    ///
    /// Returns owned strings so the iterator can outlive a borrow
    /// on the class. Cheap enough for interactive lookups
    /// (each class has tens of properties typically).
    /// Cached variant of [`Self::iter_native_properties`]. UClasses
    /// live forever in GObjects, so the property list never changes
    /// after first read; caching keys on the UClass pointer.
    /// Subsequent calls share an `Arc<[NativeProperty]>` and avoid
    /// re-walking the FField chain + re-resolving every property
    /// FName.
    pub fn cached_native_properties(&self) -> Arc<[NativeProperty]> {
        let key = self as *const UClass as usize;
        let cache = property_cache();
        if let Some(p) = cache.read().get(&key) {
            return p.clone();
        }
        let fresh: Arc<[NativeProperty]> = self.iter_native_properties().into();
        cache.write().insert(key, fresh.clone());
        fresh
    }

    pub fn iter_native_properties(&self) -> Vec<NativeProperty> {
        let Some(rt) = try_runtime() else { return Vec::new() };
        let mut out = Vec::new();
        let mut cur: *const u8 = unsafe {
            (self as *const UClass as *const u8)
                .add(offsets::ustruct::CHILD_PROPERTIES)
                .cast::<*const u8>()
                .read_unaligned()
        };
        let mut depth = 0;
        while !cur.is_null() && depth < 4096 {
            unsafe {
                let name_ptr = cur.add(offsets::ffield::NAME_PRIVATE);
                let name_fname: crate::ue::fname::FName =
                    (name_ptr as *const crate::ue::fname::FName).read_unaligned();
                let name = if name_fname.is_none() {
                    String::from("<none>")
                } else {
                    rt.name_resolver.to_string(name_fname)
                };
                let offset = (cur.add(offsets::fproperty::OFFSET_INTERNAL) as *const i32)
                    .read_unaligned() as u32;
                let elem_size = (cur.add(offsets::fproperty::ELEMENT_SIZE) as *const i32)
                    .read_unaligned() as u32;
                let next: *const u8 = (cur.add(offsets::ffield::NEXT) as *const *const u8)
                    .read_unaligned();
                out.push(NativeProperty {
                    name,
                    offset,
                    element_size: elem_size,
                });
                cur = next;
            }
            depth += 1;
        }
        out
    }

    pub fn super_class(&self) -> Option<&UClass> {
        unsafe {
            let p: *mut UClass = (self as *const UClass as *const u8)
                .add(offsets::ustruct::SUPER_STRUCT)
                .cast::<*mut UClass>()
                .read_unaligned();
            p.as_ref()
        }
    }

    /// Walk the Children chain (UField list at `+0x48`) and emit
    /// every member's `(name, function_flags)`. UFunctions live
    /// in this chain alongside other UField-derived members; we
    /// emit them all and let callers filter by flags. Cost is
    /// bounded (chains in stock UE5 cap around ~100 entries per
    /// class); cold path / called at discovery-init time.
    ///
    /// VirtualQuery-guards each pointer dereference. Some UE5
    /// builds have UClasses whose Children chain points at
    /// freed / never-fully-constructed UField stubs. The eager
    /// discovery walk hits them all. Bailing on unreadable
    /// pointers keeps the walk going instead of crashing the host.
    pub fn iter_functions(&self) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        let head_addr = self as *const UClass as usize + offsets::ustruct::CHILDREN;
        if !crate::winproc::is_addr_readable(head_addr) {
            return out;
        }
        let mut cur: Option<&UObject> = unsafe {
            let p: *mut UObject = (head_addr as *const *mut UObject).read_unaligned();
            if !p.is_null() && crate::winproc::is_addr_readable(p as usize) {
                p.as_ref()
            } else {
                None
            }
        };
        let mut depth = 0;
        while let Some(field) = cur {
            if depth > 4096 {
                break;
            }
            let field_addr = field as *const UObject as usize;
            // Need to read FunctionFlags at +0xB0. The largest
            // offset we touch on a putative UFunction. Validate
            // that page is mapped before any of the field reads.
            if !crate::winproc::is_addr_readable(field_addr + offsets::ufunction::FUNCTION_FLAGS) {
                break;
            }
            let func = unsafe { &*(field as *const UObject as *const UFunction) };
            out.push((field.name(), func.function_flags()));
            let next_addr = field_addr + offsets::ufield::NEXT;
            if !crate::winproc::is_addr_readable(next_addr) {
                break;
            }
            cur = unsafe {
                let p: *mut UObject = (next_addr as *const *mut UObject).read_unaligned();
                if !p.is_null() && crate::winproc::is_addr_readable(p as usize) {
                    p.as_ref()
                } else {
                    None
                }
            };
            depth += 1;
        }
        out
    }

    /// Walk Children list looking for a UFunction whose name == `func_name`
    /// and whose outer chain contains an entry matching `class_name`.
    pub fn get_function(&self, class_name: &str, func_name: &str) -> Option<&UFunction> {
        let mut cur: Option<&UObject> = unsafe {
            let p: *mut UObject = (self as *const UClass as *const u8)
                .add(offsets::ustruct::CHILDREN)
                .cast::<*mut UObject>()
                .read_unaligned();
            p.as_ref()
        };
        while let Some(field) = cur {
            // UFunction inherits from UStruct inherits from UField. The
            // UField::Next chain at offset 0x28 gives us the sibling list.
            // We don't know up front whether `field` is a UFunction; we
            // match by name and let the caller's parms layout enforce
            // correctness.
            if field.name() == func_name {
                // Confirm it's owned (transitively) by `class_name`.
                let mut outer = field.outer();
                while let Some(o) = outer {
                    if o.name() == class_name {
                        return unsafe { Some(&*(field as *const UObject as *const UFunction)) };
                    }
                    outer = o.outer();
                }
            }
            cur = unsafe {
                let p: *mut UObject = field
                    .as_ptr()
                    .add(offsets::ufield::NEXT)
                    .cast::<*mut UObject>()
                    .read_unaligned();
                p.as_ref()
            };
        }
        None
    }
}

#[repr(transparent)]
pub struct UFunction {
    _opaque: [u8; 0],
}

impl UFunction {
    pub fn as_object(&self) -> &UObject {
        unsafe { &*(self as *const UFunction as *const UObject) }
    }

    pub fn function_flags(&self) -> u32 {
        unsafe {
            (self as *const UFunction as *const u8)
                .add(offsets::ufunction::FUNCTION_FLAGS)
                .cast::<u32>()
                .read_unaligned()
        }
    }

    /// How many bytes ProcessEvent reads and writes through the
    /// parm pointer. A caller must hand it a buffer at least
    /// this large.
    pub fn parms_size(&self) -> u16 {
        unsafe {
            (self as *const UFunction as *const u8)
                .add(offsets::ufunction::PARMS_SIZE)
                .cast::<u16>()
                .read_unaligned()
        }
    }

    /// Number of declared parameters, return value included.
    pub fn num_parms(&self) -> u8 {
        unsafe {
            (self as *const UFunction as *const u8)
                .add(offsets::ufunction::NUM_PARMS)
                .read_unaligned()
        }
    }

    pub fn set_function_flags(&self, flags: u32) {
        unsafe {
            (self as *const UFunction as *mut u8)
                .add(offsets::ufunction::FUNCTION_FLAGS)
                .cast::<u32>()
                .write_unaligned(flags);
        }
    }
}

// ---- GObjects ------------------------------------------------------------
//
// TUObjectArrayWrapper at image_base + offsets.g_objects holds a pointer to
// a TUObjectArray { Objects, MaxElements, NumElements }. Each element is an
// FUObjectItem { Object*, pad }.

pub struct GObjectsView {
    /// Address of the start of the inner array struct
    /// (FFixedUObjectArray for FlatFixed, FChunkedFixedUObjectArray
    /// for WrappedChunked).
    inner: *const u8,
    layout: GObjectsLayout,
}

impl GObjectsView {
    pub unsafe fn from_image(image_base: usize, offsets: &PlatformOffsets) -> Self {
        let g_objects = (image_base + offsets.g_objects) as *const u8;
        let inner = match offsets.g_objects_layout {
            GObjectsLayout::FlatFixed => g_objects,
            GObjectsLayout::WrappedChunked => unsafe {
                g_objects.add(chunked_uobject_array::OBJ_OBJECTS)
            },
        };
        Self {
            inner,
            layout: offsets.g_objects_layout,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.inner.is_null()
    }

    pub fn num(&self) -> i32 {
        if self.inner.is_null() {
            return 0;
        }
        let off = match self.layout {
            GObjectsLayout::FlatFixed => tuobject_array::NUM_ELEMENTS,
            GObjectsLayout::WrappedChunked => chunked_uobject_array::NUM_ELEMENTS,
        };
        unsafe { self.inner.add(off).cast::<i32>().read_unaligned() }
    }

    pub fn get(&self, index: i32) -> Option<&UObject> {
        if index < 0 || index >= self.num() {
            return None;
        }
        let item = match self.layout {
            GObjectsLayout::FlatFixed => unsafe {
                let objects: *const u8 = self
                    .inner
                    .add(tuobject_array::OBJECTS)
                    .cast::<*const u8>()
                    .read_unaligned();
                objects.add(index as usize * fuobject_item::SIZE)
            },
            GObjectsLayout::WrappedChunked => unsafe {
                // Objects[chunk_idx] is a pointer to a chunk, each
                // chunk is NUM_ELEMENTS_PER_CHUNK FUObjectItems.
                let chunks: *const *const u8 = self
                    .inner
                    .add(chunked_uobject_array::OBJECTS)
                    .cast::<*const *const u8>()
                    .read_unaligned();
                let chunk_idx = index as usize / chunked_uobject_array::NUM_ELEMENTS_PER_CHUNK;
                let in_chunk = index as usize % chunked_uobject_array::NUM_ELEMENTS_PER_CHUNK;
                let chunk: *const u8 = chunks.add(chunk_idx).read_unaligned();
                if chunk.is_null() {
                    return None;
                }
                chunk.add(in_chunk * fuobject_item::SIZE)
            },
        };
        unsafe {
            let obj: *const UObject = item
                .add(fuobject_item::OBJECT)
                .cast::<*const UObject>()
                .read_unaligned();
            obj.as_ref()
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &UObject> + '_ {
        (0..self.num()).filter_map(|i| self.get(i))
    }
}

// ---- Runtime singleton ---------------------------------------------------
//
// Captures image base + platform offsets + name resolver once at startup.
// Every UObject helper that needs offsets pulls from here. Initialized via
// `init_runtime` from the worker thread.

pub struct Runtime {
    pub image_base: usize,
    pub platform_offsets: &'static PlatformOffsets,
    pub name_resolver: NameResolver,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn runtime() -> &'static Runtime {
    RUNTIME.get().expect("sdk runtime not initialized")
}

pub fn try_runtime() -> Option<&'static Runtime> {
    RUNTIME.get()
}

/// One-shot. Subsequent calls are ignored.
pub unsafe fn init_runtime(
    image_base: usize,
    platform_offsets: &'static PlatformOffsets,
) -> &'static Runtime {
    let resolver = unsafe { NameResolver::new(image_base, platform_offsets) };
    let _ = RUNTIME.set(Runtime {
        image_base,
        platform_offsets,
        name_resolver: resolver,
    });
    runtime()
}

/// Walks GObjects looking for a UClass with the given short name. The
/// meta-class filter accepts native `Class` *and* its subclasses
/// (BlueprintGeneratedClass, WidgetBlueprintGeneratedClass, etc.) so
/// Blueprint-generated classes are returned too.
///
/// Result is cached by name. UClasses are stable in GObjects for the
/// process lifetime, so a hit is permanent. Misses (class not yet
/// loaded by the engine) are NOT cached. A later call after the
/// class loads will walk again and find it.
pub fn find_class_fast(name: &str) -> Option<&'static UClass> {
    let rt = try_runtime()?;
    let cache = class_cache();
    if let Some(c) = cache.read().get(name) {
        return Some(*c);
    }
    let view = unsafe { GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return None;
    }
    for obj in view.iter() {
        if obj.name() != name {
            continue;
        }
        if !is_uclass_meta(obj.class()) {
            continue;
        }
        let class: &'static UClass = unsafe { &*(obj as *const UObject as *const UClass) };
        cache.write().insert(name.to_string(), class);
        return Some(class);
    }
    None
}

// ---- Caches --------------------------------------------------------------

static CLASS_CACHE: OnceLock<RwLock<HashMap<String, &'static UClass>>> = OnceLock::new();
fn class_cache() -> &'static RwLock<HashMap<String, &'static UClass>> {
    CLASS_CACHE.get_or_init(|| RwLock::new(HashMap::with_capacity(256)))
}

/// Per-UClass property list cache. Key is the UClass pointer cast
/// to usize. UClasses are stable in GObjects, so this is safe.
static PROPERTY_CACHE: OnceLock<RwLock<HashMap<usize, Arc<[NativeProperty]>>>> = OnceLock::new();
fn property_cache() -> &'static RwLock<HashMap<usize, Arc<[NativeProperty]>>> {
    PROPERTY_CACHE.get_or_init(|| RwLock::new(HashMap::with_capacity(256)))
}

fn is_uclass_meta(meta: Option<&UClass>) -> bool {
    let mut cur = meta;
    let mut depth = 0;
    while let Some(c) = cur {
        if depth > 16 {
            return false;
        }
        if c.as_object().name() == "Class" {
            return true;
        }
        cur = c.super_class();
        depth += 1;
    }
    false
}
