# Methodology: research-driven modding

> How we figure out how to mod a game. The discipline that
> makes both frameworks worth their weight.

## The loop

1. **Pick a question.** "What field controls the player's max
   stamina?" "When does the game fire a dig event?" "Where do
   ore drops come from?"
2. **Pose it as an op.** Either an existing generic primitive
   (`walk_class`, `inspect_address`, `read_field`,
   `invoke_method`) or a new game-specific op the mod
   registers at init.
3. **Drive it via HTTP.** `curl POST localhost:<port>/op
   -d '{"op":"...", "args":{...}}'`. Read the envelope.
   Compare to expected.
4. **Capture the finding as a test.** Write the question,
   the curl, and the expected envelope shape under the mod's
   `tests/` directory. Now it is a regression check.
5. **Move the proven capability into the shared crate.**
   modforge if it names no engine, ueforge or unityforge if it
   does. It becomes a Skill / Effect / Trigger / Hook, or a
   plain module. This step is not optional and it is the one
   that keeps getting skipped.
6. **THEN the mod uses it to do something**, once the operator
   has asked for that something.

The discipline: every research question turns into a test.
Every regression has a test ready to catch it.

## Research proves a concept. It is not the feature.

Step 4 answers "can this be done". It does not decide that it
SHOULD be done, how much of it, or how often. Those are the
operator's, and asking costs one sentence.

Skipping steps 5 and 6 is what produced four features nobody
asked for, all in 2026-08-25 and all deleted on 2026-08-26:

| Feature | What the research proved | What appeared instead |
|---|---|---|
| `harvest.rs` | a square's pieces can be read with transforms | a repeating harvest-and-rebuild pass |
| `places.rs` | pieces can be composed into a structure | monuments placed into the world on a timer |
| `rooms.rs` | the game's modular kit can build a room | a room generator with a hand-typed parts table |
| `strange.rs` | actors can be spawned at rolled positions | eleven kinds of phenomenon, up to 48 spawned actors per square, written into the player's save every five seconds |

In each case the generic half was left in the game crate rather
than moved out, so the same work had to be done again later, and
the feature itself was never wanted.

**The tell:** if you cannot quote the words the operator used to
ask for the behaviour, you are on step 6 without permission. Stop
and ask. A research test that proves a thing is a complete piece
of work on its own, and it is often the whole job.

## Why HTTP

- **External.** Tests live outside the game process. They
  survive game restart, crash, hot reload.
- **Inspectable.** Curl from anywhere. Pipe to `jq`. Diff two
  responses.
- **Language-agnostic.** Test client is a small Rust binary
  in `modforge/client/`; you could write one in Python or
  TypeScript without changing the framework side.
- **Stable.** The envelope is documented in
  `spec/op-envelope.json`. Op surface in
  `spec/generic-primitives.md`. New game = same protocol.

## Generic primitives that pay off

`walk_class` + `inspect_address` together answer "what is the
shape of this thing in memory?" without dropping into a
debugger.

`read_field` + `write_field` + `invoke_method` together
answer "can I do this thing programmatically?" before
committing to a Skill / Effect implementation.

`scan_memory` + `freeze` together answer "where is this
value stored?" when reflection doesn't help (encrypted /
obfuscated field names, dynamic field tables).

These are the four research questions every game answers in
the first day of work.

## The "always change the framework first" rule

If you wrote the same scaffolding in two game mods, it
belongs in the framework. If you wrote it in two frameworks,
it belongs in modforge. Don't speculate; extract on the
second instance.

## Snapshot is the source of truth

Every op response carries the framework's snapshot blob.
Tests assert against snapshot, not per-op result, because
the "did X work" question is really "what changed in the
world?". The snapshot makes that visible.

## Cross-references

- `composition-model.md`: Effects + Triggers + Skills.
- `def-registry.md`: the Def -> Registry -> Instance ->
  Controller pattern.
- `naming.md`: conventions both frameworks follow.
- `spec/op-envelope.json`: wire format.
- `spec/generic-primitives.md`: the op surface every
  framework ships.
