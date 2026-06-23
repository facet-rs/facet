# fable

[![crates.io](https://img.shields.io/crates/v/fable.svg)](https://crates.io/crates/fable)
[![documentation](https://docs.rs/fable/badge.svg)](https://docs.rs/fable)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/fable.svg)](../../LICENSE-MIT)

**fable** is a tiny typed language for inspecting and mutating
[facet](https://facet.rs)-reflected Rust values, then lowering toward canonical
[Weavy](https://docs.rs/weavy) IR. You write a small script that navigates a
struct by field name and assigns or tests scalar values; fable resolves every
path against the Facet shape at compile time, type-checks the expressions, and
produces a reusable bytecode plan that runs directly against a live `&mut T`
without any serialization round-trip.

The crate currently owns the **syntax layer** — a lossless lexer/parser, the
[cstree](https://docs.rs/cstree) language tags, and a typed AST facade — plus
the **lowering and interpreter** that compiles source to Weavy IR and executes
it against Facet-reflected Rust values. "Lossless" means the concrete syntax
tree preserves every byte — whitespace, comments, trivia — so tooling can
round-trip and rewrite Fable source without losing anything.

```rust
use facet::Facet;
use fable::{FablePlan, apply};

#[derive(Facet)]
struct Config {
    name: String,
    max_retries: u32,
    threshold: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = Config {
        name: "  acme  ".into(),
        max_retries: 100,
        threshold: 0.5,
    };

    // Compile once, apply many times.
    let plan = FablePlan::<Config>::compile(
        r#"
        root.name = trim(root.name);
        root.max_retries = clamp(root.max_retries, 1, 10);
        if root.threshold > 1.0 {
            root.threshold = 1.0;
        }
        "#,
    )?;
    plan.apply(&mut cfg)?;

    // One-shot convenience wrapper.
    apply(&mut cfg, r#"root.name = root.name + "-v2";"#)?;

    Ok(())
}
```

## How it fits with Facet and Weavy

Fable sits between the two: it reads struct layouts from [facet-core](https://docs.rs/facet-core)
reflection (`Shape`, field offsets, scalar types) and emits [Weavy](https://docs.rs/weavy) IR
that the Weavy interpreter executes. Because field paths are resolved at
`FablePlan::compile` time, repeated `apply` calls pay only interpreter cost —
no parsing, no field lookup.

Supported scalar types cover the full Rust numeric tower (`i8`–`i128`, `u8`–`u128`,
`f32`, `f64`), `bool`, `char`, `String`, and `Cow<str>`. Built-in intrinsics
include `min`, `max`, `clamp`, `len`, `trim`, `contains`, `starts_with`, and
`ends_with`.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT license](../../LICENSE-MIT) at your option.
