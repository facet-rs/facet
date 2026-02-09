import Foundation

public struct ShmTransportDiagnosticsSnapshot: Sendable {
    public let id: UUID
    public let peerId: UInt8
    public let maxPayloadSize: UInt32
    public let initialCredit: UInt32
    public let maxFrameSize: Int
    public let closed: Bool
    public let hostGoodbye: Bool
    public let timestamp: Date
}

public enum ShmDiagnosticsRegistry {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var enabled = false
    nonisolated(unsafe) private static var providers: [UUID: () -> ShmTransportDiagnosticsSnapshot] = [:]

    public static func setEnabled(_ value: Bool) {
        lock.lock()
        enabled = value
        lock.unlock()
    }

    public static func isEnabled() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return enabled
    }

    static func register(id: UUID, provider: @escaping () -> ShmTransportDiagnosticsSnapshot) {
        lock.lock()
        defer { lock.unlock() }
        if enabled {
            providers[id] = provider
        }
    }

    static func unregister(id: UUID) {
        lock.lock()
        defer { lock.unlock() }
        providers.removeValue(forKey: id)
    }

    public static func dumpAllState() -> String {
        let snapshots: [ShmTransportDiagnosticsSnapshot] = lock.withLock {
            providers.values.map { $0() }
        }

        if snapshots.isEmpty {
            return "(no swift shm transports registered)\n"
        }

        var out = "=== swift shm transports ===\n"
        for snap in snapshots.sorted(by: { $0.id.uuidString < $1.id.uuidString }) {
            out +=
                "id=\(snap.id.uuidString) peer=\(snap.peerId) "
                + "max_payload=\(snap.maxPayloadSize) initial_credit=\(snap.initialCredit) "
                + "max_frame=\(snap.maxFrameSize) closed=\(snap.closed) "
                + "host_goodbye=\(snap.hostGoodbye)\n"
        }
        return out
    }
}

private extension NSLock {
    func withLock<T>(_ body: () -> T) -> T {
        lock()
        defer { unlock() }
        return body()
    }
}
