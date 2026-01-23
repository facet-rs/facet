# facet-python

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-python/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-python.svg)](https://crates.io/crates/facet-python)
[![documentation](https://docs.rs/facet-python/badge.svg)](https://docs.rs/facet-python)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-python.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Generate Python type definitions from facet type metadata.

## Overview

This crate uses facet's reflection capabilities to generate Python type hints
and TypedDicts from any Rust type that implements `Facet`. This enables
type-safe interop when your Rust code exchanges data with Python.

## Example

```rust
use facet::Facet;
use facet_python::to_python;

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

let python_code = to_python::<User>(false);
```

This generates:

```python
from typing import TypedDict, Required, NotRequired

class User(TypedDict, total=False):
    name: Required[str]
    age: Required[int]
    email: str  # Optional fields become NotRequired
```

## Type Mappings

| Rust Type | Python Type |
|-----------|-------------|
| `String`, `&str` | `str` |
| `i32`, `u32`, etc. | `int` |
| `f32`, `f64` | `float` |
| `bool` | `bool` |
| `Vec<T>` | `list[T]` |
| `Option<T>` | `T` (NotRequired in TypedDict) |
| `HashMap<K, V>` | `dict[K, V]` |
| Struct | `TypedDict` |
| Enum | `Union[...]` of variants |

## Features

- **Recursive types**: Handles nested structs and enums
- **Documentation**: Preserves doc comments as Python docstrings
- **Reserved keywords**: Automatically handles Python reserved words as field names
- **Generic support**: Maps Rust generics to Python type parameters

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
