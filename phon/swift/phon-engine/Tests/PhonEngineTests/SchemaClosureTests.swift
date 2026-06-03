// Schema-closure parsing + registry merge — the schema-exchange machinery the
// vox runtime and codegen use to fold a peer's advertised writer schema onto the
// local registry. Mirrors TypeScript's vox-wire/codec.ts framing.

import Testing

@testable import PhonEngine
import PhonSchema

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
    let reg = Registry([]).with(parsedSchemas)
    let value: Value = .object([
        .init(key: "x", value: .number(.canonical(unsigned: 7))),
        .init(key: "child", value: .object([.init(key: "v", value: .bool(true))])),
    ])
    let bytes = try encode(value, root, reg)
    #expect(try decode(bytes, root, reg) == value, "round-trip through merged registry failed")
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
