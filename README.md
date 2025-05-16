<h1>
<picture>
    <source type="image/webp" media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-dark.webp">
    <source type="image/png" media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-dark.png">
    <source type="image/webp" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-light.webp">
    <img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-light.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture>
</h1>

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/facet.svg)](https://crates.io/crates/facet)
[![documentation](https://docs.rs/facet/badge.svg)](https://docs.rs/facet)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet.svg)](./LICENSE)

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

## Known issues and limitations

We have the occasional soundness issue, see the [soundness tag](https://github.com/facet-rs/facet/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22%F0%9F%8E%BA%20soundness%22).
Those are prioritized and fixed as soon as possible.

Format crates like `facet-json` are still very much incomplete, and will
probably always be slower than the `serde-*` equivalent. Reflection has
advantages in terms of flexibility, ergonomics and compile time but has
a runtime cost compared to "monomorphize all the things".

(Note: with codegen, one could get the best of both worlds. To be researched
more)

Some format crates (like `facet-toml`) first deserialize to a DOM, and _then_
to partial structs — this is not as efficient as it could be, but it's a good
first step.

`type_eq` is not const, thus preventing many useful things in `const fn`, see
<https://github.com/facet-rs/facet/issues/98>. Language changes are needed
to address that.

The design of arbitrary attributes is still in flux, see:

  * <https://github.com/facet-rs/facet/issues/108#issue-2986732759>
  * <https://github.com/facet-rs/facet/issues/151#issuecomment-2820476301>

There isn't a comparison between facet-reflect and bevy-reflect available right
now— [this is by design](https://github.com/facet-rs/facet/issues/298). We're
letting facet develop on its own for a while; we'll make a detailed comparison
once things have settled.

There are more issues and limitations of course, but this should get you started.

## Ecosystem

The core crates, `facet-trait`, `facet-types` etc. are no_std-friendly.

The main `facet` crate re-exports symbols from:

- [facet-core](https://github.com/facet-rs/facet/tree/main/facet-core), which defines the main components:
  - The `Facet` trait and implementations for foreign types (mostly `libstd`)
  - The `Shape` struct along with various vtables and the whole `Def` tree
  - Type-erased pointer helpers like `PtrUninit`, `PtrConst`, and `Opaque`
  - Autoderef specialization trick needed for `facet-derive`
- [facet-derive](https://github.com/facet-rs/facet/tree/main/facet-derive), which implements the `Facet` derive attribute as a fast/light proc macro powered by [unsynn](https://docs.rs/unsynn)

For struct manipulation and reflection, the following is available:

- [facet-reflect](https://github.com/facet-rs/facet/tree/main/facet-reflect),
  allows building values of arbitrary shapes in safe code, respecting invariants.
  It also allows peeking at existing values.
- [facet-pretty](https://github.com/facet-rs/facet/tree/main/facet-pretty) is able to pretty-print Facet types.

facet supports deserialization from multiple data formats through dedicated crates:

- [facet-json](https://github.com/facet-rs/facet/tree/main/facet-json): JSON deserialization
- [facet-yaml](https://github.com/facet-rs/facet/tree/main/facet-yaml): YAML deserialization
- [facet-toml](https://github.com/facet-rs/facet/tree/main/facet-toml): TOML deserialization
- [facet-msgpack](https://github.com/facet-rs/facet/tree/main/facet-msgpack): MessagePack deserialization
- [facet-urlencoded](https://github.com/facet-rs/facet/tree/main/facet-urlencoded): URL-encoded form data deserialization
- [facet-args](https://github.com/facet-rs/facet/tree/main/facet-args): CLI arguments (a-la clap)

Internal crates include:

- [facet-codegen](https://github.com/facet-rs/facet/tree/main/facet-codegen) is internal and generates some of the code of `facet-core`
- [facet-testhelpers](https://github.com/facet-rs/facet/tree/main/facet-testhelpers) a simpler log logger and color-backtrace configured with the lightweight btparse backend

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
