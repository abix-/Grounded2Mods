// Debug HTTP endpoint and state snapshot.
//
// Single POST /debug endpoint, opt-in via `settings.debug.http_port`.
// Body: `{"op": "<name>", "args": {...}}`. Response always includes a
// full `state` snapshot so callers see post-op state in one round-trip.
//
// First op: `snapshot` (read-only). Future ops (skill_spend,
// skill_toggle, simulate_damage / simulate_heal, reload_slot) get
// queued onto a known-game-thread surface and drained from
// `kill_hook`'s PE trampoline. Read-only first because reads are safe
// off-thread and immediately useful for diagnosis.
//
// See `../../docs/todo.md` "Integration testing" for the full design and
// the bandage-regression test that gates the next milestone.

use serde::Serialize;
use serde_json::Value as Json;
use ueforge::debug::{CatalogEntry, DamageRing, PlayerStateView, ProcessSnapshot};
pub use ueforge::debug::DamageEvent;
use ueforge::envelope::handle_request;
use ueforge::ops::{OP_REGISTRY, OpDef};
use ueforge::pe_queue::GameThread;

use crate::inv_hook;
use crate::rpg::{apply, fall_hook, skills, tracker, world_loader};
use ueforge::ue::{ClassRef, UObject};

static HEALTH_CLASS: ClassRef = ClassRef::new("HealthComponent");

// PE-thread queue. The HTTP handler enqueues a command + a reply
// channel; `drain_pending()` is called from `kill_hook`'s
// trampoline (which UE calls on the game thread for every
// MulticastHandleEffectsWithDamageFlags event = many per second
// during gameplay). The trampoline drains the queue, executes
// each command on the game thread, and sends the result back via
// the channel. The HTTP handler blocks on `recv_timeout(5s)` so
// the response sees post-op state.
//
// This is the load-bearing piece for `simulate_*` ops: native
// UFunctions like `AddHealth` and `ApplyDamageFromInfo` are only
// safe to call via process_event on the game thread.

/// Game-thread job queue. The HTTP handler enqueues a closure
/// that runs from `drain_pending` on the game thread. See
/// `ueforge::pe_queue` for mechanics. ueforge's
/// `rpg::health::register` + `debug::register_pe_call` enqueue
/// onto this site; consumers can also enqueue their own jobs via
/// `ueforge::debug::enqueue_pe(&PE_QUEUE, ...)`.
pub(crate) static PE_QUEUE: GameThread = GameThread::new();

/// Ring buffer of recent damage / multicast events captured by
/// the `kill_hook` trampoline. Populated via [`record_damage_event`];
/// drained by the snapshot endpoint.
static DAMAGE_RING: DamageRing = DamageRing::new(64);

pub fn record_damage_event(ev: DamageEvent) {
    DAMAGE_RING.record(ev);
}

pub(crate) fn damage_ring_pushes() -> u64 {
    DAMAGE_RING.pushes()
}

pub(crate) fn damage_ring_peak() -> usize {
    DAMAGE_RING.peak()
}

const PE_TIMEOUT_HINT: &str =
    "Is the UEngine::Tick hook installed? (ueforge::game_thread::status)";

/// Register every g2rpg op + selector into the workspace
/// registries. Called once from `worker()` at init, BEFORE the
/// HTTP listener starts.
fn register_ops() {
    // Selectors first so the resolver this game passes to the op
    // registrars walks a registry that already contains both
    // framework + game-specific entries.
    ueforge::selector::register_builtins();
    ueforge::selector::SELECTOR_REGISTRY.register_many([
        ueforge::selector::SelectorDef {
            prefix: "live_player",
            summary: "First live ASurvivalCharacter (BP_SurvivalPlayerCharacter)",
            resolver: try_live_player,
        },
        ueforge::selector::SelectorDef {
            prefix: "live_player_hc",
            summary: "Live player's HealthComponent",
            resolver: try_live_player_hc,
        },
    ]);

    ueforge::ops::register_builtins();
    ueforge::ops::register_with_resolver(ueforge::selector::resolve);
    ueforge::debug::register_pe_call(
        &PE_QUEUE,
        PE_TIMEOUT_HINT,
        ueforge::selector::resolve,
    );
    ueforge::rpg::ops::register(&tracker::TRACKER);
    // Lift: simulate_add_health + simulate_set_current_health
    // are framework-shipped. g2rpg only declares the binding;
    // ueforge::rpg::health::register pushes both ops with the
    // Maine constants captured.
    ueforge::rpg::health::register(&HEALTH_BINDING, &PE_QUEUE, PE_TIMEOUT_HINT);

    // simulate_apply_damage stays game-side as a stub: the parm
    // shape is FDamageInfo (0x100 bytes) and the call re-enters
    // ProcessEvent through the engine's damage-replication path,
    // which crashes the host. Keeps the op in `list_ops` so tests
    // detect-and-skip cleanly. Lift when Wave E1 lands.
    OP_REGISTRY.register(OpDef::new(
        "simulate_apply_damage",
        "Disabled -- ApplyDamageFromInfo from a PE trampoline crashes the game",
        "{}",
        |args| op_simulate_apply_damage(args),
    ));
}

/// HC binding consumed by `ueforge::rpg::health::register`.
/// Offsets verified against Maine SDK
/// (`SDK/HealthComponent_classes.hpp`).
static HEALTH_BINDING: ueforge::rpg::health::HealthBinding =
    ueforge::rpg::health::HealthBinding {
        hc_class: &HEALTH_CLASS,
        hc_selector: "live_player_hc",
        current_damage_offset: 0x032C,
        max_health_offset: 0x0328,
        add_health_function: Some("AddHealth"),
        set_current_health_function: Some("SetCurrentHealth"),
    };

pub fn spawn(port: u16) {
    register_ops();
    // Serve the queue from UEngine::Tick, every frame, world or
    // no world. `drain_pending` from kill_hook / fall_hook stays
    // as a lower-latency path, but it only fires on gameplay
    // events: with the impact_resistance mask on, multicast drops
    // to near zero and call ops used to time out waiting for it.
    ueforge::game_thread::serve(&PE_QUEUE);
    ueforge::spawn(
        ueforge::Config {
            port,
            endpoint: "/debug",
            thread_name: "g2rpg-debug-http",
            auth_token: None,
        },
        |body| {
            let _t = ueforge::counters::time_scope(&crate::counters::TIME_NS_HTTP_HANDLE);
            ueforge::counters::bump(&crate::counters::HTTP_REQUESTS);
            let resp = handle_request(body, &OP_REGISTRY, build_snapshot);
            serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec())
        },
        |msg| ueforge::log!("{}", msg),
    );
}


fn op_simulate_apply_damage(_args: &Json) -> Result<Json, String> {
    // Disabled: calling ApplyDamageFromInfo via process_event from
    // any of our current PE trampolines (kill_hook, fall_hook,
    // inv_hook) hangs the game because the engine's damage
    // replication path re-enters ProcessEvent. The op stays in
    // the API surface (so tests can detect-and-skip cleanly) but
    // refuses with an explanation until the safe drain site
    // lands. See `../../docs/todo.md` "Endpoint parity gap".
    Err(
        "simulate_apply_damage temporarily disabled: \
         calling ApplyDamageFromInfo from inside a PE trampoline \
         crashes the game (replication re-entry). \
         Use simulate_add_health / simulate_set_current_health for \
         healing tests, or watch state.damage_ring while a real \
         bandage is used in-game."
            .to_string(),
    )
}

// ---- generic call primitive ----
//
// op: "call"
// args: { class, function, instance_selector, parms_hex }
// returns: { parms_hex_after, instance_kind }
//
// Tests build the parm bytes test-side (using #[repr(C)] structs
// matching the SDK), hex-encode, send. The mod calls
// process_event with the buffer and returns the (possibly
// engine-mutated) buffer back as hex. This is the test-side
// "do anything" surface.


fn try_live_player(s: &str) -> Option<Result<&'static UObject, String>> {
    if s != "live_player" {
        return None;
    }
    Some(
        unsafe { apply::PLAYER.first_live_static() }
            .ok_or_else(|| "no live player".to_string()),
    )
}

fn try_live_player_hc(s: &str) -> Option<Result<&'static UObject, String>> {
    if s != "live_player_hc" {
        return None;
    }
    Some(live_player_hc())
}

// ---- Game-thread executors. Called from kill_hook trampoline. ----

fn live_player_hc() -> Result<&'static UObject, String> {
    // SAFETY: we're on the game thread (inside the kill-hook PE
    // drain). The pawn + its HC live for the duration of the
    // closure that consumes them.
    let pawn = unsafe { apply::PLAYER.first_live_static() }
        .ok_or_else(|| "no live player".to_string())?;
    let hc = apply::read_component_ptr(pawn, apply::ASC_HEALTH_COMPONENT)
        .ok_or_else(|| "no live player HealthComponent".to_string())?;
    Ok(unsafe { std::mem::transmute::<&UObject, &'static UObject>(hc) })
}

#[allow(dead_code)] // kept as reference; tests should use `call` instead
fn exec_apply_damage(amount: f32, type_flags: u32) -> Result<Json, String> {
    use std::ffi::c_void;
    let hc = live_player_hc()?;
    let func = HEALTH_CLASS
        .find_function("ApplyDamageFromInfo")
        .ok_or_else(|| "ApplyDamageFromInfo UFunction not found".to_string())?;

    // Parm struct layout (Maine_parameters.hpp HealthComponent_ApplyDamageFromInfo):
    //   +0x000 Damage          f32  (OUT, the engine writes back the
    //                                applied damage after gates / mitigation)
    //   +0x004 Pad
    //   +0x008 FDamageEvent    16 bytes (Pad8 + DamageTypeClass)
    //   +0x018 FDamageInfo     0xE8 bytes
    // Total: 0x100. We allocate zeroed and only set the fields we care about.
    //
    // FDamageInfo layout (Maine_structs.hpp FDamageInfo, relative to parm+0x018):
    //   +0x70 DamageFlags    int32   (the gate consults this for the type-flag check)
    //   +0x78 OriginDamageData FDamageData (its first field is `float Damage`)
    const PARM_SIZE: usize = 0x100;
    const DAMAGE_OUT_OFFSET: usize = 0x000;
    const DAMAGE_INFO_OFFSET: usize = 0x018;
    const DAMAGE_FLAGS_REL: usize = 0x070;
    const ORIGIN_DAMAGE_DATA_REL: usize = 0x078;

    let mut parms = [0u8; PARM_SIZE];
    unsafe {
        // Damage value (input lives inside FDamageInfo.OriginDamageData.Damage).
        let damage_in_ptr = parms
            .as_mut_ptr()
            .add(DAMAGE_INFO_OFFSET + ORIGIN_DAMAGE_DATA_REL) as *mut f32;
        damage_in_ptr.write_unaligned(amount);

        // DamageFlags (the int32 the gate reads).
        let flags_ptr = parms
            .as_mut_ptr()
            .add(DAMAGE_INFO_OFFSET + DAMAGE_FLAGS_REL) as *mut u32;
        flags_ptr.write_unaligned(type_flags);
    }

    let cd_before = apply::read_f32(hc, 0x032C);
    let max_before = apply::read_f32(hc, 0x0328);

    unsafe {
        hc.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }

    let damage_out: f32 = unsafe {
        (parms.as_ptr().add(DAMAGE_OUT_OFFSET) as *const f32).read_unaligned()
    };
    let cd_after = apply::read_f32(hc, 0x032C);

    Ok(serde_json::json!({
        "amount_requested": amount,
        "type_flags_requested": format!("0x{type_flags:08X}"),
        "damage_out": damage_out,
        "current_damage_before": cd_before,
        "current_damage_after": cd_after,
        "current_damage_delta": cd_after - cd_before,
        "max_health": max_before,
    }))
}

#[derive(Serialize)]
pub struct Snapshot {
    pub slot: Option<String>,
    pub player_state: Option<PlayerStateView>,
    pub disabled_skills: Vec<&'static str>,
    pub vanilla: VanillaView,
    pub live_player: Option<LivePlayerView>,
    pub cdo: CdoView,
    pub inventory: InventoryView,
    /// Status effects currently active on the player, by data
    /// table row. Drives status-effect-migration research and
    /// regression tests for skills that interact with the engine's
    /// canonical `UStatusEffectComponent` surface.
    pub status_effects: Option<Vec<StatusEffectView>>,
    /// Catalog dump (server-authoritative). Tests iterate this
    /// instead of hardcoding skill ids; new skills get coverage
    /// automatically.
    pub catalog: Vec<CatalogEntry>,
    /// Settings echo: what the mod loaded from settings.json.
    /// Tests verify settings.json was honored without re-parsing
    /// the file from the test side.
    pub settings: SettingsView,
    /// Recent damage / multicast events captured by the kill_hook
    /// trampoline. Read-only (no process_event re-entry).
    pub damage_ring: Vec<DamageEvent>,
    /// System-metric block: counters + process memory / cpu /
    /// threads + GObjects population + process regions. Flattened
    /// so existing test assertions on the top-level field names
    /// keep working.
    #[serde(flatten)]
    pub process: ProcessSnapshot,
}

#[derive(Serialize)]
pub struct SettingsView {
    pub inventory_slot_count: i32,
    pub thirst_multiplier: f32,
    pub hunger_multiplier: f32,
    pub buggy_kill_xp_multiplier: f32,
    pub debug_http_port: Option<u16>,
}

#[derive(Serialize)]
pub struct StatusEffectView {
    pub row: String,
    pub table: String,
    pub stat_type: Option<u8>,
    pub value: Option<f32>,
}

#[derive(Serialize)]
pub struct InventoryView {
    /// Slot count currently applied via the backpack patch. The
    /// inv_hook also stores this as the in-memory authoritative
    /// view that the inventory UI reads.
    pub current_slot_count: i32,
}

#[derive(Serialize)]
pub struct VanillaView {
    pub hunger: Option<f32>,
    pub thirst: Option<f32>,
    pub fall_damage_ratio: Option<f32>,
    pub max_health: Option<f32>,
    pub hc_u32_masks: Vec<HcMaskView>,
}

#[derive(Serialize)]
pub struct HcMaskView {
    pub offset: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct LivePlayerView {
    pub instance_count: usize,
    /// Reads of well-known UHealthComponent fields on the first
    /// live player encountered. Multiple players (multi-character
    /// switch) are typically all the same character at runtime, so
    /// the first one is representative.
    pub hc: Option<HcFields>,
    /// CharMovementComponent fields. Drives the per-skill
    /// expectations for move_speed, jump_height, leap_distance,
    /// glide_speed (and the per-character fall_resistance writes).
    pub cmc: Option<CmcFields>,
    /// ASurvivalCharacter-level fields (combat multipliers, fall
    /// damage trio).
    pub asc: Option<AscFields>,
}

#[derive(Serialize)]
pub struct HcFields {
    pub required_damage_type_flags: String,
    pub immunity_flags: String,
    pub base_damage_reduction: f32,
    pub max_health: f32,
    pub current_damage: f32,
}

#[derive(Serialize)]
pub struct CmcFields {
    pub max_walk_speed: f32,
    pub max_sprint_speed: f32,
    pub max_swim_speed: f32,
    pub max_swim_sprint_speed: f32,
    pub max_fly_speed: f32,
    pub jump_z_velocity: f32,
    pub air_control: f32,
    pub air_control_boost_multiplier: f32,
    pub air_control_boost_velocity_threshold: f32,
    pub custom_ground_speed_multiplier: f32,
    pub custom_fly_speed_multiplier: f32,
    pub custom_swim_speed_multiplier: f32,
}

#[derive(Serialize)]
pub struct AscFields {
    pub custom_damage_multiplier: f32,
    pub take_fall_damage: bool,
    pub minimum_fall_damage_velocity: f32,
    pub fall_damage_ratio: f32,
}

/// Per-class-default-object snapshot: drain rates, combat scalars,
/// regen knobs. Each sub-struct is None when the class isn't loaded
/// or vanilla hasn't been observed yet.
#[derive(Serialize)]
pub struct CdoView {
    pub survival: Option<SurvivalCdoFields>,
    pub global_combat_data: Option<GlobalCombatDataFields>,
    pub game_mode_settings: Option<GmsFields>,
    pub survival_mode_manager: Option<SmmcFields>,
}

#[derive(Serialize)]
pub struct SurvivalCdoFields {
    pub hunger_adjustment_per_second: f32,
    pub thirst_adjustment_per_second: f32,
}

#[derive(Serialize)]
pub struct GlobalCombatDataFields {
    pub combat_regen_delay: f32,
    pub combat_regen_tick_percentage: f32,
    pub combat_regen_tick_rate: f32,
}

#[derive(Serialize)]
pub struct GmsFields {
    pub fall_damage_multiplier: f32,
}

#[derive(Serialize)]
pub struct SmmcFields {
    pub custom_fall_damage_multiplier: f32,
}

fn build_snapshot() -> Snapshot {
    let slot = tracker::current_slot();
    let player_state_view = tracker::with_state(PlayerStateView::from_state);

    let vanilla_masks = apply::vanilla_hc_masks_snapshot()
        .into_iter()
        .map(|(o, v)| HcMaskView {
            offset: format!("0x{:X}", o),
            value: format!("0x{:08X}", v),
        })
        .collect();

    let vanilla = VanillaView {
        hunger: apply::vanilla_hunger(),
        thirst: apply::vanilla_thirst(),
        fall_damage_ratio: apply::vanilla_fall_damage_ratio(),
        max_health: apply::vanilla_max_health(),
        hc_u32_masks: vanilla_masks,
    };

    let live_player = collect_live_player();
    let cdo = collect_cdo();
    let inventory = InventoryView {
        current_slot_count: inv_hook::current_slot_count(),
    };
    let status_effects = fall_hook::snapshot_player_status_effects().map(|v| {
        v.into_iter()
            .map(|e| StatusEffectView {
                row: e.row,
                table: e.table,
                stat_type: e.stat_type,
                value: e.value,
            })
            .collect()
    });

    let catalog = ueforge::debug::catalog_view(skills::CATALOG.entries());

    let settings = world_loader::loaded_settings()
        .map(|s| SettingsView {
            inventory_slot_count: s.inventory.slot_count,
            thirst_multiplier: s.survival.thirst_multiplier,
            hunger_multiplier: s.survival.hunger_multiplier,
            buggy_kill_xp_multiplier: s.rpg.buggy_kill_xp_multiplier,
            debug_http_port: s.debug.http_port,
        })
        .unwrap_or(SettingsView {
            inventory_slot_count: 0,
            thirst_multiplier: 0.0,
            hunger_multiplier: 0.0,
            buggy_kill_xp_multiplier: 0.0,
            debug_http_port: None,
        });

    Snapshot {
        slot,
        player_state: player_state_view,
        disabled_skills: apply::disabled_skills_snapshot(),
        vanilla,
        live_player,
        cdo,
        inventory,
        status_effects,
        catalog,
        settings,
        damage_ring: DAMAGE_RING.snapshot(),
        process: ProcessSnapshot::collect(crate::counters::snapshot_json(), 40),
    }
}

fn collect_live_player() -> Option<LivePlayerView> {
    let mut count = 0usize;
    let mut hc: Option<HcFields> = None;
    let mut cmc: Option<CmcFields> = None;
    let mut asc: Option<AscFields> = None;
    apply::apply_to_live_player_characters(|player| {
        count += 1;
        if hc.is_none()
            && let Some(hc_obj) = apply::read_component_ptr(player, apply::ASC_HEALTH_COMPONENT)
        {
            hc = Some(read_hc_fields(hc_obj));
        }
        if cmc.is_none()
            && let Some(cmc_obj) =
                apply::read_component_ptr(player, apply::ASC_CHAR_MOVEMENT_COMPONENT)
        {
            cmc = Some(read_cmc_fields(cmc_obj));
        }
        if asc.is_none() {
            asc = Some(read_asc_fields(player));
        }
    });
    if count == 0 {
        None
    } else {
        Some(LivePlayerView {
            instance_count: count,
            hc,
            cmc,
            asc,
        })
    }
}

fn read_cmc_fields(cmc: &UObject) -> CmcFields {
    use crate::rpg::skills::*;
    CmcFields {
        max_walk_speed: apply::read_f32(cmc, CMC_MAX_WALK_SPEED),
        max_sprint_speed: apply::read_f32(cmc, CMC_MAX_SPRINT_SPEED),
        max_swim_speed: apply::read_f32(cmc, CMC_MAX_SWIM_SPEED),
        max_swim_sprint_speed: apply::read_f32(cmc, CMC_MAX_SWIM_SPRINT_SPEED),
        max_fly_speed: apply::read_f32(cmc, CMC_MAX_FLY_SPEED),
        jump_z_velocity: apply::read_f32(cmc, CMC_JUMP_Z_VELOCITY),
        air_control: apply::read_f32(cmc, CMC_AIR_CONTROL),
        air_control_boost_multiplier: apply::read_f32(cmc, CMC_AIR_CONTROL_BOOST_MULT),
        air_control_boost_velocity_threshold: apply::read_f32(cmc, CMC_AIR_CONTROL_BOOST_THRESHOLD),
        custom_ground_speed_multiplier: apply::read_f32(cmc, CMC_CUSTOM_GROUND_SPEED_MULTIPLIER),
        custom_fly_speed_multiplier: apply::read_f32(cmc, CMC_CUSTOM_FLY_SPEED_MULTIPLIER),
        custom_swim_speed_multiplier: apply::read_f32(cmc, CMC_CUSTOM_SWIM_SPEED_MULTIPLIER),
    }
}

fn read_asc_fields(asc: &UObject) -> AscFields {
    use crate::rpg::skills::*;
    AscFields {
        custom_damage_multiplier: apply::read_f32(asc, ASC_CUSTOM_DAMAGE_MULTIPLIER),
        take_fall_damage: unsafe {
            (asc.field_ptr(ASC_TAKE_FALL_DAMAGE) as *const u8).read() != 0
        },
        minimum_fall_damage_velocity: apply::read_f32(asc, ASC_MINIMUM_FALL_DAMAGE_VELOCITY),
        fall_damage_ratio: apply::read_f32(asc, ASC_FALL_DAMAGE_RATIO),
    }
}

fn collect_cdo() -> CdoView {
    use crate::rpg::skills::*;

    let mut survival = None;
    ueforge::ue::with_first_cdo_of("SurvivalComponent", |obj| {
        survival = Some(SurvivalCdoFields {
            hunger_adjustment_per_second: apply::read_f32(obj, SURVIVAL_HUNGER_OFFSET),
            thirst_adjustment_per_second: apply::read_f32(obj, SURVIVAL_THIRST_OFFSET),
        });
    });

    let mut gcd = None;
    ueforge::ue::with_first_instance_of("GlobalCombatData", |obj| {
        gcd = Some(GlobalCombatDataFields {
            combat_regen_delay: apply::read_f32(obj, GCD_COMBAT_REGEN_DELAY),
            combat_regen_tick_percentage: apply::read_f32(obj, GCD_COMBAT_REGEN_TICK_PERCENTAGE),
            combat_regen_tick_rate: apply::read_f32(obj, GCD_COMBAT_REGEN_TICK_RATE),
        });
    });

    let mut gms = None;
    ueforge::ue::with_first_instance_of("SurvivalGameModeSettings", |obj| {
        gms = Some(GmsFields {
            fall_damage_multiplier: apply::read_f32(obj, GMS_FALL_DAMAGE_MULTIPLIER),
        });
    });

    let mut smmc = None;
    ueforge::ue::with_first_instance_of("SurvivalModeManagerComponent", |obj| {
        smmc = Some(SmmcFields {
            custom_fall_damage_multiplier: apply::read_f32(obj, SMMC_CUSTOM_FALL_DAMAGE_MULTIPLIER),
        });
    });

    CdoView {
        survival,
        global_combat_data: gcd,
        game_mode_settings: gms,
        survival_mode_manager: smmc,
    }
}

fn read_hc_fields(hc: &UObject) -> HcFields {
    // Offsets verified against Maine_classes.hpp + skills.rs.
    const HC_IMMUNITY_FLAGS: usize = 0x00F8;
    const HC_BASE_DAMAGE_REDUCTION: usize = 0x00EC;
    const HC_REQUIRED_DAMAGE_TYPE_FLAGS: usize = 0x00FC;
    const HC_MAX_HEALTH: usize = 0x0328;
    const HC_CURRENT_DAMAGE: usize = 0x032C;
    HcFields {
        required_damage_type_flags: format!(
            "0x{:08X}",
            apply::read_u32(hc, HC_REQUIRED_DAMAGE_TYPE_FLAGS)
        ),
        immunity_flags: format!("0x{:08X}", apply::read_u32(hc, HC_IMMUNITY_FLAGS)),
        base_damage_reduction: apply::read_f32(hc, HC_BASE_DAMAGE_REDUCTION),
        max_health: apply::read_f32(hc, HC_MAX_HEALTH),
        current_damage: apply::read_f32(hc, HC_CURRENT_DAMAGE),
    }
}

/// All registered skill ids in the catalog. Exposed so docs and
/// tests can iterate the canonical list without re-deriving it.
#[allow(dead_code)]
pub fn catalog_skill_ids() -> Vec<&'static str> {
    skills::CATALOG.iter().map(|s| s.id).collect()
}
