# facet-singularize

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-singularize/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
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
