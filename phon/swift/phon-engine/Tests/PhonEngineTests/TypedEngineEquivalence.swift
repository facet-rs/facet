// The cross-engine equivalence harness: one definition, every backend.
//
// `assertTypedEquivalence` encodes a value through each `TypedEngine` and asserts
// (1) every engine agrees byte-for-byte, (2) the bytes are canonical phon (the
// tree-walk decodes and re-emits them identically — an independent oracle that needs
// no per-type `T → Value` bridge), and (3) every engine round-trips back to the value.
//
// The corpus below is written ONCE. Adding the copy-and-patch JIT is a single line —
// append `JITEngine()` to `allTypedEngines` — and every case here gains
// `tree-walk == interpreter == JIT` coverage with no new tests.

import Testing

import PhonIR
import PhonSchema

@testable import PhonEngine

/// Backends every equivalence check runs. The JIT appends here once it lands.
let allTypedEngines: [any TypedEngine] = [InterpreterEngine()]

func assertTypedEquivalence<T: Equatable>(
    _ value: T, descriptor: Descriptor, registry: Registry,
    _ label: String = "",
    engines: [any TypedEngine] = allTypedEngines,
    sourceLocation: SourceLocation = #_sourceLocation
) throws {
    let root = descriptor.rootId
    var v = value

    // Reference bytes from the first engine.
    let ref = withUnsafeBytes(of: &v) { try! engines[0].compileEncode(descriptor, registry)($0.baseAddress!) }

    // Independent oracle: the tree-walk reads the typed bytes and re-emits them
    // identically — proving they are canonical phon for the value they encode.
    let dyn = try decode(ref, root, registry)
    #expect(
        try encode(dyn, root, registry) == ref,
        "\(label): typed bytes are not canonical phon", sourceLocation: sourceLocation)

    for e in engines {
        let bytes = withUnsafeBytes(of: &v) { try! e.compileEncode(descriptor, registry)($0.baseAddress!) }
        #expect(bytes == ref, "\(label): \(e.name) encode diverges", sourceLocation: sourceLocation)

        let dec = try e.compileDecode(root, descriptor, registry)
        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<T>.size, alignment: MemoryLayout<T>.alignment)
        defer { raw.deallocate() }
        try dec(bytes, raw)
        #expect(
            raw.assumingMemoryBound(to: T.self).move() == value,
            "\(label): \(e.name) round-trip diverges", sourceLocation: sourceLocation)
    }
}

// MARK: - A small corpus exercising several Access/MemOp shapes through the harness.

private func u32Field(_ name: String) -> Field {
    Field(name: name, schema: .concrete(primitiveId(.u32)), required: true)
}
private func u32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.u32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

private struct Pair: Equatable {
    var a: UInt32
    var b: UInt32
}

@Test func equivalenceRecord() throws {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "Pair", fields: [u32Field("a"), u32Field("b")]))
    let reg = Registry([schema])
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)), layout: MemoryLayout<Pair>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.a)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.b)!, descriptor: u32Desc()),
            ], construct: .inPlace)))
    try assertTypedEquivalence(Pair(a: 7, b: 99), descriptor: desc, registry: reg, "record")
}

private struct OptHolder: Equatable { var v: UInt32? }

@Test func equivalenceOption() throws {
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "H", fields: [
            Field(name: "v", schema: .concrete(SchemaId(2)), required: false),
        ])),
        Schema(id: SchemaId(2), kind: .option(element: .concrete(primitiveId(.u32)))),
    ])
    let reg = Registry(batch)
    let optDesc = Descriptor(
        schema: .concrete(batch[1].id),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc())))
    let desc = Descriptor(
        schema: .concrete(batch[0].id), layout: MemoryLayout<OptHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<OptHolder>.offset(of: \OptHolder.v)!, descriptor: optDesc)],
            construct: .inPlace)))
    try assertTypedEquivalence(OptHolder(v: 42), descriptor: desc, registry: reg, "option-some")
    try assertTypedEquivalence(OptHolder(v: nil), descriptor: desc, registry: reg, "option-none")
}
