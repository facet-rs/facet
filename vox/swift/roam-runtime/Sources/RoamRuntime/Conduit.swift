import Foundation

public protocol Conduit: Sendable {
    func send(_ message: MessageV7) async throws
    func recv() async throws -> MessageV7?
    func setMaxFrameSize(_ size: Int) async throws
    func close() async throws
}

public final class BareConduit: Conduit, @unchecked Sendable {
    public let link: any Link

    public init(link: any Link) {
        self.link = link
    }

    public func send(_ message: MessageV7) async throws {
        try await link.sendFrame(message.encode())
    }

    public func recv() async throws -> MessageV7? {
        guard let bytes = try await link.recvFrame() else {
            return nil
        }
        return try MessageV7.decode(from: Data(bytes))
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        try await link.setMaxFrameSize(size)
    }

    public func close() async throws {
        try await link.close()
    }
}

public extension Link {
    func bareConduit() -> BareConduit {
        BareConduit(link: self)
    }
}
