import XCTest
import Foundation
@testable import Rapace
@testable import ConformanceRunner

/// Test case from the conformance harness
struct ConformanceTestCase: Decodable {
    let name: String
    let rules: [String]
}

/// Conformance tests for Swift Rapace implementation.
///
/// These tests run the Swift implementation against the
/// `rapace-conformance` reference peer to validate spec compliance.
final class ConformanceTests: XCTestCase {

    /// Path to the conformance binary
    static var conformanceBinary: String {
        let workspaceRoot = findWorkspaceRoot()
        let debugPath = workspaceRoot.appendingPathComponent("target/debug/rapace-conformance").path
        let releasePath = workspaceRoot.appendingPathComponent("target/release/rapace-conformance").path
        
        if FileManager.default.fileExists(atPath: debugPath) {
            return debugPath
        } else if FileManager.default.fileExists(atPath: releasePath) {
            return releasePath
        }
        return "rapace-conformance"
    }

    /// Find workspace root (where Cargo.toml is)
    static func findWorkspaceRoot() -> URL {
        var dir = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<10 {
            let cargoToml = dir.appendingPathComponent("Cargo.toml")
            if FileManager.default.fileExists(atPath: cargoToml.path) {
                return dir
            }
            let parent = dir.deletingLastPathComponent()
            if parent == dir { break }
            dir = parent
        }
        return URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    }

    /// Get all test cases from the conformance harness
    static func getTestCases() -> [ConformanceTestCase] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: conformanceBinary)
        process.arguments = ["--list", "--format", "json"]
        
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        
        do {
            try process.run()
            process.waitUntilExit()
            
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            return try JSONDecoder().decode([ConformanceTestCase].self, from: data)
        } catch {
            print("Failed to get test cases: \(error)")
            return []
        }
    }

    /// Run a single conformance test case
    func runConformanceTestCase(_ testCase: ConformanceTestCase) {
        let runner = ConformanceRunner()
        
        do {
            try runner.start(testCase: testCase.name, binary: Self.conformanceBinary)
            
            // For now, just try to do a handshake for interactive tests
            // This will fail for most tests but shows they're being exercised
            if testCase.name.hasPrefix("handshake.") {
                try runner.doHandshakeAsInitiator()
            }
            
            let result = runner.finish()
            
            if !result.passed {
                XCTFail("\(testCase.name) failed with exit code \(result.exitCode)")
            }
        } catch {
            XCTFail("\(testCase.name) failed: \(error)")
        }
    }

    // MARK: - All Conformance Tests
    
    /// This test runs ALL conformance test cases from the harness
    func testAllConformanceTests() throws {
        let testCases = Self.getTestCases()
        
        guard !testCases.isEmpty else {
            throw XCTSkip("rapace-conformance binary not found or returned no tests")
        }
        
        var failures: [(name: String, error: String)] = []
        
        for testCase in testCases {
            let runner = ConformanceRunner()
            
            do {
                try runner.start(testCase: testCase.name, binary: Self.conformanceBinary)
                
                // Try to do handshake for tests that need it
                if testCase.name.hasPrefix("handshake.") ||
                   testCase.name.hasPrefix("call.") ||
                   testCase.name.hasPrefix("channel.") ||
                   testCase.name.hasPrefix("control.") ||
                   testCase.name.hasPrefix("cancel.") {
                    do {
                        try runner.doHandshakeAsInitiator()
                    } catch {
                        // Some tests may not need handshake or may fail it intentionally
                    }
                }
                
                let result = runner.finish()
                
                if !result.passed {
                    failures.append((testCase.name, "exit code \(result.exitCode)"))
                }
            } catch {
                failures.append((testCase.name, "\(error)"))
            }
        }
        
        if !failures.isEmpty {
            let failureMessages = failures.map { "  - \($0.name): \($0.error)" }.joined(separator: "\n")
            XCTFail("\(failures.count)/\(testCases.count) conformance tests failed:\n\(failureMessages)")
        }
    }
}
