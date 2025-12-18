+++
title = "Comparisons"
weight = 2
+++

## salsa

[salsa](https://salsa-rs.netlify.app/) is the closest conceptual ancestor:

- incremental recomputation
- dependency tracking
- memoization

In [dodeca](https://dodeca.dev), [salsa](https://salsa-rs.netlify.app/) persistence was implemented by serializing the salsa database with [postcard](https://docs.rs/postcard) and writing it to disk.

picante is different in a few key ways:

- **tokio-first / async-first**: derived queries are `async` and single-flight.
- **facet-based persistence**: picante avoids serde and uses [facet](https://facets.rs) + [facet-postcard](https://docs.rs/facet-postcard).
- **no durability tiers**: picante tracks dependencies precisely, so it doesn't need them.
- **MVCC snapshots**: picante uses structural sharing for point-in-time database snapshots.

### MVCC vs cooperative locking

salsa and picante take fundamentally different approaches to handling concurrent access:

**salsa: cooperative locking / cancellation**

When inputs change while a query is running, salsa can *cancel* the in-progress computation and restart it with the new inputs. This ensures queries always see consistent, up-to-date values — but requires queries to be cancellation-aware.

**picante: MVCC (Multi-Version Concurrency Control)**

picante uses `im::HashMap` for structural sharing. You can create cheap O(1) *snapshots* that capture a point-in-time view of the database:

```rust
let snapshot = DatabaseSnapshot::from_database(&db).await;

// Snapshot sees inputs as they were at creation time
// Changes to `db` don't affect the snapshot
let result = my_query(&snapshot, key).await?;
```

This means:
- Readers never block writers
- Multiple snapshots can exist concurrently at different versions
- No cancellation needed — queries run to completion against their snapshot
- Useful for diffing, parallel processing, and consistent multi-query reads

### Why no durability?

salsa has "durability" levels (Low, Medium, High) that let you hint how often an input changes. High-durability inputs (like config) trigger broader invalidation when changed.

picante doesn't need this because it tracks dependencies *precisely*:

- Each derived query records exactly which keys it read
- A reverse dependency graph maps each input to its dependents
- When an input changes, only queries that actually depend on that specific key are invalidated

Durability is an optimization to *skip* dependency checking — the assumption being "if config changed, just rebuild everything." But with precise tracking, you get the same result automatically: config changes only invalidate queries that read config. Template changes only invalidate pages using that template. No manual annotation needed.

## Plain memoization

Simple memoization caches values, but without dependency edges you can't answer:

- "what depends on this input?"
- "what needs to be recomputed?"

picante records explicit dependencies between queries while computing.
