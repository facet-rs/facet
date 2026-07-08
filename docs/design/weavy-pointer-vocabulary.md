# Weavy Pointer Vocabulary

Status: design plus first proving slice. This document is the narrow pointer
vocabulary charter for machine-side access to value memory; it does not change
identity bytes, store dedupe, or write discipline.

## Charter

The hash-as-field Q1(b) charter names four primitives needed before persistent
machine-side collections can stop crossing the Rust host boundary:

1. a first-class node pointer or handle with declared provenance;
2. load/store through that node reference;
3. structural-share retain/release;
4. path-copy copy-on-write.

This slice implements only the read half of the first two primitives, with
native indexed reads from scalar arrays as the proving customer. Both frozen
store payloads and read-only molten array-word snapshots are visible to the
machine; retain/release, copy-on-write, and store-through remain design-only
until the write-side zero-padding and epoch canaries are in place.

## Reference Values

A Weavy reference is not an untyped machine address. It is a word-sized handle
whose provenance says which arena owns the bytes and which descriptor checks are
valid for accesses through it.

Current provenance classes:

- `StorePayload`: an immutable `ValueStore` payload. A non-negative vix store
  handle indexes the store table. The task lane receives a read-only memory
  table for the current burst; each entry is a `(ptr, len)` pair for one store
  payload.
- `MoltenSlot`: a mutable per-driver arena slot. The design treats it as a
  distinct provenance because uniqueness and write obligations differ. This
  slice exposes only immutable per-burst snapshots of molten array-word slots to
  native reads; it does not hand the JIT a stable mutable molten pointer.
- `Frame`: the existing task frame arena. Current `LoadIndexedI64` and
  `StoreIndexedI64` stay frame-relative and are not value-memory references.

A native access op must carry enough descriptor facts to reject the wrong
payload shape before reading. For array words, the op checks the payload kind,
the element schema ref, the dynamic index bounds, and the 8-byte word payload
layout before loading the element.

## Native Array Read

The first op is a read-only value-memory operation:

```text
LoadArrayWord {
  dst,
  present,
  array,
  index,
  elem_schema_ref,
}
```

Semantics:

- `array` and `index` are frame slots containing the array handle and requested
  index.
- the op succeeds for either a non-negative store handle or a negative molten
  handle whose payload table entry is an `Array<T>` word payload (`kind == 0`)
  with `elem_schema_ref` equal to the immediate baked by lowering;
- if the index is non-negative and less than the stored length, the op writes
  the element word to `dst` and `1` to `present`;
- otherwise it writes `0` to `dst` and `0` to `present`.

That gives `Array<Int>.get(i)` a machine-visible bounds check and element load.
The lowerer can consume the native option directly for `unwrap` in the scalar
hot path. Store-backed array arguments are charged to the projection read set
when the driver materializes the store value-memory table for a task containing
`LoadArrayWord`; the stencil itself stays a pure checked load. That records the
same whole-array observation the host path records, including arrays already
resident before the current execution window. Aggregate elements remain pinned
to the host fallback until their representation can be materialized without
hiding the read behind FFI again.

## Interpreter And JIT

The interpreter executes the same semantics directly against the read-only
memory table. It is slower, but it is the semantic authority.

The JIT emits the same operation as a copied-code stencil:

```text
store handle -> memory table entry -> base + 24 + index * 8 -> load i64
```

The tables are process-local and valid for the task burst. Store entries are
snapshotted as payload bytes, and molten array-word slots are encoded into the
same checked array-word payload shape. The op never owns the canonical value and
never extends its lifetime.

Projection receipts are driver-side, not stencil-side. Materializing a
store-backed scalar array argument into the native value-memory table records a
whole-argument projection read once for the burst. Molten snapshots do not
record store reads because they are execution-local values; if they were derived
from a store argument, the host operation that copied or mutated them already
records the whole input just like the pre-native host path.

Sync hosts that can mutate value-memory provenance use a yielding host-call op.
That keeps ordinary request-producing hosts on the old call-and-continue path,
but gives the driver a rebuild point after array allocation or mutation before a
native load executes.

## Safety Story

Reads preserve identity. The op never writes store or molten bytes, never bumps
refcounts, and never mutates `ValueStore::by_content`.

Store payload reads are safe because interned values are immutable. A store
handle names a frozen payload; the op validates that the payload is a word array
with the declared element schema before reading a word.

Molten payload reads use a separate provenance table. Molten slots can be unique
or shared, can be reallocated by host operations, and are the place where future
writes will have to enforce canonical zero padding. This slice does not pretend
a molten handle is a stable mutable pointer; it only snapshots array words for a
read-only burst.

Store-through is deferred. Any future `StoreArrayWord`/field store through a
value reference must obey the zero-padding law: fresh bytes are zeroed, narrowing
writes re-zero slack, and inactive enum payload bytes are cleared atomically
with variant switches. Until the epoch-2 padding canary is present, native
writes stay out of scope.

## Later Primitives

`retain/release` belongs to persistent structure sharing, not scalar reads. It
will need a shared arena node header and a rule for retaining immutable subtrees
without making HandleTier or refcount state observable in identity.

Path-copy COW belongs with the first native persistent collection. The copy path
must allocate a new molten node, copy the path from root to leaf, retain
unchanged children, and preserve identity fields for untouched subtrees.

Those primitives are required before the Q1(b) Merkle/B-tree can be truly
Weavy-native. They are not required for the scalar `Array<Int>.get(i)` proving
slice.
