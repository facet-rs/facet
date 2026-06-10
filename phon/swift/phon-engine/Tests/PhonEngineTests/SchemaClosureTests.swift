// Schema-closure parsing + registry merge — the schema-exchange machinery the
// vox runtime and codegen use to fold a peer's advertised writer schema onto the
// local registry. Mirrors TypeScript's vox-wire/codec.ts framing.

import Testing

@testable import PhonEngine
import PhonSchema

// r[verify schema-identity.closure]
// r[verify validate.bundles]
@Test
func parseSchemaClosureRoundTripsAndMerges() throws {
    // Two mutually-referential schemas, resolved to real ids.
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "P", fields: [
            Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "child", schema: .concrete(SchemaId(2)), required: true),
        ])),
        Schema(id: SchemaId(2), kind: .structure(name: "C", fields: [
            Field(name: "v", schema: .concrete(primitiveId(.bool)), required: true),
        ])),
    ])
    let root = batch[0].id

    // Frame a closure blob: [u64 root][u32 count] then [u32 len][bytes] per schema.
    var blob = ByteSink()
    blob.writeU64(root.raw)
    blob.writeU32(UInt32(batch.count))
    for s in batch {
        let body = schemaToBytes(s)
        blob.writeU32(UInt32(body.count))
        body.withUnsafeBytes { blob.put($0) }
    }

    let (parsedRoot, parsedSchemas, auxiliaryRoots) = try parseSchemaClosure(blob.bytes)
    #expect(parsedRoot == root)
    #expect(Set(parsedSchemas) == Set(batch), "parsed closure schemas differ")
    #expect(auxiliaryRoots.isEmpty)

    // Merge onto an empty registry and confirm the root is now resolvable: encode
    // a value of P against it.
    let reg = try Registry([]).withValidating(parsedSchemas)
    let value: Value = .object([
        .init(key: "x", value: .number(.canonical(unsigned: 7))),
        .init(key: "child", value: .object([.init(key: "v", value: .bool(true))])),
    ])
    let bytes = try encode(value, root, reg)
    #expect(try decode(bytes, root, reg) == value, "round-trip through merged registry failed")
}

// r[verify validate.bundles]
@Test
func registryValidatingAcceptsResolvedBundles() throws {
    let point = Schema(id: SchemaId(1), kind: .structure(name: "Point", fields: [
        Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
    ]))
    _ = try Registry(validating: resolveIds([point]))
}

// r[verify validate.bundles]
@Test
func registryValidatingRejectsStaleSchemaIds() {
    let unit = Schema(id: SchemaId(1), kind: .structure(name: "UnitLike", fields: []))
    var resolved = resolveIds([unit])
    resolved[0].id = SchemaId(resolved[0].id.raw ^ 1)

    #expect(throws: CompactError.self) {
        _ = try Registry(validating: resolved)
    }
}

// r[verify validate.bundles]
// r[verify schema-identity.unknown-is-error]
@Test
func registryValidatingRejectsIncompleteSchemaClosures() {
    let holder = Schema(id: SchemaId(1), kind: .structure(name: "Holder", fields: [
        Field(
            name: "missing",
            schema: .concrete(SchemaId(0xFEED_FACE_CAFE_BEEF)),
            required: true
        ),
    ]))
    let resolved = resolveIds([holder])

    #expect(throws: CompactError.self) {
        _ = try Registry(validating: resolved)
    }
}

// r[verify validate.bundles]
// r[verify validate.dimensions]
@Test
func registryValidatingRejectsUnboundedZeroWireFixedArrays() {
    let array = Schema(id: SchemaId(1), kind: .array(
        element: .concrete(primitiveId(.unit)),
        dimensions: [UInt64(zstCountCap) + 1]
    ))
    let resolved = resolveIds([array])

    #expect(throws: CompactError.self) {
        _ = try Registry(validating: resolved)
    }
}

// r[verify type-system.generic-resolution]
@Test
func registryResolveSubstitutesGenericReferences() throws {
    let box = Schema(
        id: SchemaId(1),
        typeParams: ["T"],
        kind: .structure(name: "Box", fields: [
            Field(name: "value", schema: .variable(name: "T"), required: true),
        ])
    )
    let reg = Registry([box])

    let resolved = try resolve(
        reg,
        .concrete(id: SchemaId(1), args: [.concrete(primitiveId(.u32))])
    )

    guard case .composite(.structure(_, let fields)) = resolved else {
        Issue.record("generic Box did not resolve to a struct")
        return
    }
    #expect(fields[0].schema == .concrete(primitiveId(.u32)))
}

@Test
func parseSchemaClosureReadsAuxiliaryRoots() throws {
    var blob = ByteSink()
    blob.writeU64(1)
    blob.writeU32(0)
    blob.writeU32(1)
    blob.writeStr("channel.arg.0.tx.element")
    blob.writeU64(2)

    let parsed = try parseSchemaClosure(blob.bytes)
    #expect(parsed.root == SchemaId(1))
    #expect(parsed.schemas.isEmpty)
    #expect(parsed.auxiliaryRoots == [
        AuxiliaryRoot(role: "channel.arg.0.tx.element", root: SchemaId(2))
    ])
}

// r[verify compact.schema-driven]
// r[verify compact.alignment]
// r[verify decode.chained]
@Test
func compactValuesCanBeDecodedBackToBack() throws {
    let reg = Registry([])
    let firstRoot = primitiveId(.u8)
    let secondRoot = primitiveId(.bool)
    var bytes = try encode(.number(.canonical(unsigned: 7)), firstRoot, reg)
    bytes.append(contentsOf: try encode(.bool(true), secondRoot, reg))

    var reader = Reader(bytes)
    let first = try decodeRef(&reader, .concrete(firstRoot), reg, 0)
    let second = try decodeRef(&reader, .concrete(secondRoot), reg, 0)

    #expect(first == .number(.canonical(unsigned: 7)))
    #expect(second == .bool(true))
    #expect(reader.remaining == 0)
}

// r[verify decode.whole-message]
@Test
func compactDecodeRejectsTrailingBytes() throws {
    let reg = Registry([])
    var bytes = try encode(.bool(true), primitiveId(.bool), reg)
    bytes.append(0)

    #expect(throws: CompactError.self) {
        _ = try decode(bytes, primitiveId(.bool), reg)
    }
}
