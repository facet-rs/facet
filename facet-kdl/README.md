# facet-kdl

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-kdl/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-kdl.svg)](https://crates.io/crates/facet-kdl)
[![documentation](https://docs.rs/facet-kdl/badge.svg)](https://docs.rs/facet-kdl)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-kdl.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-kdl

KDL serialization and deserialization for Facet types.

## Quick start

Add `facet-kdl` alongside your Facet types and derive `Facet`:

```rust
use facet::Facet;
use facet_kdl as kdl;

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

fn main() -> Result<(), facet_kdl::KdlError> {
    let cfg: Config = facet_kdl::from_str(r#"server "localhost" port=8080"#)?;
    assert_eq!(cfg.server.port, 8080);

    let text = facet_kdl::to_string(&cfg)?;
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

## Feature flags

- `default`/`std`: enables `std` for dependencies.
- `alloc`: `no_std` builds with `alloc` only.

## Error reporting

Errors use `miette` spans where possible, so diagnostics can point back to the offending KDL source.

## License

MIT OR Apache-2.0, at your option.

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
