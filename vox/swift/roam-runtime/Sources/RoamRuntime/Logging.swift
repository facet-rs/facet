import Foundation
import OSLog

private let debugEnabled = ProcessInfo.processInfo.environment["ROAM_DEBUG"] != nil
private let traceCategories = parseTraceCategories()
private let traceToStderr = ProcessInfo.processInfo.environment["ROAM_TRACE_STDERR"] != nil
private let traceFilePath = ProcessInfo.processInfo.environment["ROAM_TRACE_FILE"]
private let subsystem = "bearcove.roam.runtime"

enum TraceCategory: String, CaseIterable {
    case driver
    case resume
    case handshake
    case shm
}

private func parseTraceCategories() -> Set<TraceCategory> {
    guard let raw = ProcessInfo.processInfo.environment["ROAM_TRACE"] else {
        return []
    }
    let tokens = raw
        .split(separator: ",")
        .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
        .filter { !$0.isEmpty }
    if tokens.contains("1") || tokens.contains("all") || tokens.contains("*") {
        return Set(TraceCategory.allCases)
    }
    return Set(tokens.compactMap(TraceCategory.init(rawValue:)))
}

private func logger(category: String) -> Logger {
    Logger(subsystem: subsystem, category: category)
}

private func writeStderr(_ line: String) {
    guard let data = (line + "\n").data(using: .utf8) else {
        return
    }
    FileHandle.standardError.write(data)
}

private func writeTraceFile(_ line: String) {
    guard let traceFilePath,
        let data = (line + "\n").data(using: .utf8)
    else {
        return
    }
    let url = URL(fileURLWithPath: traceFilePath)
    if !FileManager.default.fileExists(atPath: traceFilePath) {
        FileManager.default.createFile(atPath: traceFilePath, contents: nil)
    }
    guard let handle = try? FileHandle(forWritingTo: url) else {
        return
    }
    defer { try? handle.close() }
    _ = try? handle.seekToEnd()
    try? handle.write(contentsOf: data)
}

func traceLog(_ category: TraceCategory, _ message: @autoclosure () -> String) {
    guard traceCategories.contains(category) else {
        return
    }
    let rendered = message()
    logger(category: category.rawValue).debug("\(rendered, privacy: .public)")
    if traceToStderr {
        writeStderr("[trace:\(category.rawValue)] \(rendered)")
    }
    writeTraceFile("[trace:\(category.rawValue)] \(rendered)")
}

func debugLog(_ message: String) {
    if debugEnabled {
        logger(category: "debug").debug("\(message, privacy: .public)")
        if traceToStderr {
            writeStderr("[debug] \(message)")
        }
        writeTraceFile("[debug] \(message)")
    }
}

func warnLog(_ message: String) {
    logger(category: "warn").warning("\(message, privacy: .public)")
    if traceToStderr {
        writeStderr("[warn] \(message)")
    }
    writeTraceFile("[warn] \(message)")
}
