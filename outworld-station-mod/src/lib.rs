//! Outworld Station Tweaks.
//!
//! Multi-mod scaffold. First feature: item stack tweaks. Layered
//! on top of ueforge's UE4SS / debug / UI infrastructure.
//!
//! Game-specific code in this crate is intentionally minimal:
//! `ModInfo`, the on_unreal_init / on_shutdown hooks, the
//! game's `PlatformOffsets`, the `Snapshot` type, the op
//! dispatcher, and the tab render bodies. Everything else
//! is `use ueforge::*`.

pub mod debug;
pub mod settings;
pub mod stacks;

use ueforge::ue::{GObjectsLayout, PlatformOffsets};

// ---- Platform detection ----
//
// OutworldStation-Win64-Shipping.exe (Steam, UE 5.4.4).
// Image base + addresses captured by UE4SS's pattern scanner on
// 2026-05-09 (`UE4SS.log` has the absolute addresses; image
// base 0x7ff6da6f0000 from our own log). Image-relative offsets
// computed by subtracting the base.
//
// g_names: UE4SS didn't log an explicit address for this on
// this exe. Fill in if needed, or let patternsleuth resolve it.
//
// process_event_idx 0x4C: vtable slot for UObject::ProcessEvent,
// stable across UE 5.x.
const STEAM: PlatformOffsets = PlatformOffsets {
    g_objects: 0x07A9_38D0,    // GUObjectArray
    append_string: 0x010D_F9D0, // FName::ToString
    g_names: 0x0,               // not logged by UE4SS scanner; fill in if needed
    process_event_idx: 0x4C,
    // UE 5.4 stock. FUObjectArray wraps FChunkedFixedUObjectArray
    // at +0x10. Verified live: NumElements=142650, NumChunks=3.
    g_objects_layout: GObjectsLayout::WrappedChunked,
};

const PLATFORMS: &[(&str, &PlatformOffsets)] = &[
    ("OutworldStation-Win64-Shipping.exe", &STEAM),
];

// ---- Mod metadata + entry points ----

static MOD_INFO: ueforge::ModDef = ueforge::ModDef {
    name: "Outworld Station Mod",
    version: "0.1.0",
    log_file: "outworld_station_mod.log",
    console_title: "Outworld Station Mod",
    console: cfg!(feature = "console"),
    on_unreal_init,
    on_shutdown,
    tabs: &[
        ueforge::TabDef {
            name: "Tweaks",
            render: render_tweaks_tab,
        },
        ueforge::TabDef {
            name: "Scanner",
            render: ueforge::ui_scanner::render,
        },
        ueforge::TabDef {
            name: "Tables",
            render: ueforge::ui_data_table_browser::render,
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
    ueforge::log::log(format_args!("on_unreal_init"));

    let image_base = ueforge::ue::platform::host_image_base();
    let exe = ueforge::ue::platform::host_exe_name().unwrap_or_default();
    ueforge::log::log(format_args!(
        "image_base = 0x{image_base:x}, host = {exe}"
    ));

    // Once OWS offsets are dumped: detect platform, init runtime,
    // spawn debug server, install drain trampoline.
    let offsets = ueforge::ue::platform::detect(PLATFORMS);
    if offsets.is_none() || offsets.is_some_and(|o| o.g_objects == 0) {
        ueforge::log::log(format_args!(
            "WARN: OWS PlatformOffsets are stub zeros. Dump via \
             UE4SS CXXHeaderDumper, fill in src/lib.rs::STEAM, \
             then rebuild. /debug snapshot still works."
        ));
    } else if let Some(off) = offsets {
        let _rt = unsafe { ueforge::ue::init_runtime(image_base, off) };
        ueforge::log::log(format_args!(
            "ue runtime ready, GObjects = 0x{:x}",
            image_base + off.g_objects
        ));

        // Walk GObjects once and cache every UDataTable / UClass /
        // UScriptStruct for the discover_* ops + future TweakDef
        // offset resolution.
        let _ = ueforge::discovery::run_at_load();

        // Spawn the stack-bump worker. Polls for DT_Materials,
        // mutates MaxCanStack 4x on first sight, exits. Running
        // this BEFORE any save loads is what makes the mutation
        // propagate to all downstream caches (UI widget copies,
        // inventory slot init, etc.).
        stacks::spawn_apply_worker();

        // Apply every settings.json dynamic tweak when its target
        // table loads. At init time data tables haven't streamed
        // in yet, so we mirror the stacks worker pattern: one
        // on_first_sight worker per unique table.
        let s = settings::get().get();
        ueforge::dynamic_tweaks::apply_all_when_ready(
            &s.dynamic_tweaks,
            std::time::Duration::from_secs(60),
        );
    }

    // Always bring the debug server up (offset-independent). Use
    // a fixed port for now; later, read from settings.json.
    debug::spawn(17172);
}

fn on_shutdown() {
    ueforge::log::log(format_args!("on_shutdown"));
}

// ---- ImGui tab render ----

fn render_tweaks_tab() {
    ueforge::ui::text("Outworld Station Tweaks");
    ueforge::ui::separator();
    ueforge::ui::spacing();
    render_stacks_section();
    ueforge::ui::spacing();
    ueforge::ui::separator();
    ueforge::ui::spacing();
    render_dynamic_tweaks_section();
}

fn render_dynamic_tweaks_section() {
    let s = settings::get().get();
    ueforge::ui_dynamic_tweaks::render(&s.dynamic_tweaks, || {
        let _ = settings::get().reload();
    });
}

fn render_stacks_section() {
    ueforge::ui::text("Item stack multiplier");
    ueforge::ui::text_disabled(
        "Multiplies every DT_Materials.MaxCanStack by this factor. \
         Applied once at game start; tweak + click Apply to re-run \
         live. Equipment items (vanilla cap 1) stay non-stackable.",
    );
    ueforge::ui::spacing();

    // Slider state lives in stacks::MULTIPLIER. Read, present, write.
    let mut m = stacks::current_multiplier();
    ueforge::ui::set_next_item_width(220.0);
    if ueforge::ui::slider_i32(
        "Multiplier (x vanilla)",
        &mut m,
        stacks::MIN_MULTIPLIER,
        stacks::MAX_MULTIPLIER,
    ) {
        stacks::set_multiplier(m);
    }

    ueforge::ui::spacing();
    if ueforge::ui::button("Apply") {
        match stacks::apply_now() {
            Ok(n) => ueforge::log::log(format_args!(
                "stacks: tab applied {}x to {n} rows",
                stacks::current_multiplier()
            )),
            Err(e) => ueforge::log::log(format_args!("stacks: tab apply failed: {e}")),
        }
    }
    ueforge::ui::same_line();
    if ueforge::ui::button("Reset to 1x") {
        stacks::set_multiplier(1);
        match stacks::apply_now() {
            Ok(n) => ueforge::log::log(format_args!("stacks: tab reset to vanilla on {n} rows")),
            Err(e) => ueforge::log::log(format_args!("stacks: tab reset failed: {e}")),
        }
    }

    ueforge::ui::spacing();
    ueforge::ui::separator();
    ueforge::ui::text_disabled("Status");
    if stacks::ever_applied() {
        ueforge::ui::text(&format!(
            "Current: {}x   Last apply: {} rows",
            stacks::current_multiplier(),
            stacks::last_applied_rows(),
        ));
        ueforge::ui::text(&format!(
            "Vanilla baseline captured: {} rows",
            stacks::vanilla_count(),
        ));
    } else {
        ueforge::ui::text_disabled("Not applied yet — DT_Materials still loading?");
    }
}
