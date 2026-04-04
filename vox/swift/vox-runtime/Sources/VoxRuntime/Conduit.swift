import Foundation

public protocol Conduit: Sendable {
    func send(_ message: Message) async throws
    func recv() async throws -> Message?
    func setMaxFrameSize(_ size: Int) async throws
    func close() async throws
}

public final class BareConduit: Conduit, @unchecked Sendable {
    public let link: any Link

    public init(link: any Link) {
        self.link = link
    }

    public func send(_ message: Message) async throws {
        try await link.sendFrame(message.encode())
    }

    public func recv() async throws -> Message? {
        guard let bytes = try await link.recvFrame() else {
            return nil
        }
        return try Message.decode(fromBytes: bytes)
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        try await link.setMaxFrameSize(size)
    }

    public func close() async throws {
        try await link.close()
    }
}

extension Link {
    public func bareConduit() -> BareConduit {
        BareConduit(link: self)
    }
}
