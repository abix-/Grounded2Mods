# misery-mod open issues

## Todo rules

- Keep one table with exactly four columns: `Priority`, `System`, `Todo`, and
  `Done when`.
- Put a number from `1` through `100` in the Priority column. Lower numbers
  come first. Use the full range so the table shows real differences between
  tasks.
- Priority measures how quickly completing the row gets the project closer to
  its next shipped state. It does not measure alarm, age, code size, test
  status, or how easy the work looks.
- Priority `1` is any concrete task with the greatest direct effect on
  shipping. Multiple open rows may have priority `1` when each has that
  effect. Priority `100` is valid work with the least direct effect.
- Reassign priorities when the goal or todo changes. Sort rows from lowest
  number to highest. The first unchecked row is next unless the operator says
  otherwise.
- Put the actual system being changed in the System column, using its name
  from the codebase (crate, module, package, file). Do not use a generic
  category or a made-up name.
- Start the Todo column with one checkbox: `[ ] Do the thing`. No preceding
  hyphen.
- The table is the ordered work plan. Before changing source code, split the
  work into the concrete rows required to finish it.
- Each row is exactly one action that can be started, completed, and proved in
  one session.
- Each row must be independently executable as written. Completing it must
  leave production source compiling without temporarily adding a second path,
  partially implementing another row, or requiring an unlisted follow-up edit.
- Write the Todo cell as a direct instruction to change one behavior. Write
  the Done when cell as the one exact result that proves that instruction is
  finished.
- Use plain terms. Do not use vague tasks, invented language, unexplained code
  names, or design essays.
- Done when names the exact observable result or proof that closes the row.
  Partial work stays unchecked.
- A newly found issue gets a new prioritized row and checkbox. Never enlarge
  the active row or create a nested or duplicate checklist.
- `Update the todo` means edit affected table rows only. Do not add status
  paragraphs, percentages, summaries, diaries, or evidence dumps.
- When a row is complete, change `[ ]` to `[x]`, record the shipped result in
  the repo changelog (`docs/changelog.md`), then delete the completed row
  from the table.

## Open

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `lib.rs` | [ ] Set up pe_queue DrainSite + ProcessEventHook for game-thread dispatch | ProcessEvent calls from Rust execute on the game thread via the drain site. |
| 1 | `lib.rs` | [ ] Find a UE class that fires ProcessEvent before main menu to use as drain site | Drain site class identified and documented. |
| 5 | `lib.rs` | [ ] Suppress nag screen (WD_PlaytestNote01_C) via game-thread ProcessEvent | Nag screen dismissed automatically on load without manual input. |
| 10 | `skills` | [ ] Find melee damage address for strength stat | Memory offset for player melee damage documented and verified with a write test. |
| 10 | `skills` | [ ] Find player max health address for constitution stat | Memory offset for player max health documented and verified with a write test. |
| 10 | `skills` | [ ] Hook or poll for kill events as XP source | Kill event fires reliably and delivers XP to the tracker. |
| 10 | `skills` | [ ] Find craft completion event as XP source | Craft event fires reliably and delivers XP to the tracker. |
| 15 | `skills` | [ ] Implement RPG system: XP, leveling, stat/skill point allocation | Skill catalog, tracker, and level-up ops registered. See `docs/rpg.md`. |
| 15 | `skills` | [ ] Implement RPG persistence (JSON save/load for level, XP, allocations) | RPG state survives save/reload cycle. |
| 20 | `vendors` | [ ] Build vendor list config and auto-apply on game load with UI tab | Vendor sell/buy list modifications apply automatically from config on each load. Item list editable from ImGui tab. |
| 50 | `shining` | [ ] Research what depends on shining regeneration before shipping permanent freeze | Dependencies documented; safe to ship or workaround identified. |
| 50 | `debug` | [ ] Research biome number to area mapping (section 19.4) | Biome numbers mapped to area names in research doc. |
