import Foundation
import Testing
@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

@testable import VoxRuntime
@testable import subject_swift

private enum RetryTransportTestError: Error {
    case timedOut(String)
    case missingPort
}

private let retryProbeCount: UInt32 = 12

private func withTimeout<T: Sendable>(
    milliseconds: UInt64,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: milliseconds * 1_000_000)
            throw RetryTransportTestError.timedOut("operation timed out after \(milliseconds)ms")
        }
        let result = try await group.next()!
        group.cancelAll()
        return result
    }
}

private func collectUpTo<T: Sendable>(_ count: Int, from rx: UnboundRx<T>) async throws -> [T] {
    var values: [T] = []
    values.reserveCapacity(count)
    for _ in 0..<count {
        guard let value = try await rx.recv() else {
            break
        }
        values.append(value)
    }
    return values
}

private func collectValues<T: Sendable>(from rx: UnboundRx<T>) async throws -> [T] {
    var values: [T] = []
    for try await value in rx {
        values.append(value)
    }
    return values
}

private struct RetryNoopDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId _: UInt64,
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

private actor RetrySocketHarness {
    private let host = "127.0.0.1"
    private let registry = SessionRegistry()
    private let dispatcher = TestbedDispatcher(handler: TestbedService())

    private var port: Int?
    private var serverChannel: Channel?
    private var listenerGroup: MultiThreadedEventLoopGroup?
    private var acceptTask: Task<Void, Never>?
    private var sessionTasks: [Task<Void, Never>] = []
    private var currentLink: NIOFrameLink?
    private var restartCountValue = 0

    func start() async throws -> Int {
        if port == nil {
            try await bind(port: 0)
        } else {
            try await bind(port: port!)
        }
        guard let port else {
            throw RetryTransportTestError.missingPort
        }
        return port
    }

    func restart() async throws {
        restartCountValue += 1
        await stopListener()
        guard let port else {
            throw RetryTransportTestError.missingPort
        }
        try await bind(port: port)
    }

    func restartCount() -> Int {
        restartCountValue
    }

    func stop() async {
        await stopListener()
        if let listenerGroup {
            self.listenerGroup = nil
            try? await listenerGroup.shutdownGracefully()
        }
    }

    private func bind(port: Int) async throws {
        let group: MultiThreadedEventLoopGroup
        if let listenerGroup {
            group = listenerGroup
        } else {
            let newGroup = MultiThreadedEventLoopGroup(numberOfThreads: 1)
            self.listenerGroup = newGroup
            group = newGroup
        }
        let (linkStream, linkContinuation) = AsyncStream<NIOFrameLink>.makeStream()
        let frameLimit = FrameLimit(1024 * 1024)

        let bootstrap = ServerBootstrap(group: group)
            .serverChannelOption(ChannelOptions.backlog, value: 16)
            .serverChannelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)
            .childChannelInitializer { channel in
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
                    linkContinuation.yield(link)
                    return channel.eventLoop.makeSucceededVoidFuture()
                } catch {
                    return channel.eventLoop.makeFailedFuture(error)
                }
            }

        let serverChannel = try await bootstrap.bind(host: host, port: port).get()
        guard let boundPort = serverChannel.localAddress?.port else {
            try? await serverChannel.close()
            try? await group.shutdownGracefully()
            throw RetryTransportTestError.missingPort
        }

        self.port = boundPort
        self.serverChannel = serverChannel
        serverChannel.closeFuture.whenComplete { _ in
            linkContinuation.finish()
        }

        acceptTask = Task { [weak self] in
            var iterator = linkStream.makeAsyncIterator()
            while !Task.isCancelled {
                guard let link = await iterator.next() else {
                    break
                }
                await self?.handleAcceptedLink(link)
            }
        }
    }

    private func handleAcceptedLink(_ link: NIOFrameLink) async {
        currentLink = link
        do {
            let outcome = try await Session.acceptFreshAttachmentOrResume(
                .fresh(link),
                conduit: .stable,
                registry: registry,
                dispatcher: dispatcher,
                resumable: true
            )
            if case .established(let session) = outcome {
                let task = Task {
                    do {
                        try await session.run()
                    } catch {
                        _ = error
                    }
                }
                sessionTasks.append(task)
            }
        } catch {
            _ = error
        }
    }

    private func stopListener() async {
        if let link = currentLink {
            currentLink = nil
            try? await link.close()
        }
        acceptTask?.cancel()
        acceptTask = nil
        if let serverChannel {
            self.serverChannel = nil
            try? await serverChannel.close()
        }
    }
}

@Suite(.serialized)
struct ClientRetryTransportTests {
    @Test func generatedClientNonIdemRetryFailsClosedOverSocketRestart() async throws {
        let harness = RetrySocketHarness()
        defer {
            Task { await harness.stop() }
        }

        let port = try await harness.start()
        let connector = TcpConnector(host: "127.0.0.1", port: port, transport: .stable)
        let session = try await Session.initiator(
            connector,
            dispatcher: RetryNoopDispatcher(),
            resumable: true
        )

        let sessionTask = Task {
            try await session.run()
        }

        let client = TestbedClient(connection: session.connection)
        let (tx, rx) = channel(
            serialize: { value, buf in encodeI32(value, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
        )

        let callTask = Task {
            try await client.generateRetryNonIdem(count: retryProbeCount, output: tx)
        }
        let initialValues = try await withTimeout(milliseconds: 2_000) {
            try await collectUpTo(3, from: rx)
        }

        try await harness.restart()

        do {
            _ = try await withTimeout(milliseconds: 10_000) {
                try await callTask.value
            }
            Issue.record("expected VoxError.indeterminate")
        } catch VoxError.indeterminate {
        }

        let remainingValues = try await withTimeout(milliseconds: 10_000) {
            try await collectValues(from: rx)
        }
        #expect(initialValues == [0, 1, 2])
        #expect(initialValues.count + remainingValues.count < Int(retryProbeCount))
        #expect(await harness.restartCount() == 1)

        sessionTask.cancel()
        _ = try? await sessionTask.value
    }

    @Test func generatedClientIdemRetryRerunsOverSocketRestart() async throws {
        let harness = RetrySocketHarness()
        defer {
            Task { await harness.stop() }
        }

        let port = try await harness.start()
        let connector = TcpConnector(host: "127.0.0.1", port: port, transport: .stable)
        let session = try await Session.initiator(
            connector,
            dispatcher: RetryNoopDispatcher(),
            resumable: true
        )

        let sessionTask = Task {
            try await session.run()
        }

        let client = TestbedClient(connection: session.connection)
        let (tx, rx) = channel(
            serialize: { value, buf in encodeI32(value, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
        )

        let callTask = Task {
            try await client.generateRetryIdem(count: retryProbeCount, output: tx)
        }
        let initialValues = try await withTimeout(milliseconds: 2_000) {
            try await collectUpTo(3, from: rx)
        }

        try await harness.restart()

        try await withTimeout(milliseconds: 10_000) {
            try await callTask.value
        }
        let remainingValues = try await withTimeout(milliseconds: 10_000) {
            try await collectValues(from: rx)
        }

        let expected = (0..<Int32(retryProbeCount)).map { $0 }
        #expect(initialValues == [0, 1, 2])
        #expect(remainingValues == expected)
        #expect(await harness.restartCount() == 1)

        sessionTask.cancel()
        _ = try? await sessionTask.value
    }
}
