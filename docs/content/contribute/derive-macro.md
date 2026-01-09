+++
title = "The Derive Macro"
weight = 5
insert_anchor_links = "heading"
+++

## How it works

The `#[derive(Facet)]` macro:

1. Parses the type definition using [unsynn](https://docs.rs/unsynn)
2. Collects field information, attributes, and doc comments
3. Generates a `Facet` impl with a `SHAPE` constant
4. Processes `#[facet(...)]` attributes (both built-in and extension)

## Why unsynn?

[Syn](https://docs.rs/syn) is the standard Rust parsing library. It ships with a complete Rust grammar and parses everything into a full AST. That's convenient, but you pay for parsing features you don't use.

[Unsynn](https://docs.rs/unsynn) takes a different approach: it's a parser generator that gives you combinators to define your own grammar. You only pay for what you get.

For the derive macro, we don't need to parse function bodies, complex expressions, or most of Rust's syntax. We need struct/enum declarations, field names, types, attributes, and generics. So we define a grammar for just that in `facet-macros-impl/src/lib.rs` — types like `Struct`, `Enum`, `StructField`, `GenericParams`.

We also skip things we don't need to understand. Field types are grabbed as raw tokens with `VerbatimUntil<Comma>` — we don't parse them, we just pass them through to the generated code.

**The tradeoff**: we maintain our own incomplete Rust grammar. Some exotic syntax might not parse correctly. But for the common case, we get faster compile times and avoid pulling in heavy dependencies.

## Generated code

For this input:

```rust
#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}
```

The macro generates a `Facet` impl with a `SHAPE` constant containing:

- Type identifier (`"Person"`)
- Field metadata (names, types, offsets)
- VTable with auto-detected trait implementations
- Any doc comments and attributes

The generated code uses `offset_of!` to compute field offsets and closures to lazily resolve field shapes (avoiding infinite recursion for recursive types).

## Extension attributes

The derive macro supports namespaced extension attributes. See the [Extend guide](/extend/) for details.
