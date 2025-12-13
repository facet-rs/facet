+++
title = "Architecture"
weight = 3
+++

picante is intentionally layered:

1. **Runtime** (`Runtime`): owns the global `Revision` counter and event channels.
2. **Execution frames** (`frame`): Tokio task-local stack frames that record dependencies and detect cycles.
3. **Ingredients**: storage and logic for each query kind (currently: input + derived).

## Revisions and invalidation

picante v1 uses a single monotonically increasing `Revision`.

- Any input update bumps the revision.
- Memoized derived values are tagged with the revision they were computed for.
- If a caller asks for a value at revision `Rnew`, a value computed at `Rold != Rnew` is considered stale and will be recomputed.

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

