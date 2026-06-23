+++
title = "strid"
weight = 20
insert_anchor_links = "heading"
+++

`strid` makes strongly-typed string identifiers cheap to define and use. It is
a Facet-adjacent building block for APIs that should not pass unrelated string
values through the same type.

The `#[braid]` macro generates an owned string wrapper plus its borrowed form:

```rust
use strid::braid;

#[braid]
pub struct DatabaseName;

fn use_database(name: &DatabaseNameRef) {
    println!("{}", name.as_str());
}
```

## Crates

| Crate | What it does |
|-------|--------------|
| [`strid`](https://docs.rs/strid) | Public macro entry point and generated string wrapper support. |
| [`strid-macros`](https://docs.rs/strid-macros) | Procedural macro implementation for `strid`. |
| [`strid-examples`](https://docs.rs/strid-examples) | Published examples for custom string storage, validation, and normalization. |

## Facet integration

`strid` re-exports `facet` and can generate Facet implementations for strongly
typed string wrappers, including wrappers backed by string-like crates such as
`bytestring`, `compact_str`, and `smartstring` when the corresponding Facet
support is enabled.

## Source

`strid` now lives in the Facet monorepo:

- [`strid`](https://github.com/facet-rs/facet/tree/main/strid)
- [`strid-macros`](https://github.com/facet-rs/facet/tree/main/strid-macros)
- [`strid-examples`](https://github.com/facet-rs/facet/tree/main/strid-examples)
