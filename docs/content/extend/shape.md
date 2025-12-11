+++
title = "Shape"
weight = 3
insert_anchor_links = "heading"
+++

## What you can get from `Shape`

### Identity

- `ConstTypeId` — A stable, hashable identifier for the type
- `type_identifier()` — Human-readable name (e.g., "MyStruct", "Vec<i32>")
- Generics metadata — Information about generic type parameters

### Layout

- `size()` — Size in bytes
- `alignment()` — Alignment requirement
- `owned` vs `borrowed` — Whether the type owns or borrows its data

### Structure

`Shape` contains:
- `Type` — Structural classification (Struct, Enum, Primitive, Pointer, etc.)
- `Def` — Semantic definition (Scalar, List, Map, Struct, Enum, etc.)
- Fields/variants/docstrings — For aggregate types
- Attributes — Including `skip_unless_truthy`, `sensitive`, custom extension attributes

Access fields through `Peek` or `Partial` for safe, ergonomic inspection and mutation.

### VTables

Operations available on this type via function pointers:
- `clone_into` — Runtime cloning without Clone bound
- `display` / `debug` — Formatting
- `parse` — Parsing from strings
- `hash` — Computing hashes
- `partial_eq` — Equality comparison
- `truthiness_fn()` — Checking if a value is truthy (when available)

Use `Characteristic` to query support for these operations:

```rust,noexec
if shape.is(Characteristic::Clone) {
    // Safe to call clone_into from the vtable
}
```

### Truthiness

Types can register a **truthiness predicate** — a function that determines if a value is "truthy" or "falsy". This is used by `#[facet(skip_unless_truthy)]` to conditionally skip serialization.

Call `shape.truthiness_fn()` to get the predicate, if available:

```rust,noexec
if let Some(truthy) = shape.truthiness_fn() {
    let is_truthy = unsafe { truthy(ptr) };
    // Use is_truthy to decide whether to serialize
}
```

**Built-in truthiness rules:**
- **bool**: `true` is truthy
- **Numbers**: non-zero is truthy (floats also exclude NaN)
- **Collections** (Vec, String, slice, etc.): non-empty is truthy
- **Option**: `Some(_)` is truthy, `None` is falsy
- **Arrays**: non-zero-length arrays are truthy
- **Custom types**: Can register a custom truthiness function via `#[facet(truthy = path::to::fn)]`

### Safety

Why `Facet` is `unsafe`:
- Facet requires you to ensure layout matches between Rust and the shape
- Pointers in the shape (vtable, type_ops) must be valid
- When implementing custom Facet, you're responsible for correctness

Invariants you must respect when consuming `Shape`:
- Don't call vtable functions with mismatched pointer types
- Don't assume a type has a characteristic it doesn't claim
- Truthiness predicates assume well-formed values

### Examples

**Listing fields:**
```rust,noexec
use facet::Peek;

let value = MyStruct { /* ... */ };
let peek = Peek::new(&value);

for field in peek.fields() {
    println!("Field: {}", field.name());
}
```

**Checking marker traits:**
```rust,noexec
if shape.is(Characteristic::Clone) {
    // This type is Clone
}
```

**Rendering type names:**
```rust,noexec
println!("Type: {}", shape.type_identifier());
```

**Checking truthiness support:**
```rust,noexec
if let Some(truthy) = shape.truthiness_fn() {
    let is_truthy = unsafe { truthy(ptr) };
}
```
