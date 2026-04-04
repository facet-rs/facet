import Foundation
import Testing

@testable import VoxRuntime
@testable import subject_swift

private enum RetryHarnessTimeout: Error {
    case step(String)
}

private func retryTestLog(_ message: String) {
    FileHandle.standardError.write(Data("[retry-harness] \(message)\n".utf8))
}

private func awaitRetryStep<T: Sendable>(
    _ label: String,
    timeoutNs: UInt64 = 1_000_000_000,
    _ operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: timeoutNs)
            throw RetryHarnessTimeout.step(label)
        }
        let result = try await group.next()
        group.cancelAll()
        return try #require(result)
    }
}

private actor RetryTestFrameQueue {
    private let capacity = 1
    private var buffered: [[UInt8]] = []
    private var waiters: [CheckedContinuation<[UInt8]?, Error>] = []
    private var sendWaiters: [CheckedContinuation<Void, Error>] = []
    private var closed = false

    func send(_ bytes: [UInt8]) async throws {
        while true {
            if closed {
                throw TransportError.connectionClosed
            }
            if let waiter = waiters.first {
                waiters.removeFirst()
                waiter.resume(returning: bytes)
                return
            }
            if buffered.count < capacity {
                buffered.append(bytes)
                return
            }
            try await withCheckedThrowingContinuation { continuation in
                sendWaiters.append(continuation)
            }
        }
    }

    func recv() async throws -> [UInt8]? {
        if !buffered.isEmpty {
            let bytes = buffered.removeFirst()
            if let sender = sendWaiters.first {
                sendWaiters.removeFirst()
                sender.resume()
            }
            return bytes
        }
        if closed {
            return nil
        }
        return try await withCheckedThrowingContinuation { continuation in
            waiters.append(continuation)
        }
    }

    func close() {
        closed = true
        buffered.removeAll()
        let waiters = waiters
        let sendWaiters = sendWaiters
        self.waiters.removeAll()
        self.sendWaiters.removeAll()
        for waiter in waiters {
            waiter.resume(returning: nil)
        }
        for sender in sendWaiters {
            sender.resume(throwing: TransportError.connectionClosed)
        }
    }
}

private final class RetryTestLink: Link, @unchecked Sendable {
    private actor Behavior {
        private var autoCloseAfterFrames: Int?
        private var sentFrames = 0
        private var triggered = false

        func armAutoClose(afterFrames: Int) {
            autoCloseAfterFrames = afterFrames
            sentFrames = 0
            triggered = false
        }

        func noteSend() -> Bool {
            guard let autoCloseAfterFrames, !triggered else {
                return false
            }
            sentFrames += 1
            if sentFrames >= autoCloseAfterFrames {
                triggered = true
                return true
            }
            return false
        }
    }

    private let inbound: RetryTestFrameQueue
    private let outbound: RetryTestFrameQueue
    private let behavior = Behavior()

    init(inbound: RetryTestFrameQueue, outbound: RetryTestFrameQueue) {
        self.inbound = inbound
        self.outbound = outbound
    }

    func armAutoClose(afterFrames: Int) async {
        await behavior.armAutoClose(afterFrames: afterFrames)
    }

    func sendFrame(_ bytes: [UInt8]) async throws {
        try await Task.sleep(nanoseconds: 1_000_000)
        try await outbound.send(bytes)
        if await behavior.noteSend() {
            try await close()
        }
    }

    func recvFrame() async throws -> [UInt8]? {
        try await inbound.recv()
    }

    func setMaxFrameSize(_: Int) async throws {}

    func close() async throws {
        await inbound.close()
        await outbound.close()
    }
}

private func retryTestLinkPair() -> (RetryTestLink, RetryTestLink) {
    let leftInbound = RetryTestFrameQueue()
    let rightInbound = RetryTestFrameQueue()
    return (
        RetryTestLink(inbound: leftInbound, outbound: rightInbound),
        RetryTestLink(inbound: rightInbound, outbound: leftInbound)
    )
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

private struct RetryProbeDispatcher: ServiceDispatcher {
    actor Lifetime {
        private var closed = false
        private var attemptsByMethod: [UInt64: Int] = [:]

        func close() {
            closed = true
        }

        func isClosed() -> Bool {
            closed
        }

        func nextAttempt(methodId: UInt64) -> Int {
            let next = (attemptsByMethod[methodId] ?? 0) + 1
            attemptsByMethod[methodId] = next
            return next
        }
    }

    let lifetime: Lifetime

    func retryPolicy(methodId: UInt64) -> RetryPolicy {
        switch methodId {
        case TestbedMethodId.generateRetryIdem:
            return .idem
        case TestbedMethodId.generateRetryNonIdem:
            return .volatile
        default:
            return .volatile
        }
    }

    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        requestId: UInt64,
        registry: ChannelRegistry,
        schemaSendTracker: SchemaSendTracker,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
        ) async {
            _ = schemaSendTracker
            do {
                var cursor = 0
                let payloadData = Data(payload)
                let count = try decodeU32(from: payloadData, offset: &cursor)
            let outputChannelId = try decodeVarint(from: payloadData, offset: &cursor)
            let output = await createServerTx(
                channelId: outputChannelId,
                taskSender: taskTx,
                registry: registry,
                initialCredit: 16,
                serialize: { encodeI32($0) }
            )
            defer { output.close() }
            let attempt = await lifetime.nextAttempt(methodId: methodId)

            for i in 0..<Int32(count) {
                if await lifetime.isClosed() || Task.isCancelled {
                    return
                }
                if attempt == 1 && i >= 3 {
                    return
                }
                retryTestLog("dispatcher: method=\(String(methodId, radix: 16)) send \(i)")
                try await output.send(i)
                try await Task.sleep(nanoseconds: 20_000_000)
            }
            taskTx(.response(requestId: requestId, payload: encodeResultOk((), encoder: { _ in [] })))
        } catch {
            if error is ChannelError || error is ConnectionError {
                return
            }
            taskTx(.response(requestId: requestId, payload: encodeInvalidPayloadError()))
        }
    }
}

private actor QueuedStableClientConnector: SessionConnector {
    let transport: ConduitKind = .bare
    private var links: [RetryTestLink]

    init(links: [RetryTestLink]) {
        self.links = links
    }

    func openAttachment() async throws -> LinkAttachment {
        guard !links.isEmpty else {
            throw TransportError.connectionClosed
        }
        return .initiator(links.removeFirst())
    }
}

private func cancelAndDrain<T>(_ task: Task<T, Error>) async {
    task.cancel()
    _ = try? await task.value
}

private func withResumableHarness<T: Sendable>(
    _ body: @escaping @Sendable (ResumableStableHarness) async throws -> T
) async throws -> T {
    let harness = try await awaitRetryStep("harness make") {
        try await ResumableStableHarness.make()
    }
    do {
        let result = try await body(harness)
        await harness.shutdown()
        return result
    } catch {
        await harness.shutdown()
        throw error
    }
}

private final class ResumableStableHarness: @unchecked Sendable {
    let client: TestbedClient

    private let clientSession: Session
    private let serverSession: Session
    private let clientLinks: [RetryTestLink]
    private let serverLinks: [RetryTestLink]
    private let lifetime: RetryProbeDispatcher.Lifetime
    private let clientDriverTask: Task<Void, Error>
    private let serverDriverTask: Task<Void, Error>
    private let serverResumeTask: Task<Void, Error>
    private init(
        clientSession: Session,
        serverSession: Session,
        clientLinks: [RetryTestLink],
        serverLinks: [RetryTestLink],
        lifetime: RetryProbeDispatcher.Lifetime,
        clientDriverTask: Task<Void, Error>,
        serverDriverTask: Task<Void, Error>,
        serverResumeTask: Task<Void, Error>
    ) {
        self.client = TestbedClient(connection: clientSession.connection, timeout: 5.0)
        self.clientSession = clientSession
        self.serverSession = serverSession
        self.clientLinks = clientLinks
        self.serverLinks = serverLinks
        self.lifetime = lifetime
        self.clientDriverTask = clientDriverTask
        self.serverDriverTask = serverDriverTask
        self.serverResumeTask = serverResumeTask
    }

    static func make() async throws -> ResumableStableHarness {
        retryTestLog("make: creating links")
        let (clientLink0, serverLink0) = retryTestLinkPair()
        let (clientLink1, serverLink1) = retryTestLinkPair()
        let connector = QueuedStableClientConnector(links: [clientLink0, clientLink1])
        let lifetime = RetryProbeDispatcher.Lifetime()
        let dispatcher = RetryProbeDispatcher(lifetime: lifetime)

        async let clientSession = Session.initiator(
            connector,
            dispatcher: RetryNoopDispatcher(),
            resumable: true
        )
        async let serverSession: Session = {
            return try await Session.acceptorOn(
                serverLink0,
                transport: .bare,
                dispatcher: dispatcher,
                resumable: true
            )
        }()

        let establishedClientSession = try await clientSession
        let establishedServerSession = try await serverSession
        retryTestLog("make: sessions established")

        let clientDriverTask = Task {
            try await establishedClientSession.run()
        }
        let serverDriverTask = Task {
            try await establishedServerSession.run()
        }
        let serverResumeTask = Task {
            retryTestLog("resume-task: waiting for replacement attachment")
            let attachment = LinkAttachment(link: serverLink1)
            retryTestLog("resume-task: attachment ready")
            try await establishedServerSession.handle.acceptResumedAttachment(attachment)
            retryTestLog("resume-task: replacement accepted")
        }

        await serverLink0.armAutoClose(afterFrames: 3)

        return ResumableStableHarness(
            clientSession: establishedClientSession,
            serverSession: establishedServerSession,
            clientLinks: [clientLink0, clientLink1],
            serverLinks: [serverLink0, serverLink1],
            lifetime: lifetime,
            clientDriverTask: clientDriverTask,
            serverDriverTask: serverDriverTask,
            serverResumeTask: serverResumeTask
        )
    }

    func shutdown() async {
        retryTestLog("shutdown: begin")
        await lifetime.close()
        for link in clientLinks {
            try? await link.close()
        }
        for link in serverLinks {
            try? await link.close()
        }
        await clientSession.handle.shutdown()
        await serverSession.handle.shutdown()
        await cancelAndDrain(clientDriverTask)
        await cancelAndDrain(serverDriverTask)
        await cancelAndDrain(serverResumeTask)
        retryTestLog("shutdown: complete")
    }
}

private func collectRetryStream(
    from rx: UnboundRx<Int32>
) async throws -> [Int32] {
    var values: [Int32] = []
    for try await value in rx {
        values.append(value)
    }
    return values
}

@Suite(.serialized)
struct GeneratedClientRetryTransportTests {
    @Test func generatedClientNonIdemRetryFailsClosedOverResumableTransport() async throws {
        try await withResumableHarness { harness in
            let (tx, rx) = channel(
                serialize: { encodeI32($0) },
                deserialize: { bytes in
                    var offset = 0
                    return try decodeI32(from: Data(bytes), offset: &offset)
                }
            )

            let callTask: Task<Void, Error> = Task {
                retryTestLog("non-idem: call start")
                try await harness.client.generateRetryNonIdem(count: 12, output: tx)
            }
            let receiveTask: Task<[Int32], Error> = Task {
                retryTestLog("non-idem: receive start")
                return try await collectRetryStream(from: rx)
            }

            do {
                try await awaitRetryStep("non-idem call") {
                    try await callTask.value
                }
                Issue.record("expected VoxError.indeterminate")
            } catch VoxError.indeterminate {
                retryTestLog("non-idem: received indeterminate")
            }

            let received = try await awaitRetryStep("non-idem receive") {
                try await receiveTask.value
            }
            retryTestLog("non-idem: received \(received.count) values")
            let expected = (0..<Int32(received.count)).map { $0 }
            #expect(received == expected)
        }
    }

    @Test func generatedClientIdemRetryRerunsOverResumableTransport() async throws {
        try await withResumableHarness { harness in
            let (tx, rx) = channel(
                serialize: { encodeI32($0) },
                deserialize: { bytes in
                    var offset = 0
                    return try decodeI32(from: Data(bytes), offset: &offset)
                }
            )

            let callTask: Task<Void, Error> = Task {
                retryTestLog("idem: call start")
                try await harness.client.generateRetryIdem(count: 12, output: tx)
            }
            let receiveTask: Task<[Int32], Error> = Task {
                retryTestLog("idem: receive start")
                return try await collectRetryStream(from: rx)
            }

            try await awaitRetryStep("idem call") {
                try await callTask.value
            }
            let received = try await awaitRetryStep("idem receive") {
                try await receiveTask.value
            }
            retryTestLog("idem: received \(received.count) values")
            guard let restart = received.enumerated().dropFirst().first(where: { $0.element == 0 })?.offset else {
                Issue.record("expected retry restart in stream")
                return
            }
            let expectedPrefix = (0..<Int32(restart)).map { $0 }
            #expect(Array(received[..<restart]) == expectedPrefix)
            let expectedRerun = (0..<Int32(12)).map { $0 }
            #expect(Array(received[restart...]) == expectedRerun)
        }
    }
}
