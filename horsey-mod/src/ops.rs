//! Horsey-specific ops registered on modforge's global op
//! registry. Every op here is a closure that reads or writes the
//! game state via `gamestate.rs` / `horse.rs`.
//!
//! Wire format: requests are JSON `{"op":"name","args":{...}}`,
//! posted to `127.0.0.1:33077/op` with the auth header
//! `X-Ueforge-Auth: <token>` (token is in `horsey.auth`).
//!
//! Each op returns a `Json` result; the envelope around it
//! (`{"ok":..., "op":..., "result":..., "state":...}`) is added
//! by lib.rs's handler.

use modforge::ops::{OP_REGISTRY, OpDef};
use serde_json::{Value as Json, json};

use crate::fatigue;
use crate::gamestate;
use crate::genes::{self, EXT_GENE_COUNT, ExtGene, ExtHorseGenome, RenderMapping, RenderMode};
use crate::genes_xml;
use crate::horse;
use crate::patches;

/// Register every Horsey op on the modforge global registry.
/// Called once from the worker thread at DLL init.
pub fn register_all() {
    OP_REGISTRY.register_many(vec![
        OpDef::new("ping", "Liveness check", "", |_| Ok(json!("pong"))),

        // ===== Hot-reload protocol =====
        //
        // The injector (`horsey-inject.exe --reload`) issues this op
        // before remotely calling FreeLibrary on our HMODULE. After the
        // op returns we drop every SpawnHandle in modforge's registry
        // so the HTTP listener thread joins and releases port 33077.
        //
        // We schedule the actual shutdown on a fresh thread so we can
        // RETURN the success response to the client first. If we
        // shut the server down before returning, the client never
        // sees the OK and may panic.
        OpDef::new(
            "_shutdown",
            "Stop the HTTP server. Used by the injector before unloading the DLL for hot-reload.",
            "",
            |_| {
                std::thread::Builder::new()
                    .name("horsey-shutdown".into())
                    .spawn(|| {
                        // Tiny delay so the in-flight HTTP response
                        // has time to flush to the client socket.
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        modforge::log!("horsey-mod: _shutdown -> dropping server");
                        modforge::server::shutdown_all();
                    })
                    .ok();
                Ok(json!({"shutting_down": true}))
            },
        ),

        OpDef::new(
            "list_ops",
            "List every op registered on the global registry.",
            "",
            |_| Ok(OP_REGISTRY.list_json()),
        ),

        // ===== GameState reads =====
        OpDef::new(
            "game.read",
            "Read the high-level game state snapshot.",
            "",
            |_| {
                Ok(json!({
                    "money": gamestate::money(),
                    "year": gamestate::year(),
                    "sleeps": gamestate::sleeps(),
                    "races": gamestate::races(),
                    "horse_count": gamestate::horse_count(),
                    "world_loaded": gamestate::looks_loaded(),
                    "no_tire": gamestate::no_tire(),
                    "debug_mode": gamestate::debug_mode(),
                }))
            },
        ),

        // Forensic dump of every byte / value the loaded-save heuristic
        // depends on. Driven by `tests/gamestate_diag.rs`. The op
        // itself never asserts; consumers print + diff.
        OpDef::new(
            "gamestate.diag",
            "Raw values used by gamestate::looks_loaded(). For tests / debugging.",
            "",
            |_| Ok(gamestate::diag()),
        ),

        // Heap string scanner. Walks the process address space via
        // VirtualQuery; for every committed, readable, PRIVATE region
        // (heap allocations), searches for the needle bytes. Returns
        // up to `max_hits` absolute addresses. Used to find string
        // names that don't live in image sections.
        OpDef::new(
            "mem.scan_heap_string",
            "Scan process heap for an ASCII string. Body: {needle: string, max_hits?: u32}. Returns hits with 64-byte context.",
            "{needle: string, max_hits?: u32}",
            |args| {
                use windows_sys::Win32::System::Memory::{
                    MEM_COMMIT, MEM_PRIVATE, MEMORY_BASIC_INFORMATION,
                    PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE, VirtualQuery,
                };
                let needle = args
                    .get("needle")
                    .and_then(Json::as_str)
                    .ok_or_else(|| "missing needle".to_string())?;
                let needle_bytes = needle.as_bytes();
                if needle_bytes.is_empty() || needle_bytes.len() > 256 {
                    return Err("needle length out of range (1..=256)".to_string());
                }
                let max_hits = args.get("max_hits").and_then(Json::as_u64).unwrap_or(50) as usize;
                const MAX_REGION: usize = 16 * 1024 * 1024; // 16MB per region
                const MAX_TOTAL: usize = 256 * 1024 * 1024; // 256MB total
                let mut scanned: usize = 0;
                let mut hits: Vec<serde_json::Value> = Vec::new();
                let mut addr: usize = 0;
                let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
                let info_size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();
                loop {
                    // SAFETY: caller-supplied addr is forwarded to VirtualQuery
                    // which is safe for any address.
                    let n = unsafe { VirtualQuery(addr as *const _, &mut info, info_size) };
                    if n == 0 { break; }
                    let base = info.BaseAddress as usize;
                    let size = info.RegionSize;
                    let next_addr = base.saturating_add(size);
                    if next_addr <= addr { break; }
                    addr = next_addr;
                    if info.State != MEM_COMMIT { continue; }
                    if info.Type != MEM_PRIVATE { continue; }
                    let prot_ok = matches!(info.Protect, PAGE_READWRITE | PAGE_READONLY);
                    if !prot_ok { continue; }
                    if info.Protect == PAGE_NOACCESS { continue; }
                    if size > MAX_REGION { continue; }
                    if scanned + size > MAX_TOTAL { break; }
                    scanned += size;
                    // SAFETY: region is MEM_COMMIT and readable per VirtualQuery.
                    let slice = unsafe { std::slice::from_raw_parts(base as *const u8, size) };
                    let mut i = 0usize;
                    while i + needle_bytes.len() <= slice.len() {
                        if &slice[i..i + needle_bytes.len()] == needle_bytes {
                            let hit_addr = base + i;
                            let ctx_lo = i.saturating_sub(16);
                            let ctx_hi = (i + needle_bytes.len() + 16).min(slice.len());
                            let ctx: Vec<String> = slice[ctx_lo..ctx_hi]
                                .iter().map(|b| format!("{b:02x}")).collect();
                            hits.push(json!({
                                "addr": format!("0x{hit_addr:x}"),
                                "region_base": format!("0x{base:x}"),
                                "context_around": ctx.join(" "),
                            }));
                            if hits.len() >= max_hits { break; }
                        }
                        i += 1;
                    }
                    if hits.len() >= max_hits { break; }
                }
                Ok(json!({
                    "needle": needle,
                    "bytes_scanned": scanned,
                    "hit_count": hits.len(),
                    "hits": hits,
                }))
            },
        ),

        // Walks .text for `cmp ecx, -1` (83 f9 ff) + look forward for a
        // `lea r64, [rip+disp32]` within 32 bytes. The combination is
        // the FUN_1400c78c0 name-resolver function entry. Reports each
        // match with the decoded lea target so we can spot the name
        // table address directly.
        OpDef::new(
            "diag.scan_cmp_neg1",
            "Scan .text for `cmp ecx, -1` then nearby `lea r64, [rip+disp32]`. Returns hits with decoded targets.",
            "",
            |_| {
                use modforge::patterns::sleuth;
                use modforge::winproc::is_addr_readable;
                let sites = sleuth::scan_all_matches("83 f9 ff").unwrap_or_default();
                let mut hits = Vec::new();
                for site in &sites {
                    if !is_addr_readable(*site + 32) {
                        continue;
                    }
                    // SAFETY: site+32 readability checked.
                    let bytes = unsafe {
                        std::slice::from_raw_parts(*site as *const u8, 32)
                    };
                    for i in 0..bytes.len().saturating_sub(7) {
                        if bytes[i] != 0x48 || bytes[i + 1] != 0x8d {
                            continue;
                        }
                        let modrm = bytes[i + 2];
                        if (modrm & 0b1100_0111) != 0b0000_0101 {
                            continue;
                        }
                        let disp = i32::from_le_bytes([
                            bytes[i + 3], bytes[i + 4], bytes[i + 5], bytes[i + 6],
                        ]) as isize;
                        let next_ip = (*site + i + 7) as isize;
                        let target = (next_ip + disp) as usize;
                        hits.push(json!({
                            "site": format!("0x{site:x}"),
                            "lea_offset": i,
                            "modrm": format!("0x{modrm:02x}"),
                            "target": format!("0x{target:x}"),
                        }));
                    }
                }
                Ok(json!({
                    "total_cmp_sites": sites.len(),
                    "hits_with_lea": hits.len(),
                    "first_30": hits.into_iter().take(30).collect::<Vec<_>>(),
                }))
            },
        ),

        // Diagnostic for the horse name resolver. Given a name_id, dumps
        // the rebased table base, computed entry address, every gated
        // deref result, and the bytes the resolver tried to read. Used
        // to confirm whether NAME_TABLE drifted between game builds.
        OpDef::new(
            "horse.name_diag",
            "Trace the name-resolver chain for one name_id. Body: {name_id: u32}",
            "{name_id: u32}",
            |args| {
                use modforge::winproc::is_addr_readable;
                let nid = args.get("name_id").and_then(Json::as_u64).unwrap_or(0) as u32;
                let slot = crate::targets_registry::resolve::name_table()
                    .unwrap_or_else(|| crate::targets::rebase(crate::targets::NAME_TABLE));
                // Resolver returns the static `.data` slot; deref to the live
                // heap table base (allocated after the save loads).
                let table = if is_addr_readable(slot + 8) {
                    unsafe { *(slot as *const usize) }
                } else {
                    0
                };
                let entry = table + (nid as usize) * 0x88;
                let entry_readable = is_addr_readable(entry + 0x88);
                let mut out = json!({
                    "name_id": nid,
                    "name_table_rebased": format!("0x{table:x}"),
                    "entry": format!("0x{entry:x}"),
                    "entry_readable": entry_readable,
                });
                if entry_readable {
                    // SAFETY: readability checked above.
                    let first_qword = unsafe { *(entry as *const usize) };
                    let qw_at_18 = unsafe { *((entry + 0x18) as *const usize) };
                    let byte_at_40 = unsafe { *((entry + 0x40) as *const u8) };
                    let preview: Vec<String> = (0..0x40)
                        .map(|i| {
                            // SAFETY: 0x40 < 0x88, page already verified.
                            let b = unsafe { *((entry + i) as *const u8) };
                            format!("{b:02x}")
                        })
                        .collect();
                    out["bytes_00_3f"] = json!(preview.join(" "));
                    out["first_qword"] = json!(format!("0x{first_qword:x}"));
                    out["size_at_18"] = json!(qw_at_18);
                    out["invalid_at_40"] = json!(byte_at_40);
                }
                Ok(out)
            },
        ),

        // Probe every slot in *(GS+0x438) to find which sub-system holds
        // the user's owned horses. For slot offset 0..0x200 step 8, deref
        // the slot, then read the +0x130/+0x138 begin/end pair, compute
        // count = (end-begin)/8. Caller picks the slot whose count matches
        // the number of owned horses in-game. Every deref guarded with
        // is_addr_readable.
        OpDef::new(
            "gamestate.scan_438_slots",
            "Scan *(GS+0x438) slot-by-slot, decode each slot's +0x130/+0x138 \
             vector<Horse*> count. Used to locate which slot holds owned horses.",
            "",
            |_| {
                use modforge::winproc::is_addr_readable;
                let gs = gamestate::ptr();
                let asid = gamestate::active_scene_id();
                if gs == 0 {
                    return Ok(json!({
                        "error": "no gamestate",
                        "gs_ptr": "0x0",
                        "active_scene_id": asid,
                    }));
                }
                let hop1_slot = gs + 0x438;
                if !is_addr_readable(hop1_slot) {
                    return Ok(json!({
                        "error": "GS+0x438 unreadable",
                        "gs_ptr": format!("0x{gs:x}"),
                        "active_scene_id": asid,
                    }));
                }
                // SAFETY: GS+0x438 readability just checked.
                let arr_ptr = unsafe { *(hop1_slot as *const usize) };
                let mut slots = Vec::new();
                let mut slot_off = 0usize;
                while slot_off < 0x200 {
                    let addr = arr_ptr.wrapping_add(slot_off);
                    if !is_addr_readable(addr) {
                        slot_off += 8;
                        continue;
                    }
                    // SAFETY: addr readability just checked.
                    let sub_ptr = unsafe { *(addr as *const usize) };
                    if sub_ptr != 0
                        && is_addr_readable(sub_ptr + 0x130)
                        && is_addr_readable(sub_ptr + 0x138)
                    {
                        // SAFETY: both endpoints readability checked above.
                        let begin = unsafe { *((sub_ptr + 0x130) as *const usize) };
                        let end = unsafe { *((sub_ptr + 0x138) as *const usize) };
                        if end >= begin {
                            let span = end - begin;
                            if span <= 0x10000 && span % 8 == 0 {
                                let count = span / 8;
                                slots.push(json!({
                                    "slot": format!("0x{slot_off:x}"),
                                    "sub_ptr": format!("0x{sub_ptr:x}"),
                                    "begin": format!("0x{begin:x}"),
                                    "end": format!("0x{end:x}"),
                                    "count": count,
                                }));
                            }
                        }
                    }
                    slot_off += 8;
                }
                Ok(json!({
                    "arr_ptr": format!("0x{arr_ptr:x}"),
                    "gs_ptr": format!("0x{gs:x}"),
                    "active_scene_id": asid,
                    "slots_with_horse_vec": slots,
                }))
            },
        ),

        // Player-owned horse list via the GS+0x438 -> +0x90 -> +0x130/+0x138
        // chain (per decomp FUN_140107510). Every deref is guarded with
        // is_addr_readable so a stale chain cannot fault the HTTP worker.
        OpDef::new(
            "gamestate.owned_horses",
            "Player-owned horse vector reached via GS+0x438 -> +0x90 -> +0x130/+0x138. \
             Returns count and per-entry Horse* + age/skill/tired flags.",
            "",
            |_| {
                let count = gamestate::owned_horse_count();
                let mut horses = Vec::with_capacity(count);
                for i in 0..count {
                    let Some(p) = gamestate::owned_horse_ptr(i) else {
                        horses.push(json!({"idx": i, "ptr": "0x0", "err": "ptr_unreadable"}));
                        continue;
                    };
                    let age = crate::horse::age(p);
                    let max_age = crate::horse::max_age(p);
                    let skill = crate::horse::skill(p);
                    let tired_a = crate::horse::tired_a(p);
                    let tired_b = crate::horse::tired_b(p);
                    let name_id = crate::horse::name_id(p);
                    let name = crate::horse::name(p);
                    let species = crate::horse::species(p);
                    // Trailer vs pasture is POSITIONAL (live-confirmed
                    // 2026-06-23): the scene placement at +0x1d4/+0x1d8 is a
                    // trailer position (~13, 9) for trailer horses and (0,0)
                    // for pasture horses on the overworld. The old +0x1d0
                    // guess is just a pickup-order counter, not the container.
                    let scene = crate::horse::scene_pos(p);
                    let container = match scene {
                        Some((x, y)) if x.abs() > 0.01 || y.abs() > 0.01 => "trailer",
                        _ => "pasture",
                    };
                    horses.push(json!({
                        "idx": i,
                        "ptr": format!("0x{p:x}"),
                        "age": age,
                        "max_age": max_age,
                        "skill": skill,
                        "tired_a": tired_a,
                        "tired_b": tired_b,
                        "name_id": name_id,
                        "name": name,
                        "species": species,
                        "scene_x": scene.map(|(x, _)| x),
                        "scene_y": scene.map(|(_, y)| y),
                        "container": container,
                    }));
                }
                Ok(json!({
                    "count": count,
                    "horses": horses,
                }))
            },
        ),

        // HK1 detection: the truck/trailer horse list lives on the MapState
        // (world-map state), NOT on the horse. Decomp FUN_1400cd5a0
        // (truck enter/leave handler) unloads the vector at
        // `*MAP_STATE_PTR + 0x130/+0x138` into a location on arrival and
        // refills it on leave. So a horse is "in the trailer" iff its Horse*
        // is in that vector; owned horses not in it are in the pasture.
        OpDef::new(
            "horse.trailer",
            "Read the truck/trailer horse list at *MAP_STATE_PTR + 0x130/+0x138 (the world-map \
             carrier) and cross-reference it against the owned horses. Decomp: FUN_1400cd5a0.",
            "",
            |_| {
                use modforge::winproc::is_addr_readable;
                let slot = crate::targets::rebase(crate::targets::MAP_STATE_PTR);
                if !is_addr_readable(slot) {
                    return Ok(json!({"error": "MAP_STATE_PTR slot unreadable"}));
                }
                // SAFETY: slot readability checked.
                let map_state = unsafe { *(slot as *const usize) };
                if map_state < 0x1_0000 || !is_addr_readable(map_state + 0x138) {
                    return Ok(json!({
                        "error": "MapState ptr not heap-shaped / unreadable",
                        "map_state": format!("0x{map_state:x}")
                    }));
                }
                // SAFETY: map_state+0x138 readability checked just above.
                let begin = unsafe { *((map_state + 0x130) as *const usize) };
                let end = unsafe { *((map_state + 0x138) as *const usize) };
                let mut trailer: Vec<usize> = Vec::new();
                if begin != 0
                    && end >= begin
                    && (end - begin) % 8 == 0
                    && is_addr_readable(begin)
                {
                    let n = (end - begin) / 8;
                    for i in 0..n.min(256) {
                        let pp = begin + i * 8;
                        if !is_addr_readable(pp) {
                            break;
                        }
                        // SAFETY: pp readability checked.
                        trailer.push(unsafe { *(pp as *const usize) });
                    }
                }
                let count = gamestate::owned_horse_count();
                let mut owned = Vec::with_capacity(count);
                for i in 0..count {
                    let Some(p) = gamestate::owned_horse_ptr(i) else { continue };
                    let in_trailer = trailer.contains(&p);
                    owned.push(json!({
                        "idx": i,
                        "ptr": format!("0x{p:x}"),
                        "container": if in_trailer { "trailer" } else { "pasture" },
                    }));
                }
                Ok(json!({
                    "map_state": format!("0x{map_state:x}"),
                    "trailer_begin": format!("0x{begin:x}"),
                    "trailer_end": format!("0x{end:x}"),
                    "trailer_count": trailer.len(),
                    "trailer_horses": trailer.iter().map(|h| format!("0x{h:x}")).collect::<Vec<_>>(),
                    "owned": owned,
                }))
            },
        ),

        // R3: pattern-scan-resolve the GAMESTATE_PTR data address
        // via modforge::patterns::sleuth. Recovery path for the
        // 'hardcoded address drifted' bug class.
        // Scan `.text` for RIP-relative instructions whose decoded
        // disp32 resolves to `target_addr`. Backed by
        // `modforge::patterns::sleuth::scan_all_matches` with a
        // patternsleuth `X<target>` xref pattern; no hand-rolled
        // byte scanning (rule locked in horsey-mod/docs/todo.md
        // "P0 RULE: USE PATTERNSLEUTH" and in global CLAUDE.md).
        // For each hit returns a 16-byte context window so
        // signatures can be authored directly from real compiler
        // output.
        OpDef::new(
            "mem.find_xrefs",
            "Scan .text for RIP-relative xrefs to a target data address \
            via patternsleuth X<target> pattern. Returns hits with byte context.",
            "{target_addr: u64, max_hits?: usize}",
            |args| {
                let target = args
                    .get("target_addr")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: target_addr".to_string())?
                    as usize;
                let max_hits = args
                    .get("max_hits")
                    .and_then(Json::as_u64)
                    .unwrap_or(64) as usize;
                // Patternsleuth's SIMD grouper needs anchor bytes
                // in the signature, so a bare `X<target>` won't
                // scan. Instead enumerate the common RIP-relative
                // opcode prefixes for x86_64 MSVC output and scan
                // each separately. Patternsleuth handles each
                // anchored scan; we union, dedupe, and return.
                //
                // The prefix list covers the encodings actually
                // emitted by MSVC for GameState-style global access:
                // u32/u8 loads, stores, compares with imm8/imm32,
                // adds with imm32, byte XORs, SSE moves.
                // Each entry: (name, prefix_bytes, disp32_offset_from_match_start, imm_suffix_size).
                // imm_suffix_size is how many bytes the actual instruction
                // has AFTER its disp32. Patternsleuth's `X<addr>` constraint
                // computes target relative to the byte right after the
                // disp32 bytes in the PATTERN. The CPU computes target
                // relative to the byte right after the FULL INSTRUCTION
                // (which includes the imm suffix). So for imm-suffixed
                // ops, patternsleuth_target = CPU_target - imm_size.
                // We compensate by querying patternsleuth with
                // (target - imm_size).
                const PREFIXES: &[(&str, &str, usize, usize)] = &[
                    ("mov_r64",     "48 8b 05",     3, 0),
                    ("mov_r32",     "8b 05",        2, 0),
                    ("mov_store64", "48 89 05",     3, 0),
                    ("mov_store32", "89 05",        2, 0),
                    ("lea_rax",     "48 8d 05",     3, 0),
                    ("lea_rcx",     "48 8d 0d",     3, 0),
                    ("lea_rdx",     "48 8d 15",     3, 0),
                    ("lea_rbx",     "48 8d 1d",     3, 0),
                    ("lea_rbp",     "48 8d 2d",     3, 0),
                    ("lea_rsi",     "48 8d 35",     3, 0),
                    ("lea_rdi",     "48 8d 3d",     3, 0),
                    ("lea_r8",      "4c 8d 05",     3, 0),
                    ("lea_r9",      "4c 8d 0d",     3, 0),
                    ("add_imm32",   "81 05",        2, 4),
                    ("add_imm8",    "83 05",        2, 1),
                    ("cmp_dw_imm8", "83 3d",        2, 1),
                    ("cmp_dw_imm32","81 3d",        2, 4),
                    ("mov_dw_imm",  "c7 05",        2, 4),
                    ("mov_b_imm",   "c6 05",        2, 1),
                    ("cmp_b_imm",   "80 3d",        2, 1),
                    ("xor_b_imm",   "80 35",        2, 1),
                    ("movzx",       "0f b6 05",     3, 0),
                    ("movdqa_xmm6", "66 0f 7f 35",  4, 0),
                ];
                let mut all_hits: Vec<(usize, &'static str, usize)> = Vec::new();
                for (name, prefix, disp_off, imm_size) in PREFIXES {
                    // Compensate for patternsleuth's X<addr> being
                    // computed against the end of the disp32 rather
                    // than the end of the (longer) instruction.
                    let patternsleuth_target = target.wrapping_sub(*imm_size);
                    let sig = format!("{prefix} X0x{patternsleuth_target:x}");
                    match modforge::patterns::sleuth::scan_all_matches(&sig) {
                        Ok(addrs) => {
                            for addr in addrs {
                                all_hits.push((addr, name, *disp_off));
                            }
                        }
                        Err(e) => {
                            modforge::log!("find_xrefs sig {sig:?} skipped: {e}");
                        }
                    }
                }
                all_hits.sort_unstable_by_key(|(a, _, _)| *a);
                all_hits.dedup_by_key(|(a, _, _)| *a);
                all_hits.truncate(max_hits);
                let entries: Vec<Json> = all_hits.iter().map(|&(instr_addr, name, disp_off)| {
                    let disp_addr = instr_addr + disp_off;
                    let start = instr_addr;
                    let end = disp_addr + 4 + 6;
                    let len = end - start;
                    // SAFETY: address is inside the .text section
                    // patternsleuth scanned; readable for the
                    // process lifetime.
                    let bytes: Vec<u8> = unsafe {
                        std::slice::from_raw_parts(start as *const u8, len).to_vec()
                    };
                    let context_hex = bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    json!({
                        "instr_addr": format!("0x{instr_addr:x}"),
                        "disp32_addr": format!("0x{disp_addr:x}"),
                        "opcode_class": name,
                        "context_hex": context_hex,
                    })
                }).collect();
                Ok(json!({
                    "target_addr": format!("0x{target:x}"),
                    "max_hits": max_hits,
                    "hits": entries,
                }))
            },
        ),

        // Read a primitive at an absolute address. For tests that
        // need to verify candidate addresses by inspecting their
        // surroundings (e.g. "this candidate is the money slot iff
        // money+0xC reads as a plausible year").
        OpDef::new(
            "mem.peek",
            "Read a u8 / u32 / u64 at addr. Used to verify candidate \
            addresses by their surrounding bytes.",
            "{addr: u64, kind?: 'u8'|'u32'|'u64'}",
            |args| {
                let addr = args
                    .get("addr")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: addr".to_string())?
                    as usize;
                if addr == 0 {
                    return Err("zero address rejected".to_string());
                }
                let kind = args
                    .get("kind")
                    .and_then(Json::as_str)
                    .unwrap_or("u32");
                // SAFETY: caller-controlled address; misuse crashes
                // the host (acceptable for a test-only op).
                let value = match kind {
                    "u8" => json!(unsafe { (addr as *const u8).read_unaligned() }),
                    "u32" => json!(unsafe { (addr as *const u32).read_unaligned() }),
                    "u64" => json!(format!(
                        "0x{:x}",
                        unsafe { (addr as *const u64).read_unaligned() }
                    )),
                    other => return Err(format!("unknown kind: {other}")),
                };
                Ok(json!({
                    "addr": format!("0x{addr:x}"),
                    "kind": kind,
                    "value": value,
                }))
            },
        ),

        // Generic byte-poker: write a typed value at a runtime addr.
        // Use for live RE / experimentation, not production writes.
        // Always reads back the value after writing for confirmation.
        OpDef::new(
            "mem.poke",
            "Write a u8/u32/u64 at addr. Body: {addr: u64, kind: 'u8'|'u32'|'u64', value: u64}. Returns the readback.",
            "{addr: u64, kind: 'u8'|'u32'|'u64', value: u64}",
            |args| {
                use modforge::winproc::is_addr_readable;
                let addr = args.get("addr").and_then(Json::as_u64)
                    .ok_or_else(|| "missing addr".to_string())? as usize;
                if addr == 0 { return Err("zero address rejected".to_string()); }
                let kind = args.get("kind").and_then(Json::as_str).unwrap_or("u32");
                let value = args.get("value").and_then(Json::as_u64)
                    .ok_or_else(|| "missing value".to_string())?;
                let nbytes = match kind {
                    "u8" => 1, "u32" => 4, "u64" => 8,
                    other => return Err(format!("unknown kind: {other}")),
                };
                if !is_addr_readable(addr) || !is_addr_readable(addr + nbytes - 1) {
                    return Err(format!("addr 0x{addr:x} not readable/writable"));
                }
                // SAFETY: caller-controlled write. Misuse will corrupt
                // game state or crash. Intended for live experimentation.
                let readback = unsafe {
                    match kind {
                        "u8"  => { (addr as *mut u8).write_unaligned(value as u8); json!((addr as *const u8).read_unaligned()) }
                        "u32" => { (addr as *mut u32).write_unaligned(value as u32); json!((addr as *const u32).read_unaligned()) }
                        "u64" => { (addr as *mut u64).write_unaligned(value); json!(format!("0x{:x}", (addr as *const u64).read_unaligned())) }
                        _ => unreachable!(),
                    }
                };
                Ok(json!({
                    "addr": format!("0x{addr:x}"),
                    "kind": kind,
                    "wrote": value,
                    "readback": readback,
                }))
            },
        ),

        // Find every `.data` position containing a given u32 / u64
        // / arbitrary-byte value. Backed by patternsleuth
        // (`sleuth::scan_data_matches`); no hand-rolled scanning.
        // Use to anchor address resolution on observed live values
        // (e.g. the player's on-screen money) instead of guessing
        // MSVC encodings from decomp output.
        OpDef::new(
            "mem.scan_data",
            "Patternsleuth scan of .data for a u32 (default) or u64 value, \
            or arbitrary bytes. Returns every matching address.",
            "{value: u64, kind?: 'u32'|'u64'|'bytes', bytes?: 'aa bb ..', max_hits?: usize}",
            |args| {
                let kind = args
                    .get("kind")
                    .and_then(Json::as_str)
                    .unwrap_or("u32");
                let sig = match kind {
                    "u32" => {
                        let v = args
                            .get("value")
                            .and_then(Json::as_u64)
                            .ok_or_else(|| "missing or non-u64 arg: value (kind=u32)".to_string())?
                            as u32;
                        v.to_le_bytes()
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                    "u64" => {
                        let v = args
                            .get("value")
                            .and_then(Json::as_u64)
                            .ok_or_else(|| "missing or non-u64 arg: value (kind=u64)".to_string())?;
                        v.to_le_bytes()
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                    "bytes" => args
                        .get("bytes")
                        .and_then(Json::as_str)
                        .ok_or_else(|| "missing string arg: bytes (kind=bytes)".to_string())?
                        .to_string(),
                    other => return Err(format!("unknown kind: {other}")),
                };
                let max_hits = args
                    .get("max_hits")
                    .and_then(Json::as_u64)
                    .unwrap_or(64) as usize;
                let mut addrs = modforge::patterns::sleuth::scan_data_matches(&sig)
                    .map_err(|e| format!("sleuth scan_data_matches failed: {e}"))?;
                let total = addrs.len();
                addrs.truncate(max_hits);
                let entries: Vec<Json> = addrs
                    .iter()
                    .map(|a| json!({"addr": format!("0x{a:x}")}))
                    .collect();
                Ok(json!({
                    "sig": sig,
                    "kind": kind,
                    "total_hits": total,
                    "hits": entries,
                }))
            },
        ),

        // Scan the loaded image's `.rdata` (read-only data section,
        // where string literals + vtables + constants live) for an
        // IDA-style sig. Use to locate format strings, vtable
        // signatures, and other rodata anchors that aren't reachable
        // via .text or .data scans.
        OpDef::new(
            "mem.scan_rdata",
            "Patternsleuth scan of .rdata for arbitrary bytes (IDA-style sig with `??` wildcards). \
            Returns every matching address.",
            "{sig: string, max_hits?: usize}",
            |args| {
                let sig = args
                    .get("sig")
                    .and_then(Json::as_str)
                    .ok_or_else(|| "missing string arg: sig".to_string())?
                    .to_string();
                let max_hits = args
                    .get("max_hits")
                    .and_then(Json::as_u64)
                    .unwrap_or(64) as usize;
                let mut addrs = modforge::patterns::sleuth::scan_rdata_matches(&sig)
                    .map_err(|e| format!("sleuth scan_rdata_matches failed: {e}"))?;
                let total = addrs.len();
                addrs.truncate(max_hits);
                let entries: Vec<Json> = addrs
                    .iter()
                    .map(|a| json!({"addr": format!("0x{a:x}")}))
                    .collect();
                Ok(json!({
                    "sig": sig,
                    "section": "rdata",
                    "total_hits": total,
                    "hits": entries,
                }))
            },
        ),

        // Two-address aliasing probe: write a sentinel byte at A,
        // read at B; if they're the same byte the read returns the
        // sentinel. Used by tests to prove a resolved address points
        // at the same byte as the hardcoded one. Restores A's
        // original value before returning, even on early-out.
        OpDef::new(
            "mem.alias_check",
            "Test whether two byte addresses alias the same memory. \
            Writes 0xAB then 0xCD to A, reads B after each, restores.",
            "{addr_a: u64, addr_b: u64}",
            |args| {
                let a = args.get("addr_a").and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: addr_a".to_string())? as usize;
                let b = args.get("addr_b").and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: addr_b".to_string())? as usize;
                if a == 0 || b == 0 {
                    return Err("zero address rejected".to_string());
                }
                // SAFETY: caller-controlled raw addresses. The op is
                // intended only for `.data` byte globals; misuse
                // crashes the host (acceptable for a test-only op).
                let pa = a as *mut u8;
                let pb = b as *const u8;
                let original = unsafe { *pa };
                let probe1 = 0xAB_u8;
                let probe2 = 0xCD_u8;
                unsafe { *pa = probe1; }
                let read1 = unsafe { *pb };
                unsafe { *pa = probe2; }
                let read2 = unsafe { *pb };
                unsafe { *pa = original; }
                let same = read1 == probe1 && read2 == probe2;
                Ok(json!({
                    "addr_a": format!("0x{a:x}"),
                    "addr_b": format!("0x{b:x}"),
                    "same_byte": same,
                    "read1_after_writing_0xab_to_a": read1,
                    "read2_after_writing_0xcd_to_a": read2,
                    "original_value_restored": original,
                }))
            },
        ),

        OpDef::new(
            "targets.resolve.field_offsets",
            "Return runtime-resolved struct field offsets via R4 (pattern-anchored decoding). \
             Compare to the hardcoded constants for drift detection.",
            "",
            |_| {
                Ok(json!({
                    "year": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::year()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::YEAR),
                    },
                    "sleeps": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::sleeps()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::SLEEPS),
                    },
                    "money": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::money()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::MONEY),
                    },
                    "horse_ctx_offset": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::ctx_offset()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::HORSE_CTX_OFFSET),
                    },
                    "tired_flag_a": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::tired_flag_a()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::TIRED_FLAG_A),
                    },
                    "tired_flag_b": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::tired_flag_b()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::TIRED_FLAG_B),
                    },
                    "on_track_flag": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::on_track_flag()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::ON_TRACK_FLAG),
                    },
                    "breeding_flag": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::breeding_flag()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::BREEDING_FLAG),
                    },
                    "age": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::age()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::AGE),
                    },
                    "max_age": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::max_age()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::MAX_AGE),
                    },
                    "horses_begin": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::horses_begin()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::HORSES_BEGIN),
                    },
                    "horses_end": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::horses_end()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::HORSES_END),
                    },
                    "live_horses_begin": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::live_horses_begin()),
                        "hardcoded": "0x130",
                    },
                    "live_horses_end": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::live_horses_end()),
                        "hardcoded": "0x138",
                    },
                    "sim_horses_begin": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::sim_horses_begin()),
                        "hardcoded": "0x260",
                    },
                    "sim_horses_end": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::sim_horses_end()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::FIELD_268),
                    },
                    "map_width": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::map_width()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::TRAILING_278),
                    },
                    "map_height": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::map_height()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::TRAILING_27C),
                    },
                    "horse_alloc_size": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::alloc_size()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::HORSE_ALLOC_SIZE),
                    },
                    "litter_size_stat": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::litter_size_stat()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::LITTER_SIZE_STAT),
                    },
                    "name_id": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::name_id()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::NAME_ID),
                    },
                    "skill": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::skill()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::SKILL),
                    },
                    "type_or_species": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::type_or_species()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::TYPE_OR_SPECIES),
                    },
                    "no_tire_loop_entry": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::no_tire_loop_entry()),
                        "hardcoded": format!("0x{:x}", crate::targets::rebase(crate::targets::horse_offset::NO_TIRE_LOOP_FN_RVA)),
                    },
                    "no_tire_loop_size": {
                        "resolved": format!("0x{:x}", crate::targets::horse_offset::no_tire_loop_size()),
                        "hardcoded": format!("0x{:x}", crate::targets::horse_offset::NO_TIRE_LOOP_BODY_SIZE),
                    },
                    "gamestate_alloc_size": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::alloc_size()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::GAMESTATE_ALLOC_SIZE),
                    },
                    "roster_stride": {
                        "resolved": format!("0x{:x}", crate::targets::gs_offset::roster_stride()),
                        "hardcoded": format!("0x{:x}", crate::targets::gs_offset::ROSTER_STRIDE),
                    },
                }))
            },
        ),
        OpDef::new(
            "targets.resolve.data_globals",
            "Resolve RACES_COUNTER + SAVE_VERSION_GLOBAL via patternsleuth.",
            "",
            |_| {
                let image_base = crate::targets::image_base();
                fn entry(addr: Option<usize>, fallback_rva: usize) -> Json {
                    let addr_hex = match addr {
                        Some(a) => format!("0x{a:x}"),
                        None => "0x0".to_string(),
                    };
                    json!({
                        "address": addr_hex,
                        "hardcoded": format!("0x{:x}", crate::targets::rebase(fallback_rva)),
                    })
                }
                Ok(json!({
                    "image_base": format!("0x{image_base:x}"),
                    "races_counter": entry(
                        crate::targets_registry::resolve::races_counter(),
                        crate::targets::RACES_COUNTER,
                    ),
                    "save_version_global": entry(
                        crate::targets_registry::resolve::save_version_global(),
                        crate::targets::SAVE_VERSION_GLOBAL,
                    ),
                }))
            },
        ),
        OpDef::new(
            "genes.vanilla.names",
            "Return the list of 240 vanilla gene names sourced from \
<game-root>/data/genes.xml, indexed by engine-flat gene_idx.",
            "",
            |_| {
                let names = crate::gene_names::vanilla_gene_names();
                Ok(json!({
                    "loaded": names.is_some(),
                    "names":  names.map(|n| n.to_vec()).unwrap_or_default(),
                }))
            },
        ),
        OpDef::new(
            "genes.chromosomes.dump",
            "Dump the parsed chromosome map (CRISPR table). Returns \
{chromosomes: [{id, slots: [flat_idx, ...]}], total_genes}.",
            "",
            |_| {
                let chromos = crate::chromosomes::chromosome_map();
                let arr: Vec<_> = chromos.iter().map(|c| json!({
                    "id":    c.id,
                    "slots": c.slots,
                })).collect();
                let total: usize = chromos.iter().map(|c| c.slots.len()).sum();
                Ok(json!({"chromosomes": arr, "total_genes": total}))
            },
        ),
        OpDef::new(
            "targets.resolve.chromosome_table",
            "Resolve CRISPR's chromosome -> gene-offset table (DAT_14030d110) via patternsleuth.",
            "",
            |_| {
                let addr = crate::targets_registry::resolve::chromosome_table();
                Ok(json!({
                    "address": addr.map(|a| format!("0x{a:x}")).unwrap_or_else(|| "0x0".into()),
                    "image_base": format!("0x{:x}", crate::targets::image_base()),
                }))
            },
        ),
        OpDef::new(
            "targets.resolve.cheat_globals",
            "Resolve NO_TIRE_TOGGLE, DEBUG_MODE_ACTIVE, DEBUG_LOG_GATE at runtime via patternsleuth.",
            "",
            |_| {
                let image_base = crate::targets::image_base();
                fn entry(addr: Option<usize>, fallback_rva: usize) -> Json {
                    let resolved = addr;
                    let addr_hex = match resolved {
                        Some(a) => format!("0x{a:x}"),
                        None => "0x0".to_string(),
                    };
                    let current_byte_value = resolved.map(|a| {
                        // SAFETY: byte read from a mapped .data global.
                        unsafe { *(a as *const u8) }
                    });
                    json!({
                        "address": addr_hex,
                        "hardcoded": format!("0x{:x}", crate::targets::rebase(fallback_rva)),
                        "current_byte_value": current_byte_value,
                    })
                }
                Ok(json!({
                    "image_base": format!("0x{image_base:x}"),
                    "no_tire_toggle": entry(
                        crate::targets_registry::resolve::no_tire_toggle(),
                        crate::targets::NO_TIRE_TOGGLE,
                    ),
                    "debug_mode_active": entry(
                        crate::targets_registry::resolve::debug_mode_active(),
                        crate::targets::DEBUG_MODE_ACTIVE,
                    ),
                    "debug_log_gate": entry(
                        crate::targets_registry::resolve::debug_log_gate(),
                        crate::targets::DEBUG_LOG_GATE,
                    ),
                }))
            },
        ),

        OpDef::new(
            "targets.resolve.gamestate_ptr",
            "Resolve the GAMESTATE_PTR slot address via patternsleuth, then \
             surface both the slot and its dereffed contents.",
            "",
            |_| {
                let image_base = crate::targets::image_base();
                let hardcoded_slot = crate::targets::rebase(crate::targets::GAMESTATE_PTR);
                let resolved_slot = crate::targets_registry::resolve::gamestate_ptr();
                let resolved_slot_hex = match resolved_slot {
                    Some(a) => format!("0x{a:x}"),
                    None => "0x0".to_string(),
                };
                // The slot is a `.data` pointer slot. Deref to get
                // the live GameState heap pointer (or 0 when no save
                // is loaded). Then read money at +0x308 to give the
                // test a value it can plausibility-check.
                let (live_ptr, money) = match resolved_slot {
                    Some(slot) => {
                        // SAFETY: slot is in `.data`, always mapped.
                        let raw = unsafe { *(slot as *const usize) };
                        if crate::gamestate::is_plausible_gamestate_pointer(raw) {
                            // SAFETY: heap pointer's +0x308 lives
                            // inside the 0x448-byte GameState alloc.
                            let m = unsafe {
                                ((raw + 0x308) as *const u32).read_unaligned()
                            };
                            (raw, Some(m))
                        } else {
                            (raw, None)
                        }
                    }
                    None => (0, None),
                };
                Ok(json!({
                    "slot": resolved_slot_hex,
                    "image_base": format!("0x{image_base:x}"),
                    "hardcoded_slot": format!("0x{hardcoded_slot:x}"),
                    "deref": format!("0x{live_ptr:x}"),
                    "money_at_deref_plus_0x308": money,
                }))
            },
        ),

        // ===== Money cheats =====
        OpDef::new(
            "game.money.get",
            "Read current money.",
            "",
            |_| Ok(json!({"money": gamestate::money()})),
        ),
        OpDef::new(
            "game.money.set",
            "Set money to a specific value.",
            "{value: u32}",
            |args| {
                let v = args_u32(args, "value")?;
                let ok = gamestate::set_money(v);
                Ok(json!({"set": ok, "money": gamestate::money()}))
            },
        ),
        OpDef::new(
            "game.money.add",
            "Add to money (negative subtracts; saturates at 0).",
            "{value: i32}",
            |args| {
                let delta = args_i32(args, "value")?;
                let current = gamestate::money().unwrap_or(0) as i64;
                let new_val = (current + delta as i64).max(0).min(u32::MAX as i64) as u32;
                let ok = gamestate::set_money(new_val);
                Ok(json!({"set": ok, "money": gamestate::money()}))
            },
        ),

        // ===== Year =====
        OpDef::new(
            "game.year.get",
            "Read current year (0-indexed; displayed +1 in-game).",
            "",
            |_| Ok(json!({"year": gamestate::year()})),
        ),
        OpDef::new(
            "game.year.set",
            "Set the year.",
            "{value: u32}",
            |args| {
                let v = args_u32(args, "value")?;
                let ok = gamestate::set_year(v);
                Ok(json!({"set": ok, "year": gamestate::year()}))
            },
        ),

        // ===== Cheats =====
        OpDef::new(
            "cheats.no_tire.get",
            "Read the No Tire toggle (true = fatigue disabled).",
            "",
            |_| Ok(json!({"no_tire": gamestate::no_tire()})),
        ),
        OpDef::new(
            "cheats.no_tire.set",
            "Enable / disable the GAME's built-in No Tire cheat. Note: \
this zeroes BOTH tired flags so the sleep gate breaks. Prefer \
fatigue.suppressor.set which only zeroes the race-eligibility flag.",
            "{enabled: bool}",
            |args| {
                let v = args_bool(args, "enabled")?;
                gamestate::set_no_tire(v);
                Ok(json!({"no_tire": gamestate::no_tire()}))
            },
        ),
        OpDef::new(
            "fatigue.suppressor.get",
            "Status of the (deprecated) per-frame Rust-side suppressor.",
            "",
            |_| Ok(json!({"enabled": fatigue::is_enabled()})),
        ),
        OpDef::new(
            "fatigue.suppressor.set",
            "Toggle the (deprecated) per-frame Rust-side suppressor. \
The proper fix is sleep_safe_no_tire (already auto-applied at attach).",
            "{enabled: bool}",
            |args| {
                let v = args_bool(args, "enabled")?;
                fatigue::set_enabled(v);
                Ok(json!({"enabled": fatigue::is_enabled()}))
            },
        ),
        OpDef::new(
            "patches.list",
            "List the binary patches currently applied to Horsey.exe.",
            "",
            |_| Ok(json!({"applied": patches::applied_names()})),
        ),
        OpDef::new(
            "cheats.debug_mode.get",
            "Read the debug-mode-active flag.",
            "",
            |_| Ok(json!({"debug_mode": gamestate::debug_mode()})),
        ),
        OpDef::new(
            "cheats.debug_mode.set",
            "Force debug mode on / off (skips the 'type \"debug\" in pause menu' unlock).",
            "{enabled: bool}",
            |args| {
                let v = args_bool(args, "enabled")?;
                gamestate::set_debug_mode(v);
                Ok(json!({"debug_mode": gamestate::debug_mode()}))
            },
        ),

        // ===== Horse roster =====
        OpDef::new(
            "horses.count",
            "Number of horses in the roster (36-byte records).",
            "",
            |_| Ok(json!({"count": gamestate::horse_count()})),
        ),
        OpDef::new(
            "horses.roster_addr",
            "Memory address of the roster entry at index `i`.",
            "{index: usize}",
            |args| {
                let i = args_usize(args, "index")?;
                let p = gamestate::horse_roster_entry(i);
                Ok(json!({"address": p.map(|a| format!("0x{:x}", a))}))
            },
        ),
        OpDef::new(
            "horses.live",
            "Walk gamestate+0x130/+0x138 and return every live horse with its full state. Powers the roster UI.",
            "",
            |_| {
                let n = gamestate::live_horse_count();
                let mut out = Vec::with_capacity(n);
                for i in 0..n {
                    let Some(h) = gamestate::live_horse_ptr(i) else { continue };
                    if h == 0 { continue; }
                    out.push(json!({
                        "index": i,
                        "address": format!("0x{:x}", h),
                        "age": horse::age(h),
                        "max_age": horse::max_age(h),
                        "skill": horse::skill(h),
                        "tired_a": horse::tired_a(h),
                        "tired_b": horse::tired_b(h),
                        "species": horse::species(h),
                        "name_id": horse::name_id(h),
                        "litter_stat": horse::litter_stat(h),
                    }));
                }
                Ok(json!({
                    "count": n,
                    "horses": out,
                }))
            },
        ),

        // ===== Direct horse-object accessors =====
        // Each takes a horse pointer (hex string) obtained from a hook.
        // For now the only path to get one of these is via a function
        // hook (which we'll add in the next phase). When that lands,
        // these become useful.
        OpDef::new(
            "horse.read",
            "Read all known fields of a horse at given address.",
            "{addr: hex-string}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                Ok(json!({
                    "address": format!("0x{:x}", h),
                    "age": horse::age(h),
                    "max_age": horse::max_age(h),
                    "skill": horse::skill(h),
                    "tired_a": horse::tired_a(h),
                    "tired_b": horse::tired_b(h),
                    "species": horse::species(h),
                    "name_id": horse::name_id(h),
                    "litter_stat": horse::litter_stat(h),
                }))
            },
        ),
        OpDef::new(
            "horse.set_age",
            "Set a horse's age field.",
            "{addr: hex-string, value: i32}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                let v = args_i32(args, "value")?;
                let ok = horse::set_age(h, v);
                Ok(json!({"set": ok, "age": horse::age(h)}))
            },
        ),
        OpDef::new(
            "horse.set_max_age",
            "Set a horse's max-age (lifespan) field.",
            "{addr: hex-string, value: i32}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                let v = args_i32(args, "value")?;
                let ok = horse::set_max_age(h, v);
                Ok(json!({"set": ok, "max_age": horse::max_age(h)}))
            },
        ),
        OpDef::new(
            "horse.clear_tiredness",
            "Zero out the tired flags on a horse.",
            "{addr: hex-string}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                let ok = horse::clear_tiredness(h);
                Ok(json!({"set": ok}))
            },
        ),

        // ===== Vanilla allele ops (read/write the 240-byte working genome) =====
        //
        // Backing offset is `horse_offset::ctx_offset()` (== 0x2b8 on
        // current Horsey build). Confirmed via dump_vanilla_alleles.rs
        // on 2026-05-16. Range 0..=3 per byte; one value per gene slot.
        OpDef::new(
            "horse.vanilla.alleles.get",
            "Read one vanilla allele byte. Returns {idx, value}.",
            "{addr: hex-string, idx: usize}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                let idx = args_usize(args, "idx")?;
                match horse::vanilla_allele(h, idx) {
                    Some(v) => Ok(json!({"idx": idx, "value": v})),
                    None => Err(format!(
                        "null horse or idx {idx} >= {}",
                        horse::VANILLA_GENOME_LEN
                    )),
                }
            },
        ),
        OpDef::new(
            "horse.vanilla.alleles.set",
            "Write one vanilla allele byte. `value` must fit in u8.",
            "{addr: hex-string, idx: usize, value: u8}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                let idx = args_usize(args, "idx")?;
                let value = args_usize(args, "value")?;
                if value > 0xff {
                    return Err(format!("value {value} > 0xff"));
                }
                let ok = horse::set_vanilla_allele(h, idx, value as u8);
                Ok(json!({
                    "set":   ok,
                    "idx":   idx,
                    "value": horse::vanilla_allele(h, idx),
                }))
            },
        ),
        OpDef::new(
            "horse.vanilla.genome.get",
            "Read all 240 vanilla allele bytes for a horse.",
            "{addr: hex-string}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                match horse::vanilla_alleles(h) {
                    Some(g) => Ok(json!({"alleles": g.to_vec()})),
                    None => Err("null horse".into()),
                }
            },
        ),
        OpDef::new(
            "horse.vanilla.genome.set",
            "Overwrite all 240 vanilla allele bytes for a horse. `alleles` \
must be exactly 240 u8 values.",
            "{addr: hex-string, alleles: [u8; 240]}",
            |args| {
                let h = args_hex_addr(args, "addr")?;
                let arr = args
                    .get("alleles")
                    .and_then(Json::as_array)
                    .ok_or("missing array arg: alleles")?;
                if arr.len() != horse::VANILLA_GENOME_LEN {
                    return Err(format!(
                        "expected {} alleles, got {}",
                        horse::VANILLA_GENOME_LEN,
                        arr.len()
                    ));
                }
                let mut buf = [0u8; horse::VANILLA_GENOME_LEN];
                for (i, v) in arr.iter().enumerate() {
                    let n = v.as_u64()
                        .ok_or_else(|| format!("alleles[{i}] not a number"))?;
                    if n > 0xff {
                        return Err(format!("alleles[{i}] = {n} > 0xff"));
                    }
                    buf[i] = n as u8;
                }
                let ok = horse::set_vanilla_alleles(h, &buf);
                Ok(json!({"set": ok}))
            },
        ),

        // ===== Extended-gene ops (D0 / D7 infra for the bestiary expansion mod) =====
        // These let us author and inspect extended-gene state via HTTP without
        // touching any vanilla data. Backed by `crate::genes`. See the gene-doubling
        // implementation plan in `horsey-mod/docs/todo.md`.
        OpDef::new(
            "genes.ext.count",
            "Return the configured extended-gene count and total (vanilla + ext).",
            "",
            |_| {
                Ok(json!({
                    "ext_gene_count": EXT_GENE_COUNT,
                    "vanilla_count": 240,
                    "total": 240 + EXT_GENE_COUNT,
                }))
            },
        ),
        OpDef::new(
            "genes.ext.list",
            "List every extended-gene definition (snapshot copy).",
            "",
            |_| Ok(serde_json::to_value(genes::list_ext_genes()).unwrap_or(json!([]))),
        ),
        OpDef::new(
            "genes.ext.get",
            "Read one extended-gene definition by extended-range index (0..EXT_GENE_COUNT).",
            "{ext_idx: usize}",
            |args| {
                let i = args_usize(args, "ext_idx")?;
                match genes::get_ext_gene(i) {
                    Some(g) => Ok(serde_json::to_value(g).unwrap_or(json!(null))),
                    None => Err(format!("ext_idx {i} out of range")),
                }
            },
        ),
        OpDef::new(
            "genes.ext.set",
            "Set one extended-gene definition by extended-range index. \
Args: ext_idx (usize), name (string), mutation_rate (u32), scale (u64), \
alleles ([u32; 4]), optional render { slot, mode: 'add'|'mul'|'set' }.",
            "{ext_idx, name, mutation_rate, scale, alleles, render?}",
            |args| {
                let ext_idx = args_usize(args, "ext_idx")?;
                let name = args_string(args, "name")?;
                let mutation_rate = args_u32(args, "mutation_rate")?;
                let scale = args
                    .get("scale")
                    .and_then(Json::as_u64)
                    .unwrap_or(1);
                let alleles_v = args
                    .get("alleles")
                    .and_then(Json::as_array)
                    .ok_or_else(|| "missing or non-array arg: alleles".to_string())?;
                if alleles_v.len() != 4 {
                    return Err(format!("alleles must be length 4, got {}", alleles_v.len()));
                }
                let mut alleles = [0u32; 4];
                for (i, v) in alleles_v.iter().enumerate() {
                    alleles[i] = v
                        .as_u64()
                        .ok_or_else(|| format!("alleles[{i}] not a u32"))?
                        as u32;
                }
                let render = match args.get("render") {
                    Some(r) if !r.is_null() => {
                        let slot = r
                            .get("slot")
                            .and_then(Json::as_u64)
                            .ok_or_else(|| "render.slot missing".to_string())?
                            as u32;
                        let mode_s = r
                            .get("mode")
                            .and_then(Json::as_str)
                            .ok_or_else(|| "render.mode missing".to_string())?;
                        let mode = match mode_s {
                            "add" => RenderMode::Add,
                            "mul" => RenderMode::Mul,
                            "set" => RenderMode::Set,
                            other => return Err(format!("unknown render mode: {other}")),
                        };
                        Some(RenderMapping { slot, mode })
                    }
                    _ => None,
                };
                let g = ExtGene {
                    name,
                    mutation_rate,
                    scale,
                    alleles,
                    render,
                };
                match genes::set_ext_gene(ext_idx, g) {
                    Some(_) => Ok(json!({"set": true, "ext_idx": ext_idx})),
                    None => Err(format!("ext_idx {ext_idx} out of range")),
                }
            },
        ),
        OpDef::new(
            "genes.ext.find",
            "Look up an extended gene by name. Returns ext_idx + record, or null.",
            "{name: string}",
            |args| {
                let name = args_string(args, "name")?;
                match genes::find_ext_gene_by_name(&name) {
                    Some((i, g)) => Ok(json!({
                        "ext_idx": i,
                        "total_idx": 240 + i,
                        "gene": serde_json::to_value(g).unwrap_or(json!(null)),
                    })),
                    None => Ok(json!(null)),
                }
            },
        ),
        OpDef::new(
            "genes.ext.dump",
            "Full dump of every extended-gene buffer (genes + pop weights + horse-id list). \
Heavy. Use for debugging.",
            "",
            |_| Ok(serde_json::to_value(genes::snapshot()).unwrap_or(json!(null))),
        ),

        // Per-pop extended weights.
        OpDef::new(
            "pop.ext.weights.get",
            "Read a pop's extended-gene weight matrix. Auto-initializes to defaults if unseen.",
            "{pop_id: u32}",
            |args| {
                let pop_id = args_u32(args, "pop_id")?;
                let w = genes::get_or_init_pop_weights(pop_id);
                Ok(serde_json::to_value(w).unwrap_or(json!(null)))
            },
        ),
        OpDef::new(
            "pop.ext.weight.set",
            "Set one allele weight for a pop's extended gene. Inverse weights: lower = more likely.",
            "{pop_id: u32, ext_gene_idx: usize, allele_idx: usize, weight: u32}",
            |args| {
                let pop_id = args_u32(args, "pop_id")?;
                let ext_gene_idx = args_usize(args, "ext_gene_idx")?;
                let allele_idx = args_usize(args, "allele_idx")?;
                let weight = args_u32(args, "weight")?;
                genes::set_pop_ext_weight(pop_id, ext_gene_idx, allele_idx, weight)?;
                Ok(json!({"set": true}))
            },
        ),

        // Per-horse extended genomes.
        OpDef::new(
            "horse.ext.genome.get",
            "Read a horse's extended diploid genome. Returns null if not yet stored.",
            "{horse_id: u64}",
            |args| {
                let horse_id = args
                    .get("horse_id")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: horse_id".to_string())?;
                match genes::get_horse_ext_genome(horse_id) {
                    Some(g) => Ok(serde_json::to_value(g).unwrap_or(json!(null))),
                    None => Ok(json!(null)),
                }
            },
        ),
        OpDef::new(
            "horse.ext.default_alleles.set",
            "Set one diploid allele pair on the DEFAULT extended genome -- the \
fallback every horse without its own entry inherits. Use this to give every \
horse the same extended-gene effect during early bring-up before D3 per-horse \
spawn hooks are wired.",
            "{ext_gene_idx: usize, maternal: u8, paternal: u8}",
            |args| {
                let ext_gene_idx = args_usize(args, "ext_gene_idx")?;
                let maternal = args_u32(args, "maternal")? as u8;
                let paternal = args_u32(args, "paternal")? as u8;
                genes::set_default_ext_alleles(ext_gene_idx, maternal, paternal)?;
                Ok(json!({"set": true}))
            },
        ),
        OpDef::new(
            "horse.ext.default_alleles.get",
            "Read the default extended genome (the fallback every horse inherits). \
Returns null if no default is set.",
            "",
            |_| match genes::get_default_ext_genome() {
                Some(g) => Ok(json!({"alleles": g.alleles})),
                None => Ok(json!(null)),
            },
        ),
        OpDef::new(
            "horse.ext.default_alleles.clear",
            "Clear the default extended genome. Horses with no specific entry \
contribute nothing to render after this.",
            "",
            |_| {
                genes::clear_default_ext_genome();
                Ok(json!({"cleared": true}))
            },
        ),
        OpDef::new(
            "horse.ext.alleles.set",
            "Set one diploid allele pair for one extended gene of one horse. \
Auto-creates the horse's extended genome if not present.",
            "{horse_id: u64, ext_gene_idx: usize, maternal: u8, paternal: u8}",
            |args| {
                let horse_id = args
                    .get("horse_id")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: horse_id".to_string())?;
                let ext_gene_idx = args_usize(args, "ext_gene_idx")?;
                let maternal = args_u32(args, "maternal")? as u8;
                let paternal = args_u32(args, "paternal")? as u8;
                genes::set_horse_ext_alleles(horse_id, ext_gene_idx, maternal, paternal)?;
                Ok(json!({"set": true}))
            },
        ),
        OpDef::new(
            "horse.ext.alleles.get",
            "Read one diploid allele pair for one extended gene of one horse. \
Returns {maternal, paternal} (both in [0..3]). Errors if the horse has no ext \
genome entry.",
            "{horse_id: u64, ext_gene_idx: usize}",
            |args| {
                let horse_id = args
                    .get("horse_id")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: horse_id".to_string())?;
                let ext_gene_idx = args_usize(args, "ext_gene_idx")?;
                match genes::get_horse_ext_alleles(horse_id, ext_gene_idx) {
                    Some((m, p)) => Ok(json!({"maternal": m, "paternal": p})),
                    None => Err(format!(
                        "no ext genome for horse_id 0x{horse_id:x} (or gene {ext_gene_idx} out of range)"
                    )),
                }
            },
        ),
        OpDef::new(
            "horse.ext.genome.set",
            "Set a horse's full extended genome. `alleles` must be length 2*EXT_GENE_COUNT.",
            "{horse_id: u64, alleles: [u8]}",
            |args| {
                let horse_id = args
                    .get("horse_id")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: horse_id".to_string())?;
                let alleles_v = args
                    .get("alleles")
                    .and_then(Json::as_array)
                    .ok_or_else(|| "missing or non-array arg: alleles".to_string())?;
                let want = 2 * EXT_GENE_COUNT;
                if alleles_v.len() != want {
                    return Err(format!("alleles must be length {want}, got {}", alleles_v.len()));
                }
                let mut alleles = Vec::with_capacity(want);
                for (i, v) in alleles_v.iter().enumerate() {
                    alleles.push(
                        v.as_u64()
                            .ok_or_else(|| format!("alleles[{i}] not a u8"))?
                            as u8 & 0x3,
                    );
                }
                genes::set_horse_ext_genome(horse_id, ExtHorseGenome { alleles });
                Ok(json!({"set": true}))
            },
        ),
        OpDef::new(
            "horse.ext.genome.drop",
            "Drop a horse's extended genome (call from D3.2 detour after vanilla free).",
            "{horse_id: u64}",
            |args| {
                let horse_id = args
                    .get("horse_id")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: horse_id".to_string())?;
                let was_present = genes::drop_horse_ext_genome(horse_id);
                Ok(json!({"dropped": was_present}))
            },
        ),
        OpDef::new(
            "horse.ext.evaluate",
            "Evaluate one extended gene's diploid blend for one horse. \
Mirrors FUN_1400a5d20's math against our sidecar buffers.",
            "{horse_id: u64, ext_gene_idx: usize}",
            |args| {
                let horse_id = args
                    .get("horse_id")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing or non-u64 arg: horse_id".to_string())?;
                let ext_gene_idx = args_usize(args, "ext_gene_idx")?;
                let value = genes::evaluate_ext_gene(horse_id, ext_gene_idx);
                Ok(json!({"value": value}))
            },
        ),
        OpDef::new(
            "genes.ext.dryrun",
            "Walk each D1 detour target and return its runtime address + first 16 bytes. \
Read-only; never mutates game memory. Use to verify target addresses look correct \
before calling `genes.ext.arm`.",
            "",
            |_| {
                let reports = patches::ext_genes::dryrun();
                let json_reports: Vec<_> = reports
                    .iter()
                    .map(|r| {
                        json!({
                            "name": r.name,
                            "rva": format!("0x{:x}", r.rva),
                            "runtime_addr": format!("0x{:x}", r.runtime_addr),
                            "prologue_bytes": r.prologue_bytes
                                .iter()
                                .map(|b| format!("{b:02x}"))
                                .collect::<Vec<_>>()
                                .join(" "),
                        })
                    })
                    .collect();
                Ok(json!({
                    "armed": patches::ext_genes::is_armed(),
                    "targets": json_reports,
                }))
            },
        ),
        OpDef::new(
            "genes.ext.arm",
            "Install the DI-A v1 detour set (currently: EVAL_DIPLOID_BLEND_A). \
After this returns OK, every call to FUN_1400a5d20 routes through our handler: \
idx<240 falls through to vanilla via the saved trampoline; idx>=240 returns \
`genes::evaluate_ext_gene` against our sidecar tables. \
Use `genes.ext.dryrun` first to validate addresses, `genes.ext.stats` after to \
confirm the detour is firing.",
            "",
            |_| match patches::ext_genes::arm() {
                Ok(()) => Ok(json!({"armed": true})),
                Err(e) => Err(e.to_string()),
            },
        ),
        OpDef::new(
            "genes.ext.stats",
            "DI-A detour invocation counters across all wired handlers \
(EVAL_DIPLOID_BLEND_A, EVAL_DIPLOID_BLEND_B, GENE_ALLELE_SWAP). \
Use for end-to-end verification: after arming, walk the game; \
`call_count` should climb (detours fire); `ext_call_count` stays at 0 \
until a caller passes idx>=240 (i.e. once the render trampoline drives \
extended evaluations).",
            "",
            |_| {
                let s = patches::ext_genes::stats();
                Ok(json!({
                    "armed": patches::ext_genes::is_armed(),
                    "call_count": s.call_count,
                    "ext_call_count": s.ext_call_count,
                    "max_idx_seen": s.max_idx_seen,
                    "per_target": {
                        "eval_a": {
                            "calls": s.call_count_a,
                            "ext_path": s.ext_count_a,
                            "last_idx": s.last_idx_a,
                        },
                        "eval_b": {
                            "calls": s.call_count_b,
                            "ext_path": s.ext_count_b,
                            "last_idx": s.last_idx_b,
                        },
                        "allele_swap": {
                            "calls": s.call_count_swap,
                            "ext_path": s.ext_count_swap,
                            "last_idx": s.last_idx_swap,
                        },
                    },
                }))
            },
        ),
        OpDef::new(
            "genes.ext.disarm",
            "Revert the DI-A detours. Returns to vanilla behavior for the 3 \
wired functions. No-op if not armed. Also runs automatically on DLL detach.",
            "",
            |_| {
                patches::ext_genes::revert();
                Ok(json!({"armed": patches::ext_genes::is_armed()}))
            },
        ),
        OpDef::new(
            "genes.ext.render.dryrun",
            "Read-only address + prologue dump for the D5 post-hook target \
(APPLY_GENE_TO_HORSE / FUN_14009f680). Use to verify the address is plausible \
before calling `genes.ext.render.arm`.",
            "",
            |_| {
                let r = patches::render_trampoline::dryrun();
                Ok(json!({
                    "armed": patches::render_trampoline::is_armed(),
                    "name": r.name,
                    "rva": format!("0x{:x}", r.rva),
                    "runtime_addr": format!("0x{:x}", r.runtime_addr),
                    "prologue_bytes": r.prologue_bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                }))
            },
        ),
        OpDef::new(
            "genes.ext.render.arm",
            "Install the D5 post-hook on FUN_14009f680. After this returns OK, every \
call to the gene-effect engine runs vanilla first, then our handler walks \
EXT_GENE_TABLE applying render mappings to the buf. Visual extended-gene effects \
become live for every horse with an extended genome.",
            "",
            |_| match patches::render_trampoline::arm() {
                Ok(()) => Ok(json!({"armed": true})),
                Err(e) => Err(e.to_string()),
            },
        ),
        OpDef::new(
            "genes.ext.render.disarm",
            "Revert the D5 post-hook. Visuals return to pure vanilla. No-op if not \
armed. Runs automatically on DLL detach.",
            "",
            |_| {
                patches::render_trampoline::revert();
                Ok(json!({"armed": patches::render_trampoline::is_armed()}))
            },
        ),
        OpDef::new(
            "genes.ext.render.stats",
            "D5 post-hook invocation counters. `call_count` = total invocations of \
FUN_14009f680 since arming; `genes_applied_total` = sum of extended-gene render \
applications across all calls (0 until a horse with an extended genome is \
evaluated).",
            "",
            |_| {
                let s = patches::render_trampoline::stats();
                Ok(json!({
                    "armed": patches::render_trampoline::is_armed(),
                    "call_count": s.call_count,
                    "genes_applied_total": s.genes_applied_total,
                }))
            },
        ),
        OpDef::new(
            "genes.ext.combinator.dryrun",
            "Read-only address + prologue dump for the D3.4 post-hook target \
(GENE_COMBINATOR / FUN_1400a2d80). Use to verify the address is plausible before \
calling `genes.ext.combinator.arm`.",
            "",
            |_| {
                let r = patches::combinator::dryrun();
                Ok(json!({
                    "armed": patches::combinator::is_armed(),
                    "name": r.name,
                    "rva": format!("0x{:x}", r.rva),
                    "runtime_addr": format!("0x{:x}", r.runtime_addr),
                    "prologue_bytes": r.prologue_bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                }))
            },
        ),
        OpDef::new(
            "genes.ext.combinator.arm",
            "Install the D3.4 post-hook on FUN_1400a2d80. After this returns OK, every \
breeding event (child born from two parents) runs vanilla's Mendelian on the inline \
genome at horse+0x2b8, then our handler runs an independent Mendelian on \
EXT_HORSE_GENOMES so the child inherits extended alleles too. Without this, bred \
horses get default-zero ext alleles.",
            "",
            |_| match patches::combinator::arm() {
                Ok(()) => Ok(json!({"armed": true})),
                Err(e) => Err(e.to_string()),
            },
        ),
        OpDef::new(
            "genes.ext.combinator.disarm",
            "Revert the D3.4 post-hook. Bred horses stop inheriting ext alleles. No-op \
if not armed. Runs automatically on DLL detach.",
            "",
            |_| {
                patches::combinator::revert();
                Ok(json!({"armed": patches::combinator::is_armed()}))
            },
        ),
        OpDef::new(
            "genes.ext.combinator.stats",
            "D3.4 combinator post-hook counters. `call_count` = total invocations of \
FUN_1400a2d80 since arming; `combines_done` = ext combinations actually executed \
(equal to call_count once vanilla returns cleanly).",
            "",
            |_| {
                let s = patches::combinator::stats();
                Ok(json!({
                    "armed": patches::combinator::is_armed(),
                    "call_count": s.call_count,
                    "combines_done": s.combines_done,
                    "vanilla_crashes": s.vanilla_crashes,
                }))
            },
        ),
        OpDef::new(
            "genes.ext.lifecycle.dryrun",
            "Read-only address + prologue dump for the D3.1/D3.2 lifecycle anchors \
(HORSE_CONSTRUCTOR / FUN_1400aac60 and HORSE_DESTRUCTOR / FUN_1400bf1f0).",
            "",
            |_| {
                let c = patches::lifecycle::dryrun_ctor();
                let d = patches::lifecycle::dryrun_dtor();
                let fmt = |r: patches::lifecycle::TargetReport| {
                    json!({
                        "name": r.name,
                        "rva": format!("0x{:x}", r.rva),
                        "runtime_addr": format!("0x{:x}", r.runtime_addr),
                        "prologue_bytes": r.prologue_bytes
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                    })
                };
                Ok(json!({
                    "armed_ctor": patches::lifecycle::is_armed_ctor(),
                    "armed_dtor": patches::lifecycle::is_armed_dtor(),
                    "ctor": fmt(c),
                    "dtor": fmt(d),
                }))
            },
        ),
        OpDef::new(
            "genes.ext.lifecycle.arm",
            "Install the D3.1/D3.2 horse-struct lifecycle anchors. After this returns OK, \
every horse creation triggers a default-zero EXT_HORSE_GENOMES entry, and every horse \
destruction drops the entry. Without this, ext entries leak across game sessions and \
new horses won't have ext alleles unless explicitly set.",
            "",
            |_| match patches::lifecycle::arm() {
                Ok(()) => Ok(json!({"armed": true})),
                Err(e) => Err(e.to_string()),
            },
        ),
        OpDef::new(
            "genes.ext.lifecycle.disarm",
            "Revert the D3.1/D3.2 lifecycle anchors. EXT_HORSE_GENOMES no longer mirrors \
horse births/deaths automatically. No-op if not armed. Runs automatically on DLL detach.",
            "",
            |_| {
                patches::lifecycle::revert();
                Ok(json!({
                    "armed_ctor": patches::lifecycle::is_armed_ctor(),
                    "armed_dtor": patches::lifecycle::is_armed_dtor(),
                }))
            },
        ),
        OpDef::new(
            "genes.ext.save.dryrun",
            "Read-only address + prologue dump for the D4 save-sidecar hooks \
(SAVE_WRITER, LOAD_GAME, HORSE_SAVE_WRITER, HORSE_SAVE_LOADER).",
            "",
            |_| {
                let r = patches::save_sidecar::dryrun_all();
                let fmt = |x: &patches::save_sidecar::TargetReport| {
                    json!({
                        "name": x.name,
                        "rva": format!("0x{:x}", x.rva),
                        "runtime_addr": format!("0x{:x}", x.runtime_addr),
                        "prologue_bytes": x.prologue_bytes
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                    })
                };
                Ok(json!({
                    "armed": patches::save_sidecar::is_armed(),
                    "targets": [fmt(&r[0]), fmt(&r[1]), fmt(&r[2]), fmt(&r[3])],
                }))
            },
        ),
        OpDef::new(
            "genes.ext.save.arm",
            "Install the D4 save-sidecar hooks. After this returns OK, saves write \
a `save<N>.dat.ext` sidecar containing dense ext alleles for every horse in roster \
order. Loads consume it before vanilla regenerates render fields, so ext effects \
survive save/load.",
            "",
            |_| match patches::save_sidecar::arm() {
                Ok(()) => Ok(json!({"armed": true})),
                Err(e) => Err(e.to_string()),
            },
        ),
        OpDef::new(
            "genes.ext.save.disarm",
            "Revert the D4 save-sidecar hooks. Future saves/loads ignore ext alleles. \
Existing sidecar files are left in place. No-op if not armed. Runs automatically on \
DLL detach.",
            "",
            |_| {
                patches::save_sidecar::revert();
                Ok(json!({"armed": patches::save_sidecar::is_armed()}))
            },
        ),
        OpDef::new(
            "patterns.scan",
            "Scan the loaded Horsey.exe .text section for an IDA-style byte pattern. \
Returns the runtime address of the first match (or null if no match). Used by \
test harness + future R1/R2 address-resolution code. Body: {sig: string}, where \
sig is e.g. \"48 89 5c 24 ?? 57 48 83 ec\". Read-only.",
            "{sig: string}",
            |args| {
                let sig = args.get("sig").and_then(Json::as_str).unwrap_or("");
                if sig.is_empty() {
                    return Err("missing or empty sig".into());
                }
                let found = modforge::patterns::scan_loaded_image(sig);
                Ok(json!({
                    "sig": sig,
                    "found": found.map(|a| format!("0x{a:x}")),
                }))
            },
        ),
        OpDef::new(
            "game.build_info",
            "Return identification for the loaded Horsey.exe build: SHA-256 of the \
.text section, image base, text size, and (when available) the on-disk \
mtime + size of the .exe file. Stable per-build; changes only when Steam ships a new \
binary. Used by tests + bug reports to pin which build was tested.",
            "",
            |_| {
                use crate::targets;
                let sha = targets::image_text_sha256();
                let base = targets::image_base();
                let text_size = targets::find_text_section_size().unwrap_or(0);
                let file_info = targets::image_file_info();
                let mut out = json!({
                    "image_sha256": sha,
                    "image_base": format!("0x{base:x}"),
                    "text_size": text_size,
                });
                if let Some((path, mtime, size)) = file_info {
                    let mtime_secs = mtime
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    out.as_object_mut().unwrap().insert(
                        "exe_path".into(),
                        json!(path.display().to_string()),
                    );
                    out.as_object_mut()
                        .unwrap()
                        .insert("exe_mtime_unix".into(), json!(mtime_secs));
                    out.as_object_mut()
                        .unwrap()
                        .insert("exe_size".into(), json!(size));
                }
                Ok(out)
            },
        ),
        OpDef::new(
            "patterns.sleuth.resolve",
            "Multi-target patternsleuth resolver. Body: \
{patterns: [{name, sigs:[...]}, ...]}. Each target supplies one or more IDA-style \
signatures; first match wins per target. Returns {results: {name: \"0x...\" | null}}. \
Used by horsey-mod tests + future R2 attach-time address resolution.",
            "{patterns: [{name: string, sigs: [string]}]}",
            |args| {
                use modforge::patterns::sleuth;
                let arr = args
                    .get("patterns")
                    .and_then(Json::as_array)
                    .ok_or_else(|| "missing patterns array".to_string())?;

                // Collect owned strings first so the borrows live long
                // enough; build Target<'a> views over them.
                let mut owned: Vec<(String, Vec<String>)> =
                    Vec::with_capacity(arr.len());
                for entry in arr {
                    let name = entry
                        .get("name")
                        .and_then(Json::as_str)
                        .ok_or_else(|| "pattern entry missing `name`".to_string())?
                        .to_string();
                    let sigs = entry
                        .get("sigs")
                        .and_then(Json::as_array)
                        .ok_or_else(|| {
                            format!("pattern {name:?} missing `sigs` array")
                        })?;
                    let sig_strs: Vec<String> = sigs
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect();
                    if sig_strs.is_empty() {
                        return Err(format!("pattern {name:?} has no sigs"));
                    }
                    owned.push((name, sig_strs));
                }

                // Borrow into Target<'a>.
                let sigs_refs: Vec<Vec<&str>> = owned
                    .iter()
                    .map(|(_, sigs)| sigs.iter().map(String::as_str).collect())
                    .collect();
                let targets: Vec<sleuth::Target> = owned
                    .iter()
                    .zip(sigs_refs.iter())
                    .map(|((n, _), s)| sleuth::Target { name: n, sigs: s })
                    .collect();

                let resolution = sleuth::resolve_all(&targets)
                    .map_err(|e| e.to_string())?;

                // Emit as {name: "0x..."} or null.
                let mut results = serde_json::Map::new();
                for (name, _) in &owned {
                    let v = match resolution.get(name) {
                        Some(addr) => Json::String(format!("0x{addr:x}")),
                        None => Json::Null,
                    };
                    results.insert(name.clone(), v);
                }
                Ok(json!({ "results": Json::Object(results) }))
            },
        ),
        OpDef::new(
            "patterns.sleuth.scan_all",
            "Scan `.text` for every match of one IDA-style signature; for each match \
decode the disp32 at the given offset to a runtime target address. Returns \
{hits: [{instr_addr, decoded_target, context_hex}, ...]}. Use to discover \
where a hardcoded global has drifted to: scan for a UNIQUE instruction that \
references the global, then read the decoded target address.\n\
Body: {sig: string, disp32_offset: u32, instr_len: u32, context_bytes: u32 (default 16), max_hits: u32 (default 256)}.",
            "{sig: string, disp32_offset: u32, instr_len: u32}",
            |args| {
                use modforge::patterns::sleuth;
                let sig = args
                    .get("sig")
                    .and_then(Json::as_str)
                    .ok_or_else(|| "missing sig".to_string())?
                    .to_string();
                let disp_off = args
                    .get("disp32_offset")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing disp32_offset".to_string())?
                    as usize;
                let instr_len = args
                    .get("instr_len")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing instr_len".to_string())?
                    as usize;
                let context = args
                    .get("context_bytes")
                    .and_then(Json::as_u64)
                    .unwrap_or(16) as usize;
                let max_hits = args
                    .get("max_hits")
                    .and_then(Json::as_u64)
                    .unwrap_or(256) as usize;
                if disp_off + 4 > instr_len {
                    return Err(format!(
                        "disp32_offset+4 ({}) > instr_len ({instr_len})",
                        disp_off + 4
                    ));
                }
                let mut addrs = sleuth::scan_all_matches(&sig).map_err(|e| e.to_string())?;
                addrs.truncate(max_hits);
                let entries: Vec<Json> = addrs.into_iter().map(|instr_addr| {
                    // SAFETY: instr_addr is inside `.text`, mapped.
                    let disp_ptr = (instr_addr + disp_off) as *const i32;
                    let disp32 = unsafe { disp_ptr.read_unaligned() } as isize;
                    let next_ip = instr_addr.wrapping_add(instr_len);
                    let target = next_ip.wrapping_add(disp32 as usize);
                    let ctx_len = context.max(instr_len);
                    // SAFETY: ctx_len bytes starting at instr_addr
                    // are inside `.text` for any reasonable context.
                    let ctx: Vec<u8> = unsafe {
                        std::slice::from_raw_parts(instr_addr as *const u8, ctx_len)
                            .to_vec()
                    };
                    let context_hex = ctx
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    json!({
                        "instr_addr": format!("0x{instr_addr:x}"),
                        "disp32": format!("{disp32:#x}"),
                        "decoded_target": format!("0x{target:x}"),
                        "context_hex": context_hex,
                    })
                }).collect();
                Ok(json!({
                    "sig": sig,
                    "hits": entries,
                }))
            },
        ),
        OpDef::new(
            "patterns.read_bytes",
            "Read `n` bytes at a runtime address inside the loaded Horsey.exe image, \
return as a hex-byte string suitable for direct use as a `patterns.scan` sig. Used \
to derive test signatures from known-good function entries. Body: \
{addr: \"0x...\", n: u32}.",
            "{addr: string, n: u32}",
            |args| {
                let addr_str =
                    args.get("addr").and_then(Json::as_str).unwrap_or("");
                let n = args.get("n").and_then(Json::as_u64).unwrap_or(16) as usize;
                let addr_str = addr_str.trim_start_matches("0x");
                let addr = usize::from_str_radix(addr_str, 16)
                    .map_err(|e| format!("bad addr: {e}"))?;
                if n == 0 || n > 4096 {
                    return Err(format!("n out of range: {n}"));
                }
                // Guard with VirtualQuery so a bad address returns
                // a typed error instead of faulting and tearing down
                // the HTTP worker. Check both ends of the range.
                if !modforge::winproc::is_addr_readable(addr)
                    || !modforge::winproc::is_addr_readable(addr + n - 1)
                {
                    return Err(format!(
                        "addr 0x{addr:x} + {n} bytes not readable (page noaccess / unmapped)"
                    ));
                }
                // SAFETY: VirtualQuery confirmed both endpoints lie in
                // committed, non-guard, non-NOACCESS pages.
                let view = unsafe {
                    std::slice::from_raw_parts(addr as *const u8, n)
                };
                let hex: String = view
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(json!({ "addr": format!("0x{addr:x}"), "n": n, "bytes": hex }))
            },
        ),

        // HK1 Stage S0 probe: walk GS+0x438 -> *(arr + active_scene_id*8)
        // to find the active scene's controller (the Location object the
        // click-drag handler runs against). Returns the location ptr, the
        // vtable RVA at +0x0, the horse-vec count via +0x130/+0x138 and
        // the click-drag fields (LOC[0x26], LOC[0x2d], LOC[0x2e]) decoded
        // from raw bytes. With active_scene_id == -1 (overworld) the
        // walk reports the no-location state and dumps slot 0x00 instead.
        OpDef::new(
            "hk1.probe.active_location",
            "Walk GS+0x438 -> *(arr + asid*8); dump location_ptr, vtable rva, click-drag fields, 0x240 bytes raw.",
            "",
            |_| {
                use modforge::winproc::is_addr_readable;
                let gs = gamestate::ptr();
                let asid = gamestate::active_scene_id();
                let image_base = crate::targets::image_base();
                if gs == 0 {
                    return Ok(json!({ "error": "no gamestate" }));
                }
                let arr_slot = gs + 0x438;
                if !is_addr_readable(arr_slot) {
                    return Ok(json!({ "error": "GS+0x438 unreadable", "gs_ptr": format!("0x{gs:x}") }));
                }
                // SAFETY: arr_slot just checked.
                let arr_ptr = unsafe { *(arr_slot as *const usize) };
                // When asid == -1 (overworld), probe slot 0x00 (owned).
                let slot_off: usize = match asid {
                    Some(id) if id >= 0 && id < 256 => (id as usize) * 8,
                    _ => 0,
                };
                let slot_addr = arr_ptr.wrapping_add(slot_off);
                if !is_addr_readable(slot_addr) {
                    return Ok(json!({
                        "error": "slot unreadable",
                        "gs_ptr": format!("0x{gs:x}"),
                        "arr_ptr": format!("0x{arr_ptr:x}"),
                        "active_scene_id": asid,
                        "slot_off": format!("0x{slot_off:x}"),
                    }));
                }
                // SAFETY: just checked.
                let loc_ptr = unsafe { *(slot_addr as *const usize) };
                if loc_ptr == 0 || !is_addr_readable(loc_ptr + 0x240) {
                    return Ok(json!({
                        "error": "loc_ptr null or short",
                        "gs_ptr": format!("0x{gs:x}"),
                        "arr_ptr": format!("0x{arr_ptr:x}"),
                        "active_scene_id": asid,
                        "slot_off": format!("0x{slot_off:x}"),
                        "loc_ptr": format!("0x{loc_ptr:x}"),
                    }));
                }
                // SAFETY: loc_ptr..loc_ptr+0x240 readability checked.
                let vtable_ptr = unsafe { *(loc_ptr as *const usize) };
                let vtable_rva = vtable_ptr.saturating_sub(image_base);
                let vec_begin = unsafe { *((loc_ptr + 0x130) as *const usize) };
                let vec_end = unsafe { *((loc_ptr + 0x138) as *const usize) };
                let horse_count = if vec_end >= vec_begin {
                    let span = vec_end - vec_begin;
                    if span <= 0x10000 && span % 8 == 0 { Some(span / 8) } else { None }
                } else { None };
                // LOC[0x26 .. 0x2f] are qword-indexed in the decomp;
                // byte offset = idx * 8.
                let drag_idx = unsafe { *((loc_ptr + 0x2d * 8) as *const i32) };
                let cand_idx = unsafe { *((loc_ptr + 0x2e * 8) as *const i32) };
                let armed    = unsafe { *((loc_ptr + 0x2c * 8) as *const u8) };
                // Dump 0x240 bytes starting at loc_ptr for offline inspection.
                let bytes = unsafe {
                    std::slice::from_raw_parts(loc_ptr as *const u8, 0x240)
                };
                let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
                Ok(json!({
                    "gs_ptr": format!("0x{gs:x}"),
                    "arr_ptr": format!("0x{arr_ptr:x}"),
                    "active_scene_id": asid,
                    "slot_off": format!("0x{slot_off:x}"),
                    "loc_ptr": format!("0x{loc_ptr:x}"),
                    "vtable_ptr": format!("0x{vtable_ptr:x}"),
                    "vtable_rva": format!("0x{vtable_rva:x}"),
                    "image_base": format!("0x{image_base:x}"),
                    "loc_horse_vec_begin": format!("0x{vec_begin:x}"),
                    "loc_horse_vec_end": format!("0x{vec_end:x}"),
                    "loc_horse_count": horse_count,
                    "loc_drag_idx": drag_idx,
                    "loc_cand_idx": cand_idx,
                    "loc_armed": armed,
                    "bytes_240": hex,
                }))
            },
        ),

        // HK1 Stage S0 probe: for every slot found by scan_438_slots,
        // also report the slot occupant's vtable RVA so we can classify
        // which slots are Location objects vs subsystem singletons.
        OpDef::new(
            "hk1.probe.scene_slot_vtables",
            "Walk all GS+0x438 slots with a horse-vec; report each slot's vtable RVA + count.",
            "",
            |_| {
                use modforge::winproc::is_addr_readable;
                let gs = gamestate::ptr();
                let image_base = crate::targets::image_base();
                if gs == 0 { return Ok(json!({ "error": "no gamestate" })); }
                let arr_slot = gs + 0x438;
                if !is_addr_readable(arr_slot) {
                    return Ok(json!({ "error": "GS+0x438 unreadable" }));
                }
                // SAFETY: arr_slot just checked.
                let arr_ptr = unsafe { *(arr_slot as *const usize) };
                let mut slots = Vec::new();
                let mut slot_off = 0usize;
                while slot_off < 0x200 {
                    let addr = arr_ptr.wrapping_add(slot_off);
                    if !is_addr_readable(addr) {
                        slot_off += 8; continue;
                    }
                    // SAFETY: addr just checked.
                    let sub_ptr = unsafe { *(addr as *const usize) };
                    if sub_ptr != 0
                        && is_addr_readable(sub_ptr)
                        && is_addr_readable(sub_ptr + 0x138)
                    {
                        // SAFETY: readability of sub_ptr and +0x138 checked above.
                        let vtable_ptr = unsafe { *(sub_ptr as *const usize) };
                        let vtable_rva = vtable_ptr.saturating_sub(image_base);
                        let begin = unsafe { *((sub_ptr + 0x130) as *const usize) };
                        let end = unsafe { *((sub_ptr + 0x138) as *const usize) };
                        let count: Option<usize> = if end >= begin {
                            let span = end - begin;
                            if span <= 0x10000 && span % 8 == 0 { Some(span / 8) } else { None }
                        } else { None };
                        slots.push(json!({
                            "slot": format!("0x{slot_off:x}"),
                            "sub_ptr": format!("0x{sub_ptr:x}"),
                            "vtable_rva": format!("0x{vtable_rva:x}"),
                            "count": count,
                        }));
                    }
                    slot_off += 8;
                }
                Ok(json!({
                    "image_base": format!("0x{image_base:x}"),
                    "arr_ptr": format!("0x{arr_ptr:x}"),
                    "slots": slots,
                }))
            },
        ),

        // HK1 Stage S0 probe: given a horse pointer, walk every
        // GS+0x438 slot's +0x130/+0x138 vector and report which slots
        // contain it (with the index). Used to identify the truck /
        // paddock / race-line slot by moving a known horse into that
        // location in-game and seeing where it shows up.
        OpDef::new(
            "hk1.probe.locate_horse",
            "Find which scene-table slot(s) hold the given Horse*. Body: {horse_ptr: '0x...'}",
            "{horse_ptr: string}",
            |args| {
                use modforge::winproc::is_addr_readable;
                let s = args.get("horse_ptr").and_then(Json::as_str)
                    .ok_or_else(|| "missing horse_ptr".to_string())?;
                let s = s.trim_start_matches("0x");
                let want = usize::from_str_radix(s, 16)
                    .map_err(|e| format!("bad horse_ptr: {e}"))?;
                let gs = gamestate::ptr();
                let image_base = crate::targets::image_base();
                if gs == 0 { return Ok(json!({ "error": "no gamestate" })); }
                let arr_slot = gs + 0x438;
                if !is_addr_readable(arr_slot) {
                    return Ok(json!({ "error": "GS+0x438 unreadable" }));
                }
                // SAFETY: just checked.
                let arr_ptr = unsafe { *(arr_slot as *const usize) };
                let mut hits = Vec::new();
                let mut slot_off = 0usize;
                while slot_off < 0x200 {
                    let addr = arr_ptr.wrapping_add(slot_off);
                    if !is_addr_readable(addr) { slot_off += 8; continue; }
                    // SAFETY: just checked.
                    let sub_ptr = unsafe { *(addr as *const usize) };
                    if sub_ptr == 0 || !is_addr_readable(sub_ptr + 0x138) {
                        slot_off += 8; continue;
                    }
                    // SAFETY: readability checked.
                    let vtable_ptr = unsafe { *(sub_ptr as *const usize) };
                    let begin = unsafe { *((sub_ptr + 0x130) as *const usize) };
                    let end = unsafe { *((sub_ptr + 0x138) as *const usize) };
                    if end < begin || end - begin > 0x10000 || (end - begin) % 8 != 0 {
                        slot_off += 8; continue;
                    }
                    let count = (end - begin) / 8;
                    if !is_addr_readable(begin) || (count > 0 && !is_addr_readable(end - 8)) {
                        slot_off += 8; continue;
                    }
                    for i in 0..count {
                        // SAFETY: range checked.
                        let p = unsafe { *((begin + i * 8) as *const usize) };
                        if p == want {
                            hits.push(json!({
                                "slot": format!("0x{slot_off:x}"),
                                "sub_ptr": format!("0x{sub_ptr:x}"),
                                "vtable_rva": format!("0x{:x}", vtable_ptr.saturating_sub(image_base)),
                                "index": i,
                                "vec_count": count,
                            }));
                        }
                    }
                    slot_off += 8;
                }
                Ok(json!({
                    "horse_ptr": format!("0x{want:x}"),
                    "hits": hits,
                }))
            },
        ),

        // HK1 research: dump a configurable window of the Home Location's
        // bytes. Used by `hk1_watch_loc_during_drag` to log every LOC
        // field that changes while the user does a real drag, so we
        // can reproduce the full state-setup.
        OpDef::new(
            "hk1.loc_bytes",
            "Dump bytes from the Home Location object. Body: {offset?: u64, n?: u64}. Defaults to (0, 0x240).",
            "{offset?: u64, n?: u64}",
            |args| {
                use modforge::winproc::is_addr_readable;
                let off = args.get("offset").and_then(Json::as_u64).unwrap_or(0) as usize;
                let n = args.get("n").and_then(Json::as_u64).unwrap_or(0x240) as usize;
                if n == 0 || n > 4096 {
                    return Err(format!("n out of range: {n}"));
                }
                let loc = crate::hk1::home_loc_ptr()
                    .ok_or_else(|| "no home location".to_string())?;
                let addr = loc + off;
                if !is_addr_readable(addr) || !is_addr_readable(addr + n - 1) {
                    return Err(format!("LOC+{off:#x}..+{n} unreadable"));
                }
                // SAFETY: both endpoints checked.
                let slice = unsafe { std::slice::from_raw_parts(addr as *const u8, n) };
                let hex: String = slice.iter().map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>().join(" ");
                Ok(json!({
                    "loc_ptr": format!("0x{loc:x}"),
                    "offset": format!("0x{off:x}"),
                    "n": n,
                    "bytes": hex,
                }))
            },
        ),

        // HK1: read the cursor position the Home Location's click
        // handler watches (`LOC[0x174]/+0x178`). Floats. Used by the
        // overlay calibration UI and by automated tests.
        OpDef::new(
            "hk1.read_cursor",
            "Read the Home Location's cursor floats (LOC+0x174, LOC+0x178). Returns {x, y}.",
            "",
            |_| {
                match crate::hk1::read_cursor() {
                    Some((x, y)) => Ok(json!({ "x": x as f64, "y": y as f64 })),
                    None => Err("cursor unreadable (gamestate? home location?)".to_string()),
                }
            },
        ),

        // HK1: directly set a calibrated target (truck or pasture) to
        // arbitrary cursor floats. Lets automated tests bypass the
        // human-clicks-overlay calibration step.
        OpDef::new(
            "hk1.set_target",
            "Save (x, y) as the named calibration target. Body: {which: 'truck'|'pasture', x: f32, y: f32}",
            "{which: string, x: f32, y: f32}",
            |args| {
                let which = args.get("which").and_then(Json::as_str)
                    .ok_or_else(|| "missing which".to_string())?;
                let x = args.get("x").and_then(Json::as_f64)
                    .ok_or_else(|| "missing x".to_string())? as f32;
                let y = args.get("y").and_then(Json::as_f64)
                    .ok_or_else(|| "missing y".to_string())? as f32;
                let mut t = crate::hk1::load_targets();
                match which {
                    "truck"   => t.truck   = Some((x, y)),
                    "pasture" => t.pasture = Some((x, y)),
                    _ => return Err(format!("which must be truck|pasture, got {which}")),
                }
                crate::hk1::save_targets_pub(t);
                Ok(json!({ "which": which, "x": x, "y": y }))
            },
        ),

        // HK1: invoke the Home Location's drop-commit vtable (the
        // game's own drag-drop completion path) to transfer a horse
        // to the calibrated target. Returns the vtable call result
        // (non-zero == drop accepted).
        OpDef::new(
            "hk1.transfer",
            "Transfer a horse via the game's drop-commit vtable. Body: {horse_ptr: u64, dest: 'truck'|'pasture'}",
            "{horse_ptr: u64, dest: string}",
            |args| {
                let horse_ptr = args.get("horse_ptr").and_then(Json::as_u64)
                    .ok_or_else(|| "missing horse_ptr".to_string())? as usize;
                let dest = args.get("dest").and_then(Json::as_str)
                    .ok_or_else(|| "missing dest".to_string())?;
                match crate::hk1::transfer_horse(horse_ptr, dest) {
                    Some(r) => Ok(json!({ "result": r as u64, "dest": dest })),
                    None => Err("transfer setup failed (no calibration? no gamestate?)".to_string()),
                }
            },
        ),

        // HK1 Stage S0 probe: read mouse-screen coordinates at HLT's
        // documented RVAs. Returns NaN-as-null when unresolved. Used to
        // confirm the RVAs still work in our build; if drift > epsilon
        // between two calls while the tester moves the mouse, the
        // anchor is correct.
        OpDef::new(
            "hk1.probe.mouse_globals",
            "Read mouse_screen_x/y at HLT RVAs (0x3ED970, 0x3ED978). Returns floats + raw u32 bits.",
            "",
            |_| {
                use modforge::winproc::is_addr_readable;
                let base = crate::targets::image_base();
                const MOUSE_X_RVA: usize = 0x3ED970;
                const MOUSE_Y_RVA: usize = 0x3ED978;
                let x_addr = base + MOUSE_X_RVA;
                let y_addr = base + MOUSE_Y_RVA;
                let x = if is_addr_readable(x_addr) {
                    // SAFETY: just checked.
                    Some(unsafe { (x_addr as *const f32).read_unaligned() })
                } else { None };
                let y = if is_addr_readable(y_addr) {
                    // SAFETY: just checked.
                    Some(unsafe { (y_addr as *const f32).read_unaligned() })
                } else { None };
                let x_bits = x.map(|v| v.to_bits());
                let y_bits = y.map(|v| v.to_bits());
                Ok(json!({
                    "image_base": format!("0x{base:x}"),
                    "x_addr": format!("0x{x_addr:x}"),
                    "y_addr": format!("0x{y_addr:x}"),
                    "x": x.and_then(|v| if v.is_finite() { Some(v as f64) } else { None }),
                    "y": y.and_then(|v| if v.is_finite() { Some(v as f64) } else { None }),
                    "x_bits": x_bits.map(|b| format!("0x{b:08x}")),
                    "y_bits": y_bits.map(|b| format!("0x{b:08x}")),
                }))
            },
        ),

        OpDef::new(
            "genes.ext.save.stats",
            "D4 save-sidecar counters. `save_calls`/`load_calls` = top-level save/load \
invocations; `horse_writes`/`horse_reads` = per-horse records appended/consumed; \
`files_written`/`files_read` = sidecar files successfully written/read.",
            "",
            |_| {
                let s = patches::save_sidecar::stats();
                Ok(json!({
                    "armed": patches::save_sidecar::is_armed(),
                    "save_calls": s.save_calls,
                    "load_calls": s.load_calls,
                    "horse_writes": s.horse_writes,
                    "horse_reads": s.horse_reads,
                    "files_written": s.files_written,
                    "files_read": s.files_read,
                }))
            },
        ),
        OpDef::new(
            "genes.ext.save.write_now",
            "Directly write the BXSAVEXT sidecar for the given channel, bypassing the \
detour. Walks current EXT_HORSE_GENOMES + writes the file at sidecar_path(channel). \
Used by the end-to-end save/restart/reload test to skip the vanilla save trigger \
while still proving cross-launch persistence of the codec. Body: {channel: u32}.",
            "{channel: u32}",
            |args| {
                use std::io::Write as _;
                let channel = args
                    .get("channel")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing channel".to_string())? as u32;
                let path = patches::save_sidecar::sidecar_path(channel);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let genomes: Vec<(u64, genes::ExtHorseGenome)> = {
                    let ids = genes::list_horse_genome_ids();
                    ids.into_iter()
                        .filter_map(|id| {
                            genes::get_horse_ext_genome(id).map(|g| (id, g))
                        })
                        .collect()
                };
                let mut f = std::fs::File::create(&path)
                    .map_err(|e| format!("create {}: {e}", path.display()))?;
                patches::save_sidecar::write_header(
                    &mut f,
                    genes::EXT_GENE_COUNT as u32,
                    genomes.len() as u32,
                )
                .map_err(|e| format!("write_header: {e}"))?;
                for (_id, g) in &genomes {
                    patches::save_sidecar::write_horse_record(&mut f, g)
                        .map_err(|e| format!("write_horse_record: {e}"))?;
                }
                f.flush().map_err(|e| format!("flush: {e}"))?;
                Ok(json!({
                    "path": path.display().to_string(),
                    "horses": genomes.len(),
                }))
            },
        ),
        OpDef::new(
            "genes.ext.save.read_now",
            "Directly read the BXSAVEXT sidecar for the given channel into \
EXT_HORSE_GENOMES (clears existing entries first). Mirror of write_now. Body: \
{channel: u32}. Returns {path, horses} on success.",
            "{channel: u32}",
            |args| {
                let channel = args
                    .get("channel")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| "missing channel".to_string())? as u32;
                let path = patches::save_sidecar::sidecar_path(channel);
                if !path.exists() {
                    return Err(format!("sidecar missing: {}", path.display()));
                }
                let mut f = std::fs::File::open(&path)
                    .map_err(|e| format!("open {}: {e}", path.display()))?;
                let hdr = patches::save_sidecar::read_header(&mut f)
                    .map_err(|e| format!("read_header: {e}"))?;
                for i in 0..hdr.horse_count {
                    // Drop any existing entry at this index so the
                    // sidecar's values are authoritative.
                    let _ = genes::drop_horse_ext_genome(i as u64);
                }
                for i in 0..hdr.horse_count {
                    let g = patches::save_sidecar::read_horse_record(&mut f)
                        .map_err(|e| format!("read_horse_record {i}: {e}"))?;
                    // Key by sequential index; the real path keys by horse pointer
                    // through the detour. For the e2e test we use index keys so
                    // assertions stay self-consistent across launches.
                    genes::set_horse_ext_genome(i as u64, g);
                }
                Ok(json!({
                    "path": path.display().to_string(),
                    "horses": hdr.horse_count,
                }))
            },
        ),
        OpDef::new(
            "genes.ext.lifecycle.stats",
            "D3.1/D3.2 lifecycle anchor counters. `ctor_calls`/`dtor_calls` = total \
invocations of FUN_1400aac60/FUN_1400bf1f0; `entries_created`/`entries_dropped` = \
new vs removed EXT_HORSE_GENOMES entries.",
            "",
            |_| {
                let s = patches::lifecycle::stats();
                Ok(json!({
                    "armed_ctor": patches::lifecycle::is_armed_ctor(),
                    "armed_dtor": patches::lifecycle::is_armed_dtor(),
                    "ctor_calls": s.ctor_calls,
                    "dtor_calls": s.dtor_calls,
                    "entries_created": s.entries_created,
                    "entries_dropped": s.entries_dropped,
                }))
            },
        ),
        OpDef::new(
            "genes.ext.reload",
            "Re-parse `genes-extended.xml` from disk and replace EXT_GENE_TABLE entries. \
Default path: <DLL_DIR>/genes-extended.xml. Pass `path` to override.",
            "{path?: string}",
            |args| {
                let path: std::path::PathBuf = match args.get("path").and_then(Json::as_str) {
                    Some(p) => std::path::PathBuf::from(p),
                    None => modforge::log::dll_dir_wait(std::time::Duration::from_secs(1))
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("genes-extended.xml"),
                };
                if !path.exists() {
                    return Err(format!("file not found: {}", path.display()));
                }
                match genes_xml::load_from_file(&path) {
                    Ok(stats) => Ok(json!({
                        "path": path.display().to_string(),
                        "parsed": stats.parsed,
                        "placed": stats.placed,
                        "skipped_oor": stats.skipped_oor,
                        "skipped_conflict": stats.skipped_conflict,
                        "render_mappings": stats.render_mappings,
                        "errors": stats.errors,
                    })),
                    Err(e) => Err(e),
                }
            },
        ),

        // ===== In-game ImGui overlay via hudhook =====
        //
        // Hooks IDXGISwapChain::Present so our panel renders INSIDE the
        // game window. Step 1 spike: a single frame-counter panel.
        // Phase A `ui.native.*` ops retired: hudhook bundles imgui-sys,
        // cannot coexist with modforge's vendored C++ ImGui shim in one
        // DLL. See docs/IN-GAME-OVERLAY-PLAN.md.
        OpDef::new(
            "ui.overlay.arm",
            "Install the DXGI Present hook so the in-game overlay renders. Idempotent.",
            "",
            |_| match crate::overlay::arm() {
                Ok(()) => Ok(json!({"armed": true})),
                Err(e) => Err(e),
            },
        ),
        OpDef::new(
            "ui.overlay.disarm",
            "Best-effort overlay unhook. Hudhook eject walks the registered hooks.",
            "",
            |_| {
                crate::overlay::disarm();
                Ok(json!({"armed": crate::overlay::is_armed()}))
            },
        ),
        OpDef::new(
            "ui.overlay.stats",
            "Overlay state: armed flag and per-frame counter.",
            "",
            |_| Ok(json!({
                "armed": crate::overlay::is_armed(),
                "frames": crate::overlay::frame_count(),
            })),
        ),

        // ===== V5: invoke vanilla code through the registry =====

        OpDef::new(
            "horse.rebuild",
            "Invoke HORSE_REBUILD (FUN_1400b3070) on a horse_ptr via \
             modforge::vanilla::Invoker. Body: {addr: \"0x<hex>\"}. \
             Re-runs the engine's post-genome-edit equipment + physics \
             pass; use after any allele/gene poke that bypassed the \
             render trampoline. Crashes contained by SEH; returns \
             {ok, elapsed_us}.",
            "{addr: string}",
            |args| {
                let addr_s = args.get("addr").and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "missing or non-string 'addr'".to_string())?;
                let addr = u64::from_str_radix(
                    addr_s.trim_start_matches("0x").trim_start_matches("0X"), 16,
                ).map_err(|e| format!("addr parse: {e}"))?;
                let invoker = modforge::vanilla::Invoker::new(
                    &crate::targets_registry::HORSEY_RESOLVER,
                );
                let t0 = std::time::Instant::now();
                let r = invoker.call(
                    "HORSE_REBUILD",
                    &[modforge::vanilla::ArgValue::Ptr(addr)],
                );
                let elapsed_us = t0.elapsed().as_micros() as u64;
                match r {
                    Ok(_) => Ok(json!({"ok": true, "elapsed_us": elapsed_us})),
                    Err(e) => Ok(json!({
                        "ok": false,
                        "error": format!("{e}"),
                        "elapsed_us": elapsed_us,
                    })),
                }
            },
        ),

        OpDef::new(
            "rng.next_modulo",
            "Invoke RNG_NEXT_MODULO (FUN_1400c6580) via the registry. \
             Body: {modulus: u32}. Returns {ok, value: u32, elapsed_us}. \
             Generic-range RNG; replaces ad-hoc per-mod RNG.",
            "{modulus: u32}",
            |args| {
                let m = args.get("modulus").and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| "missing or non-u64 'modulus'".to_string())?
                    as u32;
                let invoker = modforge::vanilla::Invoker::new(
                    &crate::targets_registry::HORSEY_RESOLVER,
                );
                let t0 = std::time::Instant::now();
                let r = invoker.call(
                    "RNG_NEXT_MODULO",
                    &[modforge::vanilla::ArgValue::U32(m)],
                );
                let elapsed_us = t0.elapsed().as_micros() as u64;
                match r {
                    Ok(modforge::vanilla::RetValue::U32(v)) => Ok(json!({
                        "ok": true, "value": v, "elapsed_us": elapsed_us,
                    })),
                    Ok(other) => Ok(json!({
                        "ok": false,
                        "error": format!("unexpected return: {other:?}"),
                        "elapsed_us": elapsed_us,
                    })),
                    Err(e) => Ok(json!({
                        "ok": false,
                        "error": format!("{e}"),
                        "elapsed_us": elapsed_us,
                    })),
                }
            },
        ),
        OpDef::new(
            "game.exit_to_overworld",
            "Invoke EXIT_TO_OVERWORLD (FUN_1401041f0) on the GameState. \
             This is the same function the in-scene 'Map' button calls: \
             sets active_scene_id = -1, zeroes scene state, triggers \
             auto-save, plays the 'World' sound. No-op when already on \
             the overworld. Returns {ok, prev_scene_id, elapsed_us}.",
            "{}",
            |_args| {
                let prev = crate::gamestate::active_scene_id();
                let gs = crate::gamestate::ptr();
                if gs == 0 {
                    return Ok(json!({"ok": false, "error": "GameState not loaded"}));
                }
                if prev == Some(-1) {
                    return Ok(json!({
                        "ok": true, "skipped": true,
                        "prev_scene_id": -1,
                        "note": "already on overworld",
                    }));
                }
                let invoker = modforge::vanilla::Invoker::new(
                    &crate::targets_registry::HORSEY_RESOLVER,
                );
                let t0 = std::time::Instant::now();
                let r = invoker.call(
                    "EXIT_TO_OVERWORLD",
                    &[modforge::vanilla::ArgValue::Ptr(gs as u64)],
                );
                let elapsed_us = t0.elapsed().as_micros() as u64;
                match r {
                    Ok(_) => Ok(json!({
                        "ok": true,
                        "prev_scene_id": prev,
                        "new_scene_id": crate::gamestate::active_scene_id(),
                        "elapsed_us": elapsed_us,
                    })),
                    Err(e) => Ok(json!({
                        "ok": false,
                        "error": format!("{e}"),
                        "elapsed_us": elapsed_us,
                    })),
                }
            },
        ),
    ]);
    OP_REGISTRY.register_many(modforge::input::ops::all());
    modforge::log!("horsey-mod: registered ops");
}

// =============================================================================
// Argument helpers (mirroring modforge::args style)
// =============================================================================

fn args_u32(args: &Json, key: &str) -> Result<u32, String> {
    args.get(key)
        .and_then(Json::as_u64)
        .map(|v| v as u32)
        .ok_or_else(|| format!("missing or non-u32 arg: {key}"))
}

fn args_i32(args: &Json, key: &str) -> Result<i32, String> {
    args.get(key)
        .and_then(Json::as_i64)
        .map(|v| v as i32)
        .ok_or_else(|| format!("missing or non-i32 arg: {key}"))
}

fn args_usize(args: &Json, key: &str) -> Result<usize, String> {
    args.get(key)
        .and_then(Json::as_u64)
        .map(|v| v as usize)
        .ok_or_else(|| format!("missing or non-usize arg: {key}"))
}

fn args_bool(args: &Json, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Json::as_bool)
        .ok_or_else(|| format!("missing or non-bool arg: {key}"))
}

fn args_string(args: &Json, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Json::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn args_hex_addr(args: &Json, key: &str) -> Result<usize, String> {
    let s = args
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("missing or non-string arg: {key}"))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    usize::from_str_radix(s, 16).map_err(|e| format!("bad hex addr in {key}: {e}"))
}

// `scan_xrefs` removed in favour of patternsleuth-backed
// `modforge::patterns::sleuth::scan_all_matches` with an `X<target>`
// pattern. See horsey-mod/docs/todo.md "P0 RULE: USE PATTERNSLEUTH"
// and global CLAUDE.md.
