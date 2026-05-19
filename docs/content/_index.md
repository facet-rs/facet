+++
title = "facet"
insert_anchor_links = "heading"
+++

**facet** is a reflection library for Rust. One derive macro gives you serialization, pretty-printing, diffing, and more.

```rust
#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    #[facet(sensitive)]
    api_key: String,
}
```

From this single derive, you get:

- **Serialization** — JSON, YAML, TOML, MessagePack
- **Pretty-printing** — Colored output with sensitive field redaction
- **Diffing** — Structural comparison between values
- **Introspection** — Query type information at runtime

## Choose your path

<div class="guide-cards">
<a class="guide-card" href="/guide">
  <div class="guide-card__icon"><img src="/icons/learn.svg" alt="" loading="lazy"></div>
  <h3 id="guide">Guide</h3>
  <p class="tagline">Use facet in your app</p>
  <p class="description">Install a format crate, derive <code>Facet</code>, configure attributes, and ship with great diagnostics.</p>
</a>
<a class="guide-card" href="/ecosystem">
  <div class="guide-card__icon"><img src="/icons/ecosystem.svg" alt="" loading="lazy"></div>
  <h3 id="ecosystem">Ecosystem</h3>
  <p class="tagline">See the whole map</p>
  <p class="description">Every facet-related crate — formats, schema codegen, CLI, diffing, pretty-printing — with links to docs and source.</p>
</a>
<a class="guide-card" href="/showcases">
  <div class="guide-card__icon"><img src="/icons/showcases.svg" alt="" loading="lazy"></div>
  <h3 id="showcases">Showcases</h3>
  <p class="tagline">See it in action</p>
  <p class="description">Interactive examples for JSON, YAML, args parsing, pretty-printing, diffing, and more.</p>
</a>
<a class="guide-card" href="/reference">
  <div class="guide-card__icon"><img src="/icons/reference.svg" alt="" loading="lazy"></div>
  <h3 id="reference">Reference</h3>
  <p class="tagline">Look it up fast</p>
  <p class="description">Attributes catalog and format matrix.</p>
</a>
<a class="guide-card" href="/extend">
  <div class="guide-card__icon"><img src="/icons/extend.svg" alt="" loading="lazy"></div>
  <h3 id="extend">Extend</h3>
  <p class="tagline">Build on reflection</p>
  <p class="description">Create extension attributes, read data with <code>Peek</code>, build values with <code>Partial</code>, and architect new format crates.</p>
</a>
<a class="guide-card" href="/contribute">
  <div class="guide-card__icon"><img src="/icons/contribute.svg" alt="" loading="lazy"></div>
  <h3 id="contribute">Contribute</h3>
  <p class="tagline">Work on facet itself</p>
  <p class="description">Architecture, derive internals, vtables, unsafe invariants, and the contributor workflow.</p>
</a>
</div>

## Quick links

- [Getting Started](@/guide/getting-started.md) — Install, derive, ship
- [Ecosystem map](@/ecosystem/_index.md) — Every facet crate and where to find it
- [JSON guide](@/guide/json.md) — The flagship workflow
- [Format Support Matrix](@/reference/format-crate-matrix/_index.md) — Feature comparison across format crates
- [GitHub](https://github.com/facet-rs/facet) · [crates.io](https://crates.io/crates/facet) · [docs.rs](https://docs.rs/facet) · [Discord](https://discord.gg/JhD7CwCJ8F)
