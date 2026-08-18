# schedule1-mod: Rust RPG + faction war for Schedule 1

> **Status:** plan, 2026-08-07. Sibling to
> [`unityforge-plan.md`](unityforge-plan.md). Work items live in
> [`schedule1-todo.md`](schedule1-todo.md).

## Context

Schedule 1 (TVGS) joins the modforge workspace as the IL2CPP
proof target, with two gameplay goals from the operator:

1. Standard RPG levelling (the modforge rpg model), with XP from
   combat only.
2. A living world: factions that control parts of town with NPCs,
   NPCs that fight the player for control, NPCs that fight each
   other for control, so the game changes even when the player
   does nothing. Harder overall: more frequent attacks, tougher
   scaling enemies, territory pressure.
3. The FF7 grind loop (added 2026-08-07): regions hold farmable
   hostile mobs, kills drop loot (cash/items), the player runs an
   area killing and levelling, then graduates to taking regions
   from factions. Mobs and loot ride the vanilla machinery
   (cartel NPCs, item pickups/dead drops), never spawned from
   nothing that the game cannot render or persist.

Standing design principle (operator, 2026-08-08): the operator
designs these mods and then plays them, so anything fully known
is mentally solved and boring. Preventing THEIR boredom is the
main point. Every system must therefore stay interesting to
someone who knows its rules:

- Rolled, not authored: mob affix packs, loot, and rare events
  roll from hidden tables at runtime (the Diablo/PoE model).
- Reactive: the director reads the player's actual state
  (level, cash, territory, deaths) and pushes back; its input
  is the player, so its behavior cannot be pre-solved (the
  Left 4 Dead AI Director + RimWorld Randy Random model; the
  plan's director split).
- Emergent: NPC-vs-NPC faction war runs without the player, so
  each save's world diverges on its own.
- Spoiler firewall: the operator approves system SHAPES; exact
  numbers, tables, affix lists, and rare-event triggers live in
  code and in clearly marked doc sections the operator skips.

Constraints, settled with the operator:

- Stay on the IL2CPP default branch. The operator's 7 Vortex mods
  (eMployee, DealerPlus, Fat Stacks, Infinite ATM, Mod Manager,
  DealersRecruitCustomers, S1API) are IL2CPP-only and run under
  MelonLoader; they must keep working. BepInEx is therefore not
  an option for this game; the shim needs a MelonLoader entry.
- All feature code in Rust. C# is only the thin shim.
- The existing C# EmployeeReset mod (Schedule1Mods repo) stays
  as-is. It is a bug fix for eMployee, not a feature mod.

What already exists in this workspace:

- `modforge/src/rpg/`: the engine-agnostic RPG core (xp, skills,
  effects, triggers, tracker, store).
- `unityforge/`: the Unity Rust SDK with `cs-shim-mono`,
  `cs-shim-il2cpp` (BepInEx 6 flavor, written), `cs-shim-common`
  (Bridge, HarmonyBridge, GenerationLoader shared sources).
- `il2cpp-smoke/`: a finished smoke crate (ping, walk_class,
  read/write field, one Harmony postfix) waiting for a target.
- `docs/todo.md`: known gap, the IL2CPP shim lacks the
  generation-loader that the Mono shim has.
- `unityforge-plan.md` deferred a "MelonLoader shim variant"
  until a target demands it. Schedule 1 demands it now.

Prior art for the gameplay: `survivalist-mod`'s faction-war
design (AI factions that grow, fight, and are destroyed under the
same rules as the player; the Randy Random + Mario Kart director
split) and the modforge rpg model shipped in grounded2-mod.

## The work, in order

### 1. MelonLoader entry for the IL2CPP shim

New C# project `unityforge/cs-shim-melonloader/` (net6.0), a
MelonMod that does exactly what `cs-shim-il2cpp/Plugin.cs` does,
reusing the shared sources:

- `OnInitializeMelon`: locate the Rust cdylib
  (`*.unityforge.dll` next to the mod or via `UNITYFORGE_TARGET`),
  `NativeLibrary.Load`, resolve `unityforge_init/tick/shutdown`,
  build the bridge from `Il2CppBridge` (linked from
  `cs-shim-il2cpp`), call init.
- `OnUpdate`: tick. `OnApplicationQuit`: shutdown.
- Links `cs-shim-common/*.cs` (Bridge, HarmonyBridge, Logger,
  InputBridge, GenerationLoader) the same way the other shims do.

Known risks to verify first:

- `HarmonyBridge.cs:84` uses a HarmonyX-only API. MelonLoader
  ships its own Harmony fork under the same `HarmonyLib`
  namespace. If the call is missing there, add a
  reflection-probed fallback in HarmonyBridge (shared file, so
  guard it, do not fork it).
- MelonLoader's Il2CppInterop version vs the one the shim was
  written against. Resolve references from the game's
  `MelonLoader/net6/` dir like the Schedule1Mods csproj does,
  not from NuGet.

### 2. Generation-loader parity

Mirror the generation-loader wiring into the MelonLoader entry
(the existing todo.md item), so Rust-side hot reload works the
same as the Mono shim. Rust-only changes then need no game
restart.

### 3. Smoke on Schedule 1

Deploy `il2cpp_smoke.unityforge.dll` + the MelonLoader shim into
the operator's Schedule 1 `Mods/` folder, alongside the 7
existing mods. Curl checklist from the crate: `ping`,
`smoke_state` (runtime tag = IL2CPP), `walk_class` on a known
game type, `smoke_read`/`smoke_write` round trip, postfix fire
counter increments. The operator launches the game; the agent
never runs it. Exit gate: all checks answer, and the 7 existing
mods still load clean in the MelonLoader console.

### 4. Research Schedule 1 internals

New crate `schedule1-mod/` (cdylib, depends on unityforge +
modforge, own HTTP port). First deliverable is
`schedule1-mod/docs/research.md` plus
`schedule1-mod/docs/certainty-tracking.md` (the discipline from
the Schedule1Mods repo: every claim about vanilla is
evidence-cited or marked hypothesis).

Research questions, answered via the control plane
(walk_class / inspect_object / read_field) plus dnSpy over the
MelonLoader-generated Il2CppInterop assemblies:

- Map regions: what class owns the town's areas and what state
  they carry.
- NPCs: how NPCs spawn, path, and despawn; what the cartel/goon
  NPC classes are (the vanilla cartel update added hostile NPCs
  and ambushes; find the classes that drive them).
- Combat: health, damage application, death, and aggro classes
  for player and NPCs.
- Where kills are observable (the Harmony hook point for
  combat XP).

### 5. Combat RPG levelling

On `schedule1-mod`, instantiate `modforge::rpg` with unityforge
effects, same shape as the wwm-mod plan in `unityforge-plan.md`:

- XP source: one Harmony postfix on the NPC-death path found in
  research. Combat kills only, per the operator.
- Skills: player combat stats as SkillDefs (max health, damage,
  toughness and similar), applied via the unityforge field
  effects. The exact skill list is drafted after research names
  the real fields; it uses the standard xp curve and store.
- Persistence: JSON per save slot via `modforge::rpg::store`.

### 5b. Loot drops and mob farming areas

The grind loop, after levelling works and before the full
faction war:

- Loot: NPC kills drop cash/items via the vanilla pickup or
  dead-drop path found in research. Simple loot table first
  (cash amount scaled by mob toughness), items later.
- Mob farming: each region holds hostile mobs spawned through
  the vanilla cartel/NPC spawn machinery, with per-region
  density, respawn timers, and stats scaled to player level.
  Verified in-game: walk into a region, mobs are there, kill
  them, they respawn after the timer.

### 6. Faction war

The big feature, modeled on survivalist-mod's faction-war design,
built incrementally behind the control plane so each piece is
testable:

- Factions own regions of town. Ownership is derived state held
  by the mod (idempotent from live game state where possible),
  visible via an op (`faction_state`).
- Faction NPCs contest regions: squads spawn and fight the
  region's owner, including each other (NPC vs NPC), using the
  vanilla NPC + combat machinery found in research. No cheating:
  strength comes from held territory, not free spawns from
  nowhere.
- Player pressure: attacks on the player and their dealing areas
  become more frequent; losing a region costs the player
  (customers/dealers in that region). Enemy stats scale with
  player level so combat never trivializes.
- Director split per the survivalist design: unpredictable event
  rolls vs adaptive pressure on whoever is winning, kept as two
  separate layers, never merged.

Faction war lands in slices (ownership map first, then
NPC-vs-player contests, then NPC-vs-NPC), each verified in-game
by the operator before the next.

## Verification

- Shim + smoke: the curl checklist in `il2cpp-smoke/src/lib.rs`
  against the live game, driven as repo tests via the modforge
  client (no ad-hoc probes); MelonLoader console clean; existing
  7 mods unaffected.
- RPG: `skill_state` op round trip; kill an NPC in-game, XP
  increments; level a stat, field change visible in-game;
  save/reload persists.
- Faction war: `faction_state` op reflects live ownership;
  operator observes an NPC-vs-NPC fight and a territory loss
  event in-game. Every claim logged into certainty-tracking.md
  before it is called done.
- Every step: operator launches the game and reports; the agent
  reads the MelonLoader log, never runs the game.
