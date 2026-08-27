//! Putting a rolled plan of phenomena into an Unreal world.
//!
//! [`modforge::storyteller::PhenomenonPlanner`] decides WHAT goes
//! where: which kinds, how many, scattered how widely, with what
//! session cap. It deliberately knows no engine, so it asks the
//! caller four questions: where is the ground, how many variants
//! has this kind got, what class is variant N, and please spawn
//! this.
//!
//! Answering those four is the same code in every Unreal game.
//! Only the class NAMES differ, and those come from the game's
//! own catalog of [`modforge::storyteller::Phenomenon`].
//!
//! This is what was left in MISERY's `strange.rs` once the
//! planning had moved to modforge: a world-context actor, a
//! ground trace, a class lookup and a spawn.

use modforge::storyteller::{Phenomenon, PhenomenonDef, PhenomenonPlanner};

/// How far above and below a point to look for the ground.
#[derive(Clone, Copy)]
pub struct GroundReach {
    pub up: f64,
    pub down: f64,
}

/// Place a rolled plan into the world.
///
/// `plan` is the list of catalog indices the planner chose,
/// `centre` and `half_extent` are the region to scatter inside,
/// in this crate's metres.
///
/// Spawning needs a live actor to take the world from, so one is
/// found by class chain. That IS a search, which is why this
/// runs only when a region has actually streamed in, never on a
/// timer (`misery-mod/docs/performance.md`).
///
/// Returns how many props were actually placed. A prop whose
/// ground could not be found, or whose class is not loaded, is
/// skipped and does not consume the session cap.
///
/// # Safety
///
/// Game thread only, and the caller must not hold the planner
/// lock across it.
pub unsafe fn place(
    planner: &mut PhenomenonPlanner<String>,
    catalog: &[Phenomenon],
    plan: &[usize],
    centre: (f64, f64),
    half_extent: f64,
    world_class: &str,
    reach: GroundReach,
) -> Result<usize, String> {
    let world_ctx = super::actor::find_actors_by_chain(world_class)
        .into_iter()
        .next()
        .ok_or("no live actor to take the world from")?;

    let defs: Vec<PhenomenonDef> = catalog.iter().map(|p| p.planning).collect();
    let placed = planner.execute(
        plan,
        &defs,
        centre,
        half_extent,
        |x, y| {
            // SAFETY: world_ctx is a live actor and the caller
            // promised the game thread.
            unsafe { super::trace::ground_z(world_ctx, x, y, reach.up, reach.down) }
        },
        |index| catalog[index].classes.len(),
        |index, variant| {
            let name = catalog[index].classes[variant];
            super::find_class_fast(name).map(|class| class.as_object().as_ptr() as u64)
        },
        |request| {
            // SAFETY: world_ctx is live and the class came from
            // this frame's lookup; game thread.
            let actor = unsafe {
                super::spawn::spawn_actor(
                    world_ctx,
                    request.class,
                    request.position,
                    request.yaw,
                    1.0,
                )
            };
            actor != 0
        },
    );
    Ok(placed)
}
