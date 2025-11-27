# facet-xdr

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-xdr/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-xdr.svg)](https://crates.io/crates/facet-xdr)
[![documentation](https://docs.rs/facet-xdr/badge.svg)](https://docs.rs/facet-xdr)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-xdr.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-xdr

An XDR serializer and deserializer based on facet

## Reference

| XDR IDL                    | Rust                          |
|----------------------------|-------------------------------|
| `int`                      | `i32`                         |
| `unsigned int`             | `u32`                         |
| `enum`                     | Unit `enum`                   |
| `bool`                     | `bool`                        |
| `hyper`                    | `i64`                         |
| `unsigned hyper`           | `u64`                         |
| `float`                    | `f32`                         |
| `double`                   | `f64`                         |
| `quadruple`                | Not currently supported       |
| `opaque [n]`               | `[u8; N]`                     |
| `opaque<>`                 | `Vec<u8>` or `&[u8]`          |
| `string<>`                 | `String`                      |
| Fixed length array `[n]`   | `[T; N]`                      |
| Variable length array `<>` | `Vec<T>` or `&[T]`            |
| `struct`                   | `struct`                      |
| `union`                    | `enum`                        |
| `void`                     | Unit `struct` or unit variant |
| `*` (optional-data)        | `Option`                      |

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
