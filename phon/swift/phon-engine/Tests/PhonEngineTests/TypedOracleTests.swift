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
// bytes, which is exactly what the typed path must translate.
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

// A struct with an optional scalar field — validates the witness-through-engine
// mechanism (project into scratch on encode, init in place on decode).
private struct OptHolder: Equatable {
    var v: UInt32?
}

private func uint32OptionWitness() -> OptionWitness {
    OptionWitness(
        projectSome: { option, scratch in
            guard let v = option.load(as: UInt32?.self) else { return false }
            scratch.storeBytes(of: v, as: UInt32.self)
            return true
        },
        initSome: { option, value in
            option.storeBytes(of: UInt32?(value.load(as: UInt32.self)), as: UInt32?.self)
        },
        initNone: { option in
            option.storeBytes(of: UInt32?.none, as: UInt32?.self)
        }
    )
}

private func optHolderSetup() -> (root: SchemaId, reg: Registry, desc: Descriptor) {
    let optU32 = Schema(id: SchemaId(2), kind: .option(element: .concrete(primitiveId(.u32))))
    let holder = Schema(
        id: SchemaId(1),
        kind: .structure(name: "OptHolder", fields: [
            Field(name: "v", schema: .concrete(SchemaId(2)), required: true),
        ])
    )
    let reg = Registry([optU32, holder])

    let optDesc = Descriptor(
        schema: .concrete(SchemaId(2)),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: uint32OptionWitness(), some: scalarDesc(.u32)))
    )
    let holderDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<OptHolder>.size, align: MemoryLayout<OptHolder>.alignment),
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<OptHolder>.offset(of: \OptHolder.v)!, descriptor: optDesc)],
            construct: .inPlace
        ))
    )
    return (SchemaId(1), reg, holderDesc)
}

// r[verify descriptors.encode-decode-asymmetry]
@Test
func typedOptionMatchesValueOracleAndRoundTrips() throws {
    let (root, reg, desc) = optHolderSetup()
    let program = try lowerTyped(desc, reg)

    for holder in [OptHolder(v: 42), OptHolder(v: nil)] {
        let typedBytes = withUnsafeBytes(of: holder) { encodeWith(program, $0.baseAddress!) }

        let oracleField: Value = holder.v.map { .number(.canonical(unsigned: UInt128($0))) } ?? .null
        let oracleBytes = try encode(.object([.init(key: "v", value: oracleField)]), root, reg)
        #expect(typedBytes == oracleBytes, "option \(String(describing: holder.v)): typed bytes diverge from oracle")

        var decoded = OptHolder(v: 0)
        try withUnsafeMutableBytes(of: &decoded) { try decodeInto(program, typedBytes, $0.baseAddress!) }
        #expect(decoded == holder, "option \(String(describing: holder.v)): decode did not round-trip")
    }
}

// A struct with a Dynamic (self-describing Value) field — the metadata shape.
private struct DynHolder {
    var meta: Value
}

@Test
func typedDynamicMatchesValueOracleAndRoundTrips() throws {
    let dyn = Schema(id: SchemaId(2), kind: .dynamic)
    let holder = Schema(
        id: SchemaId(1),
        kind: .structure(name: "DynHolder", fields: [
            Field(name: "meta", schema: .concrete(SchemaId(2)), required: true),
        ])
    )
    let reg = Registry([dyn, holder])

    let dynDesc = Descriptor(
        schema: .concrete(SchemaId(2)),
        layout: Layout(size: MemoryLayout<Value>.size, align: MemoryLayout<Value>.alignment),
        access: .dynamic
    )
    let holderDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<DynHolder>.size, align: MemoryLayout<DynHolder>.alignment),
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<DynHolder>.offset(of: \DynHolder.meta)!, descriptor: dynDesc)],
            construct: .inPlace
        ))
    )
    let program = try lowerTyped(holderDesc, reg)

    let meta: Value = .object([
        .init(key: "k", value: .number(.canonical(unsigned: 1))),
        .init(key: "flag", value: .bool(true)),
    ])
    let holderVal = DynHolder(meta: meta)

    let typedBytes = withUnsafeBytes(of: holderVal) { encodeWith(program, $0.baseAddress!) }
    let oracleBytes = try encode(.object([.init(key: "meta", value: meta)]), SchemaId(1), reg)
    #expect(typedBytes == oracleBytes, "dynamic: typed bytes diverge from oracle")

    // Decode into uninitialized storage (the field is a non-trivial Value).
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<DynHolder>.size, alignment: MemoryLayout<DynHolder>.alignment)
    defer { raw.deallocate() }
    try decodeInto(program, typedBytes, raw)
    let decoded = raw.assumingMemoryBound(to: DynHolder.self).move()
    #expect(decoded.meta == meta, "dynamic: decode did not round-trip")
}

// A struct with a String field — the bulk byte-run path that echo needs.
private struct StrHolder {
    var s: String
}

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

@Test
func typedStringMatchesValueOracleAndRoundTrips() throws {
    let holderSchema = Schema(
        id: SchemaId(1),
        kind: .structure(name: "StrHolder", fields: [
            Field(name: "s", schema: .concrete(primitiveId(.string)), required: true),
        ])
    )
    let reg = Registry([holderSchema])

    let strDesc = Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: stringWitness()))
    )
    let holderDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<StrHolder>.size, align: MemoryLayout<StrHolder>.alignment),
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<StrHolder>.offset(of: \StrHolder.s)!, descriptor: strDesc)],
            construct: .inPlace
        ))
    )
    let program = try lowerTyped(holderDesc, reg)

    for text in ["héllo λ", "", "plain ascii"] {
        let holder = StrHolder(s: text)
        let typedBytes = withUnsafeBytes(of: holder) { encodeWith(program, $0.baseAddress!) }
        let oracleBytes = try encode(.object([.init(key: "s", value: .string(text))]), SchemaId(1), reg)
        #expect(typedBytes == oracleBytes, "string \(text.debugDescription): typed bytes diverge from oracle")

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<StrHolder>.size, alignment: MemoryLayout<StrHolder>.alignment)
        defer { raw.deallocate() }
        try decodeInto(program, typedBytes, raw)
        let decoded = raw.assumingMemoryBound(to: StrHolder.self).move()
        #expect(decoded.s == text, "string \(text.debugDescription): decode did not round-trip")
    }
}

// An enum with all three payload shapes that lower here: unit, newtype, tuple.
private enum E: Equatable {
    case a
    case b(UInt32)
    case c(UInt8, UInt8)
}

private func enumDescriptor() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: E.self).pointee {
        case .a: return 0
        case .b: return 1
        case .c: return 2
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: E.self).pointee {
        case .a:
            break
        case .b(let x):
            scratch.storeBytes(of: x, as: UInt32.self)
        case .c(let a, let b):
            scratch.storeBytes(of: a, toByteOffset: 0, as: UInt8.self)
            scratch.storeBytes(of: b, toByteOffset: 1, as: UInt8.self)
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let e: E
        switch localIndex {
        case 0: e = .a
        case 1: e = .b(scratch.load(as: UInt32.self))
        case 2: e = .c(scratch.load(as: UInt8.self), scratch.load(fromByteOffset: 1, as: UInt8.self))
        default: fatalError("bad variant index")
        }
        slot.assumingMemoryBound(to: E.self).initialize(to: e)
    }
    return Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<E>.size, align: MemoryLayout<E>.alignment),
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            inject: inject,
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.u32))],
                    payloadLayout: Layout(size: 4, align: 4)
                ),
                VariantAccess(
                    wireIndex: 2,
                    payloadFields: [
                        FieldAccess(offset: 0, descriptor: scalarDesc(.u8)),
                        FieldAccess(offset: 1, descriptor: scalarDesc(.u8)),
                    ],
                    payloadLayout: Layout(size: 2, align: 1)
                ),
            ]
        ))
    )
}

@Test
func typedEnumMatchesValueOracleAndRoundTrips() throws {
    let eSchema = Schema(
        id: SchemaId(1),
        kind: .enumeration(name: "E", variants: [
            Variant(name: "A", index: 0, payload: .unit),
            Variant(name: "B", index: 1, payload: .newtype(.concrete(primitiveId(.u32)))),
            Variant(name: "C", index: 2, payload: .tuple([.concrete(primitiveId(.u8)), .concrete(primitiveId(.u8))])),
        ])
    )
    let reg = Registry([eSchema])
    let program = try lowerTyped(enumDescriptor(), reg)

    let cases: [(E, Value)] = [
        (.a, .object([.init(key: "A", value: .null)])),
        (.b(42), .object([.init(key: "B", value: .number(.canonical(unsigned: 42)))])),
        (.c(1, 2), .object([.init(key: "C", value: .array([
            .number(.canonical(unsigned: 1)), .number(.canonical(unsigned: 2)),
        ]))])),
    ]

    for (value, oracle) in cases {
        let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }
        let oracleBytes = try encode(oracle, SchemaId(1), reg)
        #expect(typedBytes == oracleBytes, "enum \(value): typed bytes diverge from oracle")

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: max(MemoryLayout<E>.size, 1), alignment: MemoryLayout<E>.alignment)
        defer { raw.deallocate() }
        try decodeInto(program, typedBytes, raw)
        let decoded = raw.assumingMemoryBound(to: E.self).move()
        #expect(decoded == value, "enum \(value): decode did not round-trip")
    }
}

// A list of structured (trivially-copyable) elements — get_points / swap_pair.
private struct Pair: Equatable {
    var a: UInt32
    var b: UInt32
}

private func pairDescriptor(_ id: SchemaId = SchemaId(2)) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: Layout(size: MemoryLayout<Pair>.size, align: MemoryLayout<Pair>.alignment),
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.a)!, descriptor: scalarDesc(.u32)),
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.b)!, descriptor: scalarDesc(.u32)),
            ],
            construct: .inPlace
        ))
    )
}

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

// Witnesses for [String] — managed (non-trivial) elements; decode must MOVE the
// decoded Strings out of scratch, not copy them.
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

@Test
func typedStringSequenceMatchesValueOracleAndRoundTrips() throws {
    let listSchema = Schema(id: SchemaId(1), kind: .list(element: .concrete(primitiveId(.string))))
    let reg = Registry([listSchema])

    let elemDesc = Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: stringWitness()))
    )
    let listDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<[String]>.size, align: MemoryLayout<[String]>.alignment),
        access: .sequence(SequenceAccess(
            element: elemDesc,
            stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment,
            witness: stringSeqWitness()
        ))
    )
    let program = try lowerTyped(listDesc, reg)

    for value in [["alpha", "βeta", ""], [], ["solo"]] {
        let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }
        let oracle: Value = .array(value.map { .string($0) })
        let oracleBytes = try encode(oracle, SchemaId(1), reg)
        #expect(typedBytes == oracleBytes, "string-seq \(value): typed bytes diverge from oracle")

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<[String]>.size, alignment: MemoryLayout<[String]>.alignment)
        defer { raw.deallocate() }
        try decodeInto(program, typedBytes, raw)
        let decoded = raw.assumingMemoryBound(to: [String].self).move()
        #expect(decoded == value, "string-seq \(value): decode did not round-trip")
    }
}

// A string-keyed map ([String: UInt32]) — entries emitted in sorted-key order.
private func stringU32MapWitness() -> MapWitness {
    let kStride = MemoryLayout<String>.stride
    let vStride = MemoryLayout<UInt32>.stride
    return MapWitness(
        count: { handle in handle.assumingMemoryBound(to: [String: UInt32].self).pointee.count },
        projectEntries: { handle, keys, values in
            let dict = handle.assumingMemoryBound(to: [String: UInt32].self).pointee
            for (i, e) in dict.sorted(by: { $0.key < $1.key }).enumerated() {
                keys.advanced(by: i * kStride).assumingMemoryBound(to: String.self).initialize(to: e.key)
                values.advanced(by: i * vStride).storeBytes(of: e.value, as: UInt32.self)
            }
        },
        destroyEntries: { keys, _, count in
            for i in 0..<count {
                keys.advanced(by: i * kStride).assumingMemoryBound(to: String.self).deinitialize(count: 1)
            }
        },
        initWithCapacity: { handle, cap in
            handle.assumingMemoryBound(to: [String: UInt32].self).initialize(to: .init(minimumCapacity: cap))
        },
        insert: { handle, key, value in
            let k = key.assumingMemoryBound(to: String.self).move()
            let v = value.load(as: UInt32.self)
            handle.assumingMemoryBound(to: [String: UInt32].self).pointee[k] = v
        }
    )
}

@Test
func typedMapMatchesValueOracleAndRoundTrips() throws {
    let mapSchema = Schema(
        id: SchemaId(1),
        kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(primitiveId(.u32)))
    )
    let reg = Registry([mapSchema])

    let keyDesc = Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: stringWitness()))
    )
    let mapDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<[String: UInt32]>.size, align: MemoryLayout<[String: UInt32]>.alignment),
        access: .map(MapAccess(
            key: keyDesc,
            value: scalarDesc(.u32),
            keyStride: MemoryLayout<String>.stride, keyAlign: MemoryLayout<String>.alignment,
            valueStride: MemoryLayout<UInt32>.stride, valueAlign: MemoryLayout<UInt32>.alignment,
            witness: stringU32MapWitness()
        ))
    )
    let program = try lowerTyped(mapDesc, reg)
    let decProgram = try lowerDecode(mapDesc, reg)

    for value: [String: UInt32] in [["banana": 2, "apple": 1, "cherry": 3], [:], ["solo": 9]] {
        let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }
        // Oracle: an object with entries in sorted-key order (matching projectEntries).
        let oracle: Value = .object(value.sorted(by: { $0.key < $1.key }).map {
            .init(key: $0.key, value: .number(.canonical(unsigned: UInt128($0.value))))
        })
        let oracleBytes = try encode(oracle, SchemaId(1), reg)
        #expect(typedBytes == oracleBytes, "map \(value): typed bytes diverge from oracle")

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<[String: UInt32]>.size, alignment: MemoryLayout<[String: UInt32]>.alignment)
        defer { raw.deallocate() }
        try decodeInto(decProgram, typedBytes, raw)
        let decoded = raw.assumingMemoryBound(to: [String: UInt32].self).move()
        #expect(decoded == value, "map \(value): decode did not round-trip")
    }
}

@Test
func typedSequenceMatchesValueOracleAndRoundTrips() throws {
    let pairSchema = Schema(
        id: SchemaId(2),
        kind: .structure(name: "Pair", fields: [
            Field(name: "a", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "b", schema: .concrete(primitiveId(.u32)), required: true),
        ])
    )
    let listSchema = Schema(id: SchemaId(1), kind: .list(element: .concrete(SchemaId(2))))
    let reg = Registry([pairSchema, listSchema])

    let listDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<[Pair]>.size, align: MemoryLayout<[Pair]>.alignment),
        access: .sequence(SequenceAccess(
            element: pairDescriptor(),
            stride: MemoryLayout<Pair>.stride,
            elemAlign: MemoryLayout<Pair>.alignment,
            witness: pairSeqWitness()
        ))
    )
    let program = try lowerTyped(listDesc, reg)

    for value in [[Pair(a: 1, b: 2), Pair(a: 3, b: 4)], [], [Pair(a: 9, b: 9)]] {
        let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }

        let oracle: Value = .array(value.map {
            .object([
                .init(key: "a", value: .number(.canonical(unsigned: UInt128($0.a)))),
                .init(key: "b", value: .number(.canonical(unsigned: UInt128($0.b)))),
            ])
        })
        let oracleBytes = try encode(oracle, SchemaId(1), reg)
        #expect(typedBytes == oracleBytes, "sequence \(value): typed bytes diverge from oracle")

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<[Pair]>.size, alignment: MemoryLayout<[Pair]>.alignment)
        defer { raw.deallocate() }
        try decodeInto(program, typedBytes, raw)
        let decoded = raw.assumingMemoryBound(to: [Pair].self).move()
        #expect(decoded == value, "sequence \(value): decode did not round-trip")
    }
}

// A nested struct mixing every landed shape — the envelope's complexity in
// miniature: a scalar, a String, an Option, a list<struct>, an enum, and a
// Dynamic field. Proves the shapes compose.
private struct Envelopeish {
    var id: UInt64
    var label: String
    var hint: UInt32?
    var pairs: [Pair]
    var choice: E
    var meta: Value
}

@Test
func typedCompositeMatchesValueOracleAndRoundTrips() throws {
    // Schemas: id u64, label string, hint option<u32>, pairs list<Pair>,
    // choice E, meta dynamic.
    let pairSchema = Schema(id: SchemaId(10), kind: .structure(name: "Pair", fields: [
        Field(name: "a", schema: .concrete(primitiveId(.u32)), required: true),
        Field(name: "b", schema: .concrete(primitiveId(.u32)), required: true),
    ]))
    let listSchema = Schema(id: SchemaId(11), kind: .list(element: .concrete(SchemaId(10))))
    let optSchema = Schema(id: SchemaId(12), kind: .option(element: .concrete(primitiveId(.u32))))
    let eSchema = Schema(id: SchemaId(13), kind: .enumeration(name: "E", variants: [
        Variant(name: "A", index: 0, payload: .unit),
        Variant(name: "B", index: 1, payload: .newtype(.concrete(primitiveId(.u32)))),
        Variant(name: "C", index: 2, payload: .tuple([.concrete(primitiveId(.u8)), .concrete(primitiveId(.u8))])),
    ]))
    let dynSchema = Schema(id: SchemaId(14), kind: .dynamic)
    let root = Schema(id: SchemaId(1), kind: .structure(name: "Envelopeish", fields: [
        Field(name: "id", schema: .concrete(primitiveId(.u64)), required: true),
        Field(name: "label", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "hint", schema: .concrete(SchemaId(12)), required: true),
        Field(name: "pairs", schema: .concrete(SchemaId(11)), required: true),
        Field(name: "choice", schema: .concrete(SchemaId(13)), required: true),
        Field(name: "meta", schema: .concrete(SchemaId(14)), required: true),
    ]))
    let reg = Registry([pairSchema, listSchema, optSchema, eSchema, dynSchema, root])

    func fieldDesc(_ off: Int, _ schema: SchemaRef, _ access: Access, _ layout: Layout) -> FieldAccess {
        FieldAccess(offset: off, descriptor: Descriptor(schema: schema, layout: layout, access: access))
    }
    var enumDesc = enumDescriptor()
    enumDesc.schema = .concrete(SchemaId(13))
    let rootDesc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<Envelopeish>.size, align: MemoryLayout<Envelopeish>.alignment),
        access: .record(RecordAccess(fields: [
            fieldDesc(MemoryLayout<Envelopeish>.offset(of: \Envelopeish.id)!, .concrete(primitiveId(.u64)), .scalar, Layout(size: 8, align: 8)),
            fieldDesc(MemoryLayout<Envelopeish>.offset(of: \Envelopeish.label)!, .concrete(primitiveId(.string)), .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: stringWitness())), Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment)),
            fieldDesc(MemoryLayout<Envelopeish>.offset(of: \Envelopeish.hint)!, .concrete(SchemaId(12)), .option(OptionAccess(witness: uint32OptionWitness(), some: scalarDesc(.u32))), Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment)),
            fieldDesc(MemoryLayout<Envelopeish>.offset(of: \Envelopeish.pairs)!, .concrete(SchemaId(11)), .sequence(SequenceAccess(element: pairDescriptor(SchemaId(10)), stride: MemoryLayout<Pair>.stride, elemAlign: MemoryLayout<Pair>.alignment, witness: pairSeqWitness())), Layout(size: MemoryLayout<[Pair]>.size, align: MemoryLayout<[Pair]>.alignment)),
            FieldAccess(offset: MemoryLayout<Envelopeish>.offset(of: \Envelopeish.choice)!, descriptor: enumDesc),
            fieldDesc(MemoryLayout<Envelopeish>.offset(of: \Envelopeish.meta)!, .concrete(SchemaId(14)), .dynamic, Layout(size: MemoryLayout<Value>.size, align: MemoryLayout<Value>.alignment)),
        ], construct: .inPlace))
    )
    let program = try lowerTyped(rootDesc, reg)
    // Decode through the single compat path (same-schema = no skips/defaults),
    // exercising lowerDecode for scalar/record/option/string/sequence/enum/dynamic.
    let decProgram = try lowerDecode(rootDesc, reg)

    let meta: Value = .object([.init(key: "k", value: .bool(true))])
    let value = Envelopeish(id: 0xDEAD_BEEF, label: "hï", hint: 7, pairs: [Pair(a: 1, b: 2)], choice: .c(3, 4), meta: meta)

    let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }
    let oracle: Value = .object([
        .init(key: "id", value: .number(.canonical(unsigned: 0xDEAD_BEEF))),
        .init(key: "label", value: .string("hï")),
        .init(key: "hint", value: .number(.canonical(unsigned: 7))),
        .init(key: "pairs", value: .array([.object([
            .init(key: "a", value: .number(.canonical(unsigned: 1))),
            .init(key: "b", value: .number(.canonical(unsigned: 2))),
        ])])),
        .init(key: "choice", value: .object([.init(key: "C", value: .array([
            .number(.canonical(unsigned: 3)), .number(.canonical(unsigned: 4)),
        ]))])),
        .init(key: "meta", value: meta),
    ])
    #expect(typedBytes == (try encode(oracle, SchemaId(1), reg)), "composite: typed bytes diverge from oracle")

    let raw = UnsafeMutableRawPointer.allocate(byteCount: MemoryLayout<Envelopeish>.size, alignment: MemoryLayout<Envelopeish>.alignment)
    defer { raw.deallocate() }
    try decodeInto(decProgram, typedBytes, raw)
    let decoded = raw.assumingMemoryBound(to: Envelopeish.self).move()
    #expect(decoded.id == value.id && decoded.label == value.label && decoded.hint == value.hint
        && decoded.pairs == value.pairs && decoded.choice == value.choice && decoded.meta == value.meta,
        "composite: decode did not round-trip")
}
