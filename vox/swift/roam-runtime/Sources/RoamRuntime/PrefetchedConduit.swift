import Foundation

final class PrefetchedConduit: Conduit, @unchecked Sendable {
    private let base: any Conduit
    private let lock = NSLock()
    private var firstMessage: MessageV7?

    init(firstMessage: MessageV7, base: any Conduit) {
        self.base = base
        self.firstMessage = firstMessage
    }

    func send(_ message: MessageV7) async throws {
        try await base.send(message)
    }

    func recv() async throws -> MessageV7? {
        if let first = takeFirstMessage() {
            return first
        }
        return try await base.recv()
    }

    func setMaxFrameSize(_ size: Int) async throws {
        try await base.setMaxFrameSize(size)
    }

    func close() async throws {
        try await base.close()
    }

    private func takeFirstMessage() -> MessageV7? {
        lock.lock()
        defer { lock.unlock() }
        defer { firstMessage = nil }
        return firstMessage
    }
}
