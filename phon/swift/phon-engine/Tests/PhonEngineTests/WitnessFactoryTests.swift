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

// A newtype enum wrapping a managed payload — the MessagePayload shape. Exercises
// projectPayload(retain) + destroyPayload(deinit) + inject(move).
private enum Wrap: Equatable {
    case empty
    case text(String)
}

@Test
func enumNewtypeManagedPayloadRoundTrips() throws {
    let eSchema = Schema(id: SchemaId(1), kind: .enumeration(name: "Wrap", variants: [
        Variant(name: "Empty", index: 0, payload: .unit),
        Variant(name: "Text", index: 1, payload: .newtype(.concrete(primitiveId(.string)))),
    ]))
    let reg = Registry([eSchema])

    let strDesc = Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: .string))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: Layout(size: MemoryLayout<Wrap>.size, align: MemoryLayout<Wrap>.alignment),
        access: .enumeration(EnumAccess(
            tag: { ptr in
                switch ptr.assumingMemoryBound(to: Wrap.self).pointee {
                case .empty: return 0
                case .text: return 1
                }
            },
            projectPayload: { value, _, scratch in
                switch value.assumingMemoryBound(to: Wrap.self).pointee {
                case .empty: break
                case .text(let s): scratch.assumingMemoryBound(to: String.self).initialize(to: s)
                }
            },
            destroyPayload: { scratch, localIndex in
                if localIndex == 1 { scratch.assumingMemoryBound(to: String.self).deinitialize(count: 1) }
            },
            inject: { slot, localIndex, scratch in
                let w: Wrap = localIndex == 1 ? .text(scratch.assumingMemoryBound(to: String.self).move()) : .empty
                slot.assumingMemoryBound(to: Wrap.self).initialize(to: w)
            },
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 1, payloadFields: [FieldAccess(offset: 0, descriptor: strDesc)],
                              payloadLayout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment)),
            ]
        ))
    )
    let program = try lowerTyped(desc, reg)

    for value in [Wrap.text("héllo"), .empty, .text("")] {
        let typedBytes = withUnsafeBytes(of: value) { encodeWith(program, $0.baseAddress!) }
        let oracle: Value = {
            switch value {
            case .empty: return .object([.init(key: "Empty", value: .null)])
            case .text(let s): return .object([.init(key: "Text", value: .string(s))])
            }
        }()
        #expect(typedBytes == (try encode(oracle, SchemaId(1), reg)), "enum<string> \(value): bytes diverge")

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<Wrap>.size, alignment: MemoryLayout<Wrap>.alignment)
        defer { raw.deallocate() }
        try decodeInto(program, typedBytes, raw)
        #expect(raw.assumingMemoryBound(to: Wrap.self).move() == value, "enum<string> \(value): decode mismatch")
    }
}
