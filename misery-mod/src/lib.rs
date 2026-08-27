//! MISERY mod.
//!
//! Target: the emission ("shining") timer, so a mission gives
//! more time. See `docs/research.md`.
//!
//! Game-specific code here is deliberately thin: `ModDef`, the
//! init / shutdown hooks, the game's `PlatformOffsets`, and the
//! control plane. Everything else is `use ueforge::*`.

pub mod assets;
pub mod debug;
pub mod dispatch;
pub mod gameplay;

pub mod autoload;
pub mod nag;
pub mod rooms;
pub mod shining;
pub mod spawning;
pub mod speed;
pub mod strange;
pub mod vendors;

use std::time::Duration;
use ueforge::ue::GObjectsLayout;
use ueforge::ue::datatable::FieldTweak;

/// How far above a point a ground trace starts, and how far down
/// it reaches, in engine units.
///
/// These are MISERY's, not the engine's: 40 m up is high enough
/// to clear this game's terrain and low enough that the trace
/// hits the ground rather than a building's roof, and 80 m down
/// reaches the bottom of its pits. The trace itself is
/// `ueforge::ue::trace::ground_at`.
pub const TRACE_UP: f64 = 4000.0;
pub const TRACE_DOWN: f64 = 8000.0;

// MaxStack is Int (i32) at offset 0x44 within S_ItemDetails rows.
// See docs/research.md section 23.
static STACK_TWEAK: FieldTweak<i32> = FieldTweak::new("ItemList", 0x44);

// ---- Platform detection ----
//
// All address offsets (g_objects, g_names, append_string) are
// resolved dynamically via patternsleuth at init. No hardcoded
// addresses; they break on every game patch.
//
// Historical reference (Stalker 2 Steam build 2026-07):
//   g_objects:     0x07A7_8ED0
//   append_string: 0x010D_C5E0
//   g_names:       0x079C_2180
//
// process_event_idx 0x4D is the vtable slot for
// UObject::ProcessEvent in THIS game. Measured live 2026-08-25
// by scanning the GameInstance vtable for the ProcessEvent
// address UE4SS logs (research_dispatch::vtable_compare). The
// previous 0x4C ("stable across UE 5.x") was wrong here: hooks
// installed cleanly but never fired, and call_ufunction invoked
// the wrong virtual, returning Ok with no visible effect.
const PROCESS_EVENT_IDX: usize = 0x4D;
const G_OBJECTS_LAYOUT: GObjectsLayout = GObjectsLayout::WrappedChunked;

/// Control plane port. Distinct from the other mods in this
/// workspace so two games can run side by side: wwm-mod and
/// outworld-station-mod use 17172, schedule1-mod 17175.
const DEBUG_PORT: u16 = 17176;

// ---- Mod metadata + entry points ----

static MOD_INFO: ueforge::ModDef = ueforge::ModDef {
    name: "MISERY Mod",
    version: "0.1.0",
    log_file: "misery_mod.log",
    console_title: "MISERY Mod",
    console: cfg!(feature = "console"),
    on_unreal_init,
    on_shutdown,
    tabs: &[
        ueforge::TabDef {
            name: "Shining",
            render: shining::render,
        },
        ueforge::TabDef {
            name: "Speed",
            render: speed::render,
        },
        ueforge::TabDef {
            name: "Gameplay",
            render: gameplay::render,
        },
    ],
};

ueforge::ue4ss_mod!(MOD_INFO);

/// Starts MISERY's enabled features once Unreal and UE4SS are ready.
/// Stays here because this is the mod's composition root, selecting game features and verified engine settings.
fn on_unreal_init() {
    let _rt = ueforge::ue::platform::resolve_and_init(PROCESS_EVENT_IDX, G_OBJECTS_LAYOUT);

    // Our watchers enqueue onto the game-thread drain, which the
    // ProcessEvent hook serves. Hooks tear down at order 100, so
    // the watchers must be stopped before that or they tick on
    // into a drain nobody is draining and hold the DLL open.
    ueforge::shutdown::SHUTDOWN_REGISTRY.register(ueforge::shutdown::ShutdownHandlerDef {
        name: "misery::pollers",
        order: 50,
        run: || modforge::rpg::poller::shutdown_all(),
    });

    debug::spawn(DEBUG_PORT);

    // EVERY GAMEPLAY FEATURE IS OFF (2026-08-26).
    //
    // Turned off to find what actually touches the game, after a
    // run in which a world loaded with auto-load already
    // disabled. With only pe_dispatch left, the mod resolves
    // offsets, serves the game-thread queue and answers the
    // control plane, and does nothing else: no watchers, no
    // spawning, no props, no vendor writes, no data-table
    // changes, no notice press.
    //
    // Re-enable ONE at a time. The ops for harvest, rooms and
    // assets only register endpoints, so they are inert until
    // called and are the safe ones to restore first.
    ueforge::features()
        .once("pe_dispatch", dispatch::install)
        .once("spawning", spawning::install)
        .once("strange", strange::install)
        // Reading a level as pieces, measuring meshes and placing
        // pieces are all engine work, so they live in
        // `ueforge::ue::pieces` and register from there:
        // level_pieces, level_classes, mesh_info, place_pieces.
        // misery's harvest.rs is deleted; nothing of it was
        // game-specific.
        .once("pieces", ueforge::ue::pieces::register_ops)
        // .once("rooms", || ueforge::ue::rooms::register_ops(rooms::KIT))
        .once("assets", assets::register_ops)
        .once("nag", nag::install)
        // Auto-load is OFF. It called LoadLevel on
        // BP_HostLoadGameServer, the host-a-server path, and
        // started a FRESH world every launch rather than loading
        // the save: the map square names carry a world number and
        // it differed on every run (5760, 244, 10776, 15820).
        // Turn it back on once the singleplayer load path is
        // read off the menu. See docs/todo.md.
        // .once("autoload", autoload::install)
        .on_each_load("speed_default", Duration::from_secs(2),
            || ueforge::ue::actor::find_actor("BP_SGKMasterCharacter_C", None),
            |_| {
                if let Err(e) = speed::set_multiplier(2.0) {
                    ueforge::log::log(format_args!("speed_default: {e}"));
                }
            })
        .on_each_load("vendors", Duration::from_secs(3),
            || ueforge::ue::actor::find_actors_by_chain("BP_MasterVendorBuildPart_C")
                .into_iter().next(),
            vendors::apply_all)
        .install();
}

/// Records that the MISERY mod is shutting down cleanly.
/// Stays here because it is this mod's lifecycle callback; Ueforge owns the shared shutdown mechanism.
fn on_shutdown() {
    ueforge::log::log(format_args!("on_shutdown"));
}
