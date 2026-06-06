// Cross-implementation compat conformance: replay the Rust-generated corpus
// (conformance/compat/vectors.json) through the Swift engine.
//
// For each case we build the writer->reader plan and decode the writer bytes with
// BOTH the recursive planner and the lowered-IR interpreter (they must agree),
// re-encode the result through the reader schema, and assert the bytes equal
// Rust's reader-shaped bytes (the oracle). Error cases assert decode fails
// with the expected CompactError variant.

import Foundation
import Testing

@testable import PhonEngine
import PhonIR
import PhonSchema

private struct VectorFile: Decodable {
    let schemas: [String]
    let cases: [Case]
}

private struct Case: Decodable {
    let name: String
    let writerRoot: String
    let readerRoot: String
    let writerHex: String
    let readerHex: String?
    let errorKind: String?
}

/// The repo's `conformance/compat/vectors.json`, located from this file's path.
private func vectorsURL() -> URL {
    var url = URL(fileURLWithPath: #filePath)
    // .../phon/swift/phon-engine/Tests/PhonEngineTests/ConformanceTests.swift
    for _ in 0..<5 { url.deleteLastPathComponent() } // -> .../phon
    return url
        .appendingPathComponent("conformance")
        .appendingPathComponent("compat")
        .appendingPathComponent("vectors.json")
}

private func hexToBytes(_ hex: String) -> [UInt8] {
    var out: [UInt8] = []
    out.reserveCapacity(hex.count / 2)
    var it = hex.makeIterator()
    func nibble(_ c: Character) -> UInt8 { UInt8(String(c), radix: 16)! }
    while let hi = it.next(), let lo = it.next() {
        out.append(nibble(hi) << 4 | nibble(lo))
    }
    return out
}

private func bytesToHex(_ bytes: [UInt8]) -> String {
    bytes.map { String(format: "%02x", $0) }.joined()
}

private func schemaId(_ hex: String) -> SchemaId { SchemaId(UInt64(hex, radix: 16)!) }

private func loadCorpus() throws -> VectorFile {
    let data = try Data(contentsOf: vectorsURL())
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(VectorFile.self, from: data)
}

// r[verify compat.plan-first]
// r[verify compat.field-matching]
// r[verify compat.skip-writer-only]
// r[verify compat.reader-only-fields]
// r[verify compat.defaults-are-reader-side]
// r[verify compat.type-match]
// r[verify compat.enum]
// r[verify exec.interpreter-baseline]
@Test
func compatConformanceCorpus() throws {
    let corpus = try loadCorpus()
    let reg = Registry(try corpus.schemas.map { try schemaFromBytes(hexToBytes($0)) })

    #expect(corpus.cases.count == 30, "corpus case count changed")

    for c in corpus.cases {
        let writerRoot = schemaId(c.writerRoot)
        let readerRoot = schemaId(c.readerRoot)
        let writerBytes = hexToBytes(c.writerHex)

        if let errorKind = c.errorKind {
            // Both decode paths must reject, with the expected error.
            #expect(throws: CompactError.self, "\(c.name): expected \(errorKind), recursive path succeeded") {
                _ = try planDecode(writerBytes, writerRoot, readerRoot, reg)
            }
            #expect(throws: CompactError.self, "\(c.name): expected \(errorKind), IR path succeeded") {
                _ = try decodeViaIr(writerBytes, writerRoot, readerRoot, reg)
            }
            // The recorded error_kind must match the surfaced variant exactly.
            do {
                _ = try planDecode(writerBytes, writerRoot, readerRoot, reg)
            } catch let e as CompactError {
                #expect(errorKindName(e) == errorKind, "\(c.name): error kind \(errorKindName(e)) != \(errorKind)")
            }
            continue
        }

        // Recursive planner and IR interpreter both decode and must agree.
        let interpValue = try planDecode(writerBytes, writerRoot, readerRoot, reg)
        let irValue = try decodeViaIr(writerBytes, writerRoot, readerRoot, reg)
        #expect(interpValue == irValue, "\(c.name): IR interpreter disagreed with recursive exec")

        // Re-encoding through the reader schema must reproduce Rust's reader-shaped
        // reader bytes.
        let reencoded = bytesToHex(try encode(interpValue, readerRoot, reg))
        #expect(reencoded == c.readerHex, "\(c.name): reader bytes differ")
    }
}

// r[verify exec.jit-optional]
// r[verify exec.strict-recording]
@Test
func swiftEngineRecordsInterpreterOnlyJitFallback() {
    let report = recordJitFallbacks(
        Lowered(program: [.scalar(offset: 0, size: 4, align: 4)])
    )
    #expect(report.decode == [
        JitFallbackRecord(
            path: "$",
            reason: "Swift PhonEngine currently uses the interpreter backend; no JIT backend is selected"
        )
    ])
    #expect(report.encode == [
        JitFallbackRecord(
            path: "$",
            reason: "Swift PhonEngine currently uses the interpreter backend; no JIT backend is selected"
        )
    ])
}

// r[verify compat.direction]
@Test
func compatDirectionReport() throws {
    func u32Field(_ name: String, required: Bool = true) -> Field {
        Field(name: name, schema: .concrete(primitiveId(.u32)), required: required)
    }
    let schemas = [
        Schema(id: SchemaId(1), kind: .structure(name: "P", fields: [u32Field("x")])),
        Schema(id: SchemaId(2), kind: .structure(name: "P", fields: [
            u32Field("x"), u32Field("y", required: false),
        ])),
        Schema(id: SchemaId(3), kind: .structure(name: "P", fields: [
            u32Field("x"), u32Field("y"),
        ])),
        Schema(id: SchemaId(4), kind: .structure(name: "P", fields: [
            u32Field("x"), u32Field("y"),
        ])),
        Schema(id: SchemaId(5), kind: .structure(name: "P", fields: [u32Field("x")])),
        Schema(id: SchemaId(6), kind: .structure(name: "P", fields: [
            Field(name: "x", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
    ]
    let reg = Registry(schemas)

    #expect(compatDirection(SchemaId(1), SchemaId(2), reg) == .bidirectional)
    #expect(compatDirection(SchemaId(1), SchemaId(3), reg) == .forward)
    #expect(compatDirection(SchemaId(4), SchemaId(5), reg) == .backward)
    #expect(compatDirection(SchemaId(1), SchemaId(6), reg) == .incompatible)
}
