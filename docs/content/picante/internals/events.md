+++
title = "Runtime Events"
weight = 12
+++

This page documents the runtime-side notification APIs used for live reload and diagnostics.

Primary implementation: `crates/picante/src/runtime.rs`.

## Subscribing

`Runtime` exposes two subscription channels:

- `Runtime::subscribe_revisions()` returns a `watch::Receiver<Revision>`
- `Runtime::subscribe_events()` returns a `broadcast::Receiver<RuntimeEvent>`

The event channel is a Tokio `broadcast` with capacity 1024. If a receiver lags, it may get a `Lagged` error and should decide whether to resubscribe or treat it as “best effort”.

### Example: subscribe to revision bumps

```rust,noexec
use picante::{HasRuntime, Revision};

let mut revisions = db.runtime().subscribe_revisions();
assert_eq!(*revisions.borrow(), Revision(0));

revisions.changed().await?;
let now = *revisions.borrow();
```

### Example: subscribe to events

```rust,noexec
use picante::{HasRuntime, RuntimeEvent};

let mut events = db.runtime().subscribe_events();
while let Ok(ev) = events.recv().await {
    match ev {
        RuntimeEvent::QueryInvalidated { kind, key_hash, by_kind, .. } => {
            eprintln!("invalidated {kind:?}:{key_hash:x} by {by_kind:?}");
        }
        _ => {}
    }
}
```

## Events

The `RuntimeEvent` stream includes (at least):

- revision changes (`RevisionBumped`, `RevisionSet`)
- input mutations (`InputSet`, `InputRemoved`)
- invalidation propagation (`QueryInvalidated`)
- derived query changes (`QueryChanged`)

All key references include:

- `kind: QueryKindId`
- `key_hash: u64` (a deterministic hash of the encoded key bytes, for diagnostics)
- `key: Key` (the actual encoded bytes)

## When events are emitted

- `RevisionBumped` is emitted by `Runtime::bump_revision()`.
- `RevisionSet` is emitted by `Runtime::set_current_revision()` (typically after cache load).
- `InputSet` / `InputRemoved` are emitted by `Runtime::notify_input_set` / `Runtime::notify_input_removed`.
  - those also run invalidation propagation and emit `QueryInvalidated` events for dependents found in the current reverse-deps graph.
- `QueryChanged` is emitted by derived queries when a recompute *logically changes the output* at the current revision.
  - if a recompute produces the same value (early cutoff), `QueryChanged` is not emitted.

## Dependency graph dependency

`QueryInvalidated` depends on the runtime’s reverse-dependency graph being populated.

That graph is built incrementally by `Runtime::update_query_deps(query, deps)` calls made by derived queries when they compute (and also during cache load via `PersistableIngredient::restore_runtime_state`).

This stream is intended for:

- driving “live reload” UIs
- emitting diagnostics and traces
- testing / instrumentation (see `crates/picante/tests/notifications.rs`)
