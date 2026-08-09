# schedule1-mod performance budget

every recurring operation in the mod, what it costs, and how
often it fires. the goal: seamless, invisible to the player.

## per-frame (every unity Update)

| what | cost | notes |
|------|------|-------|
| `on_tick` -> `farming::tick()` | near zero most frames | checks SETTLED atomic, then checks LAST_PASS elapsed. both are in-process reads with no IL2CPP calls. returns immediately ~99.6% of frames (4s gate at 60fps = 1 in 240 frames passes through) |

**verdict**: the per-frame path is two atomic loads and a
duration compare. no allocation, no IL2CPP, no string work.
negligible.

## every 4 seconds (war pass)

when the PASS_EVERY gate opens, `war_pass()` runs once. this is
the heaviest recurring path.

| step | IL2CPP calls | notes |
|------|-------------|-------|
| player position read | 3 (walk Player, read transform, get_position) | one walk + two chained field reads |
| region scan (per region, 5 regions) | 1 each (`GetInfluence`) | each calls `cartel_influence_instance()` which does a walk of CartelInfluence (one instance), then invokes GetInfluence. that is 2 IL2CPP calls per region = 10 total |
| spawn check | 1 lock of FORCES per region | in-process, no IL2CPP |
| spawn (0-2 per pass, capped by SPAWNS_PER_PASS) | ~3-4 per spawn | `invoke_static` for factory call, factory does the heavy lifting on the C# side |
| hold_posts aggro scan | 1 lock of FORCES | in-process distance math per mob, no IL2CPP. only IL2CPP calls happen when aggro fires (factory AttackPlayer) or affixes land (factory SetToughness/Arm) |

**total per pass**: ~13-15 IL2CPP calls baseline (player pos +
5 regions' influence reads), plus 3-4 per spawn (0-2 spawns),
plus occasional affix/aggro factory calls. worst case ~25
IL2CPP calls every 4 seconds.

**concern**: `cartel_influence_instance()` does a full type walk
every call. that is `MonoType::find` + `walk(false)` to locate
the one live CartelInfluence instance. called once per
`get_influence` and once per `change_influence`. during the
region scan, that is 5 walks per pass just for influence reads.
caching the handle across passes would cut this to 1 walk per
pass (or zero if cached across the session with a staleness
check).

## every 2 seconds (slot poller)

`SlotPoller` runs on its own Rust thread, polling every 2
seconds.

| step | cost | notes |
|------|------|-------|
| `resolve_slot` | 1 main-thread-queued IL2CPP call | checks `LoadManager.IsGameLoaded` + `LoadedGameFolderPath` via MAIN_QUEUE. fires once, blocks the poller thread until the main thread picks it up |
| on slot change | effect application | only fires when save slot changes (load, new game). applies skill effects via main-thread queue. rare |

**verdict**: 1 IL2CPP call every 2 seconds, queued to main
thread. low cost.

## permanent harmony prefixes (every NPC health event, game-wide)

three Harmony prefixes patched onto `NPCHealth`:

| prefix | fires when | cost per fire |
|--------|-----------|---------------|
| `NotifyAttackedByPlayer` | any player melee/ranged hit on any NPC | 1 handle acquire + `npc_ptr` (1 field read chain). stores ptr+timestamp in PLAYER_HITS vec |
| `Die` | any NPC death | `npc_info`: 1 handle acquire + 3-4 field reads (MaxHealth, npc, transform, get_position). plus PLAYER_HITS + CREDITED vec scans |
| `KnockOut` | any NPC knockout | same as Die |

**these fire on ALL NPCs in the game, not just garrison mobs.**
every vanilla NPC fight, every police chase, every random
brawl triggers `NotifyAttackedByPlayer` for each punch. the
callback is cheap (one field read + vec push) but at high
density with many NPCs fighting, the frequency adds up.

**cost per punch**: ~2 IL2CPP calls (handle acquire, field
read for npc ptr) + one mutex lock + one vec scan/push.

**cost per death/knockout**: ~5-6 IL2CPP calls (handle acquire,
MaxHealth read, npc field read, transform read, get_position
invoke) + two mutex locks + two vec scans + potential
on_mob_down (which locks FORCES + REGIONS) + potential loot
drop queue + potential XP record.

**concern**: the vec scans in PLAYER_HITS and CREDITED are
linear. with many hits the vec grows until entries expire
(HIT_WINDOW=15s, CREDIT_COOLDOWN=60s). in a busy fight this
could be dozens of entries. not a problem now but would be if
NPC density scaled up significantly.

## one-shot per kill (loot drop)

`drop_cash_at` queues work onto MAIN_QUEUE. runs once per
credited kill, on the next frame.

| step | IL2CPP calls | notes |
|------|-------------|-------|
| find template | 1 (`MonoType::find` + `walk(true)`) | walks ALL CashPickup instances to find the template by name. iterates the list, releasing handles for non-matches |
| clone | 1 (`Object.Instantiate`) | |
| re-find clone | 1 (second `walk(true)`) | walks ALL CashPickup instances AGAIN to find the "(Clone)" by name. same iteration + handle release |
| activate + position | 3 (get_gameObject, SetActive, set_position) | |
| network spawn | 3 (GetComponent NetworkObject, get_ServerManager, Spawn) | |
| set value + visuals + rename | 3 (write_field Value, UpdateCashStackVisuals, set_name) | |

**total**: ~12 IL2CPP calls per loot drop, plus two full walks
of all CashPickup instances in the scene.

**concern**: the double walk of CashPickup instances is the
most expensive single operation in the mod. each walk iterates
every CashPickup in the scene (could be dozens if many loot
drops are sitting uncollected). the second walk exists only to
re-find the clone by its "(Clone)" name suffix because
`Object.Instantiate` returns a base-typed handle. caching the
template handle and finding a way to use the Instantiate return
directly would eliminate both walks.

## not always active (research diagnostics)

`combat_trace.rs` ops (`combat_trace_start`, `report`, `stop`)
are research-only. when active, they add a Harmony prefix on
`NPCHealth.TakeDamage` that fires on every damage tick
game-wide. not active during normal play; no recurring cost
unless explicitly started via an op.

## summary: where the CPU goes

ranked by recurring cost (highest first):

1. **war pass influence reads** (every 4s): 10 IL2CPP calls for
   5 regions, each doing a redundant CartelInfluence type walk.
   the single best optimization is caching the CartelInfluence
   handle.

2. **harmony prefixes on NotifyAttackedByPlayer** (every punch
   on any NPC): cheap per call but highest frequency. no IL2CPP
   optimization possible (the callback IS the Harmony prefix),
   but the vec operations could use a fixed-size ring buffer
   instead of a growable vec.

3. **loot drop double walk** (per kill): 2 full scene walks of
   CashPickup instances. only fires on credited kills so
   frequency is low, but per-occurrence cost is the highest in
   the mod.

4. **slot poller** (every 2s): 1 IL2CPP call. negligible.

5. **per-frame tick gate** (every frame): no IL2CPP. negligible.

## optimization targets (not yet implemented)

| target | saves | complexity |
|--------|-------|-----------|
| cache CartelInfluence handle (with staleness check on scene change) | ~10 walks per pass -> 0 | low |
| cache CashPickup template handle | 2 scene walks per loot drop -> 0 | low |
| use Instantiate return handle directly (skip re-find walk) | 1 scene walk per loot drop -> 0 | medium (need to verify IL2CPP handle type compatibility) |
| fixed-size ring buffer for PLAYER_HITS | removes vec growth during sustained combat | low |
| batch influence reads (read all 5 in one call if the game exposes a bulk API) | 10 calls -> 1 | unknown (needs research into CartelInfluence API) |
