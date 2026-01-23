# facet-xml-node

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-xml-node/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-xml-node.svg)](https://crates.io/crates/facet-xml-node)
[![documentation](https://docs.rs/facet-xml-node/badge.svg)](https://docs.rs/facet-xml-node)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-xml-node.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Raw XML node types—represent arbitrary XML without a schema.

## Overview

This crate provides generic XML types (`Element`, `Content`) that can represent
any XML document without needing predefined Rust structs. It's useful when you
need to parse XML dynamically or work with XML of unknown structure.

## Types

### Element

Captures any XML element with its tag name, attributes, and children:

```rust
use facet_xml_node::Element;

let xml = r#"<item id="42" status="active">Hello <b>world</b></item>"#;
let element: Element = facet_xml::from_str(xml)?;

assert_eq!(element.tag, "item");
assert_eq!(element.attrs.get("id"), Some(&"42".to_string()));
```

### Content

Represents either text or a child element:

```rust
use facet_xml_node::{Element, Content};

for child in &element.children {
    match child {
        Content::Text(t) => println!("Text: {}", t),
        Content::Element(e) => println!("Element: <{}>", e.tag),
    }
}
```

## Use Cases

- Parsing XML of unknown or variable structure
- Building XML transformers or validators
- Bridging between typed and untyped XML representations
- Testing and debugging XML serialization

## Comparison

| Approach | Use Case |
|----------|----------|
| Typed structs with `#[derive(Facet)]` | Known XML schema, compile-time safety |
| `facet-xml-node::Element` | Unknown/dynamic XML, runtime flexibility |

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
