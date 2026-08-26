//! Spawning actors into a live world.
//!
//! `AIBlueprintHelperLibrary::SpawnAIFromClass` is an engine
//! function (AIModule), not a game's own, so it works anywhere
//! the module is present. It spawns a pawn AND gives it a
//! controller, which is what makes a spawned NPC actually behave
//! rather than stand still.
//!
//! Parm block measured from the object dump and proven live
//! (misery research.md 26.3, `research_spawn::spawn_one_npc`):
//!
//! ```text
//! WorldContextObject 0x00   PawnClass 0x08   BehaviorTree 0x10
//! Location 0x18 (FVector, 3 x f64)
//! Rotation 0x30 (FRotator, 3 x f64)
//! bNoCollisionFail 0x48     Owner 0x50       ReturnValue 0x58
//! ```
//!
//! Game thread only, like everything that goes through
//! ProcessEvent.

use std::ffi::c_void;

use super::UObject;

/// The `SpawnAIFromClass` parm block. Field order and padding
/// are the engine's, so this is `repr(C)`.
#[repr(C)]
struct SpawnAiParms {
    world_context: u64,
    pawn_class: u64,
    behavior_tree: u64,
    location: [f64; 3],
    rotation: [f64; 3],
    b_no_collision_fail: u8,
    _pad: [u8; 7],
    owner: u64,
    return_value: u64,
}

/// Spawn a pawn of `pawn_class` at `location`, with a controller.
///
/// `world_context` is any live actor in the target world; the
/// engine reads the world off it. `pawn_class` is a `UClass`
/// pointer, most easily taken from a live instance of the class
/// to be copied.
///
/// Returns the spawned pawn's address, or 0 when the engine
/// refused (no world, bad class, blocked location with collision
/// checking on).
///
/// `no_collision_fail` true spawns even where something is in the
/// way. False is the honest choice for gameplay; true is what a
/// bulk placer wants so a single blocked spot does not silently
/// drop the spawn.
///
/// # Safety
/// `world_context` must be a live actor, `pawn_class` a live
/// `UClass`, and this must run on the game thread.
pub unsafe fn spawn_ai_from_class(
    world_context: *const u8,
    pawn_class: u64,
    location: (f64, f64, f64),
    yaw: f64,
    no_collision_fail: bool,
) -> u64 {
    let Some(cls) = super::find_class_fast("AIBlueprintHelperLibrary") else {
        return 0;
    };
    let Some(func) = cls.get_function("AIBlueprintHelperLibrary", "SpawnAIFromClass") else {
        return 0;
    };
    let Some(cdo) = cls.class_default_object() else {
        return 0;
    };
    let mut parms = SpawnAiParms {
        world_context: world_context as u64,
        pawn_class,
        behavior_tree: 0,
        location: [location.0, location.1, location.2],
        // FRotator is pitch, yaw, roll.
        rotation: [0.0, yaw, 0.0],
        b_no_collision_fail: u8::from(no_collision_fail),
        _pad: [0; 7],
        owner: 0,
        return_value: 0,
    };
    // SAFETY: caller guarantees the game thread and live inputs;
    // the parm block matches the dumped layout.
    unsafe {
        cdo.process_event(func, &mut parms as *mut SpawnAiParms as *mut c_void);
    }
    parms.return_value
}

/// Write an `FTransform` into a parm block at `at`.
///
/// `FTransform` is a quaternion rotation (4 doubles), then
/// translation (3), then scale (3), with members aligned to
/// 0x20, so it is 0x60 bytes and the scale starts at +0x40 rather
/// than +0x38. Only yaw is taken, because that is the only
/// rotation a placed prop normally needs; the quaternion for a
/// yaw-only rotation is `(0, 0, sin(yaw/2), cos(yaw/2))`.
///
/// `yaw` is RADIANS here, unlike `FRotator`, which is degrees.
pub fn write_transform(
    buf: &mut [u8],
    at: usize,
    location: (f64, f64, f64),
    yaw: f64,
    scale: f64,
) {
    let (s, c) = (yaw / 2.0).sin_cos();
    let put = |b: &mut [u8], off: usize, v: f64| {
        b[at + off..at + off + 8].copy_from_slice(&v.to_le_bytes());
    };
    put(buf, 0x00, 0.0); // quat x
    put(buf, 0x08, 0.0); // quat y
    put(buf, 0x10, s); // quat z
    put(buf, 0x18, c); // quat w
    put(buf, 0x20, location.0);
    put(buf, 0x28, location.1);
    put(buf, 0x30, location.2);
    // 0x38 is padding: FTransform members are 0x20-aligned.
    put(buf, 0x40, scale);
    put(buf, 0x48, scale);
    put(buf, 0x50, scale);
}

/// Start placing an actor, before it runs its construction.
///
/// `GameplayStatics:BeginDeferredActorSpawnFromClass`, parm block
/// 0x90 with the actor returned at 0x88. The actor exists but is
/// not in the world until [`finish_spawn`]; between the two calls
/// is where its properties can be set so that construction sees
/// them.
///
/// Returns 0 when the engine refused.
///
/// # Safety
/// `world_context` must be a live actor, `class` a live `UClass`,
/// on the game thread.
pub unsafe fn begin_spawn(
    world_context: *const u8,
    class: u64,
    location: (f64, f64, f64),
    yaw: f64,
    scale: f64,
) -> u64 {
    let Some(cls) = super::find_class_fast("GameplayStatics") else {
        return 0;
    };
    let Some(func) = cls.get_function("GameplayStatics", "BeginDeferredActorSpawnFromClass") else {
        return 0;
    };
    let Some(cdo) = cls.class_default_object() else {
        return 0;
    };
    let mut parms = [0u8; 0x90];
    parms[0x00..0x08].copy_from_slice(&(world_context as u64).to_le_bytes());
    parms[0x08..0x10].copy_from_slice(&class.to_le_bytes());
    write_transform(&mut parms, 0x10, location, yaw, scale);
    parms[0x70] = 1; // AlwaysSpawn: place it even where something is
    // SAFETY: caller guarantees the game thread and live inputs.
    unsafe {
        cdo.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    u64::from_le_bytes(parms[0x88..0x90].try_into().unwrap_or_default())
}

/// Finish placing an actor started with [`begin_spawn`].
///
/// `GameplayStatics:FinishSpawningActor`, parm block 0x80 with
/// the actor returned at 0x78. The transform is passed again
/// because this is the one the actor ends up with.
///
/// # Safety
/// `actor` must be the pointer [`begin_spawn`] returned, on the
/// game thread.
pub unsafe fn finish_spawn(actor: u64, location: (f64, f64, f64), yaw: f64, scale: f64) -> u64 {
    let Some(cls) = super::find_class_fast("GameplayStatics") else {
        return 0;
    };
    let Some(func) = cls.get_function("GameplayStatics", "FinishSpawningActor") else {
        return 0;
    };
    let Some(cdo) = cls.class_default_object() else {
        return 0;
    };
    let mut parms = [0u8; 0x80];
    parms[0x00..0x08].copy_from_slice(&actor.to_le_bytes());
    write_transform(&mut parms, 0x10, location, yaw, scale);
    // SAFETY: caller guarantees the game thread and a live actor.
    unsafe {
        cdo.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    u64::from_le_bytes(parms[0x78..0x80].try_into().unwrap_or_default())
}

/// Place an actor in one step, for callers with nothing to set
/// between begin and finish.
///
/// # Safety
/// As [`begin_spawn`].
pub unsafe fn spawn_actor(
    world_context: *const u8,
    class: u64,
    location: (f64, f64, f64),
    yaw: f64,
    scale: f64,
) -> u64 {
    // SAFETY: forwarded from the caller's guarantee.
    let actor = unsafe { begin_spawn(world_context, class, location, yaw, scale) };
    if actor == 0 {
        return 0;
    }
    // SAFETY: actor came from begin_spawn above.
    unsafe { finish_spawn(actor, location, yaw, scale) }
}

/// The `UClass` pointer of a live object, for use as
/// `pawn_class`. Copying a class off something already in the
/// world avoids resolving it by name, which fails for Blueprint
/// classes that have been reinstanced.
///
/// # Safety
/// `obj` must be a live `UObject`.
pub unsafe fn class_of(obj: *const u8) -> u64 {
    // SAFETY: caller guarantees a live UObject.
    let Some(o) = (unsafe { (obj as *const UObject).as_ref() }) else {
        return 0;
    };
    o.class().map(|c| c as *const _ as u64).unwrap_or(0)
}
