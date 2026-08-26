//! Asking the world what is there: line traces.
//!
//! The common question is "what is the ground height at this X
//! and Y", which anything that places objects needs and which no
//! amount of reading memory can answer, because terrain height is
//! computed, not stored per point.
//!
//! `KismetSystemLibrary:LineTraceSingle` answers it: trace from
//! well above the point to well below it and take the impact.
//!
//! Parm block from the object dump, 0x180 bytes:
//!
//! ```text
//! WorldContextObject 0x00   Start 0x08 (FVector)   End 0x20
//! TraceChannel 0x38         bTraceComplex 0x39
//! ActorsToIgnore 0x40       DrawDebugType 0x50
//! OutHit 0x58 (FHitResult)  bIgnoreSelf 0x150
//! ReturnValue 0x178
//! ```
//!
//! Game thread only.

use std::ffi::c_void;

pub mod offsets {
    /// Within the 0x180 `LineTraceSingle` parm block.
    pub const WORLD_CONTEXT: usize = 0x00;
    pub const START: usize = 0x08;
    pub const END: usize = 0x20;
    pub const TRACE_CHANNEL: usize = 0x38;
    pub const OUT_HIT: usize = 0x58;
    pub const IGNORE_SELF: usize = 0x150;
    pub const RETURN_VALUE: usize = 0x178;
    pub const PARMS_SIZE: usize = 0x180;

    /// `FHitResult::ImpactPoint`, relative to `OUT_HIT`.
    ///
    /// The Z of that point is the THIRD double, so it sits at
    /// `OUT_HIT + IMPACT_POINT + 16` = 0x90. Reading 0x88 returns
    /// the point's Y instead, which silently placed everything at
    /// the wrong height until it was found.
    pub const IMPACT_POINT: usize = 0x28;
}

/// `ETraceTypeQuery`. Visibility is the channel that hits solid
/// world geometry, which is what "where is the ground" means.
pub const CHANNEL_VISIBILITY: u8 = 0;

/// Where a trace hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Trace a line and return where it hit, or `None` if it hit
/// nothing.
///
/// `world_context` is any live actor in the target world.
///
/// # Safety
/// `world_context` must be a live actor, and this must run on the
/// game thread.
pub unsafe fn line_trace(
    world_context: *const u8,
    start: (f64, f64, f64),
    end: (f64, f64, f64),
    channel: u8,
) -> Option<Hit> {
    let cls = super::find_class_fast("KismetSystemLibrary")?;
    let func = cls.get_function("KismetSystemLibrary", "LineTraceSingle")?;
    let cdo = cls.class_default_object()?;

    let mut parms = [0u8; offsets::PARMS_SIZE];
    let put = |p: &mut [u8], at: usize, v: f64| {
        p[at..at + 8].copy_from_slice(&v.to_le_bytes());
    };
    put(&mut parms, offsets::WORLD_CONTEXT, 0.0);
    parms[offsets::WORLD_CONTEXT..offsets::WORLD_CONTEXT + 8]
        .copy_from_slice(&(world_context as u64).to_le_bytes());
    put(&mut parms, offsets::START, start.0);
    put(&mut parms, offsets::START + 8, start.1);
    put(&mut parms, offsets::START + 16, start.2);
    put(&mut parms, offsets::END, end.0);
    put(&mut parms, offsets::END + 8, end.1);
    put(&mut parms, offsets::END + 16, end.2);
    parms[offsets::TRACE_CHANNEL] = channel;
    parms[offsets::IGNORE_SELF] = 1;

    // SAFETY: caller guarantees the game thread; cdo and func are
    // live and the parm block matches the dumped layout.
    unsafe {
        cdo.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }

    if parms[offsets::RETURN_VALUE] == 0 {
        return None;
    }
    let at = offsets::OUT_HIT + offsets::IMPACT_POINT;
    let read = |p: &[u8], at: usize| f64::from_le_bytes(p[at..at + 8].try_into().unwrap_or_default());
    Some(Hit {
        x: read(&parms, at),
        y: read(&parms, at + 8),
        z: read(&parms, at + 16),
    })
}

/// The ground height at an X and Y, finding the world context
/// itself.
///
/// `up` and `down` are the caller's, and they are a real
/// decision rather than a detail: too short and a point on a hill
/// returns nothing, too tall and the trace hits a roof and calls
/// it the ground.
///
/// Returns `None` when nothing is under the point, or when no
/// level is loaded.
///
/// Game thread only.
pub fn ground_at(x: f64, y: f64, up: f64, down: f64) -> Option<f64> {
    let ctx = super::actor::any_world_actor()?;
    // SAFETY: ctx is a live actor from the walk above; the
    // caller's contract puts us on the game thread.
    unsafe { ground_z(ctx, x, y, up, down) }
}

/// The ground height at an X and Y: trace straight down through
/// the point and report where it lands.
///
/// `up` and `down` bound the search around z = 0. They have to
/// span the world's real height range, or a point on a hill or in
/// a pit returns `None` rather than a wrong answer.
///
/// # Safety
/// `world_context` must be a live actor, on the game thread.
pub unsafe fn ground_z(
    world_context: *const u8,
    x: f64,
    y: f64,
    up: f64,
    down: f64,
) -> Option<f64> {
    // SAFETY: forwarded from the caller's guarantee.
    unsafe { line_trace(world_context, (x, y, up), (x, y, -down), CHANNEL_VISIBILITY) }
        .map(|h| h.z)
}
