//! MISERY mod.
//!
//! Target: the emission ("shining") timer, so a mission gives
//! more time. See `docs/research.md`.
//!
//! Game-specific code here is deliberately thin: `ModDef`, the
//! init / shutdown hooks, the game's `PlatformOffsets`, and the
//! control plane. Everything else is `use ueforge::*`.

pub mod debug;
pub mod dispatch;
pub mod gameplay;
pub mod harvest;
pub mod places;
pub mod shining;
pub mod spawning;
pub mod speed;
pub mod strange;
pub mod vendors;

use std::time::Duration;
use ueforge::ue::GObjectsLayout;
use ueforge::ue::datatable::FieldTweak;

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

fn on_unreal_init() {
    let _rt = ueforge::ue::platform::resolve_and_init(PROCESS_EVENT_IDX, G_OBJECTS_LAYOUT);

    debug::spawn(DEBUG_PORT);

    ueforge::features()
        .once("pe_dispatch", dispatch::install)
        .once("spawning", spawning::install)
        .once("strange", strange::install)
        .once("harvest", harvest::register_ops)
        .once("places", places::install)
        .once("stack_10x", || {
            STACK_TWEAK.apply_when_ready(
                Duration::from_secs(30),
                |v: i32| v.saturating_mul(10),
                |v: i32| v <= 1,
            );
        })
        .on_each_load("suppress_nag", Duration::from_millis(500),
            || ueforge::ue::actor::find_object("WD_PlaytestNote01_C", None, false),
            |_| {
                std::thread::sleep(Duration::from_secs(2));
                ueforge::input::send_key(0x20);
            })
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

fn on_shutdown() {
    ueforge::log::log(format_args!("on_shutdown"));
}
