+++
title = "facet"
description = "Reflection for Rust: derive Facet once, then serialize, inspect, diff, configure, and build tooling from one type description."
insert_anchor_links = "heading"
+++

**Reflection for Rust.** Derive `Facet` once, and your type can describe itself:
fields, variants, attributes, shape, and enough structure for other tools to do
useful work.

```rust
use facet::Facet;

#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    #[facet(sensitive)]
    api_key: String,
}
```

That one derive can power JSON, YAML, TOML, MessagePack, pretty-printing with
sensitive-field redaction, structural diffing, runtime introspection, CLI
parsing, schema generation, and more. The fun part: each tool reads the same
source of truth.

## Start here

- [Guide](/guide/) — derive `Facet`, add a format crate, and ship with helpful diagnostics.
- [Ecosystem map](/ecosystem/) — formats, schema generators, config, diffing, UI, and building blocks.
- [Showcases](/showcases/) — small examples for derives, pretty-printing, diffing, and HTML.
- [Reference](/reference/) — attributes, extension points, and the format support matrix.

## Built on facet

The reflection core is the foundation; these projects build useful things on
top of it. Pick the one that matches the job in front of you.

<div class="section-grid">
<a class="section-card" href="/facet-json/guide/"><span>facet-json →</span><small>Serialize and deserialize Facet types as JSON, with span-aware errors.</small></a>
<a class="section-card" href="/figue/guide/"><span>figue →</span><small>Read CLI args, environment variables, config files, and defaults into one type.</small></a>
<a class="section-card" href="/facet-pretty/guide/"><span>facet-pretty →</span><small>Pretty-print Facet values with structure, color, and sensitive-field redaction.</small></a>
<a class="section-card" href="/rediff/"><span>rediff →</span><small>Compare Facet values structurally and get path-aware difference reports.</small></a>
<a class="section-card" href="/strid/"><span>strid →</span><small>Define strongly-typed string identifiers without the usual wrapper boilerplate.</small></a>
<a class="section-card" href="/facet-axum/"><span>facet-axum →</span><small>Use Facet-backed extractors and responses at axum web boundaries.</small></a>
<a class="section-card" href="/rusqlite-facet/"><span>rusqlite-facet →</span><small>Bind SQLite query parameters and map rows through Facet-reflected structs.</small></a>
<a class="section-card" href="/facet-cargo-toml/"><span>facet-cargo-toml →</span><small>Parse Cargo manifests and lockfiles into typed Rust models.</small></a>
<a class="section-card" href="/styx/"><span>Styx →</span><small>Write typed documents with minimal punctuation and one obvious meaning.</small></a>
<a class="section-card" href="/picante/"><span>picante →</span><small>Build Tokio-first incremental queries with memoization and dependency tracking.</small></a>
<a class="section-card" href="/dibs/guide/"><span>dibs →</span><small>Model Postgres schemas as Rust and queries as Styx, with migrations included.</small></a>
<a class="section-card" href="/fable/"><span>fable →</span><small>Run a tiny typed language over Facet-reflected values, lowered toward Weavy IR.</small></a>
<a class="section-card" href="/weavy/"><span>weavy →</span><small>Share lowered programs across interpreters and copy-and-patch backends.</small></a>
<a class="section-card" href="/vox/"><span>vox →</span><small>Define Rust-native RPC services with cross-language codegen and transports.</small></a>
</div>

## Use facet itself

- [Guide](/guide/) — install a format crate, derive `Facet`, and ship with great diagnostics.
- [Ecosystem map](/ecosystem/) — every facet crate: formats, schema codegen, CLI, diffing, pretty-printing.
- [Showcases](/showcases/) — interactive examples for JSON, YAML, args, diffing, and more.
- [Reference](/reference/) — the attributes catalog and format support matrix.
- [Extend](/extend/) · [Contribute](/contribute/) — build on reflection, or work on facet itself.

[GitHub](https://github.com/facet-rs/facet) · [crates.io](https://crates.io/crates/facet) · [docs.rs](https://docs.rs/facet) · [Discord](https://discord.gg/JhD7CwCJ8F)
