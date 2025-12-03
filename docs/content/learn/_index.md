+++
title = "Learn"
sort_by = "weight"
insert_anchor_links = "heading"
+++

Learn how to use facet for serialization, deserialization, and rich diagnostics.

## Start here

- First run: [Getting Started](@/learn/getting-started.md) — install → derive → round-trip → errors.
- Decide if facet fits: [Why facet?](@/learn/why.md) — tradeoffs vs serde.
- See it in action: [Showcases](@/learn/showcases/_index.md).

## Guides
- [Getting Started](@/learn/getting-started.md)
- [Why facet?](@/learn/why.md)
- [Errors & diagnostics](@/learn/errors.md)
- [Comparison with serde](@/learn/migration/_index.md)

## Reference
- [Attributes Reference](@/learn/attributes.md) — complete `#[facet(...)]` catalog.
- [Format comparison matrix](@/format-crate-matrix.md) — feature support across crates.

## Works well with
- [`structstruck`](@/learn/structstruck.md) — generate structs from sample data, then add `Facet` for multi-format I/O and diagnostics.

## Format Crates

<div class="format-cards">
  <a class="format-card" href="https://docs.rs/facet-json">
    <div class="format-card__title">JSON</div>
    <div class="format-card__crate">facet-json</div>
    <p class="format-card__desc">JSON serialization/deserialization</p>
  </a>
  <a class="format-card" href="https://docs.rs/facet-yaml">
    <div class="format-card__title">YAML</div>
    <div class="format-card__crate">facet-yaml</div>
    <p class="format-card__desc">YAML support</p>
  </a>
  <a class="format-card" href="https://docs.rs/facet-toml">
    <div class="format-card__title">TOML</div>
    <div class="format-card__crate">facet-toml</div>
    <p class="format-card__desc">TOML configuration files</p>
  </a>
  <a class="format-card" href="https://docs.rs/facet-kdl">
    <div class="format-card__title">KDL</div>
    <div class="format-card__crate">facet-kdl</div>
    <p class="format-card__desc">KDL document language</p>
  </a>
  <a class="format-card" href="https://docs.rs/facet-msgpack">
    <div class="format-card__title">MessagePack</div>
    <div class="format-card__crate">facet-msgpack</div>
    <p class="format-card__desc">Binary format</p>
  </a>
  <a class="format-card" href="https://docs.rs/facet-postcard">
    <div class="format-card__title">Postcard</div>
    <div class="format-card__crate">facet-postcard</div>
    <p class="format-card__desc">Postcard-compatible binary format</p>
  </a>
</div>

See the [format comparison matrix](@/format-crate-matrix.md) for detailed feature support.

## Next Steps

- Browse the [Showcases](@/learn/showcases/_index.md) to see facet in action
- Read [Why facet?](@/learn/why.md) if you're curious about the design philosophy
- Check the [Attributes Reference](@/learn/attributes.md) for all available options
- Join the [Discord](https://discord.gg/JhD7CwCJ8F) to ask questions
