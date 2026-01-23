# facet-singularize

[![crates.io](https://img.shields.io/crates/v/facet-singularize.svg)](https://crates.io/crates/facet-singularize)
[![documentation](https://docs.rs/facet-singularize/badge.svg)](https://docs.rs/facet-singularize)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-singularize.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

This repository contains the DOM/XML island crates extracted from the main
[facet](https://github.com/facet-rs/facet) workspace.

These crates provide tree-based (DOM) deserialization for XML, SVG, and Atom formats,
using a different architecture than the streaming/JIT serialization formats.

## Workspace Contents

### Core Crates

| Crate | Description | Docs |
|-------|-------------|------|
| [facet-singularize](./facet-singularize) | Fast, no-regex English singularization | [![docs.rs](https://docs.rs/facet-singularize/badge.svg)](https://docs.rs/facet-singularize) |
| [facet-dom](./facet-dom) | Tree-based (DOM) deserializer for facet | [![docs.rs](https://docs.rs/facet-dom/badge.svg)](https://docs.rs/facet-dom) |
| [facet-xml](./facet-xml) | XML serialization and deserialization | [![docs.rs](https://docs.rs/facet-xml/badge.svg)](https://docs.rs/facet-xml) |

### Format-Specific Crates

| Crate | Description | Docs |
|-------|-------------|------|
| [facet-xml-node](./facet-xml-node) | Raw XML node types for schema-less XML | [![docs.rs](https://docs.rs/facet-xml-node/badge.svg)](https://docs.rs/facet-xml-node) |
| [facet-atom](./facet-atom) | Atom Syndication Format (RFC 4287) | [![docs.rs](https://docs.rs/facet-atom/badge.svg)](https://docs.rs/facet-atom) |
| [facet-svg](./facet-svg) | SVG (Scalable Vector Graphics) | [![docs.rs](https://docs.rs/facet-svg/badge.svg)](https://docs.rs/facet-svg) |

## Usage

Add the crates you need to your `Cargo.toml`:

```toml
[dependencies]
facet = "0.43"
facet-xml = "0.43"
```

Then derive `Facet` on your types and use `facet_xml::from_str` / `facet_xml::to_string`:

```rust
use facet::Facet;
use facet_xml::{from_str, to_string};

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}

let xml = r#"<person><name>Alice</name><age>30</age></person>"#;
let person: Person = from_str(xml).unwrap();
```

## Relationship to Main Facet Repository

These crates depend on the core facet crates (`facet`, `facet-core`, `facet-reflect`,
`facet-dessert`) from the main [facet-rs/facet](https://github.com/facet-rs/facet)
repository.

The extraction was done to:
- Reduce complexity of the main workspace
- Allow independent versioning and release cycles
- Separate the tree-based DOM architecture from streaming formats

## Part of the Facet Ecosystem

This crate is part of the [facet](https://facet.rs) ecosystem, providing reflection for Rust.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet-xml/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet-xml/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
