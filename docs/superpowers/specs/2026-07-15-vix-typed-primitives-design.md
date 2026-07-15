# Vix typed primitive registration — design

Status: approved direction (Approach A), spec elaboration for implementation.
Owner thread: single long-lived orchestration session, 2026-07-15 onward.

## Goal

Expose Rust functions to vix through one typed registration API:

```rust
let mut primitives = PrimitiveSet::new();
primitives.register_function::<ProbeResponse, ProbeRequest>(
    "probe_version",
    MemoPolicy::Hermetic,
    |req: ProbeRequest| -> Result<ProbeResponse, PrimitiveFailure> { ... },
)?;
```

where `ProbeRequest`/`ProbeResponse` are ordinary `#[derive(Facet)]` types. Vix
source calls the primitive fully typed:

```vix
let info = probe_version where { text: manifest_text };
```

This implements the `machine.primitive.*` rules from
`vix/docs/content/spec/machine/primitive.md` in the **authoritative path only**
(surface → compiler.rs → vir.rs → lowering.rs → runtime/). The legacy
`machine/` module is untouched; the daemon migrates later.

## Non-goals for v1 (deferred, spec-sanctioned)

- exec/fetch/observe backends themselves (plan normalization, toolchain
  probing, PATH traps, tier-2 verified reuse, sealed/attest, codata
  publication). The *interface* reserves every axis these need; the backends
  are later phases. `machine.primitive.exec-*`, `machine.memo.three-tier-reuse`,
  `machine.receipt.journal` are deferred-ok per the rule inventory.
- Concurrent/overlapping effect execution. The trait is non-blocking
  (`begin → EffectTicket`); the v1 scheduler completes tickets synchronously.
  `machine.scheduler.effect-overlap` is satisfied structurally (no serial
  queue is baked into types), activated later.
- Persistence backing. The memo/store persistability *traits* are day-one
  design constraints we must not violate; the vx-store backing is not ours.
- Legacy machine migration and daemon integration.

## Hard constraints (load-bearing from commit 1)

From the spec inventory (see research, `machine.*` rules):

1. **One trait, one generic effect node** — no per-primitive match arms,
   scheduler fields, receipt variants, or ontology strings anywhere
   (`machine.primitive.registered`).
2. **Requests and responses are ordinary typed vix values** — interned,
   content-addressed; lowering emits one generic effect request carrying
   (primitive identity, request value) (`machine.primitive.requests-are-values`).
3. **Effect results are memo entries** keyed by their demand under the declared
   policy; private caches are banned (`machine.memo.effect-results`,
   `machine.cache.no-private-caches`).
4. **Memo policy axis is the four-variant enum** Hermetic | Pinned | Observed |
   Volatile, declared in the descriptor. v1 implements Volatile (skip memo
   insert) and Hermetic (plain memo entry); Pinned/Observed ship with
   fetch/exec. Honest labeling is a correctness gate: nothing in v1 may
   self-declare Hermetic unless all inputs are witnessed — for
   data-in/data-out primitives the request value *is* the entire input set,
   so Hermetic is legitimately available to them.
5. **EffectCtx is the only machine window** — witness-typed reads, typed result
   interning, event emission, completion. No raw store/memo/scheduler/path/
   network handle (`machine.primitive.effectctx-witness-only`).
6. **Completion tri-state** Ok(value) | Failed(Failure) | MachineError, typed,
   never inferred from strings/statuses (`machine.lifecycle.effect-failure-is-a-result`).
   Failed memoizes under policy with a full receipt; cancellation/transient
   never memoizes (`machine.lifecycle.cancellation-poisons-not-memoizes`).
7. **Tickets are owned by the demand, not the task**
   (`machine.scheduler.tickets-outlive-tasks`); waits block on completion
   events, never the clock (`machine.scheduler.block-on-event`).
8. **Value identity is the pair (SchemaRef, ContentHash), blake3**
   (`machine.identity.value-identity-pair`, `machine.identity.blake3`).
9. **Typed errors everywhere**; `Result<_, String>` forbidden
   (`machine.error.typed`). RefCell-closure smuggling banned
   (`machine.abi.host-env-type`).
10. **No performance regressions**: pure ops never enter this API
    (`machine.execution.no-pure-hostcalls`); nothing added to pure hot paths;
    corpus/budget bench gate before merge.

## Architecture decision: demand-level dispatch, no weavy changes

Research settled the dispatch route. The lazily-awaited wire resume channel is
**scalar i64 only** (`weavy/src/exec.rs:1196`, `scheduler.rs:645-651`), so a
typed Response cannot ride `Op::AwaitWire`. The alternative — a new weavy
effect op with an aggregate resume table — means verifier contracts, drive
tables, and JIT stencil work, and `machine.execution.checked-access-membrane`
bans whole-frame host ABIs anyway.

But aggregates already have a first-class channel: **value inputs bound at
task spawn as realized store handles** (`ValueRepresentation::RealizedHandle`,
`scheduler.rs:499-537`). So:

**An effect request is resolved between weavy tasks, at the demand layer.**
The consuming island declares the primitive's response as a value input. When
the scheduler assembles that island's inputs, it finds an *effect edge*
(primitive identity + request-producing island) instead of a producer island:
it evaluates the request island (pure), interns the request value, consults
the memo under the primitive's policy, dispatches on miss, interns the typed
response, and binds it as an ordinary realized value input. The consumer task
spawns only after the effect completes — which is exactly how value inputs
already behave in the synchronous scheduler.

Consequences:

- **Zero weavy changes in v1.** No new ops, no verifier work, no JIT work.
- Memoization, identity, receipts, and events all happen at the demand layer
  where that machinery already lives.
- `DemandPreimage { closure, arguments }` (`runtime/identity.rs:38`) is reused
  verbatim: `closure` = the PrimitiveId-derived RecipeId, `arguments` =
  `[request ValueId]`. The DemandKey machinery provides effect memo keys for
  free (`machine.arch.reuse-axes-distinct`: probe key = (primitive id,
  request value)).
- The future concurrent scheduler overlaps effects by not blocking on ticket
  completion before spawning *other* demands; the type shapes (ticket owned
  by demand record) already permit this.

## Components

### 1. `vix::runtime::primitive` (new module) — the registry and trait

```rust
pub struct PrimitiveDescriptor {
    pub id: PrimitiveId,              // versioned, content-derived (below)
    pub name: PrimitiveName,          // namespaced call-surface name
    pub version: u32,                 // author-bumped behavioral version
    pub protocol: u32,                // begin/complete protocol version
    pub request: RegisteredSchema,    // taxon root id + schemas + vir::Type
    pub response: RegisteredSchema,
    pub policy: MemoPolicy,           // Hermetic | Pinned | Observed | Volatile
    pub capabilities: Vec<CapabilityRequirement>, // present, empty in v1
}

pub trait Primitive {
    fn descriptor(&self) -> &PrimitiveDescriptor;
    /// Non-blocking. Completion is delivered through the ctx.
    fn begin(&self, request: RequestRef<'_>, ctx: &mut EffectCtx<'_>)
        -> Result<EffectTicket, Box<MachineError>>;
}

pub enum Completion {
    Ok(ResponseValue),        // interned typed value
    Failed(PrimitiveFailure), // receipted language failure, memoizes per policy
    // MachineError travels the Result channel, never memoizes
}
```

`PrimitiveSet::register_function::<Resp, Req>(name, policy, f)` is the typed
adapter: it derives both schemas once (via `phon::derive::of_shape`), runs the
taxon→vir validator, builds the descriptor, and wraps the closure in an
object-safe adapter that decodes the request, runs `f`, and completes the
ticket inline (sync sugar over the async-shaped trait).
`register(Box<dyn Primitive>)` is the full-control path for future async/
world-touching primitives.

`EffectCtx` v1 surface: `intern_response`, `witness_read` (records a
`ReadWitness` into the demand's receipt), `emit_event`, `complete`. It borrows
narrow store/receipt handles internally; the primitive sees only these methods.

### 2. Identity: PrimitiveId and the three-space bridge

```
PrimitiveId = blake3("vix.primitive.v1" || name || version || protocol
                     || request taxon SchemaId || response taxon SchemaId)
RecipeId(effect) = RecipeId(PrimitiveId digest)   // domain-separated
```

taxon SchemaIds are content-derived (structure-hashed, build-stable —
`taxon/src/identity.rs`), so any structural change to Request/Response re-keys
every demand automatically (`machine.primitive.trait`). The name is part of the
id because taxon hashes only short type names (collision risk flagged in
research).

**vir type naming**: the registered Request/Response become nominal
`vir::Type::Record/Enum` values whose names embed the taxon content id, e.g.
`ProbeRequest@{taxon-id-hex}`. The runtime store id
(`semantic_schema_id` = blake3 of the vir type name) therefore inherits
content sensitivity without touching the runtime identity system. `@` is not
constructible from vix source type names, so no collision with user types.

### 3. Type bridge: facet → taxon → vir, validated at registration

`phon::derive::of_shape` gives taxon schemas; `DeriveError::Unsupported`
already rejects un-derivable shapes. We add the missing axis: a
**taxon→vir mapper/validator** that either produces a faithful `vir::Type` or
rejects registration with a typed error naming the offending field path.

v1-supported subset (lossless only — no silent narrowing):
`Bool → Bool`, `I64 → Int`, `String → String`, `Struct → Record`,
`Enum → Enum`, `Tuple → Tuple`, `List → Array`, `Set → Set`, `Map → Map`,
`Option → Option`, `Unit → unit record`. Everything else
(u*/i8-32/i128/floats/char/Bytes/DateTime/Uuid/QName/Tensor/Channel/Dynamic/
External/fixed-dim Array) is a registration-time `RegistrationError` with the
field path. Widening the subset is future work tied to vir::Type growth (e.g.
Blob), not to this API.

### 4. Value conversion: facet ↔ FramedNode

The store's aggregate values are `FramedNode` identity trees + `FrozenValue`
replay trees with *empty* resident bytes; scalars/strings hold real bytes.
Serializing a Rust struct into resident bytes would violate the weavy ABI
(research risk). So the adapter converts structurally:

- **Request decode**: `FrozenValue` tree → Rust `Req` via facet reflection
  (`facet_reflect` build/Poke path), driven by the descriptor's schema so field
  order/variants are checked, mirroring `realize_value`'s shapes.
- **Response encode**: Rust `Resp` → `FramedNode` tree (+ `FrozenValue`) via
  facet Peek walk, interned with `intern_tree` exactly as `realize_value`
  produces values. String/scalar leaves carry canonical bytes; aggregates are
  identity-tree only.

Both directions are new glue in `runtime::primitive::convert`, built against
the same node vocabulary `realize_value`/`failure_node` already use, with
round-trip property tests (facet value → vix value → facet value == identity;
vix-constructed value ↔ Rust-constructed value produce identical `ValueId`).

### 5. Compiler surface: manifest-driven names

The compiler is constructed with (or handed per-compile) a
`PrimitiveManifest` — the descriptor list, no handlers. Injection points:

- `ModuleContext` gains `primitives`; the module build seeds one synthetic
  `FunctionSignature` per primitive so `lower_call`'s existing arity/type
  checking applies unchanged.
- `lower_where_call` (currently hardcoded to `range`) generalizes: registered
  name + `where { ... }` type-checks the named fields as the Request record
  against the registered vir Type — vix's named-argument idiom *is* the
  request record (docs/content/calling.md). v1 call shape: no positional
  subject; every request field named.
- Precedence: local value bindings win (as today); registered names must not
  shadow or be shadowed by `None/Some/by_key/range/expect*/decode` — enforced
  by a registration-time reserved-name check plus a compiler test.
- The call lowers to VIR: pure nodes constructing the Request record, then one
  `Op::EffectRequest { primitive: PrimitiveId }` node consuming it, typed as
  the Response. `EffectFacts` gains `kind: EffectKind::Effect` (new variant);
  `canonical_node` hashes the primitive id (identity: any registered-behavior
  change re-keys recipes).

### 6. VIR partitioning and lowering

`Island` gains `effect_inputs: Vec<EffectEdge>` where
`EffectEdge { primitive: PrimitiveId, request: ValueIslandId }` — parallel to
`wire_inputs`, not conflated with it (research: `wire_inputs` structurally
assumes a producer island; an effect has none). The partition rewrite cuts at
`Op::EffectRequest`: the request subgraph becomes its own value island; the
consumer island's node is rewritten to read a bound value input. Lowering
binds the response as an ordinary `ValueInputBinding`
(`ValueRepresentation::RealizedHandle`) and records the effect edges on the
`LoweringArtifact` (precomputed facts, `machine.execution.facts-precomputed`).

### 7. Scheduler: effect resolution

In `Runtime::evaluate`'s input-assembly step (both lanes — the duplicated
value-input logic at `scheduler.rs:427-527`/`1124-1213` gets its shared part
extracted first, per research risk):

1. For each effect edge: evaluate the request island (ordinary demand),
   yielding the request `ValueId`.
2. Build `DemandPreimage { closure: primitive recipe, arguments: [request] }`
   → DemandKey. Consult the memo (policy-aware: Volatile skips lookup and
   insert; Hermetic uses the normal path). Emit the same demand/memo events
   pure demands get, plus `effect_spawns` and new effect EventKind entries
   parameterized by `PrimitiveId` (data, not per-primitive variants).
3. On miss: create the demand record, mint the ticket **on the demand record**,
   call `begin(request_ref, ctx)`. v1 completes synchronously inside `begin`
   (the adapter calls `ctx.complete`); the scheduler consumes the completion
   event — no polling, no clock.
4. Completion handling: `Ok` interns the response (via ctx), inserts the memo
   entry **with a populated receipt** (first real producer:
   `Receipt { demand, reads: witnessed }`), transitions the demand Ready.
   `Failed(PrimitiveFailure)` follows the existing language-failure
   propagation (`Evaluation.failure`), memoized per policy with receipt.
   `MachineError` aborts with attribution
   (`MachineOperation::Effect` — one new variant, generic over all
   primitives). Re-entrancy trips the existing `ReentrantDemand` guard.
5. Bind the response handle as the consumer's value input and proceed to
   spawn.

`FailureValue` gains one generic variant for primitive failures carrying
(primitive id, typed failure value id, site) — one variant for all primitives,
not per-primitive (`machine.primitive.registered`).

### 8. Errors

- Registration: `RegistrationError` (typed; unsupported shape w/ field path,
  reserved name, duplicate name, schema derivation failure).
- Runtime infra: `MachineError` via new `MachineOperation::Effect` +
  `RuntimeFault` variants (generic).
- Language: `PrimitiveFailure` → `FailureValue` (typed, receipted, memoizable).
- The two planes never mix (existing discipline, `error.rs:10`).

## Testing

- **Unit (02)**: descriptor identity (structural change re-keys PrimitiveId);
  taxon→vir validator accept/reject table; facet↔FramedNode round-trip
  properties incl. ValueId equality with vix-constructed values.
- **Compiler (03)**: name resolution, where-call type errors (wrong field,
  missing field, extra field, type mismatch) as typed diagnostics; precedence
  vs bindings and builtins.
- **Pipeline (04-05)**: end-to-end corpus tests — call a registered primitive
  from vix source through surface→compile→lower→run; memo behavior per policy
  (Hermetic: second call is a memo hit with zero `begin` invocations;
  Volatile: two invocations, no memo entry); failure propagation; receipt
  population assertions; event/counter assertions (`effect_spawns`).
- **Chaos/replay**: killing between demands and re-demanding joins/reuses per
  existing scheduler test patterns.
- **Perf gate (06)**: corpus-next/budget benches before vs after across the
  stack; assert no regression on pure-op corpora (nothing we add is on those
  paths); measure primitive-call overhead (target: dominated by the demand
  machinery already paid by pure islands; conversion cost linear in value
  size).

## Phase / branch stack (git town)

1. `vix-prim-01-spec` — this document.
2. `vix-prim-02-core` — `runtime::primitive` module: descriptor/id/policy,
   trait + EffectCtx + ticket + completion, PrimitiveSet, register_function
   adapter, taxon→vir validator, facet↔FramedNode conversion. Pure unit tests;
   no compiler/scheduler wiring yet (accepting `machine.conv.wired-not-just-built`:
   the stack is one deliverable; core merges only as part of the wired stack).
3. `vix-prim-03-compiler` — manifest into ModuleContext, signature seeding,
   where-call generalization, `Op::EffectRequest`, EffectKind/canonical_node,
   diagnostics.
4. `vix-prim-04-lowering` — partition effect edges, island `effect_inputs`,
   LoweringArtifact effect bindings.
5. `vix-prim-05-scheduler` — effect resolution in evaluate (shared-lane
   extraction first), memo policy, receipts, events/counters, failure
   generalization.
6. `vix-prim-06-e2e` — first real registered primitive (data-in/data-out,
   e.g. `probe_version where { text }` → Version record), corpus e2e tests,
   perf gate report, docs page under vix/docs.

Subagent routing: mechanical implementation on `gpt-5.5-think-deeper`
(schema-less workflow agents; deliverable = edits + text report), medium
judgment on `opus`, design/review verdicts stay with the orchestrator.
