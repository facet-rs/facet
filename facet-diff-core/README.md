# facet-diff-core

[![codecov](https://codecov.io/gh/facet-rs/facet/graph/badge.svg)](https://codecov.io/gh/facet-rs/facet)
[![crates.io](https://img.shields.io/crates/v/facet-diff-core.svg)](https://crates.io/crates/facet-diff-core)
[![documentation](https://docs.rs/facet-diff-core/badge.svg)](https://docs.rs/facet-diff-core)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-diff-core.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)


# {{crate}}

Core types and helpers for diff rendering in Facet.

This crate provides shared infrastructure for rendering diffs across different
serialization formats (XML, JSON, TOML, etc.).

## Symbols

| Symbol | Meaning | Color |
|--------|---------|-------|
| `-` | Deleted | Red |
| `+` | Inserted | Green |
| `←` | Moved from here | Blue |
| `→` | Moved to here | Blue |

## Value-only coloring

Keys/field names stay neutral, only the changed VALUES are colored:

```text
- fill="red"    <- "red" is red, "fill=" is white
+ fill="blue"   <- "blue" is green, "fill=" is white
```

## Usage

This crate is typically used by serializers (facet-xml, facet-json, etc.) to
render diffs consistently. You probably want to use `facet-diff` directly
instead of this crate.

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
