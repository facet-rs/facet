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

// r[verify exec.strict-recording]
private func boolDesc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.bool)), layout: Layout(size: 1, align: 1), access: .scalar)
}

// r[verify exec.strict-recording]
private func i32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.i32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

// r[verify exec.strict-recording]
private func f32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.f32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

// r[verify exec.strict-recording]
private func f64Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.f64)), layout: Layout(size: 8, align: 8), access: .scalar)
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
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

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

// r[verify compat.skip-writer-only]
// r[verify compat.field-matching]
private struct ReaderX: Equatable {
    var x: UInt32
}

// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
private struct ReaderXC: Equatable {
    var x: UInt32
    var c: UInt32?
}

// r[verify compat.type-match]
// r[verify compat.skip-writer-only]
private struct CompatListItem: Equatable {
    var x: UInt32
}

// r[verify compat.type-match]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
private struct CompatListDefaultItem: Equatable {
    var id: UInt32
    var score: UInt32
    var extra: UInt32?
}

// r[verify compat.type-match]
// r[verify compat.skip-writer-only]
private struct CompatListHolder: Equatable {
    var items: [CompatListItem]
}

// r[verify compat.type-match]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
private struct CompatListDefaultHolder: Equatable {
    var items: [CompatListDefaultItem]
}

// r[verify compat.type-match]
// r[verify compat.skip-writer-only]
private struct CompatMapHolder: Equatable {
    var values: [String: CompatListItem]
}

// r[verify compat.type-match]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
private struct CompatMapDefaultHolder: Equatable {
    var values: [String: CompatListDefaultItem]
}

// r[verify compat.skip-writer-only]
// r[verify compat.field-matching]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatWriterOnlyFieldSkipIsNativeClean() throws {
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "P", fields: [u32Field("x"), u32Field("y")])),
        Schema(id: SchemaId(2), kind: .structure(name: "P", fields: [u32Field("x")])),
    ])
    let writerRoot = batch[0].id
    let readerRoot = batch[1].id
    let reg = Registry(batch)
    let readerDesc = Descriptor(
        schema: .concrete(readerRoot),
        layout: Layout(size: MemoryLayout<ReaderX>.size, align: MemoryLayout<ReaderX>.alignment),
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<ReaderX>.offset(of: \ReaderX.x)!, descriptor: u32Desc())],
            construct: .inPlace
        ))
    )
    let lowered = try lowerDecode(writerRoot, readerDesc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support skipWire: \(report.decode)")
    #expect(report.encode.contains(JitFallbackRecord(
        path: "$.1",
        reason: "Swift native encode JIT cannot emit decode-only skip-wire ops"
    )))
    #expect(try NativeEncode.compile(lowered) == nil)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<ReaderX>.size,
        alignment: MemoryLayout<ReaderX>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run([7, 0, 0, 0, 99, 0, 0, 0], raw)
    let decoded = raw.assumingMemoryBound(to: ReaderX.self).move()
    #expect(decoded == ReaderX(x: 7))
}

// r[verify compat.skip-writer-only]
// r[verify compat.field-matching]
// r[verify compat.type-match]
// r[verify type-system.dynamic]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatNestedListAndDynamicSkipsAreNativeClean() throws {
    let writerItem = SchemaId(10)
    let readerItem = SchemaId(11)
    let writerList = SchemaId(12)
    let readerList = SchemaId(13)
    let writerRoot = SchemaId(14)
    let readerRoot = SchemaId(15)
    let dynamic = SchemaId(16)
    let batch = resolveIds([
        Schema(id: writerItem, kind: .structure(name: "CompatItem", fields: [
            u32Field("x"),
            u32Field("y"),
        ])),
        Schema(id: readerItem, kind: .structure(name: "CompatItem", fields: [
            u32Field("x"),
        ])),
        Schema(id: writerList, kind: .list(element: .concrete(writerItem))),
        Schema(id: readerList, kind: .list(element: .concrete(readerItem))),
        Schema(id: dynamic, kind: .dynamic),
        Schema(id: writerRoot, kind: .structure(name: "CompatListHolder", fields: [
            Field(name: "items", schema: .concrete(writerList), required: true),
            Field(name: "metadata", schema: .concrete(dynamic), required: false),
        ])),
        Schema(id: readerRoot, kind: .structure(name: "CompatListHolder", fields: [
            Field(name: "items", schema: .concrete(readerList), required: true),
        ])),
    ])
    let resolvedReaderItem = batch[1].id
    let resolvedReaderList = batch[3].id
    let resolvedWriterRoot = batch[5].id
    let resolvedReaderRoot = batch[6].id
    let reg = Registry(batch)

    let itemDesc = Descriptor(
        schema: .concrete(resolvedReaderItem),
        layout: MemoryLayout<CompatListItem>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<CompatListItem>.offset(of: \CompatListItem.x)!, descriptor: u32Desc()),
            ],
            construct: .inPlace
        ))
    )
    let listDesc = Descriptor(
        schema: .concrete(resolvedReaderList),
        layout: MemoryLayout<[CompatListItem]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: itemDesc,
            stride: MemoryLayout<CompatListItem>.stride,
            elemAlign: MemoryLayout<CompatListItem>.alignment,
            witness: arraySeqWitness(of: CompatListItem.self)
        ))
    )
    let readerDesc = Descriptor(
        schema: .concrete(resolvedReaderRoot),
        layout: MemoryLayout<CompatListHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<CompatListHolder>.offset(of: \CompatListHolder.items)!, descriptor: listDesc),
            ],
            construct: .inPlace
        ))
    )
    let lowered = try lowerDecode(resolvedWriterRoot, readerDesc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support nested skipWire: \(report.decode)")
    #expect(report.encode.filter { $0.reason.contains("decode-only skip-wire") }.count == 2)
    #expect(try NativeEncode.compile(lowered) == nil)

    let wire = try encode(.object([
        .init(key: "items", value: .array([
            .object([
                .init(key: "x", value: .number(.canonical(unsigned: 7))),
                .init(key: "y", value: .number(.canonical(unsigned: 99))),
            ]),
            .object([
                .init(key: "x", value: .number(.canonical(unsigned: 11))),
                .init(key: "y", value: .number(.canonical(unsigned: 123))),
            ]),
        ])),
        .init(key: "metadata", value: .object([
            .init(key: "trace", value: .string("writer-only")),
            .init(key: "attempt", value: .number(.canonical(unsigned: 1))),
        ])),
    ]), resolvedWriterRoot, reg)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<CompatListHolder>.size,
        alignment: MemoryLayout<CompatListHolder>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run(wire, raw)
    let decoded = raw.assumingMemoryBound(to: CompatListHolder.self).move()
    #expect(decoded == CompatListHolder(items: [
        CompatListItem(x: 7),
        CompatListItem(x: 11),
    ]))
}

// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify compat.type-match]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatListElementStructDriftMatchesReaderOracle() throws {
    let writerItem = SchemaId(30)
    let optionU32 = SchemaId(31)
    let readerItem = SchemaId(32)
    let writerList = SchemaId(33)
    let readerList = SchemaId(34)
    let writerRoot = SchemaId(35)
    let readerRoot = SchemaId(36)
    let batch = resolveIds([
        Schema(id: writerItem, kind: .structure(name: "CompatListItem", fields: [
            u32Field("id"),
            Field(name: "transient", schema: .concrete(primitiveId(.string)), required: true),
            u32Field("score"),
        ])),
        Schema(id: optionU32, kind: .option(element: .concrete(primitiveId(.u32)))),
        Schema(id: readerItem, kind: .structure(name: "CompatListItem", fields: [
            u32Field("id"),
            u32Field("score"),
            Field(name: "extra", schema: .concrete(optionU32), required: false),
        ])),
        Schema(id: writerList, kind: .list(element: .concrete(writerItem))),
        Schema(id: readerList, kind: .list(element: .concrete(readerItem))),
        Schema(id: writerRoot, kind: .structure(name: "CompatListDefaultHolder", fields: [
            Field(name: "items", schema: .concrete(writerList), required: true),
        ])),
        Schema(id: readerRoot, kind: .structure(name: "CompatListDefaultHolder", fields: [
            Field(name: "items", schema: .concrete(readerList), required: true),
        ])),
    ])
    let resolvedOptionU32 = batch[1].id
    let resolvedReaderItem = batch[2].id
    let resolvedWriterRoot = batch[5].id
    let resolvedReaderRoot = batch[6].id
    let reg = Registry(batch)

    let optionDesc = Descriptor(
        schema: .concrete(resolvedOptionU32),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let itemDesc = Descriptor(
        schema: .concrete(resolvedReaderItem),
        layout: MemoryLayout<CompatListDefaultItem>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(
                offset: MemoryLayout<CompatListDefaultItem>.offset(of: \CompatListDefaultItem.id)!,
                descriptor: u32Desc()
            ),
            FieldAccess(
                offset: MemoryLayout<CompatListDefaultItem>.offset(of: \CompatListDefaultItem.score)!,
                descriptor: u32Desc()
            ),
            FieldAccess(
                offset: MemoryLayout<CompatListDefaultItem>.offset(of: \CompatListDefaultItem.extra)!,
                descriptor: optionDesc,
                defaultInit: { $0.assumingMemoryBound(to: UInt32?.self).initialize(to: nil) }
            ),
        ], construct: .inPlace))
    )
    let listDesc = Descriptor(
        schema: .concrete(batch[4].id),
        layout: MemoryLayout<[CompatListDefaultItem]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: itemDesc,
            stride: MemoryLayout<CompatListDefaultItem>.stride,
            elemAlign: MemoryLayout<CompatListDefaultItem>.alignment,
            witness: arraySeqWitness(of: CompatListDefaultItem.self)
        ))
    )
    let readerDesc = Descriptor(
        schema: .concrete(resolvedReaderRoot),
        layout: MemoryLayout<CompatListDefaultHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(
                    offset: MemoryLayout<CompatListDefaultHolder>.offset(of: \CompatListDefaultHolder.items)!,
                    descriptor: listDesc
                ),
            ],
            construct: .inPlace
        ))
    )
    let lowered = try lowerDecode(resolvedWriterRoot, readerDesc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support list element struct drift: \(report.decode)")
    #expect(report.encode.filter { $0.reason.contains("decode-only skip-wire") }.count == 1)
    #expect(report.encode.filter { $0.reason.contains("decode-only default") }.count == 1)
    #expect(try NativeEncode.compile(lowered) == nil)

    let writerValue = Value.object([
        .init(key: "items", value: .array([
            .object([
                .init(key: "id", value: .number(.canonical(unsigned: 1))),
                .init(key: "transient", value: .string("drop-a")),
                .init(key: "score", value: .number(.canonical(unsigned: 10))),
            ]),
            .object([
                .init(key: "id", value: .number(.canonical(unsigned: 2))),
                .init(key: "transient", value: .string("drop-b")),
                .init(key: "score", value: .number(.canonical(unsigned: 20))),
            ]),
        ])),
    ])
    let wire = try encode(writerValue, resolvedWriterRoot, reg)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<CompatListDefaultHolder>.size,
        alignment: MemoryLayout<CompatListDefaultHolder>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run(wire, raw)
    let decoded = raw.assumingMemoryBound(to: CompatListDefaultHolder.self).move()
    let expected = CompatListDefaultHolder(items: [
        CompatListDefaultItem(id: 1, score: 10, extra: nil),
        CompatListDefaultItem(id: 2, score: 20, extra: nil),
    ])
    #expect(decoded == expected)

    let oracle = try planDecode(wire, resolvedWriterRoot, resolvedReaderRoot, reg)
    let oracleBytes = try encode(oracle, resolvedReaderRoot, reg)
    let readerLowered = try lowerTyped(readerDesc, reg)
    #expect(PhonJIT.nativeFallbackReport(readerLowered).isEmpty)
    let typedBytes = withUnsafeBytes(of: decoded) { encodeWith(readerLowered, $0.baseAddress!) }
    #expect(typedBytes == oracleBytes)

    let encoder = try NativeEncode.compile(readerLowered)
    #expect(encoder != nil)
    if let encoder {
        let nativeBytes = withUnsafeBytes(of: decoded) { encoder.run($0.baseAddress!) }
        #expect(nativeBytes == oracleBytes)
    }
}

// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify compat.type-match]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatMapValueStructDriftMatchesReaderOracle() throws {
    let writerItem = SchemaId(40)
    let optionU32 = SchemaId(41)
    let readerItem = SchemaId(42)
    let writerMap = SchemaId(43)
    let readerMap = SchemaId(44)
    let writerRoot = SchemaId(45)
    let readerRoot = SchemaId(46)
    let batch = resolveIds([
        Schema(id: writerItem, kind: .structure(name: "CompatMapItem", fields: [
            u32Field("id"),
            Field(name: "transient", schema: .concrete(primitiveId(.string)), required: true),
            u32Field("score"),
        ])),
        Schema(id: optionU32, kind: .option(element: .concrete(primitiveId(.u32)))),
        Schema(id: readerItem, kind: .structure(name: "CompatMapItem", fields: [
            u32Field("id"),
            u32Field("score"),
            Field(name: "extra", schema: .concrete(optionU32), required: false),
        ])),
        Schema(id: writerMap, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(writerItem))),
        Schema(id: readerMap, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(readerItem))),
        Schema(id: writerRoot, kind: .structure(name: "CompatMapDefaultHolder", fields: [
            Field(name: "values", schema: .concrete(writerMap), required: true),
        ])),
        Schema(id: readerRoot, kind: .structure(name: "CompatMapDefaultHolder", fields: [
            Field(name: "values", schema: .concrete(readerMap), required: true),
        ])),
    ])
    let resolvedOptionU32 = batch[1].id
    let resolvedReaderItem = batch[2].id
    let resolvedReaderMap = batch[4].id
    let resolvedWriterRoot = batch[5].id
    let resolvedReaderRoot = batch[6].id
    let reg = Registry(batch)

    let optionDesc = Descriptor(
        schema: .concrete(resolvedOptionU32),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let itemDesc = Descriptor(
        schema: .concrete(resolvedReaderItem),
        layout: MemoryLayout<CompatListDefaultItem>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(
                offset: MemoryLayout<CompatListDefaultItem>.offset(of: \CompatListDefaultItem.id)!,
                descriptor: u32Desc()
            ),
            FieldAccess(
                offset: MemoryLayout<CompatListDefaultItem>.offset(of: \CompatListDefaultItem.score)!,
                descriptor: u32Desc()
            ),
            FieldAccess(
                offset: MemoryLayout<CompatListDefaultItem>.offset(of: \CompatListDefaultItem.extra)!,
                descriptor: optionDesc,
                defaultInit: { $0.assumingMemoryBound(to: UInt32?.self).initialize(to: nil) }
            ),
        ], construct: .inPlace))
    )
    let mapDesc = Descriptor(
        schema: .concrete(resolvedReaderMap),
        layout: MemoryLayout<[String: CompatListDefaultItem]>.phonLayout,
        access: .map(MapAccess(
            key: stringDesc(),
            value: itemDesc,
            keyStride: MemoryLayout<String>.stride,
            keyAlign: MemoryLayout<String>.alignment,
            valueStride: MemoryLayout<CompatListDefaultItem>.stride,
            valueAlign: MemoryLayout<CompatListDefaultItem>.alignment,
            witness: .stringKeyed(CompatListDefaultItem.self)
        ))
    )
    let readerDesc = Descriptor(
        schema: .concrete(resolvedReaderRoot),
        layout: MemoryLayout<CompatMapDefaultHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(
                    offset: MemoryLayout<CompatMapDefaultHolder>.offset(of: \CompatMapDefaultHolder.values)!,
                    descriptor: mapDesc
                ),
            ],
            construct: .inPlace
        ))
    )
    let lowered = try lowerDecode(resolvedWriterRoot, readerDesc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support map value struct drift: \(report.decode)")
    #expect(report.encode.filter { $0.reason.contains("decode-only skip-wire") }.count == 1)
    #expect(report.encode.filter { $0.reason.contains("decode-only default") }.count == 1)
    #expect(try NativeEncode.compile(lowered) == nil)

    let writerValue = Value.object([
        .init(key: "values", value: .object([
            .init(key: "alpha", value: .object([
                .init(key: "id", value: .number(.canonical(unsigned: 1))),
                .init(key: "transient", value: .string("drop-a")),
                .init(key: "score", value: .number(.canonical(unsigned: 10))),
            ])),
            .init(key: "beta", value: .object([
                .init(key: "id", value: .number(.canonical(unsigned: 2))),
                .init(key: "transient", value: .string("drop-b")),
                .init(key: "score", value: .number(.canonical(unsigned: 20))),
            ])),
        ])),
    ])
    let wire = try encode(writerValue, resolvedWriterRoot, reg)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<CompatMapDefaultHolder>.size,
        alignment: MemoryLayout<CompatMapDefaultHolder>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run(wire, raw)
    let decoded = raw.assumingMemoryBound(to: CompatMapDefaultHolder.self).move()
    let expected = CompatMapDefaultHolder(values: [
        "alpha": CompatListDefaultItem(id: 1, score: 10, extra: nil),
        "beta": CompatListDefaultItem(id: 2, score: 20, extra: nil),
    ])
    #expect(decoded == expected)

    let oracle = try planDecode(wire, resolvedWriterRoot, resolvedReaderRoot, reg)
    let oracleBytes = try encode(oracle, resolvedReaderRoot, reg)
    let readerLowered = try lowerTyped(readerDesc, reg)
    #expect(PhonJIT.nativeFallbackReport(readerLowered).isEmpty)
    let typedBytes = withUnsafeBytes(of: decoded) { encodeWith(readerLowered, $0.baseAddress!) }
    #expect(typedBytes == oracleBytes)

    let encoder = try NativeEncode.compile(readerLowered)
    #expect(encoder != nil)
    if let encoder {
        let nativeBytes = withUnsafeBytes(of: decoded) { encoder.run($0.baseAddress!) }
        #expect(nativeBytes == oracleBytes)
    }
}

// r[verify compat.skip-writer-only]
// r[verify compat.field-matching]
// r[verify compat.type-match]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatMapValueDriftIsNativeClean() throws {
    let writerItem = SchemaId(20)
    let readerItem = SchemaId(21)
    let writerMap = SchemaId(22)
    let readerMap = SchemaId(23)
    let writerRoot = SchemaId(24)
    let readerRoot = SchemaId(25)
    let batch = resolveIds([
        Schema(id: writerItem, kind: .structure(name: "CompatItem", fields: [
            u32Field("x"),
            u32Field("y"),
        ])),
        Schema(id: readerItem, kind: .structure(name: "CompatItem", fields: [
            u32Field("x"),
        ])),
        Schema(id: writerMap, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(writerItem))),
        Schema(id: readerMap, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(readerItem))),
        Schema(id: writerRoot, kind: .structure(name: "CompatMapHolder", fields: [
            Field(name: "values", schema: .concrete(writerMap), required: true),
        ])),
        Schema(id: readerRoot, kind: .structure(name: "CompatMapHolder", fields: [
            Field(name: "values", schema: .concrete(readerMap), required: true),
        ])),
    ])
    let resolvedReaderItem = batch[1].id
    let resolvedReaderMap = batch[3].id
    let resolvedWriterRoot = batch[4].id
    let resolvedReaderRoot = batch[5].id
    let reg = Registry(batch)

    let itemDesc = Descriptor(
        schema: .concrete(resolvedReaderItem),
        layout: MemoryLayout<CompatListItem>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<CompatListItem>.offset(of: \CompatListItem.x)!, descriptor: u32Desc()),
            ],
            construct: .inPlace
        ))
    )
    let mapDesc = Descriptor(
        schema: .concrete(resolvedReaderMap),
        layout: MemoryLayout<[String: CompatListItem]>.phonLayout,
        access: .map(MapAccess(
            key: stringDesc(),
            value: itemDesc,
            keyStride: MemoryLayout<String>.stride,
            keyAlign: MemoryLayout<String>.alignment,
            valueStride: MemoryLayout<CompatListItem>.stride,
            valueAlign: MemoryLayout<CompatListItem>.alignment,
            witness: .stringKeyed(CompatListItem.self)
        ))
    )
    let readerDesc = Descriptor(
        schema: .concrete(resolvedReaderRoot),
        layout: MemoryLayout<CompatMapHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<CompatMapHolder>.offset(of: \CompatMapHolder.values)!, descriptor: mapDesc),
            ],
            construct: .inPlace
        ))
    )
    let lowered = try lowerDecode(resolvedWriterRoot, readerDesc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support map value skipWire: \(report.decode)")
    #expect(report.encode.filter { $0.reason.contains("decode-only skip-wire") }.count == 1)
    #expect(try NativeEncode.compile(lowered) == nil)

    let wire = try encode(.object([
        .init(key: "values", value: .object([
            .init(key: "alpha", value: .object([
                .init(key: "x", value: .number(.canonical(unsigned: 1))),
                .init(key: "y", value: .number(.canonical(unsigned: 10))),
            ])),
            .init(key: "beta", value: .object([
                .init(key: "x", value: .number(.canonical(unsigned: 2))),
                .init(key: "y", value: .number(.canonical(unsigned: 20))),
            ])),
        ])),
    ]), resolvedWriterRoot, reg)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<CompatMapHolder>.size,
        alignment: MemoryLayout<CompatMapHolder>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run(wire, raw)
    let decoded = raw.assumingMemoryBound(to: CompatMapHolder.self).move()
    #expect(decoded == CompatMapHolder(values: [
        "alpha": CompatListItem(x: 1),
        "beta": CompatListItem(x: 2),
    ]))
}

// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatReaderOnlyDefaultIsNativeClean() throws {
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
    let lowered = try lowerDecode(writerRoot, readerDesc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support writeDefault: \(report.decode)")
    #expect(report.encode.contains(JitFallbackRecord(
        path: "$.1",
        reason: "Swift native encode JIT cannot emit decode-only default ops"
    )))
    #expect(try NativeEncode.compile(lowered) == nil)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<ReaderXC>.size,
        alignment: MemoryLayout<ReaderXC>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run([7, 0, 0, 0], raw)
    let decoded = raw.assumingMemoryBound(to: ReaderXC.self).move()
    #expect(decoded.x == 7)
    #expect(decoded.c == nil)
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
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)
    #expect(try NativeEncode.compile(lowered) != nil)
    #expect(try NativeDecode.compile(lowered) != nil)
}

// r[verify exec.strict-recording]
private struct TextHolder: Equatable {
    var text: String
}

// r[verify type-system.dynamic]
// r[verify ir.stencils]
private struct DynamicHolder: Equatable {
    var value: Value
}

// r[verify exec.strict-recording]
private func stringWitness() -> BytesWitness {
    BytesWitness(
        count: { field in field.assumingMemoryBound(to: String.self).pointee.utf8.count },
        copyInto: { field, dst in
            var s = field.assumingMemoryBound(to: String.self).pointee
            s.withUTF8 { buf in
                if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
            }
        },
        construct: { field, src, count in
            let buf = UnsafeBufferPointer(start: src.assumingMemoryBound(to: UInt8.self), count: count)
            guard let s = String(validating: buf, as: UTF8.self) else { return false }
            field.assumingMemoryBound(to: String.self).initialize(to: s)
            return true
        }
    )
}

// r[verify exec.strict-recording]
private func stringDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: stringWitness()))
    )
}

// r[verify exec.strict-recording]
private func textHolderDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(
        id: SchemaId(1),
        kind: .structure(name: "TextHolder", fields: [
            Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
        ])
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<TextHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<TextHolder>.offset(of: \TextHolder.text)!, descriptor: stringDesc()),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([schema]))
}

// r[verify type-system.dynamic]
// r[verify ir.stencils]
private func dynamicHolderDescriptor() -> (Descriptor, Registry) {
    let dynamic = Schema(id: SchemaId(2), kind: .dynamic)
    let holder = Schema(
        id: SchemaId(1),
        kind: .structure(name: "DynamicHolder", fields: [
            Field(name: "value", schema: .concrete(SchemaId(2)), required: true),
        ])
    )
    let valueDesc = Descriptor(
        schema: .concrete(SchemaId(2)),
        layout: MemoryLayout<Value>.phonLayout,
        access: .dynamic
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<DynamicHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<DynamicHolder>.offset(of: \DynamicHolder.value)!, descriptor: valueDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([dynamic, holder]))
}

// r[verify type-system.dynamic]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeDynamicEncodeDecodeAndReportsClean() throws {
    let (desc, reg) = dynamicHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let value = DynamicHolder(value: .object([
        .init(key: "k", value: .number(.canonical(unsigned: 42))),
        .init(key: "flag", value: .bool(true)),
        .init(key: "items", value: .array([.string("docs"), .null])),
    ]))
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<DynamicHolder>.size,
        alignment: MemoryLayout<DynamicHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: DynamicHolder.self).move()
    #expect(decoded == value)
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
private struct TextAndDoubleHolder: Equatable {
    var text: String
    var value: Double
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
private func textAndDoubleHolderDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(
        id: SchemaId(11),
        kind: .structure(name: "TextAndDoubleHolder", fields: [
            Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "value", schema: .concrete(primitiveId(.f64)), required: true),
        ])
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(11)),
        layout: MemoryLayout<TextAndDoubleHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<TextAndDoubleHolder>.offset(of: \TextAndDoubleHolder.text)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<TextAndDoubleHolder>.offset(of: \TextAndDoubleHolder.value)!, descriptor: f64Desc()),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([schema]))
}

// r[verify exec.strict-recording]
@Test func nativeBytesEncodeDecodeStringAndReportsClean() throws {
    let (desc, reg) = textHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.isEmpty)
    #expect(report.scoped(method: "setMarkedText", phase: "args").isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }

    let value = TextHolder(text: "héllo λ")
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<TextHolder>.size,
        alignment: MemoryLayout<TextHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: TextHolder.self).move()
    #expect(decoded == value)
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeBytesThenScalarEncodeDecodeAndReportsClean() throws {
    let (desc, reg) = textAndDoubleHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(lowered.program.count == 2)
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }

    let value = TextAndDoubleHolder(text: "meters", value: 1.5)
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<TextAndDoubleHolder>.size,
        alignment: MemoryLayout<TextAndDoubleHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: TextAndDoubleHolder.self).move()
    #expect(decoded == value)
}

// r[verify exec.strict-recording]
private struct SamplesHolder: Equatable {
    var samples: [Float]
}

// r[verify exec.strict-recording]
private func floatArrayWitness() -> BytesWitness {
    BytesWitness(
        count: { field in field.assumingMemoryBound(to: [Float].self).pointee.count },
        copyInto: { field, dst in
            field.assumingMemoryBound(to: [Float].self).pointee.withUnsafeBytes { buf in
                if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
            }
        },
        construct: { field, src, count in
            field.assumingMemoryBound(to: [Float].self).initialize(to: Array(unsafeUninitializedCapacity: count) { dst, n in
                if count > 0 {
                    dst.baseAddress!.initialize(from: src.assumingMemoryBound(to: Float.self), count: count)
                }
                n = count
            })
            return true
        }
    )
}

// r[verify exec.strict-recording]
private func samplesHolderDescriptor() -> (Descriptor, Registry) {
    let list = Schema(id: SchemaId(2), kind: .list(element: .concrete(primitiveId(.f32))))
    let schema = Schema(
        id: SchemaId(1),
        kind: .structure(name: "SamplesHolder", fields: [
            Field(name: "samples", schema: .concrete(SchemaId(2)), required: true),
        ])
    )
    let samplesDesc = Descriptor(
        schema: .concrete(SchemaId(2)),
        layout: Layout(size: MemoryLayout<[Float]>.size, align: MemoryLayout<[Float]>.alignment),
        access: .bytes(BytesAccess(
            stride: MemoryLayout<Float>.stride,
            elemAlign: MemoryLayout<Float>.alignment,
            witness: floatArrayWitness()
        ))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<SamplesHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<SamplesHolder>.offset(of: \SamplesHolder.samples)!, descriptor: samplesDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([list, schema]))
}

// r[verify exec.strict-recording]
@Test func nativeBytesEncodeDecodeFloatArrayAndReportsClean() throws {
    let (desc, reg) = samplesHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).scoped(method: "feed", phase: "args").isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }

    let value = SamplesHolder(samples: [0.0, 0.25, -1.5, 42.0])
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<SamplesHolder>.size,
        alignment: MemoryLayout<SamplesHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: SamplesHolder.self).move()
    #expect(decoded == value)
}

// r[verify ir.stencils]
private struct PairListHolder: Equatable {
    var pairs: [Pair]
}

// r[verify ir.stencils]
private func pairElementDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<Pair>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.a)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.b)!, descriptor: u32Desc()),
            ],
            construct: .inPlace
        ))
    )
}

// r[verify ir.stencils]
private func pairSeqWitness() -> SeqWitness {
    SeqWitness(
        count: { handle in handle.assumingMemoryBound(to: [Pair].self).pointee.count },
        copyElements: { handle, dst in
            handle.assumingMemoryBound(to: [Pair].self).pointee.withUnsafeBytes { buf in
                if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
            }
        },
        construct: { handle, src, count in
            handle.assumingMemoryBound(to: [Pair].self).initialize(to: Array(unsafeUninitializedCapacity: count) { dst, n in
                if count > 0 {
                    dst.baseAddress!.moveInitialize(from: src.assumingMemoryBound(to: Pair.self), count: count)
                }
                n = count
            })
        }
    )
}

// r[verify ir.stencils]
private func pairListHolderDescriptor() -> (Descriptor, Registry) {
    let pairId = SchemaId(2)
    let listId = SchemaId(3)
    let pairSchema = Schema(id: pairId, kind: .structure(name: "Pair", fields: [
        Field(name: "a", schema: .concrete(primitiveId(.u32)), required: true),
        Field(name: "b", schema: .concrete(primitiveId(.u32)), required: true),
    ]))
    let listSchema = Schema(id: listId, kind: .list(element: .concrete(pairId)))
    let root = Schema(id: SchemaId(1), kind: .structure(name: "PairListHolder", fields: [
        Field(name: "pairs", schema: .concrete(listId), required: true),
    ]))
    let listDesc = Descriptor(
        schema: .concrete(listId),
        layout: Layout(size: MemoryLayout<[Pair]>.size, align: MemoryLayout<[Pair]>.alignment),
        access: .sequence(SequenceAccess(
            element: pairElementDesc(pairId),
            stride: MemoryLayout<Pair>.stride,
            elemAlign: MemoryLayout<Pair>.alignment,
            witness: pairSeqWitness()
        ))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<PairListHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<PairListHolder>.offset(of: \PairListHolder.pairs)!, descriptor: listDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([pairSchema, listSchema, root]))
}

// r[verify ir.stencils]
private struct StringListHolder: Equatable {
    var items: [String]
}

// r[verify ir.stencils]
private func stringSeqWitness() -> SeqWitness {
    SeqWitness(
        count: { handle in handle.assumingMemoryBound(to: [String].self).pointee.count },
        copyElements: { handle, dst in
            handle.assumingMemoryBound(to: [String].self).pointee.withUnsafeBytes { buf in
                if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
            }
        },
        construct: { handle, src, count in
            handle.assumingMemoryBound(to: [String].self).initialize(to: Array(unsafeUninitializedCapacity: count) { dst, n in
                if count > 0 {
                    dst.baseAddress!.moveInitialize(from: src.assumingMemoryBound(to: String.self), count: count)
                }
                n = count
            })
        }
    )
}

// r[verify ir.stencils]
private func stringListHolderDescriptor() -> (Descriptor, Registry) {
    let listId = SchemaId(2)
    let listSchema = Schema(id: listId, kind: .list(element: .concrete(primitiveId(.string))))
    let root = Schema(id: SchemaId(1), kind: .structure(name: "StringListHolder", fields: [
        Field(name: "items", schema: .concrete(listId), required: true),
    ]))
    let listDesc = Descriptor(
        schema: .concrete(listId),
        layout: Layout(size: MemoryLayout<[String]>.size, align: MemoryLayout<[String]>.alignment),
        access: .sequence(SequenceAccess(
            element: stringDesc(),
            stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment,
            witness: stringSeqWitness()
        ))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<StringListHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<StringListHolder>.offset(of: \StringListHolder.items)!, descriptor: listDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([listSchema, root]))
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeSequenceOfRecordsEncodeDecodeAndReportsClean() throws {
    let (desc, reg) = pairListHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let value = PairListHolder(pairs: [Pair(a: 1, b: 2), Pair(a: 3, b: 5)])
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<PairListHolder>.size,
        alignment: MemoryLayout<PairListHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: PairListHolder.self).move()
    #expect(decoded == value)
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeSequenceOfStringsEncodeDecodeAndReportsClean() throws {
    let (desc, reg) = stringListHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let value = StringListHolder(items: ["alpha", "βeta", ""])
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<StringListHolder>.size,
        alignment: MemoryLayout<StringListHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: StringListHolder.self).move()
    #expect(decoded == value)
}

// r[verify ir.stencils]
private struct U32SetHolder: Equatable {
    var values: Set<UInt32>
}

// r[verify ir.stencils]
private func u32SetSeqWitness() -> SeqWitness {
    let stride = MemoryLayout<UInt32>.stride
    return SeqWitness(
        count: { handle in handle.assumingMemoryBound(to: Set<UInt32>.self).pointee.count },
        copyElements: { handle, dst in
            let set = handle.assumingMemoryBound(to: Set<UInt32>.self).pointee
            for (index, value) in set.enumerated() {
                dst.advanced(by: index * stride).assumingMemoryBound(to: UInt32.self).initialize(to: value)
            }
        },
        construct: { handle, src, count in
            var set = Set<UInt32>(minimumCapacity: count)
            for index in 0..<count {
                let value = src.advanced(by: index * stride).assumingMemoryBound(to: UInt32.self).pointee
                set.insert(value)
            }
            handle.assumingMemoryBound(to: Set<UInt32>.self).initialize(to: set)
        }
    )
}

// r[verify ir.stencils]
private func u32SetHolderDescriptor() -> (Descriptor, Registry) {
    let setId = SchemaId(2)
    let setSchema = Schema(id: setId, kind: .set(element: .concrete(primitiveId(.u32))))
    let root = Schema(id: SchemaId(1), kind: .structure(name: "U32SetHolder", fields: [
        Field(name: "values", schema: .concrete(setId), required: true),
    ]))
    let setDesc = Descriptor(
        schema: .concrete(setId),
        layout: Layout(size: MemoryLayout<Set<UInt32>>.size, align: MemoryLayout<Set<UInt32>>.alignment),
        access: .sequence(SequenceAccess(
            element: u32Desc(),
            stride: MemoryLayout<UInt32>.stride,
            elemAlign: MemoryLayout<UInt32>.alignment,
            witness: u32SetSeqWitness()
        ))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<U32SetHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<U32SetHolder>.offset(of: \U32SetHolder.values)!, descriptor: setDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([setSchema, root]))
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeSetU32EncodeDecodeAndReportsClean() throws {
    let (desc, reg) = u32SetHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let value = U32SetHolder(values: [9, 1, 4])
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<U32SetHolder>.size,
        alignment: MemoryLayout<U32SetHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: U32SetHolder.self).move()
    #expect(decoded == value)
}

// r[verify ir.stencils]
// r[verify validate.uniqueness]
@Test func nativeSetDecodeRejectsDuplicateElement() throws {
    let (desc, reg) = u32SetHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    let duplicateWire: [UInt8] = [
        2, 0, 0, 0,
        7, 0, 0, 0,
        7, 0, 0, 0,
    ]

    let interpRaw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<U32SetHolder>.size,
        alignment: MemoryLayout<U32SetHolder>.alignment
    )
    var interpInitialized = false
    defer {
        if interpInitialized {
            interpRaw.assumingMemoryBound(to: U32SetHolder.self).deinitialize(count: 1)
        }
        interpRaw.deallocate()
    }
    do {
        try decodeInto(lowered, duplicateWire, interpRaw)
        interpInitialized = true
        Issue.record("typed set decode accepted a duplicate element")
    } catch let error as CompactError {
        if case .decode(.duplicateElement) = error {
            interpInitialized = true
        } else {
            Issue.record("expected duplicate-element decode error, got \(error)")
        }
    } catch {
        Issue.record("expected duplicate-element decode error, got \(error)")
    }

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let nativeRaw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<U32SetHolder>.size,
        alignment: MemoryLayout<U32SetHolder>.alignment
    )
    var nativeInitialized = false
    defer {
        if nativeInitialized {
            nativeRaw.assumingMemoryBound(to: U32SetHolder.self).deinitialize(count: 1)
        }
        nativeRaw.deallocate()
    }
    do {
        try decoder.run(duplicateWire, nativeRaw)
        nativeInitialized = true
        Issue.record("native set decode accepted a duplicate element")
    } catch let error as CompactError {
        if case .decode(.duplicateElement) = error {
            nativeInitialized = true
        } else {
            Issue.record("expected duplicate-element decode error, got \(error)")
        }
    } catch {
        Issue.record("expected duplicate-element decode error, got \(error)")
    }
}

// r[verify ir.stencils]
private struct StringU32MapHolder: Equatable {
    var values: [String: UInt32]
}

// r[verify ir.stencils]
private func stringU32MapHolderDescriptor() -> (Descriptor, Registry) {
    let mapId = SchemaId(2)
    let mapSchema = Schema(
        id: mapId,
        kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(primitiveId(.u32)))
    )
    let root = Schema(id: SchemaId(1), kind: .structure(name: "StringU32MapHolder", fields: [
        Field(name: "values", schema: .concrete(mapId), required: true),
    ]))
    let mapDesc = Descriptor(
        schema: .concrete(mapId),
        layout: Layout(size: MemoryLayout<[String: UInt32]>.size, align: MemoryLayout<[String: UInt32]>.alignment),
        access: .map(MapAccess(
            key: stringDesc(),
            value: u32Desc(),
            keyStride: MemoryLayout<String>.stride,
            keyAlign: MemoryLayout<String>.alignment,
            valueStride: MemoryLayout<UInt32>.stride,
            valueAlign: MemoryLayout<UInt32>.alignment,
            witness: .stringKeyed(UInt32.self)
        ))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<StringU32MapHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<StringU32MapHolder>.offset(of: \StringU32MapHolder.values)!, descriptor: mapDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([mapSchema, root]))
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeMapStringU32EncodeDecodeAndReportsClean() throws {
    let (desc, reg) = stringU32MapHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let value = StringU32MapHolder(values: ["bravo": 2, "alpha": 1, "λ": 3])
    let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
    let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
    #expect(jitBytes == interpBytes)

    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<StringU32MapHolder>.size,
        alignment: MemoryLayout<StringU32MapHolder>.alignment
    )
    defer { raw.deallocate() }
    try decoder.run(jitBytes, raw)
    let decoded = raw.assumingMemoryBound(to: StringU32MapHolder.self).move()
    #expect(decoded == value)
}

// r[verify ir.stencils]
// r[verify validate.uniqueness]
@Test func nativeMapDecodeRejectsDuplicateKey() throws {
    let (desc, reg) = stringU32MapHolderDescriptor()
    let lowered = try lowerTyped(desc, reg)
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    let duplicateWire: [UInt8] = [
        2, 0, 0, 0,
        3, 0, 0, 0, 100, 117, 112, 0, 1, 0, 0, 0,
        3, 0, 0, 0, 100, 117, 112, 0, 2, 0, 0, 0,
    ]
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<StringU32MapHolder>.size,
        alignment: MemoryLayout<StringU32MapHolder>.alignment
    )
    var initialized = false
    defer {
        if initialized {
            raw.assumingMemoryBound(to: StringU32MapHolder.self).deinitialize(count: 1)
        }
        raw.deallocate()
    }

    do {
        try decoder.run(duplicateWire, raw)
        initialized = true
        Issue.record("native map decode accepted a duplicate key")
    } catch let error as CompactError {
        if case .decode(.duplicateKey) = error {
            initialized = true
        } else {
            Issue.record("expected duplicate-key decode error, got \(error)")
        }
    } catch {
        Issue.record("expected duplicate-key decode error, got \(error)")
    }
}

// r[verify ir.stencils]
private enum Phase: Equatable {
    case dictating
    case finalizing
}

// r[verify ir.stencils]
private func phaseSchema(_ id: SchemaId) -> Schema {
    Schema(id: id, kind: .enumeration(name: "ImePhase", variants: [
        Variant(name: "Dictating", index: 0, payload: .unit),
        Variant(name: "Finalizing", index: 1, payload: .unit),
    ]))
}

// r[verify compat.enum]
private func evolvedPhaseWriterSchema(_ id: SchemaId) -> Schema {
    Schema(id: id, kind: .enumeration(name: "ImePhase", variants: [
        Variant(name: "Dictating", index: 0, payload: .unit),
        Variant(name: "Paused", index: 1, payload: .unit),
        Variant(name: "Finalizing", index: 2, payload: .unit),
    ]))
}

// r[verify ir.stencils]
private func phaseDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: Layout(size: MemoryLayout<Phase>.size, align: MemoryLayout<Phase>.alignment),
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: Phase.self).pointee {
                case .dictating: return 0
                case .finalizing: return 1
                }
            },
            projectPayload: { _, _, _ in },
            inject: { slot, localIndex, _ in
                let phase: Phase
                switch localIndex {
                case 0: phase = .dictating
                case 1: phase = .finalizing
                default: fatalError("bad phase variant")
                }
                slot.assumingMemoryBound(to: Phase.self).initialize(to: phase)
            },
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 1, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
            ]
        ))
    )
}

// r[verify ir.stencils]
private func phaseDescriptor() -> (Descriptor, Registry) {
    let id = SchemaId(1)
    return (phaseDesc(id), Registry([phaseSchema(id)]))
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeUnitEnumEncodeDecodeAndReportsClean() throws {
    let (desc, reg) = phaseDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).scoped(method: "setPhase", phase: "args").isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    for value in [Phase.dictating, .finalizing] {
        let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
        let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
        #expect(jitBytes == interpBytes)

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<Phase>.size,
            alignment: MemoryLayout<Phase>.alignment
        )
        defer { raw.deallocate() }
        try decoder.run(jitBytes, raw)
        let decoded = raw.assumingMemoryBound(to: Phase.self).move()
        #expect(decoded == value)
    }

    #expect(throws: CompactError.self) {
        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<Phase>.size,
            alignment: MemoryLayout<Phase>.alignment
        )
        defer { raw.deallocate() }
        try decoder.run([99, 0, 0, 0], raw)
    }
}

// r[verify compat.enum]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatEnumWriterOnlyVariantReportsDistinctError() throws {
    let readerId = SchemaId(1)
    let writerId = SchemaId(2)
    let desc = phaseDesc(readerId)
    let reg = Registry([
        phaseSchema(readerId),
        evolvedPhaseWriterSchema(writerId),
    ])
    let lowered = try lowerDecode(writerId, desc, reg)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support writer-only enum detection: \(report.decode)")
    #expect(report.encode.contains(JitFallbackRecord(
        path: "$.0",
        reason: "Swift native encode JIT cannot emit decode-only writer-only enum variants"
    )))

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    func decodePhase(_ bytes: [UInt8]) throws -> Phase {
        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<Phase>.size,
            alignment: MemoryLayout<Phase>.alignment
        )
        defer { raw.deallocate() }
        try decoder.run(bytes, raw)
        return raw.assumingMemoryBound(to: Phase.self).move()
    }

    #expect(try decodePhase([0, 0, 0, 0]) == .dictating)
    #expect(try decodePhase([2, 0, 0, 0]) == .finalizing)

    do {
        _ = try decodePhase([1, 0, 0, 0])
        Issue.record("native compat enum accepted a writer-only variant")
    } catch let error as CompactError {
        #expect(errorKindName(error) == "WriterOnlyVariant")
    } catch {
        Issue.record("expected WriterOnlyVariant, got \(error)")
    }

    do {
        _ = try decodePhase([99, 0, 0, 0])
        Issue.record("native compat enum accepted a bad variant index")
    } catch let error as CompactError {
        #expect(errorKindName(error) == "BadVariantIndex")
    } catch {
        Issue.record("expected BadVariantIndex, got \(error)")
    }
}

// r[verify compat.enum]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
private struct MovePayloadCompat: Equatable {
    var y: UInt32
    var x: UInt32
    var extra: UInt32?
}

// r[verify compat.enum]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
private enum CommandCompat: Equatable {
    case move(MovePayloadCompat)
    case stop
}

// r[verify compat.enum]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
private func commandCompatPayloadDriftDescriptor() -> (
    writerRoot: SchemaId,
    readerRoot: SchemaId,
    descriptor: Descriptor,
    registry: Registry
) {
    let writerId = SchemaId(10)
    let optionId = SchemaId(11)
    let readerId = SchemaId(12)
    let batch = resolveIds([
        Schema(id: writerId, kind: .enumeration(name: "CmdCompat", variants: [
            Variant(name: "Move", index: 3, payload: .structure([
                Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "transient", schema: .concrete(primitiveId(.u64)), required: true),
                Field(name: "y", schema: .concrete(primitiveId(.u32)), required: true),
            ])),
            Variant(name: "Stop", index: 4, payload: .unit),
        ])),
        Schema(id: optionId, kind: .option(element: .concrete(primitiveId(.u32)))),
        Schema(id: readerId, kind: .enumeration(name: "CmdCompat", variants: [
            Variant(name: "Move", index: 0, payload: .structure([
                Field(name: "y", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "extra", schema: .concrete(optionId), required: false),
            ])),
            Variant(name: "Stop", index: 1, payload: .unit),
        ])),
    ])
    let writerRoot = batch[0].id
    let optionRoot = batch[1].id
    let readerRoot = batch[2].id
    let optionDesc = Descriptor(
        schema: .concrete(optionRoot),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let payloadLayout = MemoryLayout<MovePayloadCompat>.phonLayout
    let descriptor = Descriptor(
        schema: .concrete(readerRoot),
        layout: Layout(size: MemoryLayout<CommandCompat>.size, align: MemoryLayout<CommandCompat>.alignment),
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: CommandCompat.self).pointee {
                case .move: return 0
                case .stop: return 1
                }
            },
            projectPayload: { value, localIndex, scratch in
                guard localIndex == 0 else { return }
                guard case .move(let payload) = value.assumingMemoryBound(to: CommandCompat.self).pointee else {
                    return
                }
                scratch.assumingMemoryBound(to: MovePayloadCompat.self).initialize(to: payload)
            },
            destroyPayload: { scratch, localIndex in
                guard localIndex == 0 else { return }
                scratch.assumingMemoryBound(to: MovePayloadCompat.self).deinitialize(count: 1)
            },
            inject: { slot, localIndex, scratch in
                let value: CommandCompat
                switch localIndex {
                case 0:
                    value = .move(scratch.assumingMemoryBound(to: MovePayloadCompat.self).move())
                case 1:
                    value = .stop
                default:
                    fatalError("bad command variant")
                }
                slot.assumingMemoryBound(to: CommandCompat.self).initialize(to: value)
            },
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [
                        FieldAccess(offset: MemoryLayout<MovePayloadCompat>.offset(of: \MovePayloadCompat.y)!, descriptor: u32Desc()),
                        FieldAccess(offset: MemoryLayout<MovePayloadCompat>.offset(of: \MovePayloadCompat.x)!, descriptor: u32Desc()),
                        FieldAccess(
                            offset: MemoryLayout<MovePayloadCompat>.offset(of: \MovePayloadCompat.extra)!,
                            descriptor: optionDesc,
                            defaultInit: { $0.assumingMemoryBound(to: UInt32?.self).initialize(to: nil) }
                        ),
                    ],
                    payloadLayout: payloadLayout
                ),
                VariantAccess(wireIndex: 1, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
            ]
        ))
    )
    return (writerRoot, readerRoot, descriptor, Registry(batch))
}

// r[verify compat.enum]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativeCompatEnumStructPayloadDriftMatchesReaderOracle() throws {
    let fixture = commandCompatPayloadDriftDescriptor()
    let writerBytes = try encode(.object([
        .init(key: "Move", value: .object([
            .init(key: "x", value: .number(.canonical(unsigned: 3))),
            .init(key: "transient", value: .number(.canonical(unsigned: 999))),
            .init(key: "y", value: .number(.canonical(unsigned: 4))),
        ])),
    ]), fixture.writerRoot, fixture.registry)

    let lowered = try lowerDecode(fixture.writerRoot, fixture.descriptor, fixture.registry)
    let report = PhonJIT.nativeFallbackReport(lowered)
    #expect(report.decode.isEmpty, "native decode should support enum payload compat: \(report.decode)")
    #expect(report.encode.filter { $0.reason.contains("decode-only skip-wire") }.count == 1)
    #expect(report.encode.filter { $0.reason.contains("decode-only default") }.count == 1)
    #expect(try NativeEncode.compile(lowered) == nil)

    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<CommandCompat>.size,
        alignment: MemoryLayout<CommandCompat>.alignment
    )
    defer { raw.deallocate() }

    try decoder.run(writerBytes, raw)
    let decoded = raw.assumingMemoryBound(to: CommandCompat.self).move()
    #expect(decoded == .move(MovePayloadCompat(y: 4, x: 3, extra: nil)))

    let oracle = try planDecode(writerBytes, fixture.writerRoot, fixture.readerRoot, fixture.registry)
    let oracleBytes = try encode(oracle, fixture.readerRoot, fixture.registry)
    let readerLowered = try lowerTyped(fixture.descriptor, fixture.registry)
    #expect(PhonJIT.nativeFallbackReport(readerLowered).isEmpty)
    let typedBytes = withUnsafeBytes(of: decoded) { encodeWith(readerLowered, $0.baseAddress!) }
    #expect(typedBytes == oracleBytes)

    let encoder = try NativeEncode.compile(readerLowered)
    #expect(encoder != nil)
    if let encoder {
        let nativeBytes = withUnsafeBytes(of: decoded) { encoder.run($0.baseAddress!) }
        #expect(nativeBytes == oracleBytes)
    }
}

// r[verify ir.stencils]
private enum UserError: Equatable {
    case engineNotLoaded
    case loadFailed(String)
}

// r[verify ir.stencils]
private func userErrorDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .enumeration(name: "BeeErrorish", variants: [
        Variant(name: "EngineNotLoaded", index: 0, payload: .unit),
        Variant(name: "LoadFailed", index: 1, payload: .newtype(.concrete(primitiveId(.string)))),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<UserError>.size, align: MemoryLayout<UserError>.alignment),
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: UserError.self).pointee {
                case .engineNotLoaded: return 0
                case .loadFailed: return 1
                }
            },
            projectPayload: { value, localIndex, scratch in
                guard localIndex == 1 else { return }
                guard case .loadFailed(let message) = value.assumingMemoryBound(to: UserError.self).pointee else {
                    return
                }
                scratch.assumingMemoryBound(to: String.self).initialize(to: message)
            },
            destroyPayload: { scratch, localIndex in
                guard localIndex == 1 else { return }
                scratch.assumingMemoryBound(to: String.self).deinitialize(count: 1)
            },
            inject: { slot, localIndex, scratch in
                let value: UserError
                switch localIndex {
                case 0:
                    value = .engineNotLoaded
                case 1:
                    value = .loadFailed(scratch.assumingMemoryBound(to: String.self).move())
                default:
                    fatalError("bad error variant")
                }
                slot.assumingMemoryBound(to: UserError.self).initialize(to: value)
            },
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: 0, descriptor: stringDesc())],
                    payloadLayout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment)
                ),
            ]
        ))
    )
    return (desc, Registry([schema]))
}

// r[verify exec.jit-optional]
// r[verify ir.stencils]
@Test func nativePayloadEnumWithStringEncodeDecodeAndReportsClean() throws {
    let (desc, reg) = userErrorDescriptor()
    let lowered = try lowerTyped(desc, reg)
    #expect(PhonJIT.nativeFallbackReport(lowered).isEmpty)

    let encoder = try NativeEncode.compile(lowered)
    #expect(encoder != nil)
    guard let encoder else { return }
    let decoder = try NativeDecode.compile(lowered)
    #expect(decoder != nil)
    guard let decoder else { return }

    for value in [UserError.engineNotLoaded, .loadFailed("boom λ")] {
        let jitBytes = withUnsafeBytes(of: value) { encoder.run($0.baseAddress!) }
        let interpBytes = withUnsafeBytes(of: value) { encodeWith(lowered, $0.baseAddress!) }
        #expect(jitBytes == interpBytes)

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<UserError>.size,
            alignment: MemoryLayout<UserError>.alignment
        )
        defer { raw.deallocate() }
        try decoder.run(jitBytes, raw)
        let decoded = raw.assumingMemoryBound(to: UserError.self).move()
        #expect(decoded == value)
    }
}

// r[verify exec.strict-recording]
private struct MarkedTextArgs {
    var text: String
    var animationBudgetMs: UInt32
}

// r[verify exec.strict-recording]
private struct PhaseArgs {
    var phase: Phase
}

// r[verify exec.strict-recording]
private struct AdvanceTranscriptArgs {
    var text: String
    var committedLen: UInt32
    var animationBudgetMs: UInt32
}

// r[verify exec.strict-recording]
private struct ImeKeyEventArgs {
    var eventType: String
    var keyCode: UInt32
    var characters: String
}

// r[verify exec.strict-recording]
private func markedTextArgsDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "MarkedTextArgs", fields: [
        Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "animationBudgetMs", schema: .concrete(primitiveId(.u32)), required: true),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<MarkedTextArgs>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<MarkedTextArgs>.offset(of: \MarkedTextArgs.text)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<MarkedTextArgs>.offset(of: \MarkedTextArgs.animationBudgetMs)!, descriptor: u32Desc()),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([schema]))
}

// r[verify exec.strict-recording]
private func phaseArgsDescriptor() -> (Descriptor, Registry) {
    let phaseId = SchemaId(2)
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "PhaseArgs", fields: [
        Field(name: "phase", schema: .concrete(phaseId), required: true),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<PhaseArgs>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<PhaseArgs>.offset(of: \PhaseArgs.phase)!, descriptor: phaseDesc(phaseId)),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([schema, phaseSchema(phaseId)]))
}

// r[verify exec.strict-recording]
private func advanceTranscriptArgsDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "AdvanceTranscriptArgs", fields: [
        Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "committedLen", schema: .concrete(primitiveId(.u32)), required: true),
        Field(name: "animationBudgetMs", schema: .concrete(primitiveId(.u32)), required: true),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<AdvanceTranscriptArgs>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<AdvanceTranscriptArgs>.offset(of: \AdvanceTranscriptArgs.text)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<AdvanceTranscriptArgs>.offset(of: \AdvanceTranscriptArgs.committedLen)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<AdvanceTranscriptArgs>.offset(of: \AdvanceTranscriptArgs.animationBudgetMs)!, descriptor: u32Desc()),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([schema]))
}

// r[verify exec.strict-recording]
private func imeKeyEventArgsDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "ImeKeyEventArgs", fields: [
        Field(name: "eventType", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "keyCode", schema: .concrete(primitiveId(.u32)), required: true),
        Field(name: "characters", schema: .concrete(primitiveId(.string)), required: true),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<ImeKeyEventArgs>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<ImeKeyEventArgs>.offset(of: \ImeKeyEventArgs.eventType)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<ImeKeyEventArgs>.offset(of: \ImeKeyEventArgs.keyCode)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<ImeKeyEventArgs>.offset(of: \ImeKeyEventArgs.characters)!, descriptor: stringDesc()),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([schema]))
}

// r[verify exec.strict-recording]
private func assertNativeClean(_ method: String, _ phase: String, _ setup: (Descriptor, Registry)) throws {
    let lowered = try lowerTyped(setup.0, setup.1)
    let scoped = PhonJIT.nativeFallbackReport(lowered).scoped(method: method, phase: phase)
    #expect(scoped.isEmpty, "\(method) \(phase) fallback report should be empty: \(scoped)")
    #expect(try NativeEncode.compile(lowered) != nil)
    #expect(try NativeDecode.compile(lowered) != nil)
}

// r[verify exec.strict-recording]
// r[verify exec.jit-optional]
@Test func swiftImeMethodRootsAreNativeClean() throws {
    try assertNativeClean("setMarkedText", "args", markedTextArgsDescriptor())
    try assertNativeClean("setMarkedText", "response", (boolDesc(), Registry([])))
    try assertNativeClean("setPhase", "args", phaseArgsDescriptor())
    try assertNativeClean("setPhase", "response", (boolDesc(), Registry([])))
    try assertNativeClean("commitText", "args", textHolderDescriptor())
    try assertNativeClean("commitText", "response", (boolDesc(), Registry([])))
    try assertNativeClean("advanceTranscript", "args", advanceTranscriptArgsDescriptor())
    try assertNativeClean("advanceTranscript", "response", (boolDesc(), Registry([])))
    try assertNativeClean("imeKeyEvent", "args", imeKeyEventArgsDescriptor())
    try assertNativeClean("imeKeyEvent", "response", (boolDesc(), Registry([])))
}

// r[verify exec.strict-recording]
private struct FeedArgsHot {
    var sessionId: String
    var samples: [Float]
}

// r[verify exec.strict-recording]
private struct ConfidenceHot: Equatable {
    var meanLp: Float
    var minLp: Float
    var meanM: Float
    var minM: Float
}

// r[verify exec.strict-recording]
private struct AlignedWordHot: Equatable {
    var word: String
    var start: Double
    var end: Double
    var confidence: ConfidenceHot
}

// r[verify exec.strict-recording]
private struct CorrectionEditHot: Equatable {
    var editId: String
    var spanStart: UInt32
    var spanEnd: UInt32
    var original: String
    var replacement: String
    var term: String
    var aliasId: Int32
    var rankerProb: Double
    var gateProb: Double
}

// r[verify exec.strict-recording]
private struct FeedResultHot: Equatable {
    var text: String
    var committedUtf16Len: UInt32
    var alignments: [AlignedWordHot]
    var isFinal: Bool
    var detectedLanguage: String
    var correctionEdits: [CorrectionEditHot]
    var correctionSessionId: String
}

// r[verify exec.strict-recording]
private enum BeeErrorHot: Equatable {
    case engineNotLoaded
    case sessionNotFound(String)
    case loadFailed(String)
    case transcriptionError(String)
    case correctionError(String)
    case notImplemented
}

// r[verify exec.strict-recording]
private enum FeedResponseHot: Equatable {
    case ok(FeedResultHot?)
    case err(BeeErrorHot)
}

// r[verify exec.strict-recording]
private func arraySeqWitness<T>(of _: T.Type) -> SeqWitness {
    SeqWitness(
        count: { handle in handle.assumingMemoryBound(to: [T].self).pointee.count },
        copyElements: { handle, dst in
            handle.assumingMemoryBound(to: [T].self).pointee.withUnsafeBytes { buf in
                if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
            }
        },
        construct: { handle, src, count in
            handle.assumingMemoryBound(to: [T].self).initialize(to: Array(unsafeUninitializedCapacity: count) { dst, n in
                if count > 0 {
                    dst.baseAddress!.moveInitialize(from: src.assumingMemoryBound(to: T.self), count: count)
                }
                n = count
            })
        }
    )
}

// r[verify exec.strict-recording]
private func feedArgsDescriptor() -> (Descriptor, Registry) {
    let samplesId = SchemaId(2)
    let samplesSchema = Schema(id: samplesId, kind: .list(element: .concrete(primitiveId(.f32))))
    let root = Schema(id: SchemaId(1), kind: .structure(name: "FeedArgs", fields: [
        Field(name: "sessionId", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "samples", schema: .concrete(samplesId), required: true),
    ]))
    let samplesDesc = Descriptor(
        schema: .concrete(samplesId),
        layout: Layout(size: MemoryLayout<[Float]>.size, align: MemoryLayout<[Float]>.alignment),
        access: .bytes(BytesAccess(
            stride: MemoryLayout<Float>.stride,
            elemAlign: MemoryLayout<Float>.alignment,
            witness: floatArrayWitness()
        ))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<FeedArgsHot>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<FeedArgsHot>.offset(of: \FeedArgsHot.sessionId)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<FeedArgsHot>.offset(of: \FeedArgsHot.samples)!, descriptor: samplesDesc),
            ],
            construct: .inPlace
        ))
    )
    return (desc, Registry([samplesSchema, root]))
}

// r[verify exec.strict-recording]
private func confidenceHotDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<ConfidenceHot>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.meanLp)!, descriptor: f32Desc()),
                FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.minLp)!, descriptor: f32Desc()),
                FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.meanM)!, descriptor: f32Desc()),
                FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.minM)!, descriptor: f32Desc()),
            ],
            construct: .inPlace
        ))
    )
}

// r[verify exec.strict-recording]
private func alignedWordHotDesc(_ id: SchemaId, confidenceId: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<AlignedWordHot>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.word)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.start)!, descriptor: f64Desc()),
                FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.end)!, descriptor: f64Desc()),
                FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.confidence)!, descriptor: confidenceHotDesc(confidenceId)),
            ],
            construct: .inPlace
        ))
    )
}

// r[verify exec.strict-recording]
private func correctionEditHotDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<CorrectionEditHot>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.editId)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.spanStart)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.spanEnd)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.original)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.replacement)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.term)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.aliasId)!, descriptor: i32Desc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.rankerProb)!, descriptor: f64Desc()),
                FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.gateProb)!, descriptor: f64Desc()),
            ],
            construct: .inPlace
        ))
    )
}

// r[verify exec.strict-recording]
private func feedResultHotDesc(
    _ id: SchemaId,
    alignedListId: SchemaId,
    alignedWordId: SchemaId,
    confidenceId: SchemaId,
    correctionListId: SchemaId,
    correctionEditId: SchemaId
) -> Descriptor {
    let alignedListDesc = Descriptor(
        schema: .concrete(alignedListId),
        layout: Layout(size: MemoryLayout<[AlignedWordHot]>.size, align: MemoryLayout<[AlignedWordHot]>.alignment),
        access: .sequence(SequenceAccess(
            element: alignedWordHotDesc(alignedWordId, confidenceId: confidenceId),
            stride: MemoryLayout<AlignedWordHot>.stride,
            elemAlign: MemoryLayout<AlignedWordHot>.alignment,
            witness: arraySeqWitness(of: AlignedWordHot.self)
        ))
    )
    let correctionListDesc = Descriptor(
        schema: .concrete(correctionListId),
        layout: Layout(size: MemoryLayout<[CorrectionEditHot]>.size, align: MemoryLayout<[CorrectionEditHot]>.alignment),
        access: .sequence(SequenceAccess(
            element: correctionEditHotDesc(correctionEditId),
            stride: MemoryLayout<CorrectionEditHot>.stride,
            elemAlign: MemoryLayout<CorrectionEditHot>.alignment,
            witness: arraySeqWitness(of: CorrectionEditHot.self)
        ))
    )
    return Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<FeedResultHot>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.text)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.committedUtf16Len)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.alignments)!, descriptor: alignedListDesc),
                FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.isFinal)!, descriptor: boolDesc()),
                FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.detectedLanguage)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.correctionEdits)!, descriptor: correctionListDesc),
                FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.correctionSessionId)!, descriptor: stringDesc()),
            ],
            construct: .inPlace
        ))
    )
}

// r[verify exec.strict-recording]
private func feedResultOptionWitness() -> OptionWitness {
    OptionWitness(
        projectSome: { option, scratch in
            guard let value = option.assumingMemoryBound(to: FeedResultHot?.self).pointee else {
                return false
            }
            scratch.assumingMemoryBound(to: FeedResultHot.self).initialize(to: value)
            return true
        },
        initSome: { option, value in
            option.assumingMemoryBound(to: FeedResultHot?.self).initialize(
                to: .some(value.assumingMemoryBound(to: FeedResultHot.self).move())
            )
        },
        initNone: { option in
            option.assumingMemoryBound(to: FeedResultHot?.self).initialize(to: .none)
        }
    )
}

// r[verify exec.strict-recording]
private func beeErrorHotDesc(_ id: SchemaId) -> Descriptor {
    let stringPayload = VariantAccess(
        wireIndex: 0,
        payloadFields: [FieldAccess(offset: 0, descriptor: stringDesc())],
        payloadLayout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment)
    )
    let variants = [
        VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
        VariantAccess(wireIndex: 1, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 2, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 3, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 4, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 5, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
    ]
    return Descriptor(
        schema: .concrete(id),
        layout: Layout(size: MemoryLayout<BeeErrorHot>.size, align: MemoryLayout<BeeErrorHot>.alignment),
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: BeeErrorHot.self).pointee {
                case .engineNotLoaded: return 0
                case .sessionNotFound: return 1
                case .loadFailed: return 2
                case .transcriptionError: return 3
                case .correctionError: return 4
                case .notImplemented: return 5
                }
            },
            projectPayload: { value, localIndex, scratch in
                let message: String?
                switch value.assumingMemoryBound(to: BeeErrorHot.self).pointee {
                case .engineNotLoaded, .notImplemented:
                    message = nil
                case .sessionNotFound(let text), .loadFailed(let text),
                     .transcriptionError(let text), .correctionError(let text):
                    message = text
                }
                if let message {
                    scratch.assumingMemoryBound(to: String.self).initialize(to: message)
                } else {
                    _ = localIndex
                }
            },
            destroyPayload: { scratch, localIndex in
                guard (1...4).contains(localIndex) else { return }
                scratch.assumingMemoryBound(to: String.self).deinitialize(count: 1)
            },
            inject: { slot, localIndex, scratch in
                let value: BeeErrorHot
                switch localIndex {
                case 0:
                    value = .engineNotLoaded
                case 1:
                    value = .sessionNotFound(scratch.assumingMemoryBound(to: String.self).move())
                case 2:
                    value = .loadFailed(scratch.assumingMemoryBound(to: String.self).move())
                case 3:
                    value = .transcriptionError(scratch.assumingMemoryBound(to: String.self).move())
                case 4:
                    value = .correctionError(scratch.assumingMemoryBound(to: String.self).move())
                case 5:
                    value = .notImplemented
                default:
                    fatalError("bad BeeError variant")
                }
                slot.assumingMemoryBound(to: BeeErrorHot.self).initialize(to: value)
            },
            variants: variants
        ))
    )
}

// r[verify exec.strict-recording]
private func feedResponseDescriptor() -> (Descriptor, Registry) {
    let responseId = SchemaId(1)
    let optionId = SchemaId(2)
    let feedResultId = SchemaId(3)
    let alignedListId = SchemaId(4)
    let alignedWordId = SchemaId(5)
    let confidenceId = SchemaId(6)
    let correctionListId = SchemaId(7)
    let correctionEditId = SchemaId(8)
    let errorId = SchemaId(9)

    let schemas = [
        Schema(id: responseId, kind: .enumeration(name: "FeedResponse", variants: [
            Variant(name: "Ok", index: 0, payload: .newtype(.concrete(optionId))),
            Variant(name: "Err", index: 1, payload: .newtype(.concrete(errorId))),
        ])),
        Schema(id: optionId, kind: .option(element: .concrete(feedResultId))),
        Schema(id: feedResultId, kind: .structure(name: "FeedResult", fields: [
            Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "committedUtf16Len", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "alignments", schema: .concrete(alignedListId), required: true),
            Field(name: "isFinal", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "detectedLanguage", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "correctionEdits", schema: .concrete(correctionListId), required: true),
            Field(name: "correctionSessionId", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: alignedListId, kind: .list(element: .concrete(alignedWordId))),
        Schema(id: alignedWordId, kind: .structure(name: "AlignedWord", fields: [
            Field(name: "word", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "start", schema: .concrete(primitiveId(.f64)), required: true),
            Field(name: "end", schema: .concrete(primitiveId(.f64)), required: true),
            Field(name: "confidence", schema: .concrete(confidenceId), required: true),
        ])),
        Schema(id: confidenceId, kind: .structure(name: "Confidence", fields: [
            Field(name: "meanLp", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "minLp", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "meanM", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "minM", schema: .concrete(primitiveId(.f32)), required: true),
        ])),
        Schema(id: correctionListId, kind: .list(element: .concrete(correctionEditId))),
        Schema(id: correctionEditId, kind: .structure(name: "CorrectionEdit", fields: [
            Field(name: "editId", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "spanStart", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "spanEnd", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "original", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "replacement", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "term", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "aliasId", schema: .concrete(primitiveId(.i32)), required: true),
            Field(name: "rankerProb", schema: .concrete(primitiveId(.f64)), required: true),
            Field(name: "gateProb", schema: .concrete(primitiveId(.f64)), required: true),
        ])),
        Schema(id: errorId, kind: .enumeration(name: "BeeError", variants: [
            Variant(name: "EngineNotLoaded", index: 0, payload: .unit),
            Variant(name: "SessionNotFound", index: 1, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "LoadFailed", index: 2, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "TranscriptionError", index: 3, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "CorrectionError", index: 4, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "NotImplemented", index: 5, payload: .unit),
        ])),
    ]

    let optionDesc = Descriptor(
        schema: .concrete(optionId),
        layout: Layout(size: MemoryLayout<FeedResultHot?>.size, align: MemoryLayout<FeedResultHot?>.alignment),
        access: .option(OptionAccess(
            witness: feedResultOptionWitness(),
            some: feedResultHotDesc(
                feedResultId,
                alignedListId: alignedListId,
                alignedWordId: alignedWordId,
                confidenceId: confidenceId,
                correctionListId: correctionListId,
                correctionEditId: correctionEditId
            )
        ))
    )
    let errorDesc = beeErrorHotDesc(errorId)
    let responseDesc = Descriptor(
        schema: .concrete(responseId),
        layout: Layout(size: MemoryLayout<FeedResponseHot>.size, align: MemoryLayout<FeedResponseHot>.alignment),
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: FeedResponseHot.self).pointee {
                case .ok: return 0
                case .err: return 1
                }
            },
            projectPayload: { value, localIndex, scratch in
                switch (localIndex, value.assumingMemoryBound(to: FeedResponseHot.self).pointee) {
                case (0, .ok(let result)):
                    scratch.assumingMemoryBound(to: FeedResultHot?.self).initialize(to: result)
                case (1, .err(let error)):
                    scratch.assumingMemoryBound(to: BeeErrorHot.self).initialize(to: error)
                default:
                    break
                }
            },
            destroyPayload: { scratch, localIndex in
                switch localIndex {
                case 0:
                    scratch.assumingMemoryBound(to: FeedResultHot?.self).deinitialize(count: 1)
                case 1:
                    scratch.assumingMemoryBound(to: BeeErrorHot.self).deinitialize(count: 1)
                default:
                    break
                }
            },
            inject: { slot, localIndex, scratch in
                let value: FeedResponseHot
                switch localIndex {
                case 0:
                    value = .ok(scratch.assumingMemoryBound(to: FeedResultHot?.self).move())
                case 1:
                    value = .err(scratch.assumingMemoryBound(to: BeeErrorHot.self).move())
                default:
                    fatalError("bad feed response variant")
                }
                slot.assumingMemoryBound(to: FeedResponseHot.self).initialize(to: value)
            },
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [FieldAccess(offset: 0, descriptor: optionDesc)],
                    payloadLayout: Layout(size: MemoryLayout<FeedResultHot?>.size, align: MemoryLayout<FeedResultHot?>.alignment)
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: 0, descriptor: errorDesc)],
                    payloadLayout: Layout(size: MemoryLayout<BeeErrorHot>.size, align: MemoryLayout<BeeErrorHot>.alignment)
                ),
            ]
        ))
    )

    return (responseDesc, Registry(schemas))
}

// r[verify exec.strict-recording]
// r[verify exec.jit-optional]
@Test func swiftBeeFeedMethodRootsAreNativeClean() throws {
    try assertNativeClean("feed", "args", feedArgsDescriptor())
    try assertNativeClean("feed", "response", feedResponseDescriptor())
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
