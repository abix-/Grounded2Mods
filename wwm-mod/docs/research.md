# Wild West Miner: modding research

> **Status:** initial research pass, 2026-05-12. No code yet. The
> question this doc answers: how hard would it be to mod Wild
> West Miner Simulator (Demo), and is the right shape (a) a
> shared "modforge" core that supports both Unreal and Unity,
> (b) a parallel `unityforge` sibling to `ueforge`, or (c)
> something else.

## 1. Why this question exists

We have two mature mod frameworks in the abix orbit:

| Framework | Engine | Language | Repo | Lines of mod-code | Posture |
|---|---|---|---|---|---|
| **ueforge** | UE5 (UE4SS host) | Rust | `grounded2mods/ueforge` | ~15k Rust framework, ~3.5k per game mod | UE-specific, deep |
| **Timberbot** | Unity Mono (Bindito DI host) | C# | `timberborn/timberbot` | ~13k C# (single mod, no framework split yet) | One game, but the patterns are generalizable |

Both expose the same loop: an HTTP control plane on localhost,
snapshots of live game state, write ops applied on the main
thread, an in-game ImGui/UI tab, hot reload, an LLM-friendly
read format (`toon`) alongside `json`.

The shapes match. The plumbing is engine-specific.

A third game (Wild West Miner) appeared in the user's library
and the natural question is: do we want to mod it, and if yes,
with what?

## 2. Wild West Miner: technical profile

### 2.1 Game shell

| Property | Value |
|---|---|
| Install path (old demo) | `C:\Games\Steam\steamapps\common\Wild West Miner Simulator Demo\` |
| Install path (First Gun demo) | `C:\Games\Steam\steamapps\common\Wild West Miner Simulator First Gun\` |
| Engine | Unity (Mono scripted, **NOT** IL2CPP) |
| Build GUID | `d498052cf0224d53bbcb80c6e5e84846` (from `boot.config`) |
| Publisher | Digital Melody Games |
| Mod loader shipped | None (no BepInEx, no MelonLoader, no built-in mod system) |
| Save format | `Newtonsoft.Json` blobs in `SaveDat` files |
| Anti-cheat | None observed in shipped DLLs |

### 2.2 Code shape (Assembly-CSharp.dll)

| Metric | Value |
|---|---|
| `Assembly-CSharp.dll` size | ~600 KB |
| Top-level classes | **723** |
| `Assembly-CSharp-firstpass.dll` | DOTween only (third-party, ignore) |
| Third-party DLLs of note | `Digger.Core` + `Digger.Runtime` + `Digger.Splines` + `Digger.AdvancedOperations` (Unity asset-store voxel terrain), `DOTween`, `Newtonsoft.Json`, `Unity.Addressables`, `Steamworks.NET` |

Top-level classes break out roughly:

- `*Manager` (Singleton<T>): `PlayerManager`, `GameplayManager`,
  `AudioManager`, `EnvironmentManager`, `NpcsManager`,
  `MinesManager`, `WorkersManager`, `BarrelsManager`,
  `FurnacesManager`, `DigManager`, `DigSpawningManager`, ... (~30
  total).
- `*Data` / `*SO` (ScriptableObject configs): `PlayerData`,
  `MineDataSO`, `WorkerModel`, `GameDataSO`, `BedDataSO`,
  `CloudSO`, `Cloud_NeedSO`, `LocalizationDataSO`,
  `ToolDataSO`, ... (~50+ SOs hand-authored as game assets).
- `*Database` (per-entity save records): `PlayerDatabase`,
  `WorkerDatabase`, `BarrelsDatabase`, `FurnaceDatabase`, ...
  (~25 total). One per persistable entity type.
- Tutorial system: `*TutorialObjective` and `TO_*` classes (~30
  total). Drives the demo's onboarding flow.
- Player controllers: `PlayerController`, `PlayerWalkingController`,
  `PlayerCarryingController`, `PlayerDynamiteController`,
  `PlayerWheelbarrowController`, `PlayerTrunkController`,
  `PlayerEqController`, `PlayerInteractionObjectsController`,
  `PlayerPlacingController`. Decomposed by verb.

### 2.3 Architecture patterns observed

**Custom service locator, not DI.** The game uses three
hand-rolled generic singletons:

| Pattern | Lifetime | Used by |
|---|---|---|
| `Singleton<T>` | scene-lifetime | gameplay managers (PlayerManager, DigManager) |
| `StaticInstance<T>` | scene-lifetime | view-layer (GameplayManager, MoneyPanelUI) |
| `PersistentSingleton<T>` | app-lifetime | infra (GameSerializationSystem) |

Translation: every interesting service is reachable via
`PlayerManager.Instance` / `StaticInstance<X>.Instance`. No
Bindito-style container to navigate. Easier than Timberborn for
this reason.

**Custom persistence interface.** Every persistable manager
implements `ISerializable`:

```csharp
public async Task SaveData(GameSerializationData data) { data.AddData("PLAYER_DATA", _playerData); }
public void LoadData(GameSerializationData data) { ... }
```

Managers register themselves with
`GameSerializationSystem` on `Start`. Save key is a string
(`PLAYER_DATA`, etc.). Newtonsoft.Json under the hood. Trivial
to read or inject extra mod-side keys.

**Events expose state changes.** `PlayerManager` ships:

```csharp
public static event Action<bool> e_OnPlayerActiveStateChanged;
public static event Action<bool> e_OnPlayerCurrencyChanged;
public static event Action e_OnPlayerExpChanged;
```

Same pattern across managers. A mod subscribes once at load and
gets state-change pushes without polling. This is rare and nice;
Timberborn doesn't even ship this much.

**Worked example: PlayerData is one float.** Full source of
the type that holds the player's wallet:

```csharp
[Serializable]
public class PlayerData {
    [JsonProperty] public float PlayerMoneyCurrency { get; private set; } = 10f;
    public void ChangeCurrencyValue(float newCurrencyValue) {
        PlayerMoneyCurrency = newCurrencyValue;
    }
}
```

That is the entire vault. The mod surface for "give the player
gold" is one method call: `PlayerManager.Instance.AddPlayerCurrency(1e9f)`.

**Digger voxel terrain is the differentiator.** The Mine system
wraps `Digger.Modules.Core.Sources.DiggerSystem`. Modifying
mined volumes (placing ore back, regenerating veins, painting
textures) goes through Digger's public API and is fully
documented at the asset-store level. Out-of-band of game code,
not difficult, just a separate API surface to learn.

## 3. ueforge stack (current state, summarized)

> Source: `ueforge/docs/architecture.md`, `lib.rs`,
> `cpp/` (502 lines of C++ shim).

**Surface layers:**

1. **C++ shim** (502 lines, fixed cost): `CppUserModBase`
   subclass + ImGui bridge + `extern "C"` boundary. Imgui itself
   is a vendored submodule. UE4SS-specific.
2. **Rust framework** (~15k lines): UObject SDK, FName/FString,
   TArray/TMap, GObjects view, vtable hooks, ProcessEvent
   trampoline, PE queue (game-thread serialization), settings,
   hot reload, logger, debug HTTP server, op envelope, selector
   grammar, snapshot machinery, RPG/stacks/difficulty/damage
   modules.
3. **Per-game mod crates** (~3.5k Rust each):
   `grounded2-rpg`, `outworld-station-tweaks`. Declare a
   `ModDef`, register Effects/Triggers/Skills, wire game-specific
   class names + offsets, supply tab content.

**Architectural rules ueforge follows:**

- **Def -> Registry -> Instance -> Controller** (CRD-style) for
  every subject (skills, hooks, ops, selectors, status effects,
  data tables, tabs, shutdown handlers).
- **Effects + Triggers + Skills** composition: a Skill is one
  Effect + one Trigger. New games add Effects only when the game
  has a unique operation no other game shares.
- Universal patterns live in ueforge. Game-specific code is
  `&'static` config + an opt-in trait impl.

**Where the framework cost lives:** the UE-specific bottom
~30%. UObject memory layout, FName interning, vtable hook
mechanics, ProcessEvent decode, PE queue (re-entrance guard +
game-thread dispatch). Everything else (HTTP, snapshots, ops,
selectors, settings, hot reload, ImGui, logger) is generic.

## 4. Timberbot stack (current state, summarized)

> Source: `timberborn/docs/architecture.md`,
> `timberborn/timberbot/src/*.cs`.

**Surface layers:**

1. **BepInEx publicizer** (`BepInEx.AssemblyPublicizer.MSBuild`).
   At compile time, exposes the game's `internal` types so the
   mod can call them. No runtime patching.
2. **Bindito DI registration** (`TimberbotConfigurator`).
   `[Context("Game")]` declares a configurator the game's DI
   container picks up at game load.
3. **Game-singleton classes** implementing
   `ILoadableSingleton` / `IUpdatableSingleton` /
   `IUnloadableSingleton`. The game calls `Load()` at game start
   and `UpdateSingleton()` every frame.
4. **The mod itself** (~13k C# in `timberbot/src/`):
   - `TimberbotService` (orchestrator, per-frame dispatch).
   - `TimberbotHttpServer` (background listener thread + main-
     thread drain).
   - `TimberbotReadV2` (snapshot pipeline with `ProjectionSnapshot
     <TDef, TState, TDetail>`).
   - `TimberbotWrite` (POST write ops, budgeted).
   - `TimberbotPlacement` (building placement, A* path routing).
   - `TimberbotWebhook` (batched push events).
   - `TimberbotEntityRegistry` (GUID/numeric ID bridge).
   - `TimberbotJw` (zero-alloc JSON writer).
   - `TimberbotAgent` (Claude/Codex launcher).
   - `TimberbotPanel` (in-game movable widget + settings modal).

**Key facts that shape the comparison:**

- Timberborn ships its own mod loader + Bindito DI. Timberbot
  does **not** need a third-party loader. Plugin just lives in
  `Documents/Timberborn/Mods/Timberbot/`.
- Timberbot has **no framework / mod split yet**. The 13k lines
  are one project. Generalizable patterns (HTTP, Jw,
  ProjectionSnapshot, agent, panel) are mixed in with
  Timberborn-specific code (Bindito wiring, Beaver/Bot logic,
  Timberborn class refs).
- The architecture rhymes with ueforge: per-frame dispatch,
  background listener, main-thread drain, snapshot publish,
  budgeted write jobs, toon vs json output. Independent
  inventions converged on the same shape.

## 5. Side-by-side: how each does the same thing

| Concern | ueforge (UE5) | Timberbot (Unity Mono) | Wild West Miner |
|---|---|---|---|
| Loader | UE4SS (third-party DLL injector + CppUserModBase) | Timberborn's built-in mod system (Bindito DI) | **BepInEx 5 (Mono)** would need to be installed |
| Calling into game | UObject -> UFunction -> ProcessEvent + vtable hooks; FName for identity | Direct C# method calls on publicized internals | Direct C# method calls on **already-public** singletons; trivially the easiest of the three |
| Reading game state | Walk UObject tree, FProperty offsets, GObjectsView | `Bindito.Inject` and call `Manager.Property` on game singletons | `StaticInstance<X>.Instance.Property` / `Singleton<X>.Instance.Property`. Same as Timberbot conceptually |
| Threading | PE queue + game-thread guard; one drain per tick | Background listener thread + `_pending` queue drained on `UpdateSingleton()` | Same Unity model as Timberbot. Same approach works verbatim |
| Hooks / events | ProcessEvent trampoline on a class vtable slot. ~100% of UE5 surface area accessible by patching one method | HarmonyLib for arbitrary method patching; many state changes already eventbus-driven | HarmonyLib **plus** the game already exposes `static event Action` per state change. Often no patch needed |
| State persistence | Mod owns its `<guid>.json` per slot | Mod owns its settings JSON | Game's own `GameSerializationData` happily accepts a mod-side `[Serializable]` blob under a new key |
| Snapshot to HTTP | `ueforge::ops::snapshot`, double-buffered, JSON envelope | `ProjectionSnapshot<TDef,TState,TDetail>` + double-buffered capture | Either pattern transfers. Pick Timberbot's; it's cleaner C# |
| Hot reload | `ueforge::hot_reload` swap of running cdylib | BepInEx ScriptEngine / dnSpy edit-and-continue | Same as Timberbot |
| In-game UI | Bundled ImGui v1.92.1 inside the mod DLL | Timberborn's own UIElements panel | Unity UI / Unity IMGUI / build your own. **Imgui is the universal answer**, ueforge's ImGui setup transfers |
| LLM-friendly output | `toon` format from `OpResponse<S>` | `TimberbotJw` writes both `toon` and `json` per endpoint | Same pattern works |
| Per-game work | Class names + offsets + UFunction parm shapes + game-specific Effect impls | Each game-specific manager + each event subscription + each write op | The game has fewer types to chase than Timberborn (723 vs Timberborn's ~thousands); estimate **smaller than Timberbot** in absolute size |

## 6. Work estimate: a "Goldbot" for Wild West Miner

Define the v0 scope identical to Timberbot v0:

- Local HTTP control plane on a port.
- GET endpoints: player money, currently equipped tool, mines
  list with progress, workers list, inventory, savefile slot.
- POST endpoints: add currency, hire/fire worker, clear a mine
  (via `Mine.ClearMine()`), set dig tool, teleport.
- Webhooks for the three `e_On*` events the game already fires.
- In-game settings + toggle panel.
- Snapshot for an LLM agent to read every frame.

**Estimated effort, fresh (no shared framework):** ~2-3 weeks
of solo work. Comparable to where Timberbot was at v0.4.

**Why it's faster than Timberbot was:**

- No DI graph to learn (it's `Singleton<T>.Instance`).
- Game already publishes state-change events.
- ~723 classes is roughly half of Timberborn's surface.
- Save format is plain Newtonsoft JSON with a string key.
- All currency / progression types are tiny (PlayerData is one
  field).

**Why it's still real work:**

- BepInEx loader, plugin manifest, deploy path: ~1 day.
- HTTP server + thread drain pattern: copy from Timberbot. ~1
  day to retype.
- Webhook batcher: copy from Timberbot. ~half day.
- Reflection-based debug inspector: ~1 day.
- Per-endpoint capture + snapshot code: most of week 2.
- Digger voxel API integration (place/remove ore): unknown,
  call it 3-5 days of reading Digger docs.
- In-game UI (imgui-based panel or Unity UI): ~2-3 days.
- Polish + skill file + LLM agent wiring: ~2 days.

If we lift the generic Timberbot pieces into a shared `unityforge`
crate **first**, the v0 work above drops to roughly **1 week**
because seven of the bullets above become "wire up the shared
framework."

## 7. Should we build "modforge" (UE+Unity) or `unityforge` (sibling to ueforge)?

> **Update 2026-05-12:** the recommendation below evolved into the
> plan in [`unityforge-plan.md`](unityforge-plan.md). The short
> version: build a `modforge` workspace (shared spec, test corpus,
> client, deploy tool, methodology) alongside `unityforge` (the
> Unity Mono framework, ueforge-equivalent in scope. Skills,
> effects, triggers, hooks, registry pattern, snapshots, agent,
> UI). Both `ueforge` and `unityforge` adopt `modforge`. Game
> mods are downstream consumers, not blockers. The options below
> are kept for context.

This is the core decision. Three options:

### Option A: one polyglot `modforge` workspace (Rust + C#, single home)

**Shape:** one repo, ueforge moves in as the UE side, a new C#
project moves in as the Unity side. Shared docs, shared
methodology, shared release notes, **separate** binaries.

**Pros:**
- Single source of methodology truth (RESEARCH.md, op envelope,
  toon format, snapshot patterns).
- Cross-pollination of features is one PR away.
- One skill file (`modforge.md`) documents both stacks.

**Cons:**
- The two binaries cannot share *code*. Rust cdylib and a C#
  netstandard2.1 DLL are different artifacts. The "shared core"
  is essentially "shared spec + shared docs."
- Build infra has to host both Cargo and dotnet pipelines.
- One repo doing two things at once tends to slow each by ~30%
  in practice (workspace gravity, mixed CI, mixed reviews).

**Verdict:** mostly a documentation framing. Real code reuse is
zero. Not worth the merge cost.

### Option B: `unityforge` as a sibling C# library to ueforge (recommended)

**Shape:** new repo (`abix-/unityforge`) or new folder
(`grounded2mods/unityforge/`) containing the generic C#
framework Timberbot's `src/` already implies. The Timberbot
project becomes the first consumer. A new Wild West Miner mod
(`wwm-goldbot`) becomes the second consumer. ueforge stays
exactly where it is, owns the UE side, no churn.

**Pros:**
- Honest about the artifact boundary. C# DLL is a different
  thing from a Rust cdylib; pretending otherwise wastes time.
- Shares the *spec* (op envelope, toon, selector grammar) and
  the *patterns* (PE queue == main-thread drain, snapshot
  publish, projection schema) without forcing shared code.
- Refactor target is real: Timberbot has a clean
  `TimberbotHttpServer` + `TimberbotJw` + `ProjectionSnapshot`
  + `TimberbotEntityRegistry` already. They generalize cleanly.
  Lift, parameterize, ship.
- The second mod (Wild West Miner) immediately proves the
  abstractions. Without a second consumer the abstractions
  drift toward "whatever Timberbot needs."
- The methodology doc (RESEARCH.md style) can live in a
  third repo or a top-level doc that both reference.

**Cons:**
- More repos to maintain.
- "Same idea expressed in two languages" is a documentation
  burden if either side drifts.
- Risk: extracting unityforge from Timberbot is itself a 1-2
  week refactor. Not free.

### Option C: do nothing, build a one-off WWM mod from scratch

**Pros:**
- No framework investment. WWM mod ships in 2-3 weeks.

**Cons:**
- The third Unity mod (whatever comes next) re-invents the same
  scaffolding a third time.
- Wastes the leverage we already paid for in Timberbot.

### Recommendation

**Option B, but build `unityforge` from day one.** We already
have two consumers: Timberbot is shipped and stable (the
reference), Goldbot is the about-to-be-built second consumer.
The usual "design against one consumer = bad abstractions"
risk does not apply here, because Timberbot already exists in
finished form. Its `src/*.cs` is the spec.

Execution:

1. **Create `grounded2mods/unityforge/`** as a `netstandard2.1`
   class library. Lift the generic pieces out of Timberbot
   line-by-line:
   - `TimberbotHttpServer` -> `UnityforgeHttpServer`
   - `TimberbotJw` -> `UnityforgeJw`
   - `ProjectionSnapshot<TDef, TState, TDetail>` -> verbatim
   - `TimberbotEntityRegistry` -> `UnityforgeEntityRegistry`
     (generic over entity-id type; Timberborn uses GUIDs,
     Wild West Miner uses int IDs)
   - thread-drain pattern (`_pending` queue + budget)
   - settings JSON load + debounced save
   - webhook batcher
   - debug reflection inspector
   - skill-file generator + agent launcher
   Game-specific code stays in Timberbot's repo; only the
   generic shapes move.
2. **Build Goldbot as `grounded2mods/wwm-goldbot/`** on top of
   `unityforge` from the first commit. Goldbot's csproj
   references `unityforge.dll`; no copy-paste of Timberbot's
   patterns.
3. **Port Timberbot onto `unityforge`** as a second step,
   on its own timeline. Timberbot in `timberborn/` keeps its
   repo identity; it consumes `unityforge` as a NuGet package
   (preferred), a git submodule, or just a referenced project.
   Mechanical refactor: replace `TimberbotJw` with
   `UnityforgeJw`, etc. Behavior unchanged.

Sequencing rationale:

- Goldbot writing against the extracted framework is the only
  way to confirm Timberbot's abstractions actually generalized.
  If something doesn't fit, fix it in `unityforge` before
  porting Timberbot. Cheaper than fixing it after the port.
- Timberbot stays shippable the whole time. Port is independent
  work, not a blocker.
- Wild West Miner ships v0 in ~1 week instead of 2-3, because
  Goldbot inherits HTTP server + Jw + snapshot pipeline +
  webhooks + settings + reflection inspector + agent launcher
  on day one.

Naming: `unityforge` matches `ueforge`. C# sibling, not a
Rust translation. Different artifact, same posture.

## 7.5. In-game findings (2026-05-13 session)

First live session against WWM Gold Rush (Unity 6000.0.58)
with BepInEx 5.4.23.2 Mono + unityforge bridge. Probed via
HTTP debug ops. Findings reshape the modding posture:

### The game has its own skill system. Stop fighting raw fields.

`SkillsManager` (`Singleton<SkillsManager>`) ships a complete
RPG/upgrade API. Hooking into it is dramatically cheaper than
writing field-level Effect impls.

**SkillsManager method surface** (via reflection on the live
type, 87 methods total; the relevant ones):

| Method | Args | Returns | Use |
|---|---|---|---|
| `SetSkillLevel` | `(SkillType, int)` | void | Jump a skill straight to a level. Drives all the downstream live-state mutations the game does on a real upgrade |
| `LevelUpSkill` | `(SkillType)` | void | +1 level (probably the same surface a "buy upgrade" UI button calls) |
| `GetCurrentSkillLevel` | `(SkillType)` | int | Current level |
| `GetCurrentSkillValue` | `(SkillType)` | float | Numeric value at current level (e.g. Bag level 3 -> 12 slots) |
| `GetSkillLevelsAmount` | `(SkillType)` | int | Max level for that skill |
| `GetSkillPrice` | `(SkillType, int)` | float | Money cost to reach level N |
| `GetSkillValue` | `(SkillType, int)` | float | Value at level N (for previewing) |
| `LoadSkills` | () | void | Re-applies levels from save (fires on slot load) |

**SkillType enum** (4 variants): `Bag`, `Energy`, `Rope`, `Speed`.

**Bag progression** (verified via `SkillData_Bag` SO at
`SkillsDataSO.skillDatas[0]`):

| Level | Value (slots) | Price (money) |
|---|---|---|
| 0 | 5 | 0 |
| 1 | 7 | 50 |
| 2 | 9 | 200 |
| 3 | 12 | 500 |
| 4 | 15 | 1000 |

Verified live: `SetSkillLevel("Bag", 4)` on the running game
increased the player's backpack from 5 to 12 slots
immediately (no save reload, no other action). One value
discrepancy worth tracking: SetSkillLevel(4) settled at value
12 (level 3's value), not 15 (level 4's). Either the level
arg saturates at `maxLevel-1` or skillDatas[0] is the
"start" sentinel and gameplay levels start at 1. Verify by
calling `GetSkillLevelsAmount("Bag")` and reading what
SetSkillLevel actually wrote post-call.

### State graph

```
SkillsManager (Singleton)
  ├─ skillsData : SkillsDataSO
  │     └─ skillDatas : List<SkillDataSO>  (4 items: Bag/Energy/Rope/Speed)
  │           └─ each SkillDataSO has:
  │                ├─ skillType : SkillType
  │                └─ skillDatas : List<SkillData>  (5 levels)
  │                       └─ each SkillData has: value (float), price (float)
  └─ database : SkillsDatabase
        └─ skillsData : SkillsData
              └─ skillsData : Dictionary<SkillType, int>  (current level per skill)
```

The SO subtree is config (level table). The database subtree
is mutable state (current level per skill).

### What this means for wwm-rpg

The framework's `UnityFieldMultiplyEffect` was the wrong
shape for WWM. The right shape is a **skill-proxy effect**
that maps our level (0..max_level) to a game-skill level via
`SetSkillLevel`. The game does the live mutation; we drive
*when* it fires + gate it behind whatever XP/condition system
we own.

Sketch:

```rust
pub struct UnitySkillProxyEffect {
    /// Game skill name as the enum variant ("Bag", "Energy",
    /// "Rope", "Speed").
    pub game_skill: &'static str,
    /// Mapping from our level to a game-skill level. Identity
    /// (our level = game level) is the typical case; a u8
    /// array lets a 10-step our-side map to a 5-step game-side
    /// with custom pacing.
    pub level_map: &'static [u8],
}
```

apply: read our level, look up `level_map[level]`, call
`SkillsManager.SetSkillLevel(game_skill, mapped_level)`. The
game handles everything downstream (slot count, energy regen,
rope length, speed multiplier, save persistence, UI refresh).

### Other live findings

- `GameSerializationSystem._currentLoadedSaveNumber` confirmed
  as the slot key (Int32, 0 at main menu). Slot poller in
  unityforge::rpg::slot_key resolves it correctly.
- `GameDataSO._inventoryBaseSlotsCount` is config consulted
  at slot-spawn time, NOT a live cap field. Writing 20 to it
  from main menu did not grow the live carrying list. Don't
  target it as an effect field.
- `PlayerCarryingController.carryingSocketDatas` is a
  `List<CarringSocketData>`; capacity grows by `Add`-ing
  entries, not by writing a count. The game does this
  internally on Bag-skill upgrades.
- `PlayerStaminaController` and `WorkersManager` singletons
  don't exist on this build; the energy/worker state lives
  on different paths than the research-doc names suggested.
- `walk_class MineDataSO` returns zero instances at the main
  menu (SOs load with a save). Need an in-save session to
  inspect ore-value scaling.

### Updated open research questions

(Items from §8 below that this session DID answer are
crossed out where applicable.)

## 7.6. Demo-end screen anatomy (2026-05-14 session)

User hit the "Congratulations, you have beaten the demo"
modal mid-session and wanted to keep playing. Researched
the panel live via HTTP reads.

### What it is

| Surface | Value |
|---|---|
| Controller class | `DemoCompleteScreenUI` (MonoBehaviour) |
| Path in scene | `UI/Screens/DemoCompleteScreen` |
| Serialized fields | `panel` (child GameObject), `escapeDemoCompleteText` (TMP_Text), `selectedButton` (focused button) |
| Escape-text content | `"Press <color=#FFC500>[SPACE]</color> to return to menu"` |
| Buttons on the panel | `Steam`, `Discord`, `Survey`, `PlayWay`. Wishlist links only. No "continue" |
| Gameplay scene state when shown | Still loaded. `DigManager`, `PlayerController`, `MoneyPanelUI`, `GameplayManager` all live. Disabling the panel would resume play |

### What triggers it

The demo end fires when the last tutorial task completes.

| Probe | Reading |
|---|---|
| `TutorialDatabase.tutorialCurrentStep` | 26 (rolled past the last initialized step) |
| `TutorialDatabase.tutorialLastInitializedStep` | 25 |
| `TutorialManager.currentTask` | null (no active task remains) |
| Last task instance | `SellGoldBar` (`TutorialTaskSellItem`, `Id=24`) |
| `SellGoldBar.target` | `NpcBanker` GameObject at `Bank` building |
| `SellGoldBar.item` / `amount` | `102_GoldBar` x 2 |
| `SellGoldBar.soldItemsAmount` | 2 (task complete) |
| `SellGoldBar.onFinishEvent` | UnityEvent. Its persistent-call list (`m_PersistentCalls`) is where the "show demo-complete" wire-up lives. Our bridge couldn't read it: reflection only walks declared fields, not inherited base-class fields |

### Revert paths

1. **Block the trigger before it fires (cleanest).** Harmony
   prefix on `DemoCompleteScreenUI.OnEnable` (or whatever
   method activates the panel: `Show`, `Open`, ...) returning
   `false`. Permanent fix, survives saves. Needs a
   `list_methods` op to confirm the actual entry point.
2. **Dismiss after it shows.** `SetActive(false)` on the
   `DemoCompleteScreen` GameObject + force
   `Time.timeScale = 1`. Blocked today because every
   `invoke_method` and `write_field` returns
   `main-thread queue timed out` while the panel is up: the
   game sets `Time.timeScale = 0` and the shim drain in
   `cs-shim-mono/Plugin.cs` stalls. Fix: drive the drain
   off `Time.unscaledDeltaTime` (or unconditionally per
   Update).
3. **Rewind tutorial state.** Write `tutorialCurrentStep`
   back below 25 BEFORE SellGoldBar finishes. Too late once
   the panel is up; re-completing the task would re-trigger.

### Bridge gaps this session exposed

- **No array-index read.** Could not read `_items[25]` on
  the `List<TutorialTask>` to confirm the last entry; had
  to fall back to `walk_class TutorialTask` and trust order.
- **Reflection skips base-class fields.** UnityEvent's
  `m_PersistentCalls` / `m_Calls` (declared on
  `UnityEventBase`) returned `not found`. Same for any
  inherited fields on subclassed MonoBehaviours.
- **No type-substring search.** Finding `DemoCompleteScreenUI`
  cost ~15 guess-and-walk attempts. A
  `find_types {contains: str}` op would have answered in
  one call.
- **No method enumeration.** `list_methods {handle}` would
  let us pick the exact Harmony prefix target without
  dnSpy.

These four are the highest-leverage additions to the next
bridge revision; queued behind the drain-stall fix because
that one is blocking the user right now.

### Technique notes (worth remembering)

- **Walk transform parents to find a UI panel's root.**
  Starting from any leaf widget (button, image, text),
  repeated `read_field {handle, field: "parent"}` walks up
  the RectTransform hierarchy until `null`. Six hops from a
  wishlist button to `DemoCompleteScreen` to the UI canvas
  root. Works even when the panel's controller class name is
  unknown.
- **`walk_class UnityEngine.UI.Button` filters to active
  buttons in the scene.** Four matches all under the
  demo-end panel = strong signal that the panel is the only
  interactive modal up.
- **`include_inactive: true` on `walk_class` matters** for
  controllers behind toggled GameObjects, but for a panel
  that's currently shown, the default (active-only) is
  fine.

---

## 7.7. Demo-end block (2026-05-14): SOLVED on attempt 7

Resolved after 6 failed strategies. The winning move was
the canonical Harmony "patch upstream" pattern: target
the manager method that fires the panel, not the panel
itself.

### The fix

Harmony prefix returning `false` on:
- `TutorialManager.CompleteDemo`
- `TutorialManager.CompleteDemoCoroutine`

Both live in `unityforge/cs-shim-mono/Plugin.cs`
`InstallDemoCompleteBlock` -> `PatchSingle`. The prefix
(`WwmCompleteDemo_Prefix`) returns false unconditionally,
which tells Harmony to skip the original. The methods
are both managed C# (no extern, no lifecycle), so the
patch sticks and Unity invokes the patched version.

### Discovery path that worked

1. `list_methods` HTTP op enumerates every method on a
   type with declaring-type info.
2. `curl list_methods TutorialManager` listed 15 declared
   methods. Two stood out by name alone:
   `CompleteDemo` + `CompleteDemoCoroutine`.
3. Patch both. Done.

The discovery cost was one curl call. We should have
done this on attempt 1.

### Why this works where the others failed

| Strategy | Why it failed |
|---|---|
| 1-2. Patch DemoCompleteScreenUI.OnEnable / guessed methods | OnEnable inherited from MonoBehaviour, AccessTools returns null |
| 3, 5. Patch declared methods on DemoCompleteScreenUI (Update / Show / etc.) | Harmony issue #374: patching MonoBehaviour lifecycle methods breaks Unity's method-pointer cache. Unity stops invoking them after the patch. Patch "installed" but never fires |
| 4, 6. Patch GameObject.SetActive | `extern` method. Prefix/postfix don't work on extern; only transpiler-only patches do |
| **7. Patch TutorialManager.CompleteDemo (winner)** | Managed C# method, declared on a non-MonoBehaviour-lifecycle class. Standard Harmony prefix works. This is "patch upstream" per the edge-cases doc |

### Lessons

1. **`list_methods` is the right starting move** for any
   "block this UI thing" task. One curl call against the
   most-obvious manager class beats every guess. Add this
   to the methodology.
2. **Don't patch the UI controller**. Find the manager
   that decides to show it.
3. **Don't patch MonoBehaviour lifecycle methods**
   (Awake, Update, Start, OnEnable, OnDisable,
   OnDestroy). Per Harmony #374. Patch a non-lifecycle
   helper method the lifecycle calls into.
4. **Don't patch extern methods**. Prefix/postfix silent
   no-op. Find the managed caller instead.
5. **Defense-in-depth pattern**: also patched
   `TutorialTaskSellItem` and `TutorialTask` declared
   methods with an instance-name filter for "SellGoldBar".
   On a fresh save the task progression never advances
   past SellGoldBar -> `onFinishEvent.Invoke()` never
   fires -> `CompleteDemo` never gets called either.
   `CompleteDemo` patch is the load-time backstop; the
   task-class patches are the progression-time backstop.

### Bridge gaps that remain (queued for next session)

- **`HarmonyBridge.PatchPrefix` in `cs-shim-common/`** still
  uses `new Action(() => del(...))` (instance-method
  lambda). HarmonyLib silently rejects. Every Rust-side
  `patch_prefix` / `patch_postfix` in the workspace is a
  no-op (including wwm-rpg's XP postfixes on
  `DigManager.Dig` + `PlayerManager.AddPlayerCurrency`).
  Replace with a static dispatcher keyed by patch handle.
- **Why the main-thread queue stops draining at
  game-clock ~15s** in the in-scene state. We worked
  around it via static-method shim patches; need to
  diagnose for the long term.

## 7.8. "Plot enlargement" / Land Surveyor search (2026-05-14, partial)

User reported a Land Surveyor board appearing after they
built the Bank, offering "plot enlargement" for $300.
Wanted to know if we can trigger the enlargement without
paying.

### What we tried, what we found

- `walk_class` on ~30 candidate names (`PlotEnlargement`,
  `LandSurveyor`, `Surveyor`, `Sign`, `Board`,
  `Enlargement`, `Expand*`, `Land*`, `Plot*`, etc.) all
  miss. Either the class name doesn't contain those
  substrings, or it's not loaded yet (Addressables
  lazy-loading).
- `Building` exists (253 methods, base class of all
  buildings). `Home`, `Mine`, `MineArea`, `MineDataSO`,
  `Bank` (the prior session captured the Bank as a
  `Building` named "Bank" but as a current `walk_class`
  it returns `type 'Bank' not found`. The Bank may be
  a subclass-by-id, not a class).
- `Banker` class exists but has zero live instances
  (NPCs not spawned yet in this scene state, or named
  differently).
- `Blockade` class IS the wood-beam home blockades you
  remove with money. Methods: `Interact`,
  `CanBeInteracted`, `HandleLongInteraction`. NOT the
  surveyor board; those are for the For Sale blockades
  around the starting plot.
- `LocalizationSystem.GetLocalizedString("plot_enlargement")`
  returned the key echoed back, meaning the key doesn't
  exist with that exact spelling.
- Tried 516 `LocalizedString` entries via
  `list.get_Item` -> `inspect_object`: zero hits for
  "plot" / "enlarg" / "expan" in either Key or Text.
- Grepping `Assembly-CSharp.dll` for "plot" / "enlarg" /
  "survey" returned no matches against the
  UTF-8-stripped string table. The .NET metadata stores
  identifiers in UTF-8 too; this is genuine absence, not
  encoding skew. **Conclusion: the in-game "Plot
  Enlargement" label uses a completely different
  internal name.**

### Most likely explanations

1. The feature uses internal names that don't contain
   "plot" / "enlarg" / "survey" as English roots.
   Possibilities: `Expand`, `Unlock`, `Border`, `Area`,
   `Lot`, plus localized labels via keys that don't
   match the English visible text.
2. The Land Surveyor board is loaded as part of an
   Addressables bundle that only loads when the Bank
   reaches a specific build state. The class exists in
   the assembly but the instance isn't in memory at the
   current scene state.

### Methodology lesson

`walk_class` against substring guesses kept missing.
A `find_types {contains: str}` bridge op (substring
search across all loaded assembly types) would have
short-circuited 20 rounds of guessing. Same lesson as
the DemoCompleteScreenUI hunt; that's the highest-
leverage bridge addition for next session.

### Verdict (after live probing the board): not implemented in demo

After the user built the Bank, walked up to the board,
probed live via HTTP. The "board" sits inside the Bank
prefab itself, not as a separate buyable. Full hierarchy:

```
Bank / CompletedState / CompletedBuilding / Model /
  WoodenBoard /
    PlotOffer      ("Plot enlargement", $300)
    PlotOffer (1)  ("Rocky plot")
    PlotOffer (2)  ("Oil plot")
```

Each `PlotOffer` GameObject has **zero MonoBehaviour
components attached**. The labels (`Label1`, `LabelPrice`)
are just `TextMeshPro` text. There's no controller, no
click handler, no purchase method. The board is **purely
decorative in the demo build**. A teaser for content
that exists only as a visual asset.

### Discovery technique that worked

Direct text scan of all `TMPro.TextMeshPro` instances
for the substring "Plot enlargement". This is the
generally-applicable pattern when:
- The class name is unknown.
- The user can identify the visible label text.
- The trigger isn't event-driven (no UnityEvent to
  trace upstream).

```python
inst = op('walk_class', {'class': 'TMPro.TextMeshPro',
                          'include_inactive': True})
for i in inst:
    text = op('read_field', {'handle': i['handle'],
                              'field': 'text'})['result']
    if 'plot enlargement' in text.lower():
        # walk transform.parent chain to locate
        ...
```

The same technique worked for the wishlist-button hunt
in the demo-end panel investigation.

### Why the user can't buy it

The Land Surveyor / Plot Enlargement isn't a Harmony-
patchable feature missing a button. It's an unwired
asset. To "make it work" would require writing the
purchase + unlock + plot-spawn logic from scratch,
which is a content-creation task, not a modding task.

### Confirmed strings in the assembly

`grep` on `Assembly-CSharp.dll` for "plot" / "enlarg" /
"survey" returned ZERO hits. The text "Plot enlargement"
lives only in localization data (Addressables-loaded at
runtime), not in compiled C# code. There is genuinely no
managed code that references the feature. This is the
fingerprint of a "demo cut" feature: assets shipped,
code stripped.

### Deeper dig (when the user pushed back)

Scanned **every** managed DLL in `Managed/`, not just
`Assembly-CSharp`:

```bash
ls Managed/*.dll | xargs -I{} grep -aoE "Plot[A-Za-z_0-9]*" "{}"
# zero hits across all DLLs
```

Same for `[Ee]nlarg`: zero hits anywhere in compiled
managed code.

Then scanned `level2` (the Unity scene asset). All
"plot" strings found:

```
'- Own several different plots of land and mines'
'Oil plot'
'Plot enlargement'
'PlotOffer'
'PlotOffer (1)'
'PlotOffer (2)'
'PlotOffer (3)'      <- exists in scene but inactive (user sees 3)
'Rocky plot'
```

Plus context dump around each `PlotOffer` showed only
transform / mesh / text data. No `MonoBehaviour` script
GUIDs attached.

Plus context dump around all `[Ss]urvey`: only
`OpenSurvey` (a `SocialsPanelUI` method that opens a
demo feedback URL) and "Survey" in animation rig names.
No `LandSurveyor` class anywhere.

Plus the `Buy*` class scan (looking for whether the buy
panel system might handle plots): found `BuyPopup`,
`BuyPopupUI`, `BuyBedPopup`, `BuyBedPopupUI`, plus a
bunch of `BuySection_*` GameObjects (the standard buy
sections inside Furniture / Food / Blacksmith stands).
No `BuyPlot` or `BuyLand` class. The Plot Offers do NOT
appear in any `BuySection_*`. They're parented under
`WoodenBoard`, not under any building's buy section.

### Final verdict (after deeper dig)

The Plot Enlargement / Land Surveyor feature is
**unambiguously a demo-cut feature**:

- Visual assets present (4 PlotOffer GameObjects, prices,
  labels).
- ZERO managed code references anywhere in any DLL.
- ZERO `MonoBehaviour` scripts on the PlotOffer
  GameObjects in the scene.
- Not wired into the existing `BuyPopup` / `BuySection`
  UI system that handles every other purchase in the
  game.

To make the feature work would require:
1. Writing a new `PlotPurchase` MonoBehaviour with click
   handler, cost check, deduction, and unlock logic.
2. Wiring it onto each `PlotOffer` GameObject at runtime
   via Harmony injection or scene-load patch.
3. Defining what "unlock a plot" actually does (spawn a
   new mine? remove a blockade? load an Addressable
   bundle?). The full game's behavior is unknown.

This is content development, not modding. There is
nothing in the demo binary to patch.

---

## 7.7-OLD. Demo-end block (failed attempts, kept for reference)

Spent a long session trying to block the demo-end panel.
None of six patch strategies worked. Documenting every dead
end so we don't repeat them.

### What we built that DID work

- **`list_methods` HTTP op + bridge entry.** Enumerates
  every method on a type, walking the inheritance chain so
  base-class methods are reported with their declaring
  type. Confirmed `DemoCompleteScreenUI` declares: `Update`,
  `Show`, `FocusCoroutine`, `GetEscapeButtonName`,
  `UpdateEscape`, `<FocusCoroutine>b__5_0` (lambda). Bumped
  bridge ABI to v4.
- **`SO_REUSEADDR` on the HTTP listener bind.**
  `modforge::server::spawn` now builds the TCP socket via
  `socket2` and sets `SO_REUSEADDR` before binding.
  Eliminates the 20-attempt retry that previously failed
  after Windows TIME_WAIT held port 17172 for minutes
  across a hot reload. Confirmed working: gen0 -> gen1 hot
  swap now sees gen1's listener bind cleanly on the same
  port.
- **Hot reload (Rust cdylib) is functional.** Generation
  versioning + condvar-wake shutdown + SO_REUSEADDR makes
  `build_and_deploy.ps1 -Hot` a real iteration loop for
  Rust changes.

### The six patch strategies and why each failed

1. **Harmony prefix on `DemoCompleteScreenUI.OnEnable`.**
   `OnEnable` is a Unity message inherited from
   `MonoBehaviour`. `AccessTools.Method` with the derived
   type returns null. HarmonyX logs:
   `Could not find method for type DemoCompleteScreenUI
   and name OnEnable and parameters`.
2. **Harmony prefix on 15 guessed method names** (`Show`,
   `Open`, `Display`, `Awake`, etc.). Same `Could not find
   method` for 13 of the 15. `Awake` matched the inherited
   `Singleton<T>::Awake` and HarmonyX warned the patch
   would target the base. We didn't proceed.
3. **`list_methods` + patch every method declared on the
   class itself** (Rust-side bridge). All six declared
   methods reported `harmony_patch_prefix(...) failed`.
   Root cause: the `HarmonyBridge.PatchPrefix` in
   `cs-shim-common/HarmonyBridge.cs` constructs the Harmony
   patch from `new Action(() => del(...))`. An
   **instance-method lambda**. HarmonyLib rejects
   instance methods as patch targets (the exception is
   caught and the handle returned is 0). Every Rust-side
   `patch_prefix` call has been silently failing.
4. **Shim-side direct Harmony patch on
   `DemoCompleteScreenUI.Show`** (bypassing the Rust
   bridge). `Patch()` succeeded ("WWM block: patched ...
   .Show"). But the intercept prefix never fired. **`Show`
   is never called**. The panel activates through a
   different code path.
5. **Shim-side patch on all five declared methods
   (`Update`, `Show`, `FocusCoroutine`,
   `GetEscapeButtonName`, `UpdateEscape`).** All five
   patches "installed". None ever fired. So even `Update`
   isn't running on this MonoBehaviour, despite
   `enabled=true`, `isActiveAndEnabled=true`,
   `didAwake=true`, `didStart=true`. Either Unity has
   cached "no Update" at instantiation in a way Harmony
   can't override, OR the controller is structurally
   inactive in a way that doesn't surface to reflection.
6. **Shim-side patch on
   `UnityEngine.GameObject.SetActive(bool)` with a name
   filter.** `Patch()` succeeded. Prefix never fired for
   any GameObject. **`SetActive` is declared `extern` in
   Unity**. It has no IL body, just a native call.
   Harmony cannot detour extern methods. The patch is
   silently a no-op.

### What we still don't understand

- **The actual mechanism the panel uses to appear.** Not
  `Show()`, not `Update`, not direct `SetActive`. Likely a
  Unity-asset-level `UnityEvent` persistent call wired in
  the editor that points at SOMETHING (possibly an
  `Animator`, a custom screen-manager, or
  `GameObject.SetActiveRecursivelyOnHierarchy`). We never
  found the caller.
- **Why managed methods on `DemoCompleteScreenUI` are
  never called by Unity** even though every state flag
  says the MonoBehaviour is active and enabled. This is
  the deepest mystery; needs Unity-internal investigation.

### Live state collateral damage

While the panel is up, `Time.timeScale = 0`. The shim's
main-thread queue stops draining (this we suspected; not
yet proven, but `invoke_method` / `write_field` ops
timed out for the entire session while the panel was
shown). Once the user returned to main menu, the queue
also stopped draining at game-clock ~15s. Update on the
shim's MonoBehaviour ran briefly then stopped. We never
diagnosed why Update stops in the in-game scene.

### Why each strategy actually failed (post-rtfm research)

After the failed session, /rtfm research surfaced two
canonical Harmony gotchas that explain everything:

**Gotcha A: patching MonoBehaviour lifecycle methods breaks
Unity's internal pointer cache.** Per
[pardeike/Harmony#374](https://github.com/pardeike/Harmony/issues/374),
when Harmony patches `Awake` / `Update` / `Start` /
`OnEnable` on a MonoBehaviour, Unity's engine loses the
reference to the original method (it caches a pre-compiled
pointer at MonoBehaviour instantiation) and **stops calling
the method entirely**. Harmony reports the patch as
installed; Unity never invokes it. Strategies 3 and 5 in
the table above are textbook instances of this bug. Don't
patch lifecycle methods on a MonoBehaviour.

**Gotcha B: extern methods can only be patched via
transpilers.** Per the
[Harmony edge-cases doc](https://harmony.pardeike.net/articles/patching-edgecases.html):
> "A method that has only an external implementation (like
> a native Unity method) can normally not be patched. ...
> you can patch native methods with a transpiler-only
> patch that ignores the (empty) input and returns a new
> implementation that will replace the original."

`GameObject.SetActive(bool)`, `Behaviour.set_enabled`,
and `Object.Destroy` are all extern. Prefix/postfix on
them silently no-ops. Strategy 6 above (SetActive name
filter) is doomed.

**The canonical pattern is "patch upstream in the call
chain."** Same doc:
> "find a spot higher up in the call chain that is not
> inlined. This sometimes requires mass-patching all
> occurrences of all methods that call the inlined method
> and patching there (`TargetMethods()` is your friend)."

Translation for our problem: don't patch the panel, don't
patch SetActive. Find the manager class whose managed
method calls `panel.SetActive(true)` (or fires the
UnityEvent that ultimately does), and patch THAT method's
prefix to early-return.

### Next-session attack angles (ranked)

1. **Patch `TutorialTaskSellItem`'s task-complete method
   (whichever name `list_methods` returns).** This is the
   "patch upstream" pattern from the edge-cases doc. The
   class's condition-check method is managed C#, not a
   MonoBehaviour lifecycle method, not extern. Returning
   false from a prefix prevents `onFinishEvent.Invoke()`,
   which means the panel never opens. The list_methods op
   already exists; one curl call names the target.
   Confirmed-applicable per [Harmony #374 Gotcha A]
   and the [edge-cases doc].
2. **Patch `UnityEvent.Invoke()` (managed, on
   `UnityEngine.Events.UnityEvent`).** Filter on the
   specific UnityEvent reference (the SellGoldBar task's
   `onFinishEvent`, handle was 12301 in the
   2026-05-13 session). Hot-path overhead acceptable for
   a per-frame check.
3. **Fix the broken Rust-side
   `HarmonyBridge.PatchPrefix` first.** Replace the
   instance-method lambda with a static dispatcher keyed
   by patch handle. Every Rust-side patch in the
   workspace has been silently failing. Including
   `DigManager.Dig` and `PlayerManager.AddPlayerCurrency`
   postfixes that wwm-rpg relies on for XP tracking.
   Until this lands, anything Rust patches doesn't fire.
4. **Diagnose why Update stops at game-clock ~15s.**
   Add explicit per-frame logging to Plugin.Update.
   Until we know why, the main-thread queue is
   unreliable and we can't drive in-game state changes
   from HTTP.
5. **Add `list_callers` / `find_caller` op.** Given a
   target method (`GameObject.SetActive`), enumerate
   managed methods that reference it via IL. That would
   pinpoint which class calls SetActive on the
   `DemoCompleteScreen` GameObject. Requires IL parsing
   via Mono.Cecil; doable.

### Files touched this session

- `modforge/src/server.rs`. SO_REUSEADDR bind path.
- `modforge/Cargo.toml` + `Cargo.toml`. Added `socket2`.
- `unityforge/src/bridge.rs`. Bumped to v4, added
  `list_methods` fn pointer.
- `unityforge/src/ops.rs`. Added `list_methods` HTTP
  op.
- `unityforge/cs-shim-common/Bridge.cs`. V4 layout,
  `ListMethods` in `IBackendBridge` and `BridgeTable`.
- `unityforge/cs-shim-mono/MonoBridge.cs`. `ListMethods`
  impl walking inheritance chain.
- `unityforge/cs-shim-mono/Plugin.cs`. Six iterations of
  `InstallDemoCompleteBlock` / `DoPatch` /
  `BlockDemoComplete_*Prefix`. Final state patches
  `GameObject.SetActive` with a name filter (no-op
  because extern). Dead code left in for the next
  session to pick up from.
- `wwm-rpg/src/block_demo_end.rs`. Rust-side multi-
  method patch attempt. Currently a no-op (every
  patch_prefix returns 0 due to the
  HarmonyBridge.PatchPrefix bug).

---

## 8. Open research questions

Things this pass did NOT answer:

1. **Digger voxel API.** What does it take to programmatically
   place/remove ore? Does Digger ship public methods, or do we
   need reflection? Open the four `Digger.*.dll` files in
   ilspy and read.
2. **Save file path on disk.** Where does `SaveDat` live?
   Probably `%USERPROFILE%/AppData/LocalLow/<Publisher>/<Game>/`.
   Worth confirming so the mod's snapshot can include "current
   save slot."
3. **Addressables catalog scan.** `StreamingAssets/aa/catalog.bin`
   lists every loadable asset. Does it expose ore/worker
   prefabs by stable string ID? If yes, modifying entity spawn
   tables becomes a string-key edit instead of a code patch.
4. **In-game UI hosting choice.** Does the game's Unity version
   support `UnityEngine.IMGUI` (`OnGUI`) trivially? If yes,
   skip the imgui-bundle-in-DLL trick ueforge uses. Just call
   `GUI.Button` and ship.
5. **BepInEx 5 vs 6.** 5.x is the stable Mono branch and
   matches what every other Unity-Mono mod community ships. 6.x
   is in preview. Confirm 5.x works against this game's Unity
   version before committing.
6. **Game version churn.** This is the Demo. The full game will
   ship with different DLL hashes. Use Harmony patches by name
   and reflection for fields not for IL offsets. Do not embed
   offsets in source.
7. **What's actually fun to mod.** Currency cheat is the joke
   answer. Real interesting mod: dynamic difficulty, worker AI
   overrides, train route editor, biome mods via Digger.

## 9. Next steps (in order)

1. Install BepInEx 5.x against this game; verify a "Hello,
   miner" plugin loads. ~1 hour.
2. Spike: 50-line plugin that calls
   `PlayerManager.Instance.AddPlayerCurrency(1e6f)` on F8.
   ~1 hour. Confirms BepInEx + the singleton pattern + the
   game's mod-loadability before any framework work.
3. Read the Digger API. ~half day. Determines whether
   terraforming mods are within v0 scope.
4. **Create `grounded2mods/unityforge/`.** Walk
   `timberborn/timberbot/src/*.cs`; for each file, decide
   "generic -> unityforge" vs "Timberborn-specific -> stays in
   Timberbot." Copy the generic side into unityforge, parameterize
   on entity-id type, ship as `netstandard2.1`. Aim for the
   first usable cut in ~3-4 days.
5. **Create `grounded2mods/wwm-goldbot/`.** csproj references
   `unityforge` + `Assembly-CSharp.dll` (publicized). Wire one
   GET (player money) and one POST (add money) using the
   shared HTTP server + Jw. ~2 days.
6. Fill in the rest of Goldbot v0: mines list, workers list,
   inventory, webhooks for the three `e_On*` events, settings
   panel, agent launcher. ~3-4 days.
7. Port Timberbot onto `unityforge`. Mechanical refactor in
   Timberbot's repo. ~2-3 days; no behavior change; landed when
   Goldbot has shaken out any unityforge API gaps.
8. Land this doc plus the spike under
   `grounded2mods/wwm-goldbot/docs/` as that crate's first
   doc entry.

## 10. Summary

Wild West Miner is the easiest of the three games to mod. Plain
Mono, public singletons, JSON saves, event-driven state changes,
no DI to navigate. A Goldbot mod is **2-3 weeks** fresh or **~1
week** if we share Timberbot's generic pieces.

A unified `modforge` (UE + Unity in one crate) is a
documentation framing, not real code sharing. Skip it.

`unityforge` (C# sibling to `ueforge`) is the right shape and
we already have two consumers: Timberbot (shipped, stable, the
spec) and Goldbot (about to be built). Extract `unityforge`
**first**, build Goldbot against it from commit 1, port
Timberbot onto it after.

## 11. First Gun demo (2026-08-09)

A new demo ("Wild West Miner Simulator First Gun") was released
before the full launch. Different Steam app, different binary
name (`Wild West Miner Simulator First Gun.exe`), different
`_Data` folder. Still Unity Mono, not IL2CPP.

### 11.1 Build and deploy

The `build_and_deploy.ps1` script now auto-detects the `*_Data`
subfolder so the same script works for any WWM build. Default
path updated to the First Gun demo. Pass `-WwmDir` to target
the old demo or the eventual full release.

BepInEx 5.x Mono installs identically (same doorstop bootstrap).
Mod loads, control plane answers on port 17172.

### 11.2 Field target survival

Verified with `walk_class` + `inspect_object` at main menu:

| Target | Old demo | First Gun |
|---|---|---|
| `DigManager._digRange` (Single, 3.0) | working | **still there** |
| `DigManager.Dig` (Harmony) | working | class exists |
| `PlayerManager.AddPlayerCurrency` (Harmony) | working | class exists |
| `SkillsManager` (native skill system) | working | **still there** |
| `MineDataSO._oreValue` | no instances at menu | class exists, same |
| `PlayerCarryingController._maxCapacity` | working | **GONE** (field removed, class restructured) |
| `WorkersManager._hireCostMultiplier` | working | **GONE** (field removed) |
| `PlayerStaminaController` | working | **GONE** (entire class missing) |

Three skill targets are broken: Strong Back, Charisma, Resilient.
All three were already flagged for migration to `SkillsManager`
proxy effects. The game's native `SkillsManager` singleton
survives and is the correct path forward.

### 11.3 What changed

**Skill enum expanded.** Old demo: `Bag`, `Energy`, `Rope`,
`Speed`. First Gun: `Bag`, `Energy`, `Health`, `Speed`,
`Stamina`, `Weapon`. `Rope` removed, three new entries. The
`SkillsDatabase` still holds exactly 4 active entries (which
4 of the 6 enum values are active needs in-save verification).

**PlayerStaminaController replaced by HealthStaminaManager.**
New singleton under `VoxelDiggingMaster` namespace. Also has
`HealthStaminaSaveData` and `HealthStaminaUI`. The stamina
system was refactored into a proper manager with save support.

**EnergyManager is its own singleton now.** Fields:
`doNotTakeEnergy` (bool), `CurrentEnergyValue` (float, 2.0
at menu). Separate from stamina.

**Carry capacity moved.** `PlayerCarryingController` lost
`_maxCapacity`. The bag system is now `BagDatabase` (singleton,
live at menu) with a `BagData` holding `eqItems` (list). The
`Bag` skill in `SkillsManager` is the right proxy for capacity.

**WorkersManager simplified.** Only has `database:
WorkersDatabase` now. No `_hireCostMultiplier`. Worker hire
cost is likely in the store/upgrade system or a ScriptableObject.

**New managers of interest:**

| Manager | Notes |
|---|---|
| `WeaponsManager` | new, matches `Weapon` skill enum |
| `HorsesManager` | new, `PlayerHorseController` exists |
| `WildAnimalsManager` | new combat/wildlife system |
| `HealthStaminaManager` | replaces `PlayerStaminaController` |
| `MissionsManager` | quest/mission system |
| `PrestigeManager` | prestige/reset system |
| `VillagersManager` | NPC villagers (separate from workers) |
| `WagonsManager` | transport |
| `OrdersManager` | trade orders |
| `BuildingsManager` | building construction |
| `StoreManager` | in-game store (live at menu) |
| `UpgradeManager` | upgrade system (not live at menu) |

**New controllers:** `PlayerWeaponController`,
`PlayerHorseController`, `PlayerRunningController`,
`PlayerHandWagonController`, `HumanSoldierController`,
`WildAnimalController`, `WildAnimalBehaviourController`.

### 11.4 Mod impact and next steps

The three broken skills all have a clear path forward:

| Skill | Old target | New approach |
|---|---|---|
| Strong Back | `PlayerCarryingController._maxCapacity` | `SkillsManager` proxy to `Bag` skill |
| Resilient | `PlayerStaminaController._staminaDrainMultiplier` | `SkillsManager` proxy to `Stamina` or `Energy` skill, or raw field on `HealthStaminaManager`/`EnergyManager` |
| Charisma | `WorkersManager._hireCostMultiplier` | Needs research: check `StoreDataSO` or `UpgradeManager` for hire pricing |

The two working skills (`Quick Pickaxe` via `DigManager._digRange`,
`Greedy Miner` via `MineDataSO._oreValue`) and the two Harmony
hooks (`DigManager.Dig`, `PlayerManager.AddPlayerCurrency`) are
unchanged.

New mod opportunities from the expanded game: weapon damage
scaling, horse speed, mission rewards, wild animal behavior.
All need in-save field verification before targeting.

## 12. 1.0 release (2026-08-13)

The game left early access. New Steam app, installed at
`C:\Games\Steam\steamapps\common\Wild West Miner Simulator\`,
data folder `Wild West Miner Simulator_Data`, exe
`Wild West Miner Simulator.exe`. Still Unity Mono, still
BepInEx 5.x doorstop. `Assembly-CSharp.dll` is 1,036,800 bytes
against the First Gun demo's 978,432.

Everything below was measured in a loaded save (slot 2) by
`wwm-mod/tests/research_release_targets.rs`, which is the
permanent probe for this question. Re-run it after any game
patch:

```text
cargo test -p wwm-mod --test research_release_targets -- --test-threads=1 --nocapture
```

### 12.1 Two research-harness bugs that faked a dead game

The first two runs reported every class gone or empty. Both
were the harness, not the game, and the fixes are in
`wwm-mod/tests/common/mod.rs`:

1. `walk_class` answers `{class, instances: [...]}`, not a bare
   array. Parsing the response as an array yielded an empty
   list for every class, which read as "zero live instances".
2. The release parks its managers on inactive GameObjects.
   `walk_class` must be called with `include_inactive: true`
   or `FindObjectsOfType` returns nothing.

A third trap: in a loaded world some walks take longer than the
client's default timeout, and a timed-out op is not a missing
type. `try_walk` now separates "type not found" from any other
failure, and the test asserts that a made-up class name really
does fail (`type 'NoSuchClassZzz' not found`), so a successful
walk proves the type exists.

### 12.2 Declared target survival

| Skill | Target | 1.0 status |
|---|---|---|
| Quick Pickaxe | `DigManager._digRange` | **works**, reads 3.0 (same vanilla value as both demos) |
| slot key | `GameSerializationSystem._currentLoadedSaveNumber` | **works**, read 2 in the test save |
| Strong Back | `PlayerCarryingController._maxCapacity` | class alive, **field gone** |
| Charisma | `WorkersManager._hireCostMultiplier` | class alive, **field gone** |
| Resilient | `PlayerStaminaController._staminaDrainMultiplier` | **class gone** |
| Greedy Miner | `MineDataSO._oreValue` | class alive, **field does not exist** (see 14.1) |

Same three casualties as First Gun, plus Greedy Miner, whose
field turned out never to have existed on this build.

Of the two Harmony postfix targets only
`PlayerManager.AddPlayerCurrency` is real. `DigManager.Dig`
does not exist; the first version of this section said it did,
on a probe bug corrected in 14.1.

### 12.3 SkillsManager survived intact

The native skill system is unchanged from the demos, which
keeps the proxy plan alive. Live singleton, `isLoaded` true in
a save, two fields (`skillsData: SkillsDataSO`,
`database: SkillsDatabase`), and this declared surface:

| Method | Params | Returns |
|---|---|---|
| `LoadSkills` | 0 | Void |
| `SetSkillLevel` | 2 | Void |
| `LevelUpSkill` | 1 | Void |
| `AfterChangeSkillLevel` | 1 | Void |
| `GetCurrentSkillLevel` | 1 | Int32 |
| `GetCurrentSkillValue` | 2 | Single |
| `GetSkillValue` | 3 | Single |
| `GetSkillPrice` | 2 | Single |
| `GetSkillLevelsAmount` | 1 | Int32 |

### 12.4 Manager survey in a loaded save

Live: `SkillsManager`, `WeaponsManager`, `HorsesManager`,
`WildAnimalsManager`, `EnergyManager`, `MissionsManager`,
`PrestigeManager`, `VillagersManager`, `WagonsManager`,
`OrdersManager`, `BuildingsManager`, `StoreManager`,
`BagDatabase`, `PlayerManager`, `DigManager`.

Type exists but no instance in this save: `HealthStaminaManager`,
`UpgradeManager`.

`list_singletons` disagrees with `walk_class` on a few of
these (`HealthStaminaManager`, `MissionsManager`,
`VillagersManager`, `UpgradeManager`, `BagDatabase` report
`found: false, type_found: true` through the singleton path
while the scene walk finds most of them). The singleton path
reads `Singleton<T>.Instance`, so a false there means the
manager has not run `Awake` yet, not that it is absent.

### 12.5 Where this leaves the mod

Nothing regressed relative to First Gun, so the plan from
section 11.4 stands: ship `UnitySkillProxyEffect` and repoint
Strong Back, Resilient and Charisma at `SkillsManager`
instead of raw fields. Still unknown and needed before that:
which skill ids the release's `SkillsDataSO` actually defines
(First Gun's enum was `Bag`, `Energy`, `Health`, `Speed`,
`Stamina`, `Weapon`, with only 4 active in the database).

## 13. The 1.0 native skill system, in full (2026-08-13)

Measured in slot 2 by `wwm-mod/tests/research_skills.rs`. This
is the input for the `UnitySkillProxyEffect` that replaces the
three dead raw-field skills.

### 13.1 Shape

```
SkillsManager (singleton MonoBehaviour)
  skillsData : SkillsDataSO      authored catalog, one entry per skill
    skillDatas : List<SkillDataSO>
      skillType  : SkillType     enum: Bag | Energy | Rope | Speed
      skillDatas : List<SkillData>   one entry per level
        value  : Single
        value2 : Single
        price  : Single
  database : SkillsDatabase      live per-save state
    skillsData : SkillsData
      skillsData : Dictionary<SkillType, Int32>
```

`SkillsDataSO.GetSkillData(1)` maps a skill to its catalog
entry. `SkillsData` has its own `GetSkillLevel` / `SetSkillLevel`
/ `LevelUpSkill`, and `SkillsDatabase` adds `SaveData(1)` /
`LoadData(1)`, so a level written through the manager persists
with the game's own save, not ours.

### 13.2 The four skills

1.0 ships **Bag, Energy, Rope, Speed**. That is the original
demo's set: `Rope` is back and the First Gun enum's `Health`,
`Stamina` and `Weapon` are not in the catalog. Levels are
1-based; a level's row is at index `level - 1`.

| Skill | Levels | `value` per level | `value2` per level | Price per level |
|---|---|---|---|---|
| Bag | 5 | 5, 7, 9, 12, 15 | 2, 3, 4, 5, 6 | 0, 50, 200, 500, 1000 |
| Energy | 6 | 40, 50, 60, 70, 80, 90 | 0 | 0 (free at every level) |
| Rope | 5 | 1.0, 1.1, 1.2, 1.35, 1.5 | 0 | 0, 50, 200, 500, 1000 |
| Speed | 5 | 1.0, 1.05, 1.1, 1.15, 1.2 | 1.0, 1.25, 1.5, 1.75, 2.0 | 0, 50, 200, 500, 1000 |

Bag `value` is the backpack slot count (5 at level 1, matching
the demo finding that the player starts with 5 slots). Rope and
Speed `value` read as multipliers against 1.0. Energy `value`
is a capacity, and every Energy level costs nothing, which
suggests Energy is not bought in the skills shop but granted by
something else.

State read in the test save: Bag 1/5, Energy 2/6, Rope 1/5,
Speed 1/5.

### 13.3 The control plane can drive it by enum name

`invoke_method` accepts the enum as a plain string, so no
handle juggling is needed for the skill argument:

```json
{"op":"invoke_method","args":{"handle":<SkillsManager>,
 "method":"GetCurrentSkillLevel","args":["Bag"]}}
```

Verified working for `GetCurrentSkillLevel(skill)`,
`GetSkillLevelsAmount(skill)` and
`GetCurrentSkillValue(skill, bool)`, where the bool picks
`value2` over `value`.

### 13.4 What is still unverified

`SetSkillLevel` had not been called on the 1.0 build when this
section was written. It has been since: see section 15, it
works.

Mapping the mod's own skills onto these four is also still
open. Bag is the obvious backing for Strong Back. Resilient
maps to Energy, though the zero prices there need explaining
first. Charisma has no counterpart in this catalog at all, so
it needs either a different vanilla system (worker hire cost
moved out of `WorkersManager`) or a different design.

## 14. Static decompile of the 1.0 assembly (2026-08-13)

Live probing answers "does this field exist"; it cannot answer
"where does this value come from". `ilspycmd` decompiles the
whole game to 802 C# files in seconds, so from here the game's
own source is the authority and the control plane is for
confirming behaviour, not for discovery.

```sh
ilspycmd -p -o <outdir> \
  "C:\Games\Steam\steamapps\common\Wild West Miner Simulator\Wild West Miner Simulator_Data\Managed\Assembly-CSharp.dll"
```

802 files, 696 in the global namespace, 83 under
`VoxelDiggingMaster`, the rest third-party asset packs
(EpicToonFX, KevinIglesias, StarterAssets, SatProductions).

### 14.1 Two corrections to section 12

**`DigManager.Dig` does not exist and never did on this build.**
Section 12.2 reported it present. That was a bug in the probe:
it substring-matched the method list, and `CanDig`, `HandleDig`,
`DigDynamite`, `OnDigCompleted` and `DigPrologue` all contain
"Dig". The test now compares names exactly and reports
`Dig: NOT LISTED`. `PlayerManager.AddPlayerCurrency(float)` is
real and confirmed by exact match.

**`MineDataSO` has no `_oreValue` field.** Section 12.2 listed
the field as unverified because the ScriptableObject has no
scene instance. The source shows the class holds `id`,
`availableByDefault`, `canDigWithShovel`, `chanceFactor`,
`itemDropDatas`, the petroleum settings and the dynamite
settings. Nothing resembling an ore value. Greedy Miner has no
target on this build (see 14.5).

### 14.2 The skill system, from source

`SkillType` is exactly four values: `Bag`, `Energy`, `Rope`,
`Speed`. `SkillsData`'s constructor seeds every enum value at
level **1**, so levels are 1-based and `GetSkillValue` indexes
`SkillDatas[level - 1]`.

`SkillsData` is `[Serializable]` with a `[JsonProperty]`
`Dictionary<SkillType, int>`, so levels ride in the game's own
Newtonsoft save.

Neither `SkillsManager.SetSkillLevel` nor `SkillsData.SetSkillLevel`
clamps. Passing a level above `GetSkillLevelsAmount` stores fine
and then throws `ArgumentOutOfRangeException` the next time
anything reads the value. **A proxy Effect must clamp to
1..GetSkillLevelsAmount(skill) itself.**

`SetSkillLevel` calls `AfterChangeSkillLevel`, which is what
makes a level change take effect live:

| Skill | What `AfterChangeSkillLevel` does |
|---|---|
| Bag | `InventoryCounterUI.UpdateCounter()`, `HandsPanelUI.RefreshBagLimits()` |
| Energy | `EnergyManager.RestoreFullEnergy()` and a "max_energy_up" message |
| Rope | `PlayerController.SetCurrentRopeSpeed()` |
| Speed | `PlayerController.SetCurrentSpeed()` and `PlayerRunningController.UpdateMaxRunPoints(value2)` |

### 14.3 What each skill value actually drives

Every consumer in the assembly:

| Skill | Field | Consumer | Effect |
|---|---|---|---|
| Bag | `value` | `Bag.GetSlotsAmount()` | inventory slot count, read live on every `HasFreeSpace()` |
| Bag | `value2` | `BeersManager`, `DynamitesManager` | how many beers / dynamites the player can carry |
| Energy | `value` | `EnergyManager.GetMaxEnergyValue()` | energy ceiling |
| Rope | `value` | `PlayerController.SetCurrentRopeSpeed()` | `currentRopeSpeed = baseRopeSpeed * value` |
| Speed | `value` | `PlayerController.SetCurrentSpeed()` | `playerCurrentSpeed = baseSpeed * value` |
| Speed | `value2` | `PlayerRunningController.UpdateMaxRunPoints()` | sprint points ceiling |

Because `Bag.GetSlotsAmount()` recomputes from the skill on
every call, raising the Bag level takes effect instantly with
no field write and no reload. That is exactly the behaviour the
old demo session observed.

Energy costing 0 at every level is explained: Energy is not
bought in the skills shop. `BedsManager` sets the Energy skill
level from the bed the player owns
(`SetSkillLevel(SkillType.Energy, bedData.EnergySkillLevel)`),
and `Home` forces Energy to at least 2 on first entry. Buying a
better bed is the Energy upgrade path. Anything the mod writes
to Energy will be overwritten the next time a bed is bought or
the house is entered.

`StatsManager` hardcodes "/5" in the stats window for Speed,
Bag and Rope, so a mod-driven level above 5 will display wrong
there even where it works mechanically.

### 14.4 Worker hire cost, for Charisma

Hire cost left `WorkersManager` because it was never a field.
`HireWorkerPopupUI` hardcodes three offers, `Hire1/2/3` calling
`Hire(30f, 1)`, `Hire(80f, 3)`, `Hire(150f, 7)`. `Hire` prorates
against the current day progress, rounds, checks
`PlayerManager.CanPlayerBuy(price)` and pays with
`RemovePlayerCurrency(price)`.

So Charisma is a Harmony prefix on
`HireWorkerPopupUI.Hire(float price, int days)` that scales the
`price` argument. There is no field to write and no vanilla
skill to proxy.

### 14.5 Ore value, for Greedy Miner

Ore is an item, not a mine property. `ItemDataSO.GetPrice()` is
virtual; `ResourceItemDataSO` overrides it as
`RoundToInt(maxWeight * pricePerKg)` and `MaterialItemDataSO`
returns a flat `price`. Selling goes through `SellingNpc` /
`BuySellNpc`, and `PrestigeManager.GetFinalPrice` scales store
prices on top.

Greedy Miner therefore has two possible shapes, neither
implemented: a Harmony patch on the sell path, or a write to
`ResourceItemDataSO.pricePerKg`. The second is dangerous:
ScriptableObject edits are asset-level and outlive the save.

### 14.6 Dig flow, for the XP hook

`HandleDig()` gates on energy and tool readiness, then plays the
animation whose callback is `DigShovel()`. `DigShovel` modifies
the voxel density and fires the static event
`DigManager.OnDig`. The voxel system calls back into
`OnDigCompleted(center, radius, voxelsAffected, chunksAffected)`,
which spawns the resource item, takes 1 energy, plays the dust
and the sound.

For "+XP per swing" the honest targets are the static `OnDig`
event or a postfix on `DigShovel`. `OnDigCompleted` also fires
for dynamite digs (guarded by a `dynamite` flag), so it counts
more than swings.

### 14.7 Systems present but unexplored

`PrestigeManager` (price scaling, `GetFinalPrice`),
`MissionsManager` + `MissionsCenter` + `MissionTarget`,
`WeaponsManager` / `PlayerWeaponController` / `WeaponDataSO`,
`HorsesManager` / `HorseDataSO`, `WildAnimalsManager` with a
seven-state behaviour machine
(`Stand`, `Patrol`, `Travel`, `Chase`, `Attack`, `Hit`, `Dead`),
`SteamAchievementsManager`, `SpecialEventsManager`
(`SpecialEvent_GovernorAssassination`), the minigames
(`BottleShootingMinigame`, `DartsMinigame`), `BarrelsManager`,
`FurnacesManager`, `TrainDatabase` / `TracksManager`.

`PlayerManager` declares a `e_OnPlayerExpChanged` event that
nothing else in the assembly subscribes to or raises, so the
game has no player XP system despite the hook being there.

## 15. SetSkillLevel works on 1.0 (2026-08-13)

The one call the proxy plan depends on, now proven on the
release build. `wwm-mod/tests/research_skill_write.rs`, run
against slot 2:

```text
before: level=1 value=Some(5.0) slots=Some(5)
SetSkillLevel(Bag, 3): ok=true None
after:  level=Some(3) value=Some(9.0) slots=Some(9)
restore to 1: ok=true level=Some(1) slots=Some(5)
```

Three things are confirmed at once. The write lands
(`GetCurrentSkillLevel` returns 3). The catalog lookup follows
it (`GetCurrentSkillValue` returns 9.0, the third Bag row from
13.2, which also re-confirms the `level - 1` indexing). And the
game acts on it: `Bag.GetSlotsAmount()` reports 9 immediately,
with no reload, no field write and no save/load cycle.

The test restores the original level before it asserts, so a
failure cannot leave the player's save modified, and it asserts
the restore too.

Still not covered by this test: whether the level survives a
save and reload (the write goes through `SkillsDatabase`, which
owns `SaveData`/`LoadData`, so it should, but "should" is not
evidence), and whether prestige resets it.
