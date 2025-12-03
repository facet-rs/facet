+++
title = "Extend"
sort_by = "weight"
weight = 1
insert_anchor_links = "heading"
+++

Build on facet’s reflection system: write your own format crate, add extension attributes, or use `Peek`/`Partial` to power tools like pretty-printers and diff engines.

## Chapters
- [Extension Attributes](@/extend/extension-attributes.md) — Define namespaced attributes with compile-time validation.
- [Shape](@/extend/shape.md) — What the runtime type description contains and how to use it.
- [Peek](@/extend/peek.md) — Read values dynamically.
- [Partial](@/extend/partial.md) — Build values dynamically (strict vs deferred).
- [Solver](@/extend/solver.md) — Disambiguate `#[facet(flatten)]` and `#[facet(untagged)]` efficiently.
- [Build a Format Crate](@/extend/format-crate.md) — Architecture and testing patterns (outline).

If you just want to *use* facet for serialization, head to [Guide](/guide).
