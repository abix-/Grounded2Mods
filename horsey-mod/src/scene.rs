//! Agent-state readers for scene / world-map context.
//!
//! Surfaces every field documented in
//! [`docs/HORSE-PLACES.md`](../docs/HORSE-PLACES.md) "Top-level singletons"
//! and "Scene-handler factory" sections so the Debug tab + tests can show
//! the live values:
//!
//! - `active_scene_id` (GS+0x25C)
//! - Truck pointer (GS+0x300) + pos/vel floats (+0x28/+0x2c/+0x30/+0x34)
//! - MapState (DAT_1403f4e00) + camera (+0x254/+0x258) + map-active flag (+0x262)
//! - Active scene-handler (GS[+0x438] + active_scene_id * 8)
//! - Scene-handler scene_id (+0x08), display_name (+0x18), internal_name (+0x38)
//! - Scene-handler selected entity (+0x148), modal-visible bool (+0x270)
//!
//! All derefs go through `modforge::winproc::is_addr_readable` so a stale
//! chain cannot fault the HTTP worker.
//!
//! The scene-id -> name mapping (CRISPR = 16, Circus = 23, etc.) comes from
//! the scene-handler factory dispatcher at `FUN_140113d70`. See
//! [`docs/HORSE-PLACES.md`](../docs/HORSE-PLACES.md) "Scene-handler factory
//! + complete scene_id table".

use crate::gamestate;
use crate::targets;

const TRUCK_PTR_OFF: usize = 0x300; // GameState + 0x300 -> Truck*
const TRUCK_POS_X_OFF: usize = 0x28; // float, tile units * DAT_140303fb4
const TRUCK_POS_Y_OFF: usize = 0x2c;
const TRUCK_VEL_X_OFF: usize = 0x30;
const TRUCK_VEL_Y_OFF: usize = 0x34;

const MS_CAMERA_X_OFF: usize = 0x254; // MapState +0x254 = camera_x (float, world pixels)
const MS_CAMERA_Y_OFF: usize = 0x258;
const MS_MAP_ACTIVE_OFF: usize = 0x262; // bool: buildings loaded / map active

const SCENE_TABLE_OFF: usize = 0x438; // GameState + 0x438 -> scene table base

const SH_SCENE_ID_OFF: usize = 0x08; // scene_handler + 0x08 (i32 scene_id)
const SH_DISPLAY_NAME_OFF: usize = 0x18; // scene_handler + 0x18 (std::string display)
const SH_INTERNAL_NAME_OFF: usize = 0x38; // scene_handler + 0x38 (std::string internal)
const SH_SELECTED_ENT_OFF: usize = 0x148; // scene_handler + 0x148 (Entity*)
const SH_MODAL_OFF: usize = 0x270; // scene_handler + 0x270 (bool modal visible)

// ---------------------------------------------------------------------------
// Singletons
// ---------------------------------------------------------------------------

/// Read the MapState pointer (`*DAT_1403f4e00`). Returns 0 if not readable.
pub fn mapstate_ptr() -> usize {
    let slot = targets::rebase(targets::MAP_STATE_PTR);
    if !modforge::winproc::is_addr_readable(slot) {
        return 0;
    }
    // SAFETY: slot readability checked above.
    let p = unsafe { *(slot as *const usize) };
    if p == 0 || !modforge::winproc::is_addr_readable(p) {
        return 0;
    }
    p
}

/// World-pos -> tile-index scale (float, lives in `.rdata`).
/// Returns None if the slot is not readable yet (mod not fully attached).
pub fn tile_scale() -> Option<f32> {
    let addr = targets::rebase(targets::TILE_SCALE);
    if !modforge::winproc::is_addr_readable(addr + 4) {
        return None;
    }
    // SAFETY: 4-byte readability checked.
    let v = unsafe { *(addr as *const f32) };
    if v.is_finite() && v > 0.0 && v < 10.0 {
        Some(v)
    } else {
        None
    }
}

/// Convert raw actor position floats to tile coords via `tile_scale()`.
pub fn pos_to_tile(pos_x: f32, pos_y: f32) -> Option<(i32, i32)> {
    let s = tile_scale()?;
    Some(((pos_x * s) as i32, (pos_y * s) as i32))
}

/// Read the Truck pointer (`*(GameState + 0x300)`). Returns 0 if not readable.
pub fn truck_ptr() -> usize {
    let gs = gamestate::ptr();
    if gs == 0 {
        return 0;
    }
    let slot = gs + TRUCK_PTR_OFF;
    if !modforge::winproc::is_addr_readable(slot) {
        return 0;
    }
    // SAFETY: slot readability checked.
    let p = unsafe { *(slot as *const usize) };
    if p == 0 || !modforge::winproc::is_addr_readable(p + TRUCK_VEL_Y_OFF) {
        return 0;
    }
    p
}

// ---------------------------------------------------------------------------
// Truck position / velocity
// ---------------------------------------------------------------------------

/// Truck position floats (raw, before scale-to-tile). Returns (None, None) when
/// the truck pointer is dead.
pub fn truck_pos() -> Option<(f32, f32)> {
    let t = truck_ptr();
    if t == 0 {
        return None;
    }
    // SAFETY: truck_ptr ensures +0x34 is readable; +0x28/+0x2c are earlier.
    let x = unsafe { *((t + TRUCK_POS_X_OFF) as *const f32) };
    let y = unsafe { *((t + TRUCK_POS_Y_OFF) as *const f32) };
    Some((x, y))
}

/// Truck velocity floats.
pub fn truck_vel() -> Option<(f32, f32)> {
    let t = truck_ptr();
    if t == 0 {
        return None;
    }
    let x = unsafe { *((t + TRUCK_VEL_X_OFF) as *const f32) };
    let y = unsafe { *((t + TRUCK_VEL_Y_OFF) as *const f32) };
    Some((x, y))
}

// ---------------------------------------------------------------------------
// MapState camera + flags
// ---------------------------------------------------------------------------

/// Camera (world-pixel coords) from MapState +0x254 / +0x258.
pub fn camera() -> Option<(f32, f32)> {
    let ms = mapstate_ptr();
    if ms == 0 {
        return None;
    }
    if !modforge::winproc::is_addr_readable(ms + MS_CAMERA_Y_OFF + 4) {
        return None;
    }
    let x = unsafe { *((ms + MS_CAMERA_X_OFF) as *const f32) };
    let y = unsafe { *((ms + MS_CAMERA_Y_OFF) as *const f32) };
    Some((x, y))
}

/// Map-active / buildings-loaded bool at MapState +0x262.
pub fn map_active() -> Option<bool> {
    let ms = mapstate_ptr();
    if ms == 0 {
        return None;
    }
    if !modforge::winproc::is_addr_readable(ms + MS_MAP_ACTIVE_OFF) {
        return None;
    }
    let b = unsafe { *((ms + MS_MAP_ACTIVE_OFF) as *const u8) };
    Some(b != 0)
}

// ---------------------------------------------------------------------------
// Active scene-handler
// ---------------------------------------------------------------------------

/// Active scene-handler pointer = `*(GS[+0x438] + active_scene_id * 8)`.
/// Returns 0 when not in a scene, or when chain is dead.
pub fn active_scene_handler() -> usize {
    let id = match gamestate::active_scene_id() {
        Some(n) if n >= 0 => n as usize,
        _ => return 0,
    };
    let gs = gamestate::ptr();
    if gs == 0 {
        return 0;
    }
    let table_slot = gs + SCENE_TABLE_OFF;
    if !modforge::winproc::is_addr_readable(table_slot) {
        return 0;
    }
    // SAFETY: slot readability checked.
    let table = unsafe { *(table_slot as *const usize) };
    if table == 0 {
        return 0;
    }
    let entry_slot = table + id * 8;
    if !modforge::winproc::is_addr_readable(entry_slot) {
        return 0;
    }
    let handler = unsafe { *(entry_slot as *const usize) };
    if handler == 0 || !modforge::winproc::is_addr_readable(handler + SH_MODAL_OFF) {
        return 0;
    }
    handler
}

/// Read the scene-handler's self-reported scene_id (`+0x08`).
pub fn scene_handler_self_id(sh: usize) -> Option<i32> {
    if sh == 0 {
        return None;
    }
    Some(unsafe { *((sh + SH_SCENE_ID_OFF) as *const i32) })
}

/// Read an MSVC `std::string` at `base + off`. Returns "" if SSO empty,
/// the inline bytes if SSO, or the heap string when capacity > 15.
fn read_msvc_string(base: usize, off: usize) -> Option<String> {
    if base == 0 {
        return None;
    }
    let entry = base + off;
    if !modforge::winproc::is_addr_readable(entry + 0x20) {
        return None;
    }
    // SAFETY: readability checked.
    let size = unsafe { *((entry + 0x10) as *const usize) };
    let cap = unsafe { *((entry + 0x18) as *const usize) };
    if size == 0 || size > 1024 {
        return Some(String::new());
    }
    let data_ptr = if cap > 15 {
        // Heap string: first 8 bytes are a pointer to the buffer.
        let p = unsafe { *(entry as *const usize) };
        if p == 0 || !modforge::winproc::is_addr_readable(p + size) {
            return None;
        }
        p
    } else {
        // SSO: bytes live inline starting at entry.
        entry
    };
    // SAFETY: data_ptr+size readability verified above (for heap) / part of
    // entry's readability (for SSO).
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, size) };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Scene display name (e.g. "The Circus", "CRISPR Lab"). Read from
/// `scene_handler + 0x18`.
pub fn scene_display_name(sh: usize) -> Option<String> {
    read_msvc_string(sh, SH_DISPLAY_NAME_OFF)
}

/// Scene internal name (e.g. "Circus", "Crispr"). Read from
/// `scene_handler + 0x38`.
pub fn scene_internal_name(sh: usize) -> Option<String> {
    read_msvc_string(sh, SH_INTERNAL_NAME_OFF)
}

/// Currently selected / "relevant" entity for this scene (often a horse or
/// NPC). Read from `scene_handler + 0x148`.
pub fn scene_selected_entity(sh: usize) -> Option<usize> {
    if sh == 0 {
        return None;
    }
    if !modforge::winproc::is_addr_readable(sh + SH_SELECTED_ENT_OFF) {
        return None;
    }
    let p = unsafe { *((sh + SH_SELECTED_ENT_OFF) as *const usize) };
    if p == 0 { None } else { Some(p) }
}

/// Modal / dialog visible bool from `scene_handler + 0x270`.
pub fn scene_modal_visible(sh: usize) -> Option<bool> {
    if sh == 0 {
        return None;
    }
    if !modforge::winproc::is_addr_readable(sh + SH_MODAL_OFF) {
        return None;
    }
    Some(unsafe { *((sh + SH_MODAL_OFF) as *const u8) } != 0)
}

// ---------------------------------------------------------------------------
// Scene-id semantic naming (best-effort from decomp; some entries TBD)
// ---------------------------------------------------------------------------

/// Best-known label for a scene_id from the decomp factory dispatcher table.
/// Returns None when the id has no documented mapping (worth investigating).
pub fn scene_id_label(id: i32) -> Option<&'static str> {
    match id {
        -1 => Some("Overworld (driving the world map)"),
        0 => Some(
            "PLAYER HOME (stable / pasture / breeding) -- scene-table slot 0; confirmed 2026-05-17 live",
        ),
        1..=7 => Some("Race lane (slot 1..7 = race lanes 0..6)"),
        13 => Some("Race Track"),
        14 => Some("Paddock (broke variant of Race Track)"),
        15 => Some("(scene 15 -- unknown; possibly terrain)"),
        16 => Some("CRISPR Lab"),
        17 => Some("Sue's Glues"),
        18 => Some("Clide's Rides"),
        19 => Some("(scene 19 -- unknown; FUN_1400bfa90)"),
        20 => Some("Lasso-Mart (confirmed 2026-05-17 live)"),
        21 => Some(
            "(scene 21 -- unknown; was suspected player home but home is scene 0; possibly an NPC stable)",
        ),
        22 => Some("Bubber's Hutch"),
        23 => Some("The Circus"),
        24 => Some("The Secret Zoo"),
        25 => Some("O'er Yonder Acres"),
        26 => Some("Saloon"),
        27 => Some("Power Plant"),
        28 => Some("Sweetie's House (NPC, NOT player's home)"),
        29 => Some("Sumo Ring"),
        30 => Some("Junk Shop (debug)"),
        31 => Some("Normal Suburban House (Biohacker facade)"),
        36 => Some("The Jockey Club"),
        37 => Some("Old Abandoned Track"),
        38 => Some("(scene 38 -- unknown; empty display)"),
        47 => Some("(scene 47 -- gif_land_handler, terrain)"),
        _ => None,
    }
}
