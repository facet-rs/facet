# facet-dom

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-dom/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-dom.svg)](https://crates.io/crates/facet-dom)
[![documentation](https://docs.rs/facet-dom/badge.svg)](https://docs.rs/facet-dom)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-dom.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Tree-based (DOM) serialization and deserialization for facet.

## Overview

This crate provides the core serializers and deserializers for tree-structured
documents like HTML and XML. It handles the DOM-specific concerns that don't
apply to flat formats like JSON:

- **Tag names**: Elements have names (`<div>`, `<person>`)
- **Attributes**: Key-value pairs on elements (`id="main"`, `class="active"`)
- **Mixed content**: Text and child elements can be interleaved

## Architecture

`facet-dom` sits between the format-specific parsers (`facet-html`, `facet-xml`)
and the generic facet reflection system:

```text
facet-html / facet-xml
         ↓
     facet-dom  (DOM events: StartElement, Attribute, Text, EndElement)
         ↓
   facet-reflect (Peek/Poke)
         ↓
    Your Rust types
```

## Key Types

### DomDeserializer

Consumes DOM events and builds Rust values:

```rust
use facet_dom::{DomDeserializer, DomParser};

// Parser emits events, deserializer consumes them
let parser: impl DomParser = /* ... */;
let value: MyType = DomDeserializer::new(parser).deserialize()?;
```

### DomSerializer

Converts Rust values to DOM events for output.

## Field Mappings

The deserializer maps DOM concepts to Rust types using facet attributes:

| DOM Concept | Rust Representation | Attribute |
|-------------|---------------------|-----------|
| Tag name | Struct variant | `#[facet(rename = "tag")]` |
| Attribute | Field | `#[facet(html::attribute)]` |
| Text content | String field | `#[facet(html::text)]` |
| Child elements | Vec field | `#[facet(html::elements)]` |

## Naming Conventions

Handles automatic case conversion between DOM naming (kebab-case) and
Rust naming (snake_case), plus singularization for collection fields.

## LLM contribution policy

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
