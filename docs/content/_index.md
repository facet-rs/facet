+++
title = "facet"
insert_anchor_links = "heading"
+++

**Reflection for Rust.** One `#[derive(Facet)]` describes your type once — its
fields, variants, attributes, and shape. Everything else reads that single
description: serializers, a CLI parser, a diff engine, schema generators — and a
growing family of tools and languages built on top.

```rust
#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    #[facet(sensitive)]
    api_key: String,
}
```

From this one derive you get JSON/YAML/TOML/MessagePack, pretty-printing with
sensitive-field redaction, structural diffing, and runtime introspection — no
hand-written glue.

## Built on facet

The reflection core is the foundation. These are the products and libraries that
stand on it — each with its own docs.

<div class="section-grid">
<a class="section-card" href="/styx"><span>Styx →</span><small>A document language for mortals. Minimal punctuation, real types, one obvious meaning.</small></a>
<a class="section-card" href="/picante"><span>Picante →</span><small>Async incremental queries. Salsa-style memoization and dependency tracking, built for Tokio.</small></a>
<a class="section-card" href="/dibs"><span>Dibs →</span><small>Postgres toolkit. Schema as Rust, queries as Styx — typed end to end, migrations included.</small></a>
<a class="section-card" href="https://docs.rs/figue"><span>Figue</span><small>Type-safe CLI args, config files, and environment variables — from one derive.</small></a>
<a class="section-card" href="https://docs.rs/rediff"><span>Rediff</span><small>Structural diffing for Facet values, with detailed difference reports.</small></a>
<a class="section-card" href="https://docs.rs/strid"><span>Strid</span><small>Strongly-typed strings with far less boilerplate.</small></a>
<a class="section-card" href="https://docs.rs/fable"><span>Fable</span><small>A tiny typed language over Facet-reflected values, lowered toward Weavy IR.</small></a>
<a class="section-card" href="https://docs.rs/weavy"><span>Weavy</span><small>A shared lowered-program substrate for interpreters and copy-and-patch backends.</small></a>
<a class="section-card" href="https://docs.rs/vox"><span>Vox</span><small>Typed RPC — generated TypeScript types, a WebSocket transport, no schema duplication.</small></a>
</div>

## Use facet itself

- [Guide](@/guide/_index.md) — install a format crate, derive `Facet`, and ship with great diagnostics.
- [Ecosystem map](@/ecosystem/_index.md) — every facet crate: formats, schema codegen, CLI, diffing, pretty-printing.
- [Showcases](@/showcases/_index.md) — interactive examples for JSON, YAML, args, diffing, and more.
- [Reference](@/reference/_index.md) — the attributes catalog and format support matrix.
- [Extend](@/extend/_index.md) · [Contribute](@/contribute/_index.md) — build on reflection, or work on facet itself.

[GitHub](https://github.com/facet-rs/facet) · [crates.io](https://crates.io/crates/facet) · [docs.rs](https://docs.rs/facet) · [Discord](https://discord.gg/JhD7CwCJ8F)
