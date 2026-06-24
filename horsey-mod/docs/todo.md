# horsey-mod TODO

> **Major prior art (2026-05-15):** [NickPetrone/HorseyLiveTweaks](https://github.com/NickPetrone/HorseyLiveTweaks) by Nick Petrone (Twisternick) is an independent Horsey Game modding framework with ~4500 lines of C++, in-game ImGui UI, and pattern-scan address resolution. Cross-validates our `horse+0x2b8` working-genome model and our `FUN_1400b3070` regen function. **Different scope** (they tweak the vanilla 240; we extend to 480). Full credit + comparison in [`PRIOR-ART-HorseyLiveTweaks.md`](PRIOR-ART-HorseyLiveTweaks.md). Their pattern-scan approach is the reference we should port for our R1/R2 address-resolution work.


> **Authoritative on:** open work and feature wishlist for the `horsey-mod` crate (native-PE binding for Horsey Game) AND for Horsey-Game content mods shipping alongside it. Workspace-wide framework work lives in [`../../docs/todo.md`](../../docs/todo.md).

Roughly ordered by leverage.

---

## Table of contents

- [P0 BLOCKER: pattern-resolve EVERY hardcoded address](#p0-blocker-pattern-resolve-every-hardcoded-address)
- [Ship status pointers](#ship-status-pointers)
- [P0. Bestiary Expansion: double the species count](#p0-bestiary-expansion-double-the-species-count)
- [P0. Gene Table Doubling: 240 -> 480 implementation plan](#p0-gene-table-doubling-240---480-implementation-plan)
- [P0. Synthetic input (delegated to modforge)](#p0-synthetic-input-delegated-to-modforge)
- [P0. Scene navigation: move the player between locations](#p0-scene-navigation-move-the-player-between-locations-added-2026-05-17)
- [Other open work](#other-open-work)
- [Appendix: Feature wishlist](#appendix-feature-wishlist-merged-from-roadmapmd-2026-05-15)

---

## P0 BLOCKER: pattern-resolve EVERY hardcoded address

User-locked 2026-05-15. Every address in `targets.rs` must be pattern-resolved. Authoritative migration tracker (data-globals + function-entries tables, R3 validation primitives, definition of done): [`ADDRESS-RESOLUTION.md`](ADDRESS-RESOLUTION.md#migration-hardcoded---resolved-authoritative-status).

**Current status (2026-05-15):**
- 6/6 data globals on **R**
- 31/31 function entries on **R** (RETIRE_HORSE_HANDLER re-derived 2026-05-15 via format-string xref method)
- Field offsets: **R4 done, 29/37.** All `must` + `should` items resolved. Migrated via patternsleuth + in-process pattern decode: `gs_offset::year/sleeps/money/horses_begin/horses_end/live_horses_begin/live_horses_end/sim_horses_begin/sim_horses_end/map_width/map_height/alloc_size/roster_stride`, `horse_offset::ctx_offset/tired_flag_a/tired_flag_b/on_track_flag/breeding_flag/age/max_age/alloc_size/litter_size_stat/name_id/skill/no_tire_loop_entry/no_tire_loop_size/type_or_species`. Two duplicate patch-site hardcodes in patches.rs (0x205/0x206) now read the resolvers. Remaining 9 fields are `H-gb-low` (diag-only, no unique anchor, drift harmless).

**Open work in this section:**
- [x] **R4 toolkit + first 10 resolvers shipped.** `modforge::research` library (in-process + harness variants), 4 generic recipes (decode_field_offset_via_string, decode_imm_in_window, decode_disp_pair_with_delta, decode_imm_at_call_site). 10 field offsets migrated (see Hardcoded-constants inventory below for which).
- [ ] **Apply R4 to remaining ~12 field offsets.** Recipes proven; per-field work is authoring the right anchor (format string, allocator call site, surrounding xref) and decoding the operand. **Plan-of-attack:** see "R4 remaining: research plan" below.
- [ ] **Adopt 5 HLT patterns** (see "Learn from HorseyLiveTweaks" below): structural-plausibility validation, vtable check, direct vanilla-fn calls, SEH wrapping of vanilla calls, injector elevation auto-detect.
- [ ] Author second candidate signatures for every resolver so a single MSVC reorder between builds doesn't break it (current Definition of Done #2; one sig each today).
- [ ] CI / pre-commit refuses to ship any new `pub const usize = 0x140...;` outside `targets::resolve::*` candidate sigs (Definition of Done #4).

### R4 remaining: research plan (2026-05-15)

12 H-gb field offsets are left after this session's batch (25/37 done), but only **4 are on the production hot path or public surface** (3 `must`, 1 `should`). The remaining 9 are diagnostic-only (`gamestate.diag` dump) and can stay `H-gb-low`. Core insight: they fail not because they're hard to pattern-anchor, but because we don't know WHICH function accesses them. The decomp tells us. The HTTP gives us live verification. Once anchor function is identified, R4 recipe is mechanical.

We have: decompiled source in `horsey-mod/research/decompiled/`, running game, full HTTP introspection (`mem.scan_rdata`, `patterns.sleuth.scan_all`, `mem.find_xrefs`, `patterns.read_bytes`, `gamestate.diag`).

Remaining (by criticality):

- **must** (3): roster stride 0x24, no_tire FN_RVA, no_tire FN_SIZE
- **should** (1): TYPE_OR_SPECIES 0x1c
- **low** (9, deferred): FRAME_TICK gs+0x254, SUPPLIES_START 0x31c, 8x FIELD_* 0x37c..0x440

#### Phase A: decomp grep pass (no game; pure offline)

For each remaining target, grep `research/decompiled/all_functions.c` (and per-function files) for the offset literal. Goal: map field -> list of accessor functions, then pick the smallest / most-anchored one.

| Target | Grep term | Expected output |
|---|---|---|
| `TYPE_OR_SPECIES` 0x1c | `+ 0x1c)` / `[0x1c]` | dispatchers that cmp horse type |
| `FIELD_37C..0x440` (8 fields) | `+ 0x37c)` etc. | producer + consumer per field |
| `SUPPLIES_START` 0x31c | `+ 0x31c)` / `+ 0x344)` | supply-array loop entries |
| `FRAME_TICK` gs+0x254 | `+ 0x254)` in GameState ctx | per-frame increment site |
| roster stride 0x24 | `* 0x24` / `imul ... 0x24` | roster iterators |

Deliverable: a markdown table appended below listing each target's anchor candidates with confidence ranking. ~30 min of grep work.

#### Phase B: resolve missing anchor functions (R2 work)

Some anchor functions from Phase A won't already be in `targets::resolve::*`. For each new anchor:

1. Check decomp for unique prologue / body signature.
2. Author R2 sig in `targets::resolve::*` following the existing pattern (function entry sig with optional alias-check).
3. Validate via existing `r2_*` tests.

Likely new anchors needed: `roster_iterator` (`FUN_14006d610`), `frame_tick_handler` (unknown function), `supply_loop` (unknown).

#### Phase C: R4 resolvers (mechanical with anchor in hand)

Once anchored, each resolver is one of the four existing recipes:

- **Disp-load histogram** in function body window (already used 8x). For TYPE_OR_SPECIES, FIELD_*, SUPPLIES_START, FRAME_TICK.
- **Adjacent-pair delta** for stride if iterator does `lea r, [r+r*8+r*4]` or similar.
- **Call-site lookback** (already used 2x for alloc sizes). For stride `imul reg, reg, 0x24`.
- **Format-string xref** (already used 4x). Unlikely useful for remaining (no format strings reference these fields).

#### Phase D: `no_tire` FN_RVA + FN_SIZE (bootstrap improvement)

Self-contained. Doesn't depend on Phase A. Recipe:

1. Add `modforge::research::find_function_bounds_via_int3(addr_inside, lookback, lookahead) -> Option<(start, end)>`. Walks back/forward through `.text` until 2+ consecutive `0xcc` bytes (MSVC inter-function padding).
2. In `horse_offset::resolve_tired_pair`, capture the address of the matched pair (currently discarded after disp decoding).
3. New `resolve::no_tire_loop()` calls `find_function_bounds_via_int3(pair_addr, 4096, 4096)` -> (FN_RVA, FN_SIZE).
4. Migrate `patches::sleep_safe_no_tire::FN_RVA` and `horse_offset::NO_TIRE_LOOP_FN_RVA` to read the resolver.

Standalone test: assert resolved start/size match hardcoded.

#### Phase E: semantic discovery for unknown `FIELD_*` (8 mystery fields)

The hardest. Some may be transient render state without a useful semantic. Approach:

1. **Static**: per Phase A grep, classify each as { read-only consumed by X, written-only by Y, read-and-written in loop Z }.
2. **Dynamic**: use `gamestate.diag` HTTP op to dump these fields. Snapshot before/after specific game actions (race, sleep, save, load, day-tick). Diff identifies which field changes with which action -> semantic.
3. **Categorize**:
   - Sub-struct anchors (start of another array): pattern-resolve as ranges.
   - Policy/config fields written once during load: anchor on `LOAD_GAME` body.
   - No clear semantic: mark `H-unknown` and accept drift risk (low if not on hot path).

Realistic: 1-2 hours per field of mixed decomp + live observation.

#### Order of attack (recommended)

1. **Phase D first**. Self-contained, ships the no_tire bootstrap improvement (2 targets) in one session. Validates the `find_function_bounds_via_int3` helper, which we'll need elsewhere.
2. **Phase A**. 30 min grep gives the anchor map for everything else.
3. **Phase C** for already-anchored targets. Likely 3-4 quick wins from Phase A.
4. **Phase B + C** for new anchors. Each is ~30 min from sig to R4.
5. **Phase E**. Last; some `FIELD_*` will resolve naturally during Phase A (if found all written in one constructor, pattern-anchor on that). Truly-mystery ones get marked `H-unknown`.

Practical final coverage: **29/37** (all must + should resolved). The 9 `H-gb-low` diag-only fields stay tracked-but-not-migrated; upgrade their classification if they ever become hot-path consumers.

### Hardcoded-constants inventory (zero-hardcoding audit, 2026-05-15)

Every magic integer baked into Rust source code. User-locked: nothing is out of scope. Audit, classify, then decide what to migrate.

Status codes:
- **R**: pattern-resolved at runtime via patternsleuth.
- **H-gb-must**: hardcoded, GAME-BINARY DERIVED, ON HOT PATH. Drift breaks the mod (patch fails, corrupted reads/writes). Migration is required.
- **H-gb-should**: hardcoded, GAME-BINARY DERIVED, public surface (HTTP read ops). Drift gives wrong diagnostic value; no patch breaks. Migration is recommended.
- **H-gb-low**: hardcoded, GAME-BINARY DERIVED, diagnostic-only (dumped by `gamestate.diag` and nothing else). Drift is harmless. Migration deferred.
- **H-gb**: hardcoded, GAME-BINARY DERIVED, criticality not yet classified. Treat as `must` until reviewed.
- **H-alg**: hardcoded ALGORITHM CONSTANT. We control the algorithm; the value is fixed by definition (CRC polynomial, hash mixer). Migration value = none, but still audit so we know it's intentional.
- **H-os**: hardcoded HOST-OS CONSTANT. Windows ABI/exception codes. Migration value = none unless we ship on a non-Windows platform.
- **H-design**: hardcoded DESIGN CHOICE we own (sidecar version, UI row cap, gene-count target). Migration value = none; these are policy.
- **H-test**: hardcoded TEST FIXTURE. Migration value = none.

#### Game-binary derived (H-gb): MUST migrate

| Constant | File:line | Kind | Status | Notes |
|---|---|---|---|---|
| `GAMESTATE_PTR` 0x1403fb0d8 | targets.rs:26 | data global | **R** | constructor-anchored resolver |
| `SAVE_VERSION_GLOBAL` 0x1403fb0e0 | targets.rs:31 | data global | **R** | derived from GAMESTATE_PTR+8 |
| `RACES_COUNTER` 0x1403eded8 | targets.rs:35 | data global | **R** | -10.5f anchor |
| `NO_TIRE_TOGGLE` 0x1403d95a5 | targets.rs:44 | data global | **R** | cmp-sete pair sig |
| `DEBUG_MODE_ACTIVE` 0x1403d957b | targets.rs:51 | data global | **R** | unlock-block delta |
| `DEBUG_LOG_GATE` 0x1403d9506 | targets.rs:59 | data global | **R** | init-triplet sig |
| `fn_addr::*` (31 entries) | targets.rs:166-371 | function entry | **R** | 32-48 byte body sigs |
| `gs_offset::FRAME_TICK` 0x254 | targets.rs:66 | struct field offset | **H-gb-low** | diag-only (`gamestate.rs:320`); drift = wrong diag value, no production impact |
| `gs_offset::sim_horses_begin/end()` 0x260/0x268 | targets.rs | struct field offset (2) | **R** | HLT labels `kOffSimHorsesBegin/End`; adjacent qword-load pair (delta 8) in `[0x250, 0x270]` window |
| `gs_offset::money()` 0x308 | targets.rs | struct field offset | **R** | cheat-money `add [base+disp], 1000` literal in DRAW_PAUSE_STATUS |
| `gs_offset::year()` 0x314 | targets.rs | struct field offset | **R** | "Year %d" pause-menu format string + `mov r8d, [base+disp]` |
| `gs_offset::sleeps()` 0x318 | targets.rs | struct field offset | **R** | debug pause-menu format string + `mov r9d, [base+disp]` |
| `gs_offset::SUPPLIES_START` 0x31c | targets.rs:76 | struct field offset | **H-gb-low** | diag-only (`gamestate.diag` dump); drift harmless |
| `gs_offset::FIELD_37C/39C/410/414/415/418/41C/440` | targets.rs:77-84 | struct field offset (8) | **H-gb-low** | diag-only (`gamestate.diag` dump, lines 181-188); semantic unknown; drift harmless |
| `gs_offset::horses_begin/end()` 0x280/0x288 | targets.rs | struct field offset (2) | **R** | adjacent qword-load pair (delta 8) in `[0x200, 0x300]` window |
| `gs_offset::map_width/map_height()` 0x278/0x27c | targets.rs | struct field offset (2) | **R** | HLT labels `kOffMapWidth/Height`; adjacent r32-load pair (delta 4) in `[0x270, 0x290]` window |
| `horse_offset::type_or_species()` 0x1c | targets.rs | struct field offset | **R** | `cmp dword [horse+disp8], imm8` dispatcher sites (`83 7? <disp8> <imm8>`) across `.text`; histogram across `imm in {1,2,4,6}` with modrm-filter (cmp /7, non-rsp/rbp base); top in `[0x10, 0x80]` |
| `horse_offset::name_id()` 0x1f8 | targets.rs | struct field offset | **R** | `(bails)` retire-handler format string + `8b 8? <disp32>` (mov ecx, [horse+disp32]) before `call name_resolve`; filter excludes age/max_age |
| `horse_offset::age()` 0x1fc | targets.rs | struct field offset | **R** | `(bails) age %d ch %d` format string + `44 8b` r9d-load (4th printf arg) |
| `horse_offset::max_age()` 0x200 | targets.rs | struct field offset | **R** | derived `age + 4` (adjacent int32 in release-to-wild cmp) |
| `horse_offset::on_track_flag()` 0x204 | targets.rs | struct field offset | **R** | derived `tired_flag_a - 1` from no_tire pair anchor |
| `horse_offset::breeding_flag()` 0x207 | targets.rs | struct field offset | **R** | derived `tired_flag_b + 1` from no_tire pair anchor |
| `horse_offset::tired_flag_a/b()` 0x205/0x206 | targets.rs | struct field offset (2) | **R** | adjacent byte-zero pair inside no_tire per-frame loop |
| `horse_offset::skill()` 0x21c | targets.rs | struct field offset | **R** | `(useless)` retire-handler format string + `8b` r32 loads in ±256B window; histogram top in `[0x210, 0x240]` |
| `horse_offset::litter_size_stat()` 0x254 | targets.rs | struct field offset | **R** | `8b` r32-from-disp32 loads within BREEDING body 8KB; histogram top in `[0x240, 0x270]` (both parents read it) |
| `gs_offset::live_horses_begin/end()` 0x130/0x138 | targets.rs | struct field offset (2) | **R** | adjacent qword-load pair (delta 8) in `[0x100, 0x200]` window |
| `gs_offset::roster_stride()` 0x24 | targets.rs / gamestate.rs | struct stride | **R** | call-site lookback at `e8 X<HORSE_SAVE_WRITER>`: `6b ?? <imm8>` (imul r32, imm8) inside iterator loop within 128 bytes; histogram top |
| `horse_offset::ctx_offset()` 0x2b8 | targets.rs | struct field offset | **R** | `add rcx, imm32` inside HORSE_SAVE_LOADER body (256-byte window) |
| `horse_offset::no_tire_loop_entry/size()` 0x1400ceb60 / 2502 | targets.rs / patches.rs | function entry + size | **R** | global scan for unique adjacent tired-pair byte-zero stores; `find_function_bounds_via_int3` walks back/forward through MSVC `0xcc` padding; bootstraps the no_tire patch + all 4 downstream tired-pair resolvers |
| Patch-site offset 0x206 (TIRE_FLAG_B store) | patches.rs | struct field offset | **R** | now reads `horse_offset::tired_flag_b()` resolver |
| Patch-site offset 0x205 (TIRE_FLAG_A store) | patches.rs | struct field offset | **R** | now reads `horse_offset::tired_flag_a()` resolver |
| `gs_offset::alloc_size()` 0x448 | targets.rs | struct size | **R** | anchor on unique `mov [rip+disp32], reg` store of GAMESTATE_PTR; lookback 256B for `b9 <imm32>`; histogram top |
| `horse_offset::alloc_size()` 0x498 | targets.rs | struct size | **R** | call-site lookback at `e8 X<HORSE_CONSTRUCTOR>`: `b9 <imm32>` (mov ecx, alloc size) within 32-byte preamble; histogram top |

**H-gb migration progress: 29 done / 9 remaining (all `H-gb-low`).**

Done: `gs_offset::year/sleeps/money/horses_begin/horses_end/live_horses_begin/live_horses_end/sim_horses_begin/sim_horses_end/map_width/map_height/alloc_size/roster_stride`, `horse_offset::ctx_offset/tired_flag_a/tired_flag_b/on_track_flag/breeding_flag/age/max_age/alloc_size/litter_size_stat/name_id/skill/no_tire_loop_entry/no_tire_loop_size/type_or_species`, patches.rs duplicate sites for 0x205/0x206 now read the resolvers.

**All `must` + `should` items done.** Remaining 9 are `H-gb-low`:

- FRAME_TICK (gs+0x254), SUPPLIES_START (0x31c), 8x FIELD_* (0x37c..0x440).

These are diag-only fields dumped by `gamestate.diag` and not used anywhere else. They have no unique pattern-anchor: each field's accessors are either (a) generic `mov reg, [ptr+disp32]` indistinguishable from any other field load, or (b) localized to one obscure handler we haven't resolved. Distinguishing them from sibling fields requires either:

1. A bespoke per-field signature based on a specific imm-write or distinctive context, OR
2. Resolving every accessor function to R2 first, then anchoring there.

Both have real cost. The fields drift harmlessly (wrong values in `gamestate.diag` dump, no patch breaks, no save corruption). Accepted drift risk: HIGH (these are exact-offset reads that WILL silently return wrong data on game updates), but BLAST RADIUS: LOW (consumers are diag-only).

**Decision: stop here at 29/37 = 100% of `must` + `should`.** Revisit if any `H-gb-low` field becomes a hot-path consumer.

#### Algorithm constants (H-alg): we own; intentional

| Constant | File:line | Kind | Notes |
|---|---|---|---|
| CRC32 polynomial 0xEDB88320 | save_sidecar.rs:210 | algorithm | reflected zlib poly; sidecar format we define |
| splitmix64 mixer 0x9E3779B97F4A7C15 | genes.rs:455,488 | algorithm | seed-mixing constant for child-id RNG |
| splitmix64 mixer 0xBF58476D1CE4E5B9 | genes.rs:456,489 | algorithm | same |
| splitmix64 mixer 0x94D049BB133111EB | genes.rs:457,490 | algorithm | same |
| Initial RNG seed 0xa5a5_a5a5_a5a5_a5a5 | genes.rs:481 | algorithm | static counter seed |
| Increment 0x100000001b3 | genes.rs:482 | algorithm | FNV prime; fetch_add for RNG state |

#### Host-OS constants (H-os): Windows ABI

| Constant | File:line | Notes |
|---|---|---|
| EXCEPTION_ACCESS_VIOLATION 0xc000_0005 | lib.rs:187 | NT status code |
| EXCEPTION_ILLEGAL_INSTRUCTION 0xc000_001d | lib.rs:188 | NT status code |
| EXCEPTION_STACK_OVERFLOW 0xc000_00fd | lib.rs:189 | NT status code |
| EXCEPTION_INT_DIVIDE_BY_ZERO 0xc000_0094 | lib.rs:190 | NT status code |
| EXCEPTION_PRIV_INSTRUCTION 0xc000_0096 | lib.rs:191 | NT status code |
| DLL_PROCESS_ATTACH/THREAD_ATTACH/THREAD_DETACH/PROCESS_DETACH | lib.rs:297-300 | DllMain reason codes |
| Heap-pointer bounds 0x10000 / 0x7fff_ffff_ffff / & 0x7 alignment | gamestate.rs:74 | x64 Windows user-mode address space facts |

#### Design choices (H-design): policy we control

| Constant | File:line | Notes |
|---|---|---|
| EXT_GENE_COUNT 240 | genes.rs:33 | our target gene-table extension size |
| TOTAL_GENE_COUNT 240+EXT | genes.rs:36 | derived from above + vanilla 240 |
| DEFAULT_MUTATION_RATE 100 | genes.rs:40 | default for ext gene mutation rates |
| DEFAULT_SCALE 1 | genes.rs:43 | default ext-gene scale |
| Sidecar format VERSION 1 | save_sidecar.rs:54 | BXSAVEXT version we author |
| BUF_LEN_FLOATS 353 | render_trampoline.rs:38 | render trampoline buffer size (matches engine layout but ours to set) |
| MAX_ROWS 32 | ui.rs:68 | UI cap on Horses tab row count |
| alias_check sentinels 0xAB / 0xCD | ops.rs:392,393 | distinct sentinel bytes for byte-aliasing probe |

#### Test fixtures (H-test): not production

| Constant | File:line | Notes |
|---|---|---|
| 0x1234, 0x5678, 0xDEAD | genes.rs:795,818,822,844 | unit-test horse ids |
| 0x01010101 / 0x02020202 fill | gamestate.rs:561,562 | regression fixture for fill-pattern roster |
| 0xff / 0xfe bad-bytes | save_sidecar.rs:360,361 + genes.rs:999,1000 | corruption-handling fixtures |

#### Summary

- **Migrate (H-gb):** 10 done, ~27 remaining -> R4 work below.
- **Not migrating (H-alg / H-os / H-design / H-test):** ~25 constants, all intentional. Audited and acknowledged.

### Learn from HorseyLiveTweaks (audit 2026-05-15)

After reading all of HLT's `pattern_scan.cpp`, `pattern_targets.cpp`, `offsets.h`, `scene_resolver.cpp`, `child_blend_hook.cpp`, and `injector_main.cpp`, the following patterns are strictly better than what we have and worth porting.

#### License posture (HLT is GPL v3; we are GPL v3)

Both projects ship under GPL v3 (HLT's `LICENSE`; our workspace `LICENSE`). No collision: GPL v3 explicitly permits derivative works under the same license. We can lift code, pattern strings, offset values, or whole modules from HLT directly.

In practice we still PREFER reimplementing in Rust:
- Cross-language ports are usually cleaner than line-for-line translations.
- Our architecture (patternsleuth + xref constraint + multi-target scan) is materially different from HLT's hand-rolled byte loop; direct ports often regress on our infrastructure.
- Engineering hygiene: understand the algorithm before copying.

But direct ports are allowed and don't need clean-room hedging. CREDIT them in commit messages + the prior-art doc either way.

#### Patterns to port

##### 1. Structural-plausibility validation of dereffed pointers

HLT's `is_probable_world_root()` (scene_resolver.cpp:36-52) doesn't just check the pointer is heap-shaped. It reads multiple fields off the candidate object and checks bounded ranges:
- `active_scene_id` in `[-1, kMaxSceneSlots)` (i.e. -1..255).
- `scene_table` pointer non-null AND its `[neighbor_scene_id]` slot readable.

Only if BOTH pass is the candidate accepted as the live World Root. Catches the case where the slot deref returns a stale/dead heap object whose first 8 bytes happen to look like a pointer but whose later fields are garbage.

Our current `gamestate::is_plausible_gamestate_pointer()` only checks heap-shape. Necessary but not sufficient.

**Action items:**
- [ ] Add `gamestate::looks_like_live_gamestate(ptr)` that reads bounded structural invariants (year in `[1, 10000]`, roster pointer pair via `roster_span_looks_loaded`, money <= 100M).
- [ ] Promote `gamestate::ptr()` to call it. If the dereffed pointer fails structural checks, fall through to a re-scan or return 0.
- [ ] Same treatment for any future singleton pointer (e.g. world-root if/when we use it).

##### 2. Vtable-pointer validation for typed objects

HLT's `is_neighbor_scene_ptr_usable()` (scene_resolver.cpp:72-81) reads the candidate object's first 8 bytes as a vtable pointer and compares against the EXPECTED vtable RVA. C++ objects always have their vtable as field 0; matching it confirms the object's TYPE.

We don't do this. GameState is a C++ class with a vtable (`FUN_1400fd580:54` stores `&PTR_FUN_140313558` at `*param_1`).

**Action items:**
- [ ] Find the GameState vtable RVA (likely `0x140313558` per decomp; verify).
- [ ] Pattern-resolve it via R3 (anchor on the constructor's `mov [param_1], <vtable>` instruction; decode disp32).
- [ ] Add vtable check to `looks_like_live_gamestate`: dereffed pointer's first 8 bytes must equal the GameState vtable's runtime address.

##### 3. Direct-call vanilla functions via fn-pointer cast

HLT calls vanilla functions DIRECTLY without setting up a detour, just by casting `exe_base() + kRva*` to the function pointer type (child_blend_hook.cpp:31). Useful when we want to INVOKE vanilla logic (e.g. their `shuffle_int_array`, `copy_gene_lane_pairs`) rather than intercept it.

We already do this in `patches/save_sidecar.rs` (calling the original via the trampoline). But for vanilla helpers we just want to call (not hook), an explicit `targets::fn_addr::FOO as TyA = transmute(targets::resolve::foo()?)` pattern is cleaner than going through MinHook.

**Action items:**
- [ ] Add a tiny `modforge::vanilla::call_fn::<F: Function>(addr) -> F` helper that wraps the transmute safely.
- [ ] Use it when calling vanilla helpers from inside our patches/detours.

##### 4. SEH-wrap any call into vanilla code [SHIPPED 2026-05-16]

HLT wraps vanilla-function calls in `__try` (child_blend_hook.cpp around `copy_gene_lanes` / `rebuild_horse`). A crash inside vanilla doesn't kill their DLL; the SEH filter logs + bails.

**Shipped as `modforge::seh::guard`**: pure-Rust API backed by an MSVC C shim (`modforge/src/seh.c` + `modforge/build.rs` with the `cc` build-dep). `guard<R>(f: impl FnOnce() -> R) -> Result<R, SehError>` runs the closure inside `__try`/`__except`, returns `Err(SehError { code, fault_address })` on any structured exception (access violation, illegal instruction, stack overflow). Four unit tests under `modforge::seh::tests` confirm the success path AND that reading `0xdeadbeef` produces `Err` instead of process death.

**Also shipped `modforge::hook::Hook<F: Function>`** as the generic detour wrapper. Wraps `retour::GenericDetour` with: name + target_addr stored, drop disables the detour, a `REGISTRY` for diagnostics, and the macro `modforge::call_original!(hook, args...)` which calls the trampoline inside `seh::guard` automatically. **HK1 transfer already routes through `seh::guard`**. Confirmed in production: three vtable AVs caught, game survived, fault rip logged, led directly to fixing the call signature.

**Action items:**
- [x] `modforge::seh::guard` shipped (commit `a9573c2c`).
- [x] `modforge::hook::Hook` + `call_original!` macro shipped (commit `a9573c2c`).
- [x] `patches/combinator.rs` migrated as the first consumer (commit `09a24e1c`, net -13 lines, +`vanilla_crashes` counter).
- [ ] Migrate `patches/lifecycle.rs` to `Hook`.
- [ ] Migrate `patches/render_trampoline.rs` to `Hook`.
- [ ] Migrate `patches/save_sidecar.rs` to `Hook`.
- [ ] Migrate `patches/ext_genes.rs` to `Hook`.
- [ ] HTTP op `hooks.list` that dumps `modforge::hook::registry()` for diag.

##### 5. Injector elevation auto-detection + re-launch

HLT's injector (injector_main.cpp top) detects whether the target process needs elevation; if yes, the injector re-launches itself elevated via ShellExecute.

`horsey-inject` currently doesn't do this. If a user runs `horsey-inject` from a non-elevated shell and Horsey.exe is running elevated, the injection fails silently (or with a permission error).

**Action items:**
- [ ] Audit `horsey-inject/src/main.rs` (or wherever the injector lives). If it doesn't already re-launch on `OpenProcess(PROCESS_VM_WRITE)` failure, add the elevation check.

#### What HLT does WORSE than us (do not regress)

- Hand-rolled 94-line byte-loop scanner vs our SIMD patternsleuth (orders of magnitude faster on full-image scans).
- All 46 struct field offsets HARDCODED. No R4 migration. Game updates silently corrupt their writes.
- No xref-constraint scanning (`X<addr>`); literal-byte scan only.
- No `.rdata` scan; can't anchor on string literals.
- One-shot resolution only. No fallback chain, no sanity gate, no cross-validation between multiple sigs per target.

#### Cross-validation already done

`PRIOR-ART-HorseyLiveTweaks.md` confirms every offset we share with HLT matches their value (kWorldRootPtrRva = 0x3FB0D8 = our GAMESTATE_PTR, kOffSceneHorseVecBegin = 0x130 = our LIVE_HORSES_BEGIN, etc.). Good independent confirmation we're reading the same binary correctly.

## Ship status pointers

- **480-gene D0-D5 + D7 implementation status:** [`HOOKING-STRATEGY.md`](HOOKING-STRATEGY.md) §8 (summary) + §9 (full implementation log).
- **D0 sidecar buffers + locked design decisions + architecture diagram:** [`GENE-CATALOG.md`](GENE-CATALOG.md) Part 3.
- **D4 save sidecar + horse-id finding + BXSAVEXT format:** [`SAVE-FORMAT.md`](SAVE-FORMAT.md) "Save pipeline functions", "Horse-id field", "Sidecar format".
- **Test harness + test-first rule + live test inventory:** [`TESTING.md`](TESTING.md).
- **Cross-validation with HorseyLiveTweaks:** [`PRIOR-ART-HorseyLiveTweaks.md`](PRIOR-ART-HorseyLiveTweaks.md). Every offset matches.

### Scope and non-conflict with HorseyLiveTweaks

User-locked 2026-05-15: **horsey-mod is a CONTENT + QoL mod. We do NOT overlap with HorseyLiveTweaks.**

**In scope (ours):**
- **Content:** new genes beyond vanilla's 240 (the 480-gene extension), new species (`genes-extended.xml` bestiary), new visual modes via D5 render trampoline, sidecar persistence of all new state.
- **QoL:** smart hotkeys that reduce repetitive clicking (HK1 Shift+Click smart-transfer is the v1 example), bulk operations, anything that makes 50+ horse households tolerable. QoL means "make the existing game less tedious," NOT "tweak existing gameplay numbers."

**Out of scope (theirs):**
- Anything HorseyLiveTweaks already ships. Terrain replacement, fence-tool overrides, wild-horse cleanup, neighbor-sim tuning, money/race-bet editing, scene helpers, VAT CRISPR editor for the vanilla 240. Use their mod for those.
- Anything that "tweaks vanilla state" without adding content or removing tedium. If it's "edit a vanilla knob," it belongs in HorseyLiveTweaks.

**Co-existence rules:**
- Test that horsey-mod + HorseyLiveTweaks installed simultaneously work. Both should arm cleanly.
- Where both mods could hook the same function (combinator `FUN_1400a2d80`, regen `FUN_1400b3070`), we use post-hook trampolines that ONLY touch the ext range; vanilla data stays HorseyLiveTweaks' domain.
- Prefer entry-detour patches (broad, all callers, ext-only data). AVOID call-site rel32 patching (their preferred mechanism) so we don't clobber a site they hook.

**Default answer to overlap requests:** "Use HorseyLiveTweaks for that. We add X on top." Bundle, don't duplicate.

---

## P0. Bestiary Expansion: double the species count

**Goal:** double spawnable pops from 28 -> 56 with creatures
that are both visually AND genetically distinct. Adding
new gene types is in scope from day one; the vanilla 242
genes are not enough to express the species we want.

**Modding posture (user direction 2026-05-14):** the base
game is a substrate, like a C# class or Rust trait. We
override anything we need to override. Pop.xml, genes.xml,
horsey.tmx, gene-table allocation, gene-by-index consumers,
save format. Binary patching is a normal tool, not a last
resort.

**Plan posture:** research first, decisions second, build
third. We do NOT lock the species list, the new-gene list,
or the build order until viability research clears the
core technical questions.

Vanilla baseline (verified in
[`horsey-mod/docs/CONTENT-CREATION.md:515`](CONTENT-CREATION.md#12-reference-vanilla-populations)):
28 pops, of which roughly half are real horse-shaped
creatures and the rest are oddities (cars, plants, humans,
microbes, helix, freaks).

---

### Phase 1: Decisions (post-research)

Phase 1 starts ONLY after Phase 0 is complete. Inputs:
the answered viability questions. Outputs: a locked plan
that drives Phase 2 build work.

- [ ] Lock the species list (count + names + flavor mix).
      The 9 normal / 10 weird / 9 other split below is a
      candidate, not a commitment.
- [ ] Lock the new-gene list. For each new gene: name,
      what it controls, which patch sites enable it.
- [ ] Lock the build order. Likely "patches first, then
      content," but the answers from Q-render-3 and
      Q-save-2 might invert that.
- [ ] Lock the save-compat policy.
- [ ] Lock the iteration loop (manual restart vs.
      Gene Editor live reload).

---

### Phase 2: Build (post-decisions)

Concrete sub-phases TBD by Phase 1 decisions. The bullets
below are candidates carried over from earlier drafts;
none of them are committed yet.

#### Phase 2A (candidate): smoke-test the content pipeline

Pick the simplest possible new pop (e.g. `spaceparrot` from
the existing example). Goal: confirm end-to-end that an
edit to `pop.xml` + a spawner in `horsey.tmx` produces a
visible new creature in-game without crashing the loader.

- [ ] Back up vanilla `pop.xml`, `horsey.tmx`, and
      `genes.dat` (the cache).
- [ ] Add one `<pop name="smoketest">` block under
      `<pop name="default">` with extreme gene weights so
      it's visually unmistakable.
- [ ] Place one spawner in `horsey.tmx` near the start area
      with `class="smoketest"`, `count=3`, `radius=4`.
- [ ] Delete `genes.dat` to force regen.
- [ ] Launch, confirm spawn, screenshot.
- [ ] If it fails: read `FUN_1400a3eb0` neighbors for the
      pop loader and figure out what's hardcoded.

#### Phase 2B (candidate): species list and gene tweaks

User direction 2026-05-14: balanced split across vanilla's
three flavor buckets (normal / weird / other), every new
pop must be **both visually AND genetically distinct**
(max effort), and **new gene types are in scope**. If
a species needs a gene vanilla doesn't have, we add it.

Candidate target: 9 normal + 10 weird + 9 other = 28. The
exact list is locked in Phase 1 after viability research.

#### Normal (9): real horse breeds + believable animals

Each rides existing genes (`SIZE`, `BONES`, `LEG_LENGTH`,
color channels, tail shape) into a recognizable real-world
silhouette. These should not look weird.

- [ ] `bx_arabian`. Small head, dished face, high tail
- [ ] `bx_draft`. Huge `SIZE`, thick `BONES`, short legs
- [ ] `bx_mini`. Tiny `SIZE`, short everything
- [ ] `bx_wolf`. Canine proportions, long body
- [ ] `bx_sheep`. Compact, fluffy palette
- [ ] `bx_goat`. Short, horned, agile build
- [ ] `bx_kangaroo`. Big hind legs, tiny front, long tail
- [ ] `bx_ostrich`. Biped, long legs, no front
- [ ] `bx_elephant`. Max `SIZE`, max `BONES`, gray

#### Weird (10): "X is now a horse"

Same energy as vanilla `pepper` / `yeast` / `helix` /
`appletree`. Push gene weights to extremes; abuse
`OSTODERM`, `SPLAY`, `GUT`, `SKINNY`, color channels.

- [ ] `bx_mushroom`. Short body, huge cap-shaped head
- [ ] `bx_kelp`. Tall, skinny, swaying, green
- [ ] `bx_slime`. Max `SKINNY` off, high `GUT`, jiggly
- [ ] `bx_worm`. No legs (QUADRUPED off), elongated body
- [ ] `bx_skeleton`. Min `GUT`, max `BONES`, pale palette
- [ ] `bx_ghost`. Semi-transparent palette, no legs
- [ ] `bx_virus`. Tiny, spiky `OSTODERM`, vivid color
- [ ] `bx_mold`. Low body, fuzzy palette, blotchy
- [ ] `bx_crystal`. Angular `OSTODERM` extremes, shiny
- [ ] `bx_balloon`. Huge `GUT`, tiny legs, bright color

#### Other (9): mythical / vehicular / humanoid

Sits between "normal" and "weird," matches vanilla
`centaur test` / `car` / `human` / `jockey` / `leprechaun`
/ `freak`.

- [ ] `bx_unicorn`. Horse base + single-spike `OSTODERM`
- [ ] `bx_pegasus`. Horse base + wing-shaped extras
- [ ] `bx_kelpie`. Dark horse + aquatic palette
- [ ] `bx_motorcycle`. Vehicle pop variant of `car`
- [ ] `bx_tractor`. Bigger / boxier vehicle pop
- [ ] `bx_shopping_cart`. Small wheeled-thing pop
- [ ] `bx_robot`. Humanoid but rigid, metallic palette
- [ ] `bx_clown`. Humanoid variant under `human`
- [ ] `bx_pirate`. Humanoid variant under `human`

#### Per-pop deliverable

Before authoring XML, each entry above must have:

- 1-line description of the silhouette goal
- The 3-7 gene tweaks (`gene name`, `p0..p3`, `m`, `s`)
  that achieve it
- Parent pop choice (under `default`, `human`, `car`, etc.)

Park the full table in this file before moving to Phase
1C. The table replaces the bullet stubs above.

#### Phase 2C (candidate): patches for new gene types and table extension

- [ ] Implement the chosen table-extension strategy from
      Q-gene-4 in `horsey-mod/src/patches.rs`. The patch
      infra (revert-on-detach) already works.
- [ ] For each gene-by-INDEX consumer flagged in Q-gene-3,
      patch it to go through a name lookup OR re-bake its
      index for the new layout.
- [ ] If save format needs patching per Q-save-2, do it.
- [ ] For each new visual mode flagged in Q-render-1 that
      has no vanilla equivalent, patch the renderer
      handler to recognize the new gene.
- [ ] Add `genes.add {name, m, s, p0..p3, ...}` HTTP op
      to stage new gene definitions live for testing.

#### Phase 2D (candidate): pop.xml + genes.xml authoring

- [ ] Author all new gene definitions in `genes.xml` (or
      a sidecar `genes-extended.xml` if Q-render-3 makes
      that cleaner).
- [ ] Author one pop block per species in `pop.xml`,
      nested under the right parent (`default`, `human`,
      `car`, etc.).
- [ ] Convention: prefix our pops with `bx_` and our new
      gene names with `BX_` so they sort together and
      don't collide with future vanilla additions.

#### Phase 2E (candidate): spawner placement

- [ ] Place spawners in `horsey.tmx` for each new species.
      Decide per species: dense in one biome, or scattered.
- [ ] Avoid stomping on the demo / tutorial spawn area
      for the first build to keep verification clean.
- [ ] Reuse vanilla animal tiles (7 available) plus the
      two unused (bunny, dinosaur) where they fit
      visually.

#### Phase 2F (candidate): Gene Editor HTTP ops

Build live-tweak ops in `horsey-mod` so species design is
interactive. May happen earlier (before Phase 2D
authoring) if Q-reload-2 says it's cheap and high-value.

- [ ] `pops.list` -> all currently loaded pops.
- [ ] `pop.read {name}` -> all gene weights for that pop.
- [ ] `pop.gene.set {pop, gene, p0..p3, m, s}` -> mutate
      live (if Q-reload-1 confirms the chromomap is
      mutable post-load).
- [ ] `pop.xml.reload` -> re-parse `pop.xml` from disk
      without restarting the game.
- [ ] `genes.xml.reload` -> same for genes.xml.
- [ ] `spawner.preview {pop, x, y, count}` -> spawn N
      creatures of that pop at a coord.

#### Phase 2G (candidate): ship as a data + patch overlay

- [ ] Ship as a folder the user drops into `Horsey/data/`
      plus the existing `horsey-mod` DLL for the binary
      patches. One-line uninstall (restore backups +
      delete `genes.dat` + don't inject the DLL).
- [ ] Optional: PowerShell installer / uninstaller.
- [ ] Document save-compat policy from Q-save-3.

#### Phase 2H (candidate): in-game verification

- [ ] Walk the map, screenshot every new species.
- [ ] Confirm none of them break breeding, racing, or
      save load.
- [ ] If a species crashes the renderer, narrow which
      gene pushed it over the edge and document the safe
      range.

---

## P0. Gene Table Doubling: 240 -> 480 implementation plan

D0 architecture / locked-decisions and D1+D3+D5 detours shipped 2026-05-14 / 2026-05-15. See [`CHANGELOG.md`](CHANGELOG.md#d-phase-shipped-items-2026-05-14--2026-05-15). D0.5 (heap-redirect helper) deferred but not blocking.

### Phase D2 (per-pop weight extension): OPEN

Pop records currently hold 240 genes x 4 alleles = 3840 bytes of weights at offset `+0x28`. Extending to N genes means each pop needs more weight space. Strategy: leave vanilla pop record layout alone; store extended weights in `EXT_POP_WEIGHTS[pop_id]`.

- [ ] **D2.1.** Detour `FUN_1400a5ee0` (pop processor). When it writes to a pop record, also create an `EXT_POP_WEIGHTS[pop_id]` entry inheriting from the parent pop's extended weights (inherit-then-override mirrored).
- [ ] **D2.2.** When parsing `pop-extended.xml`, look up the named pop's `pop_id` and apply per-gene weight overrides into `EXT_POP_WEIGHTS[pop_id]`.
- [ ] **D2.3.** When the spawn function picks alleles for a new horse, also pick alleles for genes 240..479 using the pop's extended weights. Write the picks to `EXT_HORSE_GENOMES[horse_ptr]`.
- [ ] **D2.4.** Sync `FUN_1400c03a0` swaps across `EXT_POP_WEIGHTS` entries.

### Phase D2-spawn (default-pop count multiplier): OPEN

Goal: multiply vanilla spawner counts by a per-pop override declared in [`bestiary/pop-extended.xml`](../bestiary/pop-extended.xml). First customer is `<pop name="default" count_multiplier="2.0" />` which doubles wild-horse density at world load.

Schema lives in `pop-extended.xml`. Reading semantics: for each spawner the engine creates, if the spawner's `class` matches (or inherits from) a `<pop>` entry, multiply the spawner's vanilla `count` by `count_multiplier`. Round half-up at consumption.

- [ ] **D2.5. Stage the file.** Extend `horsey-mod/src/bin/inject.rs::stage_bestiary` to copy `bestiary/pop-extended.xml` next to the staged DLL alongside `genes-extended.xml`. Same idempotency rules (skip if user has a custom copy).
- [ ] **D2.6. Parser.** New module `horsey-mod/src/pop_xml.rs` mirroring `genes_xml.rs`. Parses `pop-extended.xml` at DLL attach into a static `POP_OVERRIDES: RwLock<HashMap<String, f32>>` keyed by pop name. Tolerate missing file (no overrides applied). Test: `tests/pop_xml_parses.rs` asserts the shipped file round-trips with `default -> 2.0`.
- [ ] **D2.7. Spawner-count detour.** Detour the TMX spawner-consume call site inside `FUN_140076a10` (tmx_map_loader) at the instruction that reads the `count` int property. Multiply by `POP_OVERRIDES.get(class)` if present. Wrap the multiplied write under `modforge::seh::guard`. Test: `tests/pop_spawn_doubled.rs` loads a save with a known wild-horse count, asserts the live overworld scene reports 2x after detour arms.

### Phase D4 (save sidecar): code SHIPPED, addresses STALE

Implementation lives in `horsey-mod/src/patches/save_sidecar.rs`. Pipeline, BXSAVEXT binary format, horse-id field investigation, and detour table all documented in [`SAVE-FORMAT.md`](SAVE-FORMAT.md) "Save pipeline functions" + "Horse-id field" + "Sidecar format `save<N>.dat.ext`".

The 4 detour target RVAs are mid-function in the current shipping build; `genes.ext.save.arm` is UNSAFE to arm until re-derived via pattern-scan. Tracked under R2 in [`ADDRESS-RESOLUTION.md`](ADDRESS-RESOLUTION.md). The failing harness test `dryrun_d3_d4::save_dryrun_prologues_ok` is the locking contract.

### Phase D6: Save-compat strategy for the population
mutation drift

`FUN_1400c0660` mutates gene-table mutation rates by
±5 on horse death. For extended genes, our detour at
D1.3 already directs those writes to `EXT_GENE_TABLE`.
But: vanilla's mutation drift on genes 0..239 only
gets persisted if we can find where it's saved.

- [ ] **D6.1.** Find the function that serializes the
      gene-table mutation rates back to disk. The
      `FUN_1400a4880` XML round-tripper is the obvious
      candidate. Confirm by tracing its call site (does
      it run during save, or only on a manual "export
      genes.xml" debug command?).
- [ ] **D6.2.** If vanilla DOES persist the drift to
      genes.xml on save, our extended drift goes to
      `genes-extended.xml` via mirror logic. If vanilla
      does NOT persist the drift, we have nothing to
      mirror.
- [ ] **D6.3.** Document the drift-persistence behavior
      in [`GENE-CATALOG.md` (Part 1: Conceptual model)](GENE-CATALOG.md) once
      confirmed.

### Phase D7: Iteration and testing infrastructure

- [ ] **D7.1.** Implement the `genes.xml.reload` and `pop.xml.reload` HTTP ops (Q-reload-1 still open; may not be feasible if vanilla doesn't expose a reentrant loader).
- [ ] **D7.4.** Per-horse full dump op. Needs a stable horse_id resolution per D4.4 finding (roster slot position) before it's meaningful.

### Phase D8: Verification

Most D8 items are now covered by harness + unit tests; see [`TESTING.md`](TESTING.md) for the test inventory. Specifically:

- **D8.1** (vanilla regression check): covered by `tests/smoke` + `tests/dryrun_d1_d5` + `tests/arm_full_safe_stack`.
- **D8.4** (breeding inheritance): covered by 6 unit tests in `genes::tests::combinator_*`.

Still open (need a save loaded + per-test save fixtures):

- [ ] **D8.2.** Add ONE extended gene (e.g. `BX_WING_SIZE`); spawn a horse of a pop that uses it; confirm slot value matches diploid blend math.
- [ ] **D8.3.** Save / reload round-trip for a horse with ext alleles. Blocked on D4 stale save addresses.
- [ ] **D8.5.** Stress test: add 100 ext genes, profile per-frame cost of the D5 trampoline + sidecar file size.
- [ ] **D8.6.** Hot-reload genes-extended.xml via HTTP mid-game, confirm visual change without restart.

### Phase D9: Documentation and rollout

- [ ] **D9.2.** Write a "How to author a new gene" guide aimed at non-engineers: `genes-extended.xml` format, `mode="..."` options, working examples.
- [ ] **D9.3.** Decide rollout: ship the patch infra with zero extended genes by default and let authors add them via XML, OR ship a starter pack of 20-50 useful new genes.

### Risks and unknowns (open only; resolved entries relocated to [`CHANGELOG.md`](CHANGELOG.md#d-phase-risks-resolved-2026-05-15))

| Risk | Mitigation |
|---|---|
| `FUN_1400a5d20`'s detour may be hot enough that the per-call branch costs measurable frame time. | Profile in D8.5. If it's a problem, JIT-patch the detour to skip vanilla frames where no extended genes are defined. |
| Vanilla's `genes.dat` cache format may include the gene table in a way that doesn't survive our patches. | Recommend deleting `genes.dat` on attach to force regen. Document. |
| `FUN_14009f680` runs on a non-render thread (audio? AI?) and our trampoline triggers thread-safety issues. | Audit the calling thread context for each of the 4 caller sites in D5.1. |

---

## P0. Synthetic input (delegated to modforge)

Lives in [`../../docs/todo.md`](../../docs/todo.md) under "P0. Synthetic input." Cross-game primitive, modforge is the home. Horsey is the first consumer. Per-game research steps I-R1..I-R5 (decomp grep, mouse/keyboard globals, click dispatcher, PostMessage smoke) stay tracked there too. HK1 Shift+Click is the immediate beneficiary.

---

## P0. Scene navigation: move the player between locations (added 2026-05-17)

> Goal: agent loop drives the truck around the world map and enters/exits locations the way a human plays (click on map button, click on a building, wait for truck to path and arrive, scene enters automatically).
>
> Builds on: world-map findings in [`HORSE-PLACES.md`](HORSE-PLACES.md), [`world-map-detection.md`](world-map-detection.md), the L3 input axis ([`modforge::input`](../../modforge/docs/input-prior-art.md)), and the "Where Am I" Debug panel shipped 2026-05-17.

**Confirmed live-test findings (2026-05-17):**

- scene_id 0 = PLAYER HOME (scene-table slot 0; allocated at game init, outside the factory dispatcher).
- scene_id 20 = Lasso-Mart (matches decomp factory dispatcher exactly; validates the rest of the 23-row table by analogy).
- **Game always starts with truck on the overworld**, even after save load. To enter Home the test harness clicks the house door at the pre-captured coord `home_door_from_truck_spawn = (983, 394)` (in `target/x86_64-pc-windows-msvc/release/menu_targets.json`), already used by `common::ensure_home_scene_loaded`.
- Each in-scene UI has a `Map` button at top-right. BUT we don't need to click it because the exit-scene function was found in decomp (see Phase 1 below).

**Plan (user-locked 2026-05-17): drive-and-arrive only.**

Direct teleport via `vanilla.invoke FUN_1401046c0` was considered and **rejected**. Reason: it bypasses the truck animation, auto-save, and the natural enter-state-machine. Exactly the code paths the agent loop needs to exercise. If teleport is ever wanted (e.g. CI shortcut), revisit then; do not build it now.

### Status (2026-05-17 in-progress)

| Phase | Status | Notes |
|---|---|---|
| **Phase 1**: `game.exit_to_overworld` | **PARTIALLY GREEN** | `EXIT_TO_OVERWORLD` resolved in decomp as `FUN_1401041f0` (RVA `0x1401041f0`); cite `:154755`. Sets `GS+0x25C = -1`, zeroes scene state, auto-saves, plays "World" sound, calls scene tear-down. Registered in `targets_registry.rs` (Signature `(*GameState) -> void`). `game.exit_to_overworld` HTTP op + Debug-tab button. Smoke test ran 2026-05-17. **synthetic click to enter Home WORKED** (scene_id `-1 -> 0` in 502ms after fixing the 0ms click hold; see Phase 1b). The `vanilla.invoke EXIT_TO_OVERWORLD` step **crashed the game** (HTTP worker disconnect). Root cause: vanilla function called from HTTP-worker thread instead of game's main thread; touches scene tear-down / render state concurrently held by the game thread. Same shape as `DEBUGGING.md` Cause A. Fix path: switch to synthetic-click on the in-scene `Map` button (top-right) instead of `vanilla.invoke`. The natural drive-like-a-human path. Map-button click runs the exit function from the game's own UI handler on the right thread. |
| **Phase 1b**: 60ms click hold fix (modforge::input) | **SHIPPED 2026-05-17** | `l1::click()` was emitting LEFTDOWN then LEFTUP back-to-back with 0ms hold. Horsey's input pump polls `GetAsyncKeyState` once per frame (~16ms at 60 fps) and missed the press entirely. Fixed: `l1::click()` now defaults to a 60ms hold (~3 frames); added `l1::click_held(btn, x, y, hold_ms)` for explicit control. **Confirmed working: click at (983, 394) now triggers scene transition reliably.** |
| **Phase 2**: tile-scale + camera readers | **CODE SHIPPED** | `TILE_SCALE` registered at RVA `0x140303fb4`; `scene::tile_scale()` + `scene::pos_to_tile()` helpers. Debug tab "Where Am I" panel now shows the live float + truck tile coords. |
| **Phase 3**: `game.drive_to_location` | TODO | Needs world->screen projection (TILE_SCALE + MapState camera + screen-offset calibration), then `input.mouse.click_at(screen_xy)` + poll for `active_scene_id != -1`. |
| **Phase 4**: per-scene Debug buttons | TODO | One button per known location (Sue's Glues, The Circus, etc.) calling `game.drive_to_location {scene_id}`. Currently only `[Exit to overworld]` button is in. |
| **Phase 5**: `tests/drive_to_location_smoke.rs` + `tests/round_trip_smoke.rs` | TODO | Run after Phase 3 ships. The drive-to-location test ALSO auto-fills unknown scene labels by visiting each scene and capturing the resolved `scene+0x18` display name. |

### The drive-and-arrive flow (every primitive exists; just sequence them)

1. [ ] **Exit current scene** -> synthesize a click on the `Map` button at top-right of the in-scene UI.
   - [ ] One-time calibration: capture `Map` button screen coords (relative to window) via `input.cursor.get` while hovering. Should be screen-position-stable across scenes since it's HUD-anchored.
   - [ ] Op `game.exit_to_overworld` -> `input.mouse.click_at(map_btn_xy)` -> poll `active_scene_id == -1`.
   - [ ] OR (alternative): find the in-decomp exit-scene function (grep for `*(GS+0x25C) = -1;` writes; sibling of `FUN_1401046c0` at a nearby RVA). Direct call is more reliable than synthetic click if found.
2. [ ] **Compute target building's screen coord on the world map**:
   - [ ] Parse `data/horsey.tmx` once at attach -> `Vec<{type, world_x, world_y}>`. (Asset-based, no in-memory walk needed; see [`world-map-detection.md`](world-map-detection.md) Finding 2.)
   - [ ] `world_to_screen(world_pos)` via MapState `+0x254/+0x258` camera + zoom. Needs `DAT_140303fb4` value pinned down (one `patterns.read_bytes` test in-save).
3. [ ] **Click on the building** via `input.mouse.click_at(screen_x, screen_y)` (L1 SendInput; falls back to L2 PostMessage if needed).
4. [ ] **Wait for truck to path + arrive**: poll `scene::truck_pos()` until tile == target tile, OR poll `active_scene_id != -1`. Reasonable timeout 15s.
5. [ ] Scene-handler enters automatically (game's `enter_location_handler` fires on truck arrival. No action needed).

HTTP ops to ship:

- [ ] `game.exit_to_overworld`. See step 1.
- [ ] `game.drive_to_location {scene_id?: i32, world_pos?: (f32,f32), tmx_type?: "circus"}`. Orchestrates steps 1-5. Returns `{route_seconds, final_active_scene_id, final_scene_display_name}`.
- [ ] `game.click_on_screen {x, y}`. Thin wrapper on `input.mouse.click_at` (already exists; this is just a convenience alias for tests).

### UI in the Debug tab (already has "Where Am I" panel)

- [ ] Add "Move Player" sub-section under "Where Am I".
- [ ] One button per known location: `[Drive to Sue's Glues]` `[Drive to The Circus]` `[Drive to CRISPR Lab]` `[Drive to Sumo Ring]` `[Drive to The Jockey Club]` .... Each calls `game.drive_to_location {scene_id: N}`.
- [ ] `[Exit to overworld]` button. Calls `game.exit_to_overworld`.

### Tests-first plan

- [ ] **`tests/exit_to_overworld_smoke.rs`**: from each known scene, hit `game.exit_to_overworld`, confirm `active_scene_id == -1`. First test to land because it depends on only one primitive (Map-button click).
- [ ] **`tests/drive_to_location_smoke.rs`**: from overworld, `game.drive_to_location {scene_id: N}` for each known TMX-typed building (circus, crispr, lasso-mart, etc.). Assert `active_scene_id` ends up as expected; capture the resolved `scene+0x18` display name for the live-test column of the scene table. Auto-fills the still-unknown scene labels (15, 19, 21, 26, 38, 47) by exhaustive sweep through unmapped scene_ids when there is no matching TMX object.
- [ ] **`tests/round_trip_smoke.rs`**: drive overworld -> scene N -> exit -> drive to scene M -> exit. Confirms the full agent loop survives chaining without state leaks.

### Prerequisites / blockers

- [ ] **`DAT_140303fb4` value** (tile-to-world-pixel scale). One `patterns.read_bytes` in-save. Trivial.
- [ ] **`Map` button screen coords** (one calibration). Trivial.
- [ ] **Exit-scene function in decomp** (optional; synthetic click on Map button is the chosen default). Worth a quick grep but not blocking.

### Estimated effort

End-to-end (full drive-and-arrive + per-scene UI buttons + tests): **~3 hours**.

### Why this is P0

Without it, every scene-internal feature (CRISPR mods, breeding control, race automation) requires the user to manually drive the game to the right scene before any test or development iteration. Shipping this turns the agent loop from "user babysits" into "Claude drives end-to-end."

---

## Other open work

### HK1 detector + name table + stuck-car (2026-06-24, OPEN). Detail in [HK1-SHIFT-CLICK-TRANSFER-PLAN.md](HK1-SHIFT-CLICK-TRANSFER-PLAN.md) 5g

Honest status. Nothing committed.

- **Trailer/pasture DETECTOR built** (`gamestate.owned_horses` classifies by the
  scene-placement position `horse+0x1d4/+0x1d8`; `+0x1d0` is a pickup counter,
  not the container). Rule "non-zero = trailer" is too loose: tomtato at
  `(14.5, 9)` reads trailer but is probably pasture (operator: only 1 in trailer).
  Need the real trailer RECTANGLE; the click-handler extent constants read as
  garbage in memory. Verification blocked: the op reads the static loaded-save
  snapshot (in-scene drags commit only on leave), the owned list is inconsistent,
  and Coupe is un-grabbable so can't be moved to test.
- **NAME TABLE re-derived** (`resolve_name_table_custom` anchored on
  `imul rax,rax,0x88 ; add rax,[rip+disp32]` in `FUN_1400c78c0`): slot RVA
  `0x3f45f0` (drifted +0x1110 from the stale `0x3f34e0`). Names decode correctly
  in `tests/hk1_name_table.rs` (251=Coupe DeVille, 272=Horse, 250=tomtato). OPEN
  BUG: the live op still reads `<none>`. The registry caches the NAME_TABLE
  resolve MISS at attach; a `OnceLock` bypass fixed names but broke `owned_horses`
  (returned 0) and was reverted. Fix the registry caching (resolve lazily / do
  not cache a NAME_TABLE miss) without breaking `owned_horses`.
- **COUPE DEVILLE un-grabbable**: grabbing is a box2d collision hit-test
  (`FUN_1400b6fd0` over the body list `horse+0x40..+0x48`); his collision body is
  missing/disabled (a failed physics rebuild `FUN_1400b3dc0`). NOT the car type,
  NOT a flag. Repair = force a physics rebuild (risky); not done. Not yet confirmed
  by reading his body list.
- **`owned_horses` list inconsistency**: tomtato (name_id 250) is intermittently
  absent from the read. The op walks `GS+0x438 -> +0x90 -> +0x130`, which may not
  be the canonical owned list (scene slot 0). Audit the chain.

### HK1 Shift+Click smart-transfer (REOPENED via L3 input 2026-05-16)

> **STATUS CHANGE 2026-05-16:** The synthetic-input axis shipped end-to-end ([modforge::input + HorseyInputSurface](../../modforge/docs/input-prior-art.md)). The 4-helper RE blocker below is **no longer the critical path**. The new recommended approach is to drive the GAME'S NATIVE click handler via synthetic input rather than bypass it: `input.combo` + `input.mouse.drag` with `backend: "l3"` writes `LOC+0x174`/`+0x178` directly so `FUN_1400d2ab0` runs end-to-end (vtable[+0x78] + the 4 helpers automatically). Authoritative plan: [`HK1-SHIFT-CLICK-TRANSFER-PLAN.md`](HK1-SHIFT-CLICK-TRANSFER-PLAN.md) is now superseded for the dispatch path; calibration data is still needed.

Status (legacy bypass-vtable path):

- **S0 research probes** shipped. Confirmed slot 0x00 is the Home Location (vtable_rva `0x30f3d0`, strings "My House"/"Home" inline). Slots 0x08..0x38 are race lanes sharing the same Location class. HLT mouse RVAs `0x3ED970/0x3ED978` are stale in current build, drift to `0xffffffff`.
- **Container kind field found** at `horse + 0x1d0` (u32). Observed values: 7=truck, 9=pasture, 0/2 post-load. Writing this byte directly DOES NOT move horses; field is a downstream cache, not a control.
- **Drop-commit function found**: `FUN_1400de2e0` (RVA `0xde2e0`), reached via Home Location vtable[+0x78]. NOT in our decomp dump (binary updated since decomp).
- **Function signature corrected**: Ghidra reported single-arg, disassembly shows three-arg `(this: *LOC, drag_idx: i32, param3: i32) -> u8`. Fault at entry+0x31 was `mov r15, [rax+r12*8]` reading horses[drag_idx] with r12 sign-extended from uninitialized rdx.
- **vtable[+0x78] now returns 1 (drop accepted!) without crashing** when called with proper signature and full LOC stage (LOC[0x29]=horse_ptr, LOC[0x2d]=drag_idx, LOC[0x37]=1, LOC[0x174/0x178]=target cursor coords).
- **Legacy blocker (de-prioritized):** vtable[+0x78] returns success but state doesn't change because the click handler's success branch (FUN_1400d2ab0:1786-1804) runs 4 helpers (FUN_1400b47e0, FUN_1400b3dc0, FUN_1400b6990, FUN_1400ccbd0) afterward. Bypassing the click handler means re-implementing those 4 calls. Now that L3 input is shipped, we just drive the click handler instead. Keep the legacy bypass as a backup path if synthetic input proves insufficient.

**Save-load + scene-entry automation (prereq for everything below):**

When Steam launches Horsey, the most recent save auto-loads, but it lands the player in the TRUCK on the world map, OUTSIDE the house. `active_scene_id = -1` in this state and the Home Location does NOT exist yet (no LOC pointer, `hk1.read_cursor` returns null, the horse vector at slot 0x00 is inaccessible). The user must CLICK THE HOUSE to enter the Home scene. After click, `active_scene_id` flips positive, LOC is constructed, and HK1 work becomes possible.

- **House screen position is STABLE at fresh save-load.** Truck spawn is deterministic; capturing the house door's screen coords once (via `input.cursor.get` + manual hover) and replaying them via `input.mouse.click` backend=l1 will reliably enter the Home scene. **But once the truck moves on the world map, the house's screen position changes**, so the hardcoded-coords approach only works from a fresh launch. Driving the truck back to a known position, or computing the house's screen coords dynamically (world->screen project via view matrix), is a problem for later.
- **No fast-travel shortcut.** Decomp string `EnterLocationHome` is a candidate `vanilla.invoke` target but rejected for v1 automation: clicking the house and moving around IS the user-experienced pain that HK1 ultimately reduces, so the automation has to drive it the same way.

Tasks:

- [ ] **A1. Capture house door coords.** One-shot interactive: launch game fresh, hover the house door, run `input.cursor.get`, write `{x, y}` to `<dll_dir>/menu_targets.json` keyed as `home_door_from_truck_spawn`.
- [ ] **A2. `common::ensure_home_scene_loaded(&game, timeout)`** helper in `horsey-mod/tests/common/mod.rs`. Polls `active_scene_id` (new op `gamestate.active_scene_id` if not present, else `hk1.read_cursor` non-null). If scene != Home, replays the captured house-door click via `input.mouse.click` backend=l1; polls until in-scene or timeout. Pre-gate for every in-save test going forward.
- [ ] **A3. Wire it into `input_hk1_calibration.rs`** as the first call after `launch()`. Then the calibration runs fully unattended from a fresh launch.

**New path (recommended), one in-save session:**

1. Run `cargo test -p horsey-mod --test input_hk1_calibration -- --test-threads=1 --nocapture` with a save loaded. Captures the ratio between OS screen pixels (what `input.mouse.move` accepts) and the game's LOC cursor floats. If the ratio is 1:1 (likely; same ScreenToClient transform), no math is needed; if not, modforge picks up a tiny `screen_to_game` helper.
2. Resolve the screen coordinates of a horse + the target slot. Several options: read the BPGE entity world position + project via the game's view matrix; or capture them interactively via the existing overlay calibration UI; or use `hk1.locate_horse` if it already returns screen coords.
3. Synthesize the transfer:
   ```
   input.combo {
     keys: ["shift"],
     then: {
       op: "input.mouse.drag",
       args: {
         button: "left",
         x1: horse_screen_x, y1: horse_screen_y,
         x2: slot_screen_x,  y2: slot_screen_y,
         backend: "l3",
         duration_ms: 200, steps: 16
       }
     }
   }
   ```
4. Verify via `hk1.locate_horse` that the horse's container changed.

If step 3 fires `FUN_1400d2ab0` end-to-end, the 4 helpers run for free and the transfer completes. No further RE needed.

Backing infrastructure shipped:
- `modforge::seh::guard` (SEH-protected closure invocation. Crashes inside vanilla return `SehError` instead of killing the game. **Iteration time per crashing experiment: ~5 seconds (read log, adjust, retry) vs ~5 minutes pre-SEH (restart game, recalibrate, reload save).**
- `modforge::hook::Hook<F>` (generic detour wrapper around `retour::GenericDetour`. Auto-SEH on `call_original!`. Registry of installed hooks. `combinator.rs` migrated as the first consumer (net -13 lines including new `vanilla_crashes` counter).
- HTTP ops: `hk1.probe.{active_location, scene_slot_vtables, mouse_globals, locate_horse}`, `hk1.{read_cursor, set_target, transfer, loc_bytes}`, `mem.poke`.
- Overlay UI: per-horse calibration + transfer + snapshot buttons. All actions append to `<dll_dir>/hk1_overlay.log`.
- 11 tests under `horsey-mod/tests/hk1_*.rs` covering each primitive.

### NEXT: Proper horse-editing tools (2026-05-16)

User-locked 2026-05-16: we need first-class read/write for ANY allele on ANY horse, plus a generic field-poke for everything else on the Horse struct. HTTP first, UI follows.

**Generic-test discipline.** Every primitive ships as a parameterized cmdlet-style test driven by `HORSEY_*` env vars, never a horse-name-specific one-shot. Helpers in `tests/common/mod.rs::{target_horse, find_owned, wait_for_target_horse, env_or}`. Allele tests use a single 0..=479 address space (idx < 240 = vanilla, idx >= 240 = ext). All `*_tomtato.rs` predecessors deleted 2026-05-16.

Sequenced stages, one commit + push + checkpoint between each:

1. **Stage 1. Verify vanilla allele offset.** [DONE 2026-05-16] `tests/horse_allele_get.rs` (no env) dumps all 240 vanilla bytes for the first owned horse; confirmed `horse + ctx_offset() == 0x2b8`. Values 0..=3, structure matches across related horses.
2. **Stage 2. Vanilla allele HTTP ops + generic e2e.** [DONE 2026-05-16] `horse::{vanilla_alleles, vanilla_allele, set_vanilla_allele, set_vanilla_alleles}` accessors. Ops `horse.vanilla.alleles.{get,set}`, `horse.vanilla.genome.{get,set}`. Generic test `horse_allele_set.rs` (env-driven). HTTP round-trip works on single bank.
3. **Stage 2.5. Diploid genome [RESOLVED 2026-05-16].** Writing only +0x2b8 left the render unchanged because the vanilla genome is DIPLOID: two banks of 240 bytes at `horse + 0x2b8` (primary) and `horse + 0x3a8` (paired = +0xF0). Found by reading the CRISPR apply function (`FUN_1400b39b0` at 0x1400b39b0) called from `FUN_1400fd3e0` (scan_for_chromosomes) at the end of the CRISPR Lab vial-zoom-out state. CRISPR writes EACH gene to both banks via `*(u8*)(horse + offset) = val; *(u8*)(horse + offset + 0xF0) = val;`. Render uses the paired bank (or dominance across both). Fix: `set_vanilla_allele` / `set_vanilla_alleles` now write both banks; `vanilla_alleles_both()` reads both. Visible change confirmed in-game when all 480 alleles set to 3. Lesson: when the game has an in-game feature that does what you're modding, READ ITS DECOMP FIRST. Full notes in [`HORSE-PLACES.md`](HORSE-PLACES.md#vanilla-genome-diploid-two-banks-of-240-bytes).
4. **Stage 3. Generic Horse field-poke.** `horse.field.{get,set}` ops taking `{horse_ptr, offset, width: u8|u16|u32|u64, value}`. Bounds-check against `horse_offset::alloc_size`. Generic test `horse_field_set.rs` env-driven. Needed BEFORE Stage 4 so we can probe arbitrary offsets while hunting the render-source buffer in Stage 2.5.
5. **Stage 4. Named-field pass over HORSE-PLACES.md.** Audit every documented field, add typed `horse.<name>.get/set` ops. 2-3 commits grouped by section.

After this lands, the order below resumes:

1. **Per-horse Details UI**. Chromosome-strip layout (see [per-horse details view](#per-horse-details-view-horses-tab-details-expand) below). Backed by real accessors; renders chromosomes-of-strips, not a raw 480-cell grid.

### NEXT-NEXT: Chromosome-strip Details UI (2026-05-16)

[Phase A "raw 480-button grid" SHIPPED 2026-05-16 commit pending] Works but ergonomically unmaintainable: 480 tiny single-digit buttons crammed into a 16x30 grid. User feedback: "too many buttons". Replace with CRISPR-style chromosome strips and beat the in-game CRISPR Lab's UX on every axis.

**Ground truth from CRISPR decomp:**
- `DAT_14030d110` = chromosome -> gene-offset table.
- Layout: `i32[chromosome_id][0..17]`, stride `0x44` bytes per chromosome.
- Up to 20 chromosomes (CRISPR rejects chromosome_id >= 0x14).
- `-1` sentinel = "no gene in this slot."
- Valid entry = byte offset on Horse (e.g. `0x2b8` for gene 0 on chromosome 0).

**Stages, ship + checkpoint each:**

1. **C1. Resolve `DAT_14030d110` at runtime.** [DONE 2026-05-16 commit `bf629110`] `targets::resolve::chromosome_table()` anchors on the unique `FUN_1400b39b0` prologue (`movsxd rax, [rcx+0x234]; cmp eax, 0x13`), scans the first 1KB of the function body for every `lea r64, [rip+disp32]` (16 reg + REX prefix combos), and validates each candidate. Validator: 20 rows of i32, each row terminated by the first `-1`, total 240 unique entries in `[0, 240)`. **Key correction:** entries are FLAT gene indices (0..239), NOT byte offsets. CRISPR's caller passes `horse + 0x2b8` as the destination pointer so `*(u8*)(idx + dest)` lands at the right place. Op `targets.resolve.chromosome_table`; test `tests/chromosome_table_resolves.rs` dumps the full table. 20 chromosomes confirmed: 8-16 genes each, totaling 240.
2. **C2. `genes::chromosome_map() -> &'static [Chromosome]`.** Walks the resolved table once, caches in a OnceLock. `Chromosome { id: u8, slots: Vec<u8> }` where each slot value is the flat gene index 0..239. Plus inverse lookup `flat_to_chromosome(idx: u8) -> Option<(u8, u8)>` (chromosome_id, slot_pos) so the UI can tag any flat write with its chromosome/position. Test asserts 240 unique slots across 20 chromosomes and the inverse round-trips. Op `genes.chromosomes.dump` returns the cached map as JSON.
3. **C3. Replace overlay grid with chromosome strips.** Per horse, render: chromosome list. Each strip = caret + `c0`..`c19` label + the up-to-16 vanilla cells for that chromosome (current allele tier, click cycles 0->1->2->3, writes BOTH diploid banks via `horse::set_vanilla_allele`). Strips collapsible (state per overlay session). EXT layer renders below as one extra "ext" group.
4. **C4. Beat CRISPR's UX.** Things in-game CRISPR canNOT do that we will:
   - View ALL current alleles across the whole genome at once (CRISPR only shows incoming chromosomes).
   - Direct edit (no chromosome-collection minigame).
   - Snapshot a horse's full genome to clipboard / disk; paste onto another horse. (CRISPR has a clipboard but only for the working chromosome set.)
   - Side-by-side compare two horses (next/prev arrows on the expand panel; diff highlight).
   - Bulk per-chromosome ops: "max this chromosome", "wild-type this chromosome", "copy from another horse".
   - Filter: "show only chromosomes that differ from species default" / "show only nonzero".
5. **C5. Gene name labels.** [DONE 2026-05-16] Vanilla gene names sourced from `<game-root>/data/genes.xml`, loaded via `crate::gene_names::vanilla_gene_names()` (OnceLock cache). 240 names in document order; verified XML order == engine flat index (`SIZE`=0, `ASPECT`=1, `SKINNY`=2, ...). UI shows the comma-separated name preview next to each chromosome header; hovering any cell shows `GENE_NAME (flat N, allele tier V)`. Op `genes.vanilla.names` returns the list; test `tests/vanilla_gene_names.rs` asserts the load. **Layout problems remain** (see C7 below): names blow up horizontal width and the preview line wraps unevenly. Ext gene name labels still TODO (load from `genes-extended.xml` which the mod already parses; expose via parallel `gene_names::ext_gene_name(idx)`).

5a. **C7. Detail-view polish [OPEN].** The chromosome-strip layout is structurally correct (20 chromosomes x 8-16 named genes per CRISPR's grouping). Cells are anonymous digit buttons; preview line overflows. Plan:
   - Drop the comma-separated preview line. Rely on cell tooltip for the name.
   - Swap single-digit cell buttons for colored squares with first-letter overlay (e.g. `S` for SIZE). Color saturation encodes tier 0..3.
   - Group ext genes (today they are one flat 240-cell block, no chromosome metadata). Options: `chromosome="X"` attribute in `genes-extended.xml`, or auto-group by render mode (add/mul/set), or prefix match on `BX_<FAMILY>_*`.
   - Bulk per-chromosome buttons: "max this chromosome", "wild-type this chromosome".

5b. **C8. Matrix-view tab.** First cut shipped 2026-05-16: rows = horses, columns = 480 cells (240 vanilla chromosomally grouped + 240 ext flat). Built. **Layout was wrong**: per `/rtfm` review, no real-world tool shows that many columns at once. [Dwarf Therapist](https://github.com/Dwarf-Therapist/Manual) ships ~13 specialized Grid Views; [Morpheus](https://software.broadinstitute.org/morpheus/tutorial.html) (bio heatmaps for 20k+ genes) defaults to top-500-by-variance + search + drill; [PKHeX](https://deepwiki.com/kwsch/PKHeX) never shows 1000 mons as a matrix (paginated boxes). All of them: default = SUBSET, search/sort first-class, drill-down for edit, bulk via expressions.

5c. **C8b. Matrix-view rework [OPEN, NEXT, USER-LOCKED 2026-05-16].** Three zoom levels, each fits on a screen without scrolling:
   - **Herd overview**: rows = horses, columns = 20 chromosome SUMMARIES + 1 ext summary = 21 cells per row. Each summary = colored square encoding chromosome avg (or sum) of the horse's tiers on that chromosome. 100 horses fit.
   - **Chromosome zoom**: click any chromosome swatch -> shows that chromosome's ~12 named genes across ALL horses (rows = horses, columns = ~12 named genes). Drill into "everyone's BODY_SIZE column" comparisons.
   - **Horse detail**: click a horse name -> the existing chromosome-strip Details panel (C7 polish target).
   - Filter row above: name search, sort by chromosome-N-sum, "show only horses where SIZE > 2".
   - Saved Views (Dwarf-Therapist style) deferred to follow-up.
   - Keep palette 0=dark grey, 1=blue, 2=green, 3=bright yellow across all three levels.
6. **C6. Persistence helpers.** `horse.genome.snapshot.{save,load,list}` HTTP ops backed by JSON files under `<DLL_dir>/snapshots/`. UI: "Save snapshot..." / "Load snapshot..." dropdown on the Details panel.
2. **D2 (per-pop weight extension)** so new horses spawn with non-zero ext alleles based on pop-extended.xml.
3. **D4 (save sidecar) address re-derivation.** Until D4 arms, allele edits don't survive save/load. Same R3 work as GAMESTATE_PTR today.

### Content-authoring path forward (2026-05-16) [SUPERSEDED by NEXT above]

Decision: NO hot-reload of `genes-extended.xml`. Iterate the normal way: edit XML, `k3sc cargo-lock run -p horsey-mod --bin horsey-play --release` to relaunch + inject, validate via HTTP + visible in-game effect. Hot-reload is janky and not worth the complexity.

### Per-horse details view (Horses tab "Details" expand)

Goal: surface ALL data about a single horse, including the 240 vanilla genes and our 240 extension genes (480 total).

UI plan:
- Add a "Details" button to each row in the Horses tab.
- Click toggles a single per-horse expanded section below the list (one horse expanded at a time; clicking the button on a different row swaps).
- Expanded section shows:
  - Scalar block: age, max_age, skill, tired_a, tired_b, breeding_flag, on_track_flag, name_id, ctx_offset, litter_size_stat, species, horse_ptr.
  - 24x20 compact gene grid (480 cells), no per-gene labels until we know names. Format: `g000:NN g001:NN ... g023:NN` per row.
- No new ImGui primitives required (existing text/button is enough).

Research blockers (must do before genes show up):
- Locate the 240 vanilla allele values on the Horse object. The breeding combinator (`FUN_1400a2d80`) reads them; trace its arg layout to recover the offset.
- Decide how to render the 240 extension alleles. They live in `EXT_HORSE_GENOMES` keyed by horse_ptr (genes.rs); already accessible from the mod side, just plumb through.
- Optional: load gene NAMES from `genes.xml` so the grid can show `gSIZE:NN` etc.

Scoped sub-tasks (do in order, ship each on its own):
- [ ] Phase A: Details button + scalar block only. Genes show as `(not wired)`. Ship today.
- [ ] Phase B: Vanilla 240 gene values. Decomp combinator arg layout, add `horse::vanilla_alleles(horse_ptr) -> [u8; 240]`, render grid.
- [ ] Phase C: Extension 240 gene values from `EXT_HORSE_GENOMES`. Render same grid format below vanilla block.
- [ ] Phase D: gene-name labels from `genes.xml` (vanilla) + `genes-extended.xml` (ext). Hover tooltip or replace `gNNN:` prefix.

### Field-semantic discovery: dual-purpose watch-tests (2026-05-16)

> **Finding from the HLT prior-art deep read** (see [`PRIOR-ART-HorseyLiveTweaks.md`](PRIOR-ART-HorseyLiveTweaks.md#deep-cross-reference-findings-2026-05-16)).

HLT got their offset semantic names (hungry vs tired, stamina vs age, life-stage vs species) by twiddling bytes live in their in-game ImGui UI and watching the game react. Not by static analysis. The reason they have richer field semantics than we do is feedback-loop latency, not RE skill.

Initial reaction was "we need a byte-poker UI panel." Re-examined: that's wrong for our project, and not just wrong by a little. A UI is STRICTLY SLOWER than tests in our setup, because UIs require a human-in-loop for both driving the game and observing the reaction, and that round-trip ("user plays, then tells Claude what happened") is the slowest part of any feedback cycle.

Our HTTP cmdlets already drive state (feed a horse, sleep, race, trigger CRISPR, etc.), not just observe it. That means a test can perform the trigger AND observe the reaction AND assert the result in one closed loop. Claude writes test, runs `cargo test`, reads the diff, iterates. No alt-tab. No human telling Claude what happened. End-to-end automated.

HLT's UI is human-in-loop for both drive (slider) and observe (game window). Ours is human-out-of-loop for both. Not equivalent. Strictly faster, when the driving cmdlet exists.

**Corollary:** when a cmdlet to drive a specific action doesn't exist yet, the right move is to ADD the cmdlet (and ship it as a permanent control surface usable by every future test) rather than reach for a UI workaround. Every offset discovery session ends with two artifacts shipped: the cmdlet that drives the action, and the watch-test that asserts the reaction.

#### The watch-test shape

`tests/watch_region.rs` (env-driven, generic across BASE/OFFSET/LEN/TRIGGER):

```
HORSEY_WATCH_BASE=horse_ptr       # or gamestate, scene, or raw hex addr
HORSEY_WATCH_INDEX=0              # which horse / scene
HORSEY_WATCH_OFFSET=0x1f0
HORSEY_WATCH_LEN=64
HORSEY_WATCH_HZ=10
HORSEY_WATCH_TIMEOUT_MS=2000
HORSEY_WATCH_TRIGGER=cmd.feed_horse(0)   # optional: cmdlet to drive
HORSEY_WATCH_EXPECT=0x205:1->0           # optional: assert this transition
k3sc cargo-lock test -p horsey-mod --test watch_region -- --nocapture
```

Closed-loop flow when all four are set (the production shape):

1. Test records baseline of the configured region via `mem.read`.
2. Test invokes `TRIGGER` cmdlet via HTTP.
3. Test polls the region at `HZ` until either `EXPECT` transition is observed or `TIMEOUT_MS` elapses.
4. Test asserts the transition was observed within the window AND prints the full byte-diff log (every byte that changed and when).

This is end-to-end automated. No human action. No alt-tab. The test runs, exits with pass/fail, and the diff log captures both the asserted transition and any incidental changes (which are themselves new offset-discovery leads).

Modes are picked by which env vars are set:

- **Discovery mode** (`TRIGGER` set, no `EXPECT`). Test drives the action, prints the full byte-diff log, exits success. Used to find candidate fields when we know the trigger but not which byte responds. Claude reads the log and forms hypotheses.
- **Assertion mode** (`TRIGGER` + `EXPECT` both set). Full closed loop. The moment we believe "0x205 is hungry, drops from 1 to 0 when fed," we set `EXPECT` and the test ships. Next regression run fails immediately if a game patch moves the field.
- **Manual mode** (no `TRIGGER`). Falls back to passive watch with `WAIT_SECONDS`; user does the action by hand. Only useful for actions we cannot yet drive via cmdlet. Should be a temporary state; the work item that follows is always "add the missing cmdlet, convert this to closed-loop."

Companion: `tests/watch_until_change.rs`. Polls a single (base, offset, width) tuple, prints the transition timestamp, can optionally assert max-latency-to-change (proves "feeding causes the field to drop within 500ms" not just "it eventually drops").

#### Leverage

- The 9 remaining H-gb-low field offsets, Phase E of the R4 plan: run discovery on the GameState window, trigger each game action one by one, log what changed. Convert each finding to an `EXPECT` assertion.
- The 3 HLT semantic conflicts (0x1c, 0x1fc/0x200, 0x205): run discovery on horse bytes 0x1c..0x208 across feed/race/age actions, see which field moves with which trigger. The resolved semantics become permanent assertion tests.
- The 0x280 conflict (pointer pair vs timer): one-shot read as u64. Watch-test asserts the value is in the `0x1F....` heap range (our interpretation) or a small monotonic int (HLT's). Whichever passes is the truth, and the assertion lives forever.
- Any future RE on Horsey internals or other modforge consumer games inherits the test; extract to `modforge::tools::watch_region` once Horsey proves it.

#### Why not the UI panel?

- A UI is strictly slower than tests in our setup. UI is human-in-loop for both drive (slider) and observe (game window). Test is human-out-of-loop for both. The "user plays, then tells Claude what happened" round-trip is the slowest part of any cycle; tests skip it entirely.
- A test in the repo is the deliverable that satisfies CLAUDE.md ("every probe ships as a test"). A UI panel is throwaway.
- Our existing tests already do dual-purpose work (research + correctness assertion in one artifact). The watch-test slots into the same pattern.
- ~150 LOC vs the overlay-dxgi + ImGui table + bookmarks-file + render-callback-SEH investment a UI would need.
- The UI's only unique capability is continuous slider sweep across a value range while a human watches the game character. We don't need a human watching; we need transition detection, which the test does deterministically.

#### Tasks

- [ ] Author `tests/watch_region.rs` (rolling region diff in discovery mode; transition assertion in assertion mode; env-driven; output to a reviewable log).
- [ ] Author `tests/watch_until_change.rs` (single-value transition logger with optional max-latency assertion).
- [ ] Run on the 3 HLT semantic conflicts; commit one assertion-mode test per resolved semantic; update `horse_offset` names if HLT was right.
- [ ] Run on Phase E 8 mystery `FIELD_*` offsets; commit assertion tests for each discovered semantic.
- [ ] Run on the 0x280 conflict; commit the resolved-form assertion.
- [ ] Once horsey-mod has proven the shape, extract to `modforge::tools::watch_region` so schedule1-mod / grounded2-mod / outworld-station-mod can use it too.

### In-game UI: hudhook overlay [Step 1 SHIPPED 2026-05-15]

> **Plan:** [`../../docs/IN-GAME-OVERLAY-PLAN.md`](../../docs/IN-GAME-OVERLAY-PLAN.md).
> Generic `modforge::ui::overlay` module landed behind the `overlay-ui`
> Cargo feature. horsey-mod consumes via a ~40 LOC `HorseyOverlay` impl.
> Build clean. Phase A `ui.native.*` ops removed; `crate::ui` deleted.
> **Steps 2-5 remaining:** port tab bodies to imgui-rs, live-test
> against running Horsey, delete modforge's `native-ui` files, document.

Current state: our ImGui lives in its OWN OS window (`WNDCLASS "HorseyImGuiWnd"`), with its own D3D11 device and swap chain. HLT does the same thing (literally the same class name). Easy to ship but a distinct alt-tabbable window, not an overlay.

Timberbot ([abix-/TimberbornMods](https://github.com/abix-/TimberbornMods)) ships an in-game UI that renders INSIDE the game window. That mod is Unity / C# / BepInEx, so the path is different (BepInEx attaches to Unity's render loop), but the point stands: in-game UI overlays are a normal, solved problem with open-source references. The native-PE equivalent is well documented too: hook `IDXGISwapChain::Present`, init `ImGui_ImplDX11` against the game's device on first frame, render before the game's Present. ReShade and every cheat menu does this; the [kiero](https://github.com/Rebzzel/kiero) helper is the canonical minimal implementation (vtable lookup -> MinHook on Present index).

Concrete plan when we pick this up:
- [ ] Add a `modforge::ui::overlay_dxgi` module: kiero-style vtable scan to find Horsey's `IDXGISwapChain::Present` slot, MinHook it, init `ImGui_ImplDX11` on first call, render the same TabDef set we already use in `ui::native`.
- [ ] Wire WndProc forwarding so ImGui sees mouse/keyboard from the game's HWND.
- [ ] Behind a feature flag so existing native-window path stays available as a fallback.
- [ ] First arming pass: run alongside the current window; once stable, retire the separate window.

Tradeoffs:
- Win: UI floats over the game, no alt-tab; input handled natively; matches HLT's UX target without their separate-window UX.
- Cost: pulls in d3d11/dxgi deps, adds a hook on a hot path (Present is per-frame), needs SEH guards on the render callback. Not huge but real.

### Findings session 2026-05-15/16: Horses tab live-count + Horsey binary update

Authoritative catalog of every horse-bearing memory location: [`HORSE-PLACES.md`](HORSE-PLACES.md). This section captures the chase that produced it.

#### 1. Horsey binary was updated between sessions

- `image_sha256 = 0a2689fe98232a50b9cf5c83c7c76dc179912bef246b1d8014d918a77d390253`
- `exe_size = 4374528`, `image_base = 0x7ff6ab0a0000` in this run.
- Decomp index references the older build. Re-decomp or accept drift on non-anchored consts.

#### 2. Resolver sanity-gate self-defeating bug (FIXED 2026-05-15, REGRESSED, REFIXED 2026-05-17)

The 2026-05-15 fix replaced the legacy `abs_diff` check inside the gamestate-specific resolver, but the new `TargetRegistry` framework re-introduced the same class of bug as a generic `hint_tolerance` validator with a 0x1000 ceiling. When the 2026-05-17 Horsey build shifted `GAMESTATE_PTR` from RVA `0x3fb0d8` to `0x3fc1e8` (drift +0x1110), `WithinHintTolerance` rejected the correct pattern match and the resolver silently fell through to the stale hardcoded RVA, which dereffed to `0xfffffff90000000a` (tile data) and tripped `is_plausible_gamestate_pointer`. So every gamestate-dependent op started returning "no gamestate". The mod was effectively broken on the new build until this was found by trying to enter Home scene for HK1.

**Diagnostic chain (reusable for the next drift):**
1. `targets.resolve.gamestate_ptr` returned `slot == hardcoded_slot` and `deref` looked like garbage. That's the resolver-fell-back-to-hardcoded fingerprint.
2. Live `patterns.sleuth.scan_all` against `48 89 1D ?? ?? ?? ??` found 85 mov-store hits.
3. Filtered to slots whose deref was heap-shaped -> 24 candidates.
4. HLT-style structural validation per candidate (active_scene_id at +0x25C in `[-1, 256)`, scene_table at +0x438 heap-shaped) narrowed to 2.
5. The world-map state (asid=-1) AND scene_table in real heap range identified the winner: slot RVA `0x3fc1e8`.

**Fixed 2026-05-17:**
- `targets_registry::GAMESTATE_PTR.hint_rva` -> `0x1403fc1e8`, `hint_tolerance` -> `0x8000`. Pattern unchanged; it's the same HLT pattern and it still matches uniquely.
- `targets::GAMESTATE_PTR` const updated to `0x1403fc1e8`.
- New regression test `tests/gamestate_resolver_lives.rs` asserts `money_at_deref_plus_0x308` is non-null and owned_horses populates. Catches future drift fast.

**Lesson for the registry-wide audit:** every `TargetDef` with a tight `hint_tolerance` is a future silent-break risk. The real fix is HLT-style: PER-TARGET STRUCTURAL VALIDATORS that reject any candidate whose deref fails domain invariants. With those in place, hint_tolerance can be very wide (or removed entirely) because false matches are filtered structurally.

Action items:
- [ ] Audit every `TargetDef` in `targets_registry.rs`: if it has `hint_tolerance < 0x4000` AND no custom structural validator, either bump tolerance or add a validator.
- [ ] Add an `is_probable_gamestate_root` validator type in `modforge::patterns::sleuth` (deref the candidate, check active_scene_id range + scene_table non-null). Apply to GAMESTATE_PTR.
- [ ] On resolver success log `R3 drift detected: hint=X resolved=Y delta=Z` so stale hardcodes surface in the attach log.
- [ ] Once we have a known-builds manifest, refuse to attach when resolver delta says hardcoded is stale and no manifest entry matches the current image SHA.

#### 3. Scene table layout decoded (cross-validated with HLT)

HLT prior art (`research/prior-art/HorseyLiveTweaks/src/core/offsets.h`):
```
kRootSceneTable      = 0x438   // on GameState: ptr to scene table
kRootActiveSceneId   = 0x25C   // on GameState: i32 active scene id, range [-1, 256)
kMaxSceneSlots       = 256
kOffSceneHorseVecBegin = 0x130 // on scene struct
kOffSceneHorseVecEnd   = 0x138
kNeighborSceneId     = 31
```

`*(GS+0x438)` is an array of 256 scene pointers, each pointing to a scene struct with a `vector<Horse*>` at `+0x130/+0x138`. The active scene is at index `active_scene_id`.

Our code now follows this chain (see `gamestate::owned_stable`).

#### 4. Slot 0x00 of the scene table holds the 3 owned horses

`gamestate.scan_438_slots` dump in user's save (3 horses owned, `active_scene_id = -1`):

| slot | count | likely role |
|---|---|---|
| 0x00 | **3** | **Owned-horse list (matches user's 3 owned)** |
| 0x08..0x38 | 5 each | Race lanes (per `FUN_140105260` decomp) |
| 0xb0 | 4 | unknown |
| 0xd0 | 3 | another 3-horse mirror (selection? scene?) |
| 0xe0 | 2 | unknown |
| 0x70..0xc8 (singletons) | 1 each | per-screen selected horse |
| 0x120 | 5 | "copy all horses" source per `FUN_140107550`; race-context, not stable |

Action items:
- [ ] Change `owned_stable` to read scene table slot 0x00 unconditionally (NOT `slot_offset = active_scene_id*8`). When `active_scene_id = -1` (overworld / no neighbor scene), the per-scene chain returns None even though the owned list IS available at slot 0x00.
- [ ] Verify by naming the 3 horses uniquely and confirming they all appear under slot 0x00. Test `tests/find_horse_tomtato.rs` exists but is parked.
- [ ] Re-validate slot 0xd0: is it the "owned horses in current scene" subset, or a redundant mirror?

#### 5. Horse name resolution (PARTIAL: function exists, table address unknown)

Decomp `FUN_1400c78c0` at `:116676-116697` decodes the name resolver:
```
entry = NAME_TABLE + name_id * 0x88
if name_id == -1 || *(u8)(entry+0x40) != 0: invalid
size = *(u64)(entry+0x18)
if size > 0xF: heap string, ptr = *(u64)(entry+0x00)
else:          inline at entry+0x00 (size bytes)
```

Wired up `horse::name` + `horse::name_by_id` against this layout. Works in principle. BUT the hardcoded `NAME_TABLE` RVA `0x1403f34e0` is stale in the updated binary, and pattern resolution has not yet found the new address.

Attempts so far:
- Anchor on `imul reg, reg, 0x88` (`48 6B ?? 88`) + nearby `lea r64, [rip+disp32]`. No matches found, possibly because MSVC decomposed the multiply.
- Anchor on `cmp ecx, -1` (`83 f9 ff`) + nearby `lea`. Picked 0x7ff6ab494680 (RVA 0x3f4680), drift +0x11a0 from old. Entry bytes do NOT match std::string layout: first qword = 0 for entry 250, first 5 qwords = heap pointers for entry 272. Wrong table.

Likely real entry layout in the new build is a wrapper struct containing multiple sub-objects, not a bare std::string. Or the right table is at a different lea target we haven't validated.

Action items:
- [ ] Find name_resolver in new build by anchoring on the `(bails)` format-string xref (already used for `horse_offset::name_id`). The call instruction following `mov ecx, [horse+0x1f8]` is `call FUN_1400c78c0`. Decode its disp32 to recover the function entry in the new build.
- [ ] Read FUN_1400c78c0's body and decode the disp32 of the table-loading lea. That's NAME_TABLE.
- [ ] Re-examine the table entry layout. Old decomp may describe one build; the new build may use a wrapper struct (5 sub-pointers per entry observed). Determine which sub-pointer holds the name std::string.
- [ ] Re-enable `resolve::name_table()`. Currently returns None so UI falls back to numeric `#name_id`.

#### 6. patterns.read_bytes can crash the game (PARTIALLY FIXED)

Calling `patterns.read_bytes` with a non-readable address used to fault, kill the HTTP worker, and disconnect every client. Added a `is_addr_readable` precheck on both endpoints. Kept the game alive across multiple subsequent runs.

Still vulnerable: any other op that does unguarded raw deref of caller-supplied addresses. Audit all `gamestate.*` / `horse.*` / `mem.*` ops.

#### 7. Old `live_horse_count/live_horse_ptr` was always wrong (SUPERSEDED)

Reading `*(GS+0x130)` directly on GameState returned 0 in every state. The real per-scene horse vector lives on a SUB-STRUCT reached via the `GS+0x438` table. Our `live_horse_*` functions are superseded by `owned_horse_*`. Remove old API once UI fully migrated.

#### 8. Roster (GS+0x280/+0x288) is the global horse POOL, not owned

26 entries in earlier save, 28-29 in this save. Inline 36-byte summary records (no Horse* pointers in entries). Stride 0x24 confirmed via decomp at `:53951` (`/0x24`). Likely contains all named horses ever encountered: player + NPCs + ancestors. Useful as global ID pool, not as owned-list.

### Hot-reload crash hardening

`horsey-inject.exe --reload` swap reports success but the
game crashes a few seconds later. See
[`horsey-mod/docs/ROADMAP.md`](ROADMAP.md) for the
research-side investigation.

### Save sidecar

Persist `horsey-mod` state across game restarts. Pre-req for
the RPG layer.

### Roster UI / hotkeys / RPG layer

Tracked at the framework level for now; will graduate to
this todo once the bestiary mod is shipping.

---

# Appendix: Feature wishlist (merged from ROADMAP.md, 2026-05-15)

Originally a separate feature-wishlist doc; merged here to keep todo.md authoritative on all open work and feature ideas. The status-table content at the top of the original duplicated this doc; only the feature/idea sections are kept below.

## Active blocker

### Hot-reload crash

`horsey-inject.exe --reload` performs the swap and reports success, but
the game crashes a few seconds later. The post-reload state is briefly
visible (verified: `no_tire: true` was readable after the swap before crash).

Most likely cause: helper threads inside the old DLL haven't fully unwound
their stack when `FreeLibrary` is called. The `_shutdown` op spawns a
`horsey-shutdown` thread that lives inside the old DLL; if that thread
hasn't exited, the FreeLibrary returns "successfully" but the next time
that thread tries to return into freed code: crash.

Fix candidates:
1. Make `_shutdown` synchronous: stop the server in the request handler,
   wait until `shutdown_all()` has truly returned (all listeners joined),
   then return the HTTP response.
2. Injector polls port 33077 after `_shutdown` until it's closed, then
   waits another 500ms for in-flight helper threads, then `FreeLibrary`.
3. From the last remaining thread, call `FreeLibraryAndExitThread`.

Recommended: try (1) first (simplest, no injector change). If that's
insufficient, add (2). Use (3) only if both fail.

Workaround until fixed: relaunch Horsey + re-inject fresh on each rebuild.

---

## High-priority features

### Hotkeys

User-locked scope 2026-05-15: **`Shift+Click` (on horse) = transfer to current location.** That's the first and only hotkey in v1. Everything else stays out of scope until that one ships and gets real use.

- [ ] **HK1. Shift+Click smart-transfer.** Vanilla forces a click-and-drag every time you move a horse between the three locations:
      - **Vehicle** (truck the player drives around)
      - **Pasture** (home / corral)
      - **Race line** (starting position at the track, "ready to race")
      And these transfers happen constantly: load truck before going to a race, unload at the track to put horses on the line, reload to truck after the race, unload back to pasture at home, repeat. Every transfer is a manual click-drag of one horse at a time.

      HK1 collapses each transfer to a single Shift+Click on the horse. The mod **infers the right destination from context**:
      - At the track? Vehicle horse -> race line. Race-line horse -> vehicle.
      - At the pasture? Vehicle horse -> pasture. Pasture horse -> vehicle.
      - Anywhere else? Pasture <-> vehicle, since pasture is the implicit "home" target.

      Effect: every routine "load up for race day" / "put horses on starting line" / "bring everyone home" loop becomes Shift+Click per horse instead of click-drag per horse. Future bulk variants (Shift+Click on a group, hotkey to transfer ALL eligible) are out of v1.

#### Groundwork that ships alongside HK1

Build the foundation so future hotkeys are cheap to add. Don't add any hotkeys beyond HK1 in v1.

- [ ] SDL input hook. Intercept mouse/keyboard events before the engine handles them. Single dispatch point inside `horsey-mod`.
- [ ] Modifier-key state tracker. Reusable for any future modifier-click binding.
- [ ] "Currently hovered/selected horse" resolver. Given mouse position + game state, return the horse pointer under the cursor. Reusable for any horse-targeted hotkey.
- [ ] "Where is this horse now?" + "Where should it go from here?" resolvers. Read the horse's current container (vehicle / pasture / race line) and the player's current scene to pick the destination.
- [ ] "Move horse to <destination>" op. Wraps whatever vanilla function the click-drag flow calls when the player releases a dragged horse on a valid drop target. Find it via the existing transfer-menu code; reuse rather than re-implement so the move keeps all side effects vanilla expects (fatigue clearing on race-line drop, etc.).
- [ ] Settings entry for enabling/disabling HK1 (so users can opt out).

---

### Horse-crowd management (CRITICAL for 50+ horse households)

When many horses are at the same location (e.g. all your horses parked at
home), they stack on top of each other. Vanilla problems:

- Horses overlap pixel-for-pixel; you can't see how many are there.
- Clicking one horse is a lottery (you get whichever is rendered last).
- No way to tell at a glance which horses are hungry / tired / sick.
- Breeding setup is painful: you can't easily pick two specific parents.
- Counting "how many horses do I own?" requires moving the camera to find
  every stack.

#### Sub-features

**2. Roster panel overlay**

A side-panel UI showing ALL horses with one row each, columns sortable:

| Name | Loc | Age | Hunger | Tired | Breed-Ready | Skill | Status |

Click a row to focus camera + select that horse. Filter chips at top:
"Hungry only", "Tired only", "Old", "At home", "On track", etc.

Backend: the data already lives in our `horses.count`/`horse.read` ops.
Front-end:
- **ImGui overlay in MODFORGE** (locked 2026-05-15). Same approach
  HorseyLiveTweaks ships (see
  [`PRIOR-ART-HorseyLiveTweaks.md`](PRIOR-ART-HorseyLiveTweaks.md))
  and the same one we use in other game-mod projects. Build it
  ONCE in `modforge` as a reusable in-game-window primitive so
  every per-game mod (horsey-mod, schedule1, grounded2, etc.)
  gets it for free. horsey-mod is the first consumer.
- Web UI served from `modforge::server` stays as the secondary /
  scripting surface (browser tab + curl), not the primary UX.

**5. Count display**

Floating "N" badge above a stack of overlapping horses, showing the
count. Disappears when you mouse-over (so it doesn't block the spread
view).

**7. Pasture auto-buy hay**

Horses already auto-eat from pasture hay; the actual chore is BUYING the hay from the store to keep the pasture stocked. Toggle: "auto-buy hay when the pasture stock drops below the threshold". On, the mod posts the buy at the store on the player's behalf. Off, you do it manually. Removes the trip-to-the-store loop.

Implementation: read pasture hay stock + price; when stock crosses a configurable low-water mark and the player has enough money, fire the in-game buy op for N units. Same buy primitive the player triggers; we just automate the trigger.

Group operations (multi-select move / sleep / retire / bulk-sell at glue factory) are folded into the roster UI; no separate sub-feature.

#### Why this matters

The user explicitly identified this as the second-biggest tedium source
after fatigue/aging. Without these features the late-game experience
(50+ horses) is dominated by UI friction, not gameplay.

#### Implementation path

This entire feature stack rides on `horsey-mod`:

| Sub-feature | Backend | Frontend |
|---|---|---|
| Roster panel | Existing `horses.*` ops | modforge ImGui panel |
| Count display | Hook overlap detection | rendered via render hook |
| Pasture auto-buy hay | New ops to read pasture hay stock + post a buy at the store on threshold | none (runs automatically) |

Build the ImGui shell in `modforge` once (D-X-style swap-chain hook +
window/panel API) so horsey-mod, schedule1, grounded2 and future
per-game mods all get the same in-game control window. horsey-mod is
the first consumer and drives the API shape. The web UI on
`modforge::server` remains as the secondary scripting/automation
surface; ImGui is the primary user-facing UX.

This split is also how serious modders ship overlays in 2026: the
external-tool-via-localhost pattern is friction-free vs. embedding a
full UI library into the game's render path.

---

## Mod foundations

All shipped or designed; details in [`MODFORGE-INTEGRATION.md`](MODFORGE-INTEGRATION.md):

- §1 `horsey-mod` crate + HTTP control plane: SHIPPED 2026-05-13. See MODFORGE-INTEGRATION "Implementation status (2026-05-15)".
- §3 Save-edit tool: design + reuse of BXSAVEXT codec in MODFORGE-INTEGRATION "Other modforge-consumer plans".
- §4 RPG layer: skill list + Effect/Trigger/Skill mapping in MODFORGE-INTEGRATION "Other modforge-consumer plans".
- §5 Hotkey system: SDL3 input hook design in MODFORGE-INTEGRATION "Other modforge-consumer plans".

---

## Content-design ideas (for after foundations are in place)

### Genetic complexity beyond 240 genes

- **Extension gene table** in horsey-mod (sidecar to vanilla's chromoMap).
- New axes: personality matrix (16-dim), lineage drift, environmental
  adaptation, social temperament.
- Display via custom UI panel (web frontend or imgui overlay).

### Dynasty tracking

- Build an ancestry graph from horse-parent pointers.
- Track which "dynasty" wins races, breeds successfully, etc.
- Display dynasty trees in a web UI.

### Custom locations

- Hook the world-action dispatcher.
- Add a "Repurposed Building" overlay on an unused-feeling vanilla building
  (e.g. Pawn shop) that triggers custom logic.
- Eventually: a fully scripted location built entirely in horsey-mod.

### Procedural events

- Weekly random events: "An inspector arrives", "A storm damages fences",
  "A rival breeder challenges you".
- Injected into the daily-event scheduler (we annotated this at
  `0x14002fe00`).

---

## Open research questions

These could change the modding architecture; worth investigating early:

- [ ] Does Horsey have an existing ImGui integration we can reuse? (Probably no.)
- [ ] Is the SDL3 input layer hookable cleanly via DLL? (Probably yes via SDL3
      public API forwarding.)
- [ ] How does the game integrate with Steam. Does it call SteamAPI_Init,
      SteamUtils, etc.? (Yes; we have `SteamUtils010` reference at
      `0x1400c35b0`.)
- [ ] Are there per-frame update hooks we can hijack cleanly? (`0x1400dbe10`
      post_race_wrapup is one; there's a global per-frame tick somewhere.)
- [ ] What does the engine do when DLL load order is wrong? (Test by injecting
      a no-op DLL first.)

---

## Long-term: open the platform

Goal: Horsey modding becomes a real ecosystem. Required steps:

1. Publish `horsey-mod` to crates.io (or git URL).
2. Write a tutorial: "Build your first Horsey mod in 30 minutes".
3. Reference mod (e.g. a clean RPG layer) for new modders to copy.
4. Steam Workshop integration? (Unclear if SDK supports custom uploads.)
5. Auto-updater for horsey-mod itself (so mod authors don't pin to ancient
   game versions).

---

## Known control-plane issues (carried over from first-injection milestone)

Recorded 2026-05-13 during the first successful injection (proof-of-life). These are bugs/refinements to the HTTP control plane, distinct from the mod-feature TODOs above.

1. ~~**Snapshot in WRITE responses captures pre-write state.**~~ FIXED 2026-05-15. Snapshot moved AFTER the op dispatch in `lib.rs`'s HTTP handler so write ops surface the post-write game state in the `state` sub-object.
2. **No state-change broadcasting.** Clients have to poll `game.read` to see updates. A websocket or SSE upgrade is on the medium-term roadmap.
3. **Write-op gating.** Bot operators have run write cheats (e.g. $100k money) unprompted during smoke tests. Going forward, write-ops should be user-approved only.

---

## Open next reads (priority order)

Targets for the next decomp-and-document passes, in order of expected payoff:

1. `FUN_1400df280` retirement handler. Pinpoint the age threshold for "horse is too old to race".
2. `FUN_1400a3eb0` chromomap loader. Confirm how `genes.xml` attributes map to runtime behavior, esp. `OLD_AGE` direction.
3. `FUN_140089510` save-file open/write. Get the save struct layout.
4. `FUN_14008ffc0` genome clipboard copy. Get the human-readable genome format.
5. `FUN_140033a10` price formula. Confirm the `* years` factor and the age field offset on the horse struct.

## Working principle

Every claim in any horsey-mod doc must be backed by either:

1. Decompiled code from `Horsey.exe`, with the function address cited.
2. An in-game observation that directly tests the claim (save-diff after a known action).

Anything else is labelled `(INFERRED, NOT VERIFIED)` or removed.

## User's gameplay goal (the original motivator)

User wants to mod out the tedium loop:

1. Horse stamina / fatigue: a horse can only run ONE race before it's tired and needs to sleep. Want this much higher, or remove fatigue entirely.
2. Horse lifespan: horses die too quickly, forcing constant breeding/killing/swapping. Want longer lifespans (or much slower aging).

Complaint is FREQUENCY, not the mechanics. Goal is to dial knobs down, not delete mechanics. Target outcome: play sessions focused on racing and farming, not constant rotation of dying horses.


---

# Appendix: Action items consolidated from subject docs (2026-05-15)

Each subsection below was extracted from the doc named in its header. The source doc now keeps the narrative/plan structure but the actionable checkboxes live here as the single source of truth.

## Address resolution (R1-R5 phased plan)

_Source: [`ADDRESS-RESOLUTION.md`](ADDRESS-RESOLUTION.md)._

- [ ] Add `targets::image_sha256()`: read the running Horsey.exe
      bytes from the loaded module, hash the .text + .data
      sections (skip the .reloc table that changes per-load),
      log the result at attach.
- [ ] Add an HTTP op `game.build_info` returning hash + mtime +
      `image_base()` + a "known build name" if the hash matches
      one we've recorded.
- [ ] Add `horsey-mod/research/known-builds.toml`: a per-build manifest
      with `{ hash: "...", date: "...", decomp_index_path: "..." }`
      so future sessions can be told "you're on build X, decomp
      at Y" without guessing.
- [ ] Add a `targets::resolved` module. For each function we
      hook, define a patternsleuth signature. A sequence of
      bytes from the function's prologue, with `?` wildcards
      for compiler-shifted instructions (RIP-relative LEA
      displacements, immediate constants that the linker
      adjusts).
- [ ] At DLL attach (before any patch / detour), run a single
      patternsleuth scan over the .text section for all
      registered patterns. Populate a static map
      `(symbol, resolved_addr)`.
- [ ] `targets::fn_addr::APPLY_GENE_TO_HORSE` (and friends)
      become lookups into the map instead of `const usize`
      values. Pre-existing fixed-address callers keep working
      via accessor functions.
- [ ] HTTP op `targets.scan_report` returns
      `{ symbol: "...", resolved_addr: "0x...", matched_via:
      "pattern_id", confidence: "exact" | "fuzzy" }` so we can
      diff against the previous run.
- [ ] `horsey-mod/research/extract-signatures.py`. Inputs: the
      `decompiled/INDEX.md` + the function-body files. Output:
      a TOML / Rust constants file with prologue bytes and
      suggested wildcards for each function we care about.
      Wildcards picked by diffing the same function across
      multiple known builds. Bytes that differ across builds
      become `?`.
- [ ] CI / pre-merge check: run the extractor on the latest
      decomp, diff against the committed signatures, fail if
      any drift > threshold.
- [ ] Per-function comment in `targets/resolved.rs` cites the
      decomp file the signature was derived from
      (e.g. "from 1400a/1400a5d20.c lines 1-15").
- [ ] If a critical-path function fails resolution (e.g.
      EVAL_DIPLOID_BLEND_A), `arm()` for the related detour
      refuses to install and logs a clear "function not found
      in this build; re-extract signatures or extend wildcards"
      message.
- [ ] If a non-critical function (CRISPR UI dispatcher) fails,
      log + continue. The mod still works minus that feature.
- [ ] `dryrun` HTTP ops report `resolved: bool` per target so
      the operator can see what's missing before arming.
- [ ] At attach: compare `image_sha256()` to the hash that
      generated the current `decompiled/INDEX.md`. If different,
      print a big-yellow log line: "Game updated since last
      decomp; addresses MAY be stale."
- [ ] Optional: a `tools/refresh-decomp.ps1` wrapper that
      runs `horsey-mod/research/decompile.py` + `extract-signatures.py`
      back-to-back, commits the diff under a `chore:` prefix.
- [ ] Optional: a GitHub Actions job triggered manually that
      runs the same on a Windows runner with Ghidra installed.

## External-knowledge verification gaps

_Source: [`EXTERNAL-KNOWLEDGE.md`](EXTERNAL-KNOWLEDGE.md)._

- [ ] Confirm pop.xml `p0`/`p1`/`p2`/`p3` are INVERSE weights (read the spawn code in chromomap loader or similar).
- [ ] Confirm "press 5 in balloon" = x300 speed by reading the balloon controller / pause input handler.
- [ ] Find the buried-item ID -> item-type table in code (offsets 0-47).
- [ ] Find the building-uniqueness check (first-instance-wins logic).
- [ ] Find the CRISPR-Lab world-swap logic (vial sub-map teleport).

## Bestiary viability research

_Source: [`VIABILITY.md`](VIABILITY.md)._

- [ ] `FUN_1400c0660` (±5 mutator). Find callers,
      check for literal indices.
- [ ] `FUN_1400c03a0` (allele swap). Find callers.
- [ ] `FUN_1400c1cf0` (CRISPR?). Find callers.
- [ ] Is there unused padding after `DAT_1403ee4a4` we
      could exploit for in-place expansion? Check the
      symbol immediately following in the binary.
- [ ] How much headroom do we want? 256 = double, 512 =
      quadruple, 1024 = future-proof.
- [ ] Cross-reference vanilla `pop.xml` against the 233
      referenced indices to find genes that exist in code
      but no pop authors at non-default weights. Those
      are "soft-free" and may be safely repurposed by
      having our trampoline override their effect.
- [ ] Read each of the 7 hard-free slots (56, 57, 107,
      183, 184, 209, 216) and confirm they're really
      effect-free (the regex extracted only direct
      `FUN_1400a5d20(local_508, N)` calls; the gene
      might be read by other paths I haven't grepped).
- [~] Q-render-1: `pop.xml` checked in at
      [`vanilla-pop.xml`](vanilla-pop.xml). Per-oddity
      decomposition (which genes drive `car` wheels,
      `helix` shape, etc.) partially captured in
      GENE-CATALOG.md Part 2 "Vanilla pops"; full
      breakdown still pending per-species.
- [x] **DONE.** Map each of the 61 consumer-read slots to its horse-struct destination offset. Tool: `research/extract-consumer-map.py`. Result: 23 direct-copy slots, 38 conditional/intermediate. Output documented in [`GENE-CATALOG.md`](GENE-CATALOG.md) Part 2 "Buf-slot -> horse-struct field".
- [ ] Confirm the 91 fully-unused slots are not
      touched by other consumer chains (e.g. the
      breeding compatibility check
      `FUN_1400b78d0` calls `FUN_1400c5c10` with two
      buffers but uses different stack offsets, so it
      may consume more slots).
- [ ] For each "new visual mode" we want (wings, wheels,
      transparency, etc.), determine if any vanilla
      pop already exhibits it (specifically: does `car`
      reuse some gene to drive wheel rendering?). Need
      live `pop.xml` to answer.

## Content-creation verification gaps

_Source: [`CONTENT-CREATION.md`](CONTENT-CREATION.md)._

- [ ] **`p0..p3` are inverse weights** - read the gene-allele selection code.
- [ ] **Adding new gene NAMES** - read `FUN_1400a3eb0` (chromomap loader). Does it accept arbitrary names or only the 242 enum'd ones?
- [ ] **First-instance-wins building rule** - find the building-placement scan code.
- [ ] **Item ID 48+ bug** - find the item table.
- [ ] **400x225 map size** - find the tmx parser.

## Gene system open questions

_Source: [`GENE-CATALOG.md`](GENE-CATALOG.md) Part 1: Conceptual model._

- [x] **DONE 2026-05-15.** Breeding combinator located + algorithm verified. Full details in [`GENE-CATALOG.md`](GENE-CATALOG.md) Part 1 Step 1 "Breeding combinator".
- [ ] Confirm the runtime mutation-during-breeding
      behavior: does the child get random allele
      flips beyond the parent picks?
      (Not seen in `FUN_1400a2d80` body; pure parent
      copy. Mutation may be applied elsewhere, e.g. in
      `FUN_1400b2e30` after the combinator + engine
      run, or it may not exist for child-creation and
      drift only happens via `FUN_1400c0660` death
      handler.)

## Decompilation next steps

_Source: [`DECOMPILATION-STATUS.md`](DECOMPILATION-STATUS.md)._

- [ ] Read and document each of the 20 extracted key functions. Curated names + descriptions land in [`ALL-FUNCTIONS.md`](ALL-FUNCTIONS.md) (the merged function index).
- [ ] Reconstruct the `Horse` struct (offsets like `+0x350`, `+0x39c`, `+0x3a0` from `interact_dispatch_or_status_check` are all fields of the same horse struct).
- [ ] Identify the fatigue counter byte and the age field. Mod targets.
- [ ] Walk callers and callees of named functions; document those too. Expand outward.
- [ ] Use `Function ID` Ghidra analyzer with public SDL3/MSVC signature databases to bulk-name vendor functions and exclude them from manual work.

## Save format research

_Source: [`SAVE-FORMAT.md`](SAVE-FORMAT.md)._

Find the two fields needed to address the user's complaints, both presumably in one of the big binary blocks of `save1.dat` (most likely the 55KB block at 0x0d4f, possibly the 188KB tail):

- [ ] **Fatigue counter** (post-race tiredness). Save-diff procedure: save (state A), race a horse exactly once, save (state B), byte-diff. The bytes that changed are: race counter, fatigue, timestamp, RNG state, horse position, audience state. The fatigue counter should be at the same offset relative to the raced horse's record.
- [ ] **Age field** (and the retirement threshold). Save-diff procedure: save A, let many in-game days pass (no race/breed), save B. Bytes that monotonically increased = age candidates.
- [ ] **Identify genome location**. 242 genes x ~4 bytes = ~970 bytes per horse; 55KB block / 85 horses = 646 bytes/horse. Close enough to test. Per-horse genome may be variable length depending on which genes apply to that pop.
- [ ] **Confirm pop_id mapping** in the roster. Cross-reference observed pop_id values (1, 2, 3, 4, 5, 7, 12, 255) against `pop.xml` ordering. If it matches we have a free byte to mod a horse's apparent population without re-rolling its genome.
- [ ] **Decode flag_a..flag_e** in the 22-byte trailer. Could be sex, gender-marker, pregnant state, current location, breed sub-type, color.
