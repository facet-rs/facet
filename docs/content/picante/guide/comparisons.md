+++
title = "Comparisons"
+++

## Salsa

Salsa is the closest conceptual ancestor:

- incremental recomputation
- dependency tracking
- memoization

In Dodeca, Salsa persistence was implemented by serializing the Salsa database with postcard and writing it to disk.

picante is different in two major ways:

- **Tokio-first / async-first**: derived queries are `async` and single-flight.
- **Facet-based persistence**: picante avoids serde and uses `facet` + `facet-postcard`.

## Plain memoization

Simple memoization caches values, but without dependency edges you can’t answer:

- “what depends on this input?”
- “what needs to be recomputed?”

picante records explicit dependencies between queries while computing.

