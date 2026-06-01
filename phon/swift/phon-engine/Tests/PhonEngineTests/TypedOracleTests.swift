// The typed-path oracle: the typed memory codec must produce byte-identical
// output to the dynamic compact/Value codec for the same logical value, and must
// round-trip. This is the equivalence Rust's typed.rs cites — verified here over
// real Swift values and hand-built descriptors.
//
// First milestone: fixed scalars and in-place records.

import Testing

@testable import PhonEngine
import PhonIR
import PhonSchema

// A struct of mixed-alignment scalars. Swift lays x at 0, then pads to 8 for y;
// the wire pads after x's 4 bytes to 8-align y — different layouts, identical
// bytes, which is exactly what the typed path must reconcile.
private struct Point: Equatable {
    var x: UInt32
    var y: Double
}

private func pointSchemaAndRegistry() -> (root: SchemaId, reg: Registry, desc: Descriptor) {
    // Schema: Point { x: u32, y: f64 } at a provisional id.
    let point = Schema(
        id: SchemaId(1),
        kind: .structure(name: "Point", fields: [
            Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "y", schema: .concrete(primitiveId(.f64)), required: true),
        ])
    )
    let reg = Registry([point])

    let xOffset = MemoryLayout<Point>.offset(of: \Point.x)!
    let yOffset = MemoryLayout<Point>.offset(of: \Point.y)!
    let desc = Descriptor(
        schema: .concrete(id: SchemaId(1), args: []),
        layout: Layout(size: MemoryLayout<Point>.size, align: MemoryLayout<Point>.alignment),
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: xOffset, descriptor: scalarDesc(.u32)),
                FieldAccess(offset: yOffset, descriptor: scalarDesc(.f64)),
            ],
            construct: .inPlace
        ))
    )
    return (SchemaId(1), reg, desc)
}

private func scalarDesc(_ p: Primitive) -> Descriptor {
    let size = fixedSize(p)!
    return Descriptor(
        schema: .concrete(primitiveId(p)),
        layout: Layout(size: size, align: alignment(p)),
        access: .scalar
    )
}

@Test
func typedRecordMatchesValueOracleAndRoundTrips() throws {
    let (root, reg, desc) = pointSchemaAndRegistry()
    let program = try lowerTyped(desc, reg)

    let value = Point(x: 7, y: 2.5)

    // typed encode: read the value's memory directly.
    let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }

    // oracle: the same logical value as a phon Value, compact-encoded.
    let oracle: Value = .object([
        .init(key: "x", value: .number(.canonical(unsigned: 7))),
        .init(key: "y", value: .number(.f64(2.5))),
    ])
    let oracleBytes = try encode(oracle, root, reg)

    #expect(typedBytes == oracleBytes, "typed bytes diverge from the Value oracle")

    // The compact encode of Point is 4 (x) + 4 (pad to 8) + 8 (y) = 16 bytes.
    #expect(typedBytes.count == 16)

    // typed decode: round-trip back into a fresh Point.
    var decoded = Point(x: 0, y: 0)
    try withUnsafeMutableBytes(of: &decoded) { try decodeInto(program, typedBytes, $0.baseAddress!) }
    #expect(decoded == value, "typed decode did not round-trip")
}
