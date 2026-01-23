# facet-singularize

[![crates.io](https://img.shields.io/crates/v/facet-singularize.svg)](https://crates.io/crates/facet-singularize)
[![documentation](https://docs.rs/facet-singularize/badge.svg)](https://docs.rs/facet-singularize)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-singularize.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

Fast, no-regex English singularization.

## Overview

This crate converts plural English words to their singular form without using regex.
It's designed for use in deserialization where performance matters—for example, when
mapping JSON field names like `"dependencies"` to Rust struct fields like `dependency`.

## Example

```rust
use facet_singularize::singularize;

assert_eq!(singularize("dependencies"), "dependency");
assert_eq!(singularize("items"), "item");
assert_eq!(singularize("children"), "child");
assert_eq!(singularize("boxes"), "box");
assert_eq!(singularize("matrices"), "matrix");
```

## Features

- **No regex**: Uses simple suffix matching and table lookups
- **no_std compatible**: Works without the standard library (with `alloc` feature)
- **Irregular plurals**: Handles common exceptions like children→child, mice→mouse
- **Latin/Greek plurals**: Supports -ices→-ix (matrices→matrix), -ae→-a (larvae→larva)

## Performance

Benchmarked to be fast enough for hot paths. The implementation prioritizes
predictable performance over completeness—it handles the common cases well
rather than trying to be a full linguistic library.

## Part of the Facet Ecosystem

This crate is part of the [facet](https://facet.rs) ecosystem, providing reflection for Rust.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet-xml/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet-xml/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
