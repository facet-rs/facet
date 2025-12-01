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

- **Serialization** — JSON, YAML, TOML, KDL, MessagePack
- **Pretty-printing** — Colored output with sensitive field redaction
- **Diffing** — Structural comparison between values
- **Introspection** — Query type information at runtime

## Choose Your Path

<div class="guide-cards">
<div class="guide-card">
<h3 id="learn"><a href="/learn">Learn</a></h3>
<p class="tagline">I want to serialize my types</p>
<p class="description">For application developers using facet-json, facet-yaml, etc. Covers installation, attributes, and format-specific guides.</p>
</div>
<div class="guide-card">
<h3 id="extend"><a href="/extend">Extend</a></h3>
<p class="tagline">I want to build tools with facet</p>
<p class="description"><em>Coming soon</em> — For developers building format crates or tools using reflection (Shape, Peek, Partial).</p>
</div>
<div class="guide-card">
<h3 id="contribute"><a href="/contribute">Contribute</a></h3>
<p class="tagline">I want to work on facet itself</p>
<p class="description"><em>Coming soon</em> — Architecture, proc macro internals, vtables, and development setup.</p>
</div>
</div>

## Quick Links

- [Format Support Matrix](@/format-crate-matrix.md) — Feature comparison across format crates
- [Extension Attributes](@/extension-attributes.md) — Format-specific attribute namespaces
- [GitHub](https://github.com/facet-rs/facet) — Source code and issues
- [docs.rs](https://docs.rs/facet) — API documentation
