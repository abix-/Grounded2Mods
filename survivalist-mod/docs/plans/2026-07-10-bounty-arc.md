# Bounty arc implementation plan

> Execute inline in this session, task by task, with the operator
> watching. No subagents (operator rule). Steps use checkbox
> (`- [ ]`) syntax for tracking.

**Goal:** First slice of the work pillar (docs/status.md "More to
do"): a camp at war posts a bounty on its enemy's leader; if the
player's people make the kill while the offer stands, the hirer
loads real payment from its stores onto a courier who walks it to
the player's gate.

**Architecture:** One new module `bounty.rs` modeled on the two
proven act shapes: `murder.rs` (single dramatic mission slot,
staged mission, handle discipline) and `trade.rs` (courier squad,
real Take/Add goods transfer, arrive-by-distance). No new Harmony
hooks: kill attribution rides war.rs's existing
Community.OnMemberDied prefix. The offer reaches the player as a
chronicle line; there is no acceptance step (open contract).

**Tech stack:** Rust cdylib over the unityforge bridge; deployed
by `survivalist-mod/scripts/build_and_deploy.ps1 -Hot`; verified
through control-plane ops (port 17173), Player.log, and the
in-game chronicle.

## Global constraints

- ASCII only. No em-dashes anywhere, including log strings: the
  new module logs as `survivalist-mod: bounty: ...` (colon form).
- ONE bounty map-wide at a time (single slot, like murder).
- No new hooks. The only hook touch is one call added inside
  war.rs's existing on_member_died.
- Payment is real goods from the hirer's real stores via
  common::carry_off_stored_goods (no conjuring; no-cheating
  pillar). If the stores ran down, the player gets less (honest).
- The offer reaches the player only in-world (chronicle). No UI.
- The player camp is never the mark; a war whose enemy is the
  player produces no bounty (slice 1).
- bounty_status and bounty_post ship as permanent diagnostic ops
  (repo rule: every live probe is a permanent op).
- Every commit is concise lowercase and pushed immediately; only
  the files this plan names are ever committed (the tree carries
  other agents' WIP).

## Design decisions (locked)

- Open contract: no acceptance step. The chronicle line announces
  the offer; the kill claims it. This matches the status row's
  "offers arrive in-world (a messenger at the gate, a chronicle
  line)".
- THE WORK BOARD (operator-locked 2026-07-10): the player must be
  able to SEE, in-game, a list of everything they could do and
  what each pays. The chronicle announces; the board lists. Which
  game surface carries the list is a research question (Task 4);
  the offer therefore carries a concrete reward from the moment
  it is posted (`pays`, counted from the hirer's real stores at
  offer time, capped at BOUNTY_PAY_STACKS) so any surface can
  print "X offers a bounty on Y: pays N stacks of goods".
- The mark is the enemy LEADER (decapitation; the same mark
  murder.rs uses, read from `InvasionTarget` then `Leader`).
- The hirer: an AI settlement (Normal or Looter) at war with
  another AI settlement, not Hostile to the player, able to pay
  (at least one stored non-food stack), with at least 2 living
  members. Among candidates, fewest living members wins: the camp
  losing its war hires the help.
- Payment: up to 3 non-food stacks (BOUNTY_PAY_STACKS), loaded at
  courier launch, delivered into the player's first storage
  building by the same Take/Add calls trade uses.
- The offer dies when: the war ends (InvasionTarget cleared), the
  mark dies by other hands, the hirer dies, or 2700 real seconds
  pass. A hirer that cannot pay when the debt comes due is shamed
  in the chronicle and the bounty ends (no escrow in slice 1).
- Kill attribution: dead character's Id equals the mark's Id AND
  the killer's Community has CommunityType "Player". Character
  Ids are compared, never handles (handles from separate bridge
  calls never match; trade.rs:661 documents this).
- A hot reload forgets an open offer (Rust-side state, like every
  act's mission list). A courier squad orphaned by a reload is
  reclaimed by the existing common::sweep_orphan_trade_squads.

## File structure

- Create: `survivalist-mod/src/bounty.rs` (the whole arc: scan,
  offer, expiry, kill attribution, courier, two ops).
- Modify: `survivalist-mod/src/lib.rs` (mod decl, tick call,
  register_ops call).
- Modify: `survivalist-mod/src/war.rs` (one line in
  on_member_died).

Verification is compile + deploy + live ops, matching the repo's
live-verification discipline (docs/status.md "Scoring
discipline"). There is no offline test harness for bridge code;
do not invent one.

Reference patterns an implementer should read first:
- Mission slot + stages + cleanup: `src/murder.rs:50-114,343-372`
- Member eligibility loop: `src/murder.rs:291-339`
- Squad launch (AddSquad/AddToSquad/GoalTile/SetSquadAction):
  `src/murder.rs:247-261`
- Arrive-by-distance + delivery: `src/trade.rs:551-607,736-808`
- Stored-goods loading: `src/common.rs:219-295`
- Op registration + on_main_thread: `src/war.rs:55-76,159-213`

---

### Task 1: the offer (scan, expiry, bounty_status, bounty_post)

**Files:**
- Create: `survivalist-mod/src/bounty.rs`
- Modify: `survivalist-mod/src/lib.rs`

**Interfaces:**
- Produces: `bounty::tick(now: f32)`, `bounty::register_ops()`,
  and (for Task 2) the `BOUNTY: Mutex<Option<Bounty>>` slot with
  variant `Bounty::Offered { mark_id, ... }`.

- [ ] **Step 1: write bounty.rs**

```rust
//! Bounty: the first slice of the work pillar (docs/status.md
//! "More to do (ecosystem-generated work)").
//!
//! A camp AT WAR posts a public bounty on its enemy's LEADER: a
//! chronicle line, no acceptance step. If the player's people
//! make the kill while the offer stands, the hirer loads real
//! payment from its stores onto a courier who walks it to the
//! player's gate. War over, mark dead by other hands, hirer
//! dead, or the window closing all void the offer.
//!
//! Kill attribution rides war.rs's OnMemberDied prefix (it calls
//! bounty::on_death); no hooks of its own.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, ctype, display_name,
    for_each_community, handle_of, on_main_thread, own, with,
};

/// Seconds between offer scans; work is a slow drumbeat, offset
/// from murder (240) and trade (150) so the acts interleave.
const BOUNTY_SCAN_PERIOD_SECS: f32 = 300.0;

/// Seconds between advance passes.
const MISSION_TICK_SECS: f32 = 5.0;

/// Real seconds an offer stands before it lapses.
const OFFER_WINDOW_SECS: f32 = 2700.0;

/// Non-food stacks the payment courier carries.
const BOUNTY_PAY_STACKS: i64 = 3;

/// A courier that has not resolved by then is recalled.
const COURIER_TIMEOUT_SECS: f32 = 1800.0;

/// Within this squared tile distance of a building the courier
/// has arrived; same bar trade uses.
const ARRIVE_DIST_SQ: f64 = 25.0;

/// The one bounty in flight map-wide. Each variant owns the
/// handles it names; transitions drop what they shed.
enum Bounty {
    Offered {
        hirer_h: i32,
        hirer_name: String,
        mark_h: i32,
        mark_id: i64,
        mark_name: String,
        enemy_name: String,
        /// Non-food stacks the hirer could pay at offer time; what
        /// the board and the chronicle advertise.
        pays: i64,
        expires: f32,
    },
}

static BOUNTY: Mutex<Option<Bounty>> = Mutex::new(None);
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);
/// Game clock as of the last tick, for ops that need "now".
static LAST_NOW_BITS: AtomicU32 = AtomicU32::new(0);

pub fn tick(now: f32) {
    LAST_NOW_BITS.store(now.to_bits(), Ordering::Relaxed);
    let last_tick = f32::from_bits(LAST_TICK_BITS.load(Ordering::Relaxed));
    if now - last_tick >= MISSION_TICK_SECS {
        LAST_TICK_BITS.store(now.to_bits(), Ordering::Relaxed);
        advance(now);
    }
    let last_scan = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last_scan >= BOUNTY_SCAN_PERIOD_SECS {
        LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
        if BOUNTY.lock().is_some() {
            return;
        }
        if let Err(e) = offer_scan(now) {
            if !e.contains("not found") {
                mono::log(LogLevel::Warn, &format!("survivalist-mod: bounty scan failed: {e}"));
            }
        }
    }
}

// ---- the offer ---------------------------------------------------------------

fn offer_scan(now: f32) -> Result<(), String> {
    // The player's camp: bounties exist to be claimed by them.
    let mut player_h: Option<i32> = None;
    for_each_community(|com| {
        if ctype(&com) == "Player" {
            player_h = Some(com.handle().0);
            std::mem::forget(com);
            return Ok(false);
        }
        Ok(true)
    })?;
    let Some(player_h) = player_h else { return Ok(()) };

    // The hirer: at war with another AI camp, friendly enough to
    // the player, able to pay; the smallest camp first (the side
    // losing its war hires the help).
    let mut pick: Option<(i32, String, i64, i32)> = None; // hirer_h, name, members, enemy_h
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members < 2 {
            return Ok(true);
        }
        let Some(enemy_h) = handle_of(&com.read_field("InvasionTarget")?) else {
            return Ok(true);
        };
        let enemy_ok = with(enemy_h, |e| {
            let et = ctype(e);
            (et == "Normal" || et == "Looter")
                && e.invoke("IsAISettlement", &json!([]))
                    .map(|v| v == json!(true))
                    .unwrap_or(false)
        });
        let friendly = com
            .invoke("GetRelationship", &json!([{ "handle": player_h }]))
            .map(|r| r != json!("Hostile"))
            .unwrap_or(false);
        if !enemy_ok || !friendly || count_stored_goods(&com, GoodsFilter::NonFood, 1) == 0 {
            drop(own(enemy_h));
            return Ok(true);
        }
        if pick.as_ref().map(|p| members < p.2).unwrap_or(true) {
            if let Some((old_h, _, _, old_e)) =
                pick.replace((com.handle().0, display_name(&com), members, enemy_h))
            {
                drop(own(old_h));
                drop(own(old_e));
            }
            std::mem::forget(com);
        } else {
            drop(own(enemy_h));
        }
        Ok(true)
    })?;
    drop(own(player_h));
    let Some((hirer_h, hirer_name, _, enemy_h)) = pick else {
        return Ok(());
    };
    post_offer(hirer_h, hirer_name, enemy_h, now)
}

/// Turn a hirer + enemy pair into a standing offer on the enemy's
/// leader. Consumes both handles (keeps hirer + mark, drops the
/// enemy community).
fn post_offer(hirer_h: i32, hirer_name: String, enemy_h: i32, now: f32) -> Result<(), String> {
    let enemy = own(enemy_h);
    let enemy_name = display_name(&enemy);
    let mark = handle_of(&enemy.read_field("Leader")?).and_then(|h| {
        let alive = with(h, |v| {
            v.invoke("get_AliveAndNotZombie", &json!([]))
                .map(|x| x == json!(true))
                .unwrap_or(false)
        });
        if alive { Some(h) } else { drop(own(h)); None }
    });
    drop(enemy);
    let Some(mark_h) = mark else {
        drop(own(hirer_h));
        return Ok(());
    };
    let (mark_id, mark_name) = with(mark_h, |v| {
        (
            v.read_field("Id").ok().and_then(|x| x.as_i64()).unwrap_or(-1),
            v.invoke("GetDisplayNameString", &json!([]))
                .ok()
                .and_then(|x| x.as_str().map(str::to_string))
                .unwrap_or_else(|| "<their leader>".into()),
        )
    });

    // What the offer pays, counted from the real stores NOW so
    // the board can advertise it. A broke hirer posts nothing.
    let pays = with(hirer_h, |com| {
        count_stored_goods(com, GoodsFilter::NonFood, BOUNTY_PAY_STACKS)
    });
    if pays == 0 {
        drop(own(hirer_h));
        drop(own(mark_h));
        return Ok(());
    }

    crate::chronicle::post(&format!(
        "{hirer_name} offers a bounty on {mark_name}, leader of {enemy_name}: pays {pays} stack(s) of goods"
    ));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: bounty: {hirer_name} posts a bounty on {mark_name}, leader of {enemy_name}, paying {pays} stack(s) (window {OFFER_WINDOW_SECS}s)"
        ),
    );
    *BOUNTY.lock() = Some(Bounty::Offered {
        hirer_h,
        hirer_name,
        mark_h,
        mark_id,
        mark_name,
        enemy_name,
        pays,
        expires: now + OFFER_WINDOW_SECS,
    });
    Ok(())
}

/// Count the community's stored stacks matching the filter, up to
/// `cap` (early exit; cap 1 is a cheap "has any" test, cap
/// BOUNTY_PAY_STACKS is the advertised reward).
fn count_stored_goods(com: &MonoObject, filter: GoodsFilter, cap: i64) -> i64 {
    let Some(b_h) = com.read_field("Buildings").ok().as_ref().and_then(handle_of) else {
        return 0;
    };
    let mut found = 0i64;
    let blist = own(b_h);
    let nb = blist
        .invoke("get_Count", &json!([]))
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    for bi in 0..nb {
        let Some(bh) = blist
            .invoke("get_Item", &json!([bi]))
            .ok()
            .as_ref()
            .and_then(handle_of)
        else {
            continue;
        };
        let building = own(bh);
        let Some(inv_h) = building.read_field("Inventory").ok().as_ref().and_then(handle_of)
        else {
            continue;
        };
        let inv = own(inv_h);
        let n = inv
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        for i in 0..n {
            let Some(item_h) = inv
                .invoke("GetItem", &json!([i]))
                .ok()
                .as_ref()
                .and_then(handle_of)
            else {
                continue;
            };
            let item = own(item_h);
            if matches_filter(&item, filter) {
                found += 1;
                if found >= cap {
                    return found;
                }
            }
        }
    }
    found
}

/// GoodsFilter::matches is private to common.rs; the same food
/// test, restated (GetNutrition > 0 is food, per common.rs:181).
fn matches_filter(item: &MonoObject, filter: GoodsFilter) -> bool {
    let n = item
        .invoke("GetNutrition", &json!([]))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    match filter {
        GoodsFilter::Any => true,
        GoodsFilter::Food => n > 0.0,
        GoodsFilter::NonFood => n <= 0.0,
    }
}

// ---- advancing ---------------------------------------------------------------

fn advance(now: f32) {
    let mut slot = BOUNTY.lock();
    let done = match slot.as_mut() {
        None => return,
        Some(Bounty::Offered { hirer_h, hirer_name, mark_h, mark_name, expires, .. }) => {
            advance_offered(*hirer_h, hirer_name, *mark_h, mark_name, *expires, now)
        }
    };
    if done {
        if let Some(Bounty::Offered { hirer_h, mark_h, .. }) = slot.take() {
            drop(own(hirer_h));
            drop(own(mark_h));
        }
    }
}

/// True = the offer is void; clean up.
fn advance_offered(
    hirer_h: i32,
    hirer_name: &str,
    mark_h: i32,
    mark_name: &str,
    expires: f32,
    now: f32,
) -> bool {
    if now >= expires {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: the offer on {mark_name} lapses unclaimed"),
        );
        return true;
    }
    // The hirer must still stand and still be at war.
    let hirer_standing = with(hirer_h, |c| {
        c.invoke("HasAnyLivingNonZombieMembers", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false)
    });
    let at_war = with(hirer_h, |c| {
        c.read_field("InvasionTarget")
            .ok()
            .as_ref()
            .and_then(handle_of)
            .map(|h| {
                drop(own(h));
                true
            })
            .unwrap_or(false)
    });
    if !hirer_standing || !at_war {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: {hirer_name}'s war is over; the offer on {mark_name} is void"
            ),
        );
        return true;
    }
    // Belt over the hook's braces: a mark found dead here (the
    // death fired before Task 2 wires attribution, or by a path
    // with no Killer) voids the offer.
    let mark_alive = with(mark_h, |v| {
        v.invoke("get_AliveAndNotZombie", &json!([]))
            .map(|x| x == json!(true))
            .unwrap_or(false)
    });
    if !mark_alive {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: {mark_name} died by other hands; the offer lapses"),
        );
        return true;
    }
    false
}

// ---- ops ---------------------------------------------------------------------

pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "bounty_status",
            "The one open bounty (offered/owed/paying) or null. The work-pillar observability surface.",
            "{}",
            bounty_status,
        ),
        OpDef::new(
            "bounty_post",
            "Force an offer from a named camp that is at war (reads its InvasionTarget's leader). Live-verification probe, like war_ignite.",
            "{hirer: str}",
            bounty_post,
        ),
    ]);
}

fn bounty_status(_args: &Json) -> Result<Json, String> {
    let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
    let slot = BOUNTY.lock();
    Ok(match slot.as_ref() {
        None => json!({ "bounty": null }),
        Some(Bounty::Offered { hirer_name, mark_name, enemy_name, pays, expires, .. }) => json!({
            "bounty": {
                "stage": "offered",
                "hirer": hirer_name,
                "mark": mark_name,
                "of": enemy_name,
                "pays": pays,
                "expires_in_secs": (expires - now).max(0.0),
            }
        }),
    })
}

fn bounty_post(args: &Json) -> Result<Json, String> {
    let hirer = args
        .get("hirer")
        .and_then(Json::as_str)
        .ok_or("missing arg 'hirer' (community display name)")?
        .to_string();
    on_main_thread(move || {
        if BOUNTY.lock().is_some() {
            return Err("a bounty is already open (bounty_status)".into());
        }
        let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
        let mut found: Option<(i32, String, i32)> = None;
        for_each_community(|com| {
            if display_name(&com).eq_ignore_ascii_case(&hirer) {
                let enemy_h = handle_of(&com.read_field("InvasionTarget")?);
                let name = display_name(&com);
                let h = com.handle().0;
                match enemy_h {
                    Some(e) => {
                        found = Some((h, name, e));
                        std::mem::forget(com);
                    }
                    None => return Err(format!("'{name}' is not at war (no InvasionTarget)")),
                }
                return Ok(false);
            }
            Ok(true)
        })?;
        let Some((hirer_h, hirer_name, enemy_h)) = found else {
            return Err(format!("hirer community '{hirer}' not found"));
        };
        post_offer(hirer_h, hirer_name.clone(), enemy_h, now)?;
        match &*BOUNTY.lock() {
            Some(Bounty::Offered { mark_name, enemy_name, .. }) => Ok(json!({
                "posted": true, "hirer": hirer_name, "mark": mark_name, "of": enemy_name,
            })),
            _ => Err(format!("'{hirer_name}' has no living enemy leader to mark")),
        }
    })
}
```

- [ ] **Step 2: wire lib.rs**

Three edits:
1. Module list (lib.rs:22, alphabetical): add `mod bounty;`
   before `mod chronicle;`.
2. `on_tick` (lib.rs:64, after `murder::tick(now);`): add
   `bounty::tick(now);`.
3. `on_init` (lib.rs:90, after the war install block): add

```rust
    // The work pillar, first slice: bounties on enemy leaders,
    // paid by real couriers. bounty_status + bounty_post ops.
    bounty::register_ops();
```

- [ ] **Step 3: build + deploy hot**

Run from the repo root:
`powershell -ExecutionPolicy Bypass -File survivalist-mod/scripts/build_and_deploy.ps1 -Hot`
Expected: build green, deploy lands, Player.log gains a fresh
`survivalist-mod: ready (ops + selectors installed)`.

- [ ] **Step 4: verify the ops answer**

`curl http://127.0.0.1:17173/op -d '{"op":"bounty_status","args":{}}'`
Expected: `{"bounty":null}`.

`curl http://127.0.0.1:17173/op -d '{"op":"bounty_post","args":{"hirer":"<a camp not at war>"}}'`
Expected: the not-at-war error, verbatim shape from the code.

- [ ] **Step 5: verify a staged offer**

Pick two AI camps from war_status, then:
`curl ... '{"op":"war_ignite","args":{"attacker":"<A>","defender":"<B>"}}'`
`curl ... '{"op":"bounty_post","args":{"hirer":"<A>"}}'`
Expected: `posted: true` with B's leader as mark; the in-game
status bar shows `Word spreads: <A> offers a bounty on ...`;
bounty_status shows stage "offered" with a shrinking
expires_in_secs. Then `war_end` the pair and confirm the offer
voids on the next advance pass (log line: "war is over").

- [ ] **Step 6: commit**

`git commit survivalist-mod/src/bounty.rs survivalist-mod/src/lib.rs -m "survivalist: bounty offers (work pillar slice 1): a camp at war posts a public bounty on its enemy's leader; bounty_status + bounty_post ops" && git push`

---

### Task 2: kill attribution (the player claims the bounty)

**Files:**
- Modify: `survivalist-mod/src/bounty.rs`
- Modify: `survivalist-mod/src/war.rs:88-97`

**Interfaces:**
- Consumes: `Bounty::Offered { mark_id, ... }` from Task 1.
- Produces: `bounty::on_death(member: &MonoObject)` called from
  war.rs; new variant `Bounty::Owed { hirer_h, hirer_name,
  mark_name }` consumed by Task 3.

- [ ] **Step 1: add the Owed variant**

In the `Bounty` enum:

```rust
    /// The kill is confirmed; the next advance pass launches the
    /// payment courier (never inside the death callback).
    /// waiting_logged: the no-free-courier wait logs once, not
    /// every 5s pass.
    Owed {
        hirer_h: i32,
        hirer_name: String,
        mark_name: String,
        waiting_logged: bool,
    },
```

- [ ] **Step 2: add on_death**

```rust
/// Called from war.rs's OnMemberDied prefix for every death.
/// Cheap gate first: no open offer means no bridge calls.
pub fn on_death(member: &MonoObject) {
    {
        let slot = BOUNTY.lock();
        if !matches!(slot.as_ref(), Some(Bounty::Offered { .. })) {
            return;
        }
    }
    let Some(dead_id) = member.read_field("Id").ok().and_then(|v| v.as_i64()) else {
        return;
    };
    let mut slot = BOUNTY.lock();
    let Some(Bounty::Offered { mark_id, .. }) = slot.as_ref() else {
        return;
    };
    if *mark_id != dead_id {
        return;
    }
    // The mark is down. By whose hand?
    let by_player = (|| {
        let kh = handle_of(&member.read_field("Killer").ok()?)?;
        let killer = own(kh);
        let ch = handle_of(&killer.read_field("Community").ok()?)?;
        Some(ctype(&own(ch)) == "Player")
    })()
    .unwrap_or(false);
    let Some(Bounty::Offered { hirer_h, hirer_name, mark_h, mark_name, enemy_name, .. }) =
        slot.take()
    else {
        return;
    };
    drop(own(mark_h));
    if by_player {
        crate::chronicle::post(&format!(
            "the bounty on {mark_name} is claimed; {hirer_name} owes a debt"
        ));
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: {mark_name} of {enemy_name} fell to the player; {hirer_name} owes payment"
            ),
        );
        *slot = Some(Bounty::Owed { hirer_h, hirer_name, mark_name, waiting_logged: false });
    } else {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: {mark_name} died by other hands; the offer lapses"),
        );
        drop(own(hirer_h));
    }
}
```

- [ ] **Step 3: call it from the war hook**

In war.rs's `on_member_died` (war.rs:88-97), after the
`genome::drop_individual(id)` block and before `try_ai_revenge`:

```rust
        crate::bounty::on_death(&member);
```

- [ ] **Step 4: extend advance() and bounty_status for Owed**

In `advance`, Owed is not the Offered cleanup shape; restructure
the match so each variant owns its cleanup (Owed launches in Task
3; until then it only reports):

```rust
        Some(Bounty::Owed { .. }) => false, // Task 3 launches the courier
```

and the Offered `slot.take()` cleanup keeps its current shape. In
`bounty_status`:

```rust
        Some(Bounty::Owed { hirer_name, mark_name, .. }) => json!({
            "bounty": { "stage": "owed", "hirer": hirer_name, "mark": mark_name }
        }),
```

- [ ] **Step 5: build + deploy hot** (same command; Player.log
  shows the fresh ready line)

- [ ] **Step 6: live-verify attribution**

Stage an offer (Task 1 step 5 recipe). The operator kills the
mark in-game with their own survivor. Expected: chronicle line
"the bounty on X is claimed", Player.log attribution line,
bounty_status stage "owed". Also verify the lapse branch once: on
a later offer, let an AI war kill the mark and watch the offer
lapse instead.

- [ ] **Step 7: commit**

`git commit survivalist-mod/src/bounty.rs survivalist-mod/src/war.rs -m "survivalist: bounty kill attribution rides the OnMemberDied prefix; a player kill turns the offer into a debt" && git push`

---

### Task 3: the payment courier

**Files:**
- Modify: `survivalist-mod/src/bounty.rs`

**Interfaces:**
- Consumes: `Bounty::Owed` from Task 2;
  `common::carry_off_stored_goods` (common.rs:219),
  `common::base_centre` (common.rs:169).
- Produces: variant `Bounty::Paying`; the full arc closes.

- [ ] **Step 1: add the Paying variant and courier stage**

```rust
    Paying {
        hirer_h: i32,
        hirer_name: String,
        courier_h: i32,
        courier_name: String,
        player_h: i32,
        squad_id: i64,
        home: (i64, i64),
        stage: Stage,
        loaded: i64,
        deadline: f32,
    },
```

with `enum Stage { Going, Returning }` (murder.rs:50 shape,
minus Strike).

- [ ] **Step 2: launch the courier from Owed**

In `advance`, the Owed arm calls `launch_courier`. The launch:
find the player camp (ctype "Player") and its base_centre; pick
the hirer's first free member (alive, human, conscious, not
squadded, not the leader: the murder.rs:291-339 eligibility loop
without the genome ranking); load payment with
`carry_off_stored_goods(hirer, &[courier_h], BOUNTY_PAY_STACKS,
GoodsFilter::NonFood)`. Zero stacks loaded is the deadbeat
branch: chronicle `"{hirer_name} cannot pay the bounty"`, log,
and the bounty ends (drop all handles). Otherwise launch the
1-member Trade squad at the player's centre (the exact
murder.rs:247-261 AddSquad/AddToSquad/GoalTile/SetSquadAction
calls), transition to Paying with `deadline: now +
COURIER_TIMEOUT_SECS`, and log
`"survivalist-mod: bounty: {hirer_name} sends {courier_name} with {loaded} stack(s) of payment"`.
No free member: stay Owed and retry next pass (log once via the
trade.rs:106 cooldown shape is NOT needed; a 5s retry that logs
only on state change is enough: log the wait once by keeping a
`waiting_logged: bool` on Owed).

- [ ] **Step 3: deliver and come home**

The Paying arm mirrors trade.rs:551-607: courier dead means the
payment died on the road (log + chronicle
`"the bounty payment from {hirer_name} never arrived"`, cleanup);
past deadline means recalled (log, cleanup). Arrived when
`player.GetDistSqToNearestBuilding(courier tile) <=
ARRIVE_DIST_SQ`: deliver with a `deliver_carried_payment`
function that is trade.rs:736-808's deliver_carried_food with the
food test inverted (uses `matches_filter(&item,
GoodsFilter::NonFood)` from Task 1), post the chronicle line
`"a courier from {hirer_name} brings your bounty payment"`,
retarget the squad to `home` (murder.rs:521-536 send_home shape),
flip stage to Returning. Returning ends when the courier is
within ARRIVE_DIST_SQ of the hirer's nearest building: RemoveSquad,
drop courier_h + hirer_h + player_h, slot cleared, log
`"survivalist-mod: bounty: paid and closed"`.

- [ ] **Step 4: extend bounty_status for Paying**

```rust
        Some(Bounty::Paying { hirer_name, courier_name, loaded, stage, .. }) => json!({
            "bounty": {
                "stage": "paying",
                "hirer": hirer_name,
                "courier": courier_name,
                "stacks": loaded,
                "leg": match stage { Stage::Going => "going", Stage::Returning => "returning" },
            }
        }),
```

- [ ] **Step 5: build + deploy hot**

- [ ] **Step 6: live-verify the full arc end to end**

war_ignite, bounty_post, operator kills the mark, then watch:
bounty_status walks offered, owed, paying/going,
paying/returning, null; the courier VISIBLY walks the map; the
stacks land in a player storage building (check its contents
in-game before and after); both chronicle lines post. This
watched run is the pillar's first live verification and is what
the status row's next re-rate cites.

- [ ] **Step 7: commit**

`git commit survivalist-mod/src/bounty.rs -m "survivalist: bounty payment courier walks real non-food stacks from the hirer's stores to the player's gate; full arc offer-kill-collect closes" && git push`

---

### Task 4: the work board (the in-game list of open work)

**Files:**
- Research first; implementation files are locked into this plan
  only after the surface is verified (step 2).

Requirement (operator, 2026-07-10): the player must be able to
open something in-game and see every open offer and what it pays.
The chronicle line is an announcement, not a list. The unityforge
tab UI is not player-facing yet (mod_main.rs:29-31: render fires
only from a control-plane op), so the board needs a game-owned
surface.

- [ ] **Step 1: research the game's own list surfaces**

Candidates, in preference order, all vanilla-grain:
1. The game's quest/objective system: campaign scripts show
   objectives on the HUD, so a runtime-created objective may be
   exactly the right surface. Find the classes
   (`ilspycmd Assembly-CSharp.dll`, grep the type list for
   Quest/Objective/Journal/Task) and read who renders them
   (HudBehaviour fields).
2. A notifications/message history, if the status bar keeps a
   scrollable log.
3. A physical board: a readable note at the player's base (the
   game has a Read goal, research.md:79, so readable text items
   exist).

For each candidate: verify by decompile read plus a live probe
(as a permanent diagnostic op, per repo rule) that entries can be
created at runtime, carry arbitrary text, clear on demand, and
survive a save/load. NO implementation until one surface is
verified end to end.

- [ ] **Step 2: decision gate with the operator**

Present what each surface can and cannot show. The operator
picks. Append the implementation steps to this plan (exact class
names, calls, complete code) before writing any of it.

- [ ] **Step 3: implement + live-verify** (steps appended after
  the gate)

Expected: open the chosen surface in-game and read the open
bounty with hirer, mark, and pays; the entry clears when the
bounty resolves or lapses.

- [ ] **Step 4: commit** (paths named once the files are locked)

---

### Task 5: record it

**Files:**
- Modify: `survivalist-mod/docs/status.md` (the "More to do" row)

- [ ] **Step 1: re-rate the row**

Rewrite the row's "Where it stands now, and next" against what
was actually WATCHED in Task 3 step 6 (dated), per the scoring
discipline: what fired live, what is still unwatched (the organic
scan-cadence offer, the lapse branches, the deadbeat branch), and
zero tests. Score honestly (a watched single arc with no tests
reads like the other 2/10-3/10 rows). Next line: the organic
offer watched without bounty_post, then the second offer kind.

- [ ] **Step 2: commit**

`git commit survivalist-mod/docs/status.md -m "survivalist status: bounty arc live-verified; re-rate the work pillar row" && git push`
