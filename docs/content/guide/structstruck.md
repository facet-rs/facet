+++
title = "structstruck"
weight = 6
insert_anchor_links = "heading"
+++

[`structstruck`](https://crates.io/crates/structstruck) lets you declare nested structs inline instead of defining each one separately.

## Without structstruck

```rust,noexec
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

```rust,noexec
structstruck::strike! {
    #[structstruck::each[derive(facet::Facet)]]
    struct Config {
        name: String,
        port: u16,
        limits: struct Limits {
            connections: u32,
            requests_per_second: u32,
        },
        features: Option<struct Features {
            tracing: bool,
            metrics: bool,
        }>,
    }
}
```

Same result, less scrolling. The `each[derive(...)]` applies to all generated types.

You can omit the struct name and it will be inferred from the field name (`limits: struct { ... }` becomes `Limits`).
