<h1>
<picture>
<source srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.webp">
<img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture> &nbsp; facet-reflect
</h1>

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/facet-reflect.svg)](https://crates.io/crates/facet-reflect)
[![documentation](https://docs.rs/facet-reflect/badge.svg)](https://docs.rs/facet-reflect)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-reflect.svg)](./LICENSE)

_Logo by [Misiasart](https://misiasart.com/)_

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
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
    <img src="https://depot.dev/badges/built-with-depot.svg" alt="built with depot">
</a> </p>

The `facet-reflect` crate allows reading from and writing to shapes implementing the `Facet` trait. This crate combines the functionality of the former `facet-peek` and `facet-poke` crates, providing a unified interface for reflection and manipulation of `Facet` types.

## Features

- Read (peek) values from `Facet` types
- Write (poke) values to `Facet` types
- Supports various data structures: scalars, lists, maps, structs, enums, and options
- Efficient and safe manipulation of `Facet` types

## Usage

This crate makes it trivial to write serializers and deserializers. It's used by crates like `facet-json`, `facet-yaml`, `facet-urlencoded`, etc.

For more detailed usage examples, please refer to the crate documentation.

