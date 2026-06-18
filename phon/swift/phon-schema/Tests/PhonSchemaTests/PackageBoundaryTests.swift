import Foundation
import Testing

@testable import PhonSchema

private func packageManifest() throws -> String {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { url.deleteLastPathComponent() }
    return try String(contentsOf: url.appendingPathComponent("Package.swift"), encoding: .utf8)
}

private func targetDependencies(_ target: String, in manifest: String) -> Set<String> {
    let lines = manifest.components(separatedBy: .newlines)
    for (index, line) in lines.enumerated() where line.contains("name: \"\(target)\"") {
        guard index > 0, lines[index - 1].contains(".target(") else { continue }
        for depLine in lines[index..<min(lines.count, index + 8)] where depLine.contains("dependencies:") {
            return Set(quotedValues(in: depLine).filter { $0 != target })
        }
        return []
    }
    return []
}

private func quotedValues(in line: String) -> [String] {
    var values: [String] = []
    var rest = line[...]
    while let start = rest.firstIndex(of: "\"") {
        let afterStart = rest[rest.index(after: start)...]
        guard let end = afterStart.firstIndex(of: "\"") else { break }
        values.append(String(afterStart[..<end]))
        rest = afterStart[afterStart.index(after: end)...]
    }
    return values
}

// r[verify crates.concern-separation]
// r[verify crates.engine-is-binding-free]
// r[verify descriptors.separate-implementations]
@Test
func swiftPackageKeepsSchemaIrEngineJitAndBindingSplit() throws {
    let manifest = try packageManifest()

    let schema = targetDependencies("PhonSchema", in: manifest)
    #expect(schema == ["CBlake3"], "PhonSchema dependencies changed: \(schema)")

    let ir = targetDependencies("PhonIR", in: manifest)
    #expect(ir == ["PhonSchema"], "PhonIR dependencies changed: \(ir)")

    let engine = targetDependencies("PhonEngine", in: manifest)
    #expect(engine == ["PhonSchema", "PhonIR"], "PhonEngine dependencies changed: \(engine)")
    #expect(!engine.contains("Phon"))
    #expect(!engine.contains("PhonJIT"))

    let jit = targetDependencies("PhonJIT", in: manifest)
    #expect(jit == ["CPhonJITStencils", "PhonEngine", "PhonIR", "PhonSchema"], "PhonJIT dependencies changed: \(jit)")
    #expect(!jit.contains("Phon"))

    let frontDoor = targetDependencies("Phon", in: manifest)
    #expect(frontDoor == ["PhonSchema", "PhonEngine"], "Phon dependencies changed: \(frontDoor)")
}
