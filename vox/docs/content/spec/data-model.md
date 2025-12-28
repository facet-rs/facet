+++
title = "Data Model"
description = "Supported types and primitives"
weight = 10
+++

This document defines the core Rapace data model: what types can be used in service definitions.

## Type System

Rapace supports a **postcard-compatible subset** of Rust types defined below. The wire format is non-self-describing: peers must agree on schema via [Facet](https://facets.rs)-derived structural hashing before exchanging messages.

r[data.type-system.additional]
Additional types MAY be supported by implementations but are not part of the stable public API contract.

For wire encoding details, see [Payload Encoding](@/spec/payload-encoding.md).
For schema compatibility, see [Schema Evolution](@/spec/schema-evolution.md).
For language-specific mappings, see [Language Mappings](@/spec/language-mappings.md).

### Supported Types

#### Primitives

- **Integers**: `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`
- **Floats**: `f32`, `f64`
- **Boolean**: `bool`
- **Text**: `char` (Unicode scalar), `String` (UTF-8)
- **Bytes**: `Vec<u8>` (owned byte vectors)

#### Compound Types

- **Structs**: Named fields in declaration order
- **Tuples**: Fixed-size heterogeneous sequences
- **Arrays**: Fixed-size homogeneous sequences `[T; N]` (including `[u8; N]` for fixed-size byte arrays like UUIDs, hashes)
- **Sequences**: Dynamic-size vectors `Vec<T>`
- **Maps**: Key-value dictionaries `HashMap<K, V>`, `BTreeMap<K, V>`. Key types must be primitives, strings, or fixed-size byte arrays—types that can be compared for equality and (for `BTreeMap`) ordered.
- **Enums**: Sum types with unit, tuple, and struct variants
- **Option**: `Option<T>` / nullable types
- **Unit**: `()` / void

### Explicitly Unsupported

r[data.unsupported.usize]
`usize` and `isize` MUST NOT be used in public service APIs. Use explicit sizes (`u32`, `u64`, etc.) for cross-platform compatibility.

r[data.unsupported.pointers]
Raw pointers MUST NOT be used; they are not serializable.

r[data.unsupported.self-ref]
Self-referential types MUST NOT be used; they are not supported by Postcard.

r[data.unsupported.unions]
Untagged unions MUST NOT be used; they are not supported by Postcard.

r[data.unsupported.borrowed-return]
Borrowed types like `&[u8]` or `&str` in return position MUST NOT be used. Use owned types (`Vec<u8>`, `String`) instead.

> **Cross-language note**: Borrowed arguments (like `&str`) are a Rust API convenience. On the wire, all data is transmitted as owned bytes. Non-Rust implementations always work with owned data.

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

r[data.service.facet-required]
All argument and return types MUST implement `Facet`.

## Design Principles

### Non-Self-Describing

r[data.wire.non-self-describing]
The wire format MUST NOT encode type information. Field names, struct names, and type tags are not sent over the wire. This makes the encoding:

- ✅ **Compact**: No metadata overhead
- ✅ **Fast**: Direct serialization, no schema lookups
- ❌ **Requires shared schema**: Both peers must have identical type definitions

Schema mismatches are caught at **handshake time** via structural hashing (see [Handshake & Capabilities](@/spec/handshake.md)).

### Field-Order Dependent

r[data.wire.field-order]
Struct fields MUST be encoded in declaration order with no names or indices. This means:

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

### Determinism

#### Map Ordering

r[data.determinism.map-order]
Map encoding (`HashMap<K, V>`, `BTreeMap<K, V>`) is NOT canonical. Implementations MUST NOT rely on byte-for-byte equality for maps. The wire representation depends on iteration order, which may vary between different instances, implementations, and program runs.

If you need canonical ordering, sort keys at the application level before encoding.

#### Float Encoding

r[data.float.encoding]
Floating-point types (`f32`, `f64`) MUST be encoded as IEEE 754 little-endian bit patterns.

r[data.float.nan-canonicalization]
All NaN values MUST be canonicalized to the quiet NaN with all-zero payload before encoding:
- `f32` NaN: `0x7FC00000`
- `f64` NaN: `0x7FF8000000000000`

This ensures consistent encoding across platforms and languages.

r[data.float.negative-zero]
Negative zero (`-0.0`) and positive zero (`+0.0`) MUST be encoded as their distinct bit patterns. They are NOT canonicalized.

### Schema Hashing

The `Facet`-derived schema hash is computed from:

- ✅ **Field names**: Changing a field name changes the hash
- ✅ **Field order**: Reordering fields changes the hash
- ✅ **Field types**: Changing types changes the hash (recursively)
- ✅ **Enum variant names and discriminants**
- ❌ **Type names**: Struct/enum names do NOT affect the hash
- ❌ **Module paths**: Paths do NOT affect the hash
- ❌ **Documentation**: Comments do NOT affect the hash

This means two types with the same structure but different names are schema-compatible, while two types with the same name but different field names are NOT compatible.

## Result<T, E> and Protocol Status

Rust's `Result<T, E>` type is supported as a normal enum-like type. It does NOT replace the protocol-level status in the `CallResult` envelope.

```rust
#[rapace::service]
pub trait Files {
    // Returns Result<Vec<u8>, FileError>
    // Protocol status is ALWAYS OK if the method returns successfully
    // FileError is just a regular enum variant, not a protocol error
    async fn read(&self, path: String) -> Result<Vec<u8>, FileError>;
}
```

**Protocol errors** (connection failures, serialization errors, deadline exceeded) are signaled via the `CallResult.status` field with a non-zero error code.

**Application errors** (like `FileError::NotFound`) are encoded as normal return values with protocol status `OK`.

If you want other languages to recognize application-level errors without understanding your specific error type, consider using a standard error enum or including error metadata in the response type.

## Next Steps

- [Payload Encoding](@/spec/payload-encoding.md) – How types are encoded on the wire
- [Schema Evolution](@/spec/schema-evolution.md) – Compatibility rules and versioning
- [Language Mappings](@/spec/language-mappings.md) – How types map to other languages
- [Code Generation](@/spec/codegen.md) – Generating bindings from Rust definitions
