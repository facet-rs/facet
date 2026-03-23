import Foundation

public final class SessionRegistry: @unchecked Sendable {
    private let lock = NSLock()
    private var sessions: [String: SessionHandle] = [:]

    public init() {}

    public func get(_ key: [UInt8]) -> SessionHandle? {
        lock.lock()
        defer { lock.unlock() }
        return sessions[sessionResumeRegistryKey(key)]
    }

    public func insert(_ key: [UInt8], handle: SessionHandle) {
        lock.lock()
        defer { lock.unlock() }
        sessions[sessionResumeRegistryKey(key)] = handle
    }

    public func remove(_ key: [UInt8]) {
        lock.lock()
        defer { lock.unlock() }
        sessions.removeValue(forKey: sessionResumeRegistryKey(key))
    }
}

private func sessionResumeRegistryKey(_ key: [UInt8]) -> String {
    key.map { String(format: "%02x", $0) }.joined()
}
