+++
title = "Schema Evolution"
description = "Compatibility, hashing, and versioning"
weight = 30
+++

This document defines how Rapace handles schema changes, compatibility checking, and API versioning.

## Core Principle

**Schema changes are breaking by default.** Field-order encoding means:

❌ **Adding fields breaks compatibility**
❌ **Removing fields breaks compatibility**
❌ **Reordering fields breaks compatibility**
❌ **Changing field types breaks compatibility**

This is not a bug—it's a deliberate design choice for:

✅ **Minimal wire overhead** (no field tags, no names)
✅ **Fast encoding/decoding** (direct serialization, no schema lookups)
✅ **Deterministic hashing** (structural compatibility via digests)

## Compatibility Strategy

Rapace detects incompatibilities at **handshake time** via **structural schema hashing**.

### Schema Hashing

Each method's argument and return types are hashed to produce a **structural schema digest**:

```
method_hash = hash(
    method_name,
    hash(arg_types),      // recursively hash all argument types
    hash(return_type)     // recursively hash return type
)
```

**Hash properties**:
- **Structural, not nominal**: Types with different names but identical structure hash the same
- **Recursive**: Hashes include all nested types (struct fields, enum variants, etc.)
- **Deterministic**: Same schema always produces same hash
- **Collision-resistant**: Uses cryptographic hash function (e.g., BLAKE3 or SHA-256)

**Hash includes**:
- Field names and order
- Field types (recursively)
- Enum variant names, discriminants, and payloads
- Container types (Vec, Option, HashMap, etc.)

**Hash excludes**:
- Type names (struct/enum names don't affect hash)
- Package/module paths
- Documentation comments
- Attributes (except those affecting wire format)

### Facet-Based Hashing

> **Status**: Planned. Facet does not currently provide a canonical hash function for type shapes.

The hash will be computed from the `facet::Shape` of each type:

```rust
use facet::Shape;

fn hash_shape(shape: &Shape) -> [u8; 32] {
    // Recursively hash the shape structure
    // ...
}
```

This ensures:
- Hash is derived from compile-time type information
- Same hash computation across all language bindings
- Hash can be computed at build time (no runtime cost)

### Handshake Protocol

When a connection is established, peers exchange a **method registry**:

```rust
struct HandshakeInfo {
    methods: HashMap<String, MethodHash>,
    // ... other capabilities
}

struct MethodHash {
    hash: [u8; 32],  // Structural hash of method signature
}
```

**After handshake**, both sides know:
- ✅ **Compatible methods**: Hashes match, calls proceed normally
- ❌ **Incompatible methods**: Hashes differ, calls are rejected immediately

**Example handshake exchange**:

```
Client → Server:
{
  "Calculator::add": 0xABCD1234...,
  "Calculator::mul": 0xEF567890...,
}

Server → Client:
{
  "Calculator::add": 0xABCD1234...,  // ✅ Match
  "Calculator::mul": 0x12345678...,  // ❌ Mismatch
  "Calculator::div": 0xDEADBEEF...,  // Client doesn't have this
}

Result:
- add() is callable
- mul() calls fail with schema mismatch error
- div() is unknown to client (not callable)
```

### On Incompatible Call

If a client attempts to call an incompatible method, the error flow is:

1. **Immediate rejection**: Call fails before encoding arguments
   ```
   Error: Method 'mul' is incompatible (hash mismatch)
   ```

2. **Lazy schema fetch** (optional, for debugging):
   - Client requests full schema from server
   - Server sends facet `Shape` for the method
   - Client diffs local schema vs remote schema
   - Detailed error message:
     ```
     Method 'mul' is incompatible:

     Local:  fn mul(a: i32, b: i32) -> i64
     Remote: fn mul(a: f64, b: f64) -> f64

     Difference: Argument 0 type changed from i32 to f64
                 Argument 1 type changed from i32 to f64
                 Return type changed from i64 to f64
     ```

This makes debugging schema drift straightforward while keeping the happy path fast.

## Structural Equivalence

Two types with **identical structure** are compatible, even if they have different names or live in different packages:

```rust
// Package A
#[derive(Facet)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

// Package B
#[derive(Facet)]
pub struct Coordinate {
    pub x: i32,
    pub y: i32,
}

// These are structurally identical:
// - Same field count (2)
// - Same field names ("x", "y")
// - Same field types (i32, i32)
// - Same field order
//
// → Same hash, fully compatible over the wire
```

**Benefits**:
- Refactoring freedom (rename types without breaking compatibility)
- No central type registry required
- Cross-organization interop (different codebases can define "the same" types)

**Caveats**:
- No semantic checking (compiler can't distinguish `UserId(i64)` from `OrderId(i64)` if both are structurally `{ 0: i64 }`)
- Type names are documentation only (not enforced at wire level)

## Versioning Strategies

Since breaking changes are common, Rapace encourages **explicit API versioning**.

### Strategy 1: Versioned Services

Create new service versions for breaking changes:

```rust
#[rapace::service]
pub trait CalculatorV1 {
    async fn add(&self, a: i32, b: i32) -> i32;
}

#[rapace::service]
pub trait CalculatorV2 {
    async fn add(&self, a: i64, b: i64) -> i64;  // Breaking: i32 → i64
    async fn sub(&self, a: i64, b: i64) -> i64;  // New method
}
```

**Server implementation**:
```rust
struct MyCalculator;

impl CalculatorV1 for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> i32 { a + b }
}

impl CalculatorV2 for MyCalculator {
    async fn add(&self, a: i64, b: i64) -> i64 { a + b }
    async fn sub(&self, a: i64, b: i64) -> i64 { a - b }
}

// Server registers both versions:
server.register(CalculatorV1Server::new(MyCalculator));
server.register(CalculatorV2Server::new(MyCalculator));
```

**Client usage**:
```rust
// Old clients use V1
let v1_client = CalculatorV1Client::new(transport);
v1_client.add(1, 2).await?;

// New clients use V2
let v2_client = CalculatorV2Client::new(transport);
v2_client.add(1i64, 2i64).await?;
v2_client.sub(10, 5).await?;
```

**Compatibility**: V1 and V2 methods have different hashes, so they coexist on the same connection.

### Strategy 2: Wrapper Types for Additive Changes

For non-breaking additions, use wrapper types:

```rust
// Original
#[derive(Facet)]
pub struct UserV1 {
    pub id: u64,
    pub name: String,
}

// Want to add email? Wrap the old type
#[derive(Facet)]
pub struct UserV2 {
    pub v1: UserV1,              // Embed old version
    pub email: Option<String>,   // New field
}

#[rapace::service]
pub trait UserServiceV2 {
    async fn get_user(&self, id: u64) -> UserV2;
}
```

**Wire representation**:
```
UserV2:
  v1.id: u64
  v1.name: String
  email: Option<String>
```

This is structurally different from `UserV1`, but both can exist on the same server.

**Caveat**: This is **not** backward-compatible encoding. `UserV1` and `UserV2` are different types with different hashes. Use this when you want explicit opt-in to new fields.

### Strategy 3: Feature Flags and Capabilities

For optional features, negotiate capabilities at handshake:

```rust
struct HandshakeInfo {
    methods: HashMap<String, MethodHash>,
    capabilities: HashSet<String>,  // e.g., "streaming", "compression", "extended-errors"
}
```

Clients check capabilities before using optional methods:

```rust
if handshake.capabilities.contains("streaming") {
    // Use streaming methods
    client.stream_data(...).await?;
} else {
    // Fall back to unary methods
    client.upload_batch(...).await?;
}
```

## What Breaks Compatibility

### Struct Changes

❌ **Reordering fields**:
```rust
// Before
struct Point { x: i32, y: i32 }

// After (BREAKS)
struct Point { y: i32, x: i32 }
```

❌ **Adding fields**:
```rust
// Before
struct Point { x: i32, y: i32 }

// After (BREAKS)
struct Point { x: i32, y: i32, z: i32 }
```

❌ **Removing fields**:
```rust
// Before
struct Point { x: i32, y: i32, z: i32 }

// After (BREAKS)
struct Point { x: i32, y: i32 }
```

❌ **Changing field types**:
```rust
// Before
struct Point { x: i32, y: i32 }

// After (BREAKS)
struct Point { x: f64, y: f64 }
```

✅ **Renaming struct** (structural typing):
```rust
// Before
struct Point { x: i32, y: i32 }

// After (OK)
struct Coordinate { x: i32, y: i32 }
```

### Enum Changes

❌ **Adding variants**:
```rust
// Before
enum Color { Red, Green, Blue }

// After (BREAKS)
enum Color { Red, Green, Blue, Yellow }
```

❌ **Removing variants**:
```rust
// Before
enum Color { Red, Green, Blue }

// After (BREAKS)
enum Color { Red, Green }
```

❌ **Reordering variants**:
```rust
// Before
enum Color { Red, Green, Blue }

// After (BREAKS)
enum Color { Green, Red, Blue }
```

❌ **Changing variant payload**:
```rust
// Before
enum Shape { Circle(f32) }

// After (BREAKS)
enum Shape { Circle(f64) }
```

✅ **Renaming enum or variant** (if structure unchanged):
```rust
// Before
enum Color { Red, Green, Blue }

// After (OK, structurally identical)
enum Colour { Red, Green, Blue }
```

### Method Signature Changes

❌ **Changing argument types**:
```rust
// Before
async fn add(&self, a: i32, b: i32) -> i64;

// After (BREAKS)
async fn add(&self, a: i64, b: i64) -> i64;
```

❌ **Changing return type**:
```rust
// Before
async fn get_user(&self, id: u64) -> User;

// After (BREAKS)
async fn get_user(&self, id: u64) -> Option<User>;
```

❌ **Adding/removing arguments**:
```rust
// Before
async fn log(&self, message: String);

// After (BREAKS)
async fn log(&self, message: String, level: LogLevel);
```

✅ **Renaming method** (method name is part of hash, but service can register multiple names):
```rust
// Can register same implementation under two names:
service.register_method("add", add_impl);
service.register_method("plus", add_impl);  // Alias
```

## Migration Workflow

**When making breaking changes:**

1. **Define new version** (e.g., `ServiceV2`, `TypeV2`)
2. **Implement new version** on server
3. **Keep old version running** (dual implementation)
4. **Deploy server** (now serves V1 + V2)
5. **Migrate clients** gradually to V2
6. **Monitor V1 usage** (metrics/logs)
7. **Deprecate V1** (return deprecation warnings)
8. **Remove V1** once all clients migrated

**Example timeline**:
```
Week 0: Deploy V2 server (V1 + V2 coexist)
Week 2: All new clients use V2
Week 4: Migrate 50% of old clients to V2
Week 6: Migrate 90% of old clients to V2
Week 8: Deprecation warnings for V1
Week 10: Remove V1 from server
```

## Schema Registry (Optional)

For large deployments, consider a **central schema registry**:

```
┌─────────┐          ┌──────────────┐          ┌────────┐
│ Service │─────────▶│ Registry     │◀─────────│ Client │
│ (Rust)  │  Publish │ (stores      │  Fetch   │ (TS)   │
└─────────┘   schemas│  schemas)    │  schemas └────────┘
                      └──────────────┘
```

**Benefits**:
- Centralized documentation (all schemas in one place)
- Breaking change detection (CI can reject incompatible changes)
- Client codegen (fetch schemas, generate bindings)
- Version history (track schema evolution over time)

**Implementation** (planned):
- `rapace-registry` crate: Extract facet shapes at build time
- Registry server: Store and serve schemas
- CI integration: Validate compatibility before merge

## Summary

| Aspect | Rule |
|--------|------|
| **Default** | Breaking changes only |
| **Detection** | Hash mismatch at handshake |
| **Mitigation** | Explicit versioning (V1, V2, ...) |
| **Structural typing** | Types with same structure are compatible |
| **Field order** | Immutable (part of schema hash) |
| **Migration** | Server runs multiple versions simultaneously |

For details on hash computation, see [Code Generation](@/spec/codegen.md).
For handshake protocol, see [Handshake & Capabilities](@/spec/handshake.md).
