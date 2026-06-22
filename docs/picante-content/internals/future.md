+++
title = "Future Work"
weight = 5
+++

Some next steps that are intentionally not in v1:

- **Cross-task cycle detection**: detect deadlocks across tasks (wait-graph).
- **More ingredient kinds**: accumulators for collecting values across queries.
- **Smarter cache limits**: eviction strategies and incremental persistence.
- **Parallel query execution**: run independent queries concurrently within a single request.

## Already implemented

These features were originally planned for later but are now available:

- **Database snapshots**: point-in-time views for consistent reads (see [Snapshots](./snapshots/)).
- **Dependency-based revalidation**: cached values are reused when their dependencies haven't changed.
- **Output fingerprinting**: queries track `changed_at` vs `verified_at` to enable early cutoff — if a query recomputes but produces the same result, downstream queries skip recomputation.
