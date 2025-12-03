+++
title = "structstruck"
weight = 6
insert_anchor_links = "heading"
+++

[`structstruck`](https://crates.io/crates/structstruck) lets you declare nested structs inline instead of defining each one separately.

## Without structstruck

```rust
use facet::Facet;

#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    limits: Limits,
    features: Option<Features>,
}

#[derive(Facet)]
struct Limits {
    connections: u32,
    requests_per_second: u32,
}

#[derive(Facet)]
struct Features {
    tracing: bool,
    metrics: bool,
}
```

## With structstruck

```rust
structstruck::strike! {
    #[structstruck::each[derive(facet::Facet)]]
    struct Config {
        name: String,
        port: u16,
        limits: struct {
            connections: u32,
            requests_per_second: u32,
        },
        features: Option<struct {
            tracing: bool,
            metrics: bool,
        }>,
    }
}
```

Same result, less scrolling. The `each[derive(...)]` applies to all generated types.
