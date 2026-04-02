@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

/// Listens on a Unix domain socket and yields NIOFrameLink connections.
public final class UnixListener: Sendable {
    private let group: MultiThreadedEventLoopGroup
    private let channel: Channel
    private let connections: AsyncStream<NIOFrameLink>
    private let connectionsContinuation: AsyncStream<NIOFrameLink>.Continuation

    private init(
        group: MultiThreadedEventLoopGroup,
        channel: Channel,
        connections: AsyncStream<NIOFrameLink>,
        connectionsContinuation: AsyncStream<NIOFrameLink>.Continuation
    ) {
        self.group = group
        self.channel = channel
        self.connections = connections
        self.connectionsContinuation = connectionsContinuation
    }

    /// Bind to a Unix domain socket path and start accepting connections.
    public static func bind(unixPath: String) async throws -> UnixListener {
        // Remove stale socket file if it exists
        unlink(unixPath)

        let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
        let defaultMaxFrameBytes = 1024 * 1024

        let (connStream, connContinuation) = AsyncStream<NIOFrameLink>.makeStream()
        let yieldConnection = connContinuation

        let bootstrap = ServerBootstrap(group: group)
            .serverChannelOption(ChannelOptions.backlog, value: 16)
            .childChannelInitializer { channel in
                let frameLimit = FrameLimit(defaultMaxFrameBytes)

                var rawContinuation: AsyncStream<Result<[UInt8], Error>>.Continuation!
                let rawStream = AsyncStream<Result<[UInt8], Error>> { continuation in
                    rawContinuation = continuation
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
                    yieldConnection.yield(link)
                    return channel.eventLoop.makeSucceededVoidFuture()
                } catch {
                    return channel.eventLoop.makeFailedFuture(error)
                }
            }

        do {
            let serverChannel = try await bootstrap.bind(unixDomainSocketPath: unixPath).get()
            return UnixListener(
                group: group,
                channel: serverChannel,
                connections: connStream,
                connectionsContinuation: connContinuation
            )
        } catch {
            try? await group.shutdownGracefully()
            throw error
        }
    }

    /// Async sequence of accepted connections.
    public func acceptConnections() -> AsyncStream<NIOFrameLink> {
        connections
    }

    /// Stop listening and clean up.
    public func close() async throws {
        connectionsContinuation.finish()
        if channel.isActive {
            try? await channel.close()
        }
        try await group.shutdownGracefully()
    }
}
