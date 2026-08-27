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

Three ways, in the order they are worth doing.

**Our time on the game thread is already being counted.** Every
drain is timed into a running total, `time_ns`
(`ueforge/src/pe_queue.rs:303`). It is not reported anywhere.
Adding it to `pe_stats` and reading it twice, a second apart,
gives the milliseconds we held the game thread during that second.
Against a 16 ms frame that single number says whether we are the
problem.

**Which module the threads are actually in.** The
`sample_thread_modules` control samples every thread over a window
and reports which DLL each was executing in. `main.dll` is this
mod. This is the direct answer to "is it us".

**Per-control timing.** `op_metrics` already reports it, for work
the operator asked for rather than work we do on our own.

Whatever is run, it ships as a test in `misery-mod/tests`, not as
a one-off command, so the numbers can be taken again after a
change and compared.
