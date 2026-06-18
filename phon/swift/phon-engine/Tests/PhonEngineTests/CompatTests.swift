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

private func expectUnsupported(_ reason: String, _ body: () throws -> Void) {
    do {
        try body()
        Issue.record("expected unsupported(\(reason))")
    } catch CompactError.unsupported(let got) where got == reason {
    } catch {
        Issue.record("expected unsupported(\(reason)), got \(error)")
    }
}

private func expectDecodeError(_ expected: DecodeError, _ body: () throws -> Void) {
    do {
        try body()
        Issue.record("expected decode(\(expected))")
    } catch CompactError.decode(let got) where got == expected {
    } catch {
        Issue.record("expected decode(\(expected)), got \(error)")
    }
}

private struct ReaderPair: Equatable {
    var first: UInt32
    var second: UInt32
}

// r[verify compat.plan-first]
@Test
func compatWriterTupleDecodesIntoPositionalRecordDescriptor() throws {
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .tuple(elements: [
            .concrete(primitiveId(.u32)),
            .concrete(primitiveId(.u32)),
        ])),
    ])
    let root = batch[0].id
    let reg = Registry(batch)

    let writerBytes = try encode(.array([
        .number(.canonical(unsigned: 11)),
        .number(.canonical(unsigned: 22)),
    ]), root, reg)

    let readerDesc = Descriptor(
        schema: .concrete(root),
        layout: Layout(size: MemoryLayout<ReaderPair>.size, align: MemoryLayout<ReaderPair>.alignment),
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<ReaderPair>.offset(of: \ReaderPair.first)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<ReaderPair>.offset(of: \ReaderPair.second)!, descriptor: u32Desc()),
            ],
            construct: .inPlace))
    )
    let program = try lowerDecode(root, readerDesc, reg)

    var decoded = ReaderPair(first: 0, second: 0)
    try withUnsafeMutableBytes(of: &decoded) { try decodeInto(program, writerBytes, $0.baseAddress!) }
    #expect(decoded == ReaderPair(first: 11, second: 22))

    let oracle = try planDecode(writerBytes, root, root, reg)
    #expect(oracle == .array([
        .number(.canonical(unsigned: 11)),
        .number(.canonical(unsigned: 22)),
    ]))
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

// r[verify validate.uniqueness]
@Test
func planDecodeRejectsDuplicateSetElements() throws {
    let schema = Schema(
        id: SchemaId(1),
        kind: .set(element: .concrete(primitiveId(.u32)))
    )
    let reg = Registry([schema])
    var wire = ByteSink()
    wire.writeU32(2)
    wire.writeU32(7)
    wire.writeU32(7)

    expectDecodeError(.duplicateElement) {
        _ = try decode(wire.bytes, SchemaId(1), reg)
    }
    expectDecodeError(.duplicateElement) {
        _ = try planDecode(wire.bytes, SchemaId(1), SchemaId(1), reg)
    }
}

// r[verify validate.uniqueness]
@Test
func planDecodeRejectsDuplicateMapKeys() throws {
    let schema = Schema(
        id: SchemaId(1),
        kind: .map(
            key: .concrete(primitiveId(.string)),
            value: .concrete(primitiveId(.u32))
        )
    )
    let reg = Registry([schema])
    var wire = ByteSink()
    wire.writeU32(2)
    wire.writeStr("dup")
    wire.padTo(4)
    wire.writeU32(1)
    wire.writeStr("dup")
    wire.padTo(4)
    wire.writeU32(2)

    expectDecodeError(.duplicateKey) {
        _ = try decode(wire.bytes, SchemaId(1), reg)
    }
    expectDecodeError(.duplicateKey) {
        _ = try planDecode(wire.bytes, SchemaId(1), SchemaId(1), reg)
    }
}

// r[verify compat.type-match]
// r[verify type-system.channel]
// r[verify type-system.external]
@Test
func compatTreatsTransportCapabilityRootsSeparatelyFromItemAndMetadataPayloads() throws {
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "DodecaTunnelItem", fields: [
            Field(name: "seq", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "chunk_len", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "transient_id", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: SchemaId(2), kind: .structure(name: "DodecaTunnelItem", fields: [
            Field(name: "seq", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "chunk_len", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: SchemaId(3), kind: .channel(direction: .tx, element: .concrete(SchemaId(1)))),
        Schema(id: SchemaId(4), kind: .structure(name: "StaxFdMetadata", fields: [
            Field(name: "path", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "flags", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "probe_id", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: SchemaId(5), kind: .structure(name: "StaxFdMetadata", fields: [
            Field(name: "path", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "flags", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: SchemaId(6), kind: .external(kind: "fd", metadata: .concrete(SchemaId(4)))),
    ])
    let writerItem = batch[0].id
    let readerItem = batch[1].id
    let channelRoot = batch[2].id
    let writerMetadata = batch[3].id
    let readerMetadata = batch[4].id
    let externalRoot = batch[5].id
    let reg = Registry(batch)

    expectUnsupported("channel") {
        _ = try buildPlan(channelRoot, channelRoot, reg)
    }
    expectUnsupported("external") {
        _ = try buildPlan(externalRoot, externalRoot, reg)
    }

    let itemBytes = try encode(.object([
        .init(key: "seq", value: .number(.canonical(unsigned: 7))),
        .init(key: "chunk_len", value: .number(.canonical(unsigned: 128))),
        .init(key: "transient_id", value: .number(.canonical(unsigned: 99))),
    ]), writerItem, reg)
    let item = try planDecode(itemBytes, writerItem, readerItem, reg)
    #expect(item == .object([
        .init(key: "seq", value: .number(.canonical(unsigned: 7))),
        .init(key: "chunk_len", value: .number(.canonical(unsigned: 128))),
    ]))

    let metadataBytes = try encode(.object([
        .init(key: "path", value: .string("/proc/self/fd/7")),
        .init(key: "flags", value: .number(.canonical(unsigned: 0x800))),
        .init(key: "probe_id", value: .number(.canonical(unsigned: 44))),
    ]), writerMetadata, reg)
    let metadata = try planDecode(metadataBytes, writerMetadata, readerMetadata, reg)
    #expect(metadata == .object([
        .init(key: "path", value: .string("/proc/self/fd/7")),
        .init(key: "flags", value: .number(.canonical(unsigned: 0x800))),
    ]))
}

// MARK: - fuse: the same-schema fast path emerges from lowering

private struct Triple: Equatable {
    var a: UInt32
    var b: UInt32
    var c: UInt32
}

// r[verify descriptors.fact-driven]
// r[verify ir.inlining]
// r[verify ir.memory]
// r[verify ir.one-vocabulary]
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
