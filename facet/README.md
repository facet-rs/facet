
# facet

[![experimental](https://img.shields.io/badge/status-experimental-yellow)](https://github.com/fasterthanlime/facet)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/facet.svg)](https://crates.io/crates/facet)
[![documentation](https://docs.rs/facet/badge.svg)](https://docs.rs/facet)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet.svg)](./LICENSE)

Thanks to all individual and corporate sponsors, without whom this work could not exist:

<p> <a href="https://ko-fi.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-light.svg" height="40" alt="Ko-fi">
    </picture>
</a> <a href="https://github.com/sponsors/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-light.svg" height="40" alt="GitHub Sponsors">
    </picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> &nbsp; &nbsp; &nbsp; &nbsp; &nbsp; <a href="https://zed.dev">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-light.svg" height="40" alt="Zed">
    </picture>
</a> </p>
             

facet provides runtime reflection for Rust.

Any type that implements `Facet` trait returns a `Shape`, which describes:

  * The memory layout of the type
  * Its definition: struct fields, underlying type for newtypes, etc.
  * How to drop it in place

The `Poke` type is able to allocate (or work from a `&mut MaybeUninit<T>`)
any Facet type, and gradually initialize its fields — until the fully-built
value is moved out of the partial.

The `Peek` type helps perform read operations on any Facet type.

It comes with a derive macro that uses [unsynn](https://crates.io/crates/unsynn)
for speed of compilation.

The core crates, `facet-trait`, `facet-types` etc. are nostd-friendly.

## Ecosystem

The main `facet` crate re-exports symbols from:
- [facet-trait](https://github.com/facet-rs/facet/tree/main/facet-trait), which defines the main `Facet` trait and implements it for foreign types (mostly `libstd`)
- [facet-types](https://github.com/facet-rs/facet/tree/main/facet-types), which defines the `Shape` struct, along with various vtables and the whole `Def` tree
- [facet-opaque](https://github.com/facet-rs/facet/tree/main/facet-opaque), which provides helpers around type-erased pointers like
  `OpaqueUninit`, `OpaqueConst`, `Opaque`
- [facet-derive](https://github.com/facet-rs/facet/tree/main/facet-derive), which implements the `Facet` derive attribute as a fast/light proc macro powered by [unsynn](https://docs.rs/unsynn)
- [facet-spez](https://github.com/facet-rs/facet/tree/main/facet-spez), which implements an autoderef specialization trick needed for `facet-derive`
- [facet-peek](https://github.com/facet-rs/facet/tree/main/facet-peek), which allows reading arbitrary `Facet` types
- [facet-poke](https://github.com/facet-rs/facet/tree/main/facet-poke), which allows building/altering arbitrary `Facet` types

facet supports deserialization from multiple data formats through dedicated crates:

- [facet-json](https://github.com/facet-rs/facet/tree/main/facet-json): JSON deserialization
- [facet-yaml](https://github.com/facet-rs/facet/tree/main/facet-yaml): YAML deserialization
- [facet-msgpack](https://github.com/facet-rs/facet/tree/main/facet-msgpack): MessagePack deserialization
- [facet-urlencoded](https://github.com/facet-rs/facet/tree/main/facet-urlencoded): URL-encoded form data deserialization

Additionally:

- [facet-pretty](https://github.com/facet-rs/facet/tree/main/facet-pretty) is able to pretty-print Facet types.
- [facet-codegen](https://github.com/facet-rs/facet/tree/main/facet-codegen) is internal and generates some of the code of `facet-core`

### Example usage

[facet-json](https://github.com/facet-rs/facet/tree/main/facet-json) is the one that gets updated first — look at it.


## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.