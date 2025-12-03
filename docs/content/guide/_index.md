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
- See it in action: [Showcases](@/guide/showcases/_index.md).

## Guides
- [Getting Started](@/guide/getting-started.md)
- [Why facet?](@/guide/why.md)
- [Errors & diagnostics](@/guide/errors.md)
- [Comparison with serde](@/guide/migration/_index.md)

## Reference (quick links)
- [Attributes Reference](@/reference/attributes/) — complete `#[facet(...)]` catalog.
- [Format comparison matrix](@/reference/format-crate-matrix/) — feature support across format crates.

## Works well with
- [`structstruck`](@/guide/structstruck.md) — generate structs from sample data, then add `Facet` for multi-format I/O and diagnostics.
