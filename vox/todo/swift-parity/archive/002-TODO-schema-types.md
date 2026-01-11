# Phase 002: Schema Types in Swift

**Status**: TODO

## Objective

Port the TypeScript `Schema` type hierarchy to Swift, providing the foundation for
schema-driven serialization and channel binding.

## Background

TypeScript has a comprehensive schema system in `roam-postcard/src/schema.ts`:

```typescript
export type Schema =
  | { kind: PrimitiveKind }
  | TxSchema
  | RxSchema
  | VecSchema
  | OptionSchema
  | MapSchema
  | StructSchema
  | EnumSchema
  | TupleSchema
  | RefSchema;
```

Swift needs an equivalent system. Unlike TypeScript's structural typing, Swift uses
nominal typing, so we'll use an enum with associated values.

## Design

### Schema Enum

```swift
/// Schema describing a type's wire format for postcard serialization.
public indirect enum Schema: Equatable, Sendable {
    // Primitives
    case bool
    case u8, u16, u32, u64
    case i8, i16, i32, i64
    case f32, f64
    case string
    case bytes
    
    // Containers
    case vec(element: Schema)
    case option(inner: Schema)
    case map(key: Schema, value: Schema)
    
    // Composites
    case `struct`(StructSchema)
    case `enum`(EnumSchema)
    case tuple(elements: [Schema])
    
    // Streaming
    case tx(element: Schema)
    case rx(element: Schema)
    
    // References (for complex/circular types)
    case ref(name: String)
}
```

### StructSchema

```swift
/// Schema for a struct with named fields.
public struct StructSchema: Equatable, Sendable {
    /// Struct name (for debugging/error messages).
    public let name: String
    
    /// Fields in declaration order. Order matters for wire format.
    public let fields: [(name: String, schema: Schema)]
    
    public init(name: String, fields: [(String, Schema)]) {
        self.name = name
        self.fields = fields
    }
}
```

### EnumSchema

```swift
/// Schema for an enum with variants.
public struct EnumSchema: Equatable, Sendable {
    /// Enum name (for debugging/error messages).
    public let name: String
    
    /// Variants in declaration order.
    public let variants: [EnumVariant]
    
    public init(name: String, variants: [EnumVariant]) {
        self.name = name
        self.variants = variants
    }
}

/// A variant in an enum.
public struct EnumVariant: Equatable, Sendable {
    /// Variant name (e.g., "Hello", "Goodbye").
    public let name: String
    
    /// Wire discriminant value. If nil, uses index in variants array.
    public let discriminant: UInt32?
    
    /// Variant fields.
    public let fields: VariantFields
    
    public init(name: String, discriminant: UInt32? = nil, fields: VariantFields = .unit) {
        self.name = name
        self.discriminant = discriminant
        self.fields = fields
    }
}

/// Fields of an enum variant.
public enum VariantFields: Equatable, Sendable {
    /// Unit variant (no fields): `case none`
    case unit
    
    /// Newtype variant (single unnamed field): `case some(T)`
    case newtype(Schema)
    
    /// Tuple variant (multiple unnamed fields): `case pair(A, B)`
    case tuple([Schema])
    
    /// Struct variant (named fields): `case request(id: UInt64, method: UInt64)`
    case `struct`([(name: String, schema: Schema)])
}
```

### SchemaRegistry

```swift
/// Registry of named type schemas for resolving references.
public final class SchemaRegistry: @unchecked Sendable {
    private var schemas: [String: Schema] = [:]
    private let lock = NSLock()
    
    public init() {}
    
    public init(_ entries: [(String, Schema)]) {
        for (name, schema) in entries {
            schemas[name] = schema
        }
    }
    
    public func register(name: String, schema: Schema) {
        lock.lock()
        defer { lock.unlock() }
        schemas[name] = schema
    }
    
    public func resolve(name: String) -> Schema? {
        lock.lock()
        defer { lock.unlock() }
        return schemas[name]
    }
}
```

### Helper Functions

```swift
/// Find a variant by discriminant value (for decoding).
public func findVariant(in schema: EnumSchema, discriminant: UInt32) -> EnumVariant? {
    for (index, variant) in schema.variants.enumerated() {
        let variantDiscriminant = variant.discriminant ?? UInt32(index)
        if variantDiscriminant == discriminant {
            return variant
        }
    }
    return nil
}

/// Find a variant by name (for encoding).
public func findVariant(in schema: EnumSchema, name: String) -> EnumVariant? {
    schema.variants.first { $0.name == name }
}

/// Get the discriminant for a variant.
public func getDiscriminant(in schema: EnumSchema, variant: EnumVariant) -> UInt32 {
    if let explicit = variant.discriminant {
        return explicit
    }
    guard let index = schema.variants.firstIndex(where: { $0.name == variant.name }) else {
        fatalError("Variant \(variant.name) not found in schema")
    }
    return UInt32(index)
}

/// Resolve a schema reference.
public func resolveSchema(_ schema: Schema, registry: SchemaRegistry) throws -> Schema {
    guard case .ref(let name) = schema else {
        return schema
    }
    guard let resolved = registry.resolve(name: name) else {
        throw SchemaError.unknownType(name)
    }
    return resolved
}

public enum SchemaError: Error {
    case unknownType(String)
}
```

### Checking for Streaming Types

```swift
extension Schema {
    /// Returns true if this schema is a Tx type.
    public var isTx: Bool {
        if case .tx = self { return true }
        return false
    }
    
    /// Returns true if this schema is an Rx type.
    public var isRx: Bool {
        if case .rx = self { return true }
        return false
    }
    
    /// Returns true if this schema contains any streaming types (Tx or Rx).
    public var containsStreaming: Bool {
        switch self {
        case .tx, .rx:
            return true
        case .vec(let element):
            return element.containsStreaming
        case .option(let inner):
            return inner.containsStreaming
        case .map(let key, let value):
            return key.containsStreaming || value.containsStreaming
        case .struct(let s):
            return s.fields.contains { $0.schema.containsStreaming }
        case .enum(let e):
            return e.variants.contains { variant in
                switch variant.fields {
                case .unit: return false
                case .newtype(let schema): return schema.containsStreaming
                case .tuple(let schemas): return schemas.contains { $0.containsStreaming }
                case .struct(let fields): return fields.contains { $0.schema.containsStreaming }
                }
            }
        case .tuple(let elements):
            return elements.contains { $0.containsStreaming }
        case .ref:
            // Cannot check without resolving - caller should resolve first
            return false
        default:
            return false
        }
    }
}
```

## Files to Create/Modify

| File | Action |
|------|--------|
| `swift/roam-runtime/Sources/RoamRuntime/Schema.swift` | CREATE or REPLACE |
| `swift/roam-runtime/Tests/RoamRuntimeTests/SchemaTests.swift` | CREATE |

## Implementation Steps

1. Create `Schema.swift` with all types defined above
2. Add helper functions
3. Add `containsStreaming` computed property
4. Write unit tests for:
   - Variant lookup by discriminant
   - Variant lookup by name
   - Discriminant calculation (explicit vs index)
   - Schema resolution
   - Streaming type detection

## Success Criteria

1. `Schema` enum compiles with all variants
2. `StructSchema` and `EnumSchema` are defined
3. `EnumVariant` supports unit, newtype, tuple, and struct variants
4. `SchemaRegistry` provides thread-safe type lookup
5. Helper functions work correctly
6. Unit tests pass

## Test Cases

```swift
func testFindVariantByDiscriminant() {
    let schema = EnumSchema(name: "Message", variants: [
        EnumVariant(name: "Hello", discriminant: 0, fields: .newtype(.ref("Hello"))),
        EnumVariant(name: "Goodbye", discriminant: 1, fields: .struct([("reason", .string)])),
        EnumVariant(name: "Cancel", discriminant: 4, fields: .struct([("requestId", .u64)])),
    ])
    
    XCTAssertEqual(findVariant(in: schema, discriminant: 0)?.name, "Hello")
    XCTAssertEqual(findVariant(in: schema, discriminant: 1)?.name, "Goodbye")
    XCTAssertNil(findVariant(in: schema, discriminant: 2))
    XCTAssertNil(findVariant(in: schema, discriminant: 3))
    XCTAssertEqual(findVariant(in: schema, discriminant: 4)?.name, "Cancel")
}

func testImplicitDiscriminant() {
    let schema = EnumSchema(name: "Color", variants: [
        EnumVariant(name: "Red"),
        EnumVariant(name: "Green"),
        EnumVariant(name: "Blue"),
    ])
    
    XCTAssertEqual(findVariant(in: schema, discriminant: 0)?.name, "Red")
    XCTAssertEqual(findVariant(in: schema, discriminant: 1)?.name, "Green")
    XCTAssertEqual(findVariant(in: schema, discriminant: 2)?.name, "Blue")
}

func testContainsStreaming() {
    XCTAssertTrue(Schema.tx(element: .i32).containsStreaming)
    XCTAssertTrue(Schema.rx(element: .string).containsStreaming)
    XCTAssertTrue(Schema.vec(element: .tx(element: .i32)).containsStreaming)
    XCTAssertTrue(Schema.option(inner: .rx(element: .u8)).containsStreaming)
    XCTAssertFalse(Schema.string.containsStreaming)
    XCTAssertFalse(Schema.vec(element: .i32).containsStreaming)
}
```

## Notes

- Use `indirect enum` because `Schema` is recursive (vec contains Schema, etc.)
- Make everything `Sendable` for Swift concurrency
- `StructSchema.fields` uses tuple array `[(String, Schema)]` to preserve order
- `EnumVariant.discriminant` is optional — nil means use array index
- Thread-safe `SchemaRegistry` uses `NSLock` (simple and works)

## Dependencies

- None (this is foundational)

## Blocked By

- Phase 001 (need to understand current `Schema.swift` state)
