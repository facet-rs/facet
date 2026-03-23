public protocol Link: Sendable {
    func sendFrame(_ bytes: [UInt8]) async throws
    func recvFrame() async throws -> [UInt8]?
    func setMaxFrameSize(_ size: Int) async throws
    func close() async throws
}

public extension Link {
    func sendRawPrologue(_ bytes: [UInt8]) async throws {
        try await sendFrame(bytes)
    }

    func recvRawPrologue() async throws -> [UInt8]? {
        try await recvFrame()
    }
}

final class FrameLimit: @unchecked Sendable {
    var maxFrameBytes: Int

    init(_ maxFrameBytes: Int) {
        self.maxFrameBytes = maxFrameBytes
    }
}
