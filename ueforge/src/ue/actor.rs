//! Actor / controller helpers shared across UE5 mods.
//!
//! These wrap UE5's universal class-chain layout and the
//! `AController.Pawn` slot at a stable offset that doesn't move
//! between Engine versions.

use crate::ue::uobject::NativeProperty;
use crate::ue::{self, ClassRef, UClass, UObject};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::Duration;

static GAMEPLAY_STATICS: ClassRef = ClassRef::new("GameplayStatics");
static ACTOR_CLASS: ClassRef = ClassRef::new("Actor");
static GET_ALL_ACTORS_LAYOUT: OnceLock<Result<ActorEnumerationLayout, String>> = OnceLock::new();
static GET_COMPONENT_LAYOUT: OnceLock<Result<ComponentLookupLayout, String>> = OnceLock::new();
static GET_COMPONENTS_LAYOUT: OnceLock<Result<ComponentEnumerationLayout, String>> =
    OnceLock::new();
static ACTOR_OUTPUT: Mutex<ActorOutputBuffer> = Mutex::new(ActorOutputBuffer { data: 0, max: 0 });
static COMPONENT_OUTPUT: Mutex<ActorOutputBuffer> =
    Mutex::new(ActorOutputBuffer { data: 0, max: 0 });

const INITIAL_ACTOR_CAPACITY: i32 = 4096;
const MAX_ACTOR_RESULTS: i32 = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActorEnumerationLayout {
    parms_size: usize,
    world_context: usize,
    actor_class: usize,
    out_actors: usize,
}

impl ActorEnumerationLayout {
    fn from_properties(parms_size: usize, properties: &[NativeProperty]) -> Result<Self, String> {
        let offset = |name: &str| {
            properties
                .iter()
                .find(|property| property.name == name)
                .map(|property| property.offset as usize)
                .ok_or_else(|| format!("GetAllActorsOfClass has no {name} parameter"))
        };
        let layout = Self {
            parms_size,
            world_context: offset("WorldContextObject")?,
            actor_class: offset("ActorClass")?,
            out_actors: offset("OutActors")?,
        };
        if layout.world_context + 8 > parms_size
            || layout.actor_class + 8 > parms_size
            || layout.out_actors + 16 > parms_size
        {
            return Err("GetAllActorsOfClass parameter layout exceeds ParmsSize".into());
        }
        Ok(layout)
    }
}

struct ActorOutputBuffer {
    data: usize,
    max: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ComponentLookupLayout {
    parms_size: usize,
    component_class: usize,
    return_value: usize,
}

impl ComponentLookupLayout {
    fn from_properties(parms_size: usize, properties: &[NativeProperty]) -> Result<Self, String> {
        let offset = |name: &str| {
            properties
                .iter()
                .find(|property| property.name == name)
                .map(|property| property.offset as usize)
                .ok_or_else(|| format!("GetComponentByClass has no {name} parameter"))
        };
        let layout = Self {
            parms_size,
            component_class: offset("ComponentClass")?,
            return_value: offset("ReturnValue")?,
        };
        if layout.component_class + 8 > parms_size || layout.return_value + 8 > parms_size {
            return Err("GetComponentByClass parameter layout exceeds ParmsSize".into());
        }
        Ok(layout)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ComponentEnumerationLayout {
    parms_size: usize,
    component_class: usize,
    return_value: usize,
}

impl ComponentEnumerationLayout {
    fn from_properties(parms_size: usize, properties: &[NativeProperty]) -> Result<Self, String> {
        let offset = |name: &str| {
            properties
                .iter()
                .find(|property| property.name == name)
                .map(|property| property.offset as usize)
                .ok_or_else(|| format!("K2_GetComponentsByClass has no {name} parameter"))
        };
        let layout = Self {
            parms_size,
            component_class: offset("ComponentClass")?,
            return_value: offset("ReturnValue")?,
        };
        if layout.component_class + 8 > parms_size || layout.return_value + 16 > parms_size {
            return Err("K2_GetComponentsByClass parameter layout exceeds ParmsSize".into());
        }
        Ok(layout)
    }
}

impl ActorOutputBuffer {
    fn ensure_allocated(&mut self) -> Result<(), String> {
        if self.data != 0 {
            return Ok(());
        }
        let bytes = INITIAL_ACTOR_CAPACITY as usize * std::mem::size_of::<*const UObject>();
        self.data = ue::gmalloc::alloc_zeroed(bytes, ue::gmalloc::DEFAULT_ALIGNMENT)
            .ok_or("GetAllActorsOfClass could not allocate its retained output buffer")?
            as usize;
        self.max = INITIAL_ACTOR_CAPACITY;
        Ok(())
    }
}

/// Ask Unreal for every live actor derived from `actor_class`.
///
/// This calls `UGameplayStatics::GetAllActorsOfClass`, which uses
/// the world's actor collections. It does not walk GObjects. The
/// output buffer is allocated once from Unreal's allocator and
/// retained for the process, so repeated calls neither allocate
/// nor leak one buffer per query.
///
/// Game thread only. Returned pointers are valid only for the
/// current world and must be discarded when that world ends.
pub fn actors_of_class(
    world_context: &UObject,
    actor_class: &UClass,
) -> Result<Vec<*const UObject>, String> {
    let _measurement = modforge::counters::measure("ue:actors_of_class");
    let function = GAMEPLAY_STATICS
        .find_function("GetAllActorsOfClass")
        .ok_or("GameplayStatics::GetAllActorsOfClass is not loaded")?;
    let layout = GET_ALL_ACTORS_LAYOUT.get_or_init(|| {
        ActorEnumerationLayout::from_properties(
            function.parms_size() as usize,
            &function.iter_parameters(),
        )
    });
    let layout = layout.as_ref().map_err(Clone::clone)?;
    let cdo = GAMEPLAY_STATICS
        .cdo()
        .ok_or("GameplayStatics class default object is not loaded")?;
    let mut output = ACTOR_OUTPUT.lock();
    output.ensure_allocated()?;
    let mut parms = vec![0u8; layout.parms_size];
    parms[layout.world_context..layout.world_context + 8]
        .copy_from_slice(&(world_context as *const UObject as u64).to_le_bytes());
    parms[layout.actor_class..layout.actor_class + 8]
        .copy_from_slice(&(actor_class as *const UClass as u64).to_le_bytes());
    parms[layout.out_actors..layout.out_actors + 8]
        .copy_from_slice(&(output.data as u64).to_le_bytes());
    parms[layout.out_actors + 12..layout.out_actors + 16]
        .copy_from_slice(&output.max.to_le_bytes());

    // SAFETY: the function and CDO are cached live engine objects;
    // the reflected offsets were validated inside ParmsSize; the
    // output TArray uses the engine allocator; caller guarantees the
    // game thread and a live world-context actor.
    unsafe {
        cdo.process_event(function, parms.as_mut_ptr() as *mut c_void);
    }

    let data = u64::from_le_bytes(
        parms[layout.out_actors..layout.out_actors + 8]
            .try_into()
            .expect("eight-byte actor output pointer"),
    ) as usize;
    let num = i32::from_le_bytes(
        parms[layout.out_actors + 8..layout.out_actors + 12]
            .try_into()
            .expect("four-byte actor output count"),
    );
    let max = i32::from_le_bytes(
        parms[layout.out_actors + 12..layout.out_actors + 16]
            .try_into()
            .expect("four-byte actor output capacity"),
    );
    if data == 0 || num < 0 || max < num || max > MAX_ACTOR_RESULTS {
        return Err(format!(
            "GetAllActorsOfClass returned corrupt TArray data={data:#x} num={num} max={max}"
        ));
    }
    output.data = data;
    output.max = max;
    // SAFETY: the validated TArray reports `num` initialized actor
    // pointers inside its engine-allocated `max` capacity.
    let actors = unsafe { std::slice::from_raw_parts(data as *const *const UObject, num as usize) };
    Ok(actors
        .iter()
        .copied()
        .filter(|actor| !actor.is_null())
        .collect())
}

/// Return the first component on `actor` derived from
/// `component_class`, through Unreal's own actor API.
///
/// No object search. Game thread only, and the returned pointer
/// belongs to the current world.
pub fn component_by_class(
    actor: &UObject,
    component_class: &UClass,
) -> Result<Option<*const UObject>, String> {
    let _measurement = modforge::counters::measure("ue:component_by_class");
    let function = ACTOR_CLASS
        .find_function("GetComponentByClass")
        .ok_or("Actor::GetComponentByClass is not loaded")?;
    let layout = GET_COMPONENT_LAYOUT.get_or_init(|| {
        ComponentLookupLayout::from_properties(
            function.parms_size() as usize,
            &function.iter_parameters(),
        )
    });
    let layout = layout.as_ref().map_err(Clone::clone)?;
    let mut parms = vec![0u8; layout.parms_size];
    parms[layout.component_class..layout.component_class + 8]
        .copy_from_slice(&(component_class as *const UClass as u64).to_le_bytes());
    // SAFETY: `actor` and `component_class` are live engine
    // objects; reflected offsets are validated within ParmsSize;
    // caller guarantees the game thread.
    unsafe {
        actor.process_event(function, parms.as_mut_ptr() as *mut c_void);
    }
    let result = u64::from_le_bytes(
        parms[layout.return_value..layout.return_value + 8]
            .try_into()
            .expect("eight-byte component return pointer"),
    );
    Ok((result != 0).then_some(result as *const UObject))
}

/// Return every component on `actor` derived from
/// `component_class`, through Unreal's own actor API.
///
/// No object search. The retained output buffer is reused for the
/// process. Game thread only, and returned pointers belong to the
/// current world.
pub fn components_by_class(
    actor: &UObject,
    component_class: &UClass,
) -> Result<Vec<*const UObject>, String> {
    let _measurement = modforge::counters::measure("ue:components_by_class");
    let function = ACTOR_CLASS
        .find_function("K2_GetComponentsByClass")
        .ok_or("Actor::K2_GetComponentsByClass is not loaded")?;
    let layout = GET_COMPONENTS_LAYOUT.get_or_init(|| {
        ComponentEnumerationLayout::from_properties(
            function.parms_size() as usize,
            &function.iter_parameters(),
        )
    });
    let layout = layout.as_ref().map_err(Clone::clone)?;
    let mut output = COMPONENT_OUTPUT.lock();
    output.ensure_allocated()?;
    let mut parms = vec![0u8; layout.parms_size];
    parms[layout.component_class..layout.component_class + 8]
        .copy_from_slice(&(component_class as *const UClass as u64).to_le_bytes());
    parms[layout.return_value..layout.return_value + 8]
        .copy_from_slice(&(output.data as u64).to_le_bytes());
    parms[layout.return_value + 12..layout.return_value + 16]
        .copy_from_slice(&output.max.to_le_bytes());
    // SAFETY: `actor` and `component_class` are live engine
    // objects; the reflected offsets fit ParmsSize; the output
    // TArray uses Unreal's allocator; caller guarantees game thread.
    unsafe {
        actor.process_event(function, parms.as_mut_ptr() as *mut c_void);
    }
    let data = u64::from_le_bytes(
        parms[layout.return_value..layout.return_value + 8]
            .try_into()
            .expect("eight-byte component output pointer"),
    ) as usize;
    let num = i32::from_le_bytes(
        parms[layout.return_value + 8..layout.return_value + 12]
            .try_into()
            .expect("four-byte component output count"),
    );
    let max = i32::from_le_bytes(
        parms[layout.return_value + 12..layout.return_value + 16]
            .try_into()
            .expect("four-byte component output capacity"),
    );
    if data == 0 || num < 0 || max < num || max > MAX_ACTOR_RESULTS {
        return Err(format!(
            "K2_GetComponentsByClass returned corrupt TArray data={data:#x} num={num} max={max}"
        ));
    }
    output.data = data;
    output.max = max;
    // SAFETY: the validated TArray reports `num` initialized
    // component pointers within its engine-allocated capacity.
    let components =
        unsafe { std::slice::from_raw_parts(data as *const *const UObject, num as usize) };
    Ok(components
        .iter()
        .copied()
        .filter(|component| !component.is_null())
        .collect())
}

/// `AController.Pawn` byte offset (Engine_classes.hpp:30510).
/// Stable UE5 layout, valid for any game built on Engine 5.x.
pub const A_CONTROLLER_PAWN_OFFSET: usize = 0x0308;

/// Walk `obj`'s class chain (its UClass + all ancestors via
/// `super_class()`) and return `true` if any class name contains
/// `needle`. Bounded depth (32) so a corrupted super-chain can't
/// loop forever.
///
/// ```ignore
/// if class_chain_contains(controller, "PlayerController") {
///     // ...
/// }
/// ```
pub fn class_chain_contains(obj: &UObject, needle: &str) -> bool {
    let Some(cls) = obj.class() else { return false };
    let mut cur: Option<&UClass> = Some(cls);
    let mut depth = 0;
    while let Some(c) = cur {
        if depth > 32 {
            return false;
        }
        if c.as_object().name().contains(needle) {
            return true;
        }
        cur = c.super_class();
        depth += 1;
    }
    false
}

/// Read the `AController.Pawn` slot from a controller. Returns
/// `None` if the controller has no possessed pawn.
pub fn controller_pawn(controller: &UObject) -> Option<&UObject> {
    unsafe {
        let p: *mut UObject = controller
            .field_ptr(A_CONTROLLER_PAWN_OFFSET)
            .cast::<*mut UObject>()
            .read_unaligned();
        p.as_ref()
    }
}

/// `true` if `this`'s outer's full name contains `needle`.
/// Common pattern: filter PE-hook fires by component owner
/// (`is_outer_named(hc, "BP_SurvivalPlayerCharacter")`).
pub fn is_outer_named(this: &UObject, needle: &str) -> bool {
    this.outer()
        .map(|o| o.full_name().contains(needle))
        .unwrap_or(false)
}

/// Class name of `this`'s outer, if any.
pub fn outer_class_name(this: &UObject) -> Option<String> {
    this.outer()
        .and_then(|o| o.class())
        .map(|c| c.as_object().name())
}

/// `"<name>(<class-name>)"` describing an object for log lines.
/// `None` becomes `"<none>"`.
pub fn describe(obj: Option<&UObject>) -> String {
    match obj {
        None => "<none>".to_string(),
        Some(o) => {
            let cls = o.class().map(|c| c.as_object().name()).unwrap_or_default();
            format!("{}({})", o.name(), cls)
        }
    }
}

/// Find a non-CDO instance whose class name matches
/// `class_name` and whose name contains `name_filter` (if
/// provided). Only returns objects in a PersistentLevel
/// (live world actors, not editor or CDO copies).
pub fn find_actor(class_name: &str, name_filter: Option<&str>) -> Option<*const u8> {
    find_object(class_name, name_filter, true)
}

/// Find a live non-CDO object by exact class name. The optional
/// filter matches the object's short name. Callers must use the
/// returned reference only on the game thread and must not retain
/// it across an unload.
pub fn find_live_object(
    class_name: &str,
    name_filter: Option<&str>,
    require_level: bool,
) -> Option<&'static UObject> {
    let _m = modforge::counters::measure("ue:find_object");
    let rt = ue::try_runtime()?;
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return None;
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        let class = obj.class()?;
        if class.as_object().name() != class_name {
            continue;
        }
        if let Some(filter) = name_filter {
            if !obj.name().contains(filter) {
                continue;
            }
        }
        if require_level && !obj.full_name().contains("PersistentLevel") {
            continue;
        }
        // SAFETY: UObject storage is owned by the engine's global
        // object array. The public contract forbids retaining the
        // reference across an unload.
        return Some(unsafe { &*(obj as *const UObject) });
    }
    None
}

/// One actor, found on first use and remembered until the world
/// ends.
///
/// [`find_actor`] reads EVERY UObject the game has loaded.
/// Measured in MISERY 2026-08-26: 174,000 to 230,000 objects, and
/// 20 ms to stop at the first match. On the game thread, so more
/// than a frame.
///
/// Most of what a mod looks for is one actor that lives for the
/// whole session and never moves: the player, a manager. Looking
/// for it twice pays twice for the same answer, and MISERY had
/// five separate searches, three of them hunting the same player.
///
/// **Deliberately not a timer.** Re-reading on a clock is polling
/// with a longer gap: every interval is a guess, and the right
/// interval for something that changes on an event is never. The
/// only event that invalidates the pointer is the world ending,
/// and [`on_each_load`] already notices that for the price of a
/// cached pointer and an array length.
///
/// ```ignore
/// static PLAYER: LiveActor = LiveActor::new("BP_SGKMasterCharacter_C");
/// let Some(player) = PLAYER.get() else { return };
/// ```
pub struct LiveActor {
    class: &'static str,
    /// Whether the object has to be in a level. An actor does; a
    /// game instance or a widget does not.
    require_level: bool,
    /// The actor, or 0 for "not found yet".
    addr: std::sync::atomic::AtomicUsize,
    /// Whether this one is in [`ALL_LIVE`] yet.
    listed: std::sync::atomic::AtomicUsize,
}

/// Every [`LiveActor`] that has been used, so one call drops all
/// of them when the world ends.
static ALL_LIVE: parking_lot::Mutex<Vec<&'static LiveActor>> = parking_lot::Mutex::new(Vec::new());

impl LiveActor {
    /// An actor: something living in a level.
    pub const fn new(class: &'static str) -> Self {
        Self {
            class,
            require_level: true,
            addr: std::sync::atomic::AtomicUsize::new(0),
            listed: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// An object that is NOT in a level: a game instance, a
    /// widget, a manager that outlives the world. Found and kept
    /// the same way.
    pub const fn anywhere(class: &'static str) -> Self {
        Self {
            class,
            require_level: false,
            addr: std::sync::atomic::AtomicUsize::new(0),
            listed: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// The actor, searching only if it is not already remembered.
    ///
    /// Game thread only. Do not hold the reference across a level
    /// load; call again instead, which is free.
    pub fn get(&'static self) -> Option<&'static UObject> {
        use std::sync::atomic::Ordering;
        if self.listed.swap(1, Ordering::AcqRel) == 0 {
            ALL_LIVE.lock().push(self);
        }
        let addr = self.addr.load(Ordering::Acquire);
        if addr != 0 {
            // SAFETY: stored below from a GObjects search, and
            // cleared by `forget_all` when the world that owned it
            // ended.
            return Some(unsafe { &*(addr as *const UObject) });
        }
        let found = find_live_object(self.class, None, self.require_level)?;
        self.addr
            .store(found as *const UObject as usize, Ordering::Release);
        Some(found)
    }

    /// The same as a raw pointer, for callers that take one.
    pub fn ptr(&'static self) -> Option<*const u8> {
        self.get().map(|o| o as *const UObject as *const u8)
    }

    /// Return the actor already retained for the current world without searching.
    pub fn retained(&'static self) -> Option<&'static UObject> {
        let addr = self.addr.load(std::sync::atomic::Ordering::Acquire);
        (addr != 0).then(|| {
            // SAFETY: `addr` is set only from a live GObjects result and is
            // cleared by `forget_all` when its owning world ends.
            unsafe { &*(addr as *const UObject) }
        })
    }

    /// Forget it, so the next `get` searches again.
    pub fn forget(&self) {
        self.addr.store(0, std::sync::atomic::Ordering::Release);
    }

    pub fn is_held(&self) -> bool {
        self.addr.load(std::sync::atomic::Ordering::Acquire) != 0
    }
}

/// Forget every remembered actor.
///
/// Called when the world ends, because that is the only thing
/// that makes the pointers stale.
pub fn forget_all() {
    for a in ALL_LIVE.lock().iter() {
        a.forget();
    }
    // Anything else read out of that world is stale too.
    modforge::read_once::forget_all();
    WORLD_COUNT.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// How many worlds have ended.
///
/// For anything holding state read out of a world that does not
/// want to register a callback: remember this number alongside
/// the state, and when it differs, what you are holding came from
/// a world that no longer exists.
static WORLD_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Which world we are on. See [`WORLD_COUNT`].
pub fn world_generation() -> u64 {
    WORLD_COUNT.load(std::sync::atomic::Ordering::Acquire)
}

/// How many actors are remembered right now. For diagnostics.
pub fn held_count() -> usize {
    ALL_LIVE.lock().iter().filter(|a| a.is_held()).count()
}

/// Find a non-CDO instance by class name. When
/// `require_level` is false, matches any non-CDO instance
/// (useful for widgets and other non-actor objects).
pub fn find_object(
    class_name: &str,
    name_filter: Option<&str>,
    require_level: bool,
) -> Option<*const u8> {
    find_live_object(class_name, name_filter, require_level).map(UObject::as_ptr)
}

/// Find the first non-CDO object in `/Engine/Transient` whose
/// class chain contains `class_needle`. The optional filter
/// matches the full object name so callers can distinguish
/// Blueprint widget instances that share a class.
///
/// Callers must use the returned reference only on the game
/// thread and must not retain it across an unload.
pub fn find_transient_object(
    class_needle: &str,
    full_name_filter: Option<&str>,
) -> Option<&'static UObject> {
    find_objects_by_chain(class_needle)
        .into_iter()
        .find_map(|ptr| {
            // SAFETY: ptr came from this call's GObjects walk and
            // is consumed immediately on the game thread.
            let obj = unsafe { &*(ptr as *const UObject) };
            let full_name = obj.full_name();
            if !full_name.contains("/Engine/Transient") {
                return None;
            }
            if let Some(filter) = full_name_filter {
                if !full_name.contains(filter) {
                    return None;
                }
            }
            Some(obj)
        })
}

/// Find all live world actors whose class chain (own class or
/// any ancestor) contains `class_needle`. Unlike `find_actor`,
/// this matches subclasses, so a Blueprint base class like
/// `BP_MasterVendorBuildPart_C` finds every derived vendor.
/// Skips CDOs and objects outside a PersistentLevel.
pub fn find_actors_by_chain(class_needle: &str) -> Vec<*const u8> {
    // Nests the search below, so their times overlap in the
    // report. The gap between them is the cost of building a full
    // object path for every hit just to test it for one word.
    let _m = modforge::counters::measure("ue:find_actors_by_chain");
    find_objects_by_chain(class_needle)
        .into_iter()
        .filter(|p| {
            // SAFETY: p came from find_objects_by_chain's own
            // GObjects iteration.
            let obj = unsafe { &*(*p as *const UObject) };
            obj.full_name().contains("PersistentLevel")
        })
        .collect()
}

/// Count the live actors of a class, grouped by the level that
/// owns them.
///
/// The shape every "what is in each streamed region" question
/// takes. `class_needle` matches the class chain, so a Blueprint
/// base class counts every subclass. `in_package` keeps only
/// levels whose path contains it, which is how a game excludes
/// its hub, its menu level, or anything it spawned itself.
///
/// This IS a full object search, so call it when a region has
/// actually streamed in, never on a timer. See
/// [`super::streaming`] for the cheap way to know that.
///
/// Game thread only.
pub fn count_by_level(
    class_needle: &str,
    in_package: Option<&str>,
) -> std::collections::HashMap<String, usize> {
    let mut out: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for p in find_actors_by_chain(class_needle) {
        // SAFETY: p came from that call's own GObjects iteration.
        let obj = unsafe { &*(p as *const UObject) };
        // `full_name` builds a String, so it has to outlive the
        // borrow `level_of` takes out of it.
        let full = obj.full_name();
        let Some(level) = level_of(&full) else {
            continue;
        };
        if let Some(pkg) = in_package {
            if !level.contains(pkg) {
                continue;
            }
        }
        *out.entry(level.to_string()).or_default() += 1;
    }
    out
}

/// The last segment of a path, for a log line. `/a/b/c` is `c`.
pub fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The object path out of a full name.
///
/// A UE full name is `"ClassName /Game/path/Level.PersistentLevel.Actor"`.
/// The class prefix MUST be stripped before the path is used as a
/// key, or every class becomes its own entry. That exact bug
/// over-spawned NPCs in MISERY until it was found.
pub fn path_of(full_name: &str) -> Option<&str> {
    full_name.split(' ').nth(1)
}

/// The level that owns an actor, from its full name: everything
/// before `.PersistentLevel`.
///
/// This is how streamed levels are told apart at runtime. In a
/// world built from streamed tiles it is the tile the actor
/// belongs to.
pub fn level_of(full_name: &str) -> Option<&str> {
    path_of(full_name)?.split(".PersistentLevel").next()
}

/// True when a full name names an ACTOR in a level rather than
/// one of its components.
///
/// Components live under the actor and so carry a further dot in
/// the tail after `.PersistentLevel.`.
pub fn is_level_actor(full_name: &str) -> bool {
    match full_name.split(".PersistentLevel.").nth(1) {
        Some(tail) => !tail.contains('.'),
        None => false,
    }
}

/// Any live actor in a loaded level.
///
/// Engine functions that take a `WorldContextObject` only use it
/// to find which world they are acting on, so for anything
/// world-wide (a trace, a spawn) ANY actor will do and there is
/// no reason for a caller to name a game's player class.
///
/// Returns `None` when no level is loaded, which is the honest
/// answer at a main menu.
pub fn any_world_actor() -> Option<*const u8> {
    let rt = ue::try_runtime()?;
    // SAFETY: rt came from try_runtime; the view is built from
    // the validated image base + offsets.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return None;
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        if is_level_actor(&obj.full_name()) {
            return Some(obj.as_ptr());
        }
    }
    None
}

/// Every actor (not component) in a level whose path contains
/// `path_needle`, as (class name, pointer).
///
/// Use this to sweep one streamed level or one group of them,
/// where [`find_actors_by_chain`] answers "every actor of a
/// kind" instead.
pub fn actors_in_levels(path_needle: &str) -> Vec<(String, *const u8)> {
    let mut out = Vec::new();
    let Some(rt) = ue::try_runtime() else {
        return out;
    };
    // SAFETY: rt came from try_runtime; the view is built from
    // the validated image base + offsets.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return out;
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        let full = obj.full_name();
        if !full.contains(path_needle) || !is_level_actor(&full) {
            continue;
        }
        let class = obj
            .class()
            .map(|c| c.as_object().name())
            .unwrap_or_default();
        out.push((class, obj.as_ptr()));
    }
    out
}

/// Every live object whose class chain contains `class_needle`,
/// with no level requirement. Widgets and the game instance are
/// not actors and live outside any PersistentLevel, so
/// `find_actors_by_chain` cannot see them.
pub fn find_objects_by_chain(class_needle: &str) -> Vec<*const u8> {
    let _m = modforge::counters::measure("ue:find_objects_by_chain");
    let mut found = Vec::new();
    let Some(rt) = ue::try_runtime() else {
        return found;
    };
    // SAFETY: rt came from try_runtime(); image_base + offsets
    // are what runtime init validated.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return found;
    }
    let mut read = 0u64;
    for obj in view.iter() {
        read += 1;
        if obj.is_default_object() {
            continue;
        }
        if !class_chain_contains(obj, class_needle) {
            continue;
        }
        found.push(obj.as_ptr());
    }
    // How many objects one search reads. The count matters as
    // much as the time: it says whether a search is expensive
    // because the list is huge or because it is called often.
    modforge::counters::tally("ue:objects_read", read);
    found
}

/// Spawn a background thread that calls `on_load` each time a
/// finder function returns `Some`. The finder is polled every
/// `poll_interval`. After `on_load` runs, the thread watches
/// for the world to go away, then re-polls and re-applies on the
/// next load.
///
/// The finder is EXPENSIVE: it searches the object list, which is
/// 100 ms and 185,000 objects in a streamed-in world. So it is
/// called only while actually hunting for the thing. Waiting for
/// a world, and waiting for one to end, both ask
/// [`crate::ue::streaming::world_is_up`] instead, which is a
/// cached pointer and an array length.
///
/// A mod that has not called `streaming::register` gets the old
/// behaviour, the finder run on every poll forever, because that
/// is all it has given us to work with.
///
/// The thread runs for the lifetime of the process.
pub fn on_each_load<P, F>(label: &'static str, poll_interval: Duration, finder: P, on_load: F)
where
    P: Fn() -> Option<*const u8> + Send + Sync + 'static,
    F: Fn(*const u8) + Send + Sync + 'static,
{
    use std::sync::Arc;

    /// Long enough for a busy frame, short enough that a stalled
    /// game thread does not wedge the watcher forever.
    const TIMEOUT: Duration = Duration::from_secs(5);

    let finder = Arc::new(finder);
    let on_load = Arc::new(on_load);

    // Both the finder and the action read live game objects, so
    // NEITHER may run on this background thread. A level unload
    // deletes those objects; reading one mid-delete faults and
    // kills the process. This watcher used to do exactly that,
    // and the crash landed one second after it logged
    // "gone (main menu?)". See ueforge::game_thread::run.
    let thread_name = format!("ueforge-load-{label}");
    let _ = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            /// Ask the game thread whether the finder sees anything.
            /// `None` also covers "the game thread did not answer",
            /// which is the safe reading: do nothing.
            fn look<P>(finder: &Arc<P>) -> Option<usize>
            where
                P: Fn() -> Option<*const u8> + Send + Sync + 'static,
            {
                let f = finder.clone();
                let found = crate::game_thread::run(
                    move || Ok(serde_json::json!(f().map(|p| p as usize))),
                    TIMEOUT,
                );
                found.ok()?.as_u64().map(|a| a as usize)
            }

            /// Is a world loaded? Cheap when the mod has registered
            /// where its streamed levels live, and `None` when it has
            /// not, which means "no idea" rather than "no world".
            fn world() -> Option<bool> {
                crate::game_thread::run(
                    || Ok(serde_json::json!(crate::ue::streaming::world_is_up())),
                    TIMEOUT,
                )
                .ok()?
                .as_bool()
            }

            loop {
                // Waiting for a world. Searching for the thing before
                // there is a world to hold it is 100 ms spent to
                // learn nothing.
                if world() == Some(false) {
                    std::thread::sleep(poll_interval);
                    continue;
                }
                std::thread::sleep(poll_interval);
                let Some(addr) = look(&finder) else {
                    continue;
                };
                crate::log::log(format_args!("{label}: found, applying"));
                let action = on_load.clone();
                let _ = crate::game_thread::run(
                    move || {
                        action(addr as *const u8);
                        Ok(serde_json::Value::Null)
                    },
                    TIMEOUT,
                );

                // Applied. Now the only question is whether the world
                // goes away, and that is ONE BIT. Asking it by
                // re-running the finder and seeing nothing costs a
                // full object search every poll, for the life of the
                // process: 1009 ms per 30 seconds in MISERY, the
                // biggest single cost in the mod once everything else
                // was fixed.
                loop {
                    std::thread::sleep(poll_interval);
                    match world() {
                        Some(true) => continue,
                        Some(false) => {
                            crate::log::log(format_args!(
                                "{label}: world gone, waiting for the next one"
                            ));
                            // Everything found in that world is now a
                            // stale pointer.
                            let _ = crate::game_thread::run(
                                || {
                                    forget_all();
                                    Ok(serde_json::Value::Null)
                                },
                                TIMEOUT,
                            );
                            break;
                        }
                        // No streamer registered, so fall back to the
                        // old way: run the finder and see.
                        None => {
                            if look(&finder).is_none() {
                                crate::log::log(format_args!(
                                    "{label}: gone (main menu?), waiting for reload"
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    static NEVER_FOUND: LiveActor = LiveActor::new("NeverFound_C");

    #[test]
    fn retained_actor_does_not_search_when_empty() {
        assert!(NEVER_FOUND.retained().is_none());
        assert!(!NEVER_FOUND.is_held());
    }

    #[test]
    fn actor_enumeration_layout_uses_reflected_offsets() {
        let properties = vec![
            NativeProperty {
                name: "WorldContextObject".into(),
                offset: 0,
                element_size: 8,
            },
            NativeProperty {
                name: "ActorClass".into(),
                offset: 8,
                element_size: 8,
            },
            NativeProperty {
                name: "OutActors".into(),
                offset: 16,
                element_size: 8,
            },
        ];

        assert_eq!(
            ActorEnumerationLayout::from_properties(32, &properties).unwrap(),
            ActorEnumerationLayout {
                parms_size: 32,
                world_context: 0,
                actor_class: 8,
                out_actors: 16,
            }
        );
    }

    #[test]
    fn actor_enumeration_layout_rejects_missing_or_truncated_output() {
        let properties = vec![
            NativeProperty {
                name: "WorldContextObject".into(),
                offset: 0,
                element_size: 8,
            },
            NativeProperty {
                name: "ActorClass".into(),
                offset: 8,
                element_size: 8,
            },
        ];
        assert!(ActorEnumerationLayout::from_properties(32, &properties).is_err());

        let mut with_output = properties;
        with_output.push(NativeProperty {
            name: "OutActors".into(),
            offset: 24,
            element_size: 8,
        });
        assert!(ActorEnumerationLayout::from_properties(32, &with_output).is_err());
    }

    #[test]
    fn component_lookup_layout_uses_reflected_offsets() {
        let properties = vec![
            NativeProperty {
                name: "ComponentClass".into(),
                offset: 0,
                element_size: 8,
            },
            NativeProperty {
                name: "ReturnValue".into(),
                offset: 8,
                element_size: 8,
            },
        ];
        assert_eq!(
            ComponentLookupLayout::from_properties(16, &properties).unwrap(),
            ComponentLookupLayout {
                parms_size: 16,
                component_class: 0,
                return_value: 8,
            }
        );
    }

    #[test]
    fn component_enumeration_layout_uses_reflected_offsets() {
        let properties = vec![
            NativeProperty {
                name: "ComponentClass".into(),
                offset: 0,
                element_size: 8,
            },
            NativeProperty {
                name: "ReturnValue".into(),
                offset: 8,
                element_size: 16,
            },
        ];
        assert_eq!(
            ComponentEnumerationLayout::from_properties(24, &properties).unwrap(),
            ComponentEnumerationLayout {
                parms_size: 24,
                component_class: 0,
                return_value: 8,
            }
        );
    }
}
