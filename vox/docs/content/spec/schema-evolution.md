+++
title = "Schema Evolution"
description = "Compatibility, hashing, and versioning"
weight = 35
+++

This document defines how Rapace handles schema changes, compatibility checking, and API versioning.

## Core Principle

**Schema changes are breaking by default.** Field-order encoding means:

- ❌ Adding fields breaks compatibility
- ❌ Removing fields breaks compatibility
- ❌ Reordering fields breaks compatibility
- ❌ Changing field types breaks compatibility
- ❌ Renaming fields breaks compatibility

This is not a bug—it's a deliberate design choice for:

- ✅ Minimal wire overhead (no field tags, no names on the wire)
- ✅ Fast encoding/decoding (direct serialization)
- ✅ Deterministic hashing (structural compatibility via digests)
- ✅ Semantic safety (catch mismatches at handshake, not at runtime)

## Compatibility Detection

Rapace detects incompatibilities at **handshake time** via **structural schema hashing**.

### What's In the Hash

The schema hash (also called `sig_hash`) is computed from the structural shape of a type. It includes:

| Included | Example |
|----------|---------|
| Field identifiers | `"user_id"`, `"name"` |
| Field order | `(user_id, name)` ≠ `(name, user_id)` |
| Field types (recursively) | `i32`, `String`, nested structs |
| Enum variant identifiers | `"Red"`, `"Green"`, `"Blue"` |
| Enum variant order | `(Red, Green, Blue)` ≠ `(Green, Red, Blue)` |
| Enum variant payloads | `Circle(f64)` ≠ `Circle(f32)` |
| Container shapes | `Vec<T>`, `Option<T>`, `HashMap<K,V>` |
| Integer sizes and signedness | `i32` ≠ `u32` ≠ `i64` |

### What's NOT In the Hash

| Excluded | Why |
|----------|-----|
| Type names (struct/enum names) | Allows renaming types without breaking |
| Module paths | Allows moving types between modules |
| Documentation comments | Not semantically relevant |
| Visibility (`pub`, `pub(crate)`) | Runtime irrelevant |
| Generic parameter names | Only instantiated shapes matter |

### Field and Variant Identifiers

**Critical**: Field and variant identifiers ARE included in the hash. This means renaming a field or variant is a breaking change.

The identifier is the **canonical wire name** for the field/variant:

- In Rust with Facet: the field name as declared (e.g., `user_id`)
- Facet rename attributes (if any) override the default

r[schema.identifier.normalization]
Identifiers MUST be exact UTF-8 byte strings. Identifiers are case-sensitive (`userId` ≠ `user_id` ≠ `UserId`). No Unicode normalization (NFC/NFKD) MUST be applied. Hashing MUST use the raw UTF-8 bytes.

**Tuple fields**: For tuple structs and tuple variants, implicit identifiers are used: `_0`, `_1`, `_2`, etc.

### Why Include Identifiers?

Including field/variant identifiers provides semantic safety:

```rust
// These have identical wire encoding but DIFFERENT hashes:
struct UserRef { user_id: i64 }
struct OrderRef { order_id: i64 }  // Different field name!

// This prevents accidentally treating a UserRef as an OrderRef
```

Without identifiers in the hash, these would be "compatible" and you'd only discover the bug at runtime when semantics break.

### Type Name Freedom

Type names are NOT in the hash, so you can rename types freely:

```rust
// Before
struct Point { x: i32, y: i32 }

// After (OK - same hash)
struct Coordinate { x: i32, y: i32 }
```

This enables:
- Refactoring type names without coordination
- Different codebases using different names for the same structure
- Cross-language bindings using idiomatic names

## Hash Algorithm

r[schema.hash.algorithm]
The schema hash MUST use BLAKE3 over a canonical serialization of the type shape.

### Canonical Shape Serialization

The shape is serialized deterministically as:

```
shape_bytes = serialize_shape(facet_shape)
sig_hash = blake3(shape_bytes)
```

Where `serialize_shape` produces a canonical byte representation using the following rules.

### Shape Tags

| Tag | Value (u8) | Type |
|-----|------------|------|
| `UNIT` | 0x00 | Unit type `()` |
| `BOOL` | 0x01 | Boolean |
| `U8` | 0x02 | Unsigned 8-bit |
| `U16` | 0x03 | Unsigned 16-bit |
| `U32` | 0x04 | Unsigned 32-bit |
| `U64` | 0x05 | Unsigned 64-bit |
| `U128` | 0x06 | Unsigned 128-bit |
| `I8` | 0x07 | Signed 8-bit |
| `I16` | 0x08 | Signed 16-bit |
| `I32` | 0x09 | Signed 32-bit |
| `I64` | 0x0A | Signed 64-bit |
| `I128` | 0x0B | Signed 128-bit |
| `F32` | 0x0C | 32-bit float |
| `F64` | 0x0D | 64-bit float |
| `CHAR` | 0x0E | Unicode scalar |
| `STRING` | 0x0F | UTF-8 string |
| `BYTES` | 0x10 | Byte vector |
| `OPTION` | 0x20 | Optional wrapper |
| `VEC` | 0x21 | Dynamic array |
| `ARRAY` | 0x22 | Fixed-size array |
| `MAP` | 0x23 | Key-value map |
| `STRUCT` | 0x40 | Named struct |
| `TUPLE` | 0x41 | Tuple |
| `ENUM` | 0x42 | Sum type |

### Encoding Rules

r[schema.encoding.endianness]
All multi-byte integers in the canonical shape serialization MUST be encoded as little-endian.

r[schema.encoding.lengths]
String lengths and counts MUST be encoded as u32 little-endian.

**Primitives** (tags 0x00-0x10):
```
primitive_bytes = [tag]
```

**Option**:
```
option_bytes = [OPTION] || serialize_shape(inner_type)
```

**Vec**:
```
vec_bytes = [VEC] || serialize_shape(element_type)
```

**Array**:
```
array_bytes = [ARRAY] || length_u32_le || serialize_shape(element_type)
```

**Map**:
```
map_bytes = [MAP] || serialize_shape(key_type) || serialize_shape(value_type)
```

**Struct**:
```
struct_bytes = [STRUCT] || field_count_u32_le || (field_name_len_u32_le || field_name_utf8 || serialize_shape(field_type))*
```

**Tuple**:
```
tuple_bytes = [TUPLE] || element_count_u32_le || serialize_shape(element_0) || ... || serialize_shape(element_n)
```

**Enum**:
```
enum_bytes = [ENUM] || variant_count_u32_le || (variant_name_len_u32_le || variant_name_utf8 || variant_payload_bytes)*
```

Where `variant_payload_bytes` is:
- For unit variants: empty (zero bytes)
- For newtype variants: `serialize_shape(inner_type)`
- For tuple variants: `[TUPLE] || ...` (as above)
- For struct variants: `[STRUCT] || ...` (as above, with field count and fields)

r[schema.encoding.order]
Fields and variants MUST be serialized in declaration order. Order matters for compatibility.

### Example

For this Rust type:

```rust
struct Point { x: i32, y: i32 }
```

The canonical serialization is:

```
40                      # STRUCT tag
02 00 00 00             # field_count = 2 (u32 LE)
01 00 00 00             # field[0] name length = 1
78                      # field[0] name = "x"
09                      # field[0] type = I32
01 00 00 00             # field[1] name length = 1
79                      # field[1] name = "y"
09                      # field[1] type = I32
```

### Implementation Note

r[schema.hash.cross-language]
The hash is computed from the `facet::Shape` at compile time. Code generators for other languages MUST implement the same algorithm to produce matching hashes. The reference implementation is in `rapace-registry`.

## Handshake Protocol

When a connection is established, peers exchange a **method registry** keyed by `method_id`:

```rust
struct MethodInfo {
    method_id: u32,       // FNV-1a hash of "ServiceName.MethodName"
    sig_hash: [u8; 32],   // Structural hash of (args, return_type)
    name: Option<String>, // Human-readable, for debugging only
}
```

### Compatibility Check

r[schema.compat.check]
After exchanging registries, peers MUST check compatibility as follows:

| Condition | Result |
|-----------|--------|
| Same `method_id`, same `sig_hash` | Compatible, calls proceed |
| Same `method_id`, different `sig_hash` | Incompatible, reject calls |
| `method_id` only on one side | Method unknown to other peer |

### On Incompatible Call

r[schema.compat.rejection]
If a client attempts to call a method with mismatched `sig_hash`, the client MUST reject the call immediately (before encoding) with `INCOMPATIBLE_SCHEMA` error including method name and hash mismatch details.

### Collision Policy

r[schema.collision.detection]
If two different methods hash to the same `method_id` (FNV-1a collision), code generators MUST detect this at build time and fail with an error.

r[schema.collision.runtime]
Runtime `method_id` collisions SHALL NOT occur if code generation is correct. Implementations MAY assume no collisions at runtime.

## What Breaks Compatibility

### Struct Changes

| Change | Breaking? | Why |
|--------|-----------|-----|
| Reorder fields | ❌ Yes | Order is in hash |
| Add field | ❌ Yes | Field count changes |
| Remove field | ❌ Yes | Field count changes |
| Change field type | ❌ Yes | Type hash changes |
| Rename field | ❌ Yes | Identifier is in hash |
| Rename struct | ✅ No | Type name not in hash |

### Enum Changes

| Change | Breaking? | Why |
|--------|-----------|-----|
| Reorder variants | ❌ Yes | Order is in hash |
| Add variant | ❌ Yes | Variant count changes |
| Remove variant | ❌ Yes | Variant count changes |
| Change variant payload | ❌ Yes | Payload hash changes |
| Rename variant | ❌ Yes | Identifier is in hash |
| Rename enum | ✅ No | Type name not in hash |

### Method Changes

| Change | Breaking? | Why |
|--------|-----------|-----|
| Change argument types | ❌ Yes | Arg type hash changes |
| Change return type | ❌ Yes | Return type hash changes |
| Add/remove arguments | ❌ Yes | Arg count changes |
| Rename method | ❌ Yes | method_id changes |
| Rename service | ❌ Yes | method_id changes |

## Versioning Strategies

Since breaking changes require explicit versioning, Rapace encourages clear API evolution patterns.

### Strategy 1: Versioned Methods

Add new method versions instead of modifying existing ones:

```rust
#[rapace::service]
pub trait Calculator {
    // Original
    async fn add(&self, a: i32, b: i32) -> i32;
    
    // New version with different types
    async fn add_v2(&self, a: i64, b: i64) -> i64;
}
```

Both methods coexist. Clients use whichever version they support.

### Strategy 2: Versioned Services

Create new service traits for major changes:

```rust
#[rapace::service]
pub trait CalculatorV1 {
    async fn add(&self, a: i32, b: i32) -> i32;
}

#[rapace::service]
pub trait CalculatorV2 {
    async fn add(&self, a: i64, b: i64) -> i64;
    async fn sub(&self, a: i64, b: i64) -> i64;
}
```

Server implements both; clients connect to whichever they need.

### Strategy 3: Envelope Types

For additive changes, wrap old types:

```rust
#[derive(Facet)]
pub struct UserV1 {
    pub id: u64,
    pub name: String,
}

#[derive(Facet)]
pub struct UserV2 {
    pub base: UserV1,           // Embed old version
    pub email: Option<String>,  // New field
}
```

This is NOT backward compatible (different hash), but enables gradual migration.

## Migration Workflow

When making breaking changes:

1. **Define new version** (`add_v2`, `ServiceV2`, `TypeV2`)
2. **Implement on server** (dual implementation)
3. **Deploy server** (serves both versions)
4. **Migrate clients** gradually
5. **Monitor old version usage**
6. **Deprecate old version** (log warnings)
7. **Remove old version** when safe

## Summary

| Aspect | Rule |
|--------|------|
| **Default** | All changes are breaking |
| **Field identifiers** | Included in hash (renames break) |
| **Type names** | Excluded from hash (renames OK) |
| **Detection** | Hash mismatch at handshake |
| **Handshake key** | `method_id` (routing) + `sig_hash` (compatibility) |
| **Mitigation** | Explicit versioning (new methods/services) |

## Next Steps

- [Handshake & Capabilities](@/spec/handshake.md) – How registries are exchanged
- [Data Model](@/spec/data-model.md) – Supported types
- [Core Protocol](@/spec/core.md) – Error codes for schema mismatch
