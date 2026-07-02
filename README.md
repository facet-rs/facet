# facet

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-core/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-core.svg)](https://crates.io/crates/facet-core)
[![documentation](https://docs.rs/facet-core/badge.svg)](https://docs.rs/facet-core)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-core.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

`facet` is an entire ecosystem of Rust crates built on top of reflection.

The core facet crates give types a [`SHAPE`](Facet::SHAPE) associated const with
details on the kind (struct, enum, tuple?), layout (size, alignment), fields,
doc comments, arbitrary attributes, along with 

From there, `facet-reflect` allows reading from existing values, building new
values from scratch, and even mutating existing values in-place if they're plain
old data.

A rich (de)serialization ecosystem is built on top of these, for formats like
JSON, TOML, YAML, MsgPack, Postcard, ASN1, XDR, CSV, XML, but also facet-native
(ie. designed by the same authors and leveraging some capabilities that would
be hard to get elsewhere) like [styx](facet-styx/) (a human-oriented document
language you'd use in place of YAML or TOML) and [phon](phon/) (a schema-aware
binary format that comes in self-describing form _and_ in compact form).

Inserting or fetching records from database is essentially (de)serialization
again, so the facet ecosystem also includes adapters for sqlite and Postgres
(via [dibs](dibs/), which does a little more than just data binding).

Want two programs to talk to each other? [vox](vox/) has you covered: an RPC
system built on top of the Phon binary format, which purports to support forwards
and backwards compatibility, although nobody's built the "semver checks" tooling
for it yet so, PRs welcome.

Reflection has a cost: facet-json used to be 5-7x slower than serde-json. In
comes [weavy](weavy/), an IR target that any crate can lower to, using their own
intrinsics, for which they can provide native stencils. On platforms that
support it, weavy uses a copy-patch technique to assemble native code for much
faster (citation needed) execution still. Not an option on iPhone, and generally
a memory safety liability.

For syntax highlighting, I (Amos) was a bit annoyed that
[arborium](https://github.com/bearcove/arborium) (a tree-sitter grammar
distribution) required a C toolchain, so I made [snark](snark/), a
tree-sitter-compatible parser framework, which lowers to weavy, has JIT
support, and will happily codegen an AST for you (into which it can parse
your language) given a few extra annotations.

## Website note

The <https://facet.rs> website has a lot of information about a lot of the
ecosystem but it's unfortunately not super reliable as LLMs have been doing
too much of the writing (on account on Amos being burned out). This is being
slowly repaired. Bear with us.

## Workspace contents

The main `facet` crate re-exports symbols from:

- [facet-core](https://github.com/facet-rs/facet/tree/main/facet-core), which defines the main components:
  - The [`Facet`] trait and implementations for foreign types (mostly `libstd`)
  - The [`Shape`] struct along with various vtables and the whole [`Def`] tree
  - Type-erased pointer helpers like [`PtrUninit`], [`PtrConst`], and [`Opaque`]
  - Autoderef specialization trick needed for `facet-macros`
- [facet-macros](https://github.com/facet-rs/facet/tree/main/facet-macros), which implements the [`Facet`] derive attribute as a fast/light proc macro powered by [unsynn](https://docs.rs/unsynn)

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

- [facet-json](https://github.com/facet-rs/facet/tree/main/facet-json): JSON format support
- [facet-toml](https://github.com/facet-rs/facet/tree/main/facet-toml): TOML format support
- [facet-yaml](https://github.com/facet-rs/facet/tree/main/facet-yaml): YAML format support
- [facet-msgpack](https://github.com/facet-rs/facet/tree/main/facet-msgpack): MessagePack deserialization
- [facet-asn1](https://github.com/facet-rs/facet/tree/main/facet-asn1): ASN.1 format support
- [facet-xdr](https://github.com/facet-rs/facet/tree/main/facet-xdr): XDR format support
- [facet-csv](https://github.com/facet-rs/facet/tree/main/facet-csv): CSV format support
- [facet-xml](https://github.com/facet-rs/facet/tree/main/facet-xml): XML format support

Still adjacent to serialization/deserialization, we have:

- [facet-urlencoded](https://github.com/facet-rs/facet/tree/main/facet-urlencoded): URL-encoded form data deserialization
- [figue](https://github.com/bearcove/figue): CLI arguments, config files, and environment variables (external crate)

As far as utilities go:

- [facet-value](https://github.com/facet-rs/facet/tree/main/facet-value): Memory-efficient dynamic value type, supporting JSON-like data plus bytes
- [facet-pretty](https://github.com/facet-rs/facet/tree/main/facet-pretty): Pretty-print Facet types
- [facet-diff](https://github.com/facet-rs/facet/tree/main/facet-diff): Diffing capabilities for Facet types
- [facet-assert](https://github.com/facet-rs/facet/tree/main/facet-assert): Pretty assertions for Facet types (no PartialEq required)
- [facet-serialize](https://github.com/facet-rs/facet-serialize): Generic iterative serialization facilities
- [facet-deserialize](https://github.com/facet-rs/facet-deserialize): Generic iterative deserialization facilities

And the less developed:

- [facet-inspect](https://github.com/facet-rs/facet-inspect): Utilities to inspect the content of a Facet object

## Previously separate crates

These crates previously lived in separate repositories and now live in this monorepo:

- [facet-xml](https://github.com/facet-rs/facet/tree/main/facet-xml): XML/DOM ecosystem (includes facet-xml, facet-dom, facet-svg, facet-atom, facet-xml-node, facet-singularize)
- [facet-axum](https://github.com/facet-rs/facet/tree/main/facet-axum): Axum web framework integration

## Extended cinematic universe

Some crates are developed completely independently from the facet org:

- [facet-v8](https://github.com/simonask/facet-v8) provides an experimental Facet/v8 integration
- [facet-openapi](https://github.com/ThouCheese/facet-openapi) (experimental) Generates OpenAPI definitions from types that implement Facet
- [facet_generate](https://github.com/redbadger/facet-generate) reflects Facet types into Java, Swift and TypeScript
- [multi-array-list](https://lib.rs/crates/multi-array-list) provides an experimental `MultiArrayList` type

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

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
