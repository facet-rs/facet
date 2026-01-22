# facet-diff

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-diff/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![crates.io](https://img.shields.io/crates/v/facet-diff.svg)](https://crates.io/crates/facet-diff)
[![documentation](https://docs.rs/facet-diff/badge.svg)](https://docs.rs/facet-diff)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-diff.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)

# facet-diff

Structural diffing for Facet types with human-readable output.

## Overview

facet-diff computes differences between two values using reflection.
It works on any type that implements `Facet`, without requiring manual
diff implementations or PartialEq.

```rust,ignore
use facet_diff::{FacetDiff, format_diff};

let old = MyStruct { name: "alice", count: 1 };
let new = MyStruct { name: "alice", count: 2 };

let diff = old.diff(&new);
println!("{}", format_diff(&diff));
// Shows: count: 1 → 2
```

## Features

- **Reflection-based**: Works on any `Facet` type automatically
- **Human-readable output**: Multiple output formats (colored, plain, compact)
- **Sequence diffing**: Uses Myers' algorithm for optimal sequence alignment
- **Float tolerance**: Configure epsilon for floating-point comparisons

## Usage

### Basic Diffing

```rust,ignore
use facet_diff::FacetDiff;

let diff = old_value.diff(&new_value);

if diff.is_equal() {
    println!("Values are equal");
} else {
    println!("Changes detected");
}
```

### Formatted Output

```rust,ignore
use facet_diff::{format_diff, format_diff_compact};

// Full colored diff
let output = format_diff(&diff);

// Compact single-line format
let compact = format_diff_compact(&diff);
```

### With Options

```rust,ignore
use facet_diff::{DiffOptions, diff_new_peek_with_options};
use facet_reflect::Peek;

let options = DiffOptions::new()
    .with_float_tolerance(0.001);

let diff = diff_new_peek_with_options(
    Peek::new(&old),
    Peek::new(&new),
    &options,
);
```

## Architecture

facet-diff uses facet-reflect to traverse values structurally:

1. **Peek** - facet's reflection API provides access to fields and values
2. **Myers' algorithm** - Optimal diff for sequences (lists, arrays)
3. **Recursive comparison** - Fields compared by structure, not equality
4. **Layout rendering** - facet-diff-core handles output formatting

## Related Crates

- **facet-diff-core**: Core diff types and rendering
- **facet-reflect**: Reflection API for traversing Facet values
- **facet-pretty**: Pretty-printing for Facet values

## LLM contribution policy

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
