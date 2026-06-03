// The typed compat decode (`lowerDecode`) under schema differences. Each case
// builds writer bytes via the Value codec, decodes them into a concrete reader
// Swift value through `lowerDecode(writer -> reader)`, and checks the result
// against the Value-path planner (`planDecode`, the cross-engine oracle).
// Same-schema is verified to be the no-skip identity.

import Testing

@testable import PhonEngine
import PhonIR
import PhonSchema

private func u32Desc() -> Descriptor {
    Descriptor(
        schema: .concrete(primitiveId(.u32)),
        layout: Layout(size: 4, align: 4),
        access: .scalar
    )
}

private func u32Field(_ name: String) -> Field {
    Field(name: name, schema: .concrete(primitiveId(.u32)), required: true)
}

// MARK: - Writer-only field is skipped (forward compat)

private struct ReaderX: Equatable { var x: UInt32 }

// r[verify compat.skip-writer-only]
// r[verify compat.field-matching]
@Test
func compatWriterOnlyFieldSkipped() throws {
    // writer { x: u32, y: u32 } ; reader { x: u32 } — y is writer-only.
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "P", fields: [u32Field("x"), u32Field("y")])),
        Schema(id: SchemaId(2), kind: .structure(name: "P", fields: [u32Field("x")])),
    ])
    let writerRoot = batch[0].id
    let readerRoot = batch[1].id
    let reg = Registry(batch)

    let writerBytes = try encode(.object([
        .init(key: "x", value: .number(.canonical(unsigned: 7))),
        .init(key: "y", value: .number(.canonical(unsigned: 99))),
    ]), writerRoot, reg)

    let readerDesc = Descriptor(
        schema: .concrete(readerRoot),
        layout: Layout(size: MemoryLayout<ReaderX>.size, align: MemoryLayout<ReaderX>.alignment),
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<ReaderX>.offset(of: \ReaderX.x)!, descriptor: u32Desc())],
            construct: .inPlace))
    )
    let program = try lowerDecode(writerRoot, readerDesc, reg)

    var decoded = ReaderX(x: 0)
    try withUnsafeMutableBytes(of: &decoded) { try decodeInto(program, writerBytes, $0.baseAddress!) }
    #expect(decoded.x == 7, "x decodes, y is skipped")

    // Oracle: the Value planner translates to { x: 7 }.
    let oracle = try planDecode(writerBytes, writerRoot, readerRoot, reg)
    #expect(oracle == .object([.init(key: "x", value: .number(.canonical(unsigned: 7)))]))
}

// MARK: - Reader-only field is defaulted (backward compat)

private struct ReaderXC: Equatable {
    var x: UInt32
    var c: UInt32?
}

// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
@Test
func compatReaderOnlyFieldDefaulted() throws {
    // writer { x: u32 } ; reader { x: u32, c: option<u32> (non-required) }.
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "P", fields: [u32Field("x")])),
        Schema(id: SchemaId(2), kind: .structure(name: "P", fields: [
            u32Field("x"),
            Field(name: "c", schema: .concrete(SchemaId(3)), required: false),
        ])),
        Schema(id: SchemaId(3), kind: .option(element: .concrete(primitiveId(.u32)))),
    ])
    let writerRoot = batch[0].id
    let readerRoot = batch[1].id
    let optRoot = batch[2].id
    let reg = Registry(batch)

    let writerBytes = try encode(.object([
        .init(key: "x", value: .number(.canonical(unsigned: 7))),
    ]), writerRoot, reg)

    let optDesc = Descriptor(
        schema: .concrete(optRoot),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let readerDesc = Descriptor(
        schema: .concrete(readerRoot),
        layout: Layout(size: MemoryLayout<ReaderXC>.size, align: MemoryLayout<ReaderXC>.alignment),
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<ReaderXC>.offset(of: \ReaderXC.x)!, descriptor: u32Desc()),
            FieldAccess(
                offset: MemoryLayout<ReaderXC>.offset(of: \ReaderXC.c)!,
                descriptor: optDesc,
                defaultInit: { $0.assumingMemoryBound(to: UInt32?.self).initialize(to: nil) }
            ),
        ], construct: .inPlace))
    )
    let program = try lowerDecode(writerRoot, readerDesc, reg)

    var decoded = ReaderXC(x: 0, c: 12345)
    try withUnsafeMutableBytes(of: &decoded) { try decodeInto(program, writerBytes, $0.baseAddress!) }
    #expect(decoded.x == 7, "x decodes")
    #expect(decoded.c == nil, "reader-only c defaults to nil, no wire read")

    let oracle = try planDecode(writerBytes, writerRoot, readerRoot, reg)
    #expect(oracle == .object([
        .init(key: "x", value: .number(.canonical(unsigned: 7))),
        .init(key: "c", value: .null),
    ]))
}

// r[verify compat.plan-first]
@Test
func compatRequiredReaderOnlyOptionIsIncompatible() throws {
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "P", fields: [u32Field("x")])),
        Schema(id: SchemaId(2), kind: .structure(name: "P", fields: [
            u32Field("x"),
            Field(name: "c", schema: .concrete(SchemaId(3)), required: true),
        ])),
        Schema(id: SchemaId(3), kind: .option(element: .concrete(primitiveId(.u32)))),
    ])
    let writerRoot = batch[0].id
    let readerRoot = batch[1].id
    let optRoot = batch[2].id
    let reg = Registry(batch)

    let optDesc = Descriptor(
        schema: .concrete(optRoot),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let readerDesc = Descriptor(
        schema: .concrete(readerRoot),
        layout: Layout(size: MemoryLayout<ReaderXC>.size, align: MemoryLayout<ReaderXC>.alignment),
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<ReaderXC>.offset(of: \ReaderXC.x)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<ReaderXC>.offset(of: \ReaderXC.c)!, descriptor: optDesc),
        ], construct: .inPlace))
    )

    #expect(throws: CompactError.self) {
        _ = try lowerDecode(writerRoot, readerDesc, reg)
    }
    #expect(throws: CompactError.self) {
        _ = try buildPlan(writerRoot, readerRoot, reg)
    }
}

// r[verify compat.type-match]
@Test
func compatRejectsListSetKindMismatch() throws {
    let writer = Schema(id: SchemaId(1), kind: .set(element: .concrete(primitiveId(.u32))))
    let reader = Schema(id: SchemaId(2), kind: .list(element: .concrete(primitiveId(.u32))))
    let reg = Registry([writer, reader])
    let readerDesc = Descriptor(
        schema: .concrete(SchemaId(2)),
        layout: Layout(size: MemoryLayout<[UInt32]>.size, align: MemoryLayout<[UInt32]>.alignment),
        access: .sequence(SequenceAccess(
            element: u32Desc(),
            stride: MemoryLayout<UInt32>.stride,
            elemAlign: MemoryLayout<UInt32>.alignment,
            witness: .of(UInt32.self)
        ))
    )

    #expect(throws: CompactError.self) {
        _ = try lowerDecode(SchemaId(1), readerDesc, reg)
    }
    #expect(throws: CompactError.self) {
        _ = try buildPlan(SchemaId(1), SchemaId(2), reg)
    }
}

// MARK: - fuse: the same-schema fast path emerges from lowering

private struct Triple: Equatable {
    var a: UInt32
    var b: UInt32
    var c: UInt32
}

@Test
func fuseCollapsesFlatStructToOneCopy() throws {
    // { a: u32, b: u32, c: u32 } — contiguous in wire AND memory, so the three
    // scalar copies fuse into one 12-byte memcpy.
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "T", fields: [
        u32Field("a"), u32Field("b"), u32Field("c"),
    ]))
    let reg = Registry([schema])
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<Triple>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<Triple>.offset(of: \Triple.a)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<Triple>.offset(of: \Triple.b)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<Triple>.offset(of: \Triple.c)!, descriptor: u32Desc()),
        ], construct: .inPlace))
    )
    let program = try lowerTyped(desc, reg)
    #expect(program.program.count == 1, "flat all-u32 struct fuses to one copy, got \(program.program.count)")
    if case .scalar(let offset, let size, _) = program.program[0] {
        #expect(offset == 0 && size == 12, "fused to scalar(offset: 0, size: 12)")
    } else {
        Issue.record("fused op is not a scalar")
    }

    // Byte-neutral: still matches the Value oracle and round-trips.
    let value = Triple(a: 1, b: 2, c: 3)
    let bytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }
    let oracle = try encode(.object([
        .init(key: "a", value: .number(.canonical(unsigned: 1))),
        .init(key: "b", value: .number(.canonical(unsigned: 2))),
        .init(key: "c", value: .number(.canonical(unsigned: 3))),
    ]), SchemaId(1), reg)
    #expect(bytes == oracle)
    var decoded = Triple(a: 0, b: 0, c: 0)
    try withUnsafeMutableBytes(of: &decoded) { try decodeInto(try lowerDecode(desc, reg), bytes, $0.baseAddress!) }
    #expect(decoded == value)
}

// MARK: - Same-schema is the no-skip identity

@Test
func sameSchemaLowerDecodeIsIdentity() throws {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "P", fields: [u32Field("x")]))
    let reg = Registry([schema])
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<ReaderX>.size, align: MemoryLayout<ReaderX>.alignment),
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<ReaderX>.offset(of: \ReaderX.x)!, descriptor: u32Desc())],
            construct: .inPlace))
    )
    // lowerDecode(S, S) carries no skips/defaults — equivalent to the encode lowering.
    let decProgram = try lowerDecode(SchemaId(1), desc, reg)
    for op in decProgram.program {
        if case .skipWire = op { Issue.record("same-schema decode must have no skipWire") }
        if case .writeDefault = op { Issue.record("same-schema decode must have no writeDefault") }
    }

    let value = ReaderX(x: 42)
    let encProgram = try lowerTyped(desc, reg)
    let bytes = withUnsafeBytes(of: value) { encodeWith(encProgram, $0.baseAddress!) }
    var decoded = ReaderX(x: 0)
    try withUnsafeMutableBytes(of: &decoded) { try decodeInto(decProgram, bytes, $0.baseAddress!) }
    #expect(decoded == value)
}
