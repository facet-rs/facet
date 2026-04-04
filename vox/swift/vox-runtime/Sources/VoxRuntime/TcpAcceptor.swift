import Foundation
@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

/// A TCP server that accepts one connection and returns it as a `LinkAttachment`.
/// Conforms to `LinkSource` so it can be used with `establishAcceptor`.
///
/// The server binds (optionally to an OS-assigned port when `port` is 0), writes
/// `"LISTEN_ADDR=host:port\n"` to stdout so the test harness can discover the bound
/// address, accepts exactly one incoming connection, then shuts down the server channel.
public struct TcpAcceptor: SessionConnector, LinkSource, Sendable {
    public let host: String
    public let port: Int  // 0 = OS assigns port
    public let transport: ConduitKind

    public init(host: String, port: Int = 0, transport: ConduitKind = .bare) {
        self.host = host
        self.port = port
        self.transport = transport
    }

    public func bare() -> Self {
        Self(host: host, port: port, transport: .bare)
    }

    public func stable() -> Self {
        Self(host: host, port: port, transport: .stable)
    }

    /// Binds, accepts one connection, and returns it as a `.fresh` `LinkAttachment`.
    ///
    /// The bound port is written to stdout as `"LISTEN_ADDR=host:port\n"` before
    /// blocking on accept, so callers can discover which port was assigned by the OS.
    public func nextLink() async throws -> LinkAttachment {
        let link = try await acceptOneLink(host: host, port: port)
        return .fresh(link)
    }

    /// Satisfies `SessionConnector`: returns the accepted link as a `.fresh` attachment.
    /// The acceptor-side transport prologue is handled downstream by `establishAcceptor`
    /// / `Session.acceptFreshAttachment`, so no prologue is run here.
    public func openAttachment() async throws -> LinkAttachment {
        try await nextLink()
    }
}

/// Binds a TCP server on `host:port`, announces the bound address on stdout, accepts
/// exactly one connection, shuts the server down, and returns the `NIOFrameLink` for
/// that connection.
private func acceptOneLink(host: String, port: Int) async throws -> NIOFrameLink {
    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)

    // We use a checked continuation to receive the single accepted connection from
    // the childChannelInitializer closure, which runs on an NIO event-loop thread.
    return try await withCheckedThrowingContinuation {
        (continuation: CheckedContinuation<NIOFrameLink, Error>) in
        // Wrap in a class so we can mutate "resumed" safely from the NIO thread.
        let state = AcceptState(continuation: continuation)

        let bootstrap = ServerBootstrap(group: group)
            .serverChannelOption(ChannelOptions.backlog, value: 1)
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
                            // Note: owningGroup is intentionally nil here; we manage the
                            // group's lifetime ourselves in the Task below.
                    )
                    state.resume(with: .success(link))
                    return channel.eventLoop.makeSucceededVoidFuture()
                } catch {
                    state.resume(with: .failure(error))
                    return channel.eventLoop.makeFailedFuture(error)
                }
            }

        // Bind and announce the address, then wait for one connection, all in a
        // detached task so we don't block the continuation itself.
        Task.detached {
            do {
                let serverChannel = try await bootstrap.bind(host: host, port: port).get()

                // Retrieve the actual bound port (important when port == 0).
                guard
                    let boundAddress = serverChannel.localAddress,
                    let boundPort = boundAddress.port
                else {
                    let err = TransportError.transportIO(
                        "TcpAcceptor: could not determine bound port")
                    state.resume(with: .failure(err))
                    try? await group.shutdownGracefully()
                    return
                }

                // Announce the address so the test harness can connect.
                let announcement = "LISTEN_ADDR=\(host):\(boundPort)\n"
                FileHandle.standardOutput.write(Data(announcement.utf8))

                // Wait for AcceptState to be resolved (i.e. one connection accepted),
                // then close the server channel and shut down the group.
                await state.waitUntilResolved()
                try? await serverChannel.close()
                try? await group.shutdownGracefully()
            } catch {
                state.resume(with: .failure(error))
                try? await group.shutdownGracefully()
            }
        }
    }
}

private let defaultMaxFrameBytes = 1024 * 1024

/// Thread-safe bookkeeping for the single-accept handoff between the NIO
/// `childChannelInitializer` and the `withCheckedThrowingContinuation` block.
private final class AcceptState: @unchecked Sendable {
    private let continuation: CheckedContinuation<NIOFrameLink, Error>
    private let lock = NSLock()
    private var resumed = false

    // Continuation to notify waitUntilResolved() when the state is resolved.
    private var resolvedContinuation: CheckedContinuation<Void, Never>?

    init(continuation: CheckedContinuation<NIOFrameLink, Error>) {
        self.continuation = continuation
    }

    func resume(with result: Result<NIOFrameLink, Error>) {
        lock.lock()
        let shouldResume = !resumed
        if shouldResume { resumed = true }
        let cont = resolvedContinuation
        lock.unlock()

        if shouldResume {
            continuation.resume(with: result)
            cont?.resume()
        }
    }

    /// Suspends the caller until `resume(with:)` has been called.
    func waitUntilResolved() async {
        await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
            lock.lock()
            if resumed {
                lock.unlock()
                c.resume()
            } else {
                resolvedContinuation = c
                lock.unlock()
            }
        }
    }
}
