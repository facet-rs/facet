+++
title = "Core Concepts"
weight = 4
insert_anchor_links = "heading"
+++

## The layered architecture

Facet has two main layers:

**High-level** (`facet-reflect`): `Peek` and `Partial` provide safe, ergonomic APIs for reading and building values. This is what format crates and most users interact with.

**Low-level** (`facet-core`): `Shape`, `Def`, `Type`, and vtables define the raw reflection metadata. The derive macro generates this. You rarely interact with it directly unless you're writing a format crate or implementing `Facet` manually.

## Shape, Type, and Def

Every type that implements `Facet` has a `Shape` — a complete description of the type at runtime. The shape contains:

- **Type** — Structural classification (is it a struct? an enum? a primitive?). This follows the [Rust Reference](https://doc.rust-lang.org/reference/types.html) categories.

- **Def** — Semantic definition (how do I interact with it?). A `Vec<T>` has `Type::User` (it's a struct) but `Def::List` (you push/pop/iterate). A `String` has `Type::User` but `Def::Scalar` (it's an atomic value).

- **VTables** — Function pointers for runtime operations. Can I clone this? Display it? Parse it from a string? The vtables answer these questions without requiring trait bounds at compile time.

## Peek and Partial

`Peek` wraps a reference and lets you inspect it through the shape:

- What fields does this struct have?
- What variant is this enum?
- What elements are in this list?

`Partial` is the inverse — it lets you build a value piece by piece:

- Set this field to this value
- Push this element to the list
- Select this enum variant

Format crates use `Peek` to serialize and `Partial` to deserialize. They never see the concrete types — just shapes.

## VTables

`VTableDirect` and `VTableIndirect` contain function pointers for common operations: `clone_into`, `display`, `debug`, `parse`, `hash`, `partial_eq`, etc.

The derive macro auto-detects which traits a type implements. If a type implements `Clone`, the vtable gets a clone function. If not, that slot is `None`.

This lets you clone a value at runtime without a `Clone` bound — you check `shape.is(Characteristic::Clone)` and call the vtable function.

## Characteristic

`Characteristic` lets you query whether a shape supports certain operations:

```rust,noexec
if shape.is(Characteristic::Clone) {
    // Safe to call clone from the vtable
}
```

This is how facet provides "runtime trait bounds" — you can write generic code that adapts based on what the actual type supports.
