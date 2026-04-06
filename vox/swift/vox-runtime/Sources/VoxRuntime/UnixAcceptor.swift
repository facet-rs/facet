import Foundation
@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

/// A Unix domain socket server that accepts one connection and returns it as a `LinkAttachment`.
/// Conforms to `LinkSource` so it can be used with `establishAcceptor`.
///
/// The server binds to the given socket path (removing any stale socket file first),
/// writes `"LISTEN_ADDR=<path>\n"` to stdout so the test harness can discover the
/// bound address, accepts exactly one incoming connection, then shuts down the server channel.
public struct UnixAcceptor: SessionConnector, LinkSource, Sendable {
    public let path: String
    public let transport: ConduitKind

    public init(path: String, transport: ConduitKind = .bare) {
        self.path = path
        self.transport = transport
    }

    public func bare() -> Self {
        Self(path: path, transport: .bare)
    }

    public func stable() -> Self {
        Self(path: path, transport: .stable)
    }

    /// Binds, accepts one connection, and returns it as a `.fresh` `LinkAttachment`.
    ///
    /// The socket path is written to stdout as `"LISTEN_ADDR=<path>\n"` before
    /// blocking on accept, so callers can discover which socket to connect to.
    public func nextLink() async throws -> LinkAttachment {
        let link = try await acceptOneUnixLink(path: path)
        return .fresh(link)
    }

    /// Satisfies `SessionConnector`: returns the accepted link as a `.fresh` attachment.
    /// The acceptor-side transport prologue is handled downstream by `establishAcceptor`
    /// / `Session.acceptFreshAttachment`, so no prologue is run here.
    public func openAttachment() async throws -> LinkAttachment {
        try await nextLink()
    }
}

/// Binds a Unix domain socket server at `path`, announces the path on stdout, accepts
/// exactly one connection, shuts the server down, and returns the `NIOFrameLink` for
/// that connection.
private func acceptOneUnixLink(path: String) async throws -> NIOFrameLink {
    // Remove stale socket file if it exists.
    unlink(path)

    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)

    // We use a checked continuation to receive the single accepted connection from
    // the childChannelInitializer closure, which runs on an NIO event-loop thread.
    return try await withCheckedThrowingContinuation {
        (continuation: CheckedContinuation<NIOFrameLink, Error>) in
        // Wrap in a class so we can mutate "resumed" safely from the NIO thread.
        let state = UnixAcceptState(continuation: continuation)

        let bootstrap = ServerBootstrap(group: group)
            .serverChannelOption(ChannelOptions.backlog, value: 1)
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

        // Bind and announce the path, then wait for one connection, all in a
        // detached task so we don't block the continuation itself.
        Task.detached {
            do {
                let serverChannel = try await bootstrap.bind(
                    unixDomainSocketPath: path
                ).get()

                // Announce the socket path so the test harness can connect.
                let announcement = "LISTEN_ADDR=\(path)\n"
                FileHandle.standardOutput.write(Data(announcement.utf8))

                // Wait for UnixAcceptState to be resolved (i.e. one connection accepted),
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

private let unixDefaultMaxFrameBytes = 1024 * 1024

/// Thread-safe bookkeeping for the single-accept handoff between the NIO
/// `childChannelInitializer` and the `withCheckedThrowingContinuation` block.
private final class UnixAcceptState: @unchecked Sendable {
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
