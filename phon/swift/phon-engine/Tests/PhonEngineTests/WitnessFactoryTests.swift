// The generic witness factories (PhonIR/Witnesses.swift) that codegen emits as
// one-liners. Validated against the compact/Value oracle, with emphasis on the
// non-trivial cases (Optional<String>, [String]) the hand-written oracle tests
// don't cover.

import Testing

@testable import PhonEngine
import PhonIR
import PhonSchema

private struct OptStr: Equatable {
    var s: String?
}

@Test
func optionWitnessFactoryHandlesManagedInner() throws {
    // OptStr { s: option<string> } — a managed (non-trivial) Optional inner.
    let optStr = Schema(id: SchemaId(2), kind: .option(element: .concrete(primitiveId(.string))))
    let holder = Schema(id: SchemaId(1), kind: .structure(name: "OptStr", fields: [
        Field(name: "s", schema: .concrete(SchemaId(2)), required: true),
    ]))
    let reg = Registry([optStr, holder])

    let strDesc = Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: .string))
    )
    let optDesc = Descriptor(
        schema: .concrete(SchemaId(2)),
        layout: Layout(size: MemoryLayout<String?>.size, align: MemoryLayout<String?>.alignment),
        access: .option(OptionAccess(witness: .of(String.self), some: strDesc))
    )
    let holderDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<OptStr>.size, align: MemoryLayout<OptStr>.alignment),
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<OptStr>.offset(of: \OptStr.s)!, descriptor: optDesc)],
            construct: .inPlace
        ))
    )
    let program = try lowerTyped(holderDesc, reg)

    for holder in [OptStr(s: "héllo"), OptStr(s: nil), OptStr(s: "")] {
        let typedBytes = withUnsafeBytes(of: holder) { encodeWith(program, $0.baseAddress!) }
        let oracleField: Value = holder.s.map { .string($0) } ?? .null
        let oracleBytes = try encode(.object([.init(key: "s", value: oracleField)]), SchemaId(1), reg)
        #expect(typedBytes == oracleBytes, "opt<string> \(String(describing: holder.s)): bytes diverge")

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<OptStr>.size, alignment: MemoryLayout<OptStr>.alignment)
        defer { raw.deallocate() }
        try decodeInto(program, typedBytes, raw)
        let decoded = raw.assumingMemoryBound(to: OptStr.self).move()
        #expect(decoded == holder, "opt<string> \(String(describing: holder.s)): decode mismatch")
    }
}

@Test
func sequenceWitnessFactoryHandlesStrings() throws {
    // [String] via SeqWitness.of(String.self).
    let listSchema = Schema(id: SchemaId(1), kind: .list(element: .concrete(primitiveId(.string))))
    let reg = Registry([listSchema])
    let elemDesc = Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: .string))
    )
    let listDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<[String]>.size, align: MemoryLayout<[String]>.alignment),
        access: .sequence(SequenceAccess(
            element: elemDesc, stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment, witness: .of(String.self)))
    )
    let program = try lowerTyped(listDesc, reg)

    let value = ["a", "béta", ""]
    let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }
    let oracleBytes = try encode(.array(value.map { .string($0) }), SchemaId(1), reg)
    #expect(typedBytes == oracleBytes)

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<[String]>.size, alignment: MemoryLayout<[String]>.alignment)
    defer { raw.deallocate() }
    try decodeInto(program, typedBytes, raw)
    #expect(raw.assumingMemoryBound(to: [String].self).move() == value)
}
