import PhonEngine
import PhonIR
@testable import PhonJIT
import PhonSchema
import Testing

// r[verify crates.jit-opt-in]
// r[verify ir.stencils]
// r[verify exec.jit-optional]
@Test func smokeStencilRunsFromExecutableMemory() throws {
    #expect(try PhonJIT.smoke(0) == 1)
    #expect(try PhonJIT.smoke(7) == 22)
    #expect(try PhonJIT.smoke(-3) == -8)
}

// r[verify ir.stencils]
private struct Pair: Equatable {
    var a: UInt32
    var b: UInt32
}

// r[verify ir.stencils]
private func u32Field(_ name: String) -> Field {
    Field(name: name, schema: .concrete(primitiveId(.u32)), required: true)
}

// r[verify ir.stencils]
private func u32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.u32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

// r[verify ir.stencils]
private func pairDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "Pair", fields: [u32Field("a"), u32Field("b")]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<Pair>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.a)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.b)!, descriptor: u32Desc()),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([schema]))
}

// r[verify ir.stencils]
// r[verify exec.jit-optional]
@Test func nativeScalarEncodeDecodeRecord() throws {
    let (desc, reg) = pairDescriptor()
    let lowered = try lowerTyped(desc, reg)

    let encoder = try NativeScalarEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }

    let value = Pair(a: 7, b: 99)
    let bytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    #expect(bytes == [7, 0, 0, 0, 99, 0, 0, 0])

    let decoder = try NativeScalarDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<Pair>.size,
        alignment: MemoryLayout<Pair>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run(bytes, raw)
    let decoded = raw.assumingMemoryBound(to: Pair.self).move()
    #expect(decoded == value)
}
