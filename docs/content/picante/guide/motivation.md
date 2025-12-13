+++
title = "Motivation"
weight = 1
+++

picante exists because the "[salsa](https://salsa-rs.netlify.app/) model" is extremely useful for large pipelines, but [dodeca](https://dodeca.dev)'s query graph needs **async** queries.

In [dodeca](https://dodeca.dev), many queries naturally want to:

- read files concurrently,
- call plugins (often in separate processes),
- spawn work on a thread pool,
- stream data.

Keeping those queries deterministic while still getting incremental recomputation is the point of picante.

