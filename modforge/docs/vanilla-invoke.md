# Vanilla function invocation

`modforge::vanilla` calls resolved native game functions through the Win64 ABI.
It validates runtime arguments, catches supported structured exceptions by
default, and can expose every signed function through the shared operation
registry.

This is the shipped API. Source:

| Part | Purpose | Source |
|---|---|---|
| Signature | Describes argument and return kinds | [`sig.rs`](../src/vanilla/sig.rs) |
| Dispatcher | Marshals values into Win64 registers and stack slots | [`dispatch.rs`](../src/vanilla/dispatch.rs) |
| Invoker | Resolves a named target and calls it | [`invoker.rs`](../src/vanilla/invoker.rs) |
| Operations | Registers `vanilla.invoke` and `vanilla.list` | [`ops.rs`](../src/vanilla/ops.rs) |
| Registry | Stores the signature beside the function target | [`sleuth.rs`](../src/patterns/sleuth.rs) |

## Supported values

Arguments support signed and unsigned integers from 8 to 64 bits, pointers,
booleans, `f32`, and `f64`. Returns support the same kinds plus `void`.

The dispatcher supports up to 16 arguments. The first four positions use the
Win64 integer or floating-point registers. Remaining values use stack slots.
Integers and pointers return through RAX; floats return through XMM0.

Large struct returns, variadic functions, and automatic buffer allocation are
not supported. Pass existing buffers as pointer arguments when the game
function expects them.

## Register a callable target

Declare a static signature and attach it to a function entry in the existing
target registry:

```rust
use modforge::patterns::sleuth::{Candidate, Recipe, TargetDef, TargetKind};
use modforge::vanilla::{ArgKind, RetKind, Signature};

pub static HORSE_REBUILD_SIG: Signature =
    Signature::new(&[ArgKind::Ptr], RetKind::Void);

static HORSE_REBUILD: TargetDef = TargetDef {
    name: "HORSE_REBUILD",
    kind: TargetKind::FunctionEntry {
        signature: Some(&HORSE_REBUILD_SIG),
    },
    candidates: &[Candidate {
        sig: "48 89 5C 24 ?? 48 89 6C 24 ??",
        recipe: Recipe::MatchIsAddress,
    }],
    hint_rva: Some(0x1400b3070),
    hint_tolerance: 0x4000,
    validators: &[],
};
```

Use `signature: None` for targets that are resolved for hooks, detours, or
research but must not be invoked through this API. The signature and address
remain one registry fact and cannot drift between separate catalogs.

## Call from Rust

Construct an `Invoker` from the registry's resolver and pass an `ArgValue` for
each declared argument:

```rust
use modforge::vanilla::{ArgValue, Invoker, RetValue};

let invoker = Invoker::new(&HORSEY_RESOLVER);
let result = invoker.call(
    "RNG_NEXT_MODULO",
    &[ArgValue::U32(100)],
)?;

let RetValue::U32(value) = result else {
    return Err("unexpected return kind".into());
};
```

`Invoker::call` is the normal path. It:

1. Finds the named `TargetDef`.
2. Requires a function entry with a signature.
3. Resolves a non-null address.
4. Checks argument count and kinds.
5. Dispatches inside `modforge::seh::guard`.
6. Decodes the declared return kind.

Errors distinguish an unknown target, a missing signature, an unresolved
address, argument or dispatch failure, and a caught native fault.

`unsafe Invoker::call_unsafe` skips SEH protection. Use it only where the
target, signature, address lifetime, and calling context have already been
proven and the SEH setup cost matters.

## Calling thread

Invocation runs synchronously on the caller's current thread. The API does not
move work to a game, render, or Unity main thread. A function that touches
thread-owned game state must be dispatched through the consumer's existing
main-thread mechanism before calling `Invoker`.

SEH protection can recover supported native faults. It cannot make a function
thread-safe or repair an incorrect signature that happens to return plausible
data.

## Register the shared operations

After creating a static resolver, register both operations once during mod
initialization:

```rust
modforge::vanilla::ops::register(
    &modforge::ops::OP_REGISTRY,
    &HORSEY_RESOLVER,
);
```

The resolver must be static because operation handlers retain it.

### `vanilla.list`

Lists only function entries that have signatures:

```json
{
  "op": "vanilla.list",
  "args": {}
}
```

The operation result contains sorted entries with `name`, `addr`, `signature`,
and `from_hint`. An unresolved entry has a null address.

### `vanilla.invoke`

Calls one signed target. Safe invocation is the default:

```json
{
  "op": "vanilla.invoke",
  "args": {
    "target": "RNG_NEXT_MODULO",
    "args": [
      {"kind": "u32", "value": 100}
    ]
  }
}
```

The operation result is:

```json
{
  "ok": true,
  "ret": {"kind": "u32", "value": 42},
  "elapsed_us": 12
}
```

Set `"safe": false` inside the operation arguments to select
`call_unsafe`. Pointer and `u64` values accept JSON numbers or hexadecimal
strings. Pointer and `u64` returns are encoded as hexadecimal strings.

Operation failures after request parsing are returned as
`{"ok": false, "error": "...", "elapsed_us": N}`. The Modforge server
wraps that operation result in its normal response envelope.

## Safety boundary

The registry author is responsible for proving the native signature. Runtime
validation only proves that supplied `ArgValue` variants match the declaration.
It cannot prove that the declaration matches the game binary.

Revalidate function addresses and signatures after every supported game build.
Prefer `Invoker::call`, use the correct game thread, and keep raw pointers valid
for the full call.
