# Picante Semantics Specification

Picante is an incremental query runtime for Rust: you declare **inputs** and **derived queries**, and the runtime memoizes query results, tracks dependencies automatically, and recomputes only when the dependencies’ values change.

This document specifies **observable semantics**: what a user of the API can rely on (values, errors, and visibility across revisions/snapshots). It intentionally avoids prescribing implementation techniques, data structures, async primitives, logging/tracing backends, or performance tradeoffs.

---

## Model and Terms

### Walkthrough (non-normative)

This short example illustrates how inputs, derived queries, revisions, and snapshots relate.

Definitions:

```rust
use picante::PicanteResult;
use std::path::PathBuf;
use std::sync::Arc;

#[picante::input]
pub struct FileDigest {
    #[key]
    pub path: PathBuf,
    pub hash: [u8; 32],
}

#[picante::tracked]
pub async fn read_file_bytes<DB: DatabaseTrait>(
    _db: &DB,
    path: PathBuf,
    hash: [u8; 32],
) -> PicanteResult<Arc<Vec<u8>>> {
    // Read the file at `path` and return its bytes.
    // `hash` is part of the query key: changing it forces a different cached entry.
    # let _ = (path, hash);
    Ok(Arc::new(vec![]))
}
```

Timeline (conceptual):

1. Start with a new database view at some initial revision.
2. Set `FileDigest { path: "a.txt", hash: H1 }`. This advances the view to a later revision (because observable input state changed).
3. Call `read_file_bytes(db, "a.txt", H1)`. The runtime computes it once and caches the result.
4. If you call `read_file_bytes(db, "a.txt", H1)` again at a later time *without changing inputs*, the cached value may be returned.
5. When the file changes, update `FileDigest { path: "a.txt", hash: H2 }`. This advances the revision again.
6. A new call `read_file_bytes(db, "a.txt", H2)` uses a different key and is computed/cached independently of the `H1` entry.
7. If you take a snapshot after step 5, the snapshot view “freezes” `FileDigest("a.txt") == H2` and continues to observe that value even if the primary view is later mutated.

### Concurrency model (high level)

This specification uses the standard notion of **linearizability** for concurrent operations.
Informally: every operation behaves as if it took effect at a single instant between when it started and when it returned.

> r[concurrency.linearizable]
> All observable operations on a view (input reads, input mutations, snapshot creation, and derived-query accesses) MUST be linearizable:
>
> - For each operation call, there MUST exist a single linearization point between invocation and completion.
> - There MUST exist a total order of all completed operations consistent with real-time ordering (if A completes before B starts, then A appears before B in that order).
> - Each operation’s result MUST be the same as if operations executed sequentially in that total order.
>
> This requirement forbids “torn” observations: callers MUST NOT observe partial application of an operation.

### Database and views

A **database** is the logical unit of Picante state you care about: a set of inputs and derived-query semantics, plus (optionally) any snapshots derived from it.

A **view** is a particular handle you run queries against and apply mutations to. There are two common view kinds:

- the **primary view** (the `Database` value you constructed with `Database::new()`), and
- **snapshot views** (values like `DatabaseSnapshot` created from a view).

This is not “per task” or “per thread”; it is “per database/view object”.

#### Multiple databases and views in one program (non-normative)

You can have multiple independent Picante databases in a single Rust program by constructing multiple primary views:

```rust
#[picante::db(inputs(Item), interned(Label), tracked(item_length))]
pub struct Database {}

let db_a = Database::new(); // database A, primary view
let db_b = Database::new(); // database B, primary view (independent from A)
```

Each `Database::new()` creates a new database with independent observable state.
Operationally, this means mutations in `db_a` are not observable through `db_b`, and vice versa.
By contrast, cloning a shared reference to the same view (e.g. `Arc<Database>`) does not create a new database or a new view; it is still the same database handle.

You create additional views *of the same database* by taking snapshots:

```rust
let snap_a1 = DatabaseSnapshot::from_database(&db_a).await; // database A, snapshot view 1
let snap_a2 = DatabaseSnapshot::from_database(&db_a).await; // database A, snapshot view 2
```

Snapshots are separate views (they have their own memo tables and evolve independently), but they remain views of the same database.

### Revision

r[revision.type]
A **revision** is an opaque token that identifies a database state within a view.

r[revision.order]
Within a view, revisions MUST form a total order consistent with the “happens-after” ordering of successful input mutations that change observable state in that view.

r[revision.advance]
Any successful input mutation that changes observable input state MUST advance the view to a fresh revision that is greater than the prior revision.

> r[revision.choose]
> Each read or derived-query access on a view MUST be associated with a specific revision `R` of that view.
> The revision `R` is the view’s current revision at the operation’s linearization point (see `r[concurrency.linearizable]`).

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

Here, the key is `id: u32` (e.g. `ItemKey(1)` addresses the `id == 1` record).

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

For multiple parameters after `db`, the key is the tuple of all arguments in order:

```rust
#[picante::tracked]
pub async fn item_has_label<DB: DatabaseTrait>(
    db: &DB,
    item: Item,
    label: Label,
) -> PicanteResult<bool> {
    // ...
    # let _ = (db, item, label);
    # Ok(false)
}
```

Here, the key is `(item, label)`.
This “argument tuple” rule means you can control key shape by choosing your function signature.
If you want a single structured key rather than a multi-arg tuple, wrap it in a newtype:

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ItemLabelKey {
    pub item: Item,
    pub label: Label,
}

#[picante::tracked]
pub async fn item_has_label2<DB: DatabaseTrait>(
    db: &DB,
    key: ItemLabelKey,
) -> PicanteResult<bool> {
    # let _ = (db, key);
    Ok(false)
}
```

In this second form, the key is `(ItemLabelKey,)`.

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

An intern handle is a stable identity token: you can store it, pass it between queries, and use it as part of derived-query keys (as in the tracked example above).

In this specification, record identity is always the pair `(kind, key)`.

r[kind.identity]
A kind MUST uniquely identify a specific ingredient definition within a database type.
Two distinct ingredient definitions MUST NOT share the same kind.

r[kind.mapping]
The mapping from Rust constructs (types/functions) to kinds is implementation-defined, but it MUST be deterministic within a single view and MUST preserve `r[kind.identity]`.

---

# Core Semantics

## Inputs

### API direction (Rust, non-normative)

This section is intentionally Rust-centric and describes a plausible *user-facing* API shape.
It is non-normative: implementations may expose different surface APIs, but the observable semantics are defined by the requirements below.

#### Typed keys

For keyed inputs, user code benefits from a distinct key type that:

- is cheap to copy/clone and hash,
- is unambiguous at call sites (you can’t accidentally pass a `u32` key for the wrong record type),
- carries a link to the record type it addresses.

One ergonomic pattern is to generate a `{Name}Key` type for each `#[picante::input]` kind:

```rust
#[picante::input]
pub struct Item {
    #[key]
    pub id: u32,
    pub value: String,
}

/// Generated (illustrative).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemKey(pub u32);
```

Singleton inputs can use a unit-like key (generated or implicit):

```rust
#[picante::input]
pub struct Config {
    pub debug: bool,
}

/// Generated (illustrative).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConfigKey;
```

In addition, inputs often benefit from a “data-only” type (no key fields) to represent the non-key portion of an input record:

```rust
/// Generated (illustrative).
pub struct ItemData {
    pub value: String,
}

/// Generated (illustrative).
pub struct ConfigData {
    pub debug: bool,
}
```

This enables APIs like `db.get(ItemKey(1)) -> Option<Arc<ItemData>>` without repeating field names or allocating temporary structs.

#### Core operations

At the database level, one goal is to make the mutation/read operations feel like “normal Rust data access”:

- `db.set(record)` for inserts/updates
- `db.get(key)` to read
- `db.remove(key)` to delete

An illustrative shape (not required) looks like:

```rust
pub trait InputKey: Copy + Eq + std::hash::Hash + Send + Sync + 'static {
    /// The input record type addressed by this key.
    type Record: InputRecord<Key = Self>;
}

pub trait InputRecord: Send + Sync + 'static {
    type Key: InputKey<Record = Self>;
    type Data: Send + Sync + 'static;

    fn key(&self) -> Self::Key;
    fn into_data(self) -> Self::Data;
}

pub trait DbInputs {
    fn get<K: InputKey>(&self, key: K) -> PicanteResult<Option<std::sync::Arc<<K::Record as InputRecord>::Data>>>;
    fn set<R: InputRecord>(&self, record: R) -> PicanteResult<Revision>;
    fn remove<K: InputKey>(&self, key: K) -> PicanteResult<Revision>;
}
```

This keeps `set` “obvious” (the record value carries its own key) while ensuring `get/remove` are type-safe (you must provide the correct key type).
Implementations may also offer convenience methods on the generated types (e.g. `Item::set(&db, ...)`, `ItemKey::get(&db)`) layered on top of the same semantics.

For singleton inputs, `set` can remain “obvious” while `get/remove` stay typed:

```rust
db.set(Config { debug: true })?;
let config: Option<std::sync::Arc<ConfigData>> = db.get(ConfigKey)?;
```

> r[input.get]
> Reading an input record by key MUST:
>
> - Return `Ok(Some(value))` if the record exists in the database state associated with `db`.
> - Return `Ok(None)` if the record does not exist in that database state.
>
> If called during derived query evaluation, it MUST record a dependency on that input record (see `r[dep.recording]`).

### Equality and change detection

Picante’s observable semantics depend on a notion of when a value “changed”:

- input `set` is a no-op iff the new value is equal to the old value, and
- derived recomputation applies early-cutoff iff the new value is equal to the old value.

To be implementable and predictable, this specification requires implementations to use a well-defined equality relation.

> r[equality.relation]
> For each input kind and derived kind, the implementation MUST define an equality relation `≈` over that kind’s stored values such that `≈` is an equivalence relation (reflexive, symmetric, transitive).
>
> The relation MUST be deterministic: for any two values `a` and `b`, whether `a ≈ b` holds MUST NOT depend on timing, concurrency, global mutable state, or external state.
>
> The specific relation is otherwise implementation-defined (e.g., deep structural equality), but it MUST be consistent for the lifetime of the process.

> r[input.set]
> Setting an input record MUST behave as follows:
>
> 1. If the record did not previously exist, it is created with the provided value.
> 2. If the record exists and the new value is equal to the current value (per `r[equality.relation]`), the operation MUST be a no-op.
> 3. If the record exists and the value differs from the current value, the value MUST be replaced.
> 4. The view revision MUST advance to a fresh later revision iff the operation is not a no-op.

> r[input.remove]
> Removing an input record MUST behave as follows:
>
> 1. If the record does not exist, the operation MUST be a no-op.
> 2. If the record exists, it MUST be removed and the view revision MUST advance to a fresh later revision.

### Batch mutations

Many real workloads update multiple inputs together (e.g., a filesystem scan updating digests for many paths).

An ergonomic direction is to provide a batch builder that accepts typed operations and commits them together:

```rust
pub enum Mutation {
    SetItem(Item),
    RemoveItem(ItemKey),
    // ... more generated variants, or a type-erased alternative ...
}

pub trait DbBatch {
    type Batch;

    fn batch(&self) -> Self::Batch;
}

pub trait BatchOps {
    fn set_item(&mut self, item: Item);
    fn remove_item(&mut self, key: ItemKey);
    fn commit(self) -> PicanteResult<Revision>;
}
```

The exact shape (builder vs. iterator vs. transactional closure) is up to the implementation; the semantics are captured by `r[input.batch]`.

> r[input.batch]
> If an implementation provides a batch input-mutation operation (i.e., an API that applies multiple `set`/`remove` mutations as one operation), it MUST be atomic with respect to observable database state:
>
> - There MUST exist a single revision boundary such that observers see either all batch mutations applied or none.
> - No observer MAY observe a state in which only a strict subset of the batch mutations have been applied.
> - The batch MUST have the same final effect as applying its component mutations sequentially in the order provided, and then committing the resulting state atomically at the batch’s revision boundary.
> - The view MUST advance to a fresh later revision iff at least one mutation in the batch changes observable input state; otherwise the batch is a no-op.

## Derived queries

### Revision binding (per access)

Derived queries are always evaluated “at” a revision of a view (even if the evaluation is asynchronous and takes time).

> r[derived.revision-binding]
> A derived-query access MUST be evaluated at its associated revision `R` (see `r[revision.choose]`):
>
> - All input reads and derived-query reads performed as part of that access MUST behave as reads at revision `R`.
> - If the view advances to a later revision while evaluation is in progress, that MUST NOT change the results of the in-progress access; it remains an evaluation at revision `R`.
> - A later access to the same derived query at a later associated revision `R' > R` is a distinct access and MAY yield a different result.

### Determinism contract

Picante’s caching and snapshot semantics are defined in terms of dependencies that flow through the database.
In practice, derived queries should behave like deterministic functions of the database records they read; otherwise, cached results can be surprising or stale with respect to the outside world.

Note: Picante only tracks dependencies that flow through the database (inputs/derived queries/interned IDs). External state (filesystem contents, network responses, environment variables, clocks, etc.) is not automatically tracked or snapshotted. If a derived query reads external state without routing it through inputs, caching and snapshots can return values that do not reflect changes in that external state until some input change causes recomputation.

### Modeling external state (non-normative)

A common way to use Picante in tools that must read from disk is to make “what files exist and what version of each file exists” an explicit part of database state, and then key disk-reading queries by that version.

For example, treat the filesystem’s *current digest* as an input, updated by a watcher or scanner:

```rust
#[picante::input]
pub struct FileDigest {
    #[key]
    pub path: std::path::PathBuf,
    pub hash: [u8; 32],
}
```

Then make disk reads a derived query keyed by `(path, hash)` so the cached result is specific to a particular content hash:

```rust
#[picante::tracked]
pub async fn read_file<DB: DatabaseTrait>(
    _db: &DB,
    path: std::path::PathBuf,
    hash: [u8; 32],
) -> picante::PicanteResult<std::sync::Arc<Vec<u8>>> {
    // Read from disk here; `hash` is part of the key so changing it forces a new cached entry.
    // If you care about correctness in the presence of races (e.g. a stale watcher digest or
    // TOCTOU), validate that the bytes you read actually match `hash` and return an error if not.
    let bytes: Vec<u8> = /* read bytes for `path` */;
    let _expected: [u8; 32] = hash;
    Ok(std::sync::Arc::new(bytes))
}
```

Higher-level queries can then depend on `FileDigest(path)` to obtain the current `(path, hash)` pair and call `read_file(path, hash)`.
When the file changes, updating `FileDigest` causes downstream recomputation; if a file reverts to a previous hash, the `(path, hash)`-keyed cache allows reuse.

### Dependency tracking

> r[dep.recording]
> During evaluation of a derived query, each read of an input record or derived query result MUST be recorded as a dependency of the evaluating query for the purpose of future revalidation.
>
> Dependencies MUST be recorded with enough precision to revalidate: at minimum, the dependency’s `(kind, key)` identity.
>
> Implementations MAY omit recording dependencies on records whose values are immutable for the lifetime of the process (for example, interned records addressed by an ID), since such dependencies can never affect revalidation.

### Cell state and visibility

Each derived query `(kind, key)` conceptually has a memo entry (“cell”) with:

- the last computed value (if any),
- a dependency set (from the last successful computation),
- `verified_at`: the most recent revision at which the cached value was confirmed valid with respect to its recorded dependencies,
- `changed_at`: the most recent revision at which the cached value changed (per `r[equality.relation]`).

r[revision.early-cutoff]
When a derived query is recomputed at revision `R` and produces a value equal to the previously cached value (per `r[equality.relation]`), `changed_at` MUST NOT advance (it remains the prior `changed_at`), while `verified_at` MUST advance to `R`.

### Revalidation

> r[cell.revalidate]
> When accessing a cached derived value at revision `R`, if the cached value is not known-valid at `R`, the runtime MUST revalidate it by checking the stored dependencies:
>
> - If every dependency’s `changed_at` is `<=` the cached value’s `verified_at`, revalidation succeeds and the cached value MUST be returned (with `verified_at` updated to `R`).
> - Otherwise, revalidation fails and the query MUST be recomputed.

r[cell.revalidate-missing]
If a dependency’s ingredient is not available (e.g., the kind is not registered in the current database), revalidation MUST fail and recomputation MUST be attempted.

### Errors and poisoning

> r[cell.poison]
> If computation fails (returns an error or panics), the runtime MUST record a failure for that `(kind, key, revision)` such that:
>
> - Subsequent accesses at the same revision return the same error without rerunning the computation.
> - After the revision advances due to an input change, a new access MAY attempt recomputation.

### Cancellation (dropped evaluations)

In Rust async, a computation can be *cancelled* by dropping its future. Cancellation is not the same as “the query returned an error”: it is an absence of a result.

> r[derived.cancel]
> If an in-progress derived-query evaluation at revision `R` is cancelled before it produces a successful value or an error (e.g., the future is dropped), the implementation MUST treat it as not having produced a result:
>
> - It MUST NOT update the cached value for `(kind, key)` based on the cancelled evaluation.
> - It MUST NOT record a failure for revision `R` solely due to cancellation.
> - A subsequent access at the same revision `R` MAY retry evaluation and complete successfully or with an error.
>
> Cancellation of one caller MUST NOT force other concurrent callers to observe a cancellation error; other callers MAY still obtain a normal value or error according to the semantics above.

## Invalidation semantics

> r[dep.invalidation]
> Whenever an input record changes at revision `R`, any derived query whose dependency set includes that input record MUST be treated as stale for revisions `>= R`.
>
> Staleness is a logical property: implementations MAY propagate invalidation eagerly or lazily, but MUST ensure the revalidation rules above are upheld.

## Cycles

> r[cycle.detect]
> If, within a single logical derived-query evaluation (i.e., along one evaluation stack in one view), evaluation would (directly or indirectly) require evaluating the same `(kind, key)` again at the same revision, the runtime MUST report a dependency cycle error rather than deadlocking or waiting indefinitely.
>
> This requirement does not mandate detection of cross-task/cross-evaluation cycles. (Those may still deadlock and are considered a usage hazard; see `r[sharing.nonobservable]` for the “sharing is non-observable” boundary.)

---

# Snapshots

A **snapshot** is a fork of a database’s state at a single revision, with isolated subsequent mutations.

> r[snapshot.creation]
> Creating a snapshot MUST bind it to a single revision `R` of the source database such that:
>
> - For every input record, reads from the snapshot behave exactly as reads from the source database at revision `R`.
> - For derived queries, evaluations performed against the snapshot MUST be consistent with the snapshot’s view of inputs and the derived-query semantics in this document.

> r[snapshot.linearizable]
> Snapshot creation MUST be linearizable with respect to input mutations on the source view:
>
> - There MUST exist a single revision `R` between invocation and completion of snapshot creation such that the snapshot is bound to `R`.
> - Any input mutation that completes before `R` MUST be visible in the snapshot.
> - Any input mutation that completes after `R` MUST NOT be visible in the snapshot.
>
> In particular, the snapshot MUST NOT observe a “torn” state that could not exist at any single revision (i.e., different input records MUST NOT reflect different revisions).

r[snapshot.isolation]
After snapshot creation, subsequent input changes in the source database MUST NOT be visible in the snapshot, and subsequent input changes in the snapshot MUST NOT be visible in the source database.

r[snapshot.memo-isolation]
Derived query memoization performed by the snapshot MUST be isolated from the source database: caching a derived value in one MUST NOT mutate the other’s memo tables.

r[snapshot.interned]
Interned ingredients are exempt from snapshot isolation: the intern table is append-only and is shared across all views of a database. Newly interned values become visible to both the source view and snapshot views.

---

# Sharing optimizations (non-observable)

Within a database, implementations MAY share work across views (e.g., coalescing concurrent evaluations of the same derived query at the same revision, or adopting completed results) as an optimization.

r[sharing.nonobservable]
Such sharing MUST NOT change observable behavior: the values and errors returned MUST be indistinguishable from a correct, non-sharing implementation that evaluates each view independently under the semantics above.
