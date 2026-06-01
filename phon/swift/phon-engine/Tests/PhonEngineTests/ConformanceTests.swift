// Cross-implementation compat conformance: replay the Rust-generated corpus
// (conformance/compat/vectors.json) through the Swift engine.
//
// For each case we build the writer->reader plan and decode the writer bytes with
// BOTH the recursive planner and the lowered-IR interpreter (they must agree),
// re-encode the result through the reader schema, and assert the bytes equal
// Rust's reconciled reader bytes (the oracle). Error cases assert decode fails
// with the expected CompactError variant.

import Foundation
import Testing

@testable import PhonEngine
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

@Test
func compatConformanceCorpus() throws {
    let corpus = try loadCorpus()
    let reg = Registry(try corpus.schemas.map { try schemaFromBytes(hexToBytes($0)) })

    #expect(corpus.cases.count == 26, "corpus case count drifted")

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

        // Re-encoding through the reader schema must reproduce Rust's reconciled
        // reader bytes.
        let reencoded = bytesToHex(try encode(interpValue, readerRoot, reg))
        #expect(reencoded == c.readerHex, "\(c.name): reconciled reader bytes differ")
    }
}
