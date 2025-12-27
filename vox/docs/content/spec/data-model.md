+++
title = "Data Model"
description = "Supported types and primitives"
weight = 10
+++

This document defines the core Rapace data model: what types can be used in service definitions.

## Type System

Rapace supports all types that can be encoded with [postcard](https://postcard.jamesmunns.com/). The wire format is non-self-describing: both peers must agree on the schema before exchanging messages.

For wire encoding details, see [Wire Format](@/spec/wire-format.md).
For schema compatibility, see [Schema Evolution](@/spec/schema-evolution.md).
For language-specific mappings, see [Language Mappings](@/spec/language-mappings.md).

### Supported Types

#### Primitives

- **Integers**: `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`
- **Floats**: `f32`, `f64`
- **Boolean**: `bool`
- **Text**: `char` (Unicode scalar), `String` (UTF-8)
- **Bytes**: `Vec<u8>`, byte slices

#### Compound Types

- **Structs**: Named fields in declaration order
- **Tuples**: Fixed-size heterogeneous sequences
- **Arrays**: Fixed-size homogeneous sequences `[T; N]`
- **Sequences**: Dynamic-size vectors `Vec<T>`
- **Maps**: Key-value dictionaries `HashMap<K, V>`, `BTreeMap<K, V>`
- **Enums**: Sum types with unit, tuple, and struct variants
- **Option**: `Option<T>` / nullable types
- **Unit**: `()` / void

### Explicitly Unsupported

- **Platform-dependent sizes**: `usize` and `isize` are **prohibited in public service APIs**. Use explicit sizes (`u32`, `u64`, etc.) for cross-platform compatibility.
- **Raw pointers**: Not serializable
- **Self-referential types**: Not supported by postcard
- **Untagged unions**: Not supported by postcard

## Type Definition in Rust

Types are defined in Rust using the `Facet` derive macro from the [facet](https://facets.rs) ecosystem:

```rust
use facet::Facet;

#[derive(Facet)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Facet)]
pub enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Point(Point),
}

#[derive(Facet)]
pub struct Message {
    pub id: [u8; 16],
    pub timestamp: u64,
    pub payload: Vec<u8>,
    pub metadata: Option<HashMap<String, String>>,
}
```

The `Facet` derive macro provides:
- **Type introspection** at compile time (field names, types, layout)
- **Schema hashing** for compatibility checking (see [Schema Evolution](@/spec/schema-evolution.md))
- **Code generation** for other languages (see [Code Generation](@/spec/codegen.md))

## Service Definitions

Services are defined using the `#[rapace::service]` attribute macro:

```rust
use rapace::prelude::*;

#[rapace::service]
pub trait Graphics {
    async fn draw(&self, shape: Shape) -> Result<(), String>;
    async fn clear(&self);
    async fn save(&self, path: String) -> Result<Vec<u8>, String>;
}
```

The macro generates:
- `GraphicsClient` for making calls
- `GraphicsServer<T>` for handling calls
- Method IDs for dispatch
- Schema hashes for compatibility checking

All argument and return types must implement `Facet`.

## Design Principles

### Non-Self-Describing

The wire format **does not encode type information**. Field names, struct names, and type tags are not sent over the wire. This makes the encoding:

- ✅ **Compact**: No metadata overhead
- ✅ **Fast**: Direct serialization, no schema lookups
- ❌ **Requires shared schema**: Both peers must have identical type definitions

Schema mismatches are caught at **handshake time** via structural hashing (see [Handshake & Capabilities](@/spec/handshake.md)).

### Field-Order Dependent

Struct fields are encoded **in declaration order** with no names or indices. This means:

- ✅ **Minimal encoding overhead**: Just values, no field tags
- ❌ **Field order is immutable**: Reordering fields breaks compatibility
- ❌ **Adding/removing fields breaks compatibility**: See [Schema Evolution](@/spec/schema-evolution.md)

```rust
#[derive(Facet)]
pub struct V1 {
    pub a: i32,
    pub b: i32,
}

// ❌ BREAKING CHANGE: field order changed
#[derive(Facet)]
pub struct V2 {
    pub b: i32,  // now first!
    pub a: i32,
}

// ❌ BREAKING CHANGE: field added
#[derive(Facet)]
pub struct V3 {
    pub a: i32,
    pub b: i32,
    pub c: i32,  // new field
}
```

For versioning strategies, see [Schema Evolution](@/spec/schema-evolution.md).

### Structural Typing

Two types with **identical structure** are considered compatible, even if they have different names:

```rust
// Package A
#[derive(Facet)]
pub struct Point { pub x: i32, pub y: i32 }

// Package B
#[derive(Facet)]
pub struct Coordinate { pub x: i32, pub y: i32 }

// These are structurally identical and have the same schema hash
```

This enables:
- ✅ **Refactoring freedom**: Rename types without breaking compatibility
- ✅ **Cross-organization interop**: No central type registry required
- ⚠️ **Type safety is structural**: Compiler can't distinguish semantically different types with same structure

## Next Steps

- [Wire Format](@/spec/wire-format.md) – How types are encoded on the wire
- [Schema Evolution](@/spec/schema-evolution.md) – Compatibility rules and versioning
- [Language Mappings](@/spec/language-mappings.md) – How types map to other languages
- [Code Generation](@/spec/codegen.md) – Generating bindings from Rust definitions
