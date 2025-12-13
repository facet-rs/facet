+++
title = "Future Work"
weight = 4
+++

Some next steps that are intentionally not in v1:

- **Dependency-based revalidation**: reuse values across revisions when deps are unchanged.
- **Output fingerprinting / changed_at tracking**: avoid rebuilding unchanged downstream nodes.
- **Cross-task cycle detection**: detect deadlocks across tasks (wait-graph).
- **More ingredient kinds**: accumulators, durability tiers.
- **Smarter cache limits**: eviction strategies and incremental persistence.

