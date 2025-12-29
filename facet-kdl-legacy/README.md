# facet-kdl-legacy

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-kdl-legacy/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-kdl-legacy.svg)](https://crates.io/crates/facet-kdl-legacy)
[![documentation](https://docs.rs/facet-kdl-legacy/badge.svg)](https://docs.rs/facet-kdl-legacy)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-kdl-legacy.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-kdl

KDL serialization and deserialization for Facet types.

## Quick start

Add `facet-kdl` alongside your Facet types and derive `Facet`:

```rust
use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet, Debug, PartialEq)]
struct Config {
    #[facet(kdl::child)]
    server: Server,
}

#[derive(Facet, Debug, PartialEq)]
struct Server {
    #[facet(kdl::argument)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

fn main() -> Result<(), facet_kdl_legacy::KdlError> {
    let cfg: Config = facet_kdl_legacy::from_str(r#"server "localhost" port=8080"#)?;
    assert_eq!(cfg.server.port, 8080);

    let text = facet_kdl_legacy::to_string(&cfg)?;
    assert_eq!(text, "server \"localhost\" port=8080\n");
    Ok(())
}
```

## Common patterns

- `#[facet(kdl::child)]` for a single required child node, `#[facet(kdl::children)]` for lists/maps/sets of children.
- `#[facet(kdl::property)]` maps node properties (key/value pairs) to fields.
- `#[facet(kdl::arguments)]`/`#[facet(kdl::argument)]` read positional arguments on a node.
- `#[facet(flatten)]` merges nested structs/enums; the solver uses property/child presence to choose variants.
- `Spanned<T>` is supported: properties/arguments can be captured with `miette::SourceSpan` data.

## Multiple children fields

When a struct has a single `#[facet(kdl::children)]` field, all child nodes are collected into that field.

When a struct has **multiple** `#[facet(kdl::children)]` fields, nodes are routed based on matching
the node name to the singular form of the field name:

```rust
use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet, Debug)]
struct Config {
    #[facet(kdl::children, default)]
    dependencies: Vec<Dependency>,

    #[facet(kdl::children, default)]
    samples: Vec<Sample>,
}

#[derive(Facet, Debug)]
struct Dependency {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property)]
    version: String,
}

#[derive(Facet, Debug)]
struct Sample {
    #[facet(kdl::argument)]
    path: String,
}
```

With this KDL:

```kdl
dependency "serde" version="1.0"
sample "test.txt"
dependency "tokio" version="1.0"
sample "example.txt"
```

The nodes are routed based on name matching:
- `dependency` nodes → `dependencies` field (singular matches plural)
- `sample` nodes → `samples` field

Supported pluralization patterns:
- Simple `s`: `item` → `items`
- `ies` ending: `dependency` → `dependencies`
- `es` ending: `box` → `boxes`

Note: Use `#[facet(default)]` on children fields to allow them to be empty when no matching nodes are present.

## KDL syntax: arguments vs properties

A common source of confusion is the difference between **arguments** and **properties** in KDL:

```kdl
// Arguments are positional values after the node name
server "localhost" 8080

// Properties are key=value pairs
server host="localhost" port=8080

// You can mix both - arguments come first, then properties
server "localhost" port=8080
```

This matters for your struct definitions:

```rust
use facet::Facet;
use facet_kdl_legacy as kdl;

// For: server "localhost" port=8080
#[derive(Facet)]
struct Server {
    #[facet(kdl::argument)]  // captures "localhost"
    host: String,
    #[facet(kdl::property)]  // captures port=8080
    port: u16,
}
```

### Child nodes with arguments

A particularly common KDL pattern uses child nodes with arguments:

```kdl
config {
    name "my-app"
    version "1.0.0"
    debug true
}
```

Here, `name "my-app"` is a **child node** named `name` with an **argument** `"my-app"`.
This is *not* a property (which would be `name="my-app"`).

To deserialize this pattern, each child node needs its own struct with a `kdl::argument` field:

```rust
use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::child)]
    name: Name,
    #[facet(kdl::child)]
    version: Version,
    #[facet(kdl::child)]
    debug: Debug,
}

#[derive(Facet)]
struct Name {
    #[facet(kdl::argument)]
    value: String,
}

#[derive(Facet)]
struct Version {
    #[facet(kdl::argument)]
    value: String,
}

#[derive(Facet)]
struct Debug {
    #[facet(kdl::argument)]
    value: bool,
}
```

If you're getting "no matching argument field for value" errors, check whether your KDL
uses `name "value"` (child node with argument) vs `name="value"` (property) syntax.

## Feature flags

- `default`/`std`: enables `std` for dependencies.
- `alloc`: `no_std` builds with `alloc` only.

## Error reporting

Errors use `miette` spans where possible, so diagnostics can point back to the offending KDL source.



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
