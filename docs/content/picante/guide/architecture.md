+++
title = "Architecture"
weight = 3
+++

picante is intentionally layered:

1. **Runtime** (`Runtime`): owns the global `Revision` counter, dependency graphs, and event channels.
2. **Execution frames** (`frame`): tokio task-local stack frames that record dependencies and detect cycles.
3. **Ingredients**: storage and logic for each query kind (input, derived, interned).

## Revisions and change tracking

picante uses a monotonically increasing `Revision` as a logical clock. Each cached value tracks two revisions:

- **`verified_at`**: when we last checked if the value is still valid
- **`changed_at`**: when the value actually changed (may be older than `verified_at`)

This distinction enables **early cutoff**: if a query recomputes and produces the same result, `changed_at` stays the same. Downstream queries see that their dep's `changed_at` hasn't advanced, so they don't need to recompute.

## Dependency tracking

picante maintains both forward and reverse dependency graphs:

- **Forward deps**: each derived query records which keys it read during computation
- **Reverse deps**: maps each key to the set of queries that depend on it

When an input changes, `propagate_invalidation()` walks the reverse dep graph to find all affected queries. Only those queries need revalidation — everything else is untouched.

## Derived queries (single-flight)

Each derived key maps to a “cell”:

- `Vacant`: never computed
- `Running`: one task is computing it
- `Ready`: value + deps + verified_at revision
- `Poisoned`: previous compute failed or panicked at that revision

Waiters use a `Notify` + loop pattern: nobody holds locks while awaiting.

## Persistence

picante can snapshot inputs and memoized derived values (including dependency lists) to a single on-disk file, encoded with `facet-postcard`.

This is conceptually similar to Salsa's `Database::serialize/deserialize` (which Dodeca previously stored as postcard), but picante uses `facet` instead of serde.

