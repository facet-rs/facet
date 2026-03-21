import Foundation
import OSLog

private let debugEnabled = ProcessInfo.processInfo.environment["ROAM_DEBUG"] != nil
private let traceCategories = parseTraceCategories()
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

func traceLog(_ category: TraceCategory, _ message: @autoclosure () -> String) {
    guard traceCategories.contains(category) else {
        return
    }
    let rendered = message()
    logger(category: category.rawValue).debug("\(rendered, privacy: .public)")
}

func debugLog(_ message: String) {
    if debugEnabled {
        logger(category: "debug").debug("\(message, privacy: .public)")
    }
}

func warnLog(_ message: String) {
    logger(category: "warn").warning("\(message, privacy: .public)")
}
