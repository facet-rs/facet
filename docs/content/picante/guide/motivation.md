+++
title = "Motivation"
+++

picante exists because the "Salsa model" is extremely useful for large pipelines, but Dodeca's query graph needs **async** queries.

In Dodeca, many queries naturally want to:

- read files concurrently,
- call plugins (often in separate processes),
- spawn work on a thread pool,
- stream data.

Keeping those queries deterministic while still getting incremental recomputation is the point of picante.

