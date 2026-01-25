# rediff

Structural diffing and assertions for [Facet](https://github.com/bearcove/facet) types.

Compare any Facet-derived type without requiring `PartialEq` - rediff uses reflection to compare values structurally and produce detailed, colorized diff output.

## Features

- Structural comparison without `PartialEq`
- Pretty `assert_same!` and `assert_sameish!` macros for testing
- Multi-format rendering (Rust, JSON, XML styles)
- ANSI colored terminal output
- Myers' algorithm for sequence diffing

## Installation

```bash
cargo add rediff
```

## Usage

### Assertions in tests

```rust
use facet::Facet;
use rediff::assert_same;

#[derive(Facet)]
struct Point { x: i32, y: i32 }

let a = Point { x: 10, y: 20 };
let b = Point { x: 10, y: 20 };
assert_same!(a, b);
```

### Diffing values

```rust
use facet::Facet;
use rediff::{FacetDiff, format_diff_default};

#[derive(Facet)]
struct Config {
    host: String,
    port: u16,
}

let old = Config { host: "localhost".into(), port: 8080 };
let new = Config { host: "localhost".into(), port: 9000 };

let diff = old.diff(&new);
println!("{}", format_diff_default(&diff));
```
