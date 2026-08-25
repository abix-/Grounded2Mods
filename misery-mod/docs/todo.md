# misery-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `worldgen` | [ ] Forced regeneration: call GenerateCustomBiom / GenerateBiom through the call op and verify a new world generates on demand | A test regenerates the expedition without a shining; the biome number to area mapping is recorded in worldgen.md. |
| 1 | `worldgen` | [ ] Pool swap experiment: write one Meadows square's FNames into Paneli's pool, regenerate, verify the foreign square streams in | A Meadows square appears in a Paneli world, walkable with its NPCs, verified live. |
| 2 | `worldgen` | [ ] Mixed-pool area: fill one generator's pool with squares from multiple same-size areas and generate | A world generates whose squares come from at least two areas; recorded in worldgen.md. |
| 3 | `worldgen` | [ ] Tile-size mismatch probe: put a 12000 square into the 16500 Factory grid and observe | Gap/overlap behavior documented in worldgen.md; verdict on cross-size mixing. |
| 3 | `worldgen` | [ ] New-area research: can a spawned fifth generator (or a spare grid region) run RunGenerationFromSeed with a custom grid and pool | Go/no-go with findings documented in worldgen.md. |
| 5 | `worldgen` | [ ] New-square research: how preset levels are packaged (pak, cooked umap), whether a cloned and renamed square can be loaded | Go/no-go with findings documented in worldgen.md. |
| 8 | `worldgen` | [ ] Build one new square end to end and roll it into a pool | A square that never existed streams in-game, verified live. |
| 2 | `spawning` | [ ] Record whether the hub spawn point re-reads count/class after the set_spawn_point writes | Observation from the tamed dwarf spot documented in research.md 25.4. |
| 3 | `vendors` | [ ] Build vendor list config and auto-apply on game load with UI tab | Vendor sell/buy list modifications apply automatically from config on each load. Item list editable from ImGui tab, including SELL_PRICE_PCT and SEWING_KIT_COST. |
| 5 | `lib.rs` | [ ] Suppress nag screen (WD_PlaytestNote01_C) via game-thread ProcessEvent | Nag screen dismissed automatically on load without manual input. |
| 10 | `skills` | [ ] Find melee damage address for strength stat | Memory offset for player melee damage documented and verified with a write test. |
| 10 | `skills` | [ ] Find player max health address for constitution stat | Memory offset for player max health documented and verified with a write test. |
| 10 | `skills` | [ ] Hook or poll for kill events as XP source | Kill event fires reliably and delivers XP to the tracker. |
| 10 | `skills` | [ ] Find craft completion event as XP source | Craft event fires reliably and delivers XP to the tracker. |
| 15 | `skills` | [ ] Implement RPG system: XP, leveling, stat/skill point allocation | Skill catalog, tracker, and level-up ops registered. See `docs/rpg.md`. |
| 15 | `skills` | [ ] Implement RPG persistence (JSON save/load for level, XP, allocations) | RPG state survives save/reload cycle. |
| 50 | `shining` | [ ] Research what depends on shining regeneration before shipping permanent freeze | Dependencies documented; safe to ship or workaround identified. |
