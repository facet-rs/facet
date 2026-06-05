import Foundation
import Testing

@testable import PhonSchema

// The Swift side of the conformance oracle. Reads the committed corpus — the same
// `.phon` files Rust generates and TypeScript checks — and verifies, per schema
// case:
//  - round-trip: each committed file decodes to a schema and re-encodes to the
//    same bytes (this encoder and decoder match Rust's);
//  - identity: recomputing every id from the decoded batch (this BLAKE3 + this
//    canonical encoding) reproduces the id baked into the bytes.
//
// A divergent hash, an endianness slip, or an off-by-one in a length prefix fails
// here the moment it appears.

/// The repo's `conformance/` directory, located from this file's path.
private func conformanceDir() -> URL {
    var url = URL(fileURLWithPath: #filePath)
    // .../phon/swift/phon-schema/Tests/PhonSchemaTests/CorpusTests.swift
    for _ in 0..<5 { url.deleteLastPathComponent() } // -> .../phon
    return url.appendingPathComponent("conformance")
}

/// The `.phon` files in a case directory, sorted by name for stable iteration.
private func caseFiles(_ dir: URL) throws -> [URL] {
    try FileManager.default
        .contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
        .filter { $0.pathExtension == "phon" }
        .sorted { $0.lastPathComponent < $1.lastPathComponent }
}

private func collectRef(_ ref: SchemaRef, refs: inout Set<String>) {
    switch ref {
    case .concrete(_, let args):
        refs.insert("concrete")
        for arg in args { collectRef(arg, refs: &refs) }
    case .variable:
        refs.insert("var")
    }
}

private func collectField(_ field: Field, refs: inout Set<String>) {
    collectRef(field.schema, refs: &refs)
}

private func collectPayload(_ payload: VariantPayload, refs: inout Set<String>, payloads: inout Set<String>) {
    switch payload {
    case .unit:
        payloads.insert("unit")
    case .newtype(let ref):
        payloads.insert("newtype")
        collectRef(ref, refs: &refs)
    case .tuple(let refsInTuple):
        payloads.insert("tuple")
        for ref in refsInTuple { collectRef(ref, refs: &refs) }
    case .structure(let fields):
        payloads.insert("struct")
        for field in fields { collectField(field, refs: &refs) }
    }
}

private func collectKind(_ kind: SchemaKind, kinds: inout Set<String>, refs: inout Set<String>, payloads: inout Set<String>) {
    switch kind {
    case .primitive:
        kinds.insert("primitive")
    case .structure(_, let fields):
        kinds.insert("struct")
        for field in fields { collectField(field, refs: &refs) }
    case .enumeration(_, let variants):
        kinds.insert("enum")
        for variant in variants { collectPayload(variant.payload, refs: &refs, payloads: &payloads) }
    case .tuple(let elements):
        kinds.insert("tuple")
        for ref in elements { collectRef(ref, refs: &refs) }
    case .list(let element):
        kinds.insert("list")
        collectRef(element, refs: &refs)
    case .set(let element):
        kinds.insert("set")
        collectRef(element, refs: &refs)
    case .map(let key, let value):
        kinds.insert("map")
        collectRef(key, refs: &refs)
        collectRef(value, refs: &refs)
    case .array(let element, _):
        kinds.insert("array")
        collectRef(element, refs: &refs)
    case .tensor(let element, _):
        kinds.insert("tensor")
        collectRef(element, refs: &refs)
    case .option(let element):
        kinds.insert("option")
        collectRef(element, refs: &refs)
    case .channel(_, let element):
        kinds.insert("channel")
        collectRef(element, refs: &refs)
    case .dynamic:
        kinds.insert("dynamic")
    case .external(_, let metadata):
        kinds.insert("external")
        if let metadata { collectRef(metadata, refs: &refs) }
    }
}

private func collectValue(_ value: Value, sawExtended: inout Bool) {
    switch value {
    case .datetime, .uuid, .qname:
        sawExtended = true
    case .array(let values):
        for value in values { collectValue(value, sawExtended: &sawExtended) }
    case .object(let entries):
        for entry in entries { collectValue(entry.value, sawExtended: &sawExtended) }
    default:
        return
    }
}

// r[verify self-describing.bootstraps-schemas]
// r[verify self-describing.enum-payload]
// r[verify type-system.canonical-form]
// r[verify type-system.array]
// r[verify type-system.tensor]
// r[verify type-system.channel]
// r[verify type-system.dynamic]
// r[verify type-system.external]
// r[verify type-system.generics]
// r[verify type-system.variant-payloads]
// r[verify schema-identity.canonical-encoding]
// r[verify schema-identity.closure]
// r[verify schema-identity.computation]
// r[verify schema-identity.content-hash]
@Test
func schemaCorpusRoundTripsAndIsSelfConsistent() throws {
    let casesDir = conformanceDir().appendingPathComponent("cases")
    let caseDirs = try FileManager.default
        .contentsOfDirectory(at: casesDir, includingPropertiesForKeys: [.isDirectoryKey])
        .filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }
        .sorted { $0.lastPathComponent < $1.lastPathComponent }

    #expect(!caseDirs.isEmpty, "conformance/cases is empty")

    var checked = 0
    var kinds = Set<String>()
    var refs = Set<String>()
    var payloads = Set<String>()
    var sawGenericSchema = false
    for caseDir in caseDirs {
        let caseName = caseDir.lastPathComponent
        var decodedBatch: [Schema] = []

        for file in try caseFiles(caseDir) {
            let label = file.deletingPathExtension().lastPathComponent
            let committed = [UInt8](try Data(contentsOf: file))

            // round-trip: committed bytes decode to a schema and re-encode equal.
            let decoded = try schemaFromBytes(committed)
            #expect(
                schemaToBytes(decoded) == committed,
                "\(caseName)/\(label): re-encode differs from committed bytes"
            )
            decodedBatch.append(decoded)
            collectKind(decoded.kind, kinds: &kinds, refs: &refs, payloads: &payloads)
            sawGenericSchema = sawGenericSchema || !decoded.typeParams.isEmpty
        }

        // identity: recompute every id from the decoded batch and confirm it
        // matches the id baked into the bytes.
        let recomputed = resolveIds(decodedBatch)
        for (decoded, recomputed) in zip(decodedBatch, recomputed) {
            #expect(
                decoded.id == recomputed.id,
                "\(caseName): recomputed SchemaId \(recomputed.id) differs from stated \(decoded.id)"
            )
            checked += 1
        }
    }
    #expect(checked > 0, "corpus produced no checks")
    for kind in ["array", "tensor", "channel", "dynamic", "external", "enum", "struct", "list", "map", "set"] {
        #expect(kinds.contains(kind), "schema corpus did not exercise \(kind)")
    }
    for ref in ["concrete", "var"] {
        #expect(refs.contains(ref), "schema corpus did not exercise \(ref) refs")
    }
    for payload in ["unit", "newtype", "tuple", "struct"] {
        #expect(payloads.contains(payload), "schema corpus did not exercise \(payload) enum payloads")
    }
    #expect(sawGenericSchema, "schema corpus did not exercise generic schemas")
}

// r[verify value]
// r[verify value.extended-kinds]
// r[verify self-describing.tag-led]
// r[verify self-describing.no-extra-kinds]
@Test
func valueCorpusRoundTrips() throws {
    let valuesDir = conformanceDir().appendingPathComponent("values")
    let files = try FileManager.default
        .contentsOfDirectory(at: valuesDir, includingPropertiesForKeys: nil)
        .filter { $0.pathExtension == "phon" }
        .sorted { $0.lastPathComponent < $1.lastPathComponent }

    #expect(!files.isEmpty, "conformance/values is empty")

    var sawExtended = false
    for file in files {
        let name = file.deletingPathExtension().lastPathComponent
        let committed = [UInt8](try Data(contentsOf: file))

        // round-trip: committed bytes decode to a value and re-encode equal
        // (values carry no schema and no id, so this is the whole oracle).
        let decoded = try valueFromBytes(committed)
        #expect(
            valueToBytes(decoded) == committed,
            "\(name): re-encode differs from committed bytes"
        )
        collectValue(decoded, sawExtended: &sawExtended)
    }
    #expect(sawExtended, "value corpus did not exercise extended kinds")
}

// r[verify validate.tags]
// r[verify validate.text]
// r[verify validate.lengths]
// r[verify validate.dimensions]
// r[verify validate.depth]
// r[verify decode.whole-message]
// r[verify self-describing.no-extra-kinds]
@Test
func valueDecoderRejectsHostileSelfDescribingInputs() {
    #expect(throws: DecodeError.self) {
        _ = try valueFromBytes([0xFF])
    }

    #expect(throws: DecodeError.self) {
        _ = try valueFromBytes(valueToBytes(.bool(true)) + [0])
    }

    var badString = ByteSink()
    badString.writeU8(Tag.string)
    badString.writeU32(1)
    badString.writeU8(0xFF)
    #expect(throws: DecodeError.self) {
        _ = try valueFromBytes(badString.bytes)
    }

    var badChar = ByteSink()
    badChar.writeU8(Tag.char)
    badChar.writeU32(0xD800)
    #expect(throws: DecodeError.self) {
        _ = try valueFromBytes(badChar.bytes)
    }

    var oversizedList = ByteSink()
    oversizedList.writeU8(Tag.list)
    oversizedList.writeU32(UInt32(zstCountCap + 1))
    #expect(throws: DecodeError.self) {
        _ = try valueFromBytes(oversizedList.bytes)
    }

    var truncatedArray = ByteSink()
    truncatedArray.writeU8(Tag.array)
    truncatedArray.writeU32(1)
    truncatedArray.writeU64(1)
    #expect(throws: DecodeError.self) {
        _ = try valueFromBytes(truncatedArray.bytes)
    }

    let tooDeep = Array(repeating: Tag.optionSome, count: maxDepth + 2) + [Tag.optionNone]
    #expect(throws: DecodeError.self) {
        _ = try valueFromBytes(tooDeep)
    }
}
