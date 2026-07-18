+++
title = "data-driven primitives: the vix ↔ rust interface, end-state"
+++

> **STATUS: TARGET. This is the north star the `RequestShape` work is walking
> toward, not a description of today.** As of the `vix-primitive-request-shape`
> stack, `fetch`/`observe` lower through a `RequestShape` value and primitive
> dispatch is name→`PrimitiveKind` data, but the type mirrors are still
> hand-written, the projection still lives in a central table, and the surface
> types are not nameable in vix. This note pins where we are going so each PR in
> the arc can be checked against a fixed goal.

## The one-sentence goal

**Adding or changing a vix primitive is one declarative edit in Rust, next to
the primitive's `impl`, and everything else — runtime dispatch, compiler
lowering, the surface type names, its namespaced path, and the ability to
construct those types from vix source — is derived from that one declaration.
Zero compiler arms, zero hand-synced type mirrors, no closed `PrimitiveKind`
enum. Primitives live in namespaces derived from their `PrimitiveId`, and a
curated prelude — itself a vix module of re-exports — is auto-imported into every
module, exactly like Rust's `std::prelude`. The runtime is generic over an
embedder-chosen context of shared authorities (an executor, a pool), each
primitive declaring the slice it needs via `FromRef`, the way a backend
framework shares one DB pool across handlers.**

The classification rule from [registered primitives](/registered-primitives)
still gates *what earns a primitive* (it must cross an authority boundary). This
note is about the *mechanism* once something has earned one: how that primitive
projects onto the vix surface without bespoke compiler code.

## What's wrong today: one primitive, three hand-synced spellings

A single primitive like `fetch` is described in three places that must agree
field-for-field, kept in lockstep by a human:

1. **The Rust request struct** — `PinnedFetchRequest { pin: PinnedBlobRef }` in
   `runtime/fetch_primitive.rs`, `#[derive(facet::Facet)]`. This is what the
   runtime actually executes against.
2. **A hand-written `Type` mirror** — `pinned_fetch_request_type()`,
   `origin_hint_type()`, `pinned_blob_ref_type()`, `observe_request_type()`,
   each a `Type::Record(...)` that re-spells struct #1 field-by-field. This is
   what the compiler builds requests *of*, and what `PrimitiveDescriptor`'s
   `schema_ref()` patterns point back into.
3. **The projection onto the surface** — `binding::RequestShape` (per-argument
   `ArgRole`s → how surface arguments fold into #2's record) plus the central
   `request_shape(kind)` / `builtin_bindings()` tables in `binding.rs`.

`RequestShape` already collapsed the *bespoke-compiler-arm* half of #3 into
data — `fetch` and `observe` no longer have a hand-written arm in
`lower_effect_intrinsic`. But #1 and #2 are still two spellings of the same
shape, #2 has no vix-nameable surface, and #3's mapping table is a second place
that names every primitive.

The duplication is not incidental: `facet::Facet` already gives full reflection
over struct #1, so #2 is *derivable* and only exists because we hand-wrote it
first.

## End-state: one source per fact

### 1. The struct shape *is* the schema

Derive `Type` / `schema_ref` from the `Facet` shape of the request struct and
**delete** every hand-written `*_request_type()` / `*_hint_type()` /
`*_ref_type()` constructor in `runtime/`. `RequestShape.request_ty` and
`PrimitiveDescriptor.request_schema` then read the *same* derived type — #1 and
#2 become one. The struct is the single source; the schema is a projection of
it, not a parallel artifact.

### 2. The primitive declares its own projection

The `(surface name, placement, arg roles, result type, selectors)` tuple that
`binding.rs` holds centrally moves to live *with the primitive* — a method or
associated const on the `Primitive` trait, alongside `descriptor()`. The
`BindingRegistry` is then **harvested** from the set of registered primitives
rather than maintained as a second table. `request_shape(kind)` stops being a
`match` over a closed enum and becomes "ask the registered primitive for its
shape."

Consequence: the closed `PrimitiveKind` enum, the `BUILTIN_TYPES` /
`builtin_module_item` string lists in `binder.rs`, the `== "fixture_tree"`
literal in `compiler.rs`, and the hand-registration in
`default_primitive_dispatcher()` all collapse into iteration over the registry.
Behavioural aliases (`refresh` over `observe`, `json_decode` over `decode`) stay
what they are today: ordinary vix functions over the single primitive, never new
primitives or intrinsics.

### 3. Facet type names are vix type names

Because the surface type is derived from a *named* Rust struct, `OriginHint` and
`PinnedBlobRef` become spellable in vix source. A vix program can then construct
a valid origin and drive `observe`/`fetch` directly — today `observe_binding`
notes there is "no way to spell a valid origin in vix yet," so these primitives
are Rust-only. This is the highest-value unlock in the arc: it turns the
primitives from Rust-invoked into genuinely vix-invoked.

### The end test

**You can add a new effectful primitive without touching `compiler.rs`,
`binder.rs`, or any table in `binding.rs`.** You write one Rust struct (Facet),
one `Primitive` impl carrying its descriptor and its projection, register it,
and the surface name, request lowering, runtime dispatch, and vix-nameable types
all follow. If a change to a primitive forces an edit to the compiler, the goal
is not yet met.

## Namespacing and the prelude: the Rust model

Primitives are **namespaced**, and a curated **prelude** is auto-imported into
every module — exactly like Rust. The important move is separating two things
today's `Placement` conflates:

- **where an item is *defined*** — always a real namespaced path; and
- **whether it is *auto-imported*** — a property of the prelude, not the item.

In Rust nothing is "born in the prelude": `Vec` is defined at `alloc::vec::Vec`,
and `std::prelude::v1` merely *re-exports* it; the compiler then injects
`use std::prelude::v1::*` into every module at the lowest resolution priority.
Vix follows this precisely.

### Every primitive lives at a real path, derived from its `PrimitiveId`

The surface module path is **derived from the `PrimitiveId.namespace` the
primitive already carries** — no separately-declared surface namespace to keep in
sync (that would reintroduce exactly the duplication this arc kills). The
mapping is mechanical: dotted namespace → `::`-path.

```
PrimitiveId { namespace: "vix.machine", name: "fetch", version: 1 }
        │
        ▼
    vix::machine::fetch        // its canonical, path-qualified surface name
```

So `Placement::Prelude` as a *definition site* disappears. Every binding's
placement is a `Module(ModulePath)`; the path is not hand-written but computed
from the primitive's identity. The runtime wire identity and the surface path
can no longer drift, because they are the same fact.

### The prelude is a vix module of re-exports

The prelude is written as **real vix source** — a module of re-exports — loaded
the way `crate::stdlib`'s `PRELUDE_FUNCTIONS` are loaded today, so it dogfoods
the module system rather than being a privileged Rust table:

```vix
// the auto-imported prelude, expressed in vix
pub use vix::machine::{fetch, observe};
pub use vix::fmt::{decode, try_decode};
// the behavioural aliases stay ordinary vix fns over the single primitive
pub fn refresh<Origin>(origin: Origin) -> Blob { observe(origin, Mode::Refresh) }
```

Prelude membership is therefore a line in *this* module, not a flag on the
binding. A primitive can be reachable at `vix::machine::observe` and *also*
re-exported unqualified by the prelude; a primitive can exist in a namespace and
**not** be in the prelude at all (reached only by path or `use`), which is the
whole point of namespacing.

This means the surface needs a genuine **re-export** notion (`pub use`) — a
binding target that points at another binding's canonical path — which
`BindingTarget` grows alongside `Primitive` and `VixFunction`.

### Auto-import and resolution priority

The binder injects a glob import of the prelude module into every vix module
(the mechanical equivalent of `use vix::prelude::*`). Prelude names resolve at
**lowest priority**: a local definition or an explicit `use` shadows a prelude
name, never the reverse — same rule Rust uses, and the same rule the binder
already applies for inner-scope shadowing. `is_prelude_name` stops being a
derived string set and becomes "is this name glob-imported from the prelude
module."

Everything not re-exported by the prelude is reached by its path
(`vix::machine::foo`) or an explicit `use vix::machine::foo` — the `caps::{Cc,
Ar, Rustc}` and `vix::{Int, …}` module items in `binder.rs` become instances of
this one mechanism rather than bespoke `builtin_module_item` arms.

### Not yet: prelude versioning

Rust's prelude is edition-versioned (`prelude::v1`, `prelude::rust_2021`, …).
Vix has no editions concept yet and this is very early days, so the prelude is a
single unversioned module. The structure (a named prelude module the binder
glob-imports) leaves room to key it by edition later without reworking the
model — but no edition-selection machinery gets built now.

## Runtime context: shared authorities, generically

Primitives need **shared, runtime-installed services** — an async executor, a
connection pool, an HTTP client, a cache — reused across every invocation, the
way a backend framework shares one DB pool across all its handlers. Today this
exists as `runtime::PrimitiveServices`: a `struct` with a *closed, hardcoded* set
of `Option<Arc<dyn Trait>>` fields (`value_persistence`, `origin`,
`claim_history`, `fixture_store`), installed via `set_primitive_services` and
hand-wired into the scheduler. Adding one shared service means editing that
struct **and** the scheduler — the exact closed-shape anti-pattern this arc
exists to kill.

### The model: `Runtime<Ctx>` with `FromRef` slices

The runtime becomes **generic over one application context** `Ctx`, chosen by the
embedder, threaded into every primitive's `begin`. Each primitive declares the
*slice* of that context it needs via a `FromRef<Ctx>` bound — axum's `State<S>` /
`FromRef` model — so a primitive that needs a `PgPool` names exactly that, and a
missing dependency is a **compile error at wiring time**, not a runtime failure.

```rust
trait Primitive<Ctx> {
    fn descriptor(&self) -> &PrimitiveDescriptor;
    // `app` is the whole context; the impl projects the slice it needs.
    fn begin(&self, request: ValueId, ctx: EffectCtx, app: &Ctx) -> EffectTicket;
}

// each impl pulls exactly its slice, checked at compile time:
impl<Ctx> Primitive<Ctx> for DbPrimitive
where PgPool: FromRef<Ctx> {
    fn begin(&self, request: ValueId, ctx: EffectCtx, app: &Ctx) -> EffectTicket {
        let pool = PgPool::from_ref(app);   // static, no downcast, no Option
        ...
    }
}
```

The registry / dispatcher are then generic in the *single* root `Ctx`
(`PrimitiveDispatcher<Ctx>`, `Runtime<S, Ctx>`); the trait object stays
`dyn Primitive<Ctx>` — monomorphic in `Ctx`, so it is **object-safe**. Deps must
*not* appear as an associated type in the method signature (that would break the
`dyn Primitive` registry); instead each impl projects the slice it needs out of
`&Ctx` via `FromRef`, and the `where Slice: FromRef<Ctx>` bound is what makes a
missing dependency a compile error.
Built-in primitives that need nothing from the embedder (`fetch`, `observe`) are
agnostic over `Ctx` (`type Deps = ()`), so a bare runtime still works with
`Ctx = ()`. This retires `PrimitiveServices` as a hardcoded struct — its four
fields become ordinary members of whatever `Ctx` an embedder assembles, reached
by `FromRef` like any other slice.

### The hard invariant: context is ambient authority, never a semantic input

This axis is **orthogonal** to the data-driven-request axis, and must stay so.
The request value remains the *sole* source of a primitive's identity,
admissibility, memoization, and receipts — the rule `PrimitiveServices` already
documents: "authorities, not semantic inputs." The context supplies only *how*
an effect is carried out (which executor runs the I/O, which pool serves the
query), never *what* it is or whether two invocations are the same demand. If a
value can change identity or admissibility it belongs in the request record (and
thus in the `RequestShape`), not in `Ctx`. This is what keeps deterministic
replay sound while the executor, pool, and clients vary between runs.

## The hard seams (must be answered, not glossed)

These are the parts where "everything is data" costs real design work. The
target includes them; the order defers them behind their blockers.

- **`decode` / `try_decode`** need three things a plain record shape can't
  express: a request field folded from a *compile-time constant* (the `Format`
  tag), a target type *derived from the expected type* at the call site, and
  *const-folding through monomorphized vix wrappers*. The end-state generalizes
  `ArgRole`: today's `Selector` (enum → boolean flag) widens to an
  `ArgRole::Const` (enum/int → arbitrary constant field), and an
  `ArgRole::ResultType` carries the expected-type-derived target. This is gated
  on the const-fold-through-wrappers capability that #2500 defers; until it
  lands, `decode` stays hand-lowered and `request_shape` returns `None` for it.
- **`fixture_tree` / `fixture_registry` / `untar`** lower to *dedicated VIR ops*,
  not `InvokePrimitive` — there is no request record to shape. The end-state
  gives `RequestShape` an **op-backend**: a shape whose target is a VIR op
  rather than a `PrimitiveId`, so their projection is data even though their
  lowering is an op. (The alternative — promoting them to real primitives — was
  considered and rejected: they don't cross an authority boundary the way
  `registered-primitives` requires.)

## Explicit non-goals

- **Data-driving `ExternKind`.** `Blob` / `Tree` / `Registry` is a closed enum in
  `vir.rs`. Making the *set of extern types* open/data-driven is a large lift
  with little payoff — the extern kinds are a fixed vocabulary of the machine,
  not a place primitives get added. It stays closed.
- **Method surfaces** (`.text()`, `.len()`, …) are a separate binding surface
  from free-function placement and are out of scope for this arc.

## Migration order

Tracks the workstreams in the arc; each is a clean PR against the fixed goal
above.

1. **Type-schema unification.** Derive `RequestShape.request_ty` and
   `PrimitiveDescriptor` from one Facet-reflected schema; delete the paired
   `*_type()` constructors. (Collapses #1 ↔ #2.)
2. **Nameable surface types.** Make `OriginHint` / `PinnedBlobRef` spellable in
   vix, so observe/fetch run from vix source, not just Rust. (Highest value.)
3. **Projection moves onto the primitive; registry is harvested.** Kill the
   central `request_shape` `match`, the `PrimitiveKind` enum, and the
   `binder.rs` / `compiler.rs` string lists.
4. **Namespacing + prelude-as-re-export module.** Derive each binding's module
   path from `PrimitiveId.namespace`; retire `Placement::Prelude` as a
   definition site; add a re-export (`pub use`) binding target; move the prelude
   into a vix module the binder glob-imports at lowest priority; collapse
   `builtin_module_item` / `BUILTIN_TYPES` into that one path-resolution
   mechanism.
5. **`fixture_*` / `untar` op-backend.** Give `RequestShape` a VIR-op target so
   these are data-projected.
6. **`decode` / `try_decode`.** After const-fold-through-wrappers (#2500) lands:
   `ArgRole::Const` + `ArgRole::ResultType`, retiring the last hand-lowered arm.

Independent of 1–6 (orthogonal axis, can land any time):

7. **Generic runtime context.** Make the runtime generic over an embedder
   context `Ctx`; give `Primitive` a `type Deps: FromRef<Ctx>` projected into
   `begin`; retire the hardcoded `PrimitiveServices` struct into ordinary `Ctx`
   members. Enforce the invariant that `Ctx` is ambient authority only — nothing
   in it feeds identity/memo/receipts.
