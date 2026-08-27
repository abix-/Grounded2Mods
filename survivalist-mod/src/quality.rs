//! The quality system (docs/status.md "Quality system (higher
//! quality, lower chance)"; research + design in faction-war.md
//! "The quality system").
//!
//! Quality tiers are distinct item TYPES: the engine has no
//! per-item quality field and stacks merge by type, so a tiered
//! item is a separate, better, rarer prototype, and everything
//! downstream (price, damage, saves, names) rides vanilla. The
//! full variant set is GENERATED, never hand-authored:
//! scripts/generate_quality.ps1 reads every vanilla weapon and
//! armor definition and writes <Base>_<Tier><Sibling>.xml into
//! story/Equipment with per-tier stat/price/recoil MULTIPLIERS
//! (the knobs live in that script). Factorio naming: Uncommon,
//! Rare, Epic, Legendary above the vanilla Normal; named uniques
//! (unique.rs) sit above the whole ladder.
//!
//! Each tier ships several statistical SIBLINGS with jittered
//! stats sharing one display name, so two Rare rifles are
//! usually not exactly the same (real per-item stat ranges are
//! impossible: stats live on the type).
//!
//! THE EDGE ROLLS QUALITY: every weapon and armor piece in an
//! edge-spawned band's hands rolls a tier independently, with
//! odds set by the sender (military remnants roll best, raiders
//! lower). The swap is net zero items (Take + Delete the common
//! piece, Equipment.Spawn the tiered one into the same hand) and
//! runs ONLY on edge-spawned bands, the sanctioned faucet of the
//! no-cheating boundary. Rust keeps no item lists: the variant
//! type name is derived by convention and the swap happens only
//! if that type exists.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::crafting::advance_pending_results;
use modforge::ops::{OP_REGISTRY, OpDef};
use modforge::quality::{roll_sibling, roll_tier};
use unityforge::hook::{self, HOOK_REGISTRY, HookCtx};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

use crate::common::{ctype, for_each_community, handle_of, own, with};

/// Tier names, best first (Factorio naming). Must match the
/// generator's $Tiers.
const TIER_NAMES: [&str; 4] = ["Legendary", "Epic", "Rare", "Uncommon"];

/// Per-sender tier odds in PER MILLE, best tier first, evaluated
/// cumulatively from the top. Military remnants carry the best;
/// raiders roll lower; the roving traders' shop stock rolls
/// generously (a shop worth checking). Higher quality, lower
/// chance.
const MILITARY_ODDS: [u64; 4] = [10, 40, 100, 200]; // 1%, 4%, 10%, 20%
const RAIDER_ODDS: [u64; 4] = [3, 15, 50, 120]; // 0.3%, 1.5%, 5%, 12%
const TRADER_ODDS: [u64; 4] = [4, 20, 60, 150]; // 0.4%, 2%, 6%, 15%

/// Seconds between scans for roving traders not yet rolled.
const TRADER_SCAN_PERIOD_SECS: f32 = 60.0;

/// Statistical siblings per tier. Must match the generator's
/// $Siblings.
const SIBLINGS: u64 = 3;

/// A generated prototype that always exists when the variant set
/// is loaded; its absence means the story has not loaded our
/// Equipment XML yet (loads at story restart).
const CANARY: &str = "AssaultRifle_Uncommon1";

/// Swaps per tier (indexed like TIER_NAMES), for quality_status.
static SWAPS: [AtomicU32; 4] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];
static LAST_SWAP: Mutex<Option<String>> = Mutex::new(None);
/// The data-not-loaded line logs once per generation.
static MISSING_LOGGED: AtomicU32 = AtomicU32::new(0);
/// Roving-trader community ids already rolled this generation
/// (their fresh stock rolls ONCE, when first seen).
static ROLLED_TRADERS: Mutex<Vec<i64>> = Mutex::new(Vec::new());
static LAST_TRADER_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
/// Game clock as of the last tick, for the craft hooks (hooks
/// have no `now`).
static LAST_NOW_BITS: AtomicU32 = AtomicU32::new(0);

// ---- hands roll quality (crafting) ---------------------------------------------

/// Seconds after the craft call before the job looks for the
/// product in the crafter's hands (creation is synchronous; the
/// margin covers a frame or two of settling).
const CRAFT_SETTLE_SECS: f32 = 2.0;

/// A job that never finds its product is dropped (liquid
/// products, container-routed output, or the crafter deposited
/// it faster than we looked).
const CRAFT_JOB_TIMEOUT_SECS: f32 = 30.0;

/// The recipe half of a craft event, set by the Instance-ctx
/// prefix and consumed by the Arg0-ctx prefix on the same call
/// (Harmony runs them back to back; a missed pairing skips the
/// roll, never mis-rolls).
static PENDING_RECIPE: Mutex<Option<(String, String, i64)>> = Mutex::new(None); // product, skill type, recipe level

/// One queued craft roll: the crafter (handle owned by the job),
/// what they made, and how far their skill clears the recipe.
struct CraftJob {
    crafter_h: i32,
    product: String,
    surplus: i64,
    ready_at: f32,
    deadline: f32,
}

static CRAFT_JOBS: Mutex<Vec<CraftJob>> = Mutex::new(Vec::new());
static CRAFT_ROLLS: AtomicU32 = AtomicU32::new(0);

/// Install the craft hooks: two prefixes on the game's ONE
/// product-creation entry (Recipe.UseIngredientsAndCreateProduct),
/// the first reading the recipe (instance), the second the
/// crafter (arg0, a Character). Registration order pairs them.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
pub fn install() {
    match hook::patch_prefix_ctx(
        "Recipe",
        "UseIngredientsAndCreateProduct",
        HookCtx::Instance,
        on_craft_recipe,
    ) {
        Ok(h) => HOOK_REGISTRY.register(h),
        Err(e) => {
            mono::log(
                LogLevel::Error,
                &format!("survivalist-mod: quality: craft recipe hook FAILED: {e}"),
            );
            return;
        }
    }
    match hook::patch_prefix_ctx(
        "Recipe",
        "UseIngredientsAndCreateProduct",
        HookCtx::Arg0,
        on_craft_carrier,
    ) {
        Ok(h) => {
            HOOK_REGISTRY.register(h);
            mono::log(
                LogLevel::Info,
                "survivalist-mod: quality: hands roll quality (craft hooks installed)",
            );
        }
        Err(e) => {
            mono::log(
                LogLevel::Error,
                &format!("survivalist-mod: quality: craft carrier hook FAILED: {e}"),
            );
        }
    }
}

/// Prefix 1 (instance = the Recipe): remember what is being made
/// and what skill it asks for.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
extern "C" fn on_craft_recipe(ctx: *const c_void) -> i32 {
    let h = ctx as isize as i32;
    if h == 0 {
        return 0;
    }
    let recipe = own(h);
    let mut product = recipe
        .read_field("ProductPrototypeName")
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    if product.is_empty() {
        product = recipe
            .read_field("ProductPrototype")
            .ok()
            .as_ref()
            .and_then(handle_of)
            .and_then(|ph| {
                let p = own(ph);
                p.read_field("Name")
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
            })
            .unwrap_or_default();
    }
    if product.is_empty() {
        return 0; // liquid or prop recipe; nothing to roll
    }
    let skill_type = recipe
        .read_field("SkillType")
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let recipe_level = recipe
        .read_field("SkillLevel")
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    *PENDING_RECIPE.lock() = Some((product, skill_type, recipe_level));
    0
}

/// Prefix 2 (arg0 = the carrier Character): pair with the pending
/// recipe and queue the roll for after the product exists.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
extern "C" fn on_craft_carrier(ctx: *const c_void) -> i32 {
    let Some((product, skill_type, recipe_level)) = PENDING_RECIPE.lock().take() else {
        return 0;
    };
    let h = ctx as isize as i32;
    if h == 0 {
        return 0;
    }
    let crafter = own(h);
    let level = if skill_type.is_empty() {
        0
    } else {
        crafter
            .invoke("GetSkillLevelWithEffects", &json!([skill_type]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    };
    let surplus = (level - recipe_level).max(0);
    let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
    std::mem::forget(crafter); // the job owns the handle
    CRAFT_JOBS.lock().push(CraftJob {
        crafter_h: h,
        product,
        surplus,
        ready_at: now + CRAFT_SETTLE_SECS,
        deadline: now + CRAFT_JOB_TIMEOUT_SECS,
    });
    0
}

/// Skill-scaled tier odds (per mille, best first). A novice never
/// rolls Legendary; a master's hands are worth fighting over.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
fn craft_odds(surplus: i64) -> [u64; 4] {
    let s = surplus.clamp(0, 8) as u64;
    [s, 3 * (1 + s), 12 * (1 + s), 40 * (1 + s)]
}

/// Walk the queued craft rolls: find the product in the crafter's
/// hands, roll the tier by skill, swap on a hit.
/// Stays here because Survivalist owns product discovery, tier policy,
/// inventory swaps, and queue state; Modforge owns only the existing
/// collection advancement and quality rolls.
fn process_craft_jobs(now: f32) {
    let mut jobs = CRAFT_JOBS.lock();
    advance_pending_results(
        &mut jobs,
        now,
        |job| (job.ready_at, job.deadline),
        try_craft_roll,
        |_job, error| {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: quality: craft roll failed: {error}"),
            );
        },
        |job| drop(own(job.crafter_h)),
    );
}

/// Ok(true) = job resolved (rolled, missed, or product gone).
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
fn try_craft_roll(job: &CraftJob, now: f32) -> Result<bool, String> {
    let Some(inv_h) = with(job.crafter_h, |c| {
        c.read_field("Inventory").ok().as_ref().and_then(handle_of)
    }) else {
        return Ok(true);
    };
    let inv = own(inv_h);
    let n = inv.list_len_or_zero()?;
    for i in 0..n {
        let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([i]))?) else {
            continue;
        };
        let item = own(item_h);
        let name = handle_of(&item.invoke("GetPrototype", &json!([]))?).and_then(|ph| {
            let p = own(ph);
            p.read_field("Name")
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
        });
        if name.as_deref() != Some(job.product.as_str()) {
            continue;
        }
        // The product is in hand: one roll, hit or miss. An
        // upgraded work prop's Quality track (settlement
        // upgrades, C# side) adds to the crafter's surplus:
        // better benches make finer things.
        CRAFT_ROLLS.fetch_add(1, Ordering::Relaxed);
        let crafter_id = with(job.crafter_h, |c| {
            c.read_field("Id")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1)
        });
        let prop_bonus = mono::invoke_static(
            "SettlementUpgrades",
            "TakeCraftQualityBonus",
            &json!([crafter_id]),
        )
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
        let salt = (job.crafter_h as u64).wrapping_mul(53) ^ 0xC0FFEE;
        let odds = craft_odds(job.surplus + prop_bonus);
        let Some(tier_ix) = roll_tier(&odds, now, salt) else {
            return Ok(true); // common hands, common work
        };
        let who = with(job.crafter_h, |c| {
            c.invoke("GetDisplayNameString", &json!([]))
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "<unnamed>".into())
        });
        let swapped = swap_to_variant(
            job.crafter_h,
            &inv,
            item_h,
            &job.product,
            tier_ix,
            now,
            salt,
            &who,
            "crafted",
        )?;
        let _ = swapped;
        return Ok(true);
    }
    Ok(false) // not in hand yet; retry until the deadline
}

/// Is the generated variant data loaded? One canary lookup; the
/// not-loaded line logs once per generation.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
fn variants_loaded() -> bool {
    match find_prototype(CANARY) {
        Ok(Some(h)) => {
            drop(own(h));
            true
        }
        Ok(None) => {
            if MISSING_LOGGED.swap(1, Ordering::Relaxed) == 0 {
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: quality: variant data not loaded; nothing rolls (restart the story to load the generated Equipment XML)",
                );
            }
            false
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: quality: canary lookup failed: {e}"),
            );
            false
        }
    }
}

/// The edge roll: every weapon or armor piece carried by the band
/// rolls a tier by the sender's odds. Called by incursion.rs
/// right after an edge band spawns; best-effort (a failure leaves
/// the band exactly as the game spawned it).
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
pub fn upgrade_band_gear(band_h: i32, now: f32, military: bool) {
    if !variants_loaded() {
        return;
    }
    let odds = if military {
        &MILITARY_ODDS
    } else {
        &RAIDER_ODDS
    };
    if let Err(e) = roll_band(band_h, odds, now, "edge band") {
        mono::log(
            LogLevel::Warn,
            &format!("survivalist-mod: quality: edge roll failed: {e}"),
        );
    }
}

/// The shop rolls too: a vanilla roving trader is an ambient
/// arrival (the world feeding the map, same boundary as the edge
/// bands), so its fresh stock rolls tiers ONCE when the trader is
/// first seen. Already-tiered items never re-roll (the underscore
/// guard), so a hot reload cannot inflate a trader twice beyond
/// re-rolling what stayed common.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
pub fn tick(now: f32) {
    LAST_NOW_BITS.store(now.to_bits(), Ordering::Relaxed);
    // Queued craft rolls settle on their own clock, every pass.
    if !CRAFT_JOBS.lock().is_empty() {
        process_craft_jobs(now);
    }
    let last = f32::from_bits(LAST_TRADER_SCAN_BITS.load(Ordering::Relaxed));
    if now - last < TRADER_SCAN_PERIOD_SECS {
        return;
    }
    LAST_TRADER_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
    if !variants_loaded() {
        return;
    }
    let mut seen: Vec<i64> = Vec::new();
    let _ = for_each_community(|com| {
        if ctype(&com) != "RovingTrader" {
            return Ok(true);
        }
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        seen.push(id);
        if ROLLED_TRADERS.lock().contains(&id) {
            return Ok(true);
        }
        if let Err(e) = roll_band(com.handle().0, &TRADER_ODDS, now, "trader stock") {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: quality: trader roll failed: {e}"),
            );
        }
        ROLLED_TRADERS.lock().push(id);
        Ok(true)
    });
    // Traders despawn; forget the gone ones.
    ROLLED_TRADERS.lock().retain(|id| seen.contains(id));
}

/// Roll and replace eligible gear carried by an arriving band.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
fn roll_band(band_h: i32, odds: &[u64; 4], now: f32, origin: &str) -> Result<(), String> {
    let Some(m_h) = with(band_h, |b| {
        b.read_field("Members").ok().as_ref().and_then(handle_of)
    }) else {
        return Ok(());
    };
    let mlist = own(m_h);
    let count = mlist.list_len_or_zero()?;
    for mi in 0..count {
        let Some(mh) = mlist.list_handle(mi)? else {
            continue;
        };
        let member = own(mh);
        let alive = member
            .invoke("get_AliveAndNotZombie", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false);
        if !alive {
            continue;
        }
        let Some(inv_h) = handle_of(&member.read_field("Inventory")?) else {
            continue;
        };
        let inv = own(inv_h);
        let n = inv.list_len_or_zero()?;
        // Walk top-down: a swap mutates the container.
        for i in (0..n).rev() {
            let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([i]))?) else {
                continue;
            };
            let item = own(item_h);
            let Some(base_name) =
                handle_of(&item.invoke("GetPrototype", &json!([]))?).and_then(|ph| {
                    with(ph, |p| {
                        let name = p
                            .read_field("Name")
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string));
                        drop(own(ph));
                        name
                    })
                })
            else {
                continue;
            };
            // Already tiered (or any modded underscore name): never
            // re-roll.
            if base_name.contains('_') {
                continue;
            }
            let salt = (mi as u64) * 131 + i as u64 + 7;
            let Some(tier_ix) = roll_tier(odds, now, salt) else {
                continue;
            };
            let who = member
                .invoke("GetDisplayNameString", &json!([]))
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "<unnamed>".into());
            let _ = swap_to_variant(
                mh, &inv, item_h, &base_name, tier_ix, now, salt, &who, origin,
            )?;
        }
    }
    Ok(())
}

/// The swap, net zero items: the common piece leaves the world,
/// the tiered variant (a random sibling) lands in the same hand.
/// Ok(true) on a swap; Ok(false) when no variant exists for this
/// item (not a weapon or armor piece).
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
#[allow(clippy::too_many_arguments)]
fn swap_to_variant(
    owner_h: i32,
    inv: &MonoObject,
    item_h: i32,
    base_name: &str,
    tier_ix: usize,
    now: f32,
    salt: u64,
    who: &str,
    origin: &str,
) -> Result<bool, String> {
    let sibling = roll_sibling(SIBLINGS, now, salt);
    let candidate = format!("{base_name}_{}{sibling}", TIER_NAMES[tier_ix]);
    // Only weapons and armor have variants; anything else misses
    // the lookup and stays as it was.
    let Ok(Some(proto_h)) = find_prototype(&candidate) else {
        return Ok(false);
    };
    let taken = inv.invoke(
        "Take",
        &json!([{ "handle": owner_h }, { "handle": item_h }, 1]),
    )?;
    let Some(taken_h) = handle_of(&taken) else {
        drop(own(proto_h));
        return Ok(false);
    };
    if let Err(e) = with(taken_h, |t| t.invoke("Delete", &json!([]))) {
        mono::log(
            LogLevel::Warn,
            &format!("survivalist-mod: quality: delete of the common piece failed: {e}"),
        );
    }
    drop(own(taken_h));
    let fine = mono::invoke_static("Equipment", "Spawn", &json!([{ "handle": proto_h }, 1]));
    drop(own(proto_h));
    let fine_h = match fine {
        Ok(v) => match handle_of(&v) {
            Some(h) => h,
            None => return Ok(false),
        },
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: quality: variant spawn failed: {e}"),
            );
            return Ok(false);
        }
    };
    let _ = with(owner_h, |o| {
        o.invoke("Add", &json!([{ "handle": owner_h }, { "handle": fine_h }]))
    });
    drop(own(fine_h));
    SWAPS[tier_ix].fetch_add(1, Ordering::Relaxed);
    *LAST_SWAP.lock() = Some(format!(
        "{} {base_name} ({origin}) with {who}",
        TIER_NAMES[tier_ix]
    ));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: quality: a {} {base_name} ({origin}) now in {who}'s hands",
            TIER_NAMES[tier_ix],
        ),
    );
    Ok(true)
}

/// Walk GameImpl.Instance.CurrentStories and ask each loaded
/// story for the prototype (Story.FindEquipmentPrototypeByName);
/// the one that loaded our XML answers. Shared with unique.rs.
/// Stays here because it searches Survivalist story data by the game's exact prototype API.
pub(crate) fn find_prototype(name: &str) -> Result<Option<i32>, String> {
    let game = MonoType::find("GameImpl")
        .and_then(|t| t.singleton_instance())
        .ok_or("GameImpl.Instance not found")?;
    let Some(list_h) = handle_of(&game.read_field("CurrentStories")?) else {
        return Ok(None);
    };
    let list = own(list_h);
    let n = list.list_len_or_zero()?;
    for i in 0..n {
        let Some(story_h) = list.list_handle(i)? else {
            continue;
        };
        let story = own(story_h);
        if let Ok(p) = story.invoke("FindEquipmentPrototypeByName", &json!([name])) {
            if let Some(ph) = handle_of(&p) {
                return Ok(Some(ph));
            }
        }
    }
    Ok(None)
}

// ---- ops ---------------------------------------------------------------------

/// Expose this system status and controls through the mod control endpoint.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
pub fn register_ops() {
    OP_REGISTRY.register(OpDef::new(
        "quality_status",
        "The quality system's live state: is the variant data loaded, edge swaps per tier, and the last swap.",
        "{}",
        quality_status,
    ));
}

/// Report loaded variants, quality swaps, rolled traders, and pending craft rolls.
/// Stays here because Survivalist owns the tier catalog, crafting odds, prototype names, and inventory swaps; Modforge owns the quality rolls.
fn quality_status(_args: &Json) -> Result<Json, String> {
    MAIN_QUEUE.run_result("quality_status", std::time::Duration::from_secs(5), || {
        let loaded = match find_prototype(CANARY) {
            Ok(Some(h)) => {
                drop(own(h));
                true
            }
            _ => false,
        };
        let mut swaps = serde_json::Map::new();
        for (i, name) in TIER_NAMES.iter().enumerate() {
            swaps.insert(name.to_lowercase(), json!(SWAPS[i].load(Ordering::Relaxed)));
        }
        Ok(json!({
            "variants_loaded": loaded,
            "swaps": swaps,
            "last_swap": LAST_SWAP.lock().clone(),
            "traders_rolled": ROLLED_TRADERS.lock().len(),
            "craft_rolls": CRAFT_ROLLS.load(Ordering::Relaxed),
            "craft_jobs_pending": CRAFT_JOBS.lock().len(),
        }))
    })
}
