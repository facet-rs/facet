+++
title = "Derived Cells"
weight = 10
+++

This page documents picante’s derived-query “cell” state machine (the memo table entries for a single `(kind, key)`).

Primary implementation: `crates/picante/src/ingredient/derived.rs`.

## What a “cell” is

Each derived query kind maintains a map from an erased key (`DynKey`) to an `ErasedCell`. Each `ErasedCell` owns:

- a `state` (guarded by an async `Mutex`)
- a `Notify` used to wake waiters when state changes

The outer map (`im::HashMap<DynKey, Arc<ErasedCell>>`) is itself behind a `RwLock`, so:

- lookups are fast under a read lock
- new cells are inserted under a write lock (double-checked)
- snapshots are cheap because `im::HashMap` structurally shares

## States

At a high level, each cell is in one of these states:

- `Vacant`: no cached value
- `Running { started_at }`: a task is currently computing
- `Ready { value, deps, verified_at, changed_at }`: cached value + dep list
- `Poisoned { error, verified_at }`: previous compute attempt failed for a specific revision

picante also distinguishes “ready, but stale” when `verified_at != current revision`: the cell still has a cached value and deps, but it must be revalidated before it can be reused for the current revision.

## `verified_at` vs `changed_at`

- `verified_at` is the revision where we last *checked* that the value is still valid.
- `changed_at` is the revision where the value last *actually changed*.

If a query recomputes but produces the same result (deep-equality), `changed_at` does not advance (early cutoff).

## Dependency recording and cycle detection

While a derived query is computing, picante installs an “active frame” (task-local) that:

- records dependencies when inputs/derived queries are read
- tracks the current query stack for cycle detection

Cycle detection is per-task (task-local stack). If a query attempts to access itself through the stack, it errors immediately.

## Revalidation model (precise deps)

On access at revision `rev`:

1. If the cell is `Ready` with `verified_at == rev`, return immediately.
2. Otherwise, attempt to revalidate by checking the stored dependency list:
   - for each dep, call into the dependent ingredient via `IngredientLookup`
   - compare each dep’s `touch(...).changed_at` to the cell’s `self_changed_at`
3. If revalidation succeeds, bump `verified_at` to `rev` and reuse the cached value.
4. If revalidation fails (some dep changed), run the query compute again.

The revalidation logic is what enables precise invalidation without “durability tiers”.

### What “revalidate” does in code

The current algorithm is intentionally simple:

- a derived value is considered valid if *every dependency’s* `changed_at <= self_changed_at`
- if any dependency reports `changed_at > self_changed_at`, the value is stale and must recompute
- if `IngredientLookup` can’t find a dependency ingredient (missing kind), revalidation fails

This is implemented in `DerivedCore::try_revalidate(...)`.

## Compute / leader election (local cell)

Once a caller decides it must compute, it tries to transition the cell to `Running { started_at: rev }` under the cell’s mutex.

While doing so, it captures the previous cached value (if any) so it can compute early-cutoff:

- fast path: `Arc::ptr_eq(prev, out)` (literally the same allocation)
- slow path: deep equality via an `eq_erased` function pointer (uses `facet_assert`)

Then it publishes:

- `Runtime::update_query_deps(dyn_key, deps)`
- `Runtime::notify_query_changed(rev, dyn_key)` (only when `changed_at == rev`)
- the new `Ready` state and `Notify::notify_waiters()`

## Waiters

If a caller observes `Running`, it waits on `Notify` and retries the loop. The code intentionally creates the `notified = cell.notify.notified()` future *before* inspecting the state to avoid missing a wakeup between “saw running” and “started waiting”.

## Poisoning

If compute returns an error, or panics:

- the cell becomes `Poisoned { error, verified_at: rev }`
- waiters are notified
- subsequent accesses at the same revision observe the poisoned state and return the error
- at a later revision, the cell is treated as stale and can be recomputed

This “poisoning is revision-scoped” behavior matters when you have transient failures: bumping revision (by changing an input) gives the system an opportunity to retry.

## Cross-snapshot adoption

The derived access loop also has two cross-snapshot mechanisms (documented in more detail in [In-flight Deduplication](../inflight/)):

- a shared completed-result cache (“adopt a ready record if valid”)
- a global in-flight registry (“followers await the leader across snapshots”)

## Promotion APIs (manual cache movement)

Derived cells can be extracted and inserted manually:

- `ErasedCell::ready_record()` returns an `ErasedReadyRecord` if the cell is currently `Ready`.
- `DerivedIngredient::insert_ready_record(key, record)` inserts a ready record into a different ingredient instance.
- `DerivedIngredient::record_is_valid_on(db, record)` checks whether a record can be safely promoted onto a DB at its current revision.

This is intended for “request snapshot → promote back into live DB” patterns.

## Snapshot interactions

Two snapshot-related APIs exist for derived caches:

- `DerivedIngredient::snapshot()` returns a shared `im::HashMap<DynKey, Arc<ErasedCell>>` (structural sharing).
- `DerivedIngredient::snapshot_cells_deep()` deep-snapshots only `Ready` cells into *new* `ErasedCell` instances (but values are still cheap to clone because they are `Arc<dyn Any>`).

The `#[picante::db]` snapshot constructor uses `snapshot_cells_deep()` so snapshot caches don’t observe later cell state transitions from the parent DB.

Within one database instance, concurrent callers for the same cell coordinate through the `Running` state and `Notify`:

- the leader transitions the cell to `Running` and computes
- followers observe `Running` and await a notification, then retry

No locks are held across `.await` points for followers.
