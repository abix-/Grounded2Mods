//! MISERY mod.
//!
//! Target: the emission ("shining") timer, so a mission gives
//! more time. See `docs/misery-research.md`.
//!
//! Game-specific code here is deliberately thin: `ModDef`, the
//! init / shutdown hooks, the game's `PlatformOffsets`, and the
//! control plane. Everything else is `use ueforge::*`.

pub mod debug;
pub mod gameplay;
pub mod shining;
pub mod speed;
pub mod vendors;

use std::time::Duration;
use ueforge::ue::{GObjectsLayout, PlatformOffsets};
use ueforge::ue::datatable::FieldTweak;

// MaxStack is Int (i32) at offset 0x44 within S_ItemDetails rows.
// See docs/misery-research.md section 23.
static STACK_TWEAK: FieldTweak<i32> = FieldTweak::new("ItemList", 0x44);

// ---- Platform detection ----
//
// Offsets are resolved dynamically via patternsleuth at init.
// The hardcoded STEAM block is a fallback if the resolver fails.
// process_event_idx 0x4C is the vtable slot for
// UObject::ProcessEvent, stable across UE 5.x.
const STEAM_FALLBACK: PlatformOffsets = PlatformOffsets {
    g_objects: 0x07A7_8ED0,
    append_string: 0x010D_C5E0,
    g_names: 0x079C_2180,
    g_world: 0x0,
    process_event: 0x012B_C1F0,
    process_event_idx: 0x4C,
    g_objects_layout: GObjectsLayout::WrappedChunked,
};

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
        ueforge::TabDef {
            name: "Scanner",
            render: ueforge::ui_scanner::render,
        },
        ueforge::TabDef {
            name: "Classes",
            render: ueforge::ui_class_browser::render,
        },
        ueforge::TabDef {
            name: "Structs",
            render: ueforge::ui_struct_browser::render,
        },
    ],
};

ueforge::ue4ss_mod!(MOD_INFO);

fn on_unreal_init() {
    let _rt = ueforge::ue::platform::resolve_and_init(&STEAM_FALLBACK);

    debug::spawn(DEBUG_PORT);

    STACK_TWEAK.apply_when_ready(
        Duration::from_secs(30),
        |v: i32| v.saturating_mul(10),
        |v: i32| v <= 1,
    );

    suppress_nag_screen();
    apply_speed_default();
    vendors::apply_on_load();
}

fn suppress_nag_screen() {
    ueforge::ue::actor::on_each_load(
        "suppress_nag",
        Duration::from_millis(500),
        || ueforge::ue::actor::find_object("WD_PlaytestNote01_C", None, false),
        |_widget| {
            std::thread::sleep(Duration::from_secs(2));
            synthesize_space();
        },
    );
}

fn synthesize_space() {
    #[repr(C)]
    struct RawInput {
        ty: u32,
        _pad0: u32,
        vk: u16,
        scan: u16,
        flags: u32,
        time: u32,
        _pad1: u32,
        extra: usize,
        _tail: [u8; 8],
    }
    unsafe extern "system" {
        fn SendInput(count: u32, inputs: *mut RawInput, size: i32) -> u32;
    }
    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const VK_SPACE: u16 = 0x20;
    let mk = |flags: u32| RawInput {
        ty: INPUT_KEYBOARD,
        _pad0: 0,
        vk: VK_SPACE,
        scan: 0,
        flags,
        time: 0,
        _pad1: 0,
        extra: 0,
        _tail: [0; 8],
    };
    let mut events = [mk(0), mk(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(
            events.len() as u32,
            events.as_mut_ptr(),
            std::mem::size_of::<RawInput>() as i32,
        );
    }
}

fn apply_speed_default() {
    ueforge::ue::actor::on_each_load(
        "speed_default",
        Duration::from_secs(2),
        || ueforge::ue::actor::find_actor("BP_SGKMasterCharacter_C", None),
        |_actor| {
            if let Err(e) = speed::set_multiplier(2.0) {
                ueforge::log::log(format_args!("speed_default: {e}"));
            }
        },
    );
}

fn on_shutdown() {
    ueforge::log::log(format_args!("on_shutdown"));
}
