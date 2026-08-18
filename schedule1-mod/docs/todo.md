# schedule1-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `killcredit` | [ ] Find the vanilla retaliation trigger: what calls AttackEntity on a vanilla goon when the player punches it | Harmony postfix on SetAndAttackTarget captures the call stack from a vanilla goon punch. |
| 1 | `killcredit` | [ ] Build the retaliation hook once the trigger path is known | Custom goons fight back when attacked. |
| 1 | `farming` | [ ] Mob spawn path: prove spawning a hostile NPC at a position via vanilla machinery that fights, dies, and despawns clean | Test spawns an NPC, it aggros, dies, and despawns with no errors. |
| 3 | `loot` | [ ] Fix loot drop regression: ServerManager.Spawn type mismatch (GameObject vs NetworkObject) on custom NPC kills | Loot drops on custom NPC kills the same as on vanilla kills. |
| 5 | `farming` | [ ] Per-region mob spawner for all three factions: garrison size from influence, respawn timer, body cleanup, reload respawn | All three factions spawn garrison NPCs per region, respawn after kills, bodies cleaned up, survive reload. |
| 5 | `farming` | [ ] Mob modifier types (Diablo affix model): roll on spawn, stats from affixes, XP/loot scale with affix count, region difficulty scaling | Mobs roll affixes at spawn, harder regions roll more, XP and loot reflect affix count. |
| 5 | `killcredit` | [ ] After combat ends, test whether the goon returns to idle hold or wanders | Behavior documented; fix applied if it wanders. |
| 5 | `shim` | [ ] Fix transient NRE in NPCScheduleManager.OnMinPass on custom NPCs (no schedule data) | No NRE in MelonLoader log from schedule manager on custom NPCs. |
| 10 | `farming` | [ ] Mob stats scale with player level (never trivializes) | Higher player level produces tougher mobs. |
| 10 | `loot` | [ ] Verify unclaimed loot behavior on save/reload (drops are not saveable scene objects) | Behavior documented; fix applied if drops corrupt. |
| 10 | `loot` | [ ] Item drops beyond cash once cash drops are proven | Non-cash items drop from kills. |
| 10 | `loot` | [ ] No orphaned pickups after save/reload | Unclaimed drops do not corrupt or persist incorrectly. |
| 10 | `interop` | [ ] Fix interop generator's 4th patch site (skipped metadata init breaks static field setters: set_MaxHealth crashes) | Vitality and regeneration skills come off the ice. |
| 10 | `skills` | [ ] Hot reload recaptures live values as vanilla baselines; persist vanilla in store or re-zero effects on shutdown | Hot reload does not stack boosts on top of already-boosted values. |
| 15 | `shim` | [ ] Melee separation polish: NPC-vs-NPC brawlers path into each other while attacking | Custom goon melee fights look correct with model separation. |
| 15 | `influence` | [ ] Verify ChangeInfluence shows up in the game UI (value moves via GetInfluence but does the player see it?) | Player sees influence change on screen after ChangeInfluence call. |
| 20 | `farming` | [ ] Farming exit gate: operator farms one region for several respawn cycles with XP + loot, MelonLoader log clean | Operator confirms the farming loop works for multiple cycles. |
| 20 | `performance` | [ ] Cache CartelInfluence handle at session level (currently walks entire type every get_influence call) | One cached handle reused for all reads per session, with staleness guard. |
| 20 | `performance` | [ ] Stop reading player position every pass; move aggro detection onto NPC guard behavior | War pass no longer calls player position; NPCs handle their own detection. |
| 20 | `performance` | [ ] Cache CashPickup template handle for loot drops (currently walks all instances twice per drop) | Template cached at first use, second walk eliminated. |
| 20 | `performance` | [ ] Replace PLAYER_HITS vec with a fixed-size ring buffer (32-slot, oldest-eviction) | Ring buffer caps memory and scan time during sustained combat. |
| 25 | `performance` | [ ] Decide PASS_EVERY interval after performance fixes land | Interval documented and justified. |
| 25 | `war` | [ ] Ownership map: three factions own regions; faction_state op shows drug influence, police presence, and controller | faction_state returns per-region ownership for all three factions. |
| 25 | `war` | [ ] Player garrison deployment: spend cash to place your own goons, they hold post and fight, replacement costs money | Player can deploy and lose goons with cash consequences. |
| 30 | `war` | [ ] Cartel war chest: passive income from controlled regions, spent on reinforcements | Cartel reinforcement rate reflects its treasury. |
| 30 | `war` | [ ] Police presence track: scales with drug activity, police spawn independently, attack both factions | Police garrison scales with local drug activity. |
| 30 | `war` | [ ] NPC-vs-NPC combat: all three factions fight each other with no player involvement | Cartel, police, and player NPCs engage each other autonomously. |
| 35 | `war` | [ ] Territory pressure: losing a region costs the player (unsafe dealing territory) | Player sees consequences of losing a region. |
| 35 | `war` | [ ] Player takeover: clearing cartel forces flips ownership; holding it pays off | Player can conquer and hold a region for safe dealing. |
| 40 | `war` | [ ] Director: random event rolls (police crackdowns, cartel pushes) and adaptive pressure | Director fires events that create gameplay variety. |
| 40 | `farming` | [ ] Re-base farming forces on custom NPCs (vanilla goons stay for vanilla systems) | 10+ custom goons spawn, fight, die, pay XP/loot/influence in-game. |
| 40 | `shim` | [ ] Arm NPCs with weapons that cost cash (the war economy money sink) | Weapons cost the faction cash to equip. |
| 50 | `cosmetics` | [ ] Custom NPC appearance via S1API appearance/identity APIs (real uniforms, goon looks) | NPCs have faction-appropriate appearances. |
| 50 | `cosmetics` | [ ] Name pools per faction | NPCs have faction-appropriate names. |
| 50 | `patrol` | [ ] AddComponent FootPatrolBehaviour on a custom goon (PatrolGroup has no default constructor; find how the game creates them) | Goons walk patrol routes through held territory. |
