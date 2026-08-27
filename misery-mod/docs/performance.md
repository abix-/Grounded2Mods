# What the mod does every frame, and what it costs

Written 2026-08-26, after the game felt like it was freezing.

Read this before blaming the mod, and before defending it. The
right-hand column says which numbers are MEASURED and which are
ESTIMATES. Nothing here has been profiled yet; the estimates are
read off the code, and the last section says how to replace them
with real numbers.

Two things decide the cost of anything in this list:

- **Which thread it runs on.** Work on the game thread happens
  inside a frame. At 60 frames a second a frame is 16 ms, so 16 ms
  of our work is one frame lost. Work on our own threads costs the
  player nothing directly.
- **Whether it walks the object list.** A walk visits every UObject
  the game has loaded. We never counted how many that is in a
  streamed-in MISERY world, so every walk below is an unmeasured
  cost, not a small one.

## Everything that runs

| What | How often | Thread | Cost | Measured? |
|---|---|---|---|---|
| Drain the job queue from `UEngine::Tick` | every frame | game | Returns immediately when the queue is empty. Otherwise the queued job's own cost, inside that frame. | estimate; the drain is already timed, see below |
| UE4SS `on_update` | every frame | UE4SS's own | A counter, plus a retry of the tick-hook install until it succeeds. | estimate, near zero |
| The playtest notice hook | every widget function call, all session | game | Reads the object's class name and compares one string, then forwards. Blueprint widget classes share one vtable, so this fires for EVERY UserWidget, not just the notice. Never removed after the notice is gone. | estimate. Frequency unknown and potentially very high in menus |
| `spawning` watcher | every 5 s | game | TWO full object-list walks per pass: one over `BP_MasterAICharacter_C` building a full object path per hit, one over `BP_WorldGeneration_Base_C` for the emission count. Then spawns any planned enemies in the same frame. | estimate; the biggest suspect |
| `strange` watcher | every 5 s | game | Another two full walks, same shape, plus placing props. Each prop is a ground trace, an actor spawn and a mesh assignment. Logs show 20 props placed in one pass. | estimate; the other big suspect |
| `nag` watcher | every 500 ms | game | Returns immediately once the notice is hooked, but still costs a queue round trip twice a second for the life of the process. | estimate, small |
| `autoload` watcher | every 2 s | game | Returns immediately once the load has been attempted, same round trip as above. | estimate, small |
| `speed_default` | every 2 s until found | game | Looks for the player character. Applies once per level load, then idles. | estimate, small |
| `vendors` | every 3 s until found | game | Looks for a vendor. Applies once per level load. Currently logs `grow failed` six times because the engine allocator slot is unmeasured, so it does less than intended. | estimate, small |
| Control plane | only when called | its own | Idle unless a test or the operator asks it something. Object-walking controls run on the game thread, so asking a question DOES cost a frame. | estimate |

## What changed today, and why it might be new

The watchers used to run on their own threads. They crashed the
game when a level unloaded, because a background thread was
reading objects the engine was deleting, so on 2026-08-26 they
were moved onto the game thread (research.md 26.6).

That fixed the crash and moved every one of those walks into a
frame. Nothing was made cheaper; the cost moved from a thread
nobody was waiting on into the one that draws the picture. If the
freezing is new today, this is the first place to look.

## The obvious waste, if it turns out to be us

Nothing below has been done. It is written here so the options are
on the table when the measurement comes back.

- `spawning` and `strange` each walk the object list twice, every
  5 seconds, and both walk it for the same emission count. That is
  four walks where one would do.
- Both build a full object path string for every AI character
  found, only to read the level name out of it.
- The notice hook stays installed for the whole session and fires
  for every widget, long after the notice it was for is gone.
- A watcher that has nothing left to do (`nag` once hooked,
  `autoload` once settled) still pays a game-thread round trip on
  every tick, because the check for having nothing to do happens
  after the hop, not before it.

## How to measure it, rather than guess

Built 2026-08-26 and shared by every forge, so a mod for any game
is measured the same way.

**Switch timing on, play, read the report.**

```text
MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_timing -- --test-threads=1 --nocapture
```

`tests/research_timing.rs` switches timing on, watches for 30
seconds, prints what each named piece of work cost, and switches
it off again.

**Timing is OFF by default** and is switched on by the `timing`
control. Off, a measured scope costs one atomic load and a branch,
which is why the calls can sit in the code permanently. On, it
costs two clock reads and a brief lock per scope, which is nothing
next to reading the object list and is not nothing on a path that
runs thousands of times a frame.

What is measured, and why those places:

| Named | Where the measurement lives | Covers |
|---|---|---|
| every repeating job, by its own name | `modforge::rpg::poller::spawn_interval` | Every watcher in every game. They already have names, so nothing had to be added per watcher. |
| `ue:find_object` | `ueforge::ue::actor` | Every search for one object by class, including `find_actor`. |
| `ue:find_objects_by_chain` | same | Every search of the whole object list. |
| `ue:find_actors_by_chain` | same | The same search plus the full object path built for every hit. Nests the row above, so their times overlap; the gap between them IS the cost of building those paths. |
| `ue:objects_read` | same | How many objects a search reads. A count, not a time: it says whether a search is dear because the list is huge or because it runs often. |

**Time on the game thread is always counted**, timing switch or
not, because it is one clock read per drain. `pe_stats` reports it
as `queued_work_ms`. Read it twice a second apart and the
difference is what that second cost, against a 16.7 ms frame at
60 fps.

**Which module the threads are actually in.** The
`sample_thread_modules` control samples every thread over a window
and reports which DLL each was executing in. `main.dll` is this
mod. This is the direct answer to "is it us".

**Per-control timing.** `op_metrics`, for work the operator asked
for rather than work the mod does on its own.

Everything runs as a test in `misery-mod/tests`, never as a
one-off command, so the same numbers can be taken again after a
change and compared.
