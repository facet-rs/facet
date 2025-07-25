<h1>
<picture>
    <source type="image/webp" media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-dark.webp">
    <source type="image/png" media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-dark.png">
    <source type="image/webp" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-light.webp">
    <img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-light.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture>
</h1>

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet.svg)](https://crates.io/crates/facet)
[![documentation](https://docs.rs/facet/badge.svg)](https://docs.rs/facet)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

_Logo by [Misiasart](https://misiasart.com/)_

Thanks to all individual and corporate sponsors, without whom this work could not exist:

<p> <a href="https://ko-fi.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/kofi-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/kofi-light.svg" height="40" alt="Ko-fi">
</picture>
</a> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
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


facet provides reflection for Rust: it gives types a `SHAPE` associated
const with details on the layout, fields, doc comments, attributes, etc.

It can be used for many things, from (de)serialization to pretty-printing,
rich debuggers, CLI parsing, reflection in templating engines, code
generation, etc.

See <https://facet.rs> for details.

## Workspace contents

The main `facet` crate re-exports symbols from:

- [facet-core](https://github.com/facet-rs/facet/tree/main/facet-core), which defines the main components:
  - The `Facet` trait and implementations for foreign types (mostly `libstd`)
  - The `Shape` struct along with various vtables and the whole `Def` tree
  - Type-erased pointer helpers like `PtrUninit`, `PtrConst`, and `Opaque`
  - Autoderef specialization trick needed for `facet-macros`
- [facet-macros](https://github.com/facet-rs/facet/tree/main/facet-macros), which implements the `Facet` derive attribute as a fast/light proc macro powered by [unsynn](https://docs.rs/unsynn)

For struct manipulation and reflection, we have:

- [facet-reflect](https://github.com/facet-rs/facet/tree/main/facet-reflect),
  allows building values of arbitrary shapes in safe code, respecting invariants.
  It also allows peeking at existing values.

Internal crates include:

- [facet-testhelpers](https://github.com/facet-rs/facet/tree/main/facet-testhelpers) a simple log logger and color-backtrace configured with the lightweight btparse backend

## Ecosystem

Various crates live under the <https://github.com/facet-rs> umbrella, and their
repositories are kept somewhat-consistent through [facet-dev](https://github.com/facet-rs/facet-dev).

Crates are in various states of progress, buyer beware!

In terms of data formats, we have:

- [facet-json](https://github.com/facet-rs/facet-json): JSON format support
- [facet-toml](https://github.com/facet-rs/facet-toml): TOML format support
- [facet-yaml](https://github.com/facet-rs/facet-yaml): YAML format support
- [facet-msgpack](https://github.com/facet-rs/facet-msgpack): MessagePack deserialization
- [facet-asn1](https://github.com/facet-rs/facet-asn1): ASN.1 format support
- [facet-xdr](https://github.com/facet-rs/facet-xdr): XDR format support
- [facet-kdl](https://github.com/facet-rs/facet-kdl): KDL format support (non-functional so far)

Still adjacent to serialization/deserialization, we have:

- [facet-urlencoded](https://github.com/facet-rs/facet-urlencoded): URL-encoded form data deserialization
- [facet-args](https://github.com/facet-rs/facet-args): CLI arguments (a-la clap)

As far as utilities go:

- [facet-pretty](https://github.com/facet-rs/facet-pretty) is able to pretty-print Facet types.
- [facet-serialize](https://github.com/facet-rs/facet-serialize) provides generic iterative serialization facilities
- [facet-deserialize](https://github.com/facet-rs/facet-deserialize) provides generic iterative deserialization facilities

And the less developed:

- [facet-inspect](https://github.com/facet-rs/facet-inspect): Provide utilities to inspect the content of a Facet object.
- [facet-diff](https://github.com/facet-rs/facet-diff): Provides diffing capabilities for Facet types.

## Extended cinematic universe

Some crates are developed completely independently from the facet org:

- [facet-v8](https://github.com/simonask/facet-v8) provides an experimental Facet/v8 integration
- [facet-openapi](https://github.com/ThouCheese/facet-openapi) (experimental) Generates OpenAPI definitions from types that implement Facet
- [facet_generate](https://github.com/redbadger/facet-generate) reflects Facet types into Java, Swift and TypeScript
- [multi-array-list](https://lib.rs/crates/multi-array-list) provides an experimental `MultiArrayList` type

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
