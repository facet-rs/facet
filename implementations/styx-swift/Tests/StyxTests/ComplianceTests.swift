import Foundation
import XCTest

@testable import Styx

final class ComplianceTests: XCTestCase {

    func testCompliance() throws {
        let corpusPath = try findCorpusPath()
        let styxCLI = try findStyxCLI()

        let files = try collectStyxFiles(in: corpusPath)

        for file in files.sorted() {
            let relPath = file.replacingOccurrences(of: corpusPath + "/", with: "")
            try compareOutput(file: file, relPath: relPath, styxCLI: styxCLI)
        }
    }

    private func findCorpusPath() throws -> String {
        let candidates = [
            "../../compliance/corpus",
            "../../../compliance/corpus",
            "../../../../compliance/corpus",
        ]

        let fileManager = FileManager.default
        let currentDir = fileManager.currentDirectoryPath

        for candidate in candidates {
            let path = (currentDir as NSString).appendingPathComponent(candidate)
            let standardized = (path as NSString).standardizingPath
            var isDir: ObjCBool = false
            if fileManager.fileExists(atPath: standardized, isDirectory: &isDir) && isDir.boolValue
            {
                return standardized
            }
        }

        throw XCTSkip("Could not find compliance corpus directory")
    }

    private func findStyxCLI() throws -> String {
        let candidates = [
            "../../target/debug/styx",
            "../../../target/debug/styx",
            "../../../../target/debug/styx",
            "../../target/release/styx",
            "../../../target/release/styx",
            "../../../../target/release/styx",
        ]

        let fileManager = FileManager.default
        let currentDir = fileManager.currentDirectoryPath

        for candidate in candidates {
            let path = (currentDir as NSString).appendingPathComponent(candidate)
            let standardized = (path as NSString).standardizingPath
            if fileManager.fileExists(atPath: standardized) {
                return standardized
            }
        }

        // Try PATH
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        process.arguments = ["styx"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        try? process.run()
        process.waitUntilExit()

        if process.terminationStatus == 0 {
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            if let path = String(data: data, encoding: .utf8)?.trimmingCharacters(
                in: .whitespacesAndNewlines), !path.isEmpty
            {
                return path
            }
        }

        throw XCTSkip("styx-cli not found - run 'cargo build' first")
    }

    private func collectStyxFiles(in directory: String) throws -> [String] {
        let fileManager = FileManager.default
        let enumerator = fileManager.enumerator(atPath: directory)
        var files: [String] = []

        while let file = enumerator?.nextObject() as? String {
            if file.hasSuffix(".styx") {
                files.append((directory as NSString).appendingPathComponent(file))
            }
        }

        return files
    }

    private func compareOutput(file: String, relPath: String, styxCLI: String) throws {
        let content = try String(contentsOfFile: file, encoding: .utf8)

        let swiftOutput = getSwiftOutput(content: content)
        let rustOutput = try getRustOutput(file: file, styxCLI: styxCLI)

        let swiftNorm = normalizeOutput(swiftOutput)
        let rustNorm = normalizeOutput(rustOutput)

        XCTAssertEqual(
            swiftNorm, rustNorm,
            """
            Mismatch in \(relPath)
            \(annotateErrorDiff(source: content, swiftOutput: swiftOutput, rustOutput: rustOutput))
            --- Swift output ---
            \(swiftOutput)
            --- Rust output ---
            \(rustOutput)
            """)
    }

    private func getSwiftOutput(content: String) -> String {
        var parser = Parser(source: content)
        do {
            let doc = try parser.parse()
            return doc.toSexp()
        } catch let error as ParseError {
            return error.toSexp()
        } catch {
            return "(error [-1, -1] \"parse error: \(escapeString(error.localizedDescription))\")"
        }
    }

    private func getRustOutput(file: String, styxCLI: String) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: styxCLI)
        process.arguments = ["tree", "--format", "sexp", file]

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        try process.run()
        process.waitUntilExit()

        let stdoutData = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
        let stderrData = stderrPipe.fileHandleForReading.readDataToEndOfFile()

        if process.terminationStatus != 0 {
            if let stderr = String(data: stderrData, encoding: .utf8), !stderr.isEmpty {
                return extractErrorFromStderr(stderr)
            }
            throw NSError(
                domain: "ComplianceTests", code: Int(process.terminationStatus),
                userInfo: [NSLocalizedDescriptionKey: "styx-cli failed"])
        }

        return String(data: stdoutData, encoding: .utf8) ?? ""
    }

    private func extractErrorFromStderr(_ stderr: String) -> String {
        // Parse error messages like "error: parse error at 9-10: expected a value"
        let pattern = #"parse error at (\d+)-(\d+): (.+)"#
        if let regex = try? NSRegularExpression(pattern: pattern),
            let match = regex.firstMatch(
                in: stderr, range: NSRange(stderr.startIndex..., in: stderr))
        {
            if let startRange = Range(match.range(at: 1), in: stderr),
                let endRange = Range(match.range(at: 2), in: stderr),
                let msgRange = Range(match.range(at: 3), in: stderr)
            {
                let start = String(stderr[startRange])
                let end = String(stderr[endRange])
                let msg = String(stderr[msgRange]).trimmingCharacters(in: .whitespacesAndNewlines)
                return
                    "(error [\(start), \(end)] \"parse error at \(start)-\(end): \(escapeString(msg))\")"
            }
        }
        return
            "(error [-1, -1] \"\(escapeString(stderr.trimmingCharacters(in: .whitespacesAndNewlines)))\")"
    }

    private func normalizeOutput(_ output: String) -> String {
        output
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.hasPrefix("; file:") && !$0.isEmpty }
            .joined(separator: "\n")
    }

    private func escapeString(_ s: String) -> String {
        s.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
            .replacingOccurrences(of: "\n", with: "\\n")
            .replacingOccurrences(of: "\r", with: "\\r")
            .replacingOccurrences(of: "\t", with: "\\t")
    }

    private func annotateErrorDiff(source: String, swiftOutput: String, rustOutput: String)
        -> String
    {
        let swiftSpan = parseErrorSpan(swiftOutput)
        let rustSpan = parseErrorSpan(rustOutput)

        if swiftSpan == nil && rustSpan == nil {
            return ""
        }

        var result = "\n"

        if let (start, end, msg) = rustSpan {
            result += "Expected error:\n"
            result += annotateSpan(source: source, start: start, end: end, msg: msg)
            result += "\n"
        } else {
            result += "Expected: no error\n\n"
        }

        if let (start, end, msg) = swiftSpan {
            result += "Got error:\n"
            result += annotateSpan(source: source, start: start, end: end, msg: msg)
        } else {
            result += "Got: no error\n"
        }

        return result
    }

    private func parseErrorSpan(_ output: String) -> (Int, Int, String)? {
        let pattern = #"\(error \[(\d+), (\d+)\] "([^"]*)""#
        guard let regex = try? NSRegularExpression(pattern: pattern),
            let match = regex.firstMatch(
                in: output, range: NSRange(output.startIndex..., in: output)),
            let startRange = Range(match.range(at: 1), in: output),
            let endRange = Range(match.range(at: 2), in: output),
            let msgRange = Range(match.range(at: 3), in: output),
            let start = Int(output[startRange]),
            let end = Int(output[endRange])
        else {
            return nil
        }
        return (start, end, String(output[msgRange]))
    }

    private func annotateSpan(source: String, start: Int, end: Int, msg: String) -> String {
        guard start >= 0, end >= 0, start <= source.utf8.count else {
            return "  [invalid span \(start)-\(end)]\n"
        }
        let effectiveEnd = min(end, source.utf8.count)

        // Find all lines that overlap with the span
        struct LineInfo {
            let text: String
            let lineStart: Int
            let lineEnd: Int
        }
        var lines: [LineInfo] = []
        var pos = 0
        for lineText in source.split(separator: "\n", omittingEmptySubsequences: false).map({
            String($0)
        }) {
            let lineStart = pos
            let lineEnd = pos + lineText.utf8.count
            // Check if this line overlaps with [start, end)
            if lineEnd >= start && lineStart < effectiveEnd {
                lines.append(LineInfo(text: lineText, lineStart: lineStart, lineEnd: lineEnd))
            }
            pos = lineEnd + 1  // +1 for the newline
            if lineStart >= effectiveEnd {
                break
            }
        }

        if lines.isEmpty {
            return "  [span \(start)-\(end) not found]\n"
        }

        var result = ""
        for li in lines {
            result += "  \(li.text)\n"
            // Calculate caret positions for this line
            let caretStart = max(start, li.lineStart) - li.lineStart
            let caretEnd = min(effectiveEnd, li.lineEnd) - li.lineStart
            var width = caretEnd - caretStart
            if width < 1 { width = 1 }
            let spaces = String(repeating: " ", count: caretStart)
            let carets = String(repeating: "^", count: width)
            result += "  \(spaces)\(carets)\n"
        }
        result += "  \(msg) (\(start)-\(end))\n"
        return result
    }
}
