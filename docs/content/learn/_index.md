+++
title = "Learn"
sort_by = "weight"
insert_anchor_links = "heading"
+++

Learn how to use facet for serialization, deserialization, and more.

**New to facet?** Start with [Why facet?](@/learn/why.md) to understand what facet is and how it differs from serde.

## Getting Started

Add facet to your project:

```toml
[dependencies]
facet = "1"
facet-json = "1"  # or facet-yaml, facet-toml, facet-kdl, etc.
```

Derive `Facet` on your types:

```rust
use facet::Facet;

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}
```

Serialize and deserialize:

```rust
use facet_json::{from_str, to_string};

let person = Person { name: "Alice".into(), age: 30 };
let json = to_string(&person);  // {"name":"Alice","age":30}

let parsed: Person = from_str(&json)?;
```

## Guides

- [Why facet?](@/learn/why.md) — What facet is and how it differs from serde
- [Attributes Reference](@/learn/attributes.md) — Complete reference for `#[facet(...)]` attributes
- [Comparison with serde](@/learn/migration/_index.md) — Attribute comparison and migration tips
- [Showcases](@/learn/showcases/_index.md) — Interactive examples for each format
- [FAQ](@/learn/faq.md) — Frequently asked questions

## Format Crates

| Format | Crate | Description |
|--------|-------|-------------|
| JSON | `facet-json` | JSON serialization/deserialization |
| YAML | `facet-yaml` | YAML support |
| TOML | `facet-toml` | TOML configuration files |
| KDL | `facet-kdl` | KDL document language |
| MessagePack | `facet-msgpack` | Binary format |

See the [format comparison matrix](@/format-crate-matrix.md) for detailed feature support.

## Next Steps

- Browse the [Showcases](@/learn/showcases/_index.md) to see facet in action
- Read [Why facet?](@/learn/why.md) if you're curious about the design philosophy
- Check the [Attributes Reference](@/learn/attributes.md) for all available options
- Join the [Discord](https://discord.gg/JhD7CwCJ8F) to ask questions
