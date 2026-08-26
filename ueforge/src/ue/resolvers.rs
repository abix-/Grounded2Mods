//! UE engine address resolution via [`patternsleuth`].
//!
//! `trumank/patternsleuth` is the Rust sig-scan crate UE4SS
//! itself uses to locate engine functions at runtime. Its
//! `resolvers::unreal` module ships battle-tested patterns for
//! the canonical UE primitives (`GUObjectArray`, `FNamePool`,
//! `FNameToString`, `GMalloc`, ...). We pin a specific upstream
//! SHA in the workspace and consume the resolvers we need.
//!
//! All address offsets (g_objects, g_names, append_string) are
//! resolved dynamically at init. No hardcoded address fallbacks;
//! they break on every game patch. The only constants callers
//! supply are structural: `process_event_idx` (vtable slot,
//! stable across UE 5.x) and `g_objects_layout` (array shape).

use patternsleuth::resolvers::impl_try_collector;
use patternsleuth::resolvers::unreal::fname::{FNamePool, FNameToString};
use patternsleuth::resolvers::unreal::game_loop::UGameEngineTick;
use patternsleuth::resolvers::unreal::gmalloc::GMalloc;
use patternsleuth::resolvers::unreal::guobject_array::GUObjectArray;

impl_try_collector! {
    /// Collection of resolvers patternsleuth runs in one pass.
    /// Each field's type is a resolver-singleton struct from
    /// `patternsleuth::resolvers::unreal::*`; resolution
    /// returns one absolute u64 per field.
    #[derive(Debug, PartialEq, Clone)]
    struct UeResolution {
        guobject_array: GUObjectArray,
        fname_pool: FNamePool,
        fname_to_string: FNameToString,
        gmalloc: GMalloc,
    }
}

impl_try_collector! {
    /// `UGameEngine::Tick`, resolved on its own rather than as
    /// part of [`UeResolution`]: the base offsets are required
    /// for the mod to work at all, while this one is optional
    /// and must be allowed to fail without taking init with it.
    #[derive(Debug, PartialEq, Clone)]
    struct TickResolution {
        game_engine_tick: UGameEngineTick,
    }
}

/// Absolute address of `UGameEngine::Tick`.
///
/// This is the same resolver UE4SS drives for its own EngineTick
/// hook (`HookEngineTick = 1`, `EngineTickResolveMethod = Scan`).
/// The address is what turns the engine's vtable into a slot
/// index: find the slot holding it and that index is Tick's,
/// with no per-game constant.
///
/// Absolute, not image-relative, because callers compare it
/// against live vtable entries.
pub fn resolve_game_engine_tick() -> Result<usize, String> {
    let exe = patternsleuth::process::internal::read_image()
        .map_err(|e| format!("patternsleuth: read_image failed: {e}"))?;
    let resolution = exe
        .resolve(TickResolution::resolver())
        .map_err(|e| format!("patternsleuth: UGameEngine::Tick not found: {e}"))?;
    Ok(resolution.game_engine_tick.0 as usize)
}

/// Image-relative offsets resolved via patternsleuth. Subtract
/// `host_image_base()` from each absolute address so the
/// offsets are stable across runs / ASLR slides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedOffsets {
    /// FUObjectArray instance (the `GUObjectArray` global).
    pub g_objects: usize,
    /// FNamePool / GNames (the global FName allocator).
    pub g_names: usize,
    /// `FName::ToString` / `AppendString`-shaped fn entry.
    pub append_string: usize,
    /// `GMalloc` global pointer (points to the FMalloc vtable object).
    pub gmalloc: usize,
}

/// Resolve the three base UE offsets via patternsleuth, returning
/// them image-relative.
///
/// Cost: one full scan of the host exe's `.text` (tens of ms,
/// init-only). The patternsleuth library handles the underlying
/// async + futures join internally; we drive the resulting
/// future to completion on this thread.
///
/// Returns an error string if patternsleuth couldn't read the
/// image or any resolver couldn't find a unique match. Game
/// crates wire this from their `on_unreal_init` worker as a
/// fallback when hardcoded `PlatformOffsets` are stale.
pub fn resolve_image_offsets() -> Result<ResolvedOffsets, String> {
    let exe = patternsleuth::process::internal::read_image()
        .map_err(|e| format!("patternsleuth: read_image failed: {e}"))?;
    let resolution = exe
        .resolve(UeResolution::resolver())
        .map_err(|e| format!("patternsleuth: resolve failed: {e}"))?;

    let image_base = crate::ue::platform::host_image_base();
    let to_rel = |abs: u64| -> Result<usize, String> {
        let abs = abs as usize;
        if abs < image_base {
            return Err(format!(
                "patternsleuth: absolute 0x{abs:x} < image_base 0x{image_base:x}"
            ));
        }
        Ok(abs - image_base)
    };

    Ok(ResolvedOffsets {
        g_objects: to_rel(resolution.guobject_array.0)?,
        g_names: to_rel(resolution.fname_pool.0)?,
        append_string: to_rel(resolution.fname_to_string.0)?,
        gmalloc: to_rel(resolution.gmalloc.0)?,
    })
}

/// Debug op entry. Re-runs patternsleuth and compares against the
/// offsets the runtime was initialized with.
pub fn resolve_offsets_op(_args: &serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let resolved = resolve_image_offsets()?;
    let rt = crate::ue::try_runtime()
        .ok_or_else(|| "ueforge runtime not initialized".to_string())?;
    let rt_off = rt.platform_offsets;
    Ok(json!({
        "resolved": {
            "g_objects": format!("0x{:x}", resolved.g_objects),
            "g_names": format!("0x{:x}", resolved.g_names),
            "append_string": format!("0x{:x}", resolved.append_string),
            "gmalloc": format!("0x{:x}", resolved.gmalloc),
        },
        "runtime": {
            "g_objects": format!("0x{:x}", rt_off.g_objects),
            "g_names": format!("0x{:x}", rt_off.g_names),
            "append_string": format!("0x{:x}", rt_off.append_string),
        },
        "matches": {
            "g_objects": resolved.g_objects == rt_off.g_objects,
            "g_names": resolved.g_names == rt_off.g_names,
            "append_string": resolved.append_string == rt_off.append_string,
        },
    }))
}
