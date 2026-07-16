+++
title = "Registering a primitive"
weight = 33
+++

A primitive is a Rust function the machine exposes to vix as an effect. You
register it once, typed; vix calls it by name with a `where`-record; the
scheduler resolves it at the demand layer, memoizes it under its declared
policy, and folds its response into the caller's identity like any other value.
This page is the how-to for the v1 registration API. The rules it implements
live in the [primitive spec](/spec/machine/primitive/); this is the surface, not
the law.

Primitives are host services — exec, fetch, format decode, probes. **Pure
operations are never primitives** (`machine.execution.no-pure-hostcalls`): if a
thing is a function of its arguments and nothing else, write it in vix. The API
below is for functions that go and find something.

## Registration

```rust
use vix::runtime::primitive::{MemoPolicy, PrimitiveFailure, PrimitiveSet};

#[derive(facet::Facet)]
struct ProbeRequest {
    text: String,
}

#[derive(facet::Facet)]
struct Version {
    major: i64,
    minor: i64,
    patch: i64,
}

let mut primitives = PrimitiveSet::new();
primitives.register_function::<Version, ProbeRequest, _>(
    "probe_version",
    MemoPolicy::Hermetic,
    |req: ProbeRequest| -> Result<Version, PrimitiveFailure> {
        let mut parts = req.text.split('.');
        let mut next = || parts.next().and_then(|p| p.parse().ok());
        match (next(), next(), next()) {
            (Some(major), Some(minor), Some(patch)) => Ok(Version { major, minor, patch }),
            _ => Err(PrimitiveFailure {
                code: "malformed_version".into(),
                message: format!("{:?} is not a dotted triple", req.text),
            }),
        }
    },
)?;
```

The type parameters are `<Response, Request>` — response first — and the closure
is `Fn(Request) -> Result<Response, PrimitiveFailure>`. Request and Response are
ordinary `#[derive(facet::Facet)]` types. The name is the call surface. That is
the whole registration: no per-primitive match arm, no scheduler field, no
receipt variant — the machine has no fixed effect set
(`machine.primitive.registered`).

Registration derives both schemas from the facet shapes, validates them into the
lossless vir subset, computes a content-derived `PrimitiveId`, and stores the
closure behind one object-safe trait. A structural change to Request or Response
re-keys the id, so every downstream demand recomputes automatically.

## The typed subset

Request and Response types are checked at registration time. v1 accepts a
**lossless** subset only — a shape that would silently narrow is rejected, not
coerced:

| Rust / facet | vix type |
| --- | --- |
| `bool` | `Bool` |
| `i64` | `Int` |
| `String` | `String` |
| `struct { … }` | `Record` |
| `enum { … }` | `Enum` |
| tuple `(A, B)` | `Tuple` |
| `Vec<T>` / list | `Array` |
| set | `Set` |
| map | `Map` |
| `Option<T>` | `Option` |
| unit | empty tuple |

Everything else is a registration-time `RegistrationError::UnsupportedShape`
naming the offending field path: the sized integers (`u*`, `i8`–`i32`, `i128`),
floats, `char`, raw bytes, fixed-size arrays, tensors, channels, `Dynamic`,
externals. Widening the subset is tied to vir type growth, not to this API — a
`f64` field is a hard error today, on purpose, because there is no vir `Float`
to receive it losslessly.

```rust
#[derive(facet::Facet)]
struct BadRequest {
    weight: f64, // registration fails: UnsupportedShape { path: "weight", … }
}
```

## The call surface

Vix calls a registered primitive by name, passing the request record as
named arguments after `where`:

```vix
let v = probe_version where { text: "1.2.3" };
yield expect_eq(v.major, 1);
```

`probe_version where { text: … }` type-checks `text` against the registered
request record — vix's named-argument idiom *is* the request value
(`machine.primitive.requests-are-values`). Every request field is named; there
is no positional subject in v1. The response is an ordinary value: `v.major`
projects a field, `v` compares whole, it can be `let`-bound, passed on, or fed to
another primitive. A record response frames as an identity tree with empty
resident bytes (the weavy ABI constraint — aggregates are never serialized into
resident memory); the consumer reads it back as a realized value input and
projects off it like any other record.

The compiler learns these names from a **manifest** derived from the registered
set:

```rust
let manifest = primitives.compiler_manifest();
let compilation = Compiler::new().with_primitives(manifest).compile(source)?;
```

`compiler_manifest()` projects the descriptors down to vir types and effect ids
only — **no handlers cross into the compiler**. This is the runtime→compiler
boundary: the compiler never imports the runtime, and effect identity in the IR
is a content hash (`vir::EffectId`), never a runtime `PrimitiveId`. The runtime
converts one back to the other at dispatch. Register and derive the manifest from
the *same* set and the ids match by construction.

## Memo policy

The second argument to `register_function` declares how the machine reuses the
effect's result (`machine.primitive.memo-policy`). v1 implements two of the four
variants:

- **`Hermetic`** — the result is memoized. The request value *is* the entire
  input set, so a second demand of the same request folds the same response
  identity into the caller's key and hits the memo: the closure is **not run
  again**. Only legitimate when every input is witnessed — for a data-in /
  data-out primitive, the request captures all of it, so `Hermetic` is honest.
- **`Volatile`** — never memoized. Every demand re-runs the closure. Use this
  when the result is not a pure function of the request (ambient reads the
  machine cannot witness).

`Pinned` and `Observed` are declared in the enum but are an **honest typed
error** in v1 (`RuntimeFault::UnsupportedEffectPolicy`) — they ship with fetch
and observe. Nothing silently degrades a `Pinned` primitive to `Hermetic`;
mislabeling a policy is a correctness fault, so the machine refuses rather than
guesses.

## The failure model

A primitive reports failure two ways, on two different planes that never mix:

- **A language failure** — the closure returns `Err(PrimitiveFailure { code,
  message })`. This is an expected, receipted outcome: the machine interns a
  generic `FailureValue::Primitive` keyed by the effect recipe, memoizes it under
  the policy (a Hermetic failure is cached like a Hermetic success), and
  propagates it through the ordinary failure path. The caller sees a failed
  value, not a crash. A `"version unavailable"` result is *data*.
- **A machine fault** — the request tree does not match the registered schema,
  or the primitive violates the effect protocol. This is a bug, not an outcome:
  it aborts the evaluation with a typed `MachineError` on the machine plane and
  is never memoized. Because the compiler type-checked the call, a request that
  fails to decode is a machine invariant, not a user error.

`PrimitiveFailure` carries a `code` and a `message` for rendering; in v1 they are
report bytes, not identity — a failure's identity is its recipe and site, so two
failures of the same effect at the same site share a cell.

## What the primitive sees

Inside the closure you get the decoded request and nothing else — no store, no
memo, no scheduler, no path or network handle. Interning the response, recording
the read-set into the receipt, and reporting completion all happen through the
adapter; the `register_function` sugar path does them for you. The full-control
trait (`EffectCtx`, for future async or world-touching primitives) exposes
exactly that witness window and no more (`machine.primitive.effectctx-witness-only`).
For a data-in / data-out primitive the request value is the whole read-set, so
the receipt is complete without a single opt-in call.
