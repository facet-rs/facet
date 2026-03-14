import Foundation

private let debugEnabled = ProcessInfo.processInfo.environment["ROAM_DEBUG"] != nil

func debugLog(_ message: String) {
    if debugEnabled {
        let pid = ProcessInfo.processInfo.processIdentifier
        let line = "[\(pid)] DEBUG: \(message)"
        NSLog("%@", line)
    }
}

func warnLog(_ message: String) {
    let pid = ProcessInfo.processInfo.processIdentifier
    let line = "[\(pid)] WARN: \(message)"
    NSLog("%@", line)
}
