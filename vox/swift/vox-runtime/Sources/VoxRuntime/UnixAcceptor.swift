import Foundation
@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

/// A Unix domain socket server that accepts connections and returns them as `LinkAttachment`s.
///
/// The server binds lazily on first use (removing any stale socket file first),
/// and keeps accepting connections across multiple `openAttachment()` calls.
public final class UnixAcceptor: SessionConnector, Sendable {
    public let path: String
    public let transport: ConduitKind

    private let state: AcceptorState

    public init(path: String, transport: ConduitKind = .bare) {
        self.path = path
        self.transport = transport
        self.state = AcceptorState()
    }

    public func bare() -> UnixAcceptor {
        UnixAcceptor(path: path, transport: .bare)
    }

    public func stable() -> UnixAcceptor {
        UnixAcceptor(path: path, transport: .stable)
    }

    public func openAttachment() async throws -> LinkAttachment {
        let link = try await state.nextLink(path: path)
        return .fresh(link)
    }
}

private let unixDefaultMaxFrameBytes = 1024 * 1024

/// Manages the NIO server channel and streams accepted connections.
private final class AcceptorState: @unchecked Sendable {
    private var iterator: AsyncStream<NIOFrameLink>.Iterator?

    func nextLink(path: String) async throws -> NIOFrameLink {
        if iterator == nil {
            iterator = try await bind(path: path)
        }
        var iter = iterator!
        guard let link = await iter.next() else {
            throw TransportError.connectionClosed
        }
        iterator = iter
        return link
    }

    private func bind(path: String) async throws -> AsyncStream<NIOFrameLink>.Iterator {
        // Remove stale socket file if it exists.
        unlink(path)

        let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
        let (connStream, connContinuation) = AsyncStream<NIOFrameLink>.makeStream()

        let bootstrap = ServerBootstrap(group: group)
            .serverChannelOption(ChannelOptions.backlog, value: 16)
            .childChannelInitializer { channel in
                let frameLimit = FrameLimit(unixDefaultMaxFrameBytes)

                var rawContinuation: AsyncStream<Result<[UInt8], Error>>.Continuation!
                let rawStream = AsyncStream<Result<[UInt8], Error>> { c in
                    rawContinuation = c
                }
                let rawHandler = RawFrameStreamHandler(continuation: rawContinuation!)

                do {
                    try channel.pipeline.syncOperations.addHandler(
                        ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit))
                    )
                    try channel.pipeline.syncOperations.addHandler(rawHandler)

                    let link = NIOFrameLink(
                        channel: channel,
                        frameLimit: frameLimit,
                        inboundStream: rawStream
                    )
                    connContinuation.yield(link)
                    return channel.eventLoop.makeSucceededVoidFuture()
                } catch {
                    return channel.eventLoop.makeFailedFuture(error)
                }
            }

        let serverChannel = try await bootstrap.bind(unixDomainSocketPath: path).get()

        // Announce the socket path so the test harness can connect.
        let announcement = "LISTEN_ADDR=\(path)\n"
        FileHandle.standardOutput.write(Data(announcement.utf8))

        serverChannel.closeFuture.whenComplete { _ in
            connContinuation.finish()
        }

        return connStream.makeAsyncIterator()
    }
}
