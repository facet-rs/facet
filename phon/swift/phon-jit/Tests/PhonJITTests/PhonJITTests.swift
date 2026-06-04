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

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }

    let value = Pair(a: 7, b: 99)
    let bytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    #expect(bytes == [7, 0, 0, 0, 99, 0, 0, 0])

    let decoder = try NativeDecode.compile(lowered)
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

// r[verify ir.stencils]
private struct OptHolder: Equatable {
    var v: UInt32?
}

// r[verify ir.stencils]
private func optionDescriptor() -> (Descriptor, Registry) {
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "H", fields: [
            Field(name: "v", schema: .concrete(SchemaId(2)), required: false),
        ])),
        Schema(id: SchemaId(2), kind: .option(element: .concrete(primitiveId(.u32)))),
    ])
    let optDesc = Descriptor(
        schema: .concrete(batch[1].id),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let desc = Descriptor(
        schema: .concrete(batch[0].id),
        layout: MemoryLayout<OptHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<OptHolder>.offset(of: \OptHolder.v)!, descriptor: optDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry(batch))
}

// r[verify ir.stencils]
// r[verify exec.jit-optional]
@Test func nativeOptionCompiles() throws {
    let (desc, reg) = optionDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(try NativeEncode.compile(lowered) != nil)
    #expect(try NativeDecode.compile(lowered) != nil)
}

// r[verify ir.stencils]
// r[verify exec.jit-optional]
@Test func nativeOptionEncodesRecord() throws {
    let (desc, reg) = optionDescriptor()
    let lowered = try lowerTyped(desc, reg)
    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }

    let none = OptHolder(v: nil)
    let noneBytes = withUnsafeBytes(of: none) { encoder.run($0.baseAddress!) }
    #expect(noneBytes == [0])

    let some = OptHolder(v: 42)
    let someBytes = withUnsafeBytes(of: some) { encoder.run($0.baseAddress!) }
    #expect(someBytes == [1, 0, 0, 0, 42, 0, 0, 0])
}

// r[verify ir.stencils]
// r[verify exec.jit-optional]
@Test func nativeOptionEncodeDecodeRecord() throws {
    let (desc, reg) = optionDescriptor()
    let lowered = try lowerTyped(desc, reg)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }

    let some = OptHolder(v: 42)
    let someBytes = withUnsafeBytes(of: some) { encoder.run($0.baseAddress!) }
    #expect(someBytes == [1, 0, 0, 0, 42, 0, 0, 0])

    let none = OptHolder(v: nil)
    let noneBytes = withUnsafeBytes(of: none) { encoder.run($0.baseAddress!) }
    #expect(noneBytes == [0])

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let someRaw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<OptHolder>.size,
        alignment: MemoryLayout<OptHolder>.alignment
    )
    defer { someRaw.deallocate() }
    try decoder.run(someBytes, someRaw)
    let someDecoded = someRaw.assumingMemoryBound(to: OptHolder.self).move()
    #expect(someDecoded == some)

    let noneRaw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<OptHolder>.size,
        alignment: MemoryLayout<OptHolder>.alignment
    )
    defer { noneRaw.deallocate() }
    try decoder.run(noneBytes, noneRaw)
    let noneDecoded = noneRaw.assumingMemoryBound(to: OptHolder.self).move()
    #expect(noneDecoded == none)
}
