# facet-dom

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

## Part of the Facet Ecosystem

This crate is part of the [facet](https://facet.rs) ecosystem, providing reflection for Rust.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet-xml/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet-xml/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
