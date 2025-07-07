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
[![crates.io](https://img.shields.io/crates/v/facet-asn1.svg)](https://crates.io/crates/facet-asn1)
[![documentation](https://docs.rs/facet-asn1/badge.svg)](https://docs.rs/facet-asn1)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-asn1.svg)](./LICENSE)
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

# facet-asn1

A `#![no_std]` ASN.1 serializer and deserializer based on facet

Currently supports Distinguished Encoding Rules (DER) only

## Basic Types

| ASN.1 Type        | Rust                                                                 |
|-------------------|----------------------------------------------------------------------|
| BOOLEAN           | `bool`                                                               |
| INTEGER           | `i8`, `i16`, `i32`, or `i64`                                         |
| OCTET STRING      | `Vec<u8>`                                                            |
| NULL              | Any unit struct                                                      |
| REAL              | `f32` or `f64`                                                       |
| UTF8String        | `String`                                                             |
| CHOICE            | `enum`                                                               |
| SEQUENCE          | `struct`                                                             |

## Other ASN.1 Types

Newtype structs using the `facet::Shape::type_tag` property can be used to create other basic types without any content validation:

```rust
use facet::Facet;

#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[facet(type_tag = "IA5String", transparent)]
struct IA5String(String);
```

## Context Specific Type Tags

You can also set context specific BER/DER tags to a given number. Implicit tags must be set as transparent.

```rust
use facet::Facet;

// ImplicitString ::= [5] IMPLICIT UTF8String
#[derive(Debug, Facet, PartialEq, Eq)]
#[facet(type_tag = "5", transparent)]
struct ImplicitString(String);

// ExplciitString ::= [5] EXPLICIT UTF8String
#[derive(Debug, Facet, PartialEq, Eq)]
#[facet(type_tag = "5")]
struct ExplicitString(String);
```

The tag classes `UNIVERSAL`, `APPLICATION`, and `PRIVATE` are also supported in `type_tag`s for greater flexibility.


## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
