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

@Test
func schemaCorpusRoundTripsAndIsSelfConsistent() throws {
    let casesDir = conformanceDir().appendingPathComponent("cases")
    let caseDirs = try FileManager.default
        .contentsOfDirectory(at: casesDir, includingPropertiesForKeys: [.isDirectoryKey])
        .filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }
        .sorted { $0.lastPathComponent < $1.lastPathComponent }

    #expect(!caseDirs.isEmpty, "conformance/cases is empty")

    var checked = 0
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
}

@Test
func valueCorpusRoundTrips() throws {
    let valuesDir = conformanceDir().appendingPathComponent("values")
    let files = try FileManager.default
        .contentsOfDirectory(at: valuesDir, includingPropertiesForKeys: nil)
        .filter { $0.pathExtension == "phon" }
        .sorted { $0.lastPathComponent < $1.lastPathComponent }

    #expect(!files.isEmpty, "conformance/values is empty")

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
    }
}
