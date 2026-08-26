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
