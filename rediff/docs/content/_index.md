+++
title = "rediff"
description = "Structural diffing and assertions for Facet types — no PartialEq required."
insert_anchor_links = "heading"
+++

**rediff** compares any Facet-derived type without requiring `PartialEq`. It uses
reflection to compare values structurally and produce detailed, colorized diffs.

- Structural comparison without `PartialEq`
- `assert_same!` / `assert_sameish!` macros for testing
- Multi-format rendering (Rust, JSON, XML styles)
- ANSI-colored terminal output
- Myers' algorithm for sequence diffing

## Assertions in tests

```rust
use facet::Facet;
use rediff::assert_same;

#[derive(Facet)]
struct Point { x: i32, y: i32 }

assert_same!(Point { x: 10, y: 20 }, Point { x: 10, y: 20 });
```

## Diffing values

```rust
use facet::Facet;
use rediff::{FacetDiff, format_diff_default};

#[derive(Facet)]
struct Config { host: String, port: u16 }

let old = Config { host: "localhost".into(), port: 8080 };
let new = Config { host: "localhost".into(), port: 9000 };

println!("{}", format_diff_default(&old.diff(&new)));
```

[Source on GitHub](https://github.com/bearcove/rediff) · [docs.rs](https://docs.rs/rediff)
