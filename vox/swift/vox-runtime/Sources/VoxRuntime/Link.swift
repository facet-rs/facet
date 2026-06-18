// r[impl link]
// r[impl link.message]
public protocol Link: Sendable {
    // r[impl link.tx.send]
    func sendFrame(_ bytes: [UInt8]) async throws
    // r[impl link.rx.recv]
    // r[impl link.rx.eof]
    // r[impl link.rx.error]
    func recvFrame() async throws -> [UInt8]?
    // r[impl link.tx.alloc.limits]
    func setMaxFrameSize(_ size: Int) async throws
    // r[impl link.tx.close]
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
