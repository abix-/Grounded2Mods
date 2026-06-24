//! Hardcoded function addresses + global addresses from Horsey.exe.
//!
//! Source: our 100% decompilation pass (build mtime 2026-05-08).
//! When the game updates, these will shift; the recovery path is
//! pattern-scan via `modforge::scanner` and re-derive the offset
//! table. For now we treat the May 2026 build as the reference.
//!
//! All addresses are PE relative-virtual offsets. To compute the
//! actual runtime address: `image_base + RVA`. The `image_base` is
//! resolved at process start via `GetModuleHandleW(NULL)`.

/// PE preferred image base. The game ships without ASLR randomization
/// in most launches, so this is also typically the runtime base.
/// `image_base()` below resolves the actual one.
pub const PREFERRED_IMAGE_BASE: usize = 0x140000000;

// =============================================================================
// Global game state pointers
// =============================================================================

/// Pointer to the global game-state struct.
///
/// Verified from `compute_save_path` callers and the cheat-money
/// handler at line 60473 of `decompiled/all_functions.c`:
///   `*(int *)(DAT_1403fb0d8 + 0x308) += 1000;`
///
/// Re-derived 2026-05-17 against image SHA matching the live build:
/// slot drifted from RVA 0x3fb0d8 to 0x3fc1e8. The registry pattern
/// `48 89 1D ?? ?? ?? ?? 48 89 BB 70 02 00 00` still matches
/// uniquely; the legacy 0x1000 hint tolerance was rejecting the
/// new slot.
pub const GAMESTATE_PTR: usize = 0x1403fc1e8;

/// Pointer to the loaded save format VERSION. Set by `load_game`
/// after reading the first uint32 of the save file.
/// Conditional reads in the save loader gate on `version > 11`.
pub const SAVE_VERSION_GLOBAL: usize = 0x1403fb0e0;

/// The Races counter. NOT inside GameState; a separate global.
/// Confirmed at line 84920 (`DAT_1403eded8 = 0;`).
pub const RACES_COUNTER: usize = 0x1403eded8;

/// MapState pointer slot. `*DAT_1403f4e00` -> MapState* (0x268 bytes).
/// Populated by `TMX_MAP_PARSER` from `data/horsey.tmx`. Field map in
/// HORSE-PLACES.md "Top-level singletons" section.
pub const MAP_STATE_PTR: usize = 0x1403f4e00;

/// World-coords -> tile-index scale factor (float). Multiply actor
/// position (`actor+0x28/+0x2c`, e.g. truck pos) by this to get a
/// tile index for `FUN_1401010d0(GS, tile_x, tile_y)` lookup.
/// Pervasive (62+ references in decomp). Lives in `.rdata`.
/// Likely `1.0 / 32.0 = 0.03125` (tile size = 32 px) but verify at
/// runtime via `scene::tile_scale()`.
pub const TILE_SCALE: usize = 0x140303fb4;

/// The "No Tire" / "Yes Tire" toggle byte. When non-zero, the
/// per-frame update loop zeroes horse.+0x205 / +0x206 on every
/// horse, effectively disabling tiredness.
/// Toggled from the debug cheat menu (line 60442 in old decomp).
/// Original decomp RVA was 0x1403d95c5; re-derived from the live
/// image 2026-05-15 via the cmp/sete-direct-to-same-byte sig.
/// Drift -0x20 from old hardcoded.
pub const NO_TIRE_TOGGLE: usize = 0x1403d95a5;

/// Debug-mode-active flag. Set when the player types "debug" in
/// the pause menu. Gates the cheat-menu buttons + extended status
/// line. Original decomp RVA was 0x1403d959b; re-derived from the
/// live image via the unlock-block sig (set 1 followed by set 0
/// at target1 - 0x79). Drift -0x20 from old hardcoded.
pub const DEBUG_MODE_ACTIVE: usize = 0x1403d957b;

/// Debug-mode log gate. When set, retirement messages,
/// `%s = (%d rand + ...)` price formulas, and other debug lines
/// print to the log. Independent of `DEBUG_MODE_ACTIVE`.
/// Original decomp RVA was 0x1403d9526; re-derived from the live
/// image via the global-init triplet sig (0x100, 0xffffffff, 0).
/// Drift -0x20 from old hardcoded.
pub const DEBUG_LOG_GATE: usize = 0x1403d9506;

/// Horse name table. `FUN_1400c78c0(name_id)` indexes this table at
/// stride 0x88 to resolve a `name_id` (u32 at horse+0x1f8) into a
/// std::string. See `horse::name` for the lookup. Decomp evidence at
/// `:116676-116697`. H-gb. Hardcoded for now; needs an R3 resolver
/// (anchor: any call site of `FUN_1400c78c0` decodes the RIP-rel
/// disp32 of the table reference).
pub const NAME_TABLE: usize = 0x1403f34e0;

// =============================================================================
// GameState struct field offsets (from DAT_1403fb0d8)
// =============================================================================

pub mod gs_offset {
    use std::sync::OnceLock;

    pub const FRAME_TICK: usize = 0x254;
    pub const FIELD_268: usize = 0x268;
    /// Old decomp constant. Production should call [`money()`].
    pub const MONEY: usize = 0x308;

    /// Pattern-resolved offset of the player's money on GameState.
    /// Anchors on the cheat-money handler inside DRAW_PAUSE_STATUS:
    /// `add dword [reg+disp32], 1000` (the `+$1000` cheat). The
    /// `e8 03 00 00` immediate (== 1000) is uniquely distinctive
    /// within the function body. Decodes the disp32 -> MONEY.
    ///
    /// Falls back to the hardcoded `MONEY` const when the pattern
    /// misses.
    pub fn money() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            let Some(dps) = crate::targets_registry::resolve::draw_pause_status() else {
                return MONEY;
            };
            // `add r/m32, imm32` with disp32 + 0x3e8 imm:
            //   81 <ModR/M> disp32 e8 03 00 00
            // ModR/M has reg=000 (/0 = add), mod=10, rm=base reg.
            // We wildcard the ModR/M byte and disp32; the imm is
            // literal `e8 03 00 00`. Total 10 bytes; disp32 at
            // offset 2. The function body's window covers the
            // whole DRAW_PAUSE_STATUS.
            const SIG: &str = "81 ?? ?? ?? ?? ?? e8 03 00 00";
            let hist = match modforge::research::in_process_decode_imm_in_window(
                SIG, 2, 4, dps as u64, 8192,
            ) {
                Ok(h) => h,
                Err(_) => return MONEY,
            };
            modforge::research::histogram_top(&hist)
                .map(|v| v as usize)
                .unwrap_or(MONEY)
        })
    }
    /// Old decomp constant. Production should call [`year()`] which
    /// pattern-resolves the offset at runtime.
    pub const YEAR: usize = 0x314;
    pub const SLEEPS: usize = 0x318;

    // Pause-menu format string `"< Simulation Paused - Year %d >"`.
    // Hex; used as the anchor for YEAR (only year passed).
    const PAUSE_YEAR_ONLY_HEX: &str = "3c 20 53 69 6d 75 6c 61 74 69 6f 6e 20 50 61 75 73 65 64 20 2d 20 59 65 61 72 20 25 64 20 3e";
    // Debug-mode pause-menu format string
    // `"< Simulation Paused - Year %d  Sleeps %d  Races %d >"`.
    // Hex; used as the anchor for SLEEPS (year + sleeps + races).
    const PAUSE_DEBUG_HEX: &str = "3c 20 53 69 6d 75 6c 61 74 69 6f 6e 20 50 61 75 73 65 64 20 2d 20 59 65 61 72 20 25 64 20 20 53 6c 65 65 70 73 20 25 64 20 20 52 61 63 65 73 20 25 64 20 3e";

    /// Pattern-resolved offset of the in-game year. Anchors on the
    /// pause-menu format string, finds the `lea rdx` that loads it,
    /// then decodes the `mov r8d, [base+disp32]` that supplies the
    /// year as arg3 (Windows x64 fastcall).
    ///
    /// Falls back to the hardcoded `YEAR` const when the pattern
    /// misses.
    pub fn year() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            match modforge::research::in_process_decode_field_offset_via_string(
                PAUSE_YEAR_ONLY_HEX, 0, "44 8b", 3, 4, 64,
            ) {
                Ok(hist) => modforge::research::histogram_top(&hist)
                    .map(|v| v as usize)
                    .unwrap_or(YEAR),
                Err(_) => YEAR,
            }
        })
    }

    /// Pattern-resolved offset of the sleeps counter. Anchors on
    /// the debug-mode pause-menu format string (which passes year +
    /// sleeps + races), finds the `lea rdx` to it, then decodes the
    /// `mov r9d, [base+disp32]` (arg4 = sleeps).
    ///
    /// The histogram will likely contain BOTH the year (r8d) and
    /// sleeps (r9d) loads since both use `44 8b XX disp32`. We use
    /// the SECOND-most-frequent value: year matches in BOTH format
    /// strings' xref windows, sleeps only in this one. Falls back
    /// to the hardcoded `SLEEPS` const when the pattern misses.
    pub fn sleeps() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            // `mov r9d, [base+disp32]` is encoded with ModR/M byte
            // 0x89 (r9 dest, [rcx] base) for the most common case.
            // The base register varies; we want any `44 8b XX` where
            // XX has top bit pattern 0b10001xxx (mod=10, reg=001 for
            // r9d, rm=base). All such ModR/Ms are 0x88..0x8F.
            // Easier: just scan `44 8b` and let the histogram find
            // the right disp via the YEAR vs SLEEPS frequency split.
            match modforge::research::in_process_decode_field_offset_via_string(
                PAUSE_DEBUG_HEX, 0, "44 8b", 3, 4, 64,
            ) {
                Ok(hist) => {
                    // Build a sorted view, pick the largest disp
                    // that's > YEAR's offset (sleeps is positioned
                    // after year in the struct).
                    let year_off = year() as i64;
                    let mut sorted: Vec<(i64, usize)> = hist.into_iter().collect();
                    sorted.sort_by(|a, b| b.1.cmp(&a.1));
                    // Find the most-frequent disp that's NOT year.
                    sorted.iter()
                        .find(|(d, _)| *d != year_off && *d > 0)
                        .map(|(d, _)| *d as usize)
                        .unwrap_or(SLEEPS)
                }
                Err(_) => SLEEPS,
            }
        })
    }
    /// Start of the 6-supply array. Each supply is 8 bytes:
    /// `[uint32 count][uint8 flag_a][uint8 flag_b][2 bytes pad]`.
    /// The 7th supply field (used by the Loaded cheat) at +0x344 is
    /// in the same stride; the writer/loader walks 6 then handles
    /// the 7th separately.
    pub const SUPPLIES_START: usize = 0x31c;
    pub const FIELD_37C: usize = 0x37c;
    pub const FIELD_39C: usize = 0x39c;
    pub const FIELD_410: usize = 0x410;
    pub const FIELD_414: usize = 0x414;
    pub const FIELD_415: usize = 0x415;
    pub const FIELD_418: usize = 0x418;
    pub const FIELD_41C: usize = 0x41c;
    pub const FIELD_440: usize = 0x440;
    /// Begin pointer for the horse vector (stride 0x24 = 36 bytes
    /// per in-memory horse). Old decomp constant; production
    /// should call [`horses_begin()`].
    pub const HORSES_BEGIN: usize = 0x280;
    /// End pointer for the horse vector. Old decomp constant;
    /// production should call [`horses_end()`].
    pub const HORSES_END: usize = 0x288;

    fn resolve_horses_pair() -> Option<(usize, usize)> {
        // Scan for two adjacent `mov reg64, [base+disp32]` loads
        // where disp2 == disp1 + 8. Multiple GameState fields are
        // pointer pairs (live_horses at 0x130/0x138, roster at
        // 0x280/0x288). To pick the ROSTER pair, filter to disps
        // >= 0x200 (live-horses pair is below 0x200).
        let hist = modforge::research::in_process_decode_disp_pair_with_delta(
            "48 8b ?? ?? ?? ?? ?? 48 8b ?? ?? ?? ?? ??",
            3, 10, 4, 8,
        ).ok()?;
        // Constrain to [0x200, 0x300]: above the live-horses pair
        // (0x130), below other GameState pointer pairs (e.g. 0x4a0).
        let mut top: Vec<(i64, usize)> = hist.into_iter()
            .filter(|(v, _)| *v >= 0x200 && *v < 0x300)
            .collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        top.first().map(|&(d, _)| (d as usize, (d + 8) as usize))
    }

    fn resolve_sim_horses_pair() -> Option<(usize, usize)> {
        // Same scan, narrowed to the sim-horses neighborhood
        // (HLT labels these `kOffSimHorsesBegin/End` 0x260/0x268).
        let hist = modforge::research::in_process_decode_disp_pair_with_delta(
            "48 8b ?? ?? ?? ?? ?? 48 8b ?? ?? ?? ?? ??",
            3, 10, 4, 8,
        ).ok()?;
        let mut top: Vec<(i64, usize)> = hist.into_iter()
            .filter(|(v, _)| *v >= 0x250 && *v < 0x270)
            .collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        top.first().map(|&(d, _)| (d as usize, (d + 8) as usize))
    }

    /// Pattern-resolved offset of the sim-horse pointer list begin.
    /// HLT calls this `kOffSimHorsesBegin` (0x260); separate from
    /// the roster (0x280) and live-horses (0x130) pairs.
    pub fn sim_horses_begin() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_sim_horses_pair().map(|p| p.0).unwrap_or(0x260))
    }

    /// Pattern-resolved offset of the sim-horse pointer list end
    /// (= `sim_horses_begin + 8`). Was hardcoded as `FIELD_268`.
    pub fn sim_horses_end() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_sim_horses_pair().map(|p| p.1).unwrap_or(FIELD_268))
    }

    fn resolve_live_horses_pair() -> Option<(usize, usize)> {
        // Same scan, narrowed to the live-horses neighborhood
        // (< 0x200).
        let hist = modforge::research::in_process_decode_disp_pair_with_delta(
            "48 8b ?? ?? ?? ?? ?? 48 8b ?? ?? ?? ?? ??",
            3, 10, 4, 8,
        ).ok()?;
        let mut top: Vec<(i64, usize)> = hist.into_iter()
            .filter(|(v, _)| *v >= 0x100 && *v < 0x200)
            .collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        top.first().map(|&(d, _)| (d as usize, (d + 8) as usize))
    }

    /// Pattern-resolved offset of the LIVE-horse pointer list
    /// begin (separate from the roster `horses_begin`).
    pub fn live_horses_begin() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_live_horses_pair().map(|p| p.0).unwrap_or(0x130))
    }
    /// Pattern-resolved offset of the LIVE-horse pointer list end.
    pub fn live_horses_end() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_live_horses_pair().map(|p| p.1).unwrap_or(0x138))
    }

    pub fn horses_begin() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_horses_pair().map(|p| p.0).unwrap_or(HORSES_BEGIN))
    }

    pub fn horses_end() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_horses_pair().map(|p| p.1).unwrap_or(HORSES_END))
    }
    /// Old decomp constant. Production should call [`map_width()`].
    /// HLT labels this `kOffMapWidth`.
    pub const TRAILING_278: usize = 0x278;
    /// Old decomp constant. Production should call [`map_height()`].
    /// HLT labels this `kOffMapHeight`.
    pub const TRAILING_27C: usize = 0x27c;

    fn resolve_map_dims_pair() -> Option<(usize, usize)> {
        // Width and height are loaded back-to-back for the tile-count
        // multiplication (`width * height`). Scan for two adjacent
        // 32-bit field loads (`mov r32, [base+disp32]`) with delta 4
        // and disp1 in the trailing-GameState band [0x270, 0x290].
        let hist = modforge::research::in_process_decode_disp_pair_with_delta(
            "8b ?? ?? ?? ?? ?? 8b ?? ?? ?? ?? ??",
            2, 8, 4, 4,
        ).ok()?;
        let mut top: Vec<(i64, usize)> = hist.into_iter()
            .filter(|(v, _)| *v >= 0x270 && *v < 0x290)
            .collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        top.first().map(|&(d, _)| (d as usize, (d + 4) as usize))
    }

    /// Pattern-resolved offset of the world-map width int32.
    /// Anchors on the width/height back-to-back load pair (HLT
    /// `kOffMapWidth/kOffMapHeight`). Falls back to `TRAILING_278`.
    pub fn map_width() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_map_dims_pair().map(|p| p.0).unwrap_or(TRAILING_278))
    }

    /// Pattern-resolved offset of the world-map height int32
    /// (= `map_width + 4`). Falls back to `TRAILING_27C`.
    pub fn map_height() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_map_dims_pair().map(|p| p.1).unwrap_or(TRAILING_27C))
    }

    /// Old decomp constant for the roster entry stride (each
    /// in-memory horse-list entry is 36 bytes). Production should
    /// call [`roster_stride()`].
    pub const ROSTER_STRIDE: usize = 0x24;

    /// Pattern-resolved roster entry stride. Anchors on the
    /// per-horse save writer `HORSE_SAVE_WRITER`: the iterator
    /// function (`FUN_14006d610`) calls it once per roster entry,
    /// and just before each call computes
    /// `entry = roster_base + index * stride` via
    /// `imul reg, reg, <stride>` (encoded `6b ?? <imm8>`). Look back
    /// 128 bytes from every call site, histogram imm8 of any
    /// `imul r32, r/m32, imm8` instruction, top wins.
    ///
    /// Falls back to the hardcoded `ROSTER_STRIDE` const.
    pub fn roster_stride() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_roster_stride().unwrap_or(ROSTER_STRIDE))
    }

    fn resolve_roster_stride() -> Option<usize> {
        let target = crate::targets_registry::resolve::horse_save_writer()?;
        let sig = format!("e8 X0x{target:x}");
        let call_sites = modforge::patterns::sleuth::scan_all_matches(&sig).ok()?;
        if call_sites.is_empty() {
            return None;
        }
        const LOOKBACK: usize = 128;
        let mut hist: std::collections::BTreeMap<i64, usize> =
            std::collections::BTreeMap::new();
        for call_addr in call_sites {
            let start = call_addr.saturating_sub(LOOKBACK);
            // SAFETY: start..call_addr is inside .text, mapped.
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(start as *const u8, LOOKBACK)
            };
            for i in 0..bytes.len().saturating_sub(3) {
                if bytes[i] == 0x6b {
                    let imm8 = bytes[i + 2] as i8 as i64;
                    if (0x10..=0xff).contains(&imm8) {
                        *hist.entry(imm8).or_insert(0) += 1;
                    }
                }
            }
        }
        hist.into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(v, _)| v as usize)
    }

    /// Old decomp constant for the GameState struct alloc size.
    /// Production should call [`alloc_size()`].
    pub const GAMESTATE_ALLOC_SIZE: usize = 0x448;

    /// Pattern-resolved GameState struct alloc size. Anchors on
    /// the unique `mov [rip+disp32], reg` store of the new GameState
    /// pointer to `GAMESTATE_PTR` (inside the constructor), then
    /// looks back ~256 bytes for `b9 <imm32>` (mov ecx, alloc_size)
    /// that preceded the allocator call producing the buffer.
    /// Histograms the imms; top in `[0x100, 0x10000)` is the size.
    ///
    /// Falls back to `GAMESTATE_ALLOC_SIZE` when the resolver chain
    /// misses.
    pub fn alloc_size() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_alloc_size().unwrap_or(GAMESTATE_ALLOC_SIZE))
    }

    fn resolve_alloc_size() -> Option<usize> {
        let gs_ptr = crate::targets_registry::resolve::gamestate_ptr()?;
        // `mov [rip+disp32], reg` for any of the 8 common destination
        // regs. The disp32 must rip-rel to `gs_ptr`.
        const STORE_PREFIXES: &[&str] = &[
            "48 89 05", "48 89 0d", "48 89 15", "48 89 1d",
            "48 89 2d", "48 89 35", "48 89 3d",
        ];
        let mut anchor: Option<usize> = None;
        for prefix in STORE_PREFIXES {
            let sig = format!("{prefix} X0x{gs_ptr:x}");
            if let Ok(hits) = modforge::patterns::sleuth::scan_all_matches(&sig) {
                if let Some(&a) = hits.first() {
                    anchor = Some(a);
                    break;
                }
            }
        }
        let anchor = anchor?;
        const LOOKBACK: usize = 256;
        let start = anchor.saturating_sub(LOOKBACK);
        // SAFETY: start..anchor is inside .text, mapped.
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(start as *const u8, LOOKBACK)
        };
        let mut hist: std::collections::BTreeMap<i64, usize> =
            std::collections::BTreeMap::new();
        for i in 0..bytes.len().saturating_sub(5) {
            if bytes[i] == 0xb9 {
                let imm = i32::from_le_bytes([
                    bytes[i + 1], bytes[i + 2], bytes[i + 3], bytes[i + 4],
                ]) as i64;
                if (0x100..0x10000).contains(&imm) {
                    *hist.entry(imm).or_insert(0) += 1;
                }
            }
        }
        hist.into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(v, _)| v as usize)
    }
}

// =============================================================================
// Horse struct field offsets
// =============================================================================
//
// Note: "horse pointer" in different parts of the engine references
// different concrete types. The track manager iterates one list
// (`+0x130/+0x138`); the BarnMating handler accesses parents via
// `param_1[0x88]` and `param_1[0x89]`. We use the same field
// offsets in both. The struct is the same; the wrapping containers
// differ.

pub mod horse_offset {
    use std::sync::OnceLock;

    /// Old decomp constant. Production should call [`type_or_species()`].
    /// Horse type/species code. Compared to 1/2/4/6 in various
    /// dispatchers.
    pub const TYPE_OR_SPECIES: usize = 0x1c;

    /// Pattern-resolved horse type/species offset. Anchors on the
    /// `cmp dword [horse+disp8], imm8` dispatcher sites scattered
    /// across `.text` (the type code is compared to 1, 2, 4, 6 in
    /// dozens of places). Pattern `83 7? <disp8> <imm8>` decodes to
    /// `cmp r/m32, imm8` with `/7` opcode extension; filter modrm
    /// to non-rbp/rsp register bases. Histogram disp8 across all
    /// four imm values; the dominant value in `[0x10, 0x80]` is
    /// TYPE_OR_SPECIES.
    ///
    /// Falls back to the hardcoded `TYPE_OR_SPECIES` const.
    pub fn type_or_species() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_type_or_species().unwrap_or(TYPE_OR_SPECIES))
    }

    fn resolve_type_or_species() -> Option<usize> {
        let mut hist: std::collections::BTreeMap<i64, usize> =
            std::collections::BTreeMap::new();
        for imm in &[1u8, 2, 4, 6] {
            let sig = format!("83 ?? ?? {imm:02x}");
            let hits = match modforge::patterns::sleuth::scan_all_matches(&sig) {
                Ok(h) => h,
                Err(_) => continue,
            };
            for addr in hits {
                // SAFETY: addr matched in .text, 4 bytes mapped.
                let bytes: &[u8] = unsafe {
                    std::slice::from_raw_parts(addr as *const u8, 4)
                };
                let modrm = bytes[1];
                // mod=01 (disp8) + reg=111 (cmp /7) = 0x78..0x7F.
                let is_cmp7 = (modrm & 0xC0) == 0x40 && (modrm & 0x38) == 0x38;
                if !is_cmp7 {
                    continue;
                }
                // Exclude rsp (rm=100) and rbp (rm=101) bases:
                // those are stack-frame cmps, not horse-field reads.
                let rm = modrm & 0x07;
                if rm == 4 || rm == 5 {
                    continue;
                }
                let disp = bytes[2] as i8 as i64;
                if (0x10..0x80).contains(&disp) {
                    *hist.entry(disp).or_insert(0) += 1;
                }
            }
        }
        hist.into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(v, _)| v as usize)
    }
    /// Per-horse "container kind" cache at +0x1d0 (u32). Byte-diff of a
    /// real manual drag (HK1 research, docs/HK1-SHIFT-CLICK-TRANSFER-PLAN.md
    /// section 5b) showed: 7 = trailer/truck, 9 = pasture, 0 or 2 right
    /// after a save/reload. Flagged a DISPLAY CACHE the game recomputes
    /// each tick. Writing it does NOT move the horse. But it is the
    /// first hypothesis for READING trailer-vs-pasture. Zero-hardcoding
    /// audit: H-gb research-stage; hardcoded until live-confirmed as the
    /// authoritative read source, then give it a resolver/classification.
    pub const CONTAINER_KIND: usize = 0x1d0;

    /// Per-horse scene placement position (two f32: x at +0x1d4, y at
    /// +0x1d8). Live-confirmed 2026-06-23 as the trailer-determining
    /// field: a trailer horse reads its trailer position (~13, 9); a
    /// pasture horse reads (0, 0) on the overworld. Drop writes it
    /// (FUN_1400d2ab0 ~:1888); the location-enter handler re-places
    /// horses from it (FUN_1400cd5a0 ~:110-112). H-gb research-stage;
    /// hardcoded until classified.
    pub const SCENE_POS_X: usize = 0x1d4;
    pub const SCENE_POS_Y: usize = 0x1d8;

    /// Horse name ID (passed to `name_resolve` / FUN_1400c78c0).
    pub const NAME_ID: usize = 0x1f8;
    /// Horse age (years; int32). Old decomp constant; production
    /// should call [`age()`].
    pub const AGE: usize = 0x1fc;
    /// Max age / lifespan threshold (int32). Compared as
    /// `age < max_age - 1` in the release-to-wild branch.
    /// Old decomp constant; production should call [`max_age()`].
    pub const MAX_AGE: usize = 0x200;

    // Retire handler format string: `"%s retired %s (bails) age %d ch %d"`.
    // Unique in .rdata. horse.age is the 4th printf arg, loaded into
    // r9d via `44 8b 8? <disp32>` right after the lea-to-string.
    const BAILS_AGE_HEX: &str =
        "25 73 20 72 65 74 69 72 65 64 20 25 73 20 28 62 61 69 6c 73 29 20 61 67 65 20 25 64 20 63 68 20 25 64";

    fn resolve_age() -> Option<usize> {
        let hist = modforge::research::in_process_decode_field_offset_via_string(
            BAILS_AGE_HEX, 0, "44 8b", 3, 4, 128,
        ).ok()?;
        // Filter to plausible horse-struct field range and pick the
        // most-frequent disp. AGE is loaded for the printf arg; the
        // window may also contain the horse.name pointer load
        // (different opcode, not 44 8b) or other r8d/r9d field loads.
        let mut sorted: Vec<(i64, usize)> = hist.into_iter()
            .filter(|(v, _)| *v >= 0x100 && *v < 0x300)
            .collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.first().map(|&(d, _)| d as usize)
    }

    /// Pattern-resolved horse-struct age offset. Anchors on the
    /// `(bails)` retirement format string, finds the lea xref, then
    /// decodes the `mov r9d, [horse+disp32]` that supplies age as
    /// the 4th printf arg. Falls back to the hardcoded `AGE` const.
    pub fn age() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_age().unwrap_or(AGE))
    }

    /// Pattern-resolved max-age offset. Derived by adjacency from
    /// [`age`] (documented to sit at `age + 4` as the next int32
    /// field; verified in retire_horse_handler's release-to-wild
    /// branch which loads both). Falls back to `MAX_AGE` when the
    /// AGE resolver misses.
    pub fn max_age() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_age().map(|a| a + 4).unwrap_or(MAX_AGE))
    }

    /// Pattern-resolved name-id offset. Anchors on the (bails)
    /// retire-handler format string and decodes the
    /// `mov ecx, [horse+disp32]; call name_resolve` pair that sets
    /// up the 3rd printf arg (horse.name = name_resolve(name_id)).
    /// Pattern `8b 8? <disp32> e8 <rel32>` is 11 bytes with disp32
    /// at offset 2. Filters to `[0x1d0, 0x210]` to avoid colliding
    /// with the adjacent `age` (0x1fc) which is loaded with
    /// `44 8b` (r9d), not `8b` (ecx).
    ///
    /// Falls back to the hardcoded `NAME_ID` const.
    pub fn name_id() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            let hist = match modforge::research::in_process_decode_field_offset_via_string(
                BAILS_AGE_HEX, 0, "8b", 2, 4, 128,
            ) {
                Ok(h) => h,
                Err(_) => return NAME_ID,
            };
            let mut top: Vec<(i64, usize)> = hist.into_iter()
                .filter(|(v, _)| *v >= 0x1d0 && *v < 0x210 && *v != age() as i64 && *v != max_age() as i64)
                .collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            top.first().map(|&(d, _)| d as usize).unwrap_or(NAME_ID)
        })
    }
    /// "On track / unavailable" flag (uint8). When 0, price doubles.
    pub const ON_TRACK_FLAG: usize = 0x204;
    /// Tiredness flag A (uint8). Zeroed every frame by Yes Tire.
    /// Old decomp constant; production should call [`tired_flag_a()`].
    pub const TIRED_FLAG_A: usize = 0x205;
    /// Tiredness flag B (uint8). Zeroed every frame by Yes Tire.
    /// Old decomp constant; production should call [`tired_flag_b()`].
    pub const TIRED_FLAG_B: usize = 0x206;

    /// Old decomp constants for the no_tire per-frame loop function
    /// (`FUN_1400ceb60` in the decomp). Production should call
    /// [`no_tire_loop_entry`] and [`no_tire_loop_size`].
    pub const NO_TIRE_LOOP_FN_RVA: usize = 0x1400ceb60;
    pub const NO_TIRE_LOOP_BODY_SIZE: usize = 2502;

    /// Globally scan `.text` for the unique adjacent pair of byte-
    /// zero stores at `horse+disp_a` and `horse+disp_b` (delta 1)
    /// that the no_tire per-frame loop runs each frame. Returns
    /// `(disp_a, disp_b, location_of_first_store)`. The location is
    /// inside the no_tire loop body and can be walked outward to
    /// recover function bounds.
    fn find_no_tire_pair() -> Option<(usize, usize, usize)> {
        static CACHE: OnceLock<Option<(usize, usize, usize)>> = OnceLock::new();
        *CACHE.get_or_init(|| {
            let hits = modforge::patterns::sleuth::scan_all_matches(
                "c6 ?? ?? ?? ?? ?? 00",
            ).ok()?;
            // Read disp32 from each candidate store and keep those
            // whose disp falls in the horse-struct flag region.
            let mut stores: Vec<(usize, i32)> = Vec::with_capacity(hits.len());
            for addr in hits {
                // SAFETY: addr matched in .text, 7 bytes mapped.
                let bytes: &[u8] = unsafe {
                    std::slice::from_raw_parts(addr as *const u8, 7)
                };
                let disp = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
                if (0x100..0x500).contains(&disp) {
                    stores.push((addr, disp));
                }
            }
            stores.sort_by_key(|&(a, _)| a);
            // Find adjacent stores within 64 bytes with delta=1 disps.
            for i in 0..stores.len() {
                for j in i + 1..stores.len() {
                    if stores[j].0 - stores[i].0 > 64 {
                        break;
                    }
                    let (d1, d2) = (stores[i].1, stores[j].1);
                    if d2 == d1 + 1 {
                        return Some((d1 as usize, d2 as usize, stores[i].0));
                    }
                    if d1 == d2 + 1 {
                        return Some((d2 as usize, d1 as usize, stores[j].0));
                    }
                }
            }
            None
        })
    }

    fn resolve_tired_pair() -> Option<(usize, usize)> {
        find_no_tire_pair().map(|(a, b, _)| (a, b))
    }

    /// Pattern-resolved entry RVA of the no_tire per-frame loop
    /// function. Anchors on the unique adjacent tired-flag byte-
    /// zero pair (see [`find_no_tire_pair`]) and walks back through
    /// MSVC `0xcc` inter-function padding via
    /// [`super::find_function_bounds_via_int3`] to find the
    /// function entry. Falls back to the hardcoded
    /// `NO_TIRE_LOOP_FN_RVA` (rebased) on miss.
    pub fn no_tire_loop_entry() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            no_tire_loop_bounds_resolved()
                .map(|(s, _)| s)
                .unwrap_or_else(|| super::rebase(NO_TIRE_LOOP_FN_RVA))
        })
    }

    /// Pattern-resolved body size of the no_tire per-frame loop
    /// function (end of body minus entry; the body is `[entry,
    /// entry + size)`). Falls back to `NO_TIRE_LOOP_BODY_SIZE` on
    /// miss.
    pub fn no_tire_loop_size() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            no_tire_loop_bounds_resolved()
                .map(|(s, e)| e - s)
                .unwrap_or(NO_TIRE_LOOP_BODY_SIZE)
        })
    }

    fn no_tire_loop_bounds_resolved() -> Option<(usize, usize)> {
        let (_a, _b, loc) = find_no_tire_pair()?;
        super::find_function_bounds_via_int3(loc, 4096, 4096)
    }

    /// Pattern-resolved offset of tiredness flag A. Anchors on the
    /// per-frame no_tire loop's two byte-zero stores: the LOWER of
    /// the adjacent flag-zeroing disps is TIRED_FLAG_A.
    pub fn tired_flag_a() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_tired_pair().map(|p| p.0).unwrap_or(TIRED_FLAG_A))
    }

    /// Pattern-resolved offset of tiredness flag B (= TIRED_FLAG_A + 1).
    pub fn tired_flag_b() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_tired_pair().map(|p| p.1).unwrap_or(TIRED_FLAG_B))
    }
    /// Breeding flag (uint8). Set on both parents during BarnMating.
    pub const BREEDING_FLAG: usize = 0x207;

    /// Pattern-resolved offset of the on-track flag.
    ///
    /// Derived by adjacency from the pattern-resolved tired-flag
    /// pair: documented to sit at `tired_flag_a - 1` in the
    /// horse-struct flag region (0x204 / 0x205 / 0x206 / 0x207 are
    /// on_track / tired_a / tired_b / breeding). The underlying
    /// anchor is the no_tire per-frame loop's adjacent byte-zero
    /// stores. If the no_tire pair resolver misses, falls back to
    /// the hardcoded `ON_TRACK_FLAG`.
    pub fn on_track_flag() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            resolve_tired_pair()
                .map(|(a, _)| a.saturating_sub(1))
                .unwrap_or(ON_TRACK_FLAG)
        })
    }

    /// Pattern-resolved offset of the breeding flag.
    ///
    /// Derived by adjacency from the pattern-resolved tired-flag
    /// pair (= `tired_flag_b + 1`). Same anchor and fallback as
    /// [`on_track_flag`].
    pub fn breeding_flag() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            resolve_tired_pair()
                .map(|(_, b)| b + 1)
                .unwrap_or(BREEDING_FLAG)
        })
    }
    /// Skill / fitness counter (int32). Used by retirement.
    /// Old decomp constant; production should call [`skill()`].
    pub const SKILL: usize = 0x21c;

    // Retire handler (useless) format string. Skill check is the
    // gate on the (useless) branch (`*(int*)(horse + skill) <
    // (mode != 4 ? 2 : 1)`), loaded ~50-200 bytes BEFORE the
    // (useless) lea. Wider window than BAILS_AGE_HEX.
    const USELESS_AGE_HEX: &str =
        "25 73 20 72 65 74 69 72 65 64 20 25 73 20 28 75 73 65 6c 65 73 73 29 20 61 67 65 20 25 64 20 63 68 20 25 64";

    /// Pattern-resolved skill offset. Anchors on the (useless)
    /// retire-handler format string with a 256-byte ±window and
    /// scans for `8b` (mov r32) loads. Filters to `[0x210, 0x240]`
    /// to bracket the documented `0x21c` between the surrounding
    /// known offsets (BREEDING_FLAG 0x207, LITTER_SIZE_STAT 0x254)
    /// and excludes already-resolved neighbors.
    ///
    /// Falls back to the hardcoded `SKILL` const.
    pub fn skill() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            let hist = match modforge::research::in_process_decode_field_offset_via_string(
                USELESS_AGE_HEX, 0, "8b", 2, 4, 256,
            ) {
                Ok(h) => h,
                Err(_) => return SKILL,
            };
            let mut top: Vec<(i64, usize)> = hist.into_iter()
                .filter(|(v, _)| *v >= 0x210 && *v < 0x240)
                .collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            top.first().map(|&(d, _)| d as usize).unwrap_or(SKILL)
        })
    }
    /// Old decomp constant. Production should call [`litter_size_stat()`].
    /// Litter-size stat (int32). Used in breeding: children =
    /// min(parent_a.litter, parent_b.litter) + rng_bonus.
    pub const LITTER_SIZE_STAT: usize = 0x254;

    /// Pattern-resolved litter-size stat offset. Anchors on the
    /// BREEDING state-machine body where parent_a.litter and
    /// parent_b.litter are both loaded for the `min(a, b)` step.
    /// Scans for `8b 8? <disp32>` (mov r32, [reg+disp32]) within
    /// an 8KB window from BREEDING's entry, filters to
    /// `[0x240, 0x270]`, and picks the most-frequent disp (counted
    /// at least twice since both parents read the same offset).
    ///
    /// Falls back to `LITTER_SIZE_STAT` when the resolver chain
    /// misses.
    pub fn litter_size_stat() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_litter_size_stat().unwrap_or(LITTER_SIZE_STAT))
    }

    fn resolve_litter_size_stat() -> Option<usize> {
        let entry = crate::targets_registry::resolve::breeding()?;
        let hist = modforge::research::in_process_decode_imm_in_window(
            "8b", 2, 4, entry as u64, 8192,
        ).ok()?;
        let mut top: Vec<(i64, usize)> = hist.into_iter()
            .filter(|(v, _)| *v >= 0x240 && *v < 0x270)
            .collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        top.first().map(|&(d, _)| d as usize)
    }

    /// Old decomp constant for the working-genome offset on the
    /// Horse struct. Production should call [`ctx_offset()`].
    pub const HORSE_CTX_OFFSET: usize = 0x2b8;

    /// Pattern-resolved offset of the working genome inside the
    /// Horse struct. Anchors on `HORSE_SAVE_LOADER`'s body, where
    /// the function does `add rcx, 0x2b8` (or whatever the live
    /// offset is) to advance from the Horse base to the genome
    /// buffer. Decodes the imm32 of that instruction.
    ///
    /// Falls back to the hardcoded `HORSE_CTX_OFFSET` const when
    /// the pattern misses.
    pub fn ctx_offset() -> usize {
        use std::sync::OnceLock;
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| {
            let Some(loader) = crate::targets_registry::resolve::horse_save_loader() else {
                return HORSE_CTX_OFFSET;
            };
            // Scan within the first 256 bytes of HORSE_SAVE_LOADER
            // for `48 81 c1 ?? ?? ?? ??` (add rcx, imm32).
            let hist = match modforge::research::in_process_decode_imm_in_window(
                "48 81 c1", 3, 4, loader as u64, 256,
            ) {
                Ok(h) => h,
                Err(_) => return HORSE_CTX_OFFSET,
            };
            // Pick the most-frequent imm; expect exactly one
            // `add rcx, 0x2b8` in the function prologue.
            modforge::research::histogram_top(&hist)
                .map(|v| v as usize)
                .unwrap_or(HORSE_CTX_OFFSET)
        })
    }

    /// Old decomp constant for the Horse struct alloc size. Production
    /// should call [`alloc_size()`].
    pub const HORSE_ALLOC_SIZE: usize = 0x498;

    /// Pattern-resolved Horse struct alloc size. Anchors on every
    /// `e8 X<HORSE_CONSTRUCTOR>` call site, looks back ~32 bytes for
    /// `b9 <imm32>` (mov ecx, imm32) which is the allocator-size
    /// load preceding the `FUN_1402c704c(0x498)` call that produces
    /// the buffer passed to the constructor. Histograms the imms
    /// across all call sites; the most-frequent value in
    /// `[0x100, 0x10000)` is the struct size.
    ///
    /// Falls back to `HORSE_ALLOC_SIZE` when the resolver chain
    /// (function entry or call-site lookback) misses.
    pub fn alloc_size() -> usize {
        static CACHE: OnceLock<usize> = OnceLock::new();
        *CACHE.get_or_init(|| resolve_alloc_size().unwrap_or(HORSE_ALLOC_SIZE))
    }

    fn resolve_alloc_size() -> Option<usize> {
        let target = crate::targets_registry::resolve::horse_constructor()?;
        let sig = format!("e8 X0x{target:x}");
        let call_sites = modforge::patterns::sleuth::scan_all_matches(&sig).ok()?;
        if call_sites.is_empty() {
            return None;
        }
        const LOOKBACK: usize = 32;
        let mut hist: std::collections::BTreeMap<i64, usize> =
            std::collections::BTreeMap::new();
        for call_addr in call_sites {
            let start = call_addr.saturating_sub(LOOKBACK);
            // SAFETY: start..call_addr is inside .text, mapped.
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(start as *const u8, LOOKBACK)
            };
            for i in 0..bytes.len().saturating_sub(5) {
                if bytes[i] == 0xb9 {
                    let imm = i32::from_le_bytes([
                        bytes[i + 1], bytes[i + 2], bytes[i + 3], bytes[i + 4],
                    ]) as i64;
                    if (0x100..0x10000).contains(&imm) {
                        *hist.entry(imm).or_insert(0) += 1;
                    }
                }
            }
        }
        hist.into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(v, _)| v as usize)
    }
}

// =============================================================================
// Game function addresses (for MinHook trampoline targets)
// =============================================================================

// CRITICAL CORRECTION (2026-05-14):
//
// Ghidra's pyghidra decomp pass indexes function entries OFF-BY-16
// for the MSVC-compiled Horsey.exe. Every address in the pyghidra
// `INDEX.md` points 16 bytes INTO the function body, past the
// register-save prologue. The true entry is at `ghidra_addr - 16`.
//
// Why this matters: if we hook at Ghidra's address, retour's
// trampoline copies bytes that depend on register values set up by
// the SKIPPED prologue (specifically `mov rax, rsp; push rbp; ...`
// for big functions). When our handler runs between caller-CALL and
// trampoline-execution, it clobbers RAX (volatile in Win64 ABI),
// and the trampoline's relocated `lea rbp, [rax-X]` reads garbage.
//
// Most functions Ghidra indexed in this build have one of these
// shapes 16 bytes BEFORE their indexed entry:
//   - `48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57`
//     = `mov rax, rsp; push rbp; push rbx; ... push r15`
//   - `48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57`
//     = shadow-space saves of rbx/rbp/rsi + push rdi
//
// All addresses below have been adjusted to the TRUE entry. The
// original Ghidra address is in a comment for cross-reference
// with `horsey-mod/research/decompiled/INDEX.md`.

pub mod fn_addr {
    /// `apply_gene_to_horse`. The gene expression function. The
    /// entry point for "construct a horse from its genome". Hook
    /// here to attach extension state to new horses.
    /// Ghidra indexed as `FUN_14009f680`; true entry -16 bytes.
    pub const APPLY_GENE_TO_HORSE: usize = 0x14009f670;

    /// `eval_diploid_blend_a`. One of two allele-blend evaluators.
    /// Takes `(horse_genome*, gene_idx: i32)` and returns a blended
    /// `f32`. Called by `APPLY_GENE_TO_HORSE` 233 times per horse
    /// with literal indices 0..239. D1 trampoline target:
    /// dispatch to `EXT_GENE_TABLE` when `gene_idx >= 240`.
    /// Ghidra indexed as `FUN_1400a5d20`; true entry -16 bytes.
    pub const EVAL_DIPLOID_BLEND_A: usize = 0x1400a5d10;

    /// `eval_diploid_blend_b`. Sibling evaluator using the same
    /// record layout but a different blend formula. Same trampoline
    /// strategy as the A variant.
    /// Ghidra indexed as `FUN_1400a5e00`; true entry -16 bytes.
    pub const EVAL_DIPLOID_BLEND_B: usize = 0x1400a5df0;

    /// `gene_table_consumer_or_horse_dies`. The "horse death"
    /// handler that ALSO mutates gene-table mutation rates by +/-5.
    /// D1 trampoline target: when its mutation targets
    /// `gene_idx >= 240`, redirect to `EXT_GENE_TABLE`.
    /// Ghidra indexed as `FUN_1400c0660`; true entry -16 bytes.
    pub const GENE_DEATH_DRIFT: usize = 0x1400c0650;

    /// `gene_allele_renumber_sync`. When alleles X and Y of a gene
    /// are swapped, this function syncs the swap across every pop
    /// record. D1 trampoline target: when called with
    /// `gene_idx >= 240`, swap inside `EXT_POP_WEIGHTS` and
    /// `EXT_GENE_TABLE` instead.
    /// Ghidra indexed as `FUN_1400c03a0`; true entry -16 bytes.
    pub const GENE_ALLELE_SWAP: usize = 0x1400c0390;

    /// `gene_table_xml_writer` (`FUN_1400a4880`). Serializes the
    /// 240-slot gene table back to XML. Vanilla iterates 0..239.
    /// D1 trampoline target (optional): also dump
    /// `EXT_GENE_TABLE` to a sidecar XML.
    pub const GENE_TABLE_XML_WRITER: usize = 0x1400a4880;

    /// `gene_table_loader` (`FUN_1400a3eb0`). Loads `genes.xml`
    /// into the 240-slot table at startup. Not patched in v1; we
    /// run our own loader for `genes-extended.xml` from
    /// `worker_main`.
    pub const GENE_TABLE_LOADER: usize = 0x1400a3eb0;

    /// `pop_xml_loader` (`FUN_1400a4fe0`). Loads `pop.xml`
    /// populations. Not patched in v1; pop authoring stays vanilla
    /// and our extended pop weights are stored in `EXT_POP_WEIGHTS`.
    pub const POP_XML_LOADER: usize = 0x1400a4fe0;

    /// `gene_engine_consumer`. Called immediately after
    /// `APPLY_GENE_TO_HORSE` to transcribe the temp render param
    /// array into the persistent horse render struct. The D5
    /// trampoline lives at the boundary between APPLY_GENE_TO_HORSE
    /// and this function.
    /// Ghidra indexed as `FUN_1400ab3d0`; true entry -16 bytes.
    pub const GENE_ENGINE_CONSUMER: usize = 0x1400ab3c0;

    /// `check_horse_eligibility`. Re-derived 2026-05-15 via probe;
    /// original Ghidra RVA `0x1400dde40` pointed mid-function.
    /// True entry at +1008 from stale (low confidence; may be a
    /// different function with the same prologue shape).
    pub const CHECK_HORSE_ELIGIBILITY: usize = 0x1400de230;

    /// `retire_horse_handler`. Re-derived via the format-string
    /// xref method (`tests/find_retire_horse_handler.rs`): scan
    /// `.rdata` for the unique "%s retired %s (useless)..." format
    /// literal, xref to the `lea rcx` inside the function, walk
    /// backward through .text past int3-padding to the prologue.
    /// True entry +1013 bytes from old Ghidra RVA 0x1400df280.
    pub const RETIRE_HORSE_HANDLER: usize = 0x1400df675;

    /// `compute_horse_price`. Re-derived via probe; original
    /// Ghidra RVA `0x1400dcab0` pointed mid-function. True entry
    /// at -811 from stale.
    pub const COMPUTE_HORSE_PRICE: usize = 0x1400dc785;

    /// `crispr_lab_state_machine`. Re-derived via probe; original
    /// Ghidra RVA `0x140089510` pointed mid-function. True entry
    /// at -84 (high confidence; small drift).
    pub const CRISPR_LAB: usize = 0x1400894bc;

    /// `breeding_state_machine`. Re-derived via probe; original
    /// Ghidra RVA `0x1400e0aa0` pointed mid-function. True entry
    /// at -649 from stale.
    pub const BREEDING: usize = 0x1400e0817;

    /// `save_game_writer`. Top-level save driver.
    /// True entry RVA derived 2026-05-15 via pattern-scan probe
    /// (`tests/r2_save_find_entries.rs`); the original Ghidra RVA
    /// `0x14006dc80` was -1548 bytes inside the function body.
    /// Prologue: `57 41 56 48 81 ec 40 01 00 00 48 8b e9 48 8d 51`
    /// (push rdi; push r14; sub rsp,0x140; mov rbp,rcx; lea rdx,...).
    /// Verified by `tests/r2_save_signatures.rs`.
    pub const SAVE_WRITER: usize = 0x14006d674;

    /// `load_game`. Top-level save loader.
    /// Same correction as `SAVE_WRITER`: original Ghidra RVA
    /// `0x14006e480` was -304 bytes inside the body. True entry
    /// prologue: `48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57
    /// 48 8b ec 48 83 ec 70` (shadow-save rbx + push 7 regs +
    /// mov rbp,rsp + sub rsp,0x70). Verified by
    /// `tests/r2_save_signatures.rs`.
    pub const LOAD_GAME: usize = 0x14006e350;

    /// `draw_pause_status`. Pause-menu renderer + debug-mode unlock.
    /// Re-derived 2026-05-15; original Ghidra RVA `0x140066200`
    /// pointed mid-function. True entry at -308 from stale.
    pub const DRAW_PAUSE_STATUS: usize = 0x1400660cc;

    /// `tmx_map_parser`. The .tmx loader.
    pub const TMX_MAP_PARSER: usize = 0x1400fe2e0;

    /// `pop_genome_builder`. Pop.xml runtime spawner. Re-derived
    /// 2026-05-15; original Ghidra RVA pointed mid-function. True
    /// entry at -57 (high confidence; minor drift).
    pub const POP_GENOME_BUILDER: usize = 0x1400927e7;

    /// `daily_horse_event_emitter`. Per-day per-horse event log.
    pub const DAILY_HORSE_EVENT: usize = 0x14002fe00;

    /// `track_state_machine`. Race lifecycle.
    pub const TRACK_STATE_MACHINE: usize = 0x14002d7c0;

    /// `circus_event_handler`.
    pub const CIRCUS_HANDLER: usize = 0x140039190;

    /// `sumo_match_handler`. Re-derived 2026-05-15; original Ghidra
    /// RVA pointed mid-function. True entry at -84 (high confidence).
    pub const SUMO_HANDLER: usize = 0x14007b28c;

    /// `power_plant_handler`. Re-derived 2026-05-15; True entry at
    /// -320 from stale.
    pub const POWER_PLANT: usize = 0x140069270;

    /// `world_action_dispatcher`. Re-derived 2026-05-15; True entry
    /// at -190 from stale.
    pub const WORLD_ACTION: usize = 0x1401075a2;

    /// `hot_air_balloon_controller`. Re-derived 2026-05-15; True
    /// entry at +1024 from stale (low confidence; window-edge).
    pub const BALLOON_CONTROLLER: usize = 0x14010a9e0;

    // -------------------------------------------------------------
    // D3/D4 lifecycle + breeding + per-horse save targets
    // (research closed 2026-05-15; see todo.md D3.0/D3.4/D4.1b/D4.2b)
    // -------------------------------------------------------------

    /// Horse struct constructor. Caller passes a freshly allocated
    /// 0x498-byte buffer (`FUN_1402c704c(0x498)`); the constructor
    /// installs the vtable `&PTR_FUN_14030d660`, zeros 50+ fields,
    /// sets default `max_age = 0x1e` at +0x44, increments the global
    /// live-count `_DAT_1403f311c`, and returns the same pointer.
    /// Single entry point for ALL horse-creation paths (40+ call
    /// sites). D3.1 lifecycle anchor for `EXT_HORSE_GENOMES`.
    /// Ghidra indexed as `FUN_1400aac60`; true entry -16 bytes.
    pub const HORSE_CONSTRUCTOR: usize = 0x1400aac50;

    /// Horse struct destructor. Resets vtable, decrements
    /// `_DAT_1403f311c`, releases sub-objects (`+0x3e` sub-struct,
    /// std::string SSO buffers at +0x51/+0x31/+0x2d), then frees
    /// the 0x498 buffer if `param_2 & 1` (C++ delete flag).
    /// Mirror partner of `HORSE_CONSTRUCTOR`. D3.2 lifecycle anchor.
    /// Ghidra indexed as `FUN_1400bf1f0`; true entry -16 bytes.
    pub const HORSE_DESTRUCTOR: usize = 0x1400bf1e0;

    /// Breeding combinator. Mendelian recombination of two parents'
    /// 0x1e0-byte inline genomes (at `horse + 0x2b8`) into a child's
    /// inline genome. Signature:
    /// `(parent_a + 0x2b8, parent_b + 0x2b8, child + 0x2b8)`.
    /// Body: outer loop 2 strands x inner loop 240 genes, per
    /// (strand, gene) calls `FUN_1400c6580()` for an RNG coinflip
    /// selecting one parent's strand to copy from. Plus linked-
    /// inheritance bitmask forcing same-parent across 3 ranges:
    /// 72..86 (Neck), 97..174 (Head/etc.), 183..197 (palette base).
    /// D3.4 detour target. Post-hook recovers horse base ptrs as
    /// `param_N - 0x2b8` and runs own Mendelian on `EXT_HORSE_GENOMES`.
    /// Ghidra indexed as `FUN_1400a2d80`; true entry -16 bytes.
    pub const GENE_COMBINATOR: usize = 0x1400a2d70;

    /// Per-horse save serializer. Called per pointer from the roster
    /// iterator (`FUN_14006d610`). Writes the inline genome at
    /// `horse + 0x2b8` via `FUN_14006d470`, then ~12 other small
    /// fields (name_id, age, flags, etc.). D4.1b sidecar-write
    /// anchor: post-hook appends our ext alleles for the same horse
    /// in the same iteration order.
    /// True entry RVA derived 2026-05-15 via probe; the original
    /// Ghidra RVA `0x14006ee10` was -277 bytes inside the body.
    /// Prologue: `57 41 56 48 83 ec 40 48 8b e9 e8 .. .. .. .. 48`
    /// (push rdi; push r14; sub rsp,0x40; mov rbp,rcx; call rel32).
    /// Verified by `tests/r2_save_signatures.rs`.
    pub const HORSE_SAVE_WRITER: usize = 0x14006ecfb;

    /// Per-horse save loader. Mirror of `HORSE_SAVE_WRITER`. Reads
    /// the inline genome via `FUN_14006d580(horse + 0x2b8)`, then
    /// the small fields, then calls `FUN_1400b3070(horse)` at line
    /// :65182 to regenerate render fields by re-running the engine
    /// + consumer. Our D1/D5 detours fire during that regen, so any
    /// ext alleles in `EXT_HORSE_GENOMES` populated BEFORE this
    /// returns get applied automatically. D4.2b sidecar-read anchor.
    /// True entry RVA derived 2026-05-15 via probe; the original
    /// Ghidra RVA `0x14006f150` was -287 bytes inside the body.
    /// Prologue: `53 57 48 83 ec 58 48 8b f9 48 81 c1 b8 02 00 00`
    /// (push rbx; push rdi; sub rsp,0x58; mov rdi,rcx; add rcx,0x2b8).
    /// The `add rcx, 0x2b8` is the genome-offset reference and the
    /// most distinctive signature byte sequence. Verified by
    /// `tests/r2_save_signatures.rs`.
    pub const HORSE_SAVE_LOADER: usize = 0x14006f031;
}

/// Resolve the running process's image base. Safe to call from any
/// thread after process init.
/// R3: pattern-scan resolution of data globals whose hardcoded RVAs
/// can drift between Horsey.exe builds.
///
/// Same primitive R1/R2 use for function entries: hand a few
/// candidate signatures to `modforge::patterns::sleuth`, take the
/// first that matches, then decode the RIP-relative displacement
/// inside the matched instruction to recover the data address.

/// Walk back/forward through `.text` from `addr_inside` until we
/// hit MSVC inter-function padding (2+ consecutive `0xcc`).
/// Returns `(function_start, function_end_exclusive)`. The end is
/// the first `0xcc` of the trailing padding run; function body is
/// `[start, end)`.
///
/// SAFETY: caller is responsible for `addr_inside` being inside
/// `.text` and the windows fitting within the section.
pub fn find_function_bounds_via_int3(
    addr_inside: usize,
    lookback: usize,
    lookahead: usize,
) -> Option<(usize, usize)> {
    let back_start = addr_inside.saturating_sub(lookback);
    let back_len = addr_inside - back_start;
    let back_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(back_start as *const u8, back_len)
    };
    let mut start: Option<usize> = None;
    for p in (1..back_bytes.len()).rev() {
        if back_bytes[p] == 0xcc && back_bytes[p - 1] == 0xcc {
            start = Some(back_start + p + 1);
            break;
        }
    }
    let start = start?;

    let fwd_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(addr_inside as *const u8, lookahead)
    };
    let mut end: Option<usize> = None;
    for p in 0..fwd_bytes.len().saturating_sub(1) {
        if fwd_bytes[p] == 0xcc && fwd_bytes[p + 1] == 0xcc {
            end = Some(addr_inside + p);
            break;
        }
    }
    let end = end?;

    Some((start, end))
}

pub fn image_base() -> usize {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    // SAFETY: GetModuleHandleW(NULL) returns the .exe HMODULE,
    // which on x64 IS the image base. Always safe.
    let h = unsafe { GetModuleHandleW(std::ptr::null()) };
    h as usize
}

/// Translate a hardcoded address (RVA-style, written as absolute
/// pointer in our decompilation) into a runtime address adjusted
/// for actual image base.
pub fn rebase(absolute_from_dump: usize) -> usize {
    let delta = image_base().wrapping_sub(PREFERRED_IMAGE_BASE);
    absolute_from_dump.wrapping_add(delta)
}

// =============================================================================
// Build identification (Phase R1 from docs/ADDRESS-RESOLUTION.md)
// =============================================================================

/// SHA256 of the loaded Horsey.exe image's .text section. Used to
/// pin "which build am I on" so we can correlate crashes to a
/// specific binary. Skip .reloc + .rsrc because those carry
/// per-load fixups / locale strings that drift between identical
/// builds.
///
/// Caches the result in a `OnceLock` so repeated calls are free.
pub fn image_text_sha256() -> String {
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let Some((text_addr, text_size)) = find_text_section() else {
                return "unavailable".to_string();
            };
            // SAFETY: text_addr / text_size point at the loaded image's
            // .text bytes; readable for the lifetime of the process.
            let bytes =
                unsafe { std::slice::from_raw_parts(text_addr as *const u8, text_size) };
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let digest = hasher.finalize();
            digest.iter().map(|b| format!("{b:02x}")).collect::<String>()
        })
        .clone()
}

/// Walk the PE header at `image_base()` to find the .text section's
/// runtime address and size. Returns None on any parse failure.
/// caller should treat that as "build identification not available
/// this session" and continue.
pub fn find_text_section() -> Option<(usize, usize)> {
    let base = image_base();
    if base == 0 {
        return None;
    }
    // SAFETY: PE images are mapped read-only from offset 0; the
    // DOS header is always present at the image base, and the PE
    // header follows after a small jump.
    unsafe {
        // DOS header `e_lfanew` is at offset 0x3c.
        let e_lfanew = *((base + 0x3c) as *const u32) as usize;
        let pe = base + e_lfanew;
        // PE signature must be "PE\0\0".
        let sig = *(pe as *const [u8; 4]);
        if sig != *b"PE\0\0" {
            return None;
        }
        // COFF File Header sits right after the signature (4 bytes).
        let num_sections = *((pe + 4 + 2) as *const u16) as usize;
        let opt_header_size = *((pe + 4 + 16) as *const u16) as usize;
        let opt_header = pe + 4 + 20;
        let section_table = opt_header + opt_header_size;
        // Walk section headers; each is 40 bytes.
        for i in 0..num_sections {
            let hdr = section_table + i * 40;
            let name_bytes = *(hdr as *const [u8; 8]);
            // Section name is null-padded ASCII, ".text\0\0\0".
            if &name_bytes[..5] == b".text" {
                let virtual_size = *((hdr + 8) as *const u32) as usize;
                let virtual_addr = *((hdr + 12) as *const u32) as usize;
                return Some((base + virtual_addr, virtual_size));
            }
        }
    }
    None
}

/// Public accessor for the .text section's runtime size in bytes.
/// Returns `None` on PE-parse failure.
pub fn find_text_section_size() -> Option<usize> {
    find_text_section().map(|(_addr, size)| size)
}

/// Mtime + size of the on-disk Horsey.exe, if findable.
pub fn image_file_info() -> Option<(std::path::PathBuf, std::time::SystemTime, u64)> {
    // GetModuleFileNameW for HMODULE = image_base() returns the
    // path. We resolve through std::env::current_exe() since the
    // injected DLL shares the same process and main exe.
    let path = std::env::current_exe().ok()?;
    let meta = std::fs::metadata(&path).ok()?;
    let mtime = meta.modified().ok()?;
    let size = meta.len();
    Some((path, mtime, size))
}
