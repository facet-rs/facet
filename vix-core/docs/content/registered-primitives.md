+++
title = "Registered primitives"
weight = 33
+++

A primitive is an effectful machine service exposed to Vix through the generic
primitive protocol. It is not a convenient escape hatch for code that happens
to be easier to write in Rust.

The classification rule comes first:

- A deterministic transformation of its arguments belongs in Vix and Weavy.
  String splitting, version parsing, collection transforms, tree-path
  construction, and format-independent validation are examples of pure work.
- A primitive crosses an authority boundary: fetching pinned bytes, observing a
  coordinate, executing a command, reading content from a
  capability-controlled tree or service, or performing another operation whose
  inputs must be witnessed.

Moving pure work into a primitive would hide demand structure, add a host-call
boundary, and violate `machine.execution.no-pure-hostcalls`.

## The production path

The current runtime uses one path for registered primitive execution:

1. The compiler emits `Op::InvokePrimitive` with a typed request value and a
   versioned `PrimitiveId`.
2. Lowering turns that operation into a verified Weavy host-call yield.
3. The scheduler derives the primitive demand, calls
   `PrimitiveDispatcher::begin_or_join`, retains the yielded Weavy frame
   off-stack, and returns control to the runnable loop.
4. The demand-owned `EffectTicket` completes through the unified completion
   inbox.
5. The scheduler admits the response through the demand-owned effect authority,
   writes it into the retained frame through that frame's ABI plan, and resumes
   the task.

There is no synchronous per-primitive drain and no per-primitive scheduler
branch. Independent yielded frames may coexist while their effects are in
flight.

The runtime trait is deliberately small:

```rust
pub trait Primitive: Send + Sync {
    fn descriptor(&self) -> &PrimitiveDescriptor;
    fn begin(&self, request: ValueId, ctx: EffectCtx) -> EffectTicket;
}
```

`PrimitiveDescriptor` declares the primitive identity, request, response, and
failure schemas, memo policy, protocol version, and capability schemas. A
behavioral or protocol change must change the descriptor identity.

## Implement primitives as types

A primitive implementation is a named type with explicit fields. Its effect
logic should live in free-standing functions or ordinary methods whose inputs
are visible in their signatures.

```rust
pub struct PinnedFetchPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Primitive for PinnedFetchPrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx) -> EffectTicket {
        begin_pinned_fetch(request, ctx)
    }
}

fn begin_pinned_fetch(request: ValueId, ctx: EffectCtx) -> EffectTicket {
    // Decode the typed request, perform only EffectCtx-authorized reads, and
    // complete the demand-owned ticket.
    todo!("the built-in operation is elided here")
}
```

The elided operation is a named function with explicit arguments, not the
registration API. The example shows the ownership shape, not a replacement
implementation for the built-in fetch primitive.

Registration must not accept an arbitrary closure as the primitive's hidden
environment. A closure that captures a store, client, cache, filesystem root,
or runtime handle has merely recreated global state behind an `Fn` object.
Stateful implementations use explicit named fields and capability interfaces;
the reviewable type is the dependency boundary.

## Authorities are explicit

`EffectCtx` is the primitive's machine window. It provides witnessed reads,
typed value interning, progressive publication, journal observations, and
single completion. It does not expose the memo or scheduler.

External authorities are installed as explicit trait objects through
`PrimitiveServices` and then included in the staged effect authority used for
the demand. Current examples include:

- `ValuePersistence` for self-verifying value bodies;
- `OriginAdapter` for capability-relative origin reads;
- `ClaimHistory` for append-only observation claims.

The request still carries semantic inputs such as capability identities,
coordinates, and expected value identities. A service object is execution
authority, not an omitted request argument.

This split is important for receipts. Reads performed through `EffectCtx`
become `ReadWitness` entries automatically. Ambient filesystem, environment,
network, time, or process-state reads outside that window cannot support a
`Hermetic` claim.

## Requests and responses

Primitive requests and responses are ordinary content-addressed values.
`PrimitiveValue` is the runtime-side structural representation:

- byte leaves carry resident bytes;
- products and variants carry typed fields;
- sequences carry element schemas and child values;
- aggregate resident bytes stay empty.

`EffectCtx::read` records the requested projection and observation in the
receipt. `EffectCtx::intern_value` stages a typed response for scheduler
admission. Completion carries only identities and typed lifecycle data, not an
untyped serialized side channel.

The current built-ins construct and inspect `PrimitiveValue` directly. A
general Facet-to-primitive-value adapter is a useful future layer, but it must
preserve this structural representation and cannot change the scheduler
protocol.

## Completion and failure

`PrimitiveCompletion` keeps three planes distinct:

- `Ok(ValueId)` publishes an admitted response value;
- `Failed(ValueId)` names a language-level failure value;
- `MachineError(PrimitiveMachineError)` reports a machine or authority fault.

Only the first and third are wired through the production scheduler today.
Returning `Failed` currently terminates the waiting tasks as a typed primitive
ABI fault rather than publishing a language failure. A future generic
registration surface must finish that language-failure route before documenting
fallible registered functions as shipped.

The ticket completes at most once. The scheduler also verifies that the
published identity was admitted by the demand's authority and that its schema
matches each waiting frame's response plan before writing any frame.

## Memo and lifecycle laws

`PrimitiveMemoPolicy` has four semantic variants:

- `Hermetic`: every non-request input is confined and witnessed;
- `Pinned`: the response identity is already present in the request;
- `Observed`: the response identity becomes known through a receipted
  observation;
- `Volatile`: no persistent reuse claim is made.

The policy is an obligation, not a performance hint.

Tickets belong to demands rather than tasks. A killed task may replay and join
the same ticket without starting the backend operation twice. Explicitly
abandoning the final obligation cancels the ticket and publishes no memo,
receipt, claim head, or partial result. Late or duplicate completion is ignored
as a typed stale completion.

## Current registration boundary

`PrimitiveRegistry` and generic dispatcher lookup are implemented. The
production runtime currently assembles the built-in registry for decode,
pinned fetch, and tree text reads during runtime construction. (`observe` was
removed for the 0.1 surface — see the note under "Existing implementations".)

The public manifest-driven extension surface is not implemented yet. In
particular, external code cannot currently:

- install an additional registry when constructing `Runtime`;
- project registered descriptors into compiler-visible names and types;
- compile an arbitrary surface call to a newly registered primitive without
  adding compiler-side construction for that request.

Those are real remaining gaps to the fully registered machine described by
`machine.primitive.registered`. Documentation and adapters must not present
them as shipped.

When that surface lands, it should register concrete primitive types or
explicit trait objects. A typed Facet adapter may derive request and response
schemas and perform structural conversion, but it should not revive
`register_function(name, policy, closure)` as an environment-smuggling API.

## Surface bindings

The gap "project registered descriptors into compiler-visible names" is
resolved by a **binding layer** that is deliberately separate from both the
registry and the compiler. Three concerns are kept apart:

- **registry identity** — a primitive is a `PrimitiveId` matched by schema. It
  carries no surface name and never learns one.
- **surface name and placement** — what a primitive is *called* in vix source,
  and where that name lives in resolution.
- **request construction** — how surface arguments fold into the primitive's
  request record.

The rule is **one primitive, one binding, one name**. A primitive does not get
two spellings. Behavioural variants that must share identity, claim, and memo
state — `observe`/`refresh`, `json`/`toml` decode — are *modes of a single
effect*, carried as **request fields the primitive reads**, not as extra
primitives and not as extra compiler intrinsics. The evidence that they are one
effect is that they share a claim head: a `refresh` reconciles against the
`observe` claim on the same coordinate (`PrimitiveMachineError::RefreshConflict`).
Splitting them into two `PrimitiveId`s would fork that shared state and force a
back-channel to reunite it.

Ergonomic aliases are therefore **vix functions**, not primitives — where the
alias is expressible as one:

```
fn refresh<Origin>(origin: Origin) -> Blob { observe(origin, Mode::Refresh) }
```

This is the house pattern for the ergonomic layer (`n`, `toml_n`, `json_n`,
`crate_archive` are vix wrappers in the corpus). `refresh` was the outlier —
promoted to a compiler intrinsic instead of written as a vix function — and is
now **retired**: the single `observe` binding takes its mode as a surface
argument (`observe(origin, Mode::Observe | Mode::Refresh)`, read by
`observe_mode_arg` in `compiler.rs`, the twin of `decode_format_arg`), `refresh`
is a stdlib vix function over it (`crate::stdlib`), and the `refresh` intrinsic
is deleted. The wrapper is generic in the origin because the `OriginHint` origin
type is not surface-nameable — the same reason `json_decode`'s target is a type
parameter — so the `observe` binding enforces the real origin type at the call.
A vix function so bound is effectful exactly when its body invokes an effectful
primitive; effect tracking flows through the call as for any wrapper.

**Generic aliases became vix functions when monomorphization landed.**
`json_decode` / `toml_decode` are *generic*: `decode<T>` needs its target type
at the call site. Before monomorphization a generic vix *function* was rejected
at lowering (`GenericLoweringUnsupported`); it now lowers per concrete
instantiation, so the decode aliases are ordinary stdlib vix functions over the
`decode(document, Format)` binding, with `T` recovered by return-position
inference. The fallible `try_json_decode` / `try_toml_decode` are the same shape
over a `try_decode(document, Format)` binding, returning `Result<T, DecodeError>`
with `T` recovered from the expected `Result` — no call-site turbofish, matching
the language's inference-only instantiation. Turbofish is now rejected on every
call (the decode intrinsics were the last surface that allowed it).

### Placement

A binding declares where its name lives:

- **prelude** — injected into every module's scope, callable with no `use`
  (today's `fetch`, `observe`). This is the prelude layer the binder currently
  defers (see the `binder` module docs, which name `fetch` as the example).
- **namespaced** — reached through a `::`-path or `use module::name`, e.g.
  `some::ns::cool_function`.

Placement is a binder concern: the binder consults the binding set when
resolving unqualified prelude names and qualified paths, in place of leaving
them in its `unresolved` bucket. The compiler's `lower_value` consults the same
set in place of the hardcoded `effect_intrinsic` / `decode_format` string
matches, emitting `Op::InvokePrimitive` with a request built from the binding's
request shape. Because each primitive binding is one name with a data-described
request, an arbitrary registered primitive compiles without compiler-side
construction — closing the second and third gaps above.

### Status

The model exists as representable types (`vix::binding`): `Placement`
(`Prelude` | `Module`), `Binding`, `BindingTarget` (`Primitive` | `VixFunction`),
and `BindingRegistry`, with `builtin_bindings()` encoding the intended
projection of the built-ins. It is **not yet wired**: the binder still defers
prelude names and `lower_value` still dispatches intrinsics by hardcoded
strings. Routing binder resolution and `lower_value` through a `BindingRegistry`
— in place of the hardcoded `effect_intrinsic` string matches — is the remaining
work. The mode retirements it describes are done: `refresh`, `json_decode`, and
`toml_decode` are stdlib vix functions over the mode-taking `observe` / `decode`
bindings. Until the registry is wired, this layer must not be presented as the
live binding path.

## Existing implementations

The production examples are:

- `DecodePrimitive`: synchronous typed document decoding;
- `PinnedFetchPrimitive`: asynchronous identity-first value retrieval;
- `TreeReadPrimitive`: witnessed text reads from capability-controlled trees.

They all implement the same trait and cross the same Weavy yield, ticket,
completion-inbox, and frame-resume path.

> **`observe`/`refresh` retired for 0.1.** The `observe`/`refresh` discussion
> above documents the binding-layer *principle* (behavioural modes are request
> fields of one effect, not extra primitives) using observe as its worked
> example. `ObservePrimitive` and its claim-history seam were **removed** for the
> 0.1 surface — 0.1 consumes an existing lockfile, so it never observes a live
> coordinate. The example is retained because the design rule it illustrates
> still governs any future effect with modes. See the design memo
> `surface-0.1-scope-and-layering`.
