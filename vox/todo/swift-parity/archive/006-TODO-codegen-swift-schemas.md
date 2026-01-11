# Phase 006: Code Generation - Swift Schemas

**Status**: TODO

## Objective

Extend `roam-codegen` to generate Swift schema constants for each type, enabling
schema-driven serialization and channel binding.

## Background

TypeScript generates schema constants like:
```typescript
export const PointSchema: StructSchema = {
  kind: "struct",
  fields: { x: { kind: "i32" }, y: { kind: "i32" } }
};
```

Swift needs equivalent:
```swift
public static let pointSchema = Schema.struct(StructSchema(
    name: "Point",
    fields: [
        ("x", .i32),
        ("y", .i32),
    ]
))
```

## Design

### Schema Constants

For each named type, generate a static schema constant:

```swift
// Point.swift or PointSchemas.swift
extension Point {
    public static let schema = Schema.struct(StructSchema(
        name: "Point",
        fields: [
            ("x", .i32),
            ("y", .i32),
        ]
    ))
}
```

### Enum Schemas with Discriminants

```swift
extension Message {
    public static let schema = Schema.enum(EnumSchema(
        name: "Message",
        variants: [
            EnumVariant(name: "hello", discriminant: 0, fields: .newtype(.ref("Hello"))),
            EnumVariant(name: "goodbye", discriminant: 1, fields: .struct([
                ("reason", .string)
            ])),
            EnumVariant(name: "request", discriminant: 2, fields: .struct([
                ("requestId", .u64),
                ("methodId", .u64),
                ("metadata", .vec(element: .tuple(elements: [.string, .ref("MetadataValue")]))),
                ("payload", .bytes),
            ])),
            // ... more variants
        ]
    ))
}
```

### Schema Registry

Generate a registry containing all schemas for a service:

```swift
// TestbedSchemas.swift
public let testbedSchemaRegistry: SchemaRegistry = {
    let registry = SchemaRegistry()
    registry.register(name: "Point", schema: Point.schema)
    registry.register(name: "Shape", schema: Shape.schema)
    registry.register(name: "Color", schema: Color.schema)
    // ... all types used by the service
    return registry
}()
```

### Method Argument Schemas

For each method, generate schemas for the argument tuple:

```swift
extension TestbedSchemas {
    public enum sum {
        // sum(numbers: Rx<i32>) -> i64
        public static let argsSchema: [Schema] = [
            .rx(element: .i32)
        ]
        public static let returnSchema: Schema = .i64
    }
    
    public enum echo {
        // echo(message: String) -> String
        public static let argsSchema: [Schema] = [.string]
        public static let returnSchema: Schema = .string
    }
}
```

### Wire Type Schemas

Generate schemas for wire protocol types:

```swift
// WireSchemas.swift
public enum WireSchemas {
    public static let hello = Schema.enum(EnumSchema(
        name: "Hello",
        variants: [
            EnumVariant(name: "v1", discriminant: 0, fields: .struct([
                ("maxPayloadSize", .u32),
                ("initialChannelCredit", .u32),
            ])),
        ]
    ))
    
    public static let metadataValue = Schema.enum(EnumSchema(
        name: "MetadataValue",
        variants: [
            EnumVariant(name: "string", discriminant: 0, fields: .newtype(.string)),
            EnumVariant(name: "bytes", discriminant: 1, fields: .newtype(.bytes)),
            EnumVariant(name: "u64", discriminant: 2, fields: .newtype(.u64)),
        ]
    ))
    
    public static let message = Schema.enum(EnumSchema(
        name: "Message",
        variants: [
            // All 9 message variants with their discriminants
        ]
    ))
    
    public static let registry: SchemaRegistry = {
        let r = SchemaRegistry()
        r.register(name: "Hello", schema: hello)
        r.register(name: "MetadataValue", schema: metadataValue)
        r.register(name: "Message", schema: message)
        return r
    }()
}
```

## Implementation in roam-codegen

### Schema Generation

```rust
// rust/roam-codegen/src/targets/swift/schema.rs

pub fn generate_schema(shape: &'static Shape) -> String {
    // Generate Swift Schema expression from facet Shape
    match classify_shape(shape) {
        ShapeKind::Scalar(s) => generate_scalar_schema(s),
        ShapeKind::Struct(s) => generate_struct_schema(s),
        ShapeKind::Enum(e) => generate_enum_schema(e),
        // ...
    }
}

fn generate_struct_schema(shape: &StructShape) -> String {
    // Generate StructSchema with field names and schemas
}

fn generate_enum_schema(shape: &EnumShape) -> String {
    // Generate EnumSchema with variant names, discriminants, and field schemas
}
```

### Discriminant Extraction

For `#[repr(u8)]` enums, extract the discriminant values:

```rust
fn get_variant_discriminant(variant: &EnumVariant, index: usize) -> u32 {
    // If explicit discriminant, use it
    // Otherwise, use index
    variant.discriminant.unwrap_or(index as u32)
}
```

### Handling References

When generating schemas for complex types, use `.ref("TypeName")` instead of
inlining the full schema:

```rust
fn should_use_ref(shape: &Shape) -> bool {
    // Use ref for named structs and enums
    // Inline primitives, containers, tuples
    matches!(classify_shape(shape), ShapeKind::Struct(_) | ShapeKind::Enum(_))
}
```

## Files to Modify

| File | Action |
|------|--------|
| `rust/roam-codegen/src/targets/swift/schema.rs` | AUDIT + EXTEND |
| `rust/roam-codegen/src/targets/swift/mod.rs` | Integrate schema generation |

## Implementation Steps

1. Audit existing `schema.rs` — what does it do?
2. Implement primitive schema generation
3. Implement struct schema generation
4. Implement enum schema generation with discriminants
5. Implement container schema generation (vec, option, map)
6. Implement Tx/Rx schema generation
7. Generate schema extensions on types
8. Generate method argument schemas
9. Generate schema registry
10. Test generated schemas match expected values

## Success Criteria

1. Each generated type has a `static let schema` property
2. Enum schemas include correct discriminant values
3. Struct field order matches Rust declaration order
4. Schema registry contains all service types
5. Method schemas correctly describe arguments and return type
6. Generated schemas work with `encodeWithSchema`/`decodeWithSchema`

## Test Cases

```swift
func testPointSchema() {
    let schema = Point.schema
    guard case .struct(let s) = schema else {
        XCTFail("Expected struct schema")
        return
    }
    XCTAssertEqual(s.name, "Point")
    XCTAssertEqual(s.fields.count, 2)
    XCTAssertEqual(s.fields[0].0, "x")
    XCTAssertEqual(s.fields[1].0, "y")
}

func testMessageSchemaDiscriminants() {
    let schema = Message.schema
    guard case .enum(let e) = schema else {
        XCTFail("Expected enum schema")
        return
    }
    
    // Verify discriminants match Rust #[repr(u8)] values
    XCTAssertEqual(e.variants[0].discriminant, 0) // Hello
    XCTAssertEqual(e.variants[1].discriminant, 1) // Goodbye
    XCTAssertEqual(e.variants[4].discriminant, 4) // Cancel
}

func testSchemaEncode() throws {
    let point = Point(x: 10, y: -5)
    let encoded = try encodeWithSchema(point, schema: Point.schema)
    
    // x=10 zigzag=20, y=-5 zigzag=9
    XCTAssertEqual(encoded, [20, 9])
}
```

## Notes

- Field order in schemas must match Rust declaration order (postcard is order-dependent)
- Discriminant values must match Rust `#[repr(u8)]` assignments
- Use `.ref()` for named types to avoid infinite recursion and duplication
- The schema registry must be populated before using `decodeWithSchema` with refs
- Method schemas use array for args (tuple of all arguments)

## Dependencies

- Phase 002 (Schema types in Swift)
- Phase 005 (Type generation)

## Blocked By

- Phase 002 must define Schema types
- Phase 005 should be done so types exist to extend
