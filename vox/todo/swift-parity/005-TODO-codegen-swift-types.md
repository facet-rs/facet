# Phase 005: Code Generation - Swift Types

**Status**: TODO

## Objective

Extend `roam-codegen` to generate Swift type definitions from Rust service definitions,
producing structs, enums, and type aliases that match the Rust originals.

## Background

`roam-codegen` already has a `targets/swift/` directory with `types.rs`. This phase
is about auditing, completing, and testing that code generation.

The generated Swift types need to:
1. Match the Rust type structure exactly
2. Be usable with SwiftNIO and async/await
3. Support the `PostcardCodable` protocol (generated in phase 007)
4. Work with channel binding (phase 010)

## Current State

Check `rust/roam-codegen/src/targets/swift/types.rs` to understand:
- What type mappings exist?
- Does it handle all facet shapes?
- What's the output format?

## Design

### Type Mappings

| Rust Type | Swift Type |
|-----------|------------|
| `bool` | `Bool` |
| `u8` | `UInt8` |
| `u16` | `UInt16` |
| `u32` | `UInt32` |
| `u64` | `UInt64` |
| `i8` | `Int8` |
| `i16` | `Int16` |
| `i32` | `Int32` |
| `i64` | `Int64` |
| `f32` | `Float` |
| `f64` | `Double` |
| `String` | `String` |
| `Vec<u8>` | `[UInt8]` or `Data` |
| `Vec<T>` | `[T]` |
| `Option<T>` | `T?` |
| `HashMap<K, V>` | `[K: V]` |
| `(A, B, ...)` | `(A, B, ...)` |
| `Tx<T>` | `Tx<T>` |
| `Rx<T>` | `Rx<T>` |

### Struct Generation

Rust:
```rust
#[derive(Facet)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
```

Generated Swift:
```swift
public struct Point: Equatable, Sendable {
    public var x: Int32
    public var y: Int32
    
    public init(x: Int32, y: Int32) {
        self.x = x
        self.y = y
    }
}
```

### Enum Generation

Rust:
```rust
#[derive(Facet)]
pub enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Point,
}
```

Generated Swift:
```swift
public enum Shape: Equatable, Sendable {
    case circle(radius: Double)
    case rectangle(width: Double, height: Double)
    case point
}
```

### Enum with Explicit Discriminants

Rust:
```rust
#[repr(u8)]
#[derive(Facet)]
pub enum Message {
    Hello(Hello) = 0,
    Goodbye { reason: String } = 1,
    Request { ... } = 2,
}
```

Generated Swift:
```swift
public enum Message: Equatable, Sendable {
    case hello(Hello)
    case goodbye(reason: String)
    case request(requestId: UInt64, methodId: UInt64, metadata: [(String, MetadataValue)], payload: [UInt8])
}
```

Note: Swift doesn't have explicit discriminants on enums, but the schema will encode
the discriminant values for serialization.

### Newtype Wrappers

Rust:
```rust
#[derive(Facet)]
pub struct UserId(pub u64);
```

Generated Swift:
```swift
public struct UserId: Equatable, Hashable, Sendable {
    public var value: UInt64
    
    public init(_ value: UInt64) {
        self.value = value
    }
}
```

### Type Aliases

Rust:
```rust
pub type ChannelId = u64;
```

Generated Swift:
```swift
public typealias ChannelId = UInt64
```

### Result Types

Rust:
```rust
pub type LoadResult = Result<Template, LoadError>;
```

Generated Swift:
```swift
public enum LoadResult: Equatable, Sendable {
    case ok(Template)
    case err(LoadError)
}
```

Or using Swift's native `Result`:
```swift
public typealias LoadResult = Result<Template, LoadError>
```

### RoamError

The `RoamError<E>` type is special — it's the wrapper for all RPC responses.

Generated Swift:
```swift
public enum RoamError<E: Equatable & Sendable>: Error, Equatable, Sendable {
    case user(E)
    case unknownMethod
    case invalidPayload
    case cancelled
}
```

## Implementation in roam-codegen

### Entry Point

```rust
// rust/roam-codegen/src/targets/swift/types.rs

pub fn generate_type(shape: &'static Shape) -> String {
    // Generate Swift type definition from facet Shape
}

pub fn swift_type_name(shape: &'static Shape) -> String {
    // Return the Swift type name for a shape
}
```

### Collecting Named Types

```rust
pub fn collect_named_types(service: &ServiceDetail) -> Vec<&'static Shape> {
    // Walk all method signatures and collect unique named types
    // (structs, enums, newtypes) that need definitions
}
```

### Output Structure

Generated file structure:
```
swift/generated/
├── Testbed.swift           # Types + client + server for Testbed service
├── Calculator.swift        # Types + client + server for Calculator service
└── Wire.swift              # Wire protocol types (Message, Hello, etc.)
```

Or per-component:
```
swift/generated/
├── TestbedTypes.swift
├── TestbedSchemas.swift
├── TestbedClient.swift
├── TestbedServer.swift
└── ...
```

## Files to Modify

| File | Action |
|------|--------|
| `rust/roam-codegen/src/targets/swift/types.rs` | AUDIT + EXTEND |
| `rust/roam-codegen/src/targets/swift/mod.rs` | AUDIT + EXTEND |
| `xtask/src/main.rs` | Ensure `codegen --swift` works |

## Implementation Steps

1. Audit existing `types.rs` — what does it already do?
2. Map all facet shapes to Swift types
3. Generate struct definitions with `public var` fields
4. Generate enum definitions with associated values
5. Generate newtype wrappers
6. Generate type aliases
7. Handle `Result` types appropriately
8. Add `Equatable`, `Sendable` conformances
9. Test generated code compiles

## Success Criteria

1. `cargo xtask codegen --swift` produces Swift type files
2. Generated types compile without errors
3. All primitive types map correctly
4. Structs have correct field names and types
5. Enums have correct case names and associated values
6. Generated types are `Equatable` and `Sendable`

## Test Cases

Generate types for the testbed service and verify:

```swift
// Generated Point struct should work like this:
let p = Point(x: 10, y: 20)
XCTAssertEqual(p.x, 10)
XCTAssertEqual(p.y, 20)

// Generated Shape enum should work like this:
let s = Shape.circle(radius: 5.0)
switch s {
case .circle(let r): XCTAssertEqual(r, 5.0)
default: XCTFail()
}

// Should be Equatable
XCTAssertEqual(Point(x: 1, y: 2), Point(x: 1, y: 2))
XCTAssertNotEqual(Point(x: 1, y: 2), Point(x: 3, y: 4))
```

## Notes

- Swift uses `var` for mutable fields, `let` for immutable
- Use `public var` for fields that need mutation during channel binding
- `Sendable` is required for Swift concurrency
- Tuple types in Swift use parentheses: `(String, Int)`
- Swift doesn't have trait bounds, so generic constraints are different
- Consider whether to generate `Codable` conformance for JSON debugging

## Dependencies

- Phase 001 (understand current codegen state)

## Blocked By

- Phase 001 should identify current codegen capabilities
