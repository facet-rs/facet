import XCTest
import Foundation

/// Spec coverage tests for Swift.
///
/// This test suite generates one test per spec rule. Each test fails unless
/// the rule is covered by either:
/// - A conformance test (from `rapace-conformance --list --format json`)
/// - An `[impl ...]` annotation in implementation code
final class CoverageTests: XCTestCase {

    struct RulesManifest: Decodable {
        let rules: [String: RuleInfo]
    }

    struct RuleInfo: Decodable {
        let url: String
    }

    struct TestCase: Decodable {
        let name: String
        let rules: [String]
    }

    /// Find workspace root (where Cargo.toml is)
    static func findWorkspaceRoot() -> URL {
        var dir = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<100 {
            let cargoToml = dir.appendingPathComponent("Cargo.toml")
            if FileManager.default.fileExists(atPath: cargoToml.path) {
                return dir
            }
            let parent = dir.deletingLastPathComponent()
            if parent == dir { break }
            dir = parent
        }
        fatalError("Could not find workspace root")
    }

    /// Get all spec rules from _rules.json
    static func getSpecRules(workspaceRoot: URL) -> [String] {
        let rulesPath = workspaceRoot
            .appendingPathComponent("docs")
            .appendingPathComponent("public")
            .appendingPathComponent("_rules.json")

        guard let data = try? Data(contentsOf: rulesPath),
              let manifest = try? JSONDecoder().decode(RulesManifest.self, from: data) else {
            print("Warning: _rules.json not found at \(rulesPath.path). Run 'tracey rules -o docs/public/_rules.json docs/content/spec/**/*.md' first.")
            return []
        }

        return Array(manifest.rules.keys).sorted()
    }

    /// Get covered rules from conformance harness
    static func getRulesFromConformanceHarness(workspaceRoot: URL) -> Set<String> {
        var covered = Set<String>()

        let task = Process()
        task.currentDirectoryURL = workspaceRoot
        task.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        task.arguments = ["cargo", "run", "-p", "rapace-conformance", "--quiet", "--", "--list", "--format", "json"]

        let pipe = Pipe()
        task.standardOutput = pipe
        task.standardError = FileHandle.nullDevice

        do {
            try task.run()
            task.waitUntilExit()

            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            if let tests = try? JSONDecoder().decode([TestCase].self, from: data) {
                for testCase in tests {
                    for rule in testCase.rules {
                        covered.insert(rule)
                    }
                }
            }
        } catch {
            print("Warning: Could not run conformance harness: \(error)")
        }

        return covered
    }

    /// Recursively scan for [impl ...] annotations in Swift files
    static func scanForImplAnnotations(dir: URL, covered: inout Set<String>) {
        guard let implPattern = try? NSRegularExpression(pattern: #"\[impl ([a-z][a-z0-9._-]+)\]"#) else {
            return
        }

        guard let enumerator = FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil) else {
            return
        }

        for case let fileURL as URL in enumerator {
            let path = fileURL.path

            // Skip hidden, build artifacts
            if path.contains("/.") || path.contains("/Build/") || path.contains("/.build/") {
                continue
            }

            guard fileURL.pathExtension == "swift" else { continue }

            guard let content = try? String(contentsOf: fileURL, encoding: .utf8) else { continue }

            let range = NSRange(content.startIndex..., in: content)
            let matches = implPattern.matches(in: content, range: range)

            for match in matches {
                if let ruleRange = Range(match.range(at: 1), in: content) {
                    covered.insert(String(content[ruleRange]))
                }
            }
        }
    }

    /// Get all covered rules
    static func getCoveredRules(workspaceRoot: URL) -> Set<String> {
        var covered = Set<String>()

        // 1. Get rules from conformance harness
        let fromHarness = getRulesFromConformanceHarness(workspaceRoot: workspaceRoot)
        covered.formUnion(fromHarness)

        // 2. Scan Swift implementation for [impl ...] annotations
        let swiftDir = workspaceRoot.appendingPathComponent("swift").appendingPathComponent("Sources")
        scanForImplAnnotations(dir: swiftDir, covered: &covered)

        return covered
    }

    // Static data loaded once
    static let workspaceRoot = findWorkspaceRoot()
    static let specRules = getSpecRules(workspaceRoot: workspaceRoot)
    static let coveredRules = getCoveredRules(workspaceRoot: workspaceRoot)

    /// Test that all spec rules are covered
    func testAllRulesCovered() throws {
        var failures: [String] = []

        for rule in Self.specRules {
            if !Self.coveredRules.contains(rule) {
                failures.append(rule)
            }
        }

        if !failures.isEmpty {
            XCTFail("""
                \(failures.count) rules have no conformance test or [impl ...] annotation:
                \(failures.map { "  - \($0)" }.joined(separator: "\n"))
                """)
        }
    }
}
