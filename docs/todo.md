# TODO

!!! info "Scope"
    Authoritative on open work across the workspace. What's NEXT,
    not what's done. Completed milestones live in
    [`changelog.md`](changelog.md). Per-subject deep dives live in
    each crate's `docs/` folder ([`../ueforge/docs/`](../ueforge/docs/)
    for the framework, [`../grounded2-mod/docs/`](../grounded2-mod/docs/)
    for the Grounded 2 mod). Read those FIRST when investigating a
    subject.

!!! tip "How to use"
    Roughly ordered by leverage. Pick from the top of the list when
    starting a new session.

---

## Next up (2026-05-14+)

WWM put down for now. Demo-end blocked successfully (see the
2026-05-14 entry in [`changelog.md`](changelog.md) and the Plot
Enlargement investigation under the same date). Pick up another
front (Endless / ueforge / Outworld) in the next session unless
the user redirects.

User redirect 2026-07-04: the active front is Survivalist:
Invisible Strain (official-loader host + survivalist-mod); see
the P0 section directly below.

0. **Fix the silently broken Harmony bridge in
   `cs-shim-common/HarmonyBridge.cs`.** PatchPrefix +
   PatchPostfix both construct the Harmony target from
   `new Action(() => del(...))`. An instance-method
   lambda which HarmonyLib rejects. Every Rust-side
   `patch_prefix` / `patch_postfix` call in the workspace
   has been silently failing for the whole session
   (`DigManager.Dig` and `PlayerManager.AddPlayerCurrency`
   postfixes wwm-mod relies on for XP also never fire).
   Replace with a static dispatcher keyed by patch
   handle. Until this lands, Rust-side Harmony patching
   does nothing.
1. **Verify the hot-reload cycle works in-game.** Launch
   WWM, observe `Unityforge.Shim: ready (generation 0)`,
   then run `build_and_deploy.ps1 -Hot` from a separate
   terminal, watch BepInEx log for `hot reload generation
   0 -> 1`. Confirm curl ops still answer after the swap.
   If it works, mark Phase 4 verified in
   [§6.5](unityforge-plan.md#65-hot-reload-phase-4).
2. **Fix the janky jump.** The current `Translate(0,3,0) +
   isGrounded=false` is unreliable. Investigate
   `PlayerController` per-frame movement to find what
   clobbers our position/velocity. Likely needs a Harmony
   prefix on the per-frame movement method to inject a
   jump-velocity boost.
3. **Ship `UnitySkillProxyEffect`** + repoint Strong Back
   at `SkillsManager.SetSkillLevel("Bag", N)`. The game
   already grows the slot list when you call this. A way
   bigger payoff than fighting raw fields. Verified live
   in the 2026-05-13 session (5 -> 12 slots).
4. **Verify Greedy Miner in-save.** Load a save in WWM,
   `walk_class MineDataSO` to confirm field names. Repoint
   the catalog if needed.
5. **Mirror generation-loader into IL2CPP shim** before
   shipping the IL2CPP smoke target end-to-end.
6. **horsey-mod: `sleep_safe_no_tire` patch site
   discovery.** wip in [`a31246f`](.). The patch infra
   works (VirtualProtect / write / FlushInstructionCache /
   revert on detach), but `find_patch_site` cannot
   uniquely identify the `+0x206` zero-store inside the
   no_tire loop at `FUN_1400ceb60`. Three iterations
   tried (RAX-only, all-regs / 2 candidates, proximity
   to `+0x205` sibling within 64 bytes); none uniquely
   match. Next idea: walk back from the
   `DAT_1403d95c5` read inside the function and pick
   the FIRST `+0x206` zero-store that follows it. If
   that also ambiguates, fall back to a hand-disassembled
   offset baked into a per-build constant. Until this
   lands, the split-flag suppressor (zero `+0x205` from a
   Rust worker thread) is the shipping path.

The detailed checkboxes below cover everything else, but
those listed above are the immediate path forward.

---

## P0. Feature builder: DRY mod feature application (2026-08-17)

Every UE mod has a list of features that apply at different times:
once at init, on each game load (actor appears), when a DataTable
first streams in. Today each mod hand-wires these calls in its
`on_unreal_init`. The primitives exist (`on_each_load`,
`on_first_sight`, `apply_when_ready`) but the wiring repeats.

Add a `ueforge::Features` builder that collects all of a mod's
features in one place, with consistent logging ("feature X:
installing / applied / waiting for reload") and a single
`.install()` call.

```rust
// in the game mod's on_unreal_init:
ueforge::features()
    .on_each_load("suppress_nag", Duration::from_millis(500),
        || find_object("WD_PlaytestNote01_C", None, false),
        |_| { sleep(Duration::from_secs(2)); send_key(0x20); })
    .on_each_load("speed_default", Duration::from_secs(2),
        || find_actor("BP_SGKMasterCharacter_C", None),
        |_| { speed::set_multiplier(2.0).ok(); })
    .on_each_load("vendor_food", Duration::from_secs(3),
        || find_actor(VENDOR_CLASS, Some("Barman")),
        expand_barman_sell_list)
    .install();
```

Three trigger types:
- `on_each_load(label, poll, finder, apply)`: re-applies after
  main menu reload (wraps `actor::on_each_load`)
- `on_first_table(label, table, timeout, apply)`: once when a
  DataTable streams in (wraps `datatable::on_first_sight`)
- `once(label, apply)`: runs immediately at install time

### Tasks
- [ ] Add `ueforge::features` module with `Features` builder
- [ ] Convert misery-mod to use the builder
- [ ] Convert outworld-station-mod to use the builder

---

## P0. Survivalist: Invisible Strain: official-loader host + survivalist-mod (2026-07-04)

> **Driver:** operator directive 2026-07-04. Personal-use mod to
> customize the game (NPC behavior, loot, zombies, content). Not
> for Workshop publishing. The game ships an official mod loader;
> we use it INSTEAD of BepInEx, on top of modforge/unityforge.
> The official loader becomes a second unityforge host next to
> BepInEx, so any future game with a built-in mod loader reuses
> the same seam.

### Verified game facts (decompiled from the game's Assembly-CSharp, 2026-07-04)

- Install: `C:\Games\Steam\steamapps\common\Survivalist Invisible Strain\`.
  Unity Mono (MonoBleedingEdge; plain `Assembly-CSharp.dll` in
  `Survivalist Invisible Strain_Data\Managed\`, un-obfuscated).
  Modern Unity (the dev's model-import guide says 6000.0.35f1).
- Mods and Stories are subfolders in the game's directory. "Is Mod"
  in Story Settings puts a creation in the Mods tab; one Story plus
  many Mods active at once; dependencies declared in Story Settings.
- The loader is `Story.LoadDLLs()` (re-derive the decompile any time
  with `ilspycmd -t Story Assembly-CSharp.dll`). For each `*.dll` in
  `<story-or-mod folder>/DLLs`: `Assembly.LoadFrom`, then
  `assembly.GetType("Main")` (GLOBAL namespace, no namespace), then
  invoke `public static void Load()`, then track the assembly in
  `Story.Assemblies`. Each file is wrapped in its own try/catch: a
  bad or native DLL logs an error and is skipped, the game continues.
- `Story.UnloadDLLs()` invokes `public static void Unload()` on story
  content unload (story switch / quit to menu). Managed assemblies
  never actually leave the process, so a second `Load()` after
  `Unload()` runs on the SAME statics. Load/Unload must be
  re-entrant.
- `LoadDLLs()` is called from `Story.LoadAssetBundles()`, which
  ALWAYS executes on the Unity main thread (background story loads
  park on the `WantAssetBundlesLoaded` handshake and the main thread
  services it). Creating GameObjects inside `Load()` is safe.
- Load timing: story content load (starting or loading a game), NOT
  app start. The control plane is only up while a story with our
  mod active is loaded.
- Harmony the official way: the Steam Workshop mod "Harmony 2.0.4"
  (id 2366696532) ships `0Harmony.dll` in its DLLs folder (no `Main`
  class, so the loader loads it into the process and skips the
  entry-point invoke); our mod declares it as a dependency in Story
  Settings. That is pardeike Harmony 2.0.4, NOT BepInEx's HarmonyX.
- Entry convention proof: bzy-xyz/Survivalist2-DisableHUD (github,
  trunk branch): `public class Main` in the global namespace,
  `public static Harmony HarmonyInstance`, `Load()`/`Unload()`,
  logs via `UnityEngine.Debug.Log`.
- The Rust cdylib must NOT sit in `DLLs/` (the loader would
  `Assembly.LoadFrom` it and log a BadImageFormat error on every
  story load). It lives elsewhere in the mod folder; the shim
  locates it relative to its own assembly location.

### Tasks

- [x] **SV1: extract the generation loader out of the BepInEx
  plugin.** Shipped 2026-07-04 commit `abdbfa40`:
  `cs-shim-common/GenerationLoader.cs` owns the Generation class,
  LoadGeneration, CheckHotReload/HotSwap, LocateRustDll, and the
  NativeLibrary P/Invoke wrapper, plus the ShutdownForUnload /
  ReinitAfterUnload pair for the official-loader re-entry cycle.
  Plugin.cs keeps the [BepInPlugin] entry, logger wiring, and the
  WWM patches; mono shim builds green against WWM's BepInEx.
- [x] **SV2: logger sink seam.** Shipped 2026-07-04 commit
  `abdbfa40`: `ShimLogger.Sink` delegate; both BepInEx hosts wire
  their ManualLogSource, the survivalist host wires
  `UnityEngine.Debug.Log` with a `[Unityforge]` prefix. il2cpp
  Plugin.cs updated too (written, not built here; no BepInEx 6
  install on disk).
- [x] **SV3: new host project `cs-shim-survivalist/`
  (`Unityforge.Shim.Survivalist.csproj`).** Shipped 2026-07-04
  commit `61ee122b`. Global-namespace `Main` with static
  `Load()`/`Unload()`, driver GameObject, re-entrant Load via
  GenerationLoader park + re-init. Builds green against the game's
  Managed dir + Lib.Harmony 2.0.4.
  - Load(): re-entrant guard; set the logger sink; EnsureHarmony;
    locate `*.unityforge.dll` relative to the shim assembly (mod
    folder, outside `DLLs/`); create a hidden persistent driver
    GameObject (DontDestroyOnLoad) whose Update does what the
    BepInEx host's Update does (InputBridge.PollAll, CheckHotReload,
    active.Tick); load generation 0.
  - Unload(): shut down the active generation (the Rust
    SHUTDOWN_REGISTRY joins server + poller threads and unpatches
    per-handle), InputBridge.Clear(), MonoBridge.ClearHandles(),
    destroy the driver GameObject, KEEP the module mapped, stash the
    generation so the next Load() re-inits it via gen.Init() (same
    move as the hot-reload rollback path).
  - References: official 0Harmony.dll 2.0.4 (Lib.Harmony 2.0.4
    NuGet, compile-only) + the game's UnityEngine modules
    (HintPaths via -p:UnityDir). Target framework RESOLVED: net472
    (Lib.Harmony 2.0.4 ships net4x targets only, NU1701 under
    netstandard2.1; matches the DisableHUD net40 precedent).
- [x] **SV4: make the Harmony bridge actually work + Harmony 2.0.4
  compat.** Shipped 2026-07-04 commits `61ee122b` + `b427a15d`.
  PatchPrefix/PatchPostfix now target ONE static PrefixDispatcher +
  ONE static PostfixDispatcher (real static methods Harmony
  accepts), routed per patched method through a delegate list;
  non-zero Rust prefix return = skip original per
  unityforge/src/hook.rs. UnpatchSelf confirmed HarmonyX-only
  (CS1061 against Lib.Harmony 2.0.4); UnpatchAll now unpatches
  per-dispatcher. Both shims build green. NOT yet live-verified
  (that is SV7's "one Rust-side Harmony patch observably fires").
- [x] **SV5: `survivalist-mod` Rust crate.** Shipped 2026-07-04
  commit `2926b1f9`. cdylib on unityforge + modforge,
  `unityforge_mod!` entry, http_port 17173, on_init registers
  ops::register_builtins + selector::register_builtins. Builds
  green.
- [x] **SV6: deploy.** Shipped 2026-07-04 commit `0ba153c7`:
  `survivalist-mod/scripts/build_and_deploy.ps1` (shim to
  `<game>/<ModName>/DLLs/`, cdylib to the mod root as
  `survivalist_mod.unityforge.dll`, -Hot generation staging,
  -NoCopy build path verified). The mod folder itself is created
  once by the operator in the in-game editor (Is Mod ticked,
  Harmony 2.0.4 workshop dependency added in Story Settings); the
  script only copies files.
- [x] **SV7: live smoke.** Verified against the live game
  2026-07-04 (operator mod folder: `SurvivalistTweaks`):
  - Shim + loader `[Unityforge]` lines in the player log: YES
    (Harmony 2.0.4 dependency DLL loaded first, then the shim,
    then the Rust cdylib from the mod root).
  - `{"op":"ping"}` on 17173: YES.
  - `walk_class HudBehaviour`: YES (live instance `HudPanel`,
    handle returned). NOTE: op args nest under `"args"` per the
    envelope (`{"op":"walk_class","args":{"class":"..."}}`).
  - `inspect_object` on that handle: YES (field dump with nested
    handles).
  - Hot reload generation 0 -> 1 via `-Hot`: YES; gen1
    re-initialized and ping answers after the swap.
  - Quit-to-menu + load back in: the game KEEPS the story content
    loaded; `UnloadDLLs` never fired, the control plane stayed up
    throughout, no crash. So Unload/re-Load only happens on a
    story SWITCH; the `ReinitAfterUnload` path is still
    unexercised live (structurally covered, verify when a story
    switch happens naturally).
  - One Rust-side Harmony patch observably firing: VERIFIED
    2026-07-04 (the faction-war revenge trigger fired live:
    "Almighty Rock Family sets a revenge invasion on The Golden
    Dudes (member killed)" in the player log). NOTE: patching
    required embedding Harmony 2.4.2 in the shim (the workshop
    2.0.4 cannot rebuild generic-call bodies on the game's Unity
    6 mono) and the args0 write-back ctx kind for struct args;
    full trail in survivalist-mod/docs/research.md.
- [x] **SV8: gameplay research doc.** Shipped 2026-07-04:
  `survivalist-mod/docs/research.md` (live-access recipe,
  difficulty knob cluster, class maps, Harmony constraints) plus
  `survivalist-mod/docs/faction-war.md` (the active front: the
  operator's faction-war vision, full mechanics documentation,
  and phase 1 live-verified: AI-vs-AI war ignition, a real
  assault, and the self-sustaining revenge loop). The x/10 status
  tracker now lives in `survivalist-mod/docs/status.md`, the single
  home for every score.

### Definition of done

- [x] The official loader is a first-class unityforge host: the
  BepInEx host and the survivalist host share the generation
  loader, bridge table, and hot reload from cs-shim-common.
  Live-verified 2026-07-04.
- [x] `survivalist-mod` answers curl in-game with the standard
  reflection op surface. Live-verified 2026-07-04.
- [x] One Rust-side Harmony patch verified live 2026-07-04 (the
  faction-war revenge trigger; also the infection prefix installs
  clean). Closes "Next up" item 0 for the Mono path via the
  static-slot dispatchers in cs-shim-common/HarmonyBridge.cs.
- [/] The Load/Unload cycle: quit-to-menu + reload keeps the DLLs
  loaded (no unload fires) and the control plane stays up; the
  actual story-SWITCH re-init path is structurally covered but not
  yet exercised live.
- [x] Hot reload verified in-game on this host (gen 0 -> 1,
  2026-07-04).

### Open questions

- RESOLVED: shim target framework is net472 (Lib.Harmony 2.0.4 has
  no netstandard target; DisableHUD precedent).
- RESOLVED: player log is
  `%USERPROFILE%\AppData\LocalLow\Ginormocorp Industries\Survivalist Invisible Strain\Player.log`
  (company/product read from the game's app.info).
- RESOLVED: 0Harmony.dll at build time comes from the Lib.Harmony
  2.0.4 NuGet package (compile-only, PrivateAssets=all +
  CopyLocalLockFileAssemblies=false); at runtime the game loads the
  workshop dependency mod's 0Harmony.dll.
- Whether the game re-calls Load() on the SAME Story object or a
  fresh one per story select (structurally handled either way by
  the re-entrant guard; confirm during SV7).

### SV7 prerequisites (operator, one-time)

1. Subscribe to the Steam Workshop "Harmony 2.0.4" mod
   (id 2366696532); it is NOT currently downloaded (checked
   `steamapps/workshop/content/1054510/` 2026-07-04).
2. In the game's editor: create a mod named `SurvivalistMod` (or
   any name; pass -ModName), tick "Is Mod", add the Harmony 2.0.4
   dependency in Story Settings.
3. Run `survivalist-mod/scripts/build_and_deploy.ps1`.
4. Start a game with the mod active; tail the player log for the
   `[Unityforge]` lines.

---

## P0. Extract cross-game research testkit to modforge (2026-05-16)

> **Driver:** [`../horsey-mod/tests/README.md`](../horsey-mod/tests/README.md). The horsey-mod test directory has hardened a dual-purpose (research + assertion) test pattern over ~70 tests. Many primitives have zero Horsey-specific assumptions and should live in `modforge` so every native-PE mod target (horsey-mod, grounded2-mod, schedule1, outworld-station-mod, future games) gets the same toolkit. "Research this ONCE and reuse what we know."

### Game-agnostic primitives to extract

These currently live in `horsey-mod/tests/`; the cores have zero horsey-specific assumptions.

| Primitive | Source test(s) | Game-agnostic core |
|---|---|---|
| Region watcher | `watch_region.rs` | poll bytes at any (addr, len), diff vs previous, optional cmdlet trigger, optional `OFFSET:BEFORE->AFTER` assertion |
| Single-value watcher | `watch_until_change.rs` | poll u8/u32/u64 at any addr, log transitions, optional max-latency assertion |
| Op invoker / response contract | `dump_diag.rs` (now `dump_op`) | call any cmdlet, assert `result` or `error` present, optional field-path probe |
| MSVC x64 prologue recognizer | `dump_apply_gene.rs`, `find_retire_horse_handler.rs` | byte matcher for `48 89 5c 24`, `40 53`, `48 83 ec`, etc. Duplicated across 2 files today. |
| Function-entry verifier | `dump_apply_gene.rs` (now `verify_fn_entry_bytes`) | read bytes at any RVA, assert prologue |
| `build_info` contract | `dump_build.rs` | assert image_base/exe_path/exe_size shape |
| Xref-count scanner | `find_xrefs.rs` | thin wrapper over `mem.find_xrefs` |
| Asset-next-to-exe locator | `find_genes_xml.rs` (now `find_asset_next_to_exe`) | probe paths relative to exe_path |
| String-anchored fn locator | `find_retire_horse_handler.rs` (now `locate_fn_by_rdata_string`) | scan rdata -> find xrefs -> walk back past int3 padding -> match prologue |
| Heap-string scan + MSVC `std::string` parser | `dump_name_table_heap.rs` | scan heap for needle, parse 32-byte SSO + size + cap |
| Scan-data-find-struct recipe | `find_gamestate_via_money.rs` | scan .data for known u32 field value, verify by adjacent fields, rank |
| Function-bounds via int3 | planned in `modforge::research::find_function_bounds_via_int3` | walk back/forward through .text until `cc cc cc` padding |
| Generic binary research | already in `modforge::research` | scan, xref, decode (already shared via `research_*.rs` thin wrappers) |

### Game-specific (stays in horsey-mod)

Cmdlets specific to horsey (`horse.*`, `gamestate.*`, `hk1.*`) and shape-specific assertions (HORSE_ALLOC_SIZE=0x498, GS+0x438 chain, etc.). The horsey-specific tests stay where they are. The primitives they depend on are what we extract.

### Proposed modforge layout

```
modforge/src/testkit/
  mod.rs              re-exports, top-of-tree doc
  watch.rs            region + single-value watchers; env-config parser
  op.rs               op invoker + response contract
  fn_entry.rs         is_msvc_prologue, verify_fn_entry, fn_bounds_via_int3
  build_info.rs       build_info contract
  xrefs.rs            xref count assertion
  assets.rs           asset-next-to-exe finder
  recipes.rs          find_fn_by_rdata_string, find_struct_by_field_value
  msvc.rs             std::string parser, vtable-equality check
  snapshot.rs         write/read/diff bytes-at-addr (from hk1_dump_horse_bytes)
```

Each function takes `&RunningGame` + a config struct (env-parseable). Game-specific resolvers (`base_spec -> u64`) are passed in as closures so they stay game-side.

Per-game test file collapses to ~5 lines:

```rust
mod common;
use modforge::testkit::watch;

#[test]
fn watch_region() {
    let Some(game) = common::launch("watch_region") else { return };
    let env = watch::RegionConfig::from_env("HORSEY_WATCH");
    let cfg = env.resolve(|spec| common::resolve_base(&game, spec));
    watch::run_region(&game, &cfg).assert_or_panic();
}
```

### Tasks (ordered: lowest-risk -> highest-value)

- [x] **A1: `testkit::fn_entry`.** Shipped 2026-05-16. `is_msvc_x64_prologue` (with unit tests), `find_fn_bounds_via_int3`, `verify_fn_entry` config+runner. Deduped across `dump_apply_gene` + `find_retire_horse_handler`.
- [x] **A2: `testkit::msvc`.** Shipped 2026-05-16. `MsvcStdString` parser (with unit tests) + `is_vtable_at_image_rva` plausibility check.
- [x] **A3: `testkit::op`.** Shipped 2026-05-16. Generic op invoker + response contract + dot-path field assertion.
- [x] **A4: `testkit::build_info` + `testkit::xrefs` + `testkit::assets`.** Shipped 2026-05-16.
- [x] **A5: `testkit::watch`.** Shipped 2026-05-16. Region + single-value watchers, env-parseable, three-mode (assertion / discovery-trigger / manual).
- [x] **A6: `testkit::snapshot`.** Shipped 2026-05-16. `take` + `diff_against` for bytes-at-addr snapshots.
- [x] **A7: `testkit::recipes`.** Shipped 2026-05-16. `find_fn_by_rdata_string` + `find_struct_by_field_value`.

### Definition of done

- [x] Each primitive has unit tests in modforge (where pure): `fn_entry` and `msvc` ship with tests.
- [x] horsey-mod's test files for migrated primitives are ~10 lines each (launch + base resolver + call into modforge::testkit). All 11 thin-wrapper tests compile clean.
- [ ] **One additional consumer mod (grounded2-mod or schedule1) adopts at least one primitive to prove the abstraction works cross-game.** Not yet started; grounded2-mod has an unrelated pre-existing `layout.rs` E0432 to clear first.
- [ ] `modforge::testkit` documented in [`../modforge/docs/`](../modforge/docs/) (currently only module-level rustdoc).

---

## P0. Cross-game target registry (2026-05-16)

> **Design doc:** [`../modforge/docs/target-registry.md`](../modforge/docs/target-registry.md). Authoritative on the API + migration phases.

Successor to the now-shipped testkit extraction. Same principle ("research this once and reuse what we know"), applied to address resolution.

**Extends, does not replace.** `modforge::patterns::sleuth` already ships `Target<'a>`, `Resolution`, `resolve_all` as a transient one-shot API. This work promotes those types into a persistent, validator-aware registry. No new top-level module.

Every modforge consumer currently hand-rolls its own `src/targets.rs`: ~80 hardcoded RVAs + ~30 hand-written resolver functions + ad-hoc validators + per-mod R-tier test scaffolding. Universal shapes, only data differs. Standardize into a CRD-style registry following [`../modforge/docs/def-registry.md`](../modforge/docs/def-registry.md):

| Role | Existing in sleuth | After extension |
|---|---|---|
| Def | `Target<'a> { name, sigs }` transient | `TargetDef` `'static` with `kind`, `hint_rva`, `hint_tolerance`, `validators`, richer `Candidate { sig, recipe }` |
| Registry | (none; callers built `Vec<Target>` ad hoc) | `TargetRegistry { name, entries: &'static [&'static TargetDef] }` |
| Instance | `Resolution { by_name }` one-shot | `ResolvedTarget` cached per-entry in `Resolver` |
| Controller | `resolve_all(&[Target])` | `Resolver` (caches, runs validators); delegates the scan to the existing `resolve_all` |

Targets become declarative data; the 30 resolver functions collapse into 4 `Recipe` variants on `sleuth::Candidate`. Three shared R-tier tests in modforge replace today's per-mod `r2_*` / `r3_*` / `r4_*` files. Built-in validators (`HeapShape`, `InImage`, `VtableAtRva`, `FieldInRange`, `NestedPtrReadable`, `WithinHintTolerance`) close the HLT-3 / HLT-4 work items as a side effect.

Concrete LOC win: horsey-mod's `src/targets.rs` drops from ~2400 LOC to ~600 LOC of pure `TargetDef` data.

Composes with `modforge::seh` (just landed): every `seh::call` site looks up its target by name from the registry, so a build update that moves a vanilla function is one re-resolve away from being absorbed.

### Tasks (ordered: independently shippable, no big-bang cutover)

- [x] **B1: Extend `modforge::patterns::sleuth` in place.** Shipped 2026-05-16. `TargetDef`, `TargetRegistry`, `Resolver`, `ResolvedTarget`, 4 `Recipe` variants, 6 built-in validators added alongside existing `Target`/`Resolution`/`resolve_all` (unchanged). 8 new unit tests in sleuth::tests. Commit `639d8545`.
- [x] **B2: Build horsey-mod's registry beside the existing resolvers.** Shipped 2026-05-16. `horsey-mod/src/targets_registry.rs` declares **41 targets**: 7 data globals + 4 invocable function entries (with Signatures) + 30 hint-only function entries. `hint_only_fn!` macro keeps declarations terse. Parity integration test at `horsey-mod/tests/registry_parity.rs` asserts registry-resolved GAMESTATE_PTR matches the legacy resolver byte-for-byte. Commits `12dbcaeb`, `4b349650`.
- [x] **B3: Land shared R-tier tests in modforge.** Shipped 2026-05-16. `modforge::testkit::registry` exposes 4 shared assertion functions parameterized over `(&RunningGame, &TargetRegistry, &Resolver)`: `assert_every_target_resolves`, `assert_every_target_passes_validators`, `assert_every_field_offset_matches_hint`, `assert_diagnostic_includes_every_entry`. horsey-mod consumes via 3-LOC thin wrappers in `tests/registry_parity.rs`. Commit `26938d4e`.
- [/] **B4: Sig tuning + call-site migration.** Split into two stages:
  - [/] **B4a: Sig tuning.** 37 of 42 entries have real candidate sigs (88%); 5 still hint-only. Shipped 2026-05-16 via the `prologue_fn!` macro that lifts known-good 32-byte sigs from the deleted resolver module into the registry as `Recipe::MatchIsAddress` candidates. Real recipes: GAMESTATE_PTR (DecodeRipDisp), DEBUG_MODE_ACTIVE (PairedRipDispWithDelta), DEBUG_LOG_GATE (RipDispWithRelOffset), NAME_TABLE + CHROMOSOME_TABLE (Custom), 28 function entries (MatchIsAddress with live-derived prologue sigs), 4 invocable signatures. Remaining 5 hint-only: SAVE_VERSION_GLOBAL, RACES_COUNTER, NO_TIRE_TOGGLE, RETIRE_HORSE_HANDLER, BALLOON_CONTROLLER. Commit `536f5d33`.
  - [x] **B4b: Call-site migration.** All 24 sites migrated 2026-05-16 via the new `targets_registry::resolve` wrapper module. Last 4 (NAME_TABLE + CHROMOSOME_TABLE) landed via the new `Recipe::Custom` escape hatch. Commits `43363813`, `a35cdbca`.
- [x] **B5: Delete legacy `targets::resolve::*`.** Shipped 2026-05-16. `pub mod resolve` block deleted from `horsey-mod/src/targets.rs`. File dropped from 2507 LOC to 1286 LOC (-1221 LOC). Commit `a35cdbca`.
- [x] **B6: Cross-game adoption proof.** Shipped 2026-05-16. `grounded2-mod/src/targets_registry.rs` declares `GROUNDED2_TARGETS` with 5 UE5-Augusta globals (G_OBJECTS, G_NAMES, G_WORLD, APPEND_STRING, PROCESS_EVENT) mirroring `ueforge::ue::offsets::STEAM`. Pre-existing `grounded2-mod/tests/layout.rs` E0432 blocker cleared (test now imports types from `ueforge::ue::*`). Commit `903bb91c`.

### Definition of done

- [x] `modforge::patterns::sleuth` exposes `TargetDef`, `TargetRegistry`, `Resolver`, `ResolvedTarget`, validators, and Recipe variants. The existing one-shot API is unchanged. ~30 unit tests pass.
- [x] The three (now four) shared R-tier tests in modforge cover any consumer registry.
- [x] horsey-mod declares every public legacy RVA as a `TargetDef` (42 entries).
- [x] At least one additional consumer mod adopts a `TargetRegistry`. (grounded2-mod, B6.)
- [x] No `crate::targets::resolve::*` legacy resolver functions remain. (B5 closed, commit `a35cdbca`.)
- [/] horsey-mod's `src/targets.rs` is pure `TargetDef` data. Down from 2507 LOC to 1286 LOC after B5; remaining ~1286 LOC is `fn_addr`, `gs_offset`, `horse_offset` modules (field-offset resolvers that don't fit the registry shape today). Hitting the ~600 LOC target requires extending the Recipe enum further or accepting these as game-specific helpers; deferred.
- [ ] [`../modforge/docs/target-registry.md`](../modforge/docs/target-registry.md) updated from design-doc to shipped-API doc.

### How to pick up B4a (sig tuning)

The fastest path through B4a, per-target:

1. Pick an unmigrated target from the `hint_only_fn!` block in `horsey-mod/src/targets_registry.rs`.
2. Read the function's prologue in the live image. Either grep the existing decomp (`horsey-mod/research/decompiled/all_functions.c`) for the function name, or use `MODFORGE_RVA=<hint> MODFORGE_N=32 cargo test --test research_dump_bytes -- --nocapture` to print the first 32 bytes.
3. Author a `Candidate { sig: "...", recipe: Recipe::MatchIsAddress }` based on the 12-16 bytes of prologue, wildcarding any byte that looks build-variable (typically the disp8 of `mov [rsp+disp8], reg`).
4. Replace the `hint_only_fn!(NAME, RVA);` line with an explicit `static NAME: TargetDef = TargetDef { ..., candidates: &[Candidate { ... }], ... };`.
5. Run the parity test against a live game. If `every_target_in_registry_resolves` finds the same address the hint reports, ship.

Field offsets (the 9 remaining `H-gb-low` items from `horsey-mod/docs/todo.md`) are a separate per-anchor exercise documented there.

### Open questions (from the design doc)

- Validator type erasure: `&'static dyn Validator` vs closed enum. Probably closed enum for v1.
- Recipe extensibility: same trade-off.
- Cross-target dependencies (one resolver needing another's result). Document order matters initially; add `depends_on` field on `TargetDef` if topological resolution becomes necessary.
- `unityforge` / `ueforge` parallel: managed-runtime targets follow the same CRD shape but different Recipes (`UEObjectByName`, `MonoTypeByName`). Out of scope; design a parallel `ObjectRegistry` when needed.

---

## P0. Synthetic input: let the mod (and Claude) drive the game like a human (2026-05-16)

User-locked 2026-05-16. Lives in modforge because it applies everywhere: every native-PE mod target (horsey-mod, grounded2-mod, schedule1, outworld-station-mod, future games) needs the same primitive. Horsey is just the first consumer.

**Motivation.** For Claude to make real progress long-term, the mod must expose mouse + keyboard input as first-class HTTP cmdlets, so Claude (driving from a remote terminal) can interact with the game the way the user does at the keyboard. Clicks and drags are the priority. Keyboard second. The HK1 Shift+Click effort is currently blocked precisely because we can't synthesize a shift+click from a test; once this lands, that flow becomes `input.combo {keys: ["shift"], then: {type: "mouse.click", x, y}}` and the rest is whatever the game does naturally. Same for crispr lab, breeding pen, every UI panel, every future game.

**Three layers, ranked by reliability.**

| Layer | Mechanism | Reliability | When chosen |
|---|---|---|---|
| L1: Win32 `SendInput` | Synthesize OS-level mouse/key events globally | Lowest | Last resort; steals real mouse, requires game window focused, useless when user is at the machine |
| L2: `PostMessage` / `SendMessage` to game HWND | Targeted window messages (`WM_LBUTTONDOWN`, `WM_MOUSEMOVE`, `WM_KEYDOWN`, etc.) | Medium | Works without focus stealing; raw-input games ignore it; per-game verify |
| L3: Game-internal input poke / direct call | Write the game's own input-state buffers, or call the game's click-handler functions directly (vtable-style, like HK1 already does) | Highest | Requires RE per input surface; biggest payoff |

L3 is the long-term home. L2 is the right MVP if the game honors it. L1 only if both L2 and L3 are dead ends.

### Phase I-R0: GitHub prior-art research (heavy `gh` CLI; user-attention free)

> This is a LOT of `gh` work. Many people have shipped synthetic-input layers for game mods, bots, automation frameworks, and accessibility tools. We don't write a line of code until we've harvested what already exists. Output: a single markdown digest at [`../modforge/docs/input-prior-art.md`](../modforge/docs/input-prior-art.md) ranking candidates by fit + license + maintenance.

Per-axis search plan (all via `gh search code` / `gh search repos` + `gh api`):

- [x] **PR-1: Game-mod input libraries.** Done 2026-05-16. Top finds: UnrealCV (HTTP-driven UE input via `PlayerController->InputKey`), Lyra's `LyraSimulatedInputWidget`, `ekicyou/areka` PostMessage example. Confirmed L1/L2/L3 split.
- [x] **PR-2: UI automation frameworks.** Done 2026-05-16. Pywinauto's `click()` (PostMessage) vs `click_input()` (`mouse_event`) split is the model we adopt. AHK's `dwExtraInfo`-tag pattern noted. AHK GPL-2 forbids vendoring; ideas only.
- [x] **PR-3: Game-bot ecosystems.** Done 2026-05-16. RuneLite's `Callbacks` event interface mirrors our `Hook<F>` retour pattern; humanize-curve deferred to v2.
- [x] **PR-4: Reverse-engineered input pokes per engine.** Done 2026-05-16. Canonical UE L3 = `APlayerController::InputKey(FInputKeyParams)` (Lyra + UnrealCV both confirm). Unity = `Input.GetKey*` hook or `InputSystem.QueueStateEvent`. Raw Win32 = per-game I-R.
- [x] **PR-5: Raw-input / `WM_INPUT` handling.** Done 2026-05-16. Interception driver (1849 stars, abandoned 2021, no Rust binding) is the escape hatch. Defer until a target needs it; all current targets are L1/L2/L3 reachable.
- [x] **PR-6: Accessibility / assistive tech.** Done 2026-05-16. Validates L1 default + "restore foreground window post-batch" pattern.
- [x] **PR-7: Crate audit.** Done 2026-05-16. **Winner: `enigo` 0.6.x (MIT, 1715 stars, edition 2024, last commit 2026-03-30).** Use `default-features = false`. No existing crate covers L2; hand-roll over `windows-sys`.
- [x] **PR-8: In-process injection precedents.** Done 2026-05-16. UE4SS routes input via UE's own structures (no SendInput/PostMessage in its tree). BepInEx (LGPL-2.1) + MelonLoader (Apache-2) are the Unity references. Special K is GPL-3 (cite only).
- [x] **PR-9: Replay / record-playback formats.** Done 2026-05-16. Adopted JSON event stream with absolute `t_ms`, `type` enum, `backend` selector. Inspired by xdotool + pywinauto.
- [x] **PR-10: Anti-cheat / legitimacy notes.** Done 2026-05-16. All current targets (Horsey, Grounded 2, Schedule 1, Outworld Station, Timberbot) LOW risk. `dwExtraInfo` tag + foreground restore as cheap insurance.

**Recommendation on file** in [`../modforge/docs/input-prior-art.md`](../modforge/docs/input-prior-art.md): L1 = `enigo` (default-features off), L2 = hand-rolled `modforge::input::win32_message` over `windows-sys` (~80 LOC), L3 = per-consumer `InputSurface` trait. Module + cmdlet shapes drafted. I-1 is unblocked.

Deliverable: [`../modforge/docs/input-prior-art.md`](../modforge/docs/input-prior-art.md) with one section per PR-N above, a comparison table (top 5 candidates), and a recommendation for which L1 backend (likely `enigo` or hand-rolled `SendInput`), L2 strategy (PostMessage wrapper), and L3 patterns (per-engine).

### Phase I-R: per-game research (game-specific, NOT in modforge)

After I-R0 picks the shape, each consumer mod does its own I-R steps locally. For Horsey:

- [ ] **I-R1: Decomp grep for input read sites** in `research/decompiled/all_functions.c`: `GetCursorPos`, `GetAsyncKeyState`, `GetKeyState`, `SDL_*`, `XInputGetState`, `RAWINPUT`, `PeekMessage` / `DispatchMessage`. Tells us which OS surface Horsey uses; decides L1/L2/L3 viability.
- [ ] **I-R2: Locate mouse-state globals** (expand from `hk1.probe.mouse_globals` / `hk1.read_cursor`). Pattern-resolve via `targets::resolve::mouse_state()`.
- [ ] **I-R3: Locate keyboard-state globals.** Likely a 256-byte VK array next to `GetAsyncKeyState` call sites.
- [ ] **I-R4: Identify the click dispatcher.** HK1 found `FUN_1400de2e0` for drop-commit; need the equivalent for "any-click."
- [ ] **I-R5: PostMessage smoke** (~5 min live): does `PostMessage(hwnd, WM_LBUTTONDOWN, ...)` trigger an in-game click? Binary answer; decides L2.

Same I-R checklist instantiated per future game.

### Phase I: implementation (modforge core)

- [x] **I-1: `modforge::input` primitives.** Shipped 2026-05-16 commit `e344c5dd`. Decided AGAINST adopting `enigo` after Cargo.toml inspection (would pull `windows` crate alongside our existing `windows-sys`); hand-rolled L1 over `windows-sys` instead (~150 LOC, zero new deps). `Backend` enum (L1/L2), `Button`, `Key::parse` (friendly names + hex + f1-f24 + letters/digits), `InputSurface` trait stub, `SYNTHETIC_EXTRA_INFO` tag for self-event recognition. L1 in `l1.rs` (SendInput + virtual-desktop normalized coords). L2 in `l2.rs` (PostMessage with MK_* modifier mask, client-area coords).
- [x] **I-2: HTTP cmdlets.** Shipped 2026-05-16 commit `e344c5dd`. 7 ops: `input.mouse.{move,click}`, `input.key.{down,up,press}`, `input.cursor.get`, `input.foreground.hwnd`. Mods array `["shift","ctrl"]` supported on `mouse.click`. Drag/scroll/combo/replay deferred to next slice.
- [x] **I-5: Test harness (first slice).** **SMOKE GREEN 2026-05-16.** `horsey-mod/tests/input_smoke.rs` passes against a live Horsey: L1 cursor round-trip (within 1px), L1 keyboard F24 press, L2 PostMessage WM_MOUSEMOVE to Horsey HWND. Earlier failure was a pre-existing sleuth resolver bug at `sleuth.rs:613` (mask too wide for pre-rebased-VA hints; off by `0x40_00_00_00`); fixed in place to handle the conventional x64 PE preferred base `0x140000000`. Sleuth unit tests still green.
- [ ] **I-3: Coordinate spaces.** Resolve `hwnd`, canonical = client-area px; conversions inside. Partly done in L2 (already uses client-area px); needs explicit `input.client_to_screen` + DPI awareness.
- [/] **I-4: L3 game-internal pokes** (per surface, opt-in via the per-game `InputSurface` trait).
  - **L3 routing shipped** in modforge (`Backend::L3` variant + `set_input_surface` / `input_surface` slot + per-cmdlet `l3_or_fallback`).
  - **HorseyInputSurface v1 shipped** 2026-05-16. Writes `LOC+0x174`/`+0x178` cursor floats directly via the existing `hk1::home_loc_ptr()` machinery. `screen_to_client` conversion via `find_hwnd_by_pid(self_pid)` + `ScreenToClient`. Buttons + keys delegate to L1 (engine polls `GetAsyncKeyState` which L1 `SendInput` updates). Graceful degrade when no save loaded.
  - **Smoke green:** L3 cursor move tracks target within 1px against running Horsey at main menu (L3 -> L1 fallback path). LOC-float verification waits on an in-save run of `input_hk1_calibration`.
  - Deferred (v2): direct mouse-state struct writes via `FUN_14018c5c0()` accessor (needs R2 sig-tune); direct keyboard buffer writes via `FUN_140183330(0)+0xe1` (modifiers) and `FUN_140183990` (general keys). See [`../horsey-mod/docs/input-surface.md`](../horsey-mod/docs/input-surface.md).
- [ ] **I-6: Replay format.** JSON event stream `[(t_ms, event, args)]`; record, save, replay. Shape decided by PR-9 winner.

#### Follow-ups discovered during I-1/I-2

- [x] **I-2a: `input.find_hwnd_by_pid {pid}` + `input.self.hwnd` ops.** Shipped `4f6c6dda`. EnumWindows-by-PID with visibility filter; `self.hwnd` is the in-process shortcut for cmdlets running inside the game.
- [ ] **I-2b: `input.state.get` op.** Reads game's own mouse/keyboard state for verification (so smoke tests don't trust their own writes). Needs per-game `InputSurface` impl; defer until horsey-mod has one.
- [x] **I-2c: Drag/scroll/combo cmdlets.** Shipped 2026-05-16. `input.mouse.drag` (L1+L2, interpolated steps), `input.mouse.scroll` (L1+L2, vertical+horizontal), `input.combo` (modifier hold + dispatch inner op, LIFO release on success or failure). Live smoke green: drag lands within 1px, scroll + combo succeed.
- [/] **I-2d: HK1 Shift+Click migration.**
  - **I-2d-recon shipped** 2026-05-16. `horsey-mod/tests/input_hk1_calibration.rs` captures the mapping between OS screen pixels (what `modforge::input` sends) and the Home Location's cursor floats (`LOC+0x174`/`+0x178`, what `FUN_1400d2ab0` reads). 5-point plus pattern around baseline. Test ran clean against a no-save process and correctly reported "LOC cursor unreadable at baseline; likely no save loaded." Re-run with a save active to capture the actual transform.
  - **I-2d-impl: pending.** Once recon gives the screen->game-coord ratio, the migration is: `input.combo {keys: ["shift"], then: {op: "input.mouse.drag", args: {from_screen_px, to_screen_px}}}`. If the ratio is 1:1, no math needed. If not, modforge ships a tiny `input::map_screen_to_game(transform)` helper. The post-vtable-helper RE work in [`../horsey-mod/docs/todo.md`](../horsey-mod/docs/todo.md) "HK1 Shift+Click smart-transfer" goes away regardless.

### Definition of done

- [ ] [`../modforge/docs/input-prior-art.md`](../modforge/docs/input-prior-art.md) covers PR-1..PR-10 with a recommendation.
- [ ] `modforge::input` ships L1 + L2 backends, `Mouse`, `Keyboard`, `InputSurface` trait.
- [ ] `input.*` HTTP cmdlets exposed in horsey-mod at attach.
- [ ] Claude can run `input.mouse.click {x, y}` from a remote terminal with the game running on the user's machine and the game responds as if the user clicked there.
- [ ] Chord clicks: `input.combo {keys: ["shift"], then: ...}` registers as shift+click.
- [ ] At least one end-to-end test exercises a click-driven flow ("open Horses tab, click row 3, assert detail panel opens").
- [ ] `input.state.get` reads back the game's own state so tests don't trust their own writes.
- [ ] At least one second consumer (grounded2-mod or schedule1) wires up `InputSurface` for cross-game proof.

### Out of scope (defer)

- Controller / gamepad input (no current need).
- Recording user-side input for replay (separate hook problem).
- Cross-process input on obscured / minimized windows. Validate as side effect; don't engineer for.

### Open questions (resolve during I-R0 and I-R)

- Raw-input (`WM_INPUT`) games: PostMessage useless for cursor movement, only GDI cursor moves. L3 mandatory.
- Per-frame snapshot vs scattered reads inside `GameState`. Affects L3 surface size.
- DPI / fullscreen coords: physical vs logical px. Decide at I-3 from PR-2 norms.

---

## P0. Vanilla function invocation (2026-05-16)

> **Design doc:** [`../modforge/docs/vanilla-invoke.md`](../modforge/docs/vanilla-invoke.md). Authoritative on the API + migration phases.

Third primitive in modforge's mod/game interaction triad: `hook` intercepts vanilla calls (shipped), `seh` makes vanilla calls recoverable on fault (shipped), `vanilla` invokes vanilla functions from the mod side (this work). Rides on the target-registry's `TargetKind::FunctionEntry { signature }` field; the asm dispatcher is independent.

### Tasks (independently shippable; foundation already complete)

- [x] **V1: Land `modforge::vanilla` primitives.** Shipped 2026-05-16. `Signature`, `ArgKind`, `RetKind`, `ArgValue`, `RetValue` (sig.rs); Win64 ABI dispatcher as a `global_asm!` external function with a stable `void(Frame*)` signature (dispatch.rs). 13 unit tests including the 8 dispatcher cases (register args, stack overflow, pointer deref, two-f64 add, mixed int/float/int, void return, arg-count/kind validation). All pass against `extern "system"` Rust test functions. Commit `639d8545`.
- [x] **V2: Extend `sleuth::TargetDef` with optional `Signature`.** Shipped 2026-05-16 (part of B1). `TargetKind::FunctionEntry { signature: Option<&'static Signature> }`. Backward-compatible.
- [x] **V3: `Invoker` controller + `vanilla.invoke` / `vanilla.list` HTTP cmdlets.** Shipped 2026-05-16. `Invoker::call` is SEH-wrapped by default; `call_unsafe` opt-out for hot paths. The HTTP cmdlets accept `{target, args, safe?}` and return `{ok, ret, elapsed_us}` or `{ok:false, error}`. 5 unit tests cover JSON arg parsing + return encoding + signature serialization. Commit `12dbcaeb`.
- [x] **V4 (data declaration): Signatures attached to 4 horsey functions.** Shipped 2026-05-16. `APPLY_GENE_TO_HORSE`, `HORSE_REBUILD`, `RNG_NEXT_MODULO`, `HORSE_COPY_GENE_LANE_PAIRS` all carry `signature: Some(...)` in the horsey registry. `worker_main` calls `targets_registry::register_vanilla_ops()` so the cmdlets wire up against `HORSEY_RESOLVER` at attach. Commits `26938d4e`, `4b349650`.
- [ ] **V4 (validation): Confirm the 4 signatures work against the live game.** Once the game can be launched: `curl POST /op vanilla.list` should enumerate the 4; `vanilla.invoke {target: "RNG_NEXT_MODULO", args: [{kind: "u32", value: 100}]}` should return a u32 in `[0, 100)`. Needs a live-game session; ~15 min.
- [ ] **V5: Migrate any horsey-mod call sites that invoke vanilla code.** Survey of `transmute` use in horsey-mod's `patches/*.rs` shows most are inside detour handlers chaining back to the original function via retour's trampoline pattern, NOT direct vanilla calls. So V5's actual scope is much smaller than originally estimated. Specific candidates to migrate (each ~5 LOC replaced by 1 `vanilla::call`):
  - any place that wants to call `FUN_1400b3070` (rebuild) after a gene poke
  - any place that wants `FUN_1400c6580 % N` for an arbitrary-modulo RNG
  - any new feature work that invokes vanilla rather than hooking it
- [ ] **V6: Cross-game adoption.** Same target as B6: when grounded2-mod (or schedule1) ships their first `TargetRegistry`, registering one function with a `Signature` proves the abstraction works across games.

### Definition of done

- [x] `modforge::vanilla` ships `Signature`, `ArgValue`, `RetValue`, `VanillaFn`, `Invoker`, the Win64 asm dispatcher. Unit tests covering each `ArgKind` / `RetKind`.
- [x] `vanilla.invoke` and `vanilla.list` cmdlets shipped in the modforge HTTP surface and wired into horsey-mod at attach.
- [x] horsey-mod has 4 vanilla functions registered with signatures.
- [ ] At least one production call site goes through `Invoker::call_safe` (vs `transmute`). **Pending V5.**
- [ ] At least one additional consumer mod registers a `Signature` and invokes through `Invoker::call_safe`. **Pending V6.**
- [ ] [`../modforge/docs/vanilla-invoke.md`](../modforge/docs/vanilla-invoke.md) updated from design-doc to shipped-API doc.

### Open questions

- Non-primitive arg marshalling (passing fresh-allocated buffers). v1 supports pointers as raw u64; v2 could add `ArgKind::Buffer { bytes, mode }`. Defer.
- Return-by-stack-pointer (large structs). v1 doesn't support; defer to `RetKind::Buffer { bytes }`.
- Variadic / async. Skip; no current consumer.

---

## Future: absorb the C# mod repos (Timberbot + Schedule 1)

User direction 2026-05-14: this repo is the home for ALL
game modding. The two outstanding C# repos eventually move
in:

- [`abix-/TimberbornMods`](https://github.com/abix-/TimberbornMods).
  Timberborn (Unity, IL2CPP). C# with Bindito DI +
  publicizer + Timberbot. Sister Python client.
- [`abix-/Schedule1Mods`](https://github.com/abix-/Schedule1Mods).
  Schedule 1 (Unity, IL2CPP). C# with MelonLoader +
  Harmony + Il2CppInterop.

This is a bigger lift than the horseygame subtree-merge
because the Rust forge model doesn't fit C#-only mods
end-to-end:
- Cargo workspace can't host C# projects directly. Either
  the C# stays in `<game>-mod/csharp/*.csproj` outside the
  workspace manifest with a separate build script, or we
  add a top-level `dotnet/` directory next to the Rust
  crates with its own `.sln`.
- The IL2CPP forge path (`unityforge` + Il2CppInterop) is
  the natural Rust analogue, but Timberbot + Schedule 1
  already use mature C# stacks (Bindito / MelonLoader)
  with their own ergonomics. Forcing them onto unityforge
  would be a rewrite, not an integration. Likely we keep
  them C# and just colocate the source.
- Build / deploy: Timberbot has `build.py`, Schedule 1 has
  its own MelonLoader pipeline. Both need to keep working
  after the move.
- Skills (`~/.claude/skills/timberborn`, `timberbot`,
  `schedule1`) reference the source repos by URL; those
  pointers update on the move.

When the move happens, decide between:
1. **Subtree-merge** (preserves history, mirror of how
   horseygame landed). One subtree per source repo:
   `dotnet/timberbot/`, `dotnet/schedule1/` (or kept under
   their existing repo structure inside this one).
2. **Fresh import** with a one-shot history snapshot. Less
   git overhead, loses authoring history.

Strong default: subtree-merge, same as horseygame.

Not pursuing now. Schedule for after the horsey-mod
`sleep_safe_no_tire` patch is unstuck.

---

## RE-to-Rust pipeline (`decomp`, exploratory archive)

Lives at [`../decomp/`](../decomp/). r2sleigh-based
binary-to-Rust decompiler.

**Status 2026-05-14: parked**. Honest assessment in
[`../decomp/docs/retrospective.md`](../decomp/docs/retrospective.md):
Ghidra C decomp at
[`../horsey-mod/research/decompiled/all_functions.c`](../horsey-mod/research/decompiled/all_functions.c)
already solves the actual RE workflow; `decomp`'s Rust
syntax is cosmetic and costs more than it returns. The
crate works as a spike; ladder items in
[`../decomp/docs/polish-ladder.md`](../decomp/docs/polish-ladder.md)
are real but unlikely to ship without a specific
forcing function.

If you need Rust syntax for a specific function: paste
the Ghidra C into Claude with "rewrite as unsafe Rust
pseudocode, keep semantics". 30 seconds, no
infrastructure. Beats `decomp`'s output on any given
function.

Sample artifacts kept at
[`../horsey-mod/research/decompiled/rust-r2sleigh/`](../horsey-mod/research/decompiled/rust-r2sleigh/)
as proof-of-concept evidence. Migration history at
[`../falcon-printer/docs/strategy.md`](../falcon-printer/docs/strategy.md).


---

## P0. Unityforge: generation-versioned hot reload (Phase 4 follow-ups)

Phase 4 shipped 2026-05-13 (see [`changelog.md`](changelog.md)).
Follow-ups not blocking the reload cycle:

- [ ] **Handle-table namespace per generation.** Currently
  shared across generations via `MonoBridge._handles`.
  Old gen's still-held handles are stale references after
  swap. Harmless in practice (old gen has shut down +
  exited threads) but worth high-bit-encoding the
  generation if collision matters later.
- [ ] **Periodic GC of quiesced generations.** The
  `_quiesced` list grows forever in the current shim. Once
  a gen's threads have exited, we could free its
  GCHandle. Detecting "threads exited" reliably without
  hard joins is tricky; defer.
- [ ] **IL2CPP shim also needs the gen-loader.** Only Mono
  shim got rewritten. Mirror the change into
  `cs-shim-il2cpp/Plugin.cs` before shipping IL2CPP smoke.

## P1. wwm-mod: complete the Mono proof (Phase 3a remainder)

Phase 3a milestones shipped 2026-05-13 (see
[`changelog.md`](changelog.md)). Remaining:

- [ ] **Ship `UnitySkillProxyEffect`** + repoint Strong Back
  at the game's built-in `Bag` skill. Major finding from the
  2026-05-13 session: the game ships `SkillsManager` with a
  full RPG API (`SetSkillLevel`, `LevelUpSkill`,
  `GetCurrentSkillLevel`, etc.) and four built-in skills
  (`Bag`, `Energy`, `Rope`, `Speed`). Calling
  `SetSkillLevel("Bag", 4)` via `invoke_method` grew the
  player's backpack from 5 to 12 slots live in-game (no save
  reload). The right framework shape is a proxy Effect that
  maps our level to a game-skill level and lets the game
  handle the live mutation. See
  [`wwm-mod/docs/research.md` §7.5](../wwm-mod/docs/research.md#75-in-game-findings-2026-05-13-session).
- [ ] **Research Charisma + Resilient mappings**. Neither has
  an obvious built-in game-skill match. Either find a custom
  proxy through the existing four (Energy regen for
  Resilient?) or write a raw-field Effect against
  `GameDataSO._jetpackEnergyConsumeAmount` /
  `_moneyEnergyRestoreCost`. The latter needs a one-hop
  indirection Effect (singleton -> SO field -> target field)
  since GameDataSO is reached via `GameplayManager._gameData`.
- [ ] **Verify Greedy Miner in-save**. `MineDataSO` has zero
  instances at main menu (SOs instantiate with a save). The
  game has no built-in skill for ore value either, so this
  is the one case where raw-field Effects on
  `MineDataSO._oreValue` (verified field name TBD) is the
  right path. Defer to an in-save test session.

---

## P0. In-game smoke test

The full 2026-05-10 wave is build-clean and unit-tested but not
yet validated in-game.

!!! warning "Acceptance gate"
    Drive from your machine. All checks must pass:

    - Game launches; `grounded2_mod.log` shows `ueforge` init lines.
    - ImGui tab opens on `Insert`; RPG content renders.
    - Load a save -> slot activate fires; tracker loads
      `<guid>.json`; backpack capacity reflects skill levels.
    - Kill a creature -> kill_hook fires; XP increments; save updates.
    - HTTP `/debug` endpoint responds to `op:snapshot` with non-empty
      state.

    If any of that fails, triage from the log lines.

---

## P0. DataTable write surface (remaining)

The Phase 1 / Phase 2 surface shipped (see
[`../ueforge/docs/data-table.md`](../ueforge/docs/data-table.md)).
Two open follow-ups gated on real consumers:

- [ ] **Replicated-field respect**. Some row fields are
  replicated to clients; writes during a server's authority
  drop get desynced. Defer until we hit a real case.
- [ ] **Non-primitive field types**. Writing `FString` /
  `TArray` rows needs the engine's `FMemory` allocator (ABI
  work). Defer until a real consumer needs it.

## P1. Registry alignment (remaining)

- [ ] **`simulate_apply_damage` lift** (gated on Wave E1).
  `ApplyDamageFromInfo` from inside any current PE trampoline
  re-enters ProcessEvent through the engine's damage-replication
  path and crashes the host. Lift to `ueforge::rpg::health` once
  the safe drain site lands.

### Deferred (large lifts)

- [ ] **Parm decoders**. Every PE-hooking module knows
  "for UFunction X, the parm block has shape Y". Currently
  scattered across kill_hook / inv_hook / fall_hook as bespoke
  code. Could lift to `ParmDecoderDef { function_name, decode_fn
  }` + a per-class registry. Biggest payoff: a generic
  `walk_parms` debug op. Defer until a second consumer needs
  it.
- [ ] **`ClassRef` registry**. Every declared `static FOO:
  ClassRef = ClassRef::new("Foo")` could feed a workspace-wide
  `class_refs_list` op. Marginal; defer.

## P1. Ueforge durability (kovarex review wave 2, remaining)

Wave 2 keepers (patternsleuth, zerocopy first wave, schema
versioning, SAFETY annotation status, crate-shopping verdicts)
shipped; see [`../ueforge/docs/architecture.md`](../ueforge/docs/architecture.md)
"Wave 2 durability adoptions".

### Pending crate adoptions (ordered by gain)

Sorted by leverage, not alphabetical. Maintenance status
confirmed via `gh api` 2026-05-12.

1. **`zerocopy`** (IN PROGRESS). First wave shipped; still open
   are the dynamic-offset / dynamic-type sites where one zerocopy
   struct doesn't fit all callers:
   - `damage/mod.rs::on_event` (offsets configured at runtime
     via `DamageHookConfig`).
   - `data_table.rs::decode_field` (T chosen at runtime by
     FProperty class string).
   - kill_hook / fall_hook / inv_hook parm decoders (each could
     define one `#[repr(C)]` + zerocopy struct per UFunction
     shape; ~6 structs total).
   - `FDataTableRowHandle` (has a `*const UObject`; pointer
     rejects FromBytes; would need a split type).

2. **`proptest`** (IN PROGRESS). MEDIUM-HIGH gain. Property
   tests landed on TArray garbage headers, TMap
   `find_value_by_fname_key` correctness, and `Val::from_json`
   round-trips for all 10 scanner value types. Still open:
   FieldTweak decoder + `inspect_address` byte slabs.

3. **`insta`** (IN PROGRESS). MEDIUM gain. Wired as dev-dep
   with four envelope-shape snapshots in `envelope.rs`
   (`OpResponse::ok` / `err` / `from_result` x ok/err). Run
   `cargo insta review` to accept future shape changes. Still
   open: per-op snapshots for the standard debug-op set
   (`skill_toggle` / `spend` / `refund` / etc.). Gated on
   building a stateless test fixture for those.

### Open durability follow-ups

- [/] **Annotate the existing unsafe blocks**. Down to 50 from
  271; the remaining 50 are spread thinly across `ue/probe.rs`,
  `ue/status_effect.rs`, `ue/class_tweak.rs`, `ue/core_types.rs`,
  `ue/typed_field.rs`, `ue/fname.rs`, `ue/platform.rs`,
  `ue/player.rs`, `discovery.rs`, `selector.rs`,
  `damage_info.rs`, `pe_call.rs`, `parms.rs`, `fstring.rs`, and
  single-block files. Chip away during normal edits; bump the
  lint to `deny` when the count reaches zero.
- [ ] **Hot-reload torture test**. 1000x Ctrl+R loop with hooks
  installed; assert no thread leak, hook leak, slot regression.
  Path is "tested once" today. Requires running game; gated on
  in-game test harness.

## P2. Ueforge grooming (remaining)

- [ ] **UE-version-aware `ffield` / `fproperty` / `ustruct`
  offsets**. Hardcoded for UE 5.4 in `ue/offsets.rs`. UE 5.5+
  silently returns wrong names. Pair with the sig-scan work.
- [/] **Property tests on walkers**. Boundary tests shipped (5
  on `tarray`, 7 on `tmap`). Proptests landed on TArray
  (garbage headers), TMap (find_value correctness), and
  `Val::from_json` (10 types). Still open: FieldTweak
  decoder + `inspect_address` byte slabs.
- [ ] **`tiny_http` / `ureq 2` migration window**. Both on a 2-5
  year support horizon.
- [ ] **PE hook trampoline linear search**. Index by vtable
  pointer when 6+ hooks are installed.

## ueforge framework. Wave E (deferred until needed)

- [ ] **Global ProcessEvent pre-callback**.
  `RegisterProcessEventPreCallback` wrapper +
  `Queue::install_drain` helper. Land when status-effect
  migration needs a guaranteed drain site.
- [ ] **`AddUObjectCreateListener` integration**. Land when
  ows-tweaks or g2rpg's CDO-revert-replay scenario asks. ~100 LoC.

## ueforge framework. Still-open extractions

- [ ] **Leak-source helpers**. `explore_leak_source.rs` (350
  lines) uses g2-extension `top_packages` / `loaded_levels` /
  `process_regions` fields not in the framework's
  `gobjects_population` helper today. Lift those as separate
  `ueforge::ue::probe::*` extensions when a second consumer
  needs them.

Deferred extractions (no second consumer yet):

- Damage-hook framework scaffold beyond `DamageBinder`. Maine-
  specific KillerKind / FDamageInfo parsing stays game-side.
- Status-effect discovery enumerators (`fall_hook` row-walking).
  `StandardEffect::StatusEffect` covers the apply path.
- HUD overlay drawing (DX11 hook for in-world rendering). Big
  lift, no second consumer.
- Inventory CRUD ops (add / remove / count / list). Module shape
  exists; CRUD lands when a second game's UFunction names drive
  the trait surface.

---

## ueforge buildings module (planned, not started)

The next opinionated module. Lets a mod add custom content
(buildings, structures, placeables) to a UE5 game via UE4SS,
not via paks. Player-placeable buildings that yield resources,
tick logic, persist across saves, integrate with the host
game's build menu where one exists.

The sixth module after `rpg / stacks / difficulty / inventory /
damage`. Splits framework from game-specific surface: future
mods ship a new building with ~30 lines of catalog config + ~50
lines of game-side wiring.

### What's universal (-> `ueforge::buildings`)

| Surface | Why universal |
|---|---|
| `BuildingDef<S, C>` catalog row. Id + display_name + description + default per-instance state `S` + game-specific config `C` | Every game's mod-added buildings have the same shape: a typed catalog. Same pattern as `rpg::Skill<E>`. |
| `BuildingRegistry: &'static [BuildingDef<S, C>]` + lookup-by-id | One source of truth, same as RPG catalog. |
| Per-instance state store (`HashMap<Handle, S>`) | Every game tracks "what state is this placed building in". Stored items, level, last-tick timestamp. Shape `S` is game-supplied. |
| Per-slot JSON persistence (`<DLL_dir>/buildings/<playthrough-guid>.json`) | Same `SlotStore<S>` pattern from `ueforge::rpg`. Reuse, don't reinvent. |
| Tick scheduler (closure fires per-instance every `interval`) | Auto-farm, regen, decay. All "advance state by dt" patterns. Owned by the framework; rides on the existing `SlotPoller` shape. |
| Standard debug ops (`buildings_list` / `buildings_place` / `buildings_destroy` / `buildings_get` / `buildings_set_state`) | Same shape as `rpg::ops`. Universal testing surface. |
| ImGui tab template (catalog + placed-instance list + per-instance state viewer) | Same shape as `rpg::tab::render`. |

### What's game-specific (-> game crate via trait)

| Surface | Why game-specific |
|---|---|
| The UE5 game's build-menu data table / widget | Each game has its own UI; some have no native build mode at all. |
| Spawning a placed instance (BP class name + spawn UFunction + cost-charge logic + location/rotation parms) | Per-game BP classes + per-game economy system. |
| Resolving a handle to a live UObject for state reads | Per-game inventory / storage class shapes. |
| Yield-to-storage logic (find nearest storage, transfer items via game's inventory API) | Per-game storage class + per-game item-transfer UFunction. |
| Net authority / replication (which side owns yields) | Per-game multiplayer model. |

The seam: a `BuildingSpawner` trait similar in shape to
`DamageBinder` / `ViewportBinder`.

### Module surface (proposed)

??? example "Proposed Rust module surface"

    ```rust
    // ueforge::buildings (planned, not yet implemented)

    pub struct BuildingDef<S, C> {
        pub id: &'static str,
        pub display_name: &'static str,
        pub description: &'static str,
        pub default_state: S,
        pub config: C,
    }

    pub trait BuildingSpawner: Send + Sync + 'static {
        type Config: Copy + 'static;
        type Handle: Eq + std::hash::Hash + Clone + Send + 'static;

        fn spawn(&self, cfg: &Self::Config, loc: WorldLocation)
            -> Result<Self::Handle, String>;

        fn destroy(&self, handle: &Self::Handle) -> Result<(), String>;

        fn register_in_build_menu(&self, buildings: &[(&'static str, &'static str)])
            -> Result<(), String> { Ok(()) }
    }

    pub struct BuildingsTracker<S, C, Sp: BuildingSpawner> {
        catalog: &'static [BuildingDef<S, C>],
        spawner: Sp,
        instances: Mutex<HashMap<Sp::Handle, Instance<S>>>,
        store: SlotStore<PersistedState<S, Sp::Handle>>,
    }

    pub struct Instance<S> {
        pub building_id: &'static str,
        pub state: S,
        pub spawned_at: SystemTime,
        pub last_tick: Option<SystemTime>,
    }
    ```

Plus `ueforge::buildings::ops` (standard debug-op set),
`ueforge::buildings::tab::render` (ImGui template), and
`ueforge::buildings::tick::SchedulerHandle` (riding the
SlotPoller shape).

### Phased plan

#### Phase A: research (no code)

- [ ] **A1: Grounded 2**. Find the build system entry points
  via the SDK. Likely candidates: `UProductionBuilding`,
  `BP_Building*`, `UBuildingPlacementComponent`,
  `UCraftingComponent`. Identify which UClass defines a
  "placeable building", which UFunction spawns one, how the
  cost is charged, whether there's a build-menu data table to
  extend, how instance state replicates / persists, and the
  save shape.
- [ ] **A2: Outworld Station**. Same audit. The trait surface
  needs to fit both.
- [ ] **A3: Storage / inventory transfer**. For auto-farming
  to land items into a chest, identify the game's "add item to
  inventory" UFunction.

#### Phase B: design lock-in

Trait surface against the two games' constraints. Pick the
handle type (UObject address vs mod-generated FGuid), storage
location, tick model.

#### Phase C: framework implementation

- [ ] C1: `ueforge::buildings::BuildingDef<S, C>` + catalog row.
- [ ] C2: `BuildingsTracker<S, C, Sp>` with `activate_slot` /
  `deactivate_slot` + persistence via `SlotStore`.
- [ ] C3: `BuildingSpawner` trait + game-impl skeleton.
- [ ] C4: Tick scheduler.
- [ ] C5: `ueforge::buildings::ops` standard debug ops.
- [ ] C6: `ueforge::buildings::tab::render` ImGui template.

#### Phase D: g2rpg consumer (proof of concept)

- [ ] D1: One concrete buildable. The
  **auto-fiber-harvester** (yields plant fibers at a configured
  rate, per-instance `{accumulated_units, last_yield}` state,
  transfers to nearest storage on threshold).
- [ ] D2: Catalog row in `grounded2-mod/src/buildings/catalog.rs`.
- [ ] D3: G2 spawner impl.
- [ ] D4: ImGui "Buildings" tab.

#### Phase E: tests

Pester-style scenario DSL extended for buildings (e.g.
`scenario::for_building(api, "auto_harvester").placed_at(loc).advanced_time(60s).should_yield_at_least(10, "plant_fiber")`).

### Open design questions (decide during Phase A/B)

- **Handle type**. UObject address (stable while game is running,
  invalid across saves) vs mod-generated FGuid (stable across
  saves, requires translation). Likely: FGuid for persistence +
  separate live UObject cache.
- **Asset references**. v1 reuses vanilla assets (less moving
  parts); pak-side custom assets land later.
- **Build-mode integration**. v1 ships ImGui-only spawn (lower
  risk). Vanilla-menu integration is Phase D+1.
- **Multiplayer**. Out of scope for v1. Single-player / host-only.
- **Tick clock**. Game-time vs real-time. Save-and-quit
  shouldn't run the farm for 8 hours when you reload. Use
  game-time OR clamp dt to `min(real_dt, configured_max_dt)`.
- **Network authority**. Yields go through the game's
  authoritative inventory API, not direct field writes.

### Estimated scope

Phase A: ~1 session per game. Phase B: 1 session.
Phase C: ~3-4 sessions (~500-700 lines). Phase D: ~2-3 sessions.
Phase E: ~1 session. Total: ~8-10 sessions. The framework piece
is the leverage point.

---

## g2rpg. Catalog expansion

Each entry is one [`skills::CATALOG`](../grounded2-mod/src/rpg/skills.rs)
row of an existing `SkillEffect` shape unless noted.

- [ ] **Critical Chance + Critical Damage**. `before` callback
  on the live damage hook returns `Some(damage * (1 + crit))` on
  roll. RNG needs a per-mod seed; reuse `rand` crate.
- [ ] **Evasion / Dodge**. `before` returns `Some(0.0)` on roll
  for player-taken hits.
- [ ] **Thorns**. `after` resolves the attacker's HC (instigator
  -> controller -> possessed pawn -> HC at +0x1340) and writes
  `event.damage * thorns_fraction` to its CurrentDamage. Same
  healing pattern proven via Lifesteal.
- [ ] **Leap Distance**. LANDED untested
  (`PlayerMovementMult` over AirControl trio).
- [ ] **Auto-pickup (range)**. Per-frame proximity scan picks up
  loose items. Open design: mechanism (existing proximity-pickup
  field vs per-frame scan), filter (which items qualify),
  throttle, multiplayer authority.
- [ ] **Stamina Pool + Stamina Regen**. `UStaminaComponent` on
  ASurvivalCharacter at +0x1358; offsets need a Dumper-7 dive.
- [ ] **Gear Hardiness**. Find durability-loss-per-use field;
  per-item durability scaling.
- [ ] **Climb Speed**. May or may not exist as a separate
  CharMovement field; check before adding.
- [ ] **Collision / Impact Damage Resistance**. Reduce / negate
  the lethal self-damage from slamming into plants / terrain.
  Likely home: live damage hook surface.

!!! tip "Catalog target"
    ~25 skills. Today: 13.

## g2rpg. Live-instance writes

Some combat-side skills still write only to player CDOs.
Movement now mirrors onto the live player pawn; remaining
combat effects need the same. Each `SkillEffect` variant grows a
"live walk" arm analogous to its CDO walk.

## g2rpg. Pkg(0) instigator bug

!!! bug "Repro"
    Some legitimate player kills attribute to
    `/Script/CoreUObject (Package)` because
    `LastDamageInfo.InstigatorController` is an unset
    `FWeakObjectPtr` (index=0 -> Package). User loses XP.

Investigation:

1. Sample more kills with different weapons. Identify which
   attack types fail vs succeed.
2. Hook `ApplyDamageFromInfo` to read `FDamageInfo` parm at
   function entry and cache the instigator per HC. When
   multicast fires with KillingBlow, use cached value if
   `LastDamageInfo` is null.
3. Failing that, fall back heuristically (player in attack range
   + recently swung).

## g2rpg. Status-effect migration (in progress)

Migrate Impact Damage Resistance and Lifesteal to the canonical
status-effect surface first; rest of catalog follows. Detail in
[`../grounded2-mod/docs/damage.md`](../grounded2-mod/docs/damage.md).

Concrete next steps:

- [ ] **Optimize the discovery test**.
  `tests/explore_status_effect_rows.rs`. Batch read_bytes (one
  big chunk) so it runs in seconds. Capture FName for each row.
- [ ] **Pick target row per stat type**.
- [ ] **Implement `SkillEffect::PlayerStatusEffect` variant** in
  `skills.rs`.
- [ ] **One generic apply arm in `apply.rs`** that resolves the
  table, looks up row by FName, mutates Value, calls
  `UStatusEffectComponent::CreateAndAddEffect` via process_event.
- [ ] **Migrate `impact_resistance` first** as proof of concept.
- [ ] **Validate via the regression test**: bandages must heal
  even with impact_resistance enabled.
- [ ] **Migrate the rest** row-by-row:
    - `health_regen` -> `Type=Health (24)`, drop UGlobalCombatData
      mutation
    - `max_health` -> `Type=MaxHealth (5)`, drop direct HC.MaxHealth
    - `lifesteal` -> `Type=LifeSteal (38)`, drop Runtime no-op
    - `attack_damage` -> `Type=AttackDamage (23)`
    - `armor` -> `Type=DamageReduction (29)` or
      `DamageReductionMultiplier (30)`
    - `fall_resistance` -> ALSO apply `Type=FallDamage (14)` for
      consistency (keep velocity-stomp as the validated mechanism)

Movement skills stay on direct CMC field writes. The
status-effect surface doesn't expose movement parameters with
the granularity we use. Survival drains stay on
SurvivalComponent CDO writes.

## g2rpg. RPG tuning

Open until we play more.

- XP curve: is `100 * N^1.8` (cap 50) too steep / flat?
- Per-creature XP table: ~20 species with placeholder values.
  Expand and balance.
- Skill `max_bonus`: are the level-100 caps right?
- Level-up frequency vs catalog size: 50 levels = 50 points;
  catalog max-everything = ~225 points (9 skills x 25 avg).

## g2rpg. Distribution

- [ ] **Vortex / Nexus packaging**. `cargo deploy package`
  produces the right zip layout. Need a Nexus listing
  (description, screenshots, mod page).

## g2rpg. Feature ideas (not yet scoped)

- [ ] **Auto-farming buildings**. First concrete consumer of the
  buildings module above. Plant-fiber / resin auto-harvester as
  the first building.

---

## g2rpg. Integration testing

Reference design + test coverage principle:
[`../grounded2-mod/docs/testing.md`](../grounded2-mod/docs/testing.md)
+ [`../ueforge/docs/RESEARCH.md`](../ueforge/docs/RESEARCH.md)
"Test coverage principle".

### Read surface (snapshot fields)

- [ ] **dmg-trace ring buffer in snapshot**. Last N multicast
  events. Currently log-only.
- [ ] **player world location + equipped weapon** (context for
  fall / item-use simulation).

### Drive surface

- [ ] **High-frequency drain site for the PE queue**.
  `kill_hook`'s trampoline only fires on player HC vtable
  events. With `impact_resistance` mask = 0xFFFFFFFF, multicast
  traffic drops near zero, starving the drain. `call` ops time
  out at 5s. Land Wave E1 (global ProcessEvent pre-callback)
  to fix permanently.
- [~] **simulate_consume_item**. Via `call` once we know
  TryUseItem signature on the player controller.
- [~] **simulate_kill / simulate_fall**. Test-side via `call`.
  Doable today.
- [~] **simulate_status_effect_add / _remove**. Via `call` on
  `UStatusEffectComponent::CreateAndAddEffect` once row handle
  layout is captured in a `#[repr(C)]` struct.
- [~] **set_xp / set_level**. Via `call` if a setter UFunction
  exists, or via direct field-write.
- [~] **clear_status_effects**. Via `call` or direct TArray
  write.

### Test coverage (per-skill principle)

For EACH catalog skill (x13):

- [ ] refund -> reverts (only ~half tested explicitly)
- [ ] toggle off -> reverts to vanilla (only for variants that
  honor the flag)
- [ ] toggle on -> effect restored (move_speed only; fill in)
- [ ] persist across slot reload (NOT covered for any skill)
- [ ] level 0 -> no effect (make explicit per skill)

Interaction matrix:

- [ ] armor + heal
- [ ] max_health + fall onto rocks
- [ ] fall_resistance + impact_resistance + rock collision
- [ ] every other pair touching HC fields, damage gate, regen

Error paths:

- [ ] op while no slot (only reload_slot covers this today)
- [ ] malformed args (wrong type)
- [ ] port collision / second instance

---

## Out of scope (kept for reference)

- Visible 6-row inventory grow. Replaced by paged 4x10 viewport
  with mouse-wheel scroll. Working.
- Multiplayer correctness for hunger/thirst. Stats almost
  certainly replicate server-authoritatively, so non-host
  clients won't see the effect. Document, don't fight.
- In-world UMG HUD overlay (always-visible XP bar). Out of v1
  scope; would need a custom DX11 hook or BPModLoader-shipped
  widget. ImGui tab is sufficient.

## Open: hot-reload follow-ups

Phase B (in-DLL teardown of hooks / servers / settings watchers
+ side-file rename) shipped 2026-05-10. See
[`../ueforge/docs/lifecycle.md`](../ueforge/docs/lifecycle.md)
"Hot-reload (Ctrl+R)" for the implemented surface.

Optional follow-ups (defer until needed):

- [ ] **Phase B+: programmatic Ctrl+R from `cargo deploy install
  --hot`**. Mod watches a sentinel file; on change, synthesizes
  Ctrl+R via UE4SS's `register_keydown_event`.
- [ ] **Phase B++: HTTP `reload` op** that calls
  `UE4SSProgram::queue_reinstall_mods` directly. Smaller side
  trip.
