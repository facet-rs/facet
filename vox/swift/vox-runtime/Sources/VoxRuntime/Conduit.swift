import Foundation

public protocol Conduit: Sendable {
    func send(_ message: Message) async throws
    func recv() async throws -> Message?
    func setMaxFrameSize(_ size: Int) async throws
    func close() async throws
}

/// A conduit over a raw `Link`: the envelope rides phon (`encodeMessage` /
/// `decodeMessage`). The decoder uses the peer's advertised Message schema
/// (from the handshake) against the local reader.
/// r[impl conduit]
/// r[impl conduit.bare]
public final class BareConduit: Conduit, @unchecked Sendable {
    public let link: any Link
    private let decode: MessageDecoder

    public init(link: any Link, peerMessageSchema: [UInt8]) {
        self.link = link
        self.decode = buildMessageDecoder(peerMessageSchema: peerMessageSchema)
    }

    public func send(_ message: Message) async throws {
        try await link.sendFrame(encodeMessage(message))
    }

    public func recv() async throws -> Message? {
        guard let bytes = try await link.recvFrame() else { return nil }
        return try decode(bytes)
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        try await link.setMaxFrameSize(size)
    }

    public func close() async throws {
        try await link.close()
    }
}

extension Link {
    public func bareConduit(peerMessageSchema: [UInt8]) -> BareConduit {
        BareConduit(link: self, peerMessageSchema: peerMessageSchema)
    }
}
