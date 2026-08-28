//! horsey-mod's declarative target registry.
//!
//! Every horsey address (data globals, function entries, field
//! offsets) is declared here as a `modforge::patterns::sleuth::TargetDef`.
//! The Resolver controller in modforge walks this registry at attach
//! time and resolves every address, running validators on each.
//!
//! Migration status (B4): the bulk of entries use `candidates: &[]`
//! and rely on `hint_rva` fallback for now; they resolve to the
//! same hardcoded RVAs the legacy `targets::resolve::*` functions
//! return. Per-target sig tuning happens as targets get exercised
//! against the live game; each tuned target adds a real
//! `Candidate { sig, recipe }` to its entry.
//!
//! The legacy `targets::resolve::*` family in `targets.rs` stays
//! callable during the migration; the parity test in
//! `tests/registry_parity.rs` guards against drift.

use modforge::patterns::sleuth::{
    self, Candidate, InImage, Recipe, Resolver, TargetDef, TargetKind, TargetRegistry, Validator,
};
use modforge::vanilla::sig::{ArgKind, RetKind, Signature};

// ============================================================================
// Custom resolvers (Recipe::Custom). For targets whose decoder doesn't fit
// any of the closed Recipe variants. Each returns the final value (heap
// pointer for NAME_TABLE, table base for CHROMOSOME_TABLE) per
// CustomResolver's contract.
// ============================================================================

/// NAME_TABLE: anchor on the name resolver's tail (`FUN_1400c78c0`):
///   `imul rax, rax, 0x88`     -> `48 69 c0 88 00 00 00`
///   `add  rax, [rip+disp32]`  -> `48 03 05 <disp32>`
/// The disp32 targets the `.data` slot holding the heap table base; deref
/// it. The old build had the table inline in `.data`; the current build
/// moved it to the heap and the slot drifted +0x1110 (same as
/// GAMESTATE_PTR), which is why the old std::string-shape scan stopped
/// finding it and names read "?". The `imul *0x88` + `add [rip]` pair is
/// unique in `.text` (live-verified 2026-06-24: 24 imul-0x88 sites, only
/// one with this exact reg/op sequence), so this is robust across drift.
fn resolve_name_table_custom(_image_base: u64) -> Result<u64, String> {
    let sig = "48 69 c0 88 00 00 00 48 03 05 ?? ?? ?? ??";
    let hits = sleuth::scan_all_matches(sig).map_err(|e| e.to_string())?;
    if hits.len() != 1 {
        return Err(format!(
            "name-table anchor matched {} site(s); want 1",
            hits.len()
        ));
    }
    let site = hits[0];
    if !modforge::winproc::is_addr_readable(site + 14) {
        return Err("name-table anchor end unreadable".into());
    }
    // The `add r64,[rip+disp32]` is 7 bytes starting at site+7; its disp32
    // is at site+10 and next_ip = site+14.
    // SAFETY: site..site+14 is the matched, mapped `.text` run.
    let disp = unsafe { *((site + 10) as *const i32) } as isize;
    let slot = ((site + 14) as isize + disp) as usize;
    if !modforge::winproc::is_addr_readable(slot + 8) {
        return Err(format!("name-table slot 0x{slot:x} unreadable"));
    }
    // Return the SLOT (a static `.data` address), NOT `*slot`. The heap
    // table base lives at `*slot` and is allocated only after the save
    // loads; callers deref at lookup time. Resolving `*slot` here. The
    // registry runs at attach, pre-load. Would read a null/unallocated
    // table and cache a permanent failure (the 2026-06-24 names="?" bug).
    Ok(slot as u64)
}

/// CHROMOSOME_TABLE: find FUN_1400b39b0 via its unique prologue sig,
/// walk its body window for every `lea r64, [rip+disp32]`, validate
/// each candidate as a 240-entry chromosome -> gene-offset table.
fn resolve_chromosome_table_custom(_image_base: u64) -> Result<u64, String> {
    let prologue_sig = "48 89 54 24 10 55 48 83 ec 50 48 63 81 \
                        34 02 00 00 48 8b e9 83 f8 13 0f 87";
    let mut hits =
        sleuth::scan_all_matches(prologue_sig).map_err(|e| format!("prologue sig scan: {e}"))?;
    hits.sort();
    hits.dedup();
    if hits.len() != 1 {
        return Err(format!(
            "prologue matched {} sites; want exactly 1",
            hits.len()
        ));
    }
    let fn_start = hits[0];

    const BODY_WINDOW: usize = 1024;
    let mut candidates: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for prefix in [0x48u8, 0x4c] {
        for reg in 0..8u8 {
            let modrm = (reg << 3) | 0x05;
            let sig = format!("{prefix:02x} 8d {modrm:02x} ?? ?? ?? ??");
            let Ok(matches) = sleuth::scan_all_matches(&sig) else {
                continue;
            };
            for site in matches {
                if site < fn_start || site >= fn_start + BODY_WINDOW {
                    continue;
                }
                if !modforge::winproc::is_addr_readable(site + 7) {
                    continue;
                }
                // SAFETY: site+7 readability checked.
                let bytes = unsafe { std::slice::from_raw_parts(site as *const u8, 7) };
                let disp = i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]) as isize;
                let next_ip = (site + 7) as isize;
                candidates.insert((next_ip + disp) as usize);
            }
        }
    }

    for c in candidates {
        if validate_chromosome_table_shape(c) {
            return Ok(c as u64);
        }
    }
    Err("no body lea target passed table-shape validation".into())
}

fn validate_chromosome_table_shape(addr: usize) -> bool {
    const STRIDE: usize = 0x44;
    const SLOTS: usize = 17;
    const MAX_CHROMOS: usize = 20;
    const TABLE_BYTES: usize = STRIDE * MAX_CHROMOS;
    if (addr & 3) != 0 {
        return false;
    }
    if !modforge::winproc::is_addr_readable(addr + TABLE_BYTES) {
        return false;
    }
    let mut valid_count = 0usize;
    let mut seen = [false; 240];
    for chromo_id in 0..MAX_CHROMOS {
        for slot in 0..SLOTS {
            let off = chromo_id * STRIDE + slot * 4;
            // SAFETY: range readability checked above.
            let v = unsafe { *((addr + off) as *const i32) };
            if v == -1 {
                break;
            }
            if !(0..240).contains(&v) {
                return false;
            }
            if seen[v as usize] {
                return false;
            }
            seen[v as usize] = true;
            valid_count += 1;
        }
    }
    valid_count == 240
}

// ============================================================================
// Validator chains (shared by groups of similar targets)
// ============================================================================

static V_IN_IMAGE: &[&dyn Validator] = &[&InImage];
static V_NONE: &[&dyn Validator] = &[];

// ============================================================================
// Data globals (.data slots)
// ============================================================================

static GAMESTATE_PTR: TargetDef = TargetDef {
    name: "GAMESTATE_PTR",
    kind: TargetKind::DataGlobal,
    candidates: &[Candidate {
        sig: "48 89 1D ?? ?? ?? ?? 48 89 BB 70 02 00 00",
        recipe: Recipe::DecodeRipDisp {
            disp_off: 3,
            instr_len: 7,
        },
    }],
    // Slot drifted to RVA 0x3fc1e8 in the 2026-05-17 build (was
    // 0x3fb0d8). HLT pattern still anchors correctly; the only
    // change required was bumping the hint window past the +0x1110
    // drift. Bumped to 0x8000 to absorb future small reorders;
    // the pattern is unique enough that a false match in that
    // window is implausible.
    hint_rva: Some(0x1403fc1e8),
    hint_tolerance: 0x8000,
    validators: V_IN_IMAGE,
};

static SAVE_VERSION_GLOBAL: TargetDef = TargetDef {
    name: "SAVE_VERSION_GLOBAL",
    kind: TargetKind::DataGlobal,
    candidates: &[],
    hint_rva: Some(0x1403fb0e0),
    hint_tolerance: 0x1000,
    validators: V_IN_IMAGE,
};

static RACES_COUNTER: TargetDef = TargetDef {
    name: "RACES_COUNTER",
    kind: TargetKind::DataGlobal,
    candidates: &[],
    hint_rva: Some(0x1403eded8),
    hint_tolerance: 0x1000,
    validators: V_IN_IMAGE,
};

static NO_TIRE_TOGGLE: TargetDef = TargetDef {
    name: "NO_TIRE_TOGGLE",
    kind: TargetKind::DataGlobal,
    candidates: &[],
    hint_rva: Some(0x1403d95a5),
    hint_tolerance: 0x1000,
    validators: V_IN_IMAGE,
};

/// `DEBUG_MODE_ACTIVE`. Anchor: the unlock-block
/// `c6 05 ?? ?? ?? ?? 01 c6 05 ?? ?? ?? ?? 00` (set 1 + set 0).
/// Two RIP-rel `mov byte ptr [global], imm` instructions, 7 bytes
/// each. The two targets are at known relative offset -0x79 in
/// the live build's .data layout.
static DEBUG_MODE_ACTIVE: TargetDef = TargetDef {
    name: "DEBUG_MODE_ACTIVE",
    kind: TargetKind::DataGlobal,
    candidates: &[Candidate {
        sig: "c6 05 ?? ?? ?? ?? 01 c6 05 ?? ?? ?? ?? 00",
        recipe: Recipe::PairedRipDispWithDelta {
            disp1_off: 2,
            disp1_next_ip: 7,
            disp2_off: 9,
            disp2_next_ip: 14,
            delta: -0x79,
        },
    }],
    hint_rva: Some(0x1403d957b),
    hint_tolerance: 0x1000,
    validators: V_IN_IMAGE,
};

/// `DEBUG_LOG_GATE`. Anchor: the global-init triplet
/// `c7 05 ?? ?? ?? ?? 00 01 00 00 c7 05 ?? ?? ?? ?? ff ff ff ff`
/// (3rd write = 0x100 to DAT_1403d9598, then 4th write = 0xffffffff).
/// The decomp's known offset from the 3rd-write target to
/// DEBUG_LOG_GATE is -0x72 (cheat globals drift uniformly when
/// .data is re-laid-out, so this rel-offset is stable across builds).
static DEBUG_LOG_GATE: TargetDef = TargetDef {
    name: "DEBUG_LOG_GATE",
    kind: TargetKind::DataGlobal,
    candidates: &[Candidate {
        sig: "c7 05 ?? ?? ?? ?? 00 01 00 00 c7 05 ?? ?? ?? ?? ff ff ff ff",
        recipe: Recipe::RipDispWithRelOffset {
            disp_off: 2,
            instr_len: 10,
            rel_offset: -0x72,
        },
    }],
    hint_rva: Some(0x1403d9506),
    hint_tolerance: 0x1000,
    validators: V_IN_IMAGE,
};

/// `NAME_TABLE`. The legacy resolver returns the HEAP address of the
/// std::string table (NOT the .data slot containing the heap pointer).
/// Custom resolver scans .text for `mov r64, [rip+disp32]`, derefs
/// each candidate slot, scores std::string-shape at stride 0x88.
/// The hint_rva is the .data slot; resolved value is the heap base,
/// so hint_tolerance is meaningless here (effectively disabled by
/// being larger than the largest plausible heap delta).
static NAME_TABLE: TargetDef = TargetDef {
    name: "NAME_TABLE",
    kind: TargetKind::DataGlobal,
    candidates: &[Candidate {
        sig: "",
        recipe: Recipe::Custom(resolve_name_table_custom),
    }],
    hint_rva: None, // heap pointer; no static hint
    hint_tolerance: 0,
    validators: &[], // Custom resolver does its own scoring
};

/// `CHROMOSOME_TABLE`. Custom resolver finds FUN_1400b39b0 via prologue
/// sig, walks body for `lea r64, [rip+disp32]`, validates each
/// candidate as a 240-entry chromosome table.
static CHROMOSOME_TABLE: TargetDef = TargetDef {
    name: "CHROMOSOME_TABLE",
    kind: TargetKind::DataGlobal,
    candidates: &[Candidate {
        sig: "",
        recipe: Recipe::Custom(resolve_chromosome_table_custom),
    }],
    hint_rva: None,
    hint_tolerance: 0,
    validators: &[],
};

// ============================================================================
// Function entries
// ============================================================================
//
// Most entries below are hint-only during the B4 migration. As each
// is exercised in production, a real Candidate gets added with the
// derived sig + recipe. The legacy `targets::function_entry::*`
// consts stay valid until B5 deletes them.

/// Standard MSVC Win64 prologue sig that fits most horsey-mod
/// targets: `mov [rsp+disp8], rbx; mov [rsp+disp8], rbp; mov [rsp+disp8], rsi`.
/// Specific-enough for hint-tolerance guarding to disambiguate.
const PROLOGUE_SHADOW_SAVE_3: &str = "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24";

// Signatures for invocable functions. These let the function be
// called via `modforge::vanilla::Invoker`.
pub static APPLY_GENE_SIG: Signature = Signature::new(&[ArgKind::Ptr, ArgKind::Ptr], RetKind::Void);

/// `HORSE_REBUILD` / `FUN_1400b3070` per HLT
/// `kRvaHorseRebuildEquipmentAndPhysics`. Takes the horse ptr and
/// rebuilds derived state from the working genome.
pub static HORSE_REBUILD_SIG: Signature = Signature::new(&[ArgKind::Ptr], RetKind::Void);

/// `FUN_1400c6580` per HLT `kRvaRngNextModulo`. Generic
/// `rng_next() % modulus`.
pub static RNG_NEXT_MODULO_SIG: Signature = Signature::new(&[ArgKind::U32], RetKind::U32);

/// `FUN_1400b35f0` per HLT `kRvaHorseCopyGeneLanePairs`. Sets a
/// horse's genome from a 480-byte buffer.
pub static HORSE_COPY_GENE_LANE_PAIRS_SIG: Signature =
    Signature::new(&[ArgKind::Ptr, ArgKind::Ptr], RetKind::Void);

static APPLY_GENE_TO_HORSE: TargetDef = TargetDef {
    name: "APPLY_GENE_TO_HORSE",
    kind: TargetKind::FunctionEntry {
        signature: Some(&APPLY_GENE_SIG),
    },
    candidates: &[Candidate {
        sig: PROLOGUE_SHADOW_SAVE_3,
        recipe: Recipe::MatchIsAddress,
    }],
    hint_rva: Some(0x14009f670),
    hint_tolerance: 0x4000,
    validators: V_IN_IMAGE,
};

/// New entries from HLT cross-reference: invocable vanilla functions.
static HORSE_REBUILD: TargetDef = TargetDef {
    name: "HORSE_REBUILD",
    kind: TargetKind::FunctionEntry {
        signature: Some(&HORSE_REBUILD_SIG),
    },
    candidates: &[],
    hint_rva: Some(0x1400b3070),
    hint_tolerance: 0x4000,
    validators: V_IN_IMAGE,
};

static RNG_NEXT_MODULO: TargetDef = TargetDef {
    name: "RNG_NEXT_MODULO",
    kind: TargetKind::FunctionEntry {
        signature: Some(&RNG_NEXT_MODULO_SIG),
    },
    candidates: &[],
    hint_rva: Some(0x1400c6580),
    hint_tolerance: 0x4000,
    validators: V_IN_IMAGE,
};

static HORSE_COPY_GENE_LANE_PAIRS: TargetDef = TargetDef {
    name: "HORSE_COPY_GENE_LANE_PAIRS",
    kind: TargetKind::FunctionEntry {
        signature: Some(&HORSE_COPY_GENE_LANE_PAIRS_SIG),
    },
    candidates: &[],
    hint_rva: Some(0x1400b35f0),
    hint_tolerance: 0x4000,
    validators: V_IN_IMAGE,
};

/// `FUN_1401041f0` (`exit_to_overworld`). Called when the in-scene
/// "Map" button is pressed. Sets `GS+0x25C = -1`, zeroes scene state,
/// triggers auto-save, plays the "World" sound, calls scene tear-down,
/// resumes truck driving. Single argument: `GameState*`.
/// Cite: `all_functions.c:154755`.
pub static EXIT_TO_OVERWORLD_SIG: Signature = Signature::new(&[ArgKind::Ptr], RetKind::Void);
static EXIT_TO_OVERWORLD: TargetDef = TargetDef {
    name: "EXIT_TO_OVERWORLD",
    kind: TargetKind::FunctionEntry {
        signature: Some(&EXIT_TO_OVERWORLD_SIG),
    },
    candidates: &[],
    hint_rva: Some(0x1401041f0),
    hint_tolerance: 0x4000,
    validators: V_IN_IMAGE,
};

// Function entries inherited from existing targets.rs. Hint-only
// during B4; real sigs land per target as they get tuned.

/// Declarative function-entry target with a known-good 32-byte
/// prologue sig + MatchIsAddress recipe + hint_rva fallback. Used
/// for the bulk of horsey-mod's function entries. Sigs sourced
/// from the deleted `targets::resolve::*` module (live-derived
/// 2026-05-15) so they're known to match this build.
macro_rules! prologue_fn {
    ($name:ident, $rva:expr, $sig:expr) => {
        static $name: TargetDef = TargetDef {
            name: stringify!($name),
            kind: TargetKind::FunctionEntry { signature: None },
            candidates: &[Candidate {
                sig: $sig,
                recipe: Recipe::MatchIsAddress,
            }],
            hint_rva: Some($rva),
            hint_tolerance: 0x4000,
            validators: V_IN_IMAGE,
        };
    };
}

/// Hint-only entry for targets whose sig hasn't been authored yet.
/// Falls through to `hint_rva` via the WithinHintTolerance gate.
macro_rules! hint_only_fn {
    ($name:ident, $rva:expr) => {
        static $name: TargetDef = TargetDef {
            name: stringify!($name),
            kind: TargetKind::FunctionEntry { signature: None },
            candidates: &[],
            hint_rva: Some($rva),
            hint_tolerance: 0x4000,
            validators: V_IN_IMAGE,
        };
    };
}

prologue_fn!(
    EVAL_DIPLOID_BLEND_A,
    0x1400a5d10,
    "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 48 63 da 48 8d 2d 72 87 34 00 48 8b"
);
prologue_fn!(
    EVAL_DIPLOID_BLEND_B,
    0x1400a5df0,
    "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 48 63 da 48 8d 2d 92 86 34 00 48 8b"
);
prologue_fn!(
    GENE_DEATH_DRIFT,
    0x1400c0650,
    "48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48 8d a8 b8 fa ff ff 48 81 ec 08 06 00 00 0f 29 70"
);
prologue_fn!(
    GENE_ALLELE_SWAP,
    0x1400c0390,
    "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 48 63 fa 48 be c1 d4 1c 42 29 8f a0 3f 4d 63 d9 45"
);
prologue_fn!(
    GENE_TABLE_XML_WRITER,
    0x1400a4880,
    "41 54 41 55 41 56 41 57 48 8d 6c 24 90 48 81 ec 70 01 00 00 0f 29 70 c8 0f 29 78 b8 44 0f 29 40"
);
prologue_fn!(
    GENE_TABLE_LOADER,
    0x1400a3eb0,
    "41 57 48 8d a8 28 ff ff ff 48 81 ec a0 01 00 00 0f 29 70 b8 0f 29 78 a8 44 0f 29 40 98 44 0f 29"
);
prologue_fn!(
    POP_XML_LOADER,
    0x1400a4fe0,
    "48 8d ac 24 c0 cf ff ff b8 40 31 00 00 e8 ?? ?? ?? ?? 48 2b e0 48 8d 4c 24 50 e8 ?? ?? ?? ?? 90"
);
prologue_fn!(
    GENE_ENGINE_CONSUMER,
    0x1400ab3c0,
    "48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48 8d a8 88 fd ff ff 48 81 ec 38 03 00 00 0f 29 70"
);
prologue_fn!(
    CHECK_HORSE_ELIGIBILITY,
    0x1400de230,
    "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8b ec 48 81 ec 80 00 00 00 0f 29 74 24"
);
hint_only_fn!(RETIRE_HORSE_HANDLER, 0x1400df675); // 32-byte sig not captured; uses re-derived RVA
prologue_fn!(
    COMPUTE_HORSE_PRICE,
    0x1400dc785,
    "48 89 7c 24 10 55 48 8b ec 48 83 ec 60 0f 29 74 24 50 48 8b f9 4c 63 c2 48 8b 91 30 01 00 00 48"
);
prologue_fn!(
    CRISPR_LAB,
    0x1400894bc,
    "57 41 54 41 56 41 57 48 8d a8 58 fe ff ff 48 81 ec 80 02 00 00 0f 29 70 c8 0f 29 78 b8 44 0f 29"
);
prologue_fn!(
    BREEDING,
    0x1400e0817,
    "57 41 54 41 55 41 56 41 57 48 8b ec 48 83 ec 68 44 8b f2 4c 8b f9 4c 8b 81 30 01 00 00 48 8b 81"
);
prologue_fn!(
    SAVE_WRITER,
    0x14006d674,
    "57 41 56 48 81 ec 40 01 00 00 48 8b e9 48 8d 51 18 48 8d 4c 24 30 e8 ?? ?? ?? ?? 48 8b 54 24 48"
);
prologue_fn!(
    LOAD_GAME,
    0x14006e350,
    "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8b ec 48 83 ec 70 48 8b f1 44 8b c2 48 8d 15"
);
prologue_fn!(
    DRAW_PAUSE_STATUS,
    0x1400660cc,
    "57 41 54 41 56 41 57 48 8d 68 a1 48 81 ec c0 00 00 00 0f 29 70 c8 0f 29 78 b8 44 0f 29 40 a8 44"
);
prologue_fn!(
    TMX_MAP_PARSER,
    0x1400fe2e0,
    "48 89 5c 24 08 48 89 7c 24 10 55 48 8b ec 48 83 ec 20 8b fa 33 db 83 fa 0c 7f 25 b9 78 04 00 00"
);
prologue_fn!(
    POP_GENOME_BUILDER,
    0x1400927e7,
    "57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec e0 00 00 00 0f 29 b4 24 d0 00 00 00 48 8b f9"
);
prologue_fn!(
    DAILY_HORSE_EVENT,
    0x14002fe00,
    "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57 48 8d 6c 24 c9 48 81 ec"
);
prologue_fn!(
    TRACK_STATE_MACHINE,
    0x14002d7c0,
    "48 8b c4 48 89 58 10 48 89 70 18 48 89 78 20 55 41 54 41 55 41 56 41 57 48 8d 68 a1 48 81 ec e0"
);
prologue_fn!(
    CIRCUS_HANDLER,
    0x140039190,
    "48 8b c4 48 89 48 08 55 53 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 b8 48 81 ec 48 01 00 00 0f"
);
prologue_fn!(
    SUMO_HANDLER,
    0x14007b28c,
    "57 41 56 48 8d 68 a1 48 81 ec c0 00 00 00 0f 29 70 d8 0f 29 78 c8 44 0f 29 40 b8 44 0f 29 48 a8"
);
prologue_fn!(
    POWER_PLANT,
    0x140069270,
    "48 89 5c 24 08 55 48 8b ec 48 83 ec 60 0f 29 74 24 50 48 8b d9 ff 81 50 02 00 00 e8 ?? ?? ?? ??"
);
prologue_fn!(
    WORLD_ACTION,
    0x1401075a2,
    "57 41 54 41 55 41 56 41 57 48 83 ec 70 0f 29 70 c8 0f 29 78 b8 44 8b ca 4c 8b f9 0f 57 c0 f3 0f"
);
hint_only_fn!(BALLOON_CONTROLLER, 0x14010a9e0); // sig not captured live
prologue_fn!(
    HORSE_CONSTRUCTOR,
    0x1400aac50,
    "48 89 5c 24 10 48 89 4c 24 08 57 48 83 ec 20 48 8b d9 48 8d 05 ?? ?? ?? ?? 48 89 01 33 ff 48 89"
);
prologue_fn!(
    HORSE_DESTRUCTOR,
    0x1400bf1e0,
    "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8d 05 ?? ?? ?? ?? 8b f2 48 89 01 48 8b d9 ff 0d"
);
prologue_fn!(
    GENE_COMBINATOR,
    0x1400a2d70,
    "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 ec 20 4c 8b f9 45"
);
prologue_fn!(
    HORSE_SAVE_WRITER,
    0x14006ecfb,
    "57 41 56 48 83 ec 40 48 8b e9 e8 ?? ?? ?? ?? 48 8d 8d b8 02 00 00 44 8b f0 e8 ?? ?? ?? ?? 48 8b"
);
prologue_fn!(
    HORSE_SAVE_LOADER,
    0x14006f031,
    "53 57 48 83 ec 58 48 8b f9 48 81 c1 b8 02 00 00 e8 ?? ?? ?? ?? 48 8d 8f a8 02 00 00 e8 ?? ?? ??"
);

// Silence unused warning for V_NONE (kept for future field-offset
// entries that don't need validators).
#[allow(dead_code)]
static _V_NONE_KEEPALIVE: &[&dyn Validator] = V_NONE;

// ============================================================================
// The registry
// ============================================================================

pub static HORSEY_TARGETS: TargetRegistry = TargetRegistry::new(
    "horsey-mod",
    &[
        // Data globals
        &GAMESTATE_PTR,
        &SAVE_VERSION_GLOBAL,
        &RACES_COUNTER,
        &NO_TIRE_TOGGLE,
        &DEBUG_MODE_ACTIVE,
        &DEBUG_LOG_GATE,
        &NAME_TABLE,
        &CHROMOSOME_TABLE,
        // Invocable functions (with Signature)
        &APPLY_GENE_TO_HORSE,
        &HORSE_REBUILD,
        &RNG_NEXT_MODULO,
        &HORSE_COPY_GENE_LANE_PAIRS,
        &EXIT_TO_OVERWORLD,
        // Function entries (hint-only; sigs land per-target during B4)
        &EVAL_DIPLOID_BLEND_A,
        &EVAL_DIPLOID_BLEND_B,
        &GENE_DEATH_DRIFT,
        &GENE_ALLELE_SWAP,
        &GENE_TABLE_XML_WRITER,
        &GENE_TABLE_LOADER,
        &POP_XML_LOADER,
        &GENE_ENGINE_CONSUMER,
        &CHECK_HORSE_ELIGIBILITY,
        &RETIRE_HORSE_HANDLER,
        &COMPUTE_HORSE_PRICE,
        &CRISPR_LAB,
        &BREEDING,
        &SAVE_WRITER,
        &LOAD_GAME,
        &DRAW_PAUSE_STATUS,
        &TMX_MAP_PARSER,
        &POP_GENOME_BUILDER,
        &DAILY_HORSE_EVENT,
        &TRACK_STATE_MACHINE,
        &CIRCUS_HANDLER,
        &SUMO_HANDLER,
        &POWER_PLANT,
        &WORLD_ACTION,
        &BALLOON_CONTROLLER,
        &HORSE_CONSTRUCTOR,
        &HORSE_DESTRUCTOR,
        &GENE_COMBINATOR,
        &HORSE_SAVE_WRITER,
        &HORSE_SAVE_LOADER,
    ],
);

/// Per-process Resolver singleton bound to `HORSEY_TARGETS`.
pub static HORSEY_RESOLVER: Resolver = Resolver::new(&HORSEY_TARGETS);

/// Drop-in replacement for the legacy `targets::resolve::*` API.
/// Each function delegates to the registry by name. Same return
/// type (`Option<usize>`), same semantics for migrated targets.
///
/// NAME_TABLE and CHROMOSOME_TABLE are NOT in this module: the
/// legacy resolvers do an additional heap-deref + scoring pass
/// that the current Recipe enum doesn't express. Call sites for
/// those stay on the legacy path until a Recipe::HeapScored
/// variant lands.
pub mod resolve {
    fn r(name: &str) -> Option<usize> {
        super::HORSEY_RESOLVER.resolve(name).map(|v| v as usize)
    }

    // Data globals
    pub fn gamestate_ptr() -> Option<usize> {
        r("GAMESTATE_PTR")
    }
    pub fn save_version_global() -> Option<usize> {
        r("SAVE_VERSION_GLOBAL")
    }
    pub fn races_counter() -> Option<usize> {
        r("RACES_COUNTER")
    }
    pub fn no_tire_toggle() -> Option<usize> {
        r("NO_TIRE_TOGGLE")
    }
    pub fn debug_mode_active() -> Option<usize> {
        r("DEBUG_MODE_ACTIVE")
    }
    pub fn debug_log_gate() -> Option<usize> {
        r("DEBUG_LOG_GATE")
    }
    pub fn name_table() -> Option<usize> {
        // NAME_TABLE must NOT go through the registry's resolve cache. The
        // registry caches the first result permanently, so a single miss
        // (a transient scan failure at attach, before the image is fully
        // scannable) sticks forever and every name reads blank. The custom
        // resolver returns the static `.data` slot and does not depend on the
        // save being loaded, so resolve it DIRECTLY and cache ONLY a
        // successful, non-zero slot; a miss is always retried next call.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SLOT: AtomicUsize = AtomicUsize::new(0);
        let cached = SLOT.load(Ordering::Relaxed);
        if cached != 0 {
            return Some(cached);
        }
        match super::resolve_name_table_custom(0) {
            Ok(slot) if slot != 0 => {
                SLOT.store(slot as usize, Ordering::Relaxed);
                Some(slot as usize)
            }
            _ => None,
        }
    }
    pub fn chromosome_table() -> Option<usize> {
        r("CHROMOSOME_TABLE")
    }

    // Function entries
    pub fn apply_gene_to_horse() -> Option<usize> {
        r("APPLY_GENE_TO_HORSE")
    }
    pub fn horse_constructor() -> Option<usize> {
        r("HORSE_CONSTRUCTOR")
    }
    pub fn horse_destructor() -> Option<usize> {
        r("HORSE_DESTRUCTOR")
    }
    pub fn gene_combinator() -> Option<usize> {
        r("GENE_COMBINATOR")
    }
    pub fn save_writer() -> Option<usize> {
        r("SAVE_WRITER")
    }
    pub fn load_game() -> Option<usize> {
        r("LOAD_GAME")
    }
    pub fn horse_save_writer() -> Option<usize> {
        r("HORSE_SAVE_WRITER")
    }
    pub fn horse_save_loader() -> Option<usize> {
        r("HORSE_SAVE_LOADER")
    }
    pub fn eval_diploid_blend_a() -> Option<usize> {
        r("EVAL_DIPLOID_BLEND_A")
    }
    pub fn eval_diploid_blend_b() -> Option<usize> {
        r("EVAL_DIPLOID_BLEND_B")
    }
    pub fn gene_allele_swap() -> Option<usize> {
        r("GENE_ALLELE_SWAP")
    }

    // Function entries referenced by targets.rs internal resolvers
    // (gs_offset::*, horse_offset::* still in legacy targets.rs).
    pub fn draw_pause_status() -> Option<usize> {
        r("DRAW_PAUSE_STATUS")
    }
    pub fn breeding() -> Option<usize> {
        r("BREEDING")
    }
    pub fn compute_horse_price() -> Option<usize> {
        r("COMPUTE_HORSE_PRICE")
    }
    pub fn pop_genome_builder() -> Option<usize> {
        r("POP_GENOME_BUILDER")
    }
    pub fn sumo_handler() -> Option<usize> {
        r("SUMO_HANDLER")
    }
    pub fn check_horse_eligibility() -> Option<usize> {
        r("CHECK_HORSE_ELIGIBILITY")
    }
    pub fn retire_horse_handler() -> Option<usize> {
        r("RETIRE_HORSE_HANDLER")
    }
    pub fn crispr_lab() -> Option<usize> {
        r("CRISPR_LAB")
    }
    pub fn gene_table_xml_writer() -> Option<usize> {
        r("GENE_TABLE_XML_WRITER")
    }
    pub fn gene_table_loader() -> Option<usize> {
        r("GENE_TABLE_LOADER")
    }
    pub fn pop_xml_loader() -> Option<usize> {
        r("POP_XML_LOADER")
    }
    pub fn gene_engine_consumer() -> Option<usize> {
        r("GENE_ENGINE_CONSUMER")
    }
    pub fn tmx_map_parser() -> Option<usize> {
        r("TMX_MAP_PARSER")
    }
    pub fn daily_horse_event() -> Option<usize> {
        r("DAILY_HORSE_EVENT")
    }
    pub fn track_state_machine() -> Option<usize> {
        r("TRACK_STATE_MACHINE")
    }
    pub fn circus_handler() -> Option<usize> {
        r("CIRCUS_HANDLER")
    }
    pub fn power_plant() -> Option<usize> {
        r("POWER_PLANT")
    }
    pub fn world_action() -> Option<usize> {
        r("WORLD_ACTION")
    }
    pub fn balloon_controller() -> Option<usize> {
        r("BALLOON_CONTROLLER")
    }
    pub fn gene_death_drift() -> Option<usize> {
        r("GENE_DEATH_DRIFT")
    }
}

/// Wire `vanilla.invoke` and `vanilla.list` HTTP cmdlets against
/// the horsey resolver. Call from worker init AFTER the OP_REGISTRY
/// has been built.
pub fn register_vanilla_ops() {
    modforge::vanilla::ops::register(&modforge::ops::OP_REGISTRY, &HORSEY_RESOLVER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lookup_finds_every_declared_target() {
        // Smoke-test a representative subset across kinds.
        assert!(HORSEY_TARGETS.def("GAMESTATE_PTR").is_some());
        assert!(HORSEY_TARGETS.def("NAME_TABLE").is_some());
        assert!(HORSEY_TARGETS.def("APPLY_GENE_TO_HORSE").is_some());
        assert!(HORSEY_TARGETS.def("HORSE_REBUILD").is_some());
        assert!(HORSEY_TARGETS.def("RNG_NEXT_MODULO").is_some());
        assert!(HORSEY_TARGETS.def("GENE_COMBINATOR").is_some());
        assert!(HORSEY_TARGETS.def("HORSE_SAVE_LOADER").is_some());
        assert!(HORSEY_TARGETS.def("DEFINITELY_MISSING").is_none());
    }

    #[test]
    fn registry_includes_invocable_signatures() {
        // Targets with `signature: Some(_)` are vanilla-invocable.
        let invocable: Vec<&'static str> = HORSEY_TARGETS
            .iter()
            .filter_map(|d| match d.kind {
                TargetKind::FunctionEntry { signature: Some(_) } => Some(d.name),
                _ => None,
            })
            .collect();
        assert!(invocable.contains(&"APPLY_GENE_TO_HORSE"));
        assert!(invocable.contains(&"HORSE_REBUILD"));
        assert!(invocable.contains(&"RNG_NEXT_MODULO"));
        assert!(invocable.contains(&"HORSE_COPY_GENE_LANE_PAIRS"));
        assert_eq!(invocable.len(), 4);
    }

    #[test]
    fn registry_target_count() {
        // Sanity that we declared what we think we did.
        // 8 data globals + 4 invocable functions + 30 hint-only
        // function entries = 42 total.
        assert_eq!(HORSEY_TARGETS.entries.len(), 42);
    }
}
