+++
title = "Guide"
sort_by = "weight"
weight = 1
insert_anchor_links = "heading"
+++

Learn how to use facet for serialization, deserialization, and rich diagnostics.

## Start here

- First run: [Getting Started](@/guide/getting-started.md) — install → derive → round-trip → errors.
- Decide if facet fits: [Why facet?](@/guide/why.md) — tradeoffs vs serde.
- See it in action: [Showcases](@/showcases/_index.md).

## Guides
- [Getting Started](@/guide/getting-started.md)
- [Why facet?](@/guide/why.md)
- [Dynamic Values](@/guide/dynamic-values.md) — `Value`, `assert_same!`, `RawJson`
- [Variance and Soundness](@/guide/variance.md) — lifetime safety in reflection
- [Comparison with serde](@/guide/serde/_index.md)
- [FAQ](@/guide/faq.md) — common questions and quick answers

## Reference (quick links)
- [Format comparison matrix](@/reference/format-crate-matrix/) — feature support across format crates
- [Extension Attributes](@/reference/attributes/) — namespaced attributes by crate

## Ecosystem
- [Third-party types](@/guide/ecosystem.md) — uuid, chrono, time, camino, bytes, and more
- [`structstruck`](@/guide/structstruck.md) — generate structs from sample data
