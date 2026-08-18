# traveler open issues

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
  the project's changelog, then delete the completed row from the table.

## Open

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `modforge` | [ ] Add `TravelerProfile` struct with six stats (str/dex/con/int/wis/cha, each u32 0-9999), level (u32), xp (u64), nemesis record, and game history vec. Add `TravelerStore` that reads/writes a local JSON file with atomic temp-file-plus-rename save | `modforge::traveler::TravelerStore` loads and saves a profile, and a round-trip test passes |
| 2 | `modforge` | [ ] Add traveler API: `load_profile()`, `save_profile()`, `award_stat(stat, amount)` with 9999 cap, `record_xp(amount)` feeding a cross-game Curve, `add_history_entry(game, hours, outcome)` | Each API function works and a test exercises load, mutate, save, reload |
| 10 | first game mod | [ ] Pick the most developed mod and wire it to read the traveler profile on startup. Feed the profile's level and stats into that game's storyteller difficulty, quality tier odds, and spawn rate parameters | The game mod's behavior visibly changes based on the traveler's level and stats |
| 15 | first game mod | [ ] Wire the game mod to write stat growth back to the traveler profile on save/exit. Map in-game actions to stat awards (kills to strength, crafting to intelligence, etc.) | Playing the game raises traveler stats, verified by reading the profile after exit |
| 25 | `modforge` | [ ] Add nemesis fields to the traveler profile: wins, losses, escalation float, tactics array. Add `record_nemesis_encounter(won: bool)` that ratchets escalation | Nemesis record persists and escalation grows across multiple encounters |
| 30 | first game mod | [ ] Wire the game mod to spawn a nemesis entity from the traveler's nemesis record. Escalation controls difficulty (enemy count, gear, aggression). Each encounter writes the result back | A nemesis spawns, fights, and the traveler's nemesis record updates on win or loss |
| 50 | `modforge` | [ ] Add travel protocol structs: `TravelRequest` (origin game, purpose, entry parameters, return address) and `ReturnPayload` (loot, stats gained, outcome). Read/write as part of the traveler profile file | Travel request round-trips through the profile file |
| 55 | two game mods | [ ] Wire one game mod to write a travel request and launch a second game. Wire the second game to read the travel request on startup and adjust its session accordingly | Launching from game A starts game B with modified parameters based on the travel request |
| 60 | two game mods | [ ] Wire the destination game to write a return payload on exit. Wire the origin game to read the return payload on its next load and incorporate the result | Completing an expedition in game B gives loot/stats that appear when game A resumes |
