# misery-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `spawning` | [ ] NPC spawn multiplier: on each map square load, spawn extra copies of its placed hostile NPCs via game-thread spawn calls | With multiplier 2, a square that ships 3 bandits shows 6, verified live. Single knob constant. |
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
| 50 | `debug` | [ ] Research biome number to area mapping (section 19.4) | Biome numbers mapped to area names in research doc. |
