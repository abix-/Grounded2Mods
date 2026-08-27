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

### Second run, with `strange` switched off

```text
held for 2894.6 ms over 30.0 s
that is 96.48 ms per second of play        (was 126.14)

name                              calls    total ms     avg us    worst ms
ue:find_actors_by_chain              21     2603.98   123998.8      133.94
ue:find_objects_by_chain             21     2603.21   123962.4      133.92
misery-spawning                       6     1507.78   251297.0      261.23
misery-nag                           58     1326.09    22863.7      201.28
ue:find_object                       14      289.37    20669.5       24.32
misery-autoload                      15        0.01        0.4        0.00
ue:objects_read                 4827579        0.00        0.0        0.00
```

Searches fell from 37 to 21, which is exactly the sixteen
`strange` was doing.

**But the price of a search is not a constant.** It went UP, from
94 ms to 124 ms, because the object count went up: 230,000 per
search against 174,000 in the first run. The operator was in a
different part of the world with more streamed in. **A search
costs whatever the world currently holds, so it gets worse the
more there is around the player.** The two runs are therefore not
a clean comparison, and the honest summary is 126 ms per second
down to 96, not a 24% saving on a fixed cost.

With `strange` gone, `spawning` is the whole problem: six passes,
two searches each, 251 ms a pass. It does what `strange` did, ask
the entire object list which squares are loaded and what the
emission count is, on a timer.

`misery-nag` is the other oddity. Fifty-eight ticks, no work in
any of them. It is hopping to the game thread twice a second
forever, purely to discover it has nothing to do, and waiting
behind `spawning` when it gets there. Fixed below.

### Third run, with the notice watcher ending itself

```text
held for 2899.6 ms over 30.0 s
that is 96.65 ms per second of play        (was 96.48)

name                              calls    total ms     avg us    worst ms
ue:find_actors_by_chain              21     2578.25   122773.8      132.01
ue:find_objects_by_chain             21     2577.25   122726.1      132.00
misery-spawning                       6     1478.86   246476.2      257.89
ue:find_object                       15      320.31    21354.2       24.63
misery-autoload                      15        0.00        0.3        0.00
ue:objects_read                 4660369        0.00        0.0        0.00
```

`misery-nag` is gone from the report. In the log:

```text
[01:03:36] nag: hooked WD_PlaytestNote01_C
[01:03:36] nag: pressed InpActEvt_SpaceBar_K2Node_InputKeyEvent_1
[01:03:36] hook: removed WD_PlaytestNote01_C
[01:03:36] misery-nag: stopped
```

**It did not reduce the stalling, and that was predictable.**
96.48 to 96.65 is noise. The notice watcher's 1326 ms was time
spent WAITING on its own thread for a turn behind `spawning`, not
work on the game thread. What the fix removed is 58 pointless
hops per 30 seconds and a hook that fired on every widget in the
game for the whole session. Both were real and neither was the
freezing.

Worth remembering when reading this table: **a big number in a
watcher row can be waiting rather than working.** The game thread
total at the top is the only number that says what the player
feels.

`spawning` is the freezing. 1478 ms of the 2899.

### Fourth run: `spawning` asks the generator instead of the world

```text
held for 1287.7 ms over 30.0 s
that is 42.92 ms per second of play    (was 96.65, and 126.14 at the start)

name                              calls    total ms     avg us    worst ms
ue:find_actors_by_chain              10     1009.64   100963.9      104.63
ue:find_objects_by_chain             10     1009.42   100942.1      104.61
ue:find_object                       15      277.49    18499.4       20.14
misery-spawning                       6       35.82     5970.0       10.61
misery-autoload                      15        0.00        0.3        0.00
ue:objects_read                 1854528        0.00        0.0        0.00
```

**`spawning` went from 246 ms a pass to 5.97 ms. Forty-one times
cheaper**, worst pass 10.6 ms instead of 258 ms.

It no longer searches at all. Its tick reads the generator's
`StreamingLevels`, follows each entry's `LoadedLevel` at +0x158
for the square name, and compares that set against last time. A
tick with nothing new stops there. Only a square that actually
streamed in is worth a search, and then only for that square.
See worldgen.md 10 for the chain and
`ueforge::ue::streaming::LevelStreamer` for the shared half.

Six milliseconds is also the proof it is running rather than
bailing out: an empty list would cost microseconds, like
`autoload`.

**The remaining 43 ms per second is almost all one watcher, and
it is not this mod's.** Ten searches in 30 seconds is one every
three seconds, which is the `vendors` finder. It polls for a
vendor FOREVER, searching every object each time, so it can
re-apply after a return to the main menu. That is
`ueforge::ue::actor::on_each_load`, which every game in the
workspace uses.

Same disease `spawning` and the notice watcher have now been
cured of, in shared code.

How to read the first run:

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
| `strange` watcher | OFF since 2026-08-26 | game | Was three searches a pass: **291 ms average, 305 ms worst**, then spawning up to 48 actors into a square. Switched off; see below. | MEASURED |
| `spawning` watcher | every 5 s | game | Rebuilt 2026-08-26. **5.97 ms average, 10.6 ms worst**, no search at all: it reads the generator's streaming list and compares. Was 246 ms a pass. Only a square that streamed in costs a search. | MEASURED |
| `vendors` finder | every 3 s, forever | game | **The biggest cost left: 10 searches and 1009 ms per 30 seconds.** Polls the whole object list for a vendor for the life of the process, so it can re-apply after a return to the main menu. Shared code, `ue::actor::on_each_load`. | MEASURED |
| `nag` watcher | until the notice is dismissed, then never | game | Fixed 2026-08-26. Dismisses once, uninstalls its hook, ends itself. Gone from the report entirely. Used to tick twice a second for the life of the process and leave a hook firing on every widget. | MEASURED |
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

## `strange` was switched off, 2026-08-26

The operator asked what it was doing, and it was doing this: three
searches every five seconds, then rolling from ELEVEN kinds of
phenomenon, up to four per square, spawning 2 to 12 actors for
each one. Up to 48 spawned actors in a single square, capped at
400 a session, written into the player's own save. It had placed
32 props in the four minutes before it was switched off.

It cost 291 ms a pass on the game thread. Switched off at
`lib.rs`, not deleted.

## The waste, now that it is measured

Nothing below has been done.

- `spawning` searches the whole object list twice every five
  seconds, at 124 ms each, to ask which squares are loaded and
  what the emission count is. Neither answer changes unless a
  square streams in.
- It runs on a fixed timer, so most passes re-answer a question
  whose answer has not changed.
- `ue::actor::on_each_load` searches the whole object list on a
  timer FOREVER, so that a feature can re-apply after a return to
  the main menu. It should apply once and then stop, and be woken
  by the world going away rather than polling for it. This is the
  pattern the notice watcher and `spawning` have both now been
  fixed to follow, and it is shared by every game.
- **The price of a search grows with the world.** Any fix that
  keeps searching on a timer will get worse the more of the map is
  loaded around the player.
- `autoload` still ticks every 2 seconds after it has settled, at
  0.3 microseconds a tick. Correct now that the check happens
  before the hop, and not worth touching.

## The notice watcher was fixed, 2026-08-26

It ticked twice a second for the life of the process to discover
it had nothing to do, and it left a hook installed on a vtable
every widget in the game shares. Now it dismisses the notice,
uninstalls its own hook, and ends itself, all within the same
second.

Three pieces, two of them shared with every game:

- `modforge::rpg::poller::PollerHandle::stop_soon` ends a
  repeating job without waiting for it. `stop` joins the thread,
  so a job could not end itself from inside its own tick without
  joining itself, which never returns. The loop also checks the
  flag straight after each tick now, so it goes immediately rather
  than sleeping out one more interval.
- `ueforge::hook::remove(class_name)` takes one hook back out and
  uninstalls it. The careful drop already existed, restoring the
  engine's original slot and waiting for calls already inside our
  code to leave; there was no way to reach it for a single hook.
  Never call it from inside that hook's own handler: the wait
  would be waiting for the caller.
- `ueforge::hook::install_for_live_object_until` puts them
  together, and moves the "already installed" check off the game
  thread, where it belonged.

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
