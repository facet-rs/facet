# facet-path

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-path/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-path.svg)](https://crates.io/crates/facet-path)
[![documentation](https://docs.rs/facet-path/badge.svg)](https://docs.rs/facet-path)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-path.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-path

Path tracking for navigating Facet type structures.

This crate provides lightweight path tracking that records navigation steps through a Facet type hierarchy. When an error occurs during serialization or deserialization, the path can be used to produce helpful error messages showing exactly where in the data structure the problem occurred.

## Features

- Lightweight `PathStep` enum that stores indices, not strings
- Reconstruct human-readable paths by replaying steps against a `Shape`
- Optional `pretty` feature for rich error rendering with `facet-pretty`

## Usage

```rust
use facet::Facet;
use facet_path::{Path, PathStep};

#[derive(Facet)]
struct Outer {
    items: Vec<Inner>,
}

#[derive(Facet)]
struct Inner {
    name: String,
    value: u32,
}

// Build a path during traversal
let mut path = Path::new(<Outer as Facet>::SHAPE);
path.push(PathStep::Field(0));      // "items"
path.push(PathStep::Index(2));       // [2]
path.push(PathStep::Field(0));      // "name"

// Format the path as a human-readable string
let formatted = path.format();
assert_eq!(formatted, "items[2].name");
```

## Feature Flags

- `std` (default): Enables standard library support
- `alloc`: Enables heap allocation without full std
- `pretty`: Enables rich error rendering with `facet-pretty`



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
