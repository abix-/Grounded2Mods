# What the mod does every frame, and what it costs

Written 2026-08-26, after the game felt like it was freezing.

Read this before blaming the mod, and before defending it.

**Measured 2026-08-26, 30 seconds of play in the operator's own
save. The mod is the freezing.** It held the game thread for
**126 ms of every second**, in lumps as large as 305 ms. A frame
at 60 fps is 16.7 ms, so the worst single pass cost about
eighteen frames in a row.

Two things decide the cost of anything in this list:

- **Which thread it runs on.** Work on the game thread happens
  inside a frame. At 60 frames a second a frame is 16.7 ms, so
  16.7 ms of our work is one frame lost. Work on our own threads
  costs the player nothing directly.
- **Whether it reads the object list.** One search reads every
  UObject the game has loaded. Measured: **about 174,000 objects
  per search, and 94 ms per search.** Six frames, every time,
  every search.

## The measurement

```text
=== game thread ===
held for 3784.2 ms over 30.0 s
that is 126.14 ms per second of play

name                              calls    total ms     avg us    worst ms
ue:find_actors_by_chain              37     3476.85    93969.0      100.60
ue:find_objects_by_chain             37     3475.95    93944.6      100.59
misery-nag                           57     1460.54    25623.5      260.09
misery-strange                        5     1456.68   291335.4      304.75
misery-spawning                       6     1138.99   189831.0      200.44
ue:find_object                       15      305.94    20395.8       21.80
misery-autoload                      15        0.01        0.4        0.00
ue:objects_read                 6443370        0.00        0.0        0.00
```

How to read it:

- 6,443,370 objects read across 37 searches is **174,000 objects
  per search**. That single number explains everything else.
- `strange` does three searches a pass, hence 291 ms. `spawning`
  does two, hence 190 ms. Both run every 5 seconds, both on the
  game thread, so those averages are frames the game did not draw.
- `misery-nag` is NOT doing work. It returns immediately once the
  notice is hooked. Its 25 ms average is time spent WAITING for
  its turn on the game thread, behind the big searches. Its worst,
  260 ms, is one tick stuck behind a `strange` pass.
- The watcher rows are measured on their own poller threads and
  include that waiting, so they do not add up with the game-thread
  total. Do not sum this table.

**One earlier guess in this doc was wrong.** Building a full
object path for every hit was flagged here as waste. Measured, it
is the gap between the two search rows: 3476.85 minus 3475.95,
which is **0.9 ms out of 3476**. Irrelevant. The cost is reading
174,000 objects, not what is done with the hits. Estimates read
off code are worth what they cost.

## Everything that runs

| What | How often | Thread | Cost | Measured? |
|---|---|---|---|---|
| One search of the object list | see below | game | **94 ms**, reading about **174,000 objects**. Every other cost in this table is a multiple of this one. | MEASURED |
| `strange` watcher | every 5 s | game | Three searches a pass: **291 ms average, 305 ms worst**. Then places props, each a ground trace, an actor spawn and a mesh assignment. | MEASURED |
| `spawning` watcher | every 5 s | game | Two searches a pass: **190 ms average, 200 ms worst**. Then spawns any planned enemies in the same frame. | MEASURED |
| `nag` watcher | every 500 ms | game | Does no work once the notice is hooked. Its **25 ms average** is time queued behind the searches above; **260 ms worst** is one tick stuck behind a `strange` pass. | MEASURED |
| `find_object`, one object by class | as the finders run | game | **20 ms average**, cheaper than a full search because it stops at the first match. Used by `speed_default`, `vendors` and `nag`. | MEASURED |
| `autoload` watcher | every 2 s | game | **0.4 microseconds a tick.** Returns immediately once the load has been attempted. Nothing to fix. | MEASURED |
| Run the queued jobs from `UEngine::Tick` | every frame | game | Returns immediately when nothing is waiting. Otherwise the job's own cost, inside that frame. Reported as `queued_work_ms`. | MEASURED, as the 126 ms per second total |
| UE4SS `on_update` | every frame | UE4SS's own | A counter, plus a retry of the tick-hook install until it succeeds. | estimate, near zero |
| The playtest notice hook | every widget function call, all session | game | Reads the object's class name and compares one string, then forwards. Blueprint widget classes share one vtable, so this fires for EVERY UserWidget, not just the notice. Never removed after the notice is gone. | NOT measured. It has no named scope, so it does not appear in the report at all |
| `speed_default` | every 2 s until found | game | One `find_object` per tick until the player exists, then idles. | MEASURED, inside the `find_object` row |
| `vendors` | every 3 s until found | game | One `find_object` per tick until a vendor exists. Applies once per level load. Logs `grow failed` six times because the engine allocator slot is unmeasured, so it does less than intended. | MEASURED, inside the `find_object` row |
| Control plane | only when called | its own | Idle unless asked something. The searching controls run on the game thread, so asking a question DOES cost a frame. | estimate |

## What changed today, and why it might be new

The watchers used to run on their own threads. They crashed the
game when a level unloaded, because a background thread was
reading objects the engine was deleting, so on 2026-08-26 they
were moved onto the game thread (research.md 26.6).

That fixed the crash and moved every one of those searches into a
frame. Nothing was made cheaper; the cost moved from a thread
nobody was waiting on into the one that draws the picture. The
freezing is new today, and this is why.

## The waste, now that it is measured

Nothing below has been done.

- Five searches every five seconds, `strange` three and `spawning`
  two, where the useful answer changes only when a level streams
  in. At 94 ms each that is 470 ms of held frames per five
  seconds.
- Both search separately for the same emission count.
- Both run on a fixed timer rather than when a square actually
  loads, so most passes re-answer a question whose answer has not
  changed.
- The notice hook is never removed once the notice is gone, and it
  is not measured at all.
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
