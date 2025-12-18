+++
title = "In-flight Deduplication"
weight = 11
+++

This page documents picante’s cross-snapshot in-flight query deduplication and the shared completed-result cache.

Primary implementation: `crates/picante/src/inflight.rs` (used by `crates/picante/src/ingredient/derived.rs`).

## What it does

When multiple tasks across a database and its snapshots request the same derived query `(kind, key)` at the same revision, picante can coalesce them:

- one task becomes the leader and performs the compute
- others await the leader’s result and then adopt it into their own memo tables

This is in addition to “single-flight within a single db instance” (the per-cell `Running` state).

## Scope (correctness boundary)

Coalescing is scoped to:

- the same **runtime identity** (`RuntimeId`) — shared by a db and its snapshots
- the same **revision**
- the same **query kind**
- exact **key bytes** (encoded `Key`)

## Global in-flight registry

The in-flight registry is a process-global `DashMap<InFlightKey, Arc<InFlightEntry>>`.

An `InFlightKey` is:

- `runtime_id: RuntimeId`
- `revision: Revision`
- `kind: QueryKindId`
- `key: Key` (exact bytes)

Leader election:

- `try_lead(key)` does a `DashMap::entry`
- the first caller inserts a new `InFlightEntry` and gets an `InFlightGuard`
- subsequent callers receive `Follower(Arc<InFlightEntry>)`

Completion / cleanup:

- the leader must call `guard.complete(...)` or `guard.fail(...)`
- those methods set state to `Done`/`Failed`, notify waiters, and remove the key from the registry
- if the guard is dropped without completion, it marks the entry `Cancelled` and removes it (followers should retry and may become the new leader)

Follower behavior (in `DerivedCore`):

- wait on `entry.notified()`
- then read `entry.state()` and:
  - adopt `Done { value, deps, changed_at }`
  - propagate `Failed(err)` into the local cell as `Poisoned`
  - retry on `Cancelled`

## Shared completed-result cache

There is also a bounded shared cache of *completed* derived-query results that allows snapshots to adopt work even if they didn’t overlap in time as “waiters”.

Tuning:

- `PICANTE_SHARED_CACHE_MAX_ENTRIES` controls the max number of shared records (default is currently 20,000).

Example:

```sh
PICANTE_SHARED_CACHE_MAX_ENTRIES=5000 your-app
```

This cache:

- is keyed by `(runtime_id, kind, key)` (note: no revision in the key)
- stores `{ value, deps, changed_at, verified_at, insert_id }`
- is bounded by entry count (not bytes)
- uses an insertion-order queue plus an insertion id so “duplicate key re-inserts” don’t cause fresh entries to be evicted by stale queue entries

Adoption path (in `DerivedCore`):

- after becoming the local cell leader, it checks the shared cache first
- if `record.verified_at == rev`, it can adopt immediately
- otherwise, it runs the normal revalidation check against `record.deps` / `record.changed_at`
- when adopted, the local cell becomes `Ready { verified_at: rev, changed_at: record.changed_at }`, and the record is reinserted with `verified_at` updated to `rev`
