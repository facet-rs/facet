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

**Normalization rules**:
- Identifiers are exact UTF-8 byte strings
- Case-sensitive (`userId` ≠ `user_id` ≠ `UserId`)
- No Unicode normalization (NFC/NFKD not applied)
- Hashing uses the raw UTF-8 bytes

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

The schema hash uses a cryptographic hash function (BLAKE3) over a canonical serialization of the type shape.

### Canonical Shape Serialization

The shape is serialized deterministically as:

```
shape_bytes = serialize_shape(facet_shape)
sig_hash = blake3(shape_bytes)
```

Where `serialize_shape` produces a canonical byte representation:

1. **Struct**: `STRUCT_TAG || field_count || (field_id_len || field_id_bytes || field_type_hash)*`
2. **Enum**: `ENUM_TAG || variant_count || (variant_id_len || variant_id_bytes || variant_payload_hash)*`
3. **Primitive**: `PRIMITIVE_TAG || primitive_kind`
4. **Container**: `CONTAINER_TAG || container_kind || element_type_hash(es)`

Fields and variants are serialized in declaration order (order matters!).

### Implementation Note

The hash is computed from the `facet::Shape` at compile time. Codegen for other languages must implement the same algorithm to produce matching hashes.

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

After exchanging registries:

| Condition | Result |
|-----------|--------|
| Same `method_id`, same `sig_hash` | ✅ Compatible, calls proceed |
| Same `method_id`, different `sig_hash` | ❌ Incompatible, reject calls |
| `method_id` only on one side | Method unknown to other peer |

### On Incompatible Call

If a client attempts to call an incompatible method:

1. **Immediate rejection** (before encoding): The client knows from handshake that hashes don't match
2. **Error**: `INCOMPATIBLE_SCHEMA` with method name and hash mismatch details

### Collision Policy

If two different methods hash to the same `method_id` (FNV-1a collision):

- **Build time**: Codegen MUST detect and fail with an error
- **Runtime**: Should never happen if codegen is correct

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
