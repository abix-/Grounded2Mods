# Testing

> **Authoritative on:** the test client and its framework
> helpers. How to write integration tests against a live mod's
> debug HTTP endpoint with the minimum possible boilerplate.

ueforge ships a blocking HTTP client (`ueforge::client::Api<S>`)
that talks to the mod's `/debug` endpoint. On top of that
client, three helper modules collapse the patterns every UE5
mod investigation reimplements from scratch:

| Module | What it does | Replaces |
|---|---|---|
| [`research`](#research) | Find DataTables, walk rows, find class instances, typed reads | "walk_class + parse TMap header + batch element-array read + decode FName + read row fields" boilerplate |
| [`diff`](#diff) | Snapshot delta analysis. Counters / memory / cpu / threads / GObjects population | "u64_field + saturating_sub + sort + writeln! table" boilerplate |
| [`scenario`](#scenario) | Pester-style fluent DSL for per-skill behavior tests | "connect/skip + read baseline + check skill points + spend + read after + assert + refund" boilerplate |

## Setup

Every test crate shares a `tests/common/mod.rs` that wraps
`Api<S>` for the per-mod `Snapshot` shape:

```rust
// tests/common/mod.rs
use ueforge::client::Api as InnerApi;

pub use ueforge::client::Api as RawApi;

#[derive(Deserialize)]
pub struct Snapshot {
    pub slot: Option<String>,
    /* ... game-specific fields ... */
}

pub struct Api(InnerApi<Snapshot>);

impl Api {
    pub fn try_connect() -> Option<Self> {
        InnerApi::try_connect("MY_MOD_DEBUG_PORT", "/debug").map(Self)
    }
    pub fn require() -> Self { Self(InnerApi::require("MY_MOD_DEBUG_PORT", "/debug")) }
    pub fn snapshot(&self) -> Snapshot { self.0.snapshot() }
    pub fn inner(&self) -> &InnerApi<Snapshot> { &self.0 }
    // ... game-specific helpers (that call op_ok under the hood) ...
}
```

`inner()` exposes the raw `Api<S>` so the framework helpers
work; the newtype is for game-specific convenience.

## Standard op shortcuts on `Api<S>`

Every mod that wires `ueforge::rpg::ops` gets these for free:

```rust
api.skill_spend("attack_damage", 1);    // -> S
api.skill_refund("attack_damage", 1);   // -> S
api.skill_toggle("move_speed", false);  // -> S
api.set_skill_points(50);               // -> S
api.reload_slot();                      // -> S

// Read from the snapshot's player_state JSON path:
api.skill_level("attack_damage")        // -> u32
api.skill_points()                      // -> u32
api.snapshot_value()                    // -> serde_json::Value (raw)
```

Each shortcut calls `op_ok` (panics on op failure) and returns
the post-op state. Use the raw `op` / `try_op` if you need to
inspect failures.

## research

`ueforge::client`. Find UObjects, walk DataTables,
read fields by offset.

```rust
use ueforge::client;

// Find a DataTable by short name; returns (selector, raw_addr).
let (dt_sel, _addr) = client::find_data_table_by_name(api, "DT_Materials")?;
// ...or by path substring (when multiple tables share a short name):
let (dt_sel, _) = client::find_data_table_by_path(api, "Table_StatusEffects.Table_StatusEffects")?;

// Walk every row.
let rows = client::read_data_table_rows(api, &dt_sel)?;
for row in &rows {
    let name = client::fname_to_string(api, row.fname)?;
    let stack = client::read_i32(api, row.addr, 0x48);
    println!("{name}: {stack}");
}

// Find class instances.
let cdo = client::find_class_cdo(api, "BP_EnvironmentalDamage_C")?;
let player = client::find_live_instance(api, "BP_SurvivalPlayerCharacter_Female02_C")?;
let all = client::walk_class_instances(api, "SurvivalCharacter", 100);

// Read fields at offsets.
let bytes = client::read_bytes(api, addr, offset, 16);
let i = client::read_i32(api, addr, 0x48);
let f = client::read_f32(api, addr, 0x32C);
let u = client::read_u32(api, addr, 0xFC);
let b = client::read_u8(api, addr, 0x67);
let p = client::read_u64(api, addr, 0x1340);

// Follow a component pointer (8-byte read with null check).
let hc_addr = client::read_component_ptr(api, player.addr, 0x1340)?;

// Typed UFunction call:
unsafe {
    let (out, _state) = api.call_ufunction_typed::<MyParms>(
        "HealthComponent", "AddHealth", "live_player_hc", parms,
    )?;
}

// Sample thread modules + Display the report:
let report = research::sample_thread_modules(api, 30_000, 100);
println!("{report}");
```

## diff

`ueforge::client::diff`. Snapshot deltas with `Display` impls
that emit the same table format you'd hand-roll.

```rust
use ueforge::client::diff::MetricsSnapshot;
use std::time::Duration;

// One-shot before/after window:
let s0 = MetricsSnapshot::from_api(api);
std::thread::sleep(Duration::from_secs(30));
let s1 = MetricsSnapshot::from_api(api);

// Aggregate diff:
println!("{}", s0.diff_all(&s1));

// Or per-section:
println!("{}", s0.diff_counters(&s1));
println!("{}", s0.diff_memory(&s1));
println!("{}", s0.diff_cpu(&s1));
println!("{}", s0.diff_threads(&s1));
println!("{}", s0.diff_population(&s1));

// Time-series (1Hz over 60s):
let series = MetricsSnapshot::sample_series(api, 60, Duration::from_secs(1));
println!("{series}");
```

The diff structs (`CounterDiff` / `MemoryDiff` / `CpuDiff` /
`ThreadsDiff` / `PopulationDiff`) are also useful for
test assertions:

```rust
let diff = s0.diff_counters(&s1);
assert!(diff.our_total_ns < 100_000_000, "our hot paths > 100ms wall");

let mem = s0.diff_memory(&s1);
assert!(mem.working_set_delta_mb() < 50.0, "leaking > 50 MB / window");
```

## scenario

`ueforge::client::scenario`. Pester-style fluent DSL for
per-skill behavior tests.

```rust
use ueforge::client::scenario;

#[test]
fn attack_damage_grows() {
    let Some(api) = common::Api::try_connect() else { return };
    scenario::for_skill(api.inner(), "attack_damage")
        .reads(|s: &common::Snapshot| {
            Some(s.live_player.as_ref()?.asc.as_ref()?.custom_damage_multiplier)
        })
        .should_grow_when_spent();
}
```

Four assertion shapes:

| Method | Shape |
|---|---|
| `should_grow_when_spent` | spend 1 -> value strictly greater -> refund |
| `should_shrink_when_spent` | spend 1 -> value strictly less -> refund |
| `should_revert_when_refunded` | spend then refund -> value matches baseline |
| `should_revert_when_toggled_off` | spend, toggle off (vanilla restored), toggle on (boost restored), refund |

The DSL handles environment prerequisites (skips with a clear
log line if no live player / no skill points) so partial setups
don't fail the suite. Refunds are automatic (cleanup runs even
on assertion failure).

The `reads(closure)` closure returns `Option<T>` so tests can
short-circuit on missing snapshot fields without `.unwrap()`
chains. The DSL skips the test when the closure returns `None`
twice (baseline + post-spend), with a log line naming which
extract failed.

## Run pattern

Every integration test crate uses the same shape:

```text
1. Launch the game with the mod's debug endpoint enabled
   (e.g. settings.example.json sets debug.http_port = 17171).
2. set MY_MOD_DEBUG_PORT=17171
3. cargo test --target x86_64-pc-windows-msvc --test foo \
       -- --test-threads=1 --nocapture
```

Always pass `--test-threads=1`; tests share a single global
game state, so concurrent ops would race.

## Why this saves time

Before: each new per-skill test was ~30-50 lines of
connect/skip/baseline/spend/assert/refund boilerplate.

After: 5-line scenario builder. New skill tests read like
English sentences ("for skill X, reads Y, should grow when
spent").

Across both g2rpg and ows-tweaks the migrations dropped ~570
lines of duplicated test code; ~1100 lines of reusable framework
landed in its place. **Every future UE-game investigation gets
the same savings.**

## See also

- [http.md](http.md). The underlying HTTP listener + op
  catalog the client talks to.
- [`../README.md`](../README.md). Audit table; rows under
  "Test client" name every helper in this doc.
- [RESEARCH.md](RESEARCH.md). Test-driven investigation
  doctrine. Pair with the helpers here.
