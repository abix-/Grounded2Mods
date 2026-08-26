# wwm-mod

Wild West Miner Simulator (Demo) RPG mod. Rust cdylib loaded
into the game by the `Unityforge.Shim.Mono` BepInEx plugin.

## Features

| Feature | Description | Rating |
|---|---|---:|
| [Demo boundary](src/block_demo_end.rs) | Prevents the demo-complete screen from ending play. | 2/10 |
| [Jump](src/jump.rs) | Adds a grounded spacebar jump. | 2/10 |
| [Quick Pickaxe](src/skills.rs) | Live-verified increase to digging range. | 5/10 |
| [Strong Back](src/skills.rs) | Catalogued capacity skill awaiting a game-skill proxy. | 2/10 |
| [Greedy Miner](src/skills.rs) | Catalogued ore-value skill awaiting in-save verification. | 2/10 |
| [Charisma](src/skills.rs) | Catalogued hiring skill with its target under research. | 1/10 |
| [Resilient](src/skills.rs) | Catalogued stamina skill with its target under research. | 1/10 |
| [Lucky](src/skills.rs) | Catalogued runtime-effect placeholder. | 1/10 |
| [XP](src/skills.rs) | Credits digging and currency events. | 3/10 |
| [Persistence](src/skills.rs) | Stores RPG state per save slot. | 5/10 |
| [Progression](src/skills.rs) | Supports spend, refund, toggle, and point controls. | 5/10 |
| [RPG tab](src/skills.rs) | Renders the declarative skill catalog. | 3/10 |
| [Control plane](src/lib.rs) | HTTP inspection and control on port 17172. | 9/10 |

## Status

**Phase 3a complete (declarative).** Six-skill catalog wired
through `modforge::rpg::Tracker<UnityEngine>` with
`UnityField{Additive,Multiply}Effect`s + a `RuntimeEffect`.
Save/load via `modforge::rpg::store` (JSON under
`<DLL_dir>/wwm-mod/<slot>.json`, atomic temp+fsync+rename).
Harmony postfixes route through `TRACKER.record_xp`. HTTP
control plane on port 17172 (`ping`, `walk_class`,
`inspect_object`, `read_field`, `write_field`,
`invoke_method`, `skill_state`, `skill_spend`, `skill_refund`,
`skill_toggle`, `reload_slot`, `set_skill_points`,
`skill_add_xp`, `skill_grant_points`, `list_tabs`,
`render_tab`).

Catalog (`wwm-mod/src/skills.rs`):

| Skill | Effect | Target |
|---|---|---|
| Strong Back | `UnityFieldAdditive` | `PlayerCarryingController._maxCapacity` |
| Greedy Miner | `UnityFieldMultiply` | `MineDataSO._oreValue` |
| Quick Pickaxe | `UnityFieldMultiply` | `DigManager._digRange` |
| Charisma | `UnityFieldMultiply` | `WorkersManager._hireCostMultiplier` |
| Resilient | `UnityFieldMultiply` | `PlayerStaminaController._staminaDrainMultiplier` |
| Lucky | `RuntimeEffect` (format-only) | (hot-path postfix, future) |

Harmony postfixes: `DigManager.Dig` (+5 XP/swing),
`PlayerManager.AddPlayerCurrency` (+10 XP/event).

**End-to-end verified in-game (2026-05-13 session).** Plugin
loads, HTTP control plane answers, slot poller activates,
catalog state persists to disk across spend/refund, and the
Effect dispatch round-trip works: spending 10 points on
`quick_pickaxe` mutated `DigManager._digRange` from 3.0 to
4.5 exactly (3.0 * (1 + 0.5 * 1.0), matching the
`UnityFieldMultiplyEffect` math).

**Major finding: the game ships its own skill system.**
`SkillsManager` (singleton) exposes `SetSkillLevel`,
`LevelUpSkill`, `GetCurrentSkillLevel`, `GetCurrentSkillValue`,
plus 4 built-in skills (`Bag`, `Energy`, `Rope`, `Speed`).
Calling `SkillsManager.SetSkillLevel("Bag", 4)` via
`invoke_method` grew the player's backpack from 5 to 12 slots
**live**, no save reload, no field hunting. The right shape
for wwm-mod is a `UnitySkillProxyEffect` that maps our levels
to game-skill levels and calls `SetSkillLevel`; the game does
the actual mutation + save persistence + UI refresh. See
[`docs/research.md` §7.5](docs/research.md#75-in-game-findings-2026-05-13-session)
for the full state graph + method surface.

**Field-name verification status.** Both surviving raw-field
targets (Quick Pickaxe + slot key) are right. The four others
are obsoleted by the SkillsManager finding above (any wwm-mod
skill that wants to scale a vanilla game stat should proxy
through `SkillsManager.SetSkillLevel` instead of writing
fields).

| Skill | Approach | Status |
|---|---|---|
| Quick Pickaxe | `UnityFieldMultiplyEffect` on `DigManager._digRange` (Single, vanilla 3.0). Not covered by SkillsManager. | **verified working** |
| Slot key | Read `GameSerializationSystem._currentLoadedSaveNumber` (Int32, 0 at main menu). | **verified working** |
| Strong Back | Move from raw field write to `UnitySkillProxyEffect` -> `SkillsManager.SetSkillLevel("Bag", N)`. The game does the slot-list growth + save. | open (needs new Effect) |
| Greedy Miner | Proxy candidate unclear: no built-in `Gold` or `OreValue` skill. May still need a raw-field Effect against `MineDataSO._oreValue`, but `MineDataSO` instances don't load at the main menu; verify in-save. | open |
| Charisma | No matching built-in skill. Repoint to `Energy`-flavored game skill OR write a custom proxy that calls multiple `SetSkillLevel`s. | open (research) |
| Resilient | Proxy to `Energy` game skill (controls energy capacity/regen) or scale `_jetpackEnergyConsumeAmount` directly. | open (research) |
| Lucky | `RuntimeEffect` (no target; future hot-path postfix). | unchanged |

Next change: ship `UnitySkillProxyEffect`. Repoint Strong
Back at it first (the verified path). Then research what the
other game skills (`Energy`, `Rope`, `Speed`) actually scale
to figure out the right mapping for Charisma / Resilient.

---

## Testing in WWM

### 1. One-time: install BepInEx 5.x Mono

WWM ships no mod loader. Grab **BepInEx 5.4.x x64 Mono** from
the [BepInEx releases page][bep] (file like
`BepInEx_win_x64_5.4.x.zip`; **NOT** the IL2CPP build).
Extract the zip's contents into:

```
C:\Games\Steam\steamapps\common\Wild West Miner Simulator Demo\
```

so that `BepInEx\core\BepInEx.dll` lands at the game root.
Launch the game once with BepInEx in place; it generates
`BepInEx\config\BepInEx.cfg`. Quit.

Open `BepInEx\config\BepInEx.cfg`, find the
`[Logging.Console]` section, set `Enabled = true`. Optional
but ergonomic; without it tail `BepInEx\LogOutput.log`
instead.

[bep]: https://github.com/BepInEx/BepInEx/releases

### 2. Build + deploy

From the repo root:

```powershell
.\wwm-mod\scripts\build_and_deploy.ps1
```

That:

1. Builds `wwm_mod.dll` (`cargo build --release -p wwm-mod`).
2. Builds `Unityforge.Shim.Mono.dll`
   (`dotnet build -c Release` on
   `unityforge\cs-shim-mono\Unityforge.Shim.Mono.csproj`,
   using the BepInEx + Unity DLLs that already shipped with
   WWM).
3. Copies both into
   `<WWM>\BepInEx\plugins\wwm-mod\`:
   - `wwm_mod.unityforge.dll` (renamed from `wwm_mod.dll`)
   - `Unityforge.Shim.Mono.dll`

If WWM lives somewhere else, pass `-WwmDir 'D:\Games\...'`.
Pass `-NoCopy` to skip the deploy step (build only).

The script prints the BepInEx log path + a curl sample on
success.

### 3. Launch + verify the bridge loaded

Launch WWM. In the BepInEx console (or
`BepInEx\LogOutput.log`) expect:

```
[Info   :Unityforge.Shim] Unityforge.Shim: Awake
[Info   :Unityforge.Shim] Unityforge.Shim: loading <path>\wwm_mod.unityforge.dll
[Info   :Unityforge.Shim] Unityforge.Shim: ready
[Info   :Unityforge.Shim] unityforge: init WildWestMinerRpg v0.1.0
[Info   :Unityforge.Shim] wwm-mod: ready (ops + selectors + skills installed)
```

Failure modes:

- `Unityforge.Shim: no Rust target DLL found.`. The
  `*.unityforge.dll` naming convention was missed. Check
  the file at `BepInEx\plugins\wwm-mod\` is literally named
  `wwm_mod.unityforge.dll`.
- `Unityforge.Shim: LoadLibrary failed: <code>`. Usually a
  missing dependency. The Rust cdylib only links MSVC + Win32
  imports; this is rare. If it happens, run
  `dumpbin /DEPENDENTS wwm_mod.unityforge.dll` and check for
  anything unexpected.
- `Unityforge.Shim: target DLL is missing one of
  unityforge_init / unityforge_tick / unityforge_shutdown` --
  the cdylib was built without the macro emit; verify
  `unityforge::unityforge_mod!(MOD_INFO)` is uncommented in
  `wwm-mod/src/lib.rs`.

### 4. Drive the HTTP control plane

From a separate terminal:

```powershell
# Always use 127.0.0.1 on Windows. localhost adds ~200 ms.
$op = 'http://127.0.0.1:17172/op'

# Sanity check.
curl.exe -s $op -d '{"op":"ping"}'

# What ops are registered?
curl.exe -s $op -d '{"op":"list_ops"}'

# Catalog + state snapshot (no slot yet -> "no slot active").
curl.exe -s $op -d '{"op":"skill_state"}'

# The framework's RPG ops:
curl.exe -s $op -d '{"op":"set_skill_points","args":{"count":50}}'
curl.exe -s $op -d '{"op":"skill_spend","args":{"id":"strong_back","count":5}}'
curl.exe -s $op -d '{"op":"skill_refund","args":{"id":"strong_back","count":1}}'
curl.exe -s $op -d '{"op":"skill_toggle","args":{"id":"strong_back","enabled":false}}'
curl.exe -s $op -d '{"op":"reload_slot"}'

# Render the declarative tab (logs catalog + state to BepInEx).
curl.exe -s $op -d '{"op":"render_tab","args":{"name":"RPG"}}'
```

### 5. Verify the slot poller catches a save

Load a save in-game. The slot poller polls
`GameSerializationSystem._currentLoadedSaveNumber` every
second; once a save is active, BepInEx log should show:

```
rpg/tracker: activated slot=<N> level=0 xp=0 skill_points=0
```

If that line never appears:

- The `GameSerializationSystem` class name or field name is
  wrong. Fix in `wwm-mod/src/skills.rs`:
  ```rust
  static SLOT_KEY: UnitySlotKey = UnitySlotKey::new(
      "<actual class>", "<actual field>");
  ```
  Find the actual identifiers via `walk_class` (step 6).

### 6. Re-check identifiers after a game update

Quick Pickaxe and the save-slot field are live-verified. The
other raw-field effects remain experimental and should be
re-checked after a game update:

```powershell
# Get a handle on the live singleton.
curl.exe -s $op -d '{"op":"singleton","args":{"class":"PlayerCarryingController"}}'

# Dump every field on it.
curl.exe -s $op -d '{"op":"inspect_object","args":{"handle":<N>}}'
```

Compare the returned field names against
`wwm-mod/src/skills.rs`, then rebuild and restart after any
correction.

The six declared field targets:

| Class | Field |
|---|---|
| `PlayerCarryingController` | `_maxCapacity` |
| `MineDataSO` | `_oreValue` |
| `DigManager` | `_digRange` |
| `WorkersManager` | `_hireCostMultiplier` |
| `PlayerStaminaController` | `_staminaDrainMultiplier` |

And the two declared method targets (for Harmony):

| Class | Method |
|---|---|
| `DigManager` | `Dig` |
| `PlayerManager` | `AddPlayerCurrency` |

If a Harmony patch target doesn't exist, BepInEx logs
`HarmonyX patch failed: <class>::<method> not found`. Pick a
closer-named method via `walk_class` + `inspect_object` and
fix in `wwm-mod/src/skills.rs::install_hooks`.

### 7. Drive the full skill loop

Once the field names are correct + a slot is active:

```powershell
# Grant points + level up the live-verified Quick Pickaxe effect.
curl.exe -s $op -d '{"op":"set_skill_points","args":{"count":50}}'
curl.exe -s $op -d '{"op":"skill_spend","args":{"id":"quick_pickaxe","count":10}}'

# Confirm via state snapshot.
curl.exe -s $op -d '{"op":"skill_state"}'

# DigManager._digRange should move from 3.0 to 4.5.

# Dig some ore -> XP increments; eventually levels up.
# Tail the BepInEx log for:
#   rpg/tracker: LEVEL UP! 0 -> 1 (+1 skill points; total 46)
```

### 8. Iteration loop

Quit the game, edit code, rerun `build_and_deploy.ps1`,
relaunch. Windows refuses to overwrite a DLL that's currently
mapped into a process; if the deploy step errors with
"access denied," WWM is still running.

The save file lives at:

```
<WWM>\BepInEx\plugins\wwm-mod\wwm-mod\<slot>.json
```

Delete it to reset a slot's RPG state without affecting the
game's own save.

### 9. Removing the mod

Delete `<WWM>\BepInEx\plugins\wwm-mod\`. The mod's save
sidecar JSON goes with it. The game's vanilla saves are
untouched.

---

## File layout

```
wwm-mod/
  Cargo.toml
  README.md                       # this file
  scripts/
    build_and_deploy.ps1
  src/
    lib.rs                        # ModDef + unityforge_mod!() macro
    skills.rs                     # catalog + ops + Harmony postfixes
```

## Manual build (no PowerShell)

```sh
# Rust cdylib
cargo build --release -p wwm-mod
# -> target/x86_64-pc-windows-msvc/release/wwm_mod.dll

# C# Mono shim
cd unityforge/cs-shim-mono
dotnet build -c Release \
  -p:BepInExDir="C:\Games\Steam\steamapps\common\Wild West Miner Simulator Demo\BepInEx" \
  -p:UnityDir="C:\Games\Steam\steamapps\common\Wild West Miner Simulator Demo\Wild West Miner - Gold Rush_Data\Managed"
# -> bin/Release/netstandard2.1/Unityforge.Shim.Mono.dll
```

Then copy both files into `<WWM>\BepInEx\plugins\wwm-mod\`,
renaming the Rust DLL to `wwm_mod.unityforge.dll`.

## How the loader picks the cdylib

The shim's `Awake` scans its own directory for any
`*.unityforge.dll`. If exactly one is present, it loads it.
Override with the `UNITYFORGE_TARGET` env var (absolute path
to the cdylib) when iterating against a non-default location.
