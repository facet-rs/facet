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

picante is different in two major ways:

- **tokio-first / async-first**: derived queries are `async` and single-flight.
- **facet-based persistence**: picante avoids serde and uses [facet](https://facets.rs) + [facet-postcard](https://docs.rs/facet-postcard).

## Plain memoization

Simple memoization caches values, but without dependency edges you can’t answer:

- “what depends on this input?”
- “what needs to be recomputed?”

picante records explicit dependencies between queries while computing.

