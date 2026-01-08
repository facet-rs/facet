# Picante Specification

## Introduction

Picante is an async incremental query runtime for Rust, providing automatic dependency tracking, memoization, and cache invalidation for Tokio-first async pipelines.

This specification defines the behavior of:
- **Core concepts**: revisions, keys, dependency tracking
- **Ingredients**: input, derived (tracked), and interned storage types
- **Runtime**: revision management, dependency graphs, event notification
- **Execution frames**: task-local dependency recording
- **Derived query lifecycle**: memoization, revalidation, early cutoff
- **In-flight deduplication**: cross-snapshot query coalescing
- **Snapshots**: MVCC point-in-time views
- **Persistence**: cache file encoding and semantics
- **Macros**: code generation for ergonomic API
- **Events and observability**: notifications, tracing, debugging

This document has two parts: **Core** (fundamental concepts and behaviors) and **Tooling** (macros, persistence, debugging).

## Nomenclature

**Revision**
A monotonically increasing 64-bit logical clock (u64) used to track changes. Higher revisions represent more recent states.

**Ingredient**
A storage and computation unit for one category of data. Three types exist: Input, Derived, and Interned.

**Input**
A mutable key-value store where values are set by user code. Changing an input bumps the global revision.

**Derived** (or **Tracked**)
An async memoized query that computes values from inputs or other derived queries. Dependencies are recorded automatically during execution.

**Interned**
An immutable value-to-id mapping. Values are interned once and never change; interning does not bump revision.

**Query Kind**
A stable 32-bit identifier (`QueryKindId`) for an ingredient type, computed from a name string via FNV-1a hash.

**Key**
A `facet-postcard` encoded byte sequence identifying a specific record within an ingredient.

**DynKey**
A `(kind, key)` pair that globally identifies one specific query or input record.

**Dependency (Dep)**
A recorded edge from a derived query to another ingredient's record, captured during query execution.

**Cell**
The memo table entry for a single derived query `(kind, key)`. Tracks state, cached value, and dependencies.

**Snapshot**
A point-in-time view of a database that can be queried independently. Created via MVCC structural sharing.

**Early Cutoff**
An optimization where recomputing a query that produces the same value does not bump `changed_at`, preventing unnecessary downstream recomputation.

---

# Core

This section specifies the fundamental concepts and behaviors of the picante runtime.

## Revisions

r[revision.type]
A revision MUST be represented as a 64-bit unsigned integer (`Revision(u64)`).

r[revision.monotonic]
The revision counter MUST be monotonically increasing within a runtime instance. It MUST never decrease.

r[revision.initial]
A new runtime MUST start at `Revision(0)`.

> r[revision.bump]
> Calling `Runtime::bump_revision()` MUST atomically increment the revision counter by exactly 1 and return the new revision value.

> r[revision.set]
> Calling `Runtime::set_current_revision(rev)` MUST set the current revision to the specified value. This is intended for cache restoration only and MUST NOT be called during normal operation.

### Change Tracking

Each cached value tracks two revisions:

r[revision.verified-at]
`verified_at` MUST record the revision at which the cached value was last confirmed to be valid.

r[revision.changed-at]
`changed_at` MUST record the revision at which the value actually changed. This MAY be older than `verified_at` if the value was revalidated without recomputation.

> r[revision.early-cutoff]
> When a derived query recomputes and produces a value equal to the previous cached value, `changed_at` MUST NOT advance. Only `verified_at` MUST be updated to the current revision.
>
> This enables early cutoff: downstream queries see that their dependency's `changed_at` hasn't advanced past their own `changed_at`, so they don't need to recompute.

## Query Kind IDs

r[kind.type]
A query kind id MUST be represented as a 32-bit unsigned integer (`QueryKindId(u32)`).

> r[kind.hash]
> `QueryKindId::from_str(name)` MUST compute the kind id using the FNV-1a hash algorithm over the UTF-8 bytes of the name string.
>
> ```rust
> // FNV-1a with 32-bit output
> const FNV_OFFSET: u32 = 2166136261;
> const FNV_PRIME: u32 = 16777619;
>
> fn fnv1a(bytes: &[u8]) -> u32 {
>     bytes.iter().fold(FNV_OFFSET, |hash, &byte| {
>         (hash ^ byte as u32).wrapping_mul(FNV_PRIME)
>     })
> }
> ```

r[kind.stability]
The kind id MUST remain stable across builds as long as the name string does not change. Cache files rely on this stability.

r[kind.uniqueness]
Within a single database instance, kind ids MUST be unique. The persistence layer MUST reject duplicate kind ids during save and load.

r[kind.collision]
While collisions are theoretically possible (32-bit hash space), they are treated as "should not happen" in practice.

## Keys

r[key.encoding]
Keys MUST be encoded using `facet-postcard` format. The encoded bytes are stored as `Arc<[u8]>`.

r[key.equality]
Key equality MUST be determined by exact byte equality of the encoded representation, not hash equality.

> r[key.hash]
> Each key MUST include a deterministic hash of its encoded bytes for diagnostics and tracing. This hash MUST NOT be used as a correctness boundary.

r[key.dyn-key]
A `DynKey` MUST consist of a `(kind: QueryKindId, key: Key)` pair that globally identifies one specific query or input record.

r[key.dep]
A `Dep` MUST use the same structure as `DynKey` and represents a recorded dependency edge.

## Dependency Tracking

r[dep.forward]
Picante MUST maintain forward dependencies: each derived query records which keys it read during computation.

r[dep.reverse]
Picante MUST maintain reverse dependencies: a map from each key to the set of queries that depend on it.

> r[dep.invalidation]
> When an input changes, `propagate_invalidation()` MUST walk the reverse dependency graph to find all affected queries. Only those queries need revalidation; everything else remains untouched.

r[dep.recording]
Dependencies MUST be recorded automatically during query execution via task-local frames. User code does not need to explicitly declare dependencies.

---

## Ingredients

This section specifies the behavior of each ingredient type.

### Input Ingredient

r[input.type]
An `InputIngredient<K, V>` MUST provide mutable key-value storage where `K` is the key type and `V` is the value type.

r[input.constraints]
Both `K` and `V` MUST implement `facet::Facet<'static> + Send + Sync + 'static`. `V` MUST also implement `Clone`.

> r[input.set]
> `InputIngredient::set(db, key, value)` MUST:
> 1. Encode the key using `facet-postcard`
> 2. Compare the new value with any existing value
> 3. If the value changed, bump the runtime revision
> 4. Store the value with `changed_at` set to the new revision
> 5. Notify the runtime to propagate invalidation via reverse deps
> 6. Emit an `InputSet` event

> r[input.get]
> `InputIngredient::get(db, key)` MUST:
> 1. Return `Ok(Some(value))` if the key exists
> 2. Return `Ok(None)` if the key does not exist
> 3. Record a dependency if called within an active derived query frame

> r[input.remove]
> `InputIngredient::remove(db, key)` MUST:
> 1. Remove the key-value pair if it exists
> 2. If a value was removed, bump the runtime revision
> 3. Notify the runtime to propagate invalidation
> 4. Emit an `InputRemoved` event

r[input.revision-on-change]
The revision MUST only be bumped when the value actually changes. Setting an input to its current value MUST NOT bump the revision.

### Derived Ingredient

r[derived.type]
A `DerivedIngredient<DB, K, V>` MUST provide async memoized query computation where `DB` is the database type, `K` is the key type, and `V` is the value type.

r[derived.compute-fn]
The derived ingredient MUST be constructed with a compute function that takes `(&DB, K)` and returns a future yielding `PicanteResult<V>`.

> r[derived.get]
> `DerivedIngredient::get(db, key)` MUST:
> 1. Look up the cell for `(kind, key)` in the memo table
> 2. If the cell is `Ready` with `verified_at == current_revision`, return the cached value immediately
> 3. Otherwise, attempt revalidation or recomputation (see derived query lifecycle)
> 4. Record a dependency if called within an active derived query frame

r[derived.memoization]
Derived queries MUST be memoized: the same `(kind, key)` at the same revision MUST NOT compute more than once within a single database instance.

### Interned Ingredient

r[interned.type]
An `InternedIngredient<K>` MUST provide immutable value interning where `K` is the interned value type.

r[interned.constraints]
`K` MUST implement `facet::Facet<'static> + Send + Sync + Clone + Eq + Hash + 'static`.

> r[interned.intern]
> `InternedIngredient::intern(value)` MUST:
> 1. If the value is already interned, return the existing `InternId`
> 2. Otherwise, allocate a new `InternId` and store the mapping
> 3. NOT bump the runtime revision (interning is append-only and does not invalidate)

> r[interned.get]
> `InternedIngredient::get(db, id)` MUST:
> 1. Return `Ok(Arc<K>)` containing the interned value
> 2. Record a dependency if called within an active derived query frame
> 3. Error if the id is invalid

r[interned.id-type]
`InternId` MUST be a 32-bit unsigned integer wrapper (`InternId(u32)`).

r[interned.stability]
Once a value is interned, its `InternId` MUST remain stable for the lifetime of the database (including across snapshots).

---

## Runtime

r[runtime.components]
A `Runtime` MUST own:
- The current revision counter
- Forward and reverse dependency graphs
- Event broadcast channels

r[runtime.new]
Creating a new `Runtime` MUST initialize the revision to 0 and create empty dependency graphs.

### Dependency Graph Operations

> r[runtime.update-deps]
> `Runtime::update_query_deps(dyn_key, deps)` MUST:
> 1. Store the forward dependencies for the query
> 2. Update the reverse dependency graph to point from each dep back to the query

> r[runtime.clear-deps]
> `Runtime::clear_dependency_graph()` MUST remove all forward and reverse dependencies. This is used during cache load to start fresh.

### Invalidation

> r[runtime.notify-input-set]
> `Runtime::notify_input_set(kind, key, changed_at)` MUST:
> 1. Emit an `InputSet` event
> 2. Call `propagate_invalidation` to find and invalidate all dependent queries

> r[runtime.propagate-invalidation]
> `Runtime::propagate_invalidation(kind, key)` MUST:
> 1. Look up all queries that depend on `(kind, key)` via the reverse dep graph
> 2. For each dependent query, emit a `QueryInvalidated` event
> 3. Recursively propagate to queries depending on those queries
> 4. This is a depth-first traversal; cycles are handled by tracking visited nodes

---

## Execution Frames

r[frame.task-local]
Picante MUST use Tokio task-local storage for execution frames. Frames are installed when a derived query starts computing and removed when it completes.

r[frame.purpose]
An active frame MUST:
- Record dependencies when inputs or derived queries are read
- Track the current query stack for cycle detection
- Provide the context for automatic dependency recording

### Dependency Recording

> r[frame.record-dep]
> When a derived query or input is accessed during computation, the active frame MUST record a `Dep` entry containing the accessed `(kind, key)`.

r[frame.no-lock-await]
Frames MUST NOT hold any locks across `.await` points. Dependency recording is single-threaded within a task.

### Cycle Detection

r[frame.cycle-stack]
Each frame MUST maintain a stack of currently executing queries within the current task.

> r[frame.cycle-detect]
> Before starting to compute a query, the frame MUST check if that query is already in the stack. If so, a cycle error MUST be returned immediately.
>
> Example cycle error:
> ```text
> dependency cycle detected
>   → kind_1, key_0000000000000001  (initial)
>   → kind_2, key_0000000000000002
>   → kind_3, key_0000000000000003
>   → kind_1, key_0000000000000001  ← cycle (already in stack)
> ```

r[frame.cycle-per-task]
Cycle detection is per-task (task-local stack). Cross-task cycles are not directly detected but will manifest as deadlocks.

---

## Derived Query Lifecycle

This section specifies the state machine and lifecycle of derived query cells.

### Cell States

r[cell.states]
Each cell MUST be in exactly one of these states:

| State | Description |
|-------|-------------|
| `Vacant` | No cached value exists |
| `Running { started_at }` | A task is currently computing the value |
| `Ready { value, deps, verified_at, changed_at }` | A cached value with its dependency list |
| `Poisoned { error, verified_at }` | Previous compute failed at this revision |

r[cell.stale]
A `Ready` cell is considered "stale" when `verified_at != current_revision`. It has a cached value but must be revalidated before use.

### Access Algorithm

> r[cell.access]
> When accessing a derived query `(kind, key)` at revision `rev`:
>
> 1. If cell is `Ready` with `verified_at == rev`, return cached value immediately
> 2. If cell is `Ready` but stale, attempt revalidation
> 3. If cell is `Vacant`, `Poisoned`, or revalidation failed, attempt computation
> 4. If cell is `Running`, wait for completion and retry

### Revalidation

> r[cell.revalidate]
> Revalidation MUST check each stored dependency:
>
> 1. For each `Dep` in the stored dependency list:
>    - Look up the ingredient via `IngredientLookup`
>    - Call the ingredient's `touch()` method to get its `changed_at`
>    - If any dep's `changed_at > self.changed_at`, revalidation fails
> 2. If all deps pass, bump `verified_at` to current revision and return cached value
> 3. If any dep fails (changed or missing), proceed to recomputation

r[cell.revalidate-missing]
If `IngredientLookup` cannot find a dependency's ingredient (kind not registered), revalidation MUST fail.

### Computation

> r[cell.compute]
> When computation is needed:
>
> 1. Transition cell to `Running { started_at: current_revision }` under lock
> 2. Capture any previous cached value for early cutoff comparison
> 3. Release lock and execute the compute function
> 4. Compare result with previous value:
>    - Fast path: `Arc::ptr_eq(prev, new)` for same allocation
>    - Slow path: deep equality via `facet_assert::check_same`
> 5. Update cell to `Ready { value, deps, verified_at: rev, changed_at }`
>    - If value changed: `changed_at = rev`
>    - If value unchanged: `changed_at = prev.changed_at` (early cutoff)
> 6. Notify waiters via `Notify::notify_waiters()`

### Leader Election

r[cell.leader-local]
Within a single database instance, the first task to transition a cell to `Running` becomes the "leader" for that computation.

> r[cell.waiter]
> Tasks that observe a `Running` cell MUST:
> 1. Create a `notified` future from the cell's `Notify` *before* checking state (to avoid race)
> 2. Wait on the `notified` future
> 3. Retry the access loop when notified

r[cell.no-lock-await]
The leader MUST NOT hold the cell lock while awaiting the compute future. Only state transitions happen under lock.

### Poisoning

> r[cell.poison]
> If compute returns an error or panics:
> 1. Transition cell to `Poisoned { error, verified_at: current_revision }`
> 2. Notify waiters
> 3. Subsequent accesses at the same revision observe the poison and return the error

> r[cell.poison-scoped]
> Poisoning is revision-scoped. At a later revision (after an input change), the cell is treated as stale and can be recomputed. This allows transient failures to be retried.

---

## In-flight Deduplication

This section specifies cross-snapshot query coalescing.

### Purpose

r[inflight.purpose]
When multiple tasks across a database and its snapshots request the same derived query `(kind, key)` at the same revision, picante SHOULD coalesce them into a single computation.

### Scope

r[inflight.scope]
Coalescing MUST be scoped to:
- Same `RuntimeId` (shared by database and its snapshots)
- Same `Revision`
- Same `QueryKindId`
- Exact `Key` bytes

### Global Registry

r[inflight.registry]
The in-flight registry MUST be a process-global `DashMap<InFlightKey, Arc<InFlightEntry>>`.

> r[inflight.key]
> An `InFlightKey` MUST contain:
> - `runtime_id: RuntimeId`
> - `revision: Revision`
> - `kind: QueryKindId`
> - `key: Key`

### Leader Election

> r[inflight.try-lead]
> `InFlightRegistry::try_lead(key)` MUST:
> 1. Atomically check if an entry exists for the key
> 2. If not, insert a new entry and return `Leader(InFlightGuard)`
> 3. If exists, return `Follower(Arc<InFlightEntry>)`

### Completion

> r[inflight.complete]
> The leader MUST call `guard.complete(value, deps, changed_at)` on success:
> 1. Store the result in the entry
> 2. Transition state to `Done`
> 3. Notify all waiters
> 4. Remove the entry from the registry

> r[inflight.fail]
> The leader MUST call `guard.fail(error)` on failure:
> 1. Store the error in the entry
> 2. Transition state to `Failed`
> 3. Notify all waiters
> 4. Remove the entry from the registry

> r[inflight.cancel]
> If the guard is dropped without completion, the entry MUST be marked `Cancelled` and removed. Followers should retry and may become the new leader.

### Follower Behavior

> r[inflight.follower]
> Followers MUST:
> 1. Wait on `entry.notified()`
> 2. Read `entry.state()` after notification
> 3. On `Done`: adopt the value into their local cell
> 4. On `Failed`: propagate the error as `Poisoned` into their local cell
> 5. On `Cancelled`: retry the entire access (may become leader)

### Shared Completed Cache

r[inflight.shared-cache]
A bounded shared cache of completed results MAY allow snapshots to adopt work even if they didn't overlap as waiters.

> r[inflight.shared-cache-key]
> The shared cache MUST be keyed by `(runtime_id, kind, key)` — note: no revision in the key.

r[inflight.shared-cache-size]
The cache size MUST be configurable via `PICANTE_SHARED_CACHE_MAX_ENTRIES` environment variable. Default: 20,000 entries.

> r[inflight.shared-cache-adopt]
> Adoption from shared cache MUST:
> 1. Check if `record.verified_at == current_revision` — if so, adopt immediately
> 2. Otherwise, run revalidation against `record.deps` / `record.changed_at`
> 3. On successful adoption, update the local cell and reinsert with updated `verified_at`

---

## Snapshots

This section specifies database snapshot behavior.

### Creation

r[snapshot.creation]
Snapshots MUST be created via `DatabaseSnapshot::from_database(&db).await`.

r[snapshot.async]
Snapshot creation is async because it needs to lock cells to clone their state.

### Semantics

r[snapshot.frozen]
A snapshot MUST freeze the database state at creation time. Subsequent modifications to the database MUST NOT be visible in the snapshot.

r[snapshot.independent]
Queries run on a snapshot MUST use the snapshot's cached values and MUST cache new computations in the snapshot's own memo tables.

### Per-Ingredient Behavior

> r[snapshot.input]
> `InputIngredient` snapshots MUST use O(1) structural sharing via `im::HashMap`. The snapshot shares structure with the original; only modified paths are copied (copy-on-write).

> r[snapshot.derived]
> `DerivedIngredient` snapshots MUST deep-clone cached cells into new `ErasedCell` instances. Values remain cheap to clone (they are `Arc<dyn Any>`).

> r[snapshot.interned]
> `InternedIngredient` snapshots MUST share the same `Arc` with the original. New interns after snapshot creation are visible to both (append-only semantics).

### Multiple Snapshots

r[snapshot.multiple]
Multiple snapshots MAY be created at different points in time. Each sees its respective version of the data.

### Runtime Identity

r[snapshot.runtime-id]
A snapshot MUST share the same `RuntimeId` as its parent database. This enables in-flight deduplication across snapshots.

---

# Tooling

This section specifies the tooling layer: persistence, macros, events, and debugging.

## Persistence

r[persist.format]
Cache files MUST be encoded using `facet-postcard` format.

### Cache File Structure

> r[persist.structure]
> A cache file MUST contain:
> - Format version (for forward compatibility checks)
> - Current revision at save time
> - Per-ingredient sections

> r[persist.section]
> Each section MUST contain:
> - `kind_id: u32` (QueryKindId)
> - `kind_name: String` (for mismatch detection)
> - `section_type: SectionType` (Input, Derived, or Interned)
> - `records: Vec<Vec<u8>>` (opaque facet-postcard blobs)

> r[persist.not-stored]
> Cache files MUST NOT store:
> - Custom fields on the database struct
> - The dependency graph as a separate section (it is reconstructed during load)

### Save API

r[persist.save-fn]
`save_cache(path, runtime, ingredients)` MUST save all persistable ingredients to the specified path.

> r[persist.save-options]
> `save_cache_with_options` MUST support:
> - `max_bytes: Option<u64>` — best-effort total size cap
> - `max_records_per_section: Option<usize>` — limit records per ingredient
> - `max_record_bytes: Option<usize>` — skip oversized records

> r[persist.save-atomic]
> Save MUST write to a temporary file then rename atomically into place.

> r[persist.save-unique-kinds]
> Save MUST reject duplicate kind ids in the ingredient list.

### Load API

r[persist.load-fn]
`load_cache(path, runtime, ingredients)` MUST load cached state from the specified path.

> r[persist.load-return]
> Load MUST return:
> - `Ok(true)` — file existed and was loaded successfully
> - `Ok(false)` — file did not exist
> - `Ok(false)` — file was corrupt and `on_corrupt` was `Ignore` or `Delete`
> - `Err(...)` — file was corrupt and `on_corrupt` was `Error`

> r[persist.load-options]
> `load_cache_with_options` MUST support:
> - `max_bytes: Option<u64>` — refuse files larger than this
> - `on_corrupt: OnCorruptCache` — `Error`, `Ignore`, or `Delete`

### Load Validation

r[persist.load-version]
The format version MUST match exactly. Version mismatch is treated as corruption.

r[persist.load-kind-match]
Each section's `kind_id` MUST match a provided ingredient. Unknown sections MUST be ignored with a warning.

r[persist.load-name-match]
For known sections, `kind_name` MUST match exactly. Mismatch is an error.

r[persist.load-type-match]
For known sections, `section_type` MUST match exactly. Mismatch is an error.

### Load Order

> r[persist.load-order]
> Load MUST proceed in this order:
> 1. `runtime.clear_dependency_graph()` — clear existing deps
> 2. `ingredient.clear()` for every ingredient — prevent partial state blending
> 3. `ingredient.load_records(...)` for each found section
> 4. `ingredient.restore_runtime_state(runtime)` for every ingredient — rebuild reverse deps
> 5. `runtime.set_current_revision(cache.current_revision)`

---

## Macros

This section specifies the code generation performed by picante's procedural macros.

### `#[picante::input]`

r[macro.input.purpose]
The `#[picante::input]` macro generates keyed or singleton input storage.

> r[macro.input.keyed]
> For keyed inputs (struct with `#[key]` field):
> ```rust
> #[picante::input]
> pub struct SourceFile {
>     #[key]
>     pub path: String,
>     pub content: String,
> }
> ```
>
> MUST generate:
> - Two ingredients: `{Name}KeysIngredient` (Interned) and `{Name}DataIngredient` (Input)
> - A `{Name}Data` struct for non-key fields
> - A `Has{Name}Ingredient` trait for database bounds
> - A `{Name}` newtype wrapper around `InternId`
> - Methods: `new(db, key, fields...) -> PicanteResult<Self>`
> - Getters: `{field}(db) -> PicanteResult<T>` for each field

> r[macro.input.singleton]
> For singleton inputs (no `#[key]` field):
> ```rust
> #[picante::input]
> pub struct Config {
>     pub debug: bool,
> }
> ```
>
> MUST generate:
> - Methods: `set(db, fields...) -> PicanteResult<()>`
> - Methods: `get(db) -> PicanteResult<Option<Arc<ConfigData>>>`
> - Per-field getters: `{field}(db) -> PicanteResult<Option<T>>`

r[macro.input.kind-id]
Kind ids MUST be generated via `QueryKindId::from_str(...)` using the full module path and struct name.

### `#[picante::interned]`

r[macro.interned.purpose]
The `#[picante::interned]` macro generates immutable value interning.

> r[macro.interned.output]
> ```rust
> #[picante::interned]
> pub struct CharSet {
>     pub chars: Vec<char>,
> }
> ```
>
> MUST generate:
> - A `{Name}Ingredient` type alias for `InternedIngredient<{Name}Value>`
> - A `{Name}Value` struct with all fields
> - A `Has{Name}Ingredient` trait
> - A `{Name}` newtype wrapper around `InternId`
> - Methods: `new(db, fields...) -> PicanteResult<Self>`
> - Methods: `value(db) -> PicanteResult<Arc<{Name}Value>>`

### `#[picante::tracked]`

r[macro.tracked.purpose]
The `#[picante::tracked]` macro generates async memoized derived queries.

> r[macro.tracked.output]
> ```rust
> #[picante::tracked]
> pub async fn line_count<DB: DatabaseTrait>(db: &DB, file: SourceFile)
>     -> picante::PicanteResult<usize>
> {
>     Ok(file.content(db)?.lines().count())
> }
> ```
>
> MUST generate:
> - A `{NAME}_KIND` constant with stable QueryKindId
> - A `{Name}Query<DB>` type alias for `DerivedIngredient<DB, K, V>`
> - A `Has{Name}Query` trait
> - A `make_{name}_query<DB>() -> Arc<{Name}Query<DB>>` constructor
> - The public function calling through the ingredient

r[macro.tracked.key-tuple]
If the function has multiple parameters after `db`, the query key MUST be a tuple of those parameters.

r[macro.tracked.return-wrap]
If the return type is `T` (not `PicanteResult<T>`), the macro MUST wrap it in `Ok(T)`.

### `#[picante::db]`

r[macro.db.purpose]
The `#[picante::db]` macro generates the database struct with all ingredients.

> r[macro.db.output]
> ```rust
> #[picante::db(inputs(SourceFile), tracked(line_count))]
> pub struct Database {}
> ```
>
> MUST generate:
> - Reserved fields: `runtime: Runtime`, `ingredients: IngredientRegistry<Database>`
> - Ingredient fields for each declared input/interned/tracked
> - `impl HasRuntime for Database`
> - `impl IngredientLookup for Database`
> - `impl Has*` for each ingredient trait
> - A `Database::new()` constructor
> - A combined trait `DatabaseTrait` (or custom via `db_trait(Name)`)

> r[macro.db.snapshot]
> The macro MUST also generate a `{Name}Snapshot` struct with the same trait implementations.

---

## Runtime Events

r[event.channel]
`Runtime` MUST expose two subscription channels:
- `subscribe_revisions() -> watch::Receiver<Revision>`
- `subscribe_events() -> broadcast::Receiver<RuntimeEvent>`

r[event.broadcast-capacity]
The event broadcast channel MUST have capacity 1024. Lagging receivers may miss events.

### Event Types

> r[event.types]
> `RuntimeEvent` MUST include:
>
> | Event | When emitted |
> |-------|--------------|
> | `RevisionBumped { revision }` | `Runtime::bump_revision()` called |
> | `RevisionSet { revision }` | `Runtime::set_current_revision()` called (cache load) |
> | `InputSet { kind, key_hash, key, changed_at }` | Input value changed |
> | `InputRemoved { kind, key_hash, key }` | Input value removed |
> | `QueryInvalidated { kind, key_hash, key, by_kind }` | Derived query invalidated via reverse deps |
> | `QueryChanged { kind, key_hash, key, changed_at }` | Derived query recomputed with different value |

r[event.query-changed-cutoff]
`QueryChanged` MUST NOT be emitted if the recompute produces the same value (early cutoff).

r[event.key-fields]
All key references MUST include `kind`, `key_hash` (deterministic diagnostic hash), and `key` (full encoded bytes).

---

## Debugging and Observability

### Tracing

r[trace.crate]
Picante MUST use the `tracing` crate for structured instrumentation.

r[trace.no-subscriber]
Picante MUST NOT install a tracing subscriber. Applications choose their own logging setup.

> r[trace.levels]
> Instrumentation levels MUST be:
> - `debug`: set, remove, intern operations
> - `trace`: get operations, dep recording, revalidation steps
> - `info`: persistence completion
> - `warn`: corruption policy actions, unknown sections, record dropping

### Debug Module

r[debug.graph]
`DependencyGraph::from_runtime(runtime)` MUST capture the current dependency graph and support export to Graphviz DOT format via `write_dot(path)`.

r[debug.trace-collector]
`TraceCollector::start(runtime)` MUST record runtime events. `collector.stop().await` MUST return the collected trace.

r[debug.trace-analysis]
`TraceAnalysis::from_trace(trace)` MUST compute summary statistics: total events, input changes, invalidations, recomputations, duration.

r[debug.cache-stats]
`CacheStats::collect(runtime)` MUST return statistics about the dependency graph: forward deps, reverse deps, total edges, root queries.

---

## Error Handling

r[error.type]
`PicanteError` MUST be the error type returned by all fallible operations.

> r[error.variants]
> Error variants MUST include:
> - `Cycle { ... }` — dependency cycle detected
> - `InputNotFound { ... }` — input key does not exist
> - `InternIdNotFound { ... }` — invalid intern id
> - `IngredientNotFound { ... }` — ingredient not registered
> - `QueryPoisoned { ... }` — previous compute failed
> - `Persistence { ... }` — cache file errors
> - Other internal errors as needed

r[error.result]
`PicanteResult<T>` MUST be an alias for `Result<T, PicanteError>`.

---

## Type Erasure (Compile-Time Optimization)

r[type-erasure.purpose]
Picante MUST use type erasure to avoid monomorphization bloat. The derived query state machine compiles once per database type, not once per query.

> r[type-erasure.mechanism]
> Type erasure MUST use:
> - `trait ErasedCompute<DB>` — trait object for compute functions
> - `Arc<dyn Any + Send + Sync>` — type-erased values
> - Function pointers for equality checking: `eq_erased: fn(&dyn Any, &dyn Any) -> bool`

r[type-erasure.benefit]
This SHOULD achieve ~96% reduction in LLVM IR for the state machine and ~36% faster clean builds.

> r[type-erasure.tradeoffs]
> Acceptable runtime costs:
> - One vtable dispatch per compute
> - One `BoxFuture` allocation per compute
> - Key decode per compute
> - Deep equality check on recompute
