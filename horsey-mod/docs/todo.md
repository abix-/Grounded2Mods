# horsey-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `targets` | [ ] Apply R4 to remaining ~12 field offsets (recipes proven; per-field work is authoring the right anchor and decoding the operand; see ADDRESS-RESOLUTION.md R4 remaining research plan) | All must + should field offsets pattern-resolved; H-gb-low fields tracked. |
| 1 | `targets` | [ ] Adopt HLT pattern: structural-plausibility validation of dereffed pointers (add looks_like_live_gamestate checking year in [1,10000], roster pointer pair, money <= 100M; promote gamestate::ptr() to call it) | gamestate::ptr() rejects stale heap objects whose later fields are garbage. |
| 1 | `targets` | [ ] Adopt HLT pattern: vtable-pointer validation for typed objects (find GameState vtable RVA, pattern-resolve via constructor mov, add vtable check to looks_like_live_gamestate) | Dereffed pointer's first 8 bytes validated against expected vtable address. |
| 1 | `targets` | [ ] Author second candidate signatures for every resolver so a single MSVC reorder between builds doesn't break it (Definition of Done #2; one sig each today) | Every resolver has >= 2 candidate sigs. |
| 2 | `targets` | [ ] CI / pre-commit refuses to ship any new `pub const usize = 0x140...;` outside targets::resolve::* candidate sigs (Definition of Done #4) | CI check blocks hardcoded addresses. |
| 2 | `modforge` | [ ] Adopt HLT pattern: direct-call vanilla functions via fn-pointer cast (add modforge::vanilla::call_fn::<F: Function>(addr) -> F helper wrapping the transmute safely; use when calling vanilla helpers from patches/detours) | call_fn helper available; at least one consumer uses it. |
| 2 | `patches` | [ ] Migrate patches/lifecycle.rs to modforge::hook::Hook | lifecycle.rs uses Hook with call_original! and SEH guard. |
| 2 | `patches` | [ ] Migrate patches/render_trampoline.rs to modforge::hook::Hook | render_trampoline.rs uses Hook with call_original! and SEH guard. |
| 2 | `patches` | [ ] Migrate patches/save_sidecar.rs to modforge::hook::Hook | save_sidecar.rs uses Hook with call_original! and SEH guard. |
| 2 | `patches` | [ ] Migrate patches/ext_genes.rs to modforge::hook::Hook | ext_genes.rs uses Hook with call_original! and SEH guard. |
| 2 | `ops` | [ ] HTTP op hooks.list that dumps modforge::hook::registry() for diagnostics | hooks.list returns installed hook names, addresses, panic counts. |
| 3 | `inject` | [ ] Adopt HLT pattern: injector elevation auto-detection + re-launch (audit horsey-inject; if OpenProcess(PROCESS_VM_WRITE) fails, re-launch elevated via ShellExecute) | Injection into elevated Horsey.exe works from a non-elevated shell. |
| 3 | `targets` | [ ] Audit every TargetDef in targets_registry.rs: if hint_tolerance < 0x4000 AND no custom structural validator, bump tolerance or add a validator | No TargetDef silently falls back to stale hardcoded RVA on binary drift. |
| 3 | `targets` | [ ] Add is_probable_gamestate_root validator type in modforge::patterns::sleuth (deref candidate, check active_scene_id range + scene_table non-null); apply to GAMESTATE_PTR | GAMESTATE_PTR uses structural validation, not just hint tolerance. |
| 3 | `targets` | [ ] On resolver success log "R3 drift detected: hint=X resolved=Y delta=Z" so stale hardcodes surface in the attach log | Drift between hint and resolved address is always logged. |
| 3 | `targets` | [ ] Once known-builds manifest exists, refuse to attach when resolver delta says hardcoded is stale and no manifest entry matches current image SHA | Attach fails cleanly on unknown builds instead of silently using wrong addresses. |
| 5 | `bestiary` | [ ] Lock the species list (count + names + flavor mix; 9 normal / 10 weird / 9 other split is candidate, not commitment) | Species list committed and documented. |
| 5 | `bestiary` | [ ] Lock the new-gene list (for each new gene: name, what it controls, which patch sites enable it) | Gene list committed and documented. |
| 5 | `bestiary` | [ ] Lock the build order (likely patches first then content, but Q-render-3 and Q-save-2 answers might invert) | Build order committed. |
| 5 | `bestiary` | [ ] Lock the save-compat policy | Save-compat policy documented. |
| 5 | `bestiary` | [ ] Lock the iteration loop (manual restart vs Gene Editor live reload) | Iteration workflow committed. |
| 5 | `patches` | [ ] D2.1: detour FUN_1400a5ee0 (pop processor); when it writes to a pop record, also create EXT_POP_WEIGHTS[pop_id] inheriting from parent pop's extended weights | Per-pop extended weight storage created on pop load. |
| 5 | `patches` | [ ] D2.2: when parsing pop-extended.xml, look up named pop's pop_id and apply per-gene weight overrides into EXT_POP_WEIGHTS[pop_id] | Extended weights loaded from XML and applied per pop. |
| 5 | `patches` | [ ] D2.3: when spawn function picks alleles for a new horse, also pick alleles for genes 240..479 using pop's extended weights; write picks to EXT_HORSE_GENOMES[horse_ptr] | New horses spawn with non-zero ext alleles based on pop weights. |
| 5 | `patches` | [ ] D2.4: sync FUN_1400c03a0 swaps across EXT_POP_WEIGHTS entries | Allele swaps propagate to extended weight storage. |
| 5 | `patches` | [ ] D2.5: stage pop-extended.xml alongside genes-extended.xml in inject.rs::stage_bestiary (same idempotency rules) | pop-extended.xml deployed next to DLL on inject. |
| 5 | `patches` | [ ] D2.6: new module pop_xml.rs mirroring genes_xml.rs; parse pop-extended.xml at DLL attach into static POP_OVERRIDES keyed by pop name; test pop_xml_parses.rs | pop-extended.xml parsed and cached at attach. |
| 5 | `patches` | [ ] D2.7: spawner-count detour in FUN_140076a10 (tmx_map_loader); multiply count by POP_OVERRIDES.get(class); test pop_spawn_doubled.rs | Default pop count multiplied by pop-extended.xml override. |
| 5 | `patches` | [ ] D6.1: find the function that serializes gene-table mutation rates to disk (FUN_1400a4880 XML round-tripper candidate; confirm if it runs during save) | Drift persistence function identified or confirmed absent. |
| 5 | `patches` | [ ] D6.2: if vanilla persists drift to genes.xml on save, mirror extended drift to genes-extended.xml; if not, nothing to mirror | Extended gene drift persistence matches vanilla behavior. |
| 5 | `patches` | [ ] D6.3: document drift-persistence behavior in GENE-CATALOG.md Part 1 | Drift persistence documented. |
| 8 | `scene` | [ ] game.exit_to_overworld via synthetic click on Map button (vanilla.invoke crashed from HTTP thread; Map-button click runs exit on game's own UI thread) | game.exit_to_overworld op exits current scene reliably. |
| 8 | `scene` | [ ] One-time calibration: capture Map button screen coords (relative to window) via input.cursor.get while hovering (should be HUD-anchored, stable across scenes) | Map button coords captured and stored. |
| 8 | `scene` | [ ] Parse data/horsey.tmx once at attach into Vec<{type, world_x, world_y}> for building positions (asset-based, no in-memory walk needed) | TMX building positions available at runtime. |
| 8 | `scene` | [ ] world_to_screen(world_pos) via MapState +0x254/+0x258 camera + zoom + DAT_140303fb4 tile scale | World-to-screen projection returns correct screen coords for buildings. |
| 8 | `scene` | [ ] game.drive_to_location op: click building at projected screen coord, poll truck_pos until arrival or active_scene_id != -1 (15s timeout) | game.drive_to_location drives truck to any TMX building. |
| 8 | `scene` | [ ] game.click_on_screen op (thin wrapper on input.mouse.click_at for test convenience) | game.click_on_screen available via HTTP. |
| 8 | `scene` | [ ] DAT_140303fb4 tile-to-world-pixel scale value confirmed via patterns.read_bytes in-save | Tile scale value pinned down. |
| 8 | `scene` | [ ] Exit-scene function in decomp (grep for *(GS+0x25C) = -1 writes; optional, synthetic click is chosen default) | Exit-scene function found or confirmed unnecessary. |
| 10 | `scene` | [ ] Per-scene Debug tab buttons: one button per known location (Sue's Glues, The Circus, CRISPR Lab, Sumo Ring, Jockey Club, etc.) calling game.drive_to_location | Debug tab has drive-to buttons for all known locations. |
| 10 | `scene` | [ ] Exit to overworld button in Debug tab calling game.exit_to_overworld | Debug tab has exit-to-overworld button. |
| 10 | `tests` | [ ] tests/exit_to_overworld_smoke.rs: from each known scene, hit game.exit_to_overworld, confirm active_scene_id == -1 | Exit smoke test passes from every known scene. |
| 10 | `tests` | [ ] tests/drive_to_location_smoke.rs: from overworld, drive to each known TMX-typed building, assert active_scene_id matches; auto-fill unknown scene labels | Drive smoke test passes for all known buildings. |
| 10 | `tests` | [ ] tests/round_trip_smoke.rs: drive overworld -> scene N -> exit -> drive to scene M -> exit; confirms no state leaks | Round-trip test passes without state leaks. |
| 10 | `hk1` | [ ] A1: capture house door coords (one-shot interactive: launch fresh, hover door, input.cursor.get, write to menu_targets.json as home_door_from_truck_spawn) | House door coords captured and stored. |
| 10 | `hk1` | [ ] A2: common::ensure_home_scene_loaded(&game, timeout) helper (polls active_scene_id; if not Home, replays house-door click via input.mouse.click; polls until in-scene or timeout) | ensure_home_scene_loaded helper available for all in-save tests. |
| 10 | `hk1` | [ ] A3: wire ensure_home_scene_loaded into input_hk1_calibration.rs as first call after launch() | Calibration runs fully unattended from fresh launch. |
| 10 | `ops` | [ ] Change owned_stable to read scene table slot 0x00 unconditionally (NOT slot_offset = active_scene_id*8; when active_scene_id = -1, per-scene chain returns None even though owned list IS at slot 0x00) | owned_stable returns horses from overworld state. |
| 10 | `ops` | [ ] Verify slot 0x00 by naming 3 horses uniquely and confirming all appear | Slot 0x00 holds all owned horses in every state. |
| 10 | `ops` | [ ] Re-validate slot 0xd0: is it the "owned horses in current scene" subset, or a redundant mirror? | Slot 0xd0 semantics documented. |
| 10 | `targets` | [ ] Find name_resolver in new build by anchoring on (bails) format-string xref; decode call disp32 after mov ecx, [horse+0x1f8] to recover FUN_1400c78c0 entry in new build | Name resolver function located in current build. |
| 10 | `targets` | [ ] Read name resolver body and decode disp32 of table-loading lea to find NAME_TABLE address | NAME_TABLE address recovered. |
| 10 | `targets` | [ ] Re-examine table entry layout (old decomp may describe one build; new build may use wrapper struct with 5 sub-pointers per entry) | Table entry layout documented for current build. |
| 10 | `targets` | [ ] Re-enable resolve::name_table() (currently returns None; UI falls back to numeric #name_id) | Horse names resolve to strings in UI and ops. |
| 10 | `targets` | [ ] Add targets::image_sha256(): hash .text + .data sections of loaded Horsey.exe (skip .reloc), log at attach | Image hash logged at every attach. |
| 10 | `targets` | [ ] HTTP op game.build_info returning hash + mtime + image_base() + known build name if hash matches | game.build_info op available. |
| 10 | `targets` | [ ] Add horsey-mod/research/known-builds.toml: per-build manifest with hash, date, decomp_index_path | Known-builds manifest exists and is consulted at attach. |
| 15 | `bestiary` | [ ] Back up vanilla pop.xml, horsey.tmx, genes.dat before smoke-testing content pipeline | Backups taken before any content changes. |
| 15 | `bestiary` | [ ] Add one smoketest pop block in pop.xml with extreme gene weights, place one spawner in horsey.tmx, delete genes.dat, launch and confirm spawn | Smoke-test species spawns visibly in-game. |
| 15 | `bestiary` | [ ] If smoke fails: read FUN_1400a3eb0 neighbors for pop loader hardcoding | Failure root cause documented. |
| 15 | `bestiary` | [ ] bx_arabian: small head, dished face, high tail | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_draft: huge SIZE, thick BONES, short legs | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_mini: tiny SIZE, short everything | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_wolf: canine proportions, long body | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_sheep: compact, fluffy palette | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_goat: short, horned, agile build | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_kangaroo: big hind legs, tiny front, long tail | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_ostrich: biped, long legs, no front | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_elephant: max SIZE, max BONES, gray | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_mushroom: short body, huge cap-shaped head | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_kelp: tall, skinny, swaying, green | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_slime: max SKINNY off, high GUT, jiggly | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_worm: no legs (QUADRUPED off), elongated body | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_skeleton: min GUT, max BONES, pale palette | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_ghost: semi-transparent palette, no legs | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_virus: tiny, spiky OSTODERM, vivid color | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_mold: low body, fuzzy palette, blotchy | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_crystal: angular OSTODERM extremes, shiny | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_balloon: huge GUT, tiny legs, bright color | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_unicorn: horse base + single-spike OSTODERM | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_pegasus: horse base + wing-shaped extras | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_kelpie: dark horse + aquatic palette | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_motorcycle: vehicle pop variant of car | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_tractor: bigger / boxier vehicle pop | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_shopping_cart: small wheeled-thing pop | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_robot: humanoid but rigid, metallic palette | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_clown: humanoid variant under human | Species spawns with correct silhouette. |
| 15 | `bestiary` | [ ] bx_pirate: humanoid variant under human | Species spawns with correct silhouette. |
| 15 | `patches` | [ ] Implement chosen table-extension strategy from Q-gene-4 in patches.rs (patch infra already works) | Gene table extended to 480 at runtime. |
| 15 | `patches` | [ ] For each gene-by-INDEX consumer flagged in Q-gene-3, patch to name lookup OR re-bake index for new layout | All index consumers handle extended gene table. |
| 15 | `patches` | [ ] If save format needs patching per Q-save-2, implement it | Save format handles extended genes. |
| 15 | `patches` | [ ] For each new visual mode with no vanilla equivalent, patch renderer handler to recognize new gene | New visual modes render correctly. |
| 15 | `ops` | [ ] genes.add HTTP op to stage new gene definitions live for testing | genes.add op available for live iteration. |
| 15 | `bestiary` | [ ] Author all new gene definitions in genes.xml or genes-extended.xml | All new genes defined in XML. |
| 15 | `bestiary` | [ ] Author one pop block per species in pop.xml nested under right parent (default, human, car) | All 28 new species in pop.xml. |
| 15 | `bestiary` | [ ] Place spawners in horsey.tmx for each new species (per species: dense in one biome or scattered; avoid tutorial spawn area) | All species have spawners placed. |
| 15 | `ops` | [ ] pops.list HTTP op: all currently loaded pops | pops.list returns pop data. |
| 15 | `ops` | [ ] pop.read HTTP op: all gene weights for a named pop | pop.read returns weight data. |
| 15 | `ops` | [ ] pop.gene.set HTTP op: mutate live gene weights (if chromomap is mutable post-load per Q-reload-1) | pop.gene.set mutates live weights. |
| 15 | `ops` | [ ] pop.xml.reload HTTP op: re-parse pop.xml from disk without restart | pop.xml.reload works without restart. |
| 15 | `ops` | [ ] genes.xml.reload HTTP op: re-parse genes.xml from disk without restart | genes.xml.reload works without restart. |
| 15 | `ops` | [ ] spawner.preview HTTP op: spawn N creatures of a pop at a coord | spawner.preview spawns creatures. |
| 15 | `bestiary` | [ ] Ship as folder user drops into Horsey/data/ plus DLL; one-line uninstall (restore backups + delete genes.dat + don't inject) | Bestiary mod installs and uninstalls cleanly. |
| 15 | `bestiary` | [ ] Optional: PowerShell installer / uninstaller | Installer script works. |
| 15 | `bestiary` | [ ] Document save-compat policy from Q-save-3 | Save-compat policy documented for users. |
| 15 | `tests` | [ ] Walk the map, screenshot every new species | All species visually confirmed. |
| 15 | `tests` | [ ] Confirm none of the new species break breeding, racing, or save load | No regressions from new species. |
| 15 | `tests` | [ ] If a species crashes the renderer, narrow which gene pushed it over the edge and document safe range | Renderer crash causes documented. |
| 15 | `ops` | [ ] D7.1: genes.xml.reload and pop.xml.reload HTTP ops (Q-reload-1 still open; may not be feasible if vanilla lacks reentrant loader) | Reload ops work or documented as infeasible. |
| 15 | `ops` | [ ] D7.4: per-horse full dump op (needs stable horse_id resolution per D4.4 finding) | Per-horse dump returns all fields and alleles. |
| 15 | `tests` | [ ] D8.2: add ONE extended gene (BX_WING_SIZE), spawn a horse of a pop that uses it, confirm slot value matches diploid blend math | Extended gene value correct in spawned horse. |
| 15 | `tests` | [ ] D8.3: save / reload round-trip for horse with ext alleles (blocked on D4 stale save addresses) | Ext alleles survive save/reload. |
| 15 | `tests` | [ ] D8.5: stress test: add 100 ext genes, profile per-frame cost of D5 trampoline + sidecar file size | Performance within budget; file size documented. |
| 15 | `tests` | [ ] D8.6: hot-reload genes-extended.xml via HTTP mid-game, confirm visual change without restart | Hot-reload produces visible change. |
| 15 | `docs` | [ ] D9.2: write "How to author a new gene" guide for non-engineers: genes-extended.xml format, mode options, working examples | Gene-authoring guide written. |
| 15 | `docs` | [ ] D9.3: decide rollout: ship patch infra with zero extended genes by default and let authors add via XML, OR ship starter pack of 20-50 new genes | Rollout strategy committed. |
| 20 | `hk1` | [ ] HK1 Shift+Click smart-transfer: synthesize input.combo with shift + input.mouse.drag backend=l3 to drive FUN_1400d2ab0 end-to-end (4 helpers run automatically) | Shift+Click transfers a horse to the contextually correct destination. |
| 20 | `hk1` | [ ] SDL input hook: intercept mouse/keyboard events before engine handles them, single dispatch point | Input hook installed and dispatching. |
| 20 | `hk1` | [ ] Modifier-key state tracker (reusable for future modifier-click bindings) | Modifier state tracked accurately. |
| 20 | `hk1` | [ ] "Currently hovered/selected horse" resolver: given mouse position + game state, return horse pointer under cursor | Hovered horse identified from mouse position. |
| 20 | `hk1` | [ ] "Where is this horse now?" + "Where should it go?" resolvers: read container (vehicle/pasture/race line) and current scene to pick destination | Source and destination resolved from context. |
| 20 | `hk1` | [ ] "Move horse to destination" op wrapping vanilla's click-drag release function (reuse, not reimplement, to keep all vanilla side effects) | Move op completes transfer with all vanilla side effects. |
| 20 | `hk1` | [ ] Settings entry for enabling/disabling HK1 | HK1 can be toggled off. |
| 20 | `hk1` | [ ] Trailer/pasture detector: find the real trailer RECTANGLE (click-handler extent constants read as garbage in memory; position-based rule "non-zero = trailer" is too loose) | Detector correctly classifies trailer vs pasture for all horses. |
| 20 | `hk1` | [ ] Coupe DeVille un-grabbable: collision body is missing/disabled (failed physics rebuild FUN_1400b3dc0); repair = force physics rebuild or alternative | Coupe DeVille can be grabbed and transferred. |
| 20 | `ops` | [ ] Fix stale ops.rs:354 docstring (references +0x90 which is wrong; owned_stable reads scene-table slot 0) | Docstring matches actual implementation. |
| 25 | `ui` | [ ] C7 detail-view polish: drop comma-separated preview line, swap digit buttons for colored squares with first-letter overlay (color saturation encodes tier 0..3) | Detail view cells are readable and compact. |
| 25 | `ui` | [ ] C7: group ext genes (currently one flat 240-cell block, no chromosome metadata; options: chromosome="X" in genes-extended.xml, auto-group by render mode, prefix match on BX_FAMILY_*) | Ext genes grouped meaningfully in detail view. |
| 25 | `ui` | [ ] C7: bulk per-chromosome buttons: "max this chromosome", "wild-type this chromosome" | Bulk chromosome ops available in detail view. |
| 25 | `ui` | [ ] C8b matrix-view rework: herd overview (rows = horses, columns = 20 chromosome summaries + 1 ext summary = 21 cells; colored squares encoding chromosome avg) | Herd overview fits 100 horses on screen. |
| 25 | `ui` | [ ] C8b: chromosome zoom (click swatch -> that chromosome's named genes across ALL horses) | Chromosome zoom shows per-gene comparison. |
| 25 | `ui` | [ ] C8b: horse detail (click horse name -> chromosome-strip Details panel) | Horse detail drills into full genome. |
| 25 | `ui` | [ ] C8b: filter row (name search, sort by chromosome-N-sum, "show only horses where SIZE > 2") | Filtering and sorting work in matrix view. |
| 25 | `ui` | [ ] C6: horse.genome.snapshot.save/load/list HTTP ops backed by JSON under DLL_dir/snapshots/; UI "Save snapshot" / "Load snapshot" dropdown on Details panel | Genome snapshots persist and restore. |
| 25 | `ui` | [ ] C4: snapshot a horse's full genome to clipboard / disk; paste onto another horse | Copy-paste genome between horses works. |
| 25 | `ui` | [ ] C4: side-by-side compare two horses (next/prev arrows on expand panel; diff highlight) | Two-horse diff highlights differences. |
| 25 | `ui` | [ ] C4: bulk per-chromosome ops: "copy from another horse" | Cross-horse chromosome copy works. |
| 25 | `ui` | [ ] C4: filter "show only chromosomes that differ from species default" / "show only nonzero" | Chromosome filters work in detail view. |
| 25 | `ui` | [ ] Ext gene name labels from genes-extended.xml (mod already parses it; expose via gene_names::ext_gene_name(idx)) | Ext genes show names in UI. |
| 30 | `ui` | [ ] Roster panel overlay: side-panel showing ALL horses with one row each (Name, Loc, Age, Hunger, Tired, Breed-Ready, Skill, Status); click row to focus camera + select; filter chips | Roster panel renders with sorting and filtering. |
| 30 | `ui` | [ ] Count display: floating "N" badge above stacks of overlapping horses; disappears on mouse-over | Stack count visible at a glance. |
| 30 | `ui` | [ ] Pasture auto-buy hay: read pasture hay stock + price; when stock crosses low-water mark and player has money, fire buy op; configurable threshold | Hay auto-purchased when stock is low. |
| 30 | `ui` | [ ] modforge::ui::overlay_dxgi module: kiero-style vtable scan for IDXGISwapChain::Present, MinHook it, init ImGui_ImplDX11 on first call, render TabDef set | ImGui overlay renders inside the game window. |
| 30 | `ui` | [ ] Wire WndProc forwarding so ImGui sees mouse/keyboard from game's HWND | ImGui captures input in-game. |
| 30 | `ui` | [ ] Behind feature flag so native-window path stays as fallback | Both overlay modes available. |
| 30 | `ui` | [ ] First arming pass: run alongside current window; once stable, retire separate window | In-game overlay replaces separate window. |
| 35 | `tests` | [ ] Author tests/watch_region.rs (rolling region diff in discovery mode; transition assertion in assertion mode; env-driven; output to reviewable log) | watch_region test runs in both modes. |
| 35 | `tests` | [ ] Author tests/watch_until_change.rs (single-value transition logger with optional max-latency assertion) | watch_until_change test runs. |
| 35 | `tests` | [ ] Run watch tests on 3 HLT semantic conflicts; commit one assertion-mode test per resolved semantic; update horse_offset names if HLT was right | Semantic conflicts resolved with permanent tests. |
| 35 | `tests` | [ ] Run watch tests on Phase E 8 mystery FIELD_* offsets; commit assertion tests for each discovered semantic | Mystery field semantics documented with tests. |
| 35 | `tests` | [ ] Run watch test on 0x280 conflict (pointer pair vs timer); commit resolved-form assertion | 0x280 semantics confirmed with test. |
| 35 | `modforge` | [ ] Extract watch_region to modforge::tools::watch_region once horsey-mod proves the shape | watch_region available to all game mods. |
| 35 | `ops` | [ ] Audit all gamestate.*/horse.*/mem.* ops for unguarded raw deref of caller-supplied addresses (patterns.read_bytes partially fixed with is_addr_readable precheck) | All ops survive bad addresses without crashing. |
| 40 | `targets` | [ ] Add targets::resolved module with patternsleuth signatures for each hooked function (prologue bytes with ? wildcards for compiler-shifted instructions) | All hooked functions resolved via pattern scan. |
| 40 | `targets` | [ ] At DLL attach, run single patternsleuth scan over .text for all registered patterns; populate static map (symbol, resolved_addr) | All symbols resolved in one scan at attach. |
| 40 | `targets` | [ ] fn_addr::APPLY_GENE_TO_HORSE (and friends) become lookups into resolved map instead of const usize | No const function addresses in production code. |
| 40 | `targets` | [ ] HTTP op targets.scan_report returning symbol, resolved_addr, matched_via, confidence | Scan report available for debugging. |
| 40 | `targets` | [ ] horsey-mod/research/extract-signatures.py: inputs INDEX.md + function-body files, outputs TOML/Rust constants with prologue bytes and wildcards (diff across known builds for wildcards) | Signature extraction automated. |
| 40 | `targets` | [ ] CI / pre-merge: run extractor on latest decomp, diff against committed sigs, fail if drift > threshold | CI catches signature drift. |
| 40 | `targets` | [ ] Per-function comment in targets/resolved.rs cites decomp file the sig was derived from | Signature provenance documented. |
| 40 | `targets` | [ ] If critical-path function fails resolution, arm() refuses to install with clear error message | Critical function absence logged clearly. |
| 40 | `targets` | [ ] If non-critical function fails, log + continue (mod works minus that feature) | Non-critical absence is graceful. |
| 40 | `targets` | [ ] dryrun HTTP ops report resolved: bool per target so operator sees what's missing before arming | Pre-arm visibility into resolution status. |
| 40 | `targets` | [ ] At attach: compare image_sha256() to hash that generated current decompiled/INDEX.md; if different, big-yellow log line | Build drift warned at attach. |
| 40 | `targets` | [ ] Optional: tools/refresh-decomp.ps1 wrapper (decompile.py + extract-signatures.py, commit under chore: prefix) | Decomp refresh is one command. |
| 40 | `targets` | [ ] Optional: GitHub Actions job (manual trigger, Windows runner with Ghidra) for decomp refresh | Decomp refresh available in CI. |
| 45 | `research` | [ ] Confirm pop.xml p0/p1/p2/p3 are INVERSE weights (read spawn code in chromomap loader or similar) | Weight semantics verified from code. |
| 45 | `research` | [ ] Confirm "press 5 in balloon" = x300 speed by reading balloon controller / pause input handler | Speed factor verified from code. |
| 45 | `research` | [ ] Find buried-item ID -> item-type table in code (offsets 0-47) | Item table found and documented. |
| 45 | `research` | [ ] Find building-uniqueness check (first-instance-wins logic) | Building rule found and documented. |
| 45 | `research` | [ ] Find CRISPR-Lab world-swap logic (vial sub-map teleport) | CRISPR swap logic found and documented. |
| 45 | `research` | [ ] FUN_1400c0660 (+-5 mutator): find callers, check for literal indices | Mutator callers documented. |
| 45 | `research` | [ ] FUN_1400c03a0 (allele swap): find callers | Swap callers documented. |
| 45 | `research` | [ ] FUN_1400c1cf0 (CRISPR?): find callers | Function purpose determined. |
| 45 | `research` | [ ] Check for unused padding after DAT_1403ee4a4 for in-place expansion | Padding documented. |
| 45 | `research` | [ ] Decide extension headroom: 256 (double), 512 (quadruple), 1024 (future-proof) | Extension size committed. |
| 45 | `research` | [ ] Cross-reference vanilla pop.xml against 233 referenced indices to find genes that exist in code but no pop uses at non-default weights (soft-free, repurposable) | Soft-free genes identified. |
| 45 | `research` | [ ] Read each of 7 hard-free slots (56, 57, 107, 183, 184, 209, 216) and confirm truly effect-free | Hard-free slots verified. |
| 45 | `research` | [ ] Q-render-1: per-oddity decomposition (which genes drive car wheels, helix shape, etc.) for full per-species breakdown | All oddity gene mappings documented. |
| 45 | `research` | [ ] Confirm 91 fully-unused slots not touched by other consumer chains (breeding compat check FUN_1400b78d0 calls FUN_1400c5c10 with different stack offsets) | Unused slots confirmed safe. |
| 45 | `research` | [ ] For each new visual mode wanted (wings, wheels, transparency), determine if any vanilla pop already exhibits it (does car reuse gene for wheel rendering?) | Visual mode feasibility documented per mode. |
| 45 | `research` | [ ] Confirm adding new gene NAMES: does FUN_1400a3eb0 (chromomap loader) accept arbitrary names or only 242 enum'd ones? | Gene name acceptance confirmed from code. |
| 45 | `research` | [ ] Find first-instance-wins building-placement scan code | Building placement rule documented. |
| 45 | `research` | [ ] Find item ID 48+ bug (item table) | Item table bug documented. |
| 45 | `research` | [ ] Find 400x225 map size in tmx parser | Map size limit confirmed from code. |
| 45 | `research` | [ ] Confirm runtime mutation-during-breeding behavior: does child get random allele flips beyond parent picks? (not seen in FUN_1400a2d80; mutation may be in FUN_1400b2e30 after combinator + engine run, or may not exist for child-creation) | Breeding mutation behavior documented. |
| 50 | `research` | [ ] Read and document each of 20 extracted key functions; curated names + descriptions in ALL-FUNCTIONS.md | All key functions named and documented. |
| 50 | `research` | [ ] Reconstruct Horse struct (offsets +0x350, +0x39c, +0x3a0 from interact_dispatch_or_status_check are all fields of same struct) | Horse struct layout documented. |
| 50 | `research` | [ ] Walk callers and callees of named functions; document those too; expand outward | Call graph expanded. |
| 50 | `research` | [ ] Use Function ID Ghidra analyzer with public SDL3/MSVC signature databases to bulk-name vendor functions | Vendor functions identified and excluded. |
| 50 | `research` | [ ] Find fatigue counter in save (save-diff procedure: save, race one horse, save, byte-diff) | Fatigue field offset in save documented. |
| 50 | `research` | [ ] Find age field and retirement threshold in save (save-diff procedure: save, let many days pass, save, find monotonically increased bytes) | Age field offset in save documented. |
| 50 | `research` | [ ] Identify genome location in save (242 genes x ~4 bytes = ~970 bytes/horse; 55KB block / 85 horses = 646 bytes/horse) | Genome location in save documented. |
| 50 | `research` | [ ] Confirm pop_id mapping in roster (cross-reference observed values against pop.xml ordering) | pop_id mapping confirmed. |
| 50 | `research` | [ ] Decode flag_a..flag_e in 22-byte trailer (could be sex, gender-marker, pregnant state, location, breed sub-type, color) | Trailer flags decoded. |
| 50 | `research` | [ ] Does Horsey have an existing ImGui integration we can reuse? (probably no) | ImGui integration checked. |
| 50 | `research` | [ ] Is SDL3 input layer hookable cleanly via DLL? (probably yes via SDL3 public API forwarding) | SDL3 hookability confirmed. |
| 50 | `research` | [ ] Are there per-frame update hooks we can hijack cleanly? (0x1400dbe10 post_race_wrapup is one; need global per-frame tick) | Per-frame hook candidates documented. |
| 50 | `research` | [ ] What does the engine do when DLL load order is wrong? (test by injecting no-op DLL first) | DLL load order behavior documented. |
| 50 | `inject` | [ ] Fix hot-reload crash: horsey-inject --reload performs swap but game crashes seconds later (helper threads in old DLL haven't unwound; FreeLibrary returns but thread returns into freed code; fix candidates: synchronous _shutdown, injector polls port closed + waits, FreeLibraryAndExitThread) | Hot reload completes without crash. |
| 50 | `server` | [ ] No state-change broadcasting: clients poll game.read for updates; websocket or SSE upgrade on medium-term roadmap | Clients receive push updates. |
| 50 | `server` | [ ] Write-op gating: bot operators have run write cheats unprompted during smoke tests; write-ops should be user-approved only | Write ops require explicit approval. |
