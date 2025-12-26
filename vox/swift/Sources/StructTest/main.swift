import Foundation
import Postcard

// MARK: - Test Structs (matching Rust definitions)

/// Simple struct with basic types
struct Point: PostcardEncodable {
    var x: Int32
    var y: Int32

    func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(x)
        encoder.encode(y)
    }
}

/// Struct with various field types
struct Person: PostcardEncodable {
    var name: String
    var age: UInt32
    var score: Double
    var active: Bool

    func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(name)
        encoder.encode(age)
        encoder.encode(score)
        encoder.encode(active)
    }
}

/// Struct with optional and vector fields
struct ComplexStruct: PostcardEncodable {
    var id: UInt64
    var tags: [String]
    var metadata: String?

    func encode(to encoder: inout PostcardEncoder) {
        encoder.encode(id)
        encoder.encode(tags)
        encoder.encode(metadata, using: { enc, val in enc.encode(val) })
    }
}

/// Nested struct
struct Nested: PostcardEncodable {
    var point: Point
    var label: String

    func encode(to encoder: inout PostcardEncoder) {
        point.encode(to: &encoder)  // Structs encode fields directly, no wrapper
        encoder.encode(label)
    }
}

// MARK: - Helper functions

func hex(_ bytes: [UInt8]) -> String {
    bytes.map { String(format: "%02x", $0) }.joined(separator: " ")
}

func printTestCase<T: PostcardEncodable>(_ desc: String, _ value: T) {
    let bytes = PostcardEncoder.encode(value)
    print("\(desc):")
    print("  Bytes: \(hex(bytes))")
    print("  Length: \(bytes.count) bytes")
    print()
}

// MARK: - Main

print("=== Swift Struct Serialization Test Vectors ===\n")

// Test 1: Simple Point
let point = Point(x: 10, y: -5)
printTestCase("Point { x: 10, y: -5 }", point)

// Test 2: Point with larger values
let point2 = Point(x: 1000, y: -1000)
printTestCase("Point { x: 1000, y: -1000 }", point2)

// Test 3: Person struct
let person = Person(
    name: "Alice",
    age: 30,
    score: 95.5,
    active: true
)
printTestCase("Person { name: \"Alice\", age: 30, score: 95.5, active: true }", person)

// Test 4: ComplexStruct with Some
let complex = ComplexStruct(
    id: 12345,
    tags: ["rust", "swift"],
    metadata: "test data"
)
printTestCase("ComplexStruct with Some metadata", complex)

// Test 5: ComplexStruct with None
let complexNone = ComplexStruct(
    id: 999,
    tags: [],
    metadata: nil
)
printTestCase("ComplexStruct with None metadata", complexNone)

// Test 6: Nested struct
let nested = Nested(
    point: Point(x: 42, y: -42),
    label: "origin"
)
printTestCase("Nested { point: Point { x: 42, y: -42 }, label: \"origin\" }", nested)

// Print raw arrays for comparison
print("\n=== Raw Test Vectors (for comparison with Rust) ===\n")

print("// Point { x: 10, y: -5 }")
print("let pointBytes: [UInt8] = \(PostcardEncoder.encode(point))")

print("\n// Person { name: \"Alice\", age: 30, score: 95.5, active: true }")
print("let personBytes: [UInt8] = \(PostcardEncoder.encode(person))")

print("\n// ComplexStruct with tags and Some metadata")
print("let complexBytes: [UInt8] = \(PostcardEncoder.encode(complex))")

print("\n// Nested struct")
print("let nestedBytes: [UInt8] = \(PostcardEncoder.encode(nested))")
