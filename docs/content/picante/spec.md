# Picante Semantics Specification

Picante is an incremental query runtime for Rust: you declare **inputs** and **derived queries**, and the runtime memoizes query results, tracks dependencies automatically, and recomputes only when the dependencies’ values change.

This document specifies **observable semantics**: what a user of the API can rely on (values, errors, and visibility across revisions/snapshots). It intentionally avoids prescribing implementation techniques, data structures, async primitives, logging/tracing backends, or performance tradeoffs.

---

## Model and Terms

### Runtime instance and runtime family

- A **runtime instance** is one live in-memory execution of a database (or a snapshot) with its own revision counter and memo tables.
- A **runtime family** is a database plus any snapshots derived from it. Families are relevant only for allowed sharing optimizations; see “Sharing optimizations” below.

### Revision

r[revision.type]
A **revision** is an opaque token that identifies a database state within a runtime instance.

r[revision.order]
Within a runtime instance, revisions MUST form a total order consistent with the “happens-after” ordering of successful input mutations that change observable state.

r[revision.advance]
Any successful input mutation that changes observable input state MUST advance the runtime to a fresh revision that is greater than the prior revision.

### Ingredients and records

An **ingredient** is a category of stored/computed data. Picante defines three ingredient kinds:

- **Input ingredient**: mutable key-value storage set by user code.
- **Derived ingredient**: an async query whose value is computed from inputs and other derived queries.
- **Interned ingredient**: a value-to-ID mapping that is append-only (interned values never change once created).

Each ingredient stores **records** addressed by a **key**.

### Keys and kinds

Every record is addressed by:

- a **kind** identifying which ingredient it belongs to, and
- a **key** identifying the record within that ingredient.

Rust-centric intuition: a “kind” corresponds to a specific ingredient definition in your Rust program (an `#[picante::input]` type, an `#[picante::interned]` type, or an `#[picante::tracked]` function). Each kind defines its own disjoint keyspace.

#### Input

`#[picante::input]` defines a kind for input records.

Keyed inputs use the `#[key]` field value as the key:

```rust
#[picante::input]
pub struct Item {
    #[key]
    pub id: u32,
    pub value: String,
}
```

Here, the key is `id: u32` (e.g. `Item::new(&db, 1, ...)` addresses the `id == 1` record).

Singleton inputs (no `#[key]` field) are conceptually keyed by a unit-like key: there is exactly one record.

#### Tracked (derived query)

`#[picante::tracked]` defines a kind for derived-query results.
The key is the tuple of the function’s parameters after `db` (one parameter means a 1-tuple):

```rust
use picante::PicanteResult;

#[picante::tracked]
pub async fn item_length<DB: DatabaseTrait>(db: &DB, item: Item) -> PicanteResult<u64> {
    Ok(item.value(db)?.len() as u64)
}
```

Here, the key is `(item,)`, where `item` is the `Item` handle (its stable identity), not the mutable `ItemData` contents.

#### Interned

`#[picante::interned]` defines a kind for an append-only intern table.
Interned records are addressed by the intern ID handle:

```rust
#[picante::interned]
pub struct Label {
    pub text: String,
}
```

Creating an intern (`Label::new(&db, "tag".into())`) conceptually looks up or inserts by value and returns a `Label(pub InternId)`.
Reading an intern (e.g. `label.text(&db)`) uses that ID as the key.

r[key.equality]
Two uses of the same ingredient with equal key values MUST refer to the same record.
Distinct key values MUST NOT alias the same record.

r[key.hash]
Implementations MAY expose a stable diagnostic identifier for keys, but any such identifier MUST NOT be used as a correctness boundary.

r[kind.identity]
A kind MUST uniquely identify a specific ingredient definition within a database type.
Two distinct ingredient definitions MUST NOT share the same kind.

r[kind.mapping]
The mapping from Rust constructs (types/functions) to kinds is implementation-defined, but it MUST be deterministic within a single runtime instance and MUST preserve `r[kind.identity]`.

---

# Core Semantics

## Inputs

r[input.get]
Reading an input record at `(kind, key)` at revision `R` MUST return the value that was most recently written at or before `R`, or `None` if the record does not exist at `R`.

> r[input.set]
> Setting an input record MUST behave as follows:
>
> 1. If the record did not previously exist, it is created with the provided value.
> 2. If the record exists and the value is byte-for-byte / structural-equality equal to the current value, the operation MUST be a no-op.
> 3. If the record exists and the value differs from the current value, the value MUST be replaced.
> 4. The runtime revision MUST advance to a fresh later revision iff the operation is not a no-op.

> r[input.remove]
> Removing an input record MUST behave as follows:
>
> 1. If the record does not exist, the operation MUST be a no-op.
> 2. If the record exists, it MUST be removed and the runtime revision MUST advance to a fresh later revision.

## Derived queries

### Determinism contract

r[derived.determinism]
For Picante’s caching semantics to be meaningful, derived query computations SHOULD be observationally pure with respect to the database state they read: the returned value SHOULD be a deterministic function of the values of the records they depend on.

Note: If a derived query reads external state without routing that state through an input ingredient (e.g., reading a file directly), caching can return values that do not reflect changes in that external state until some input change causes recomputation.

### Dependency tracking

r[dep.recording]
During evaluation of a derived query, each read of an input record or derived query result MUST be recorded as a dependency of the evaluating query for the purpose of future revalidation.

Dependencies MUST be recorded with enough precision to revalidate: at minimum, the dependency’s `(kind, key)` identity.

### Cell state and visibility

Each derived query `(kind, key)` conceptually has a memo entry (“cell”) with:

- the last computed value (if any),
- a dependency set (from the last successful computation),
- `verified_at`: the revision at which the cached value was last confirmed valid,
- `changed_at`: the revision at which the cached value last changed.

r[revision.early-cutoff]
When a derived query is recomputed at revision `R` and produces a value equal to the previously cached value, `changed_at` MUST NOT advance (it remains the prior `changed_at`), while `verified_at` MUST advance to `R`.

### Revalidation

r[cell.revalidate]
When accessing a cached derived value at revision `R`, if the cached value is not known-valid at `R`, the runtime MUST revalidate it by checking the stored dependencies:

- If every dependency’s `changed_at` is `<=` the cached value’s `changed_at`, revalidation succeeds and the cached value MUST be returned (with `verified_at` updated to `R`).
- Otherwise, revalidation fails and the query MUST be recomputed.

r[cell.revalidate-missing]
If a dependency’s ingredient is not available (e.g., the kind is not registered in the current database), revalidation MUST fail and recomputation MUST be attempted.

### Errors and poisoning

r[cell.poison]
If computation fails (returns an error or panics), the runtime MUST record a failure for that `(kind, key, revision)` such that:

- Subsequent accesses at the same revision return the same error without rerunning the computation.
- After the revision advances due to an input change, a new access MAY attempt recomputation.

## Invalidation semantics

r[dep.invalidation]
Whenever an input record changes at revision `R`, any derived query whose dependency set includes that input record MUST be treated as stale for revisions `>= R`.

Staleness is a logical property: implementations MAY propagate invalidation eagerly or lazily, but MUST ensure the revalidation rules above are upheld.

## Cycles

r[cycle.detect]
If derived query evaluation would (directly or indirectly) require evaluating the same `(kind, key)` again at the same revision, the runtime MUST report a dependency cycle error rather than deadlocking or waiting indefinitely.

This requirement is semantic: it constrains observable behavior (an error must be produced) without requiring a specific detection mechanism.

---

# Snapshots

A **snapshot** is a fork of a database’s state at a single revision, with isolated subsequent mutations.

r[snapshot.creation]
Creating a snapshot MUST bind it to a single revision `R` of the source database such that:

- For every input record, reads from the snapshot behave exactly as reads from the source database at revision `R`.
- For derived queries, the snapshot MAY start with empty memo tables or with a copy of memoized values from the source, but in all cases results MUST be consistent with the snapshot’s view of inputs and the revalidation rules.

r[snapshot.isolation]
After snapshot creation, subsequent input changes in the source database MUST NOT be visible in the snapshot, and subsequent input changes in the snapshot MUST NOT be visible in the source database.

r[snapshot.memo-isolation]
Derived query memoization performed by the snapshot MUST be isolated from the source database: caching a derived value in one MUST NOT mutate the other’s memo tables.

r[snapshot.interned]
Interned ingredients are exempt from snapshot isolation: the intern table is append-only and MAY be shared across a runtime family. Newly interned values MAY become visible to both the source database and snapshots.

---

# Sharing optimizations (non-observable)

Within a runtime family, implementations MAY share work across runtime instances (e.g., coalescing concurrent evaluations of the same derived query at the same revision, or adopting completed results) as an optimization.

r[sharing.nonobservable]
Such sharing MUST NOT change observable behavior: the values and errors returned MUST be indistinguishable from a correct, non-sharing implementation that evaluates each runtime instance independently under the semantics above.
