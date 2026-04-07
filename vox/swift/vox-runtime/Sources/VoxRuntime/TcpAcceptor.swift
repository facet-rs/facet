import Foundation
@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

/// A TCP server that accepts connections and returns them as `LinkAttachment`s.
///
/// The server binds lazily on first use (optionally to an OS-assigned port when `port` is 0),
/// and keeps accepting connections across multiple `openAttachment()` calls.
public final class TcpAcceptor: SessionConnector, Sendable {
    public let host: String
    public let port: Int  // 0 = OS assigns port
    public let transport: ConduitKind

    private let state: TcpAcceptorState

    public init(host: String, port: Int = 0, transport: ConduitKind = .bare) {
        self.host = host
        self.port = port
        self.transport = transport
        self.state = TcpAcceptorState()
    }

    public func bare() -> TcpAcceptor {
        TcpAcceptor(host: host, port: port, transport: .bare)
    }

    public func stable() -> TcpAcceptor {
        TcpAcceptor(host: host, port: port, transport: .stable)
    }

    public func openAttachment() async throws -> LinkAttachment {
        let link = try await state.nextLink(host: host, port: port)
        return .fresh(link)
    }
}

private let defaultMaxFrameBytes = 1024 * 1024

/// Manages the NIO server channel and streams accepted connections.
private final class TcpAcceptorState: @unchecked Sendable {
    private var iterator: AsyncStream<NIOFrameLink>.Iterator?

    func nextLink(host: String, port: Int) async throws -> NIOFrameLink {
        if iterator == nil {
            iterator = try await bind(host: host, port: port)
        }
        var iter = iterator!
        guard let link = await iter.next() else {
            throw TransportError.connectionClosed
        }
        iterator = iter
        return link
    }

    private func bind(host: String, port: Int) async throws -> AsyncStream<NIOFrameLink>.Iterator {
        let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
        let (connStream, connContinuation) = AsyncStream<NIOFrameLink>.makeStream()

        let bootstrap = ServerBootstrap(group: group)
            .serverChannelOption(ChannelOptions.backlog, value: 16)
            .serverChannelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)
            .childChannelInitializer { channel in
                let frameLimit = FrameLimit(defaultMaxFrameBytes)

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

        let serverChannel = try await bootstrap.bind(host: host, port: port).get()

        guard
            let boundAddress = serverChannel.localAddress,
            let boundPort = boundAddress.port
        else {
            try? await serverChannel.close()
            try? await group.shutdownGracefully()
            throw TransportError.transportIO("TcpAcceptor: could not determine bound port")
        }

        let announcement = "LISTEN_ADDR=\(host):\(boundPort)\n"
        FileHandle.standardOutput.write(Data(announcement.utf8))

        serverChannel.closeFuture.whenComplete { _ in
            connContinuation.finish()
        }

        return connStream.makeAsyncIterator()
    }
}
