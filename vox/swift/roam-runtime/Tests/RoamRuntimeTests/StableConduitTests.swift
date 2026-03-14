import Testing

@testable import RoamRuntime

private actor TestFrameQueue {
    private var buffered: [[UInt8]] = []
    private var waiters: [CheckedContinuation<[UInt8]?, Error>] = []
    private var closed = false

    func send(_ bytes: [UInt8]) throws {
        if closed {
            throw TransportError.connectionClosed
        }
        if let waiter = waiters.first {
            waiters.removeFirst()
            waiter.resume(returning: bytes)
            return
        }
        buffered.append(bytes)
    }

    func recv() async throws -> [UInt8]? {
        if !buffered.isEmpty {
            return buffered.removeFirst()
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
        self.waiters.removeAll()
        for waiter in waiters {
            waiter.resume(returning: nil)
        }
    }
}

private final class TestLink: Link, @unchecked Sendable {
    private let inbound: TestFrameQueue
    private let outbound: TestFrameQueue

    init(inbound: TestFrameQueue, outbound: TestFrameQueue) {
        self.inbound = inbound
        self.outbound = outbound
    }

    func sendFrame(_ bytes: [UInt8]) async throws {
        try await outbound.send(bytes)
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

private func testLinkPair() -> (TestLink, TestLink) {
    let leftInbound = TestFrameQueue()
    let rightInbound = TestFrameQueue()
    return (
        TestLink(inbound: leftInbound, outbound: rightInbound),
        TestLink(inbound: rightInbound, outbound: leftInbound)
    )
}

private actor QueuedInitiatorLinkSource: LinkSource {
    private var links: [any Link]

    init(links: [any Link]) {
        self.links = links
    }

    func nextLink() async throws -> LinkAttachment {
        guard !links.isEmpty else {
            throw TransportError.connectionClosed
        }
        return .initiator(links.removeFirst())
    }
}

private actor QueuedAcceptorLinkSource: LinkSource {
    private var links: [any Link]

    init(links: [any Link]) {
        self.links = links
    }

    func nextLink() async throws -> LinkAttachment {
        guard !links.isEmpty else {
            throw TransportError.connectionClosed
        }
        return try await prepareStableAcceptorAttachment(link: links.removeFirst())
    }
}

private func makeStablePair(
    clientLinks: [any Link],
    serverLinks: [any Link]
) async throws -> (StableConduit, StableConduit) {
    let clientSource = QueuedInitiatorLinkSource(links: clientLinks)
    let serverSource = QueuedAcceptorLinkSource(links: serverLinks)

    async let client = StableConduit.connect(source: clientSource)
    async let server = StableConduit.connect(source: serverSource)
    return try await (client, server)
}

private enum StableTestTimeout: Error {
    case step(String)
}

private func awaitStep<T: Sendable>(
    _ label: String,
    timeoutNs: UInt64 = 500_000_000,
    _ operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: timeoutNs)
            throw StableTestTimeout.step(label)
        }
        let result = try await group.next()
        group.cancelAll()
        return try #require(result)
    }
}

struct StableConduitTests {
    @Test func stableSendRecvSingle() async throws {
        let (clientLink, serverLink) = testLinkPair()
        let (client, server) = try await makeStablePair(
            clientLinks: [clientLink],
            serverLinks: [serverLink]
        )

        let message = MessageV7.ping(.init(nonce: 42))
        try await client.send(message)
        let received = try await server.recv()

        #expect(received == message)
    }

    @Test func stableReconnectReplaysUnackedFrames() async throws {
        let (clientLink1, serverLink1) = testLinkPair()
        let (clientLink2, serverLink2) = testLinkPair()
        let (client, server) = try await makeStablePair(
            clientLinks: [clientLink1, clientLink2],
            serverLinks: [serverLink1, serverLink2]
        )

        let alpha = MessageV7.ping(.init(nonce: 1))
        let ack = MessageV7.pong(.init(nonce: 1))
        let beta = MessageV7.ping(.init(nonce: 2))
        let gamma = MessageV7.ping(.init(nonce: 3))

        try await awaitStep("client send alpha") {
            try await client.send(alpha)
        }
        #expect(try await awaitStep("server recv alpha") {
            try await server.recv()
        } == alpha)

        try await awaitStep("server send ack") {
            try await server.send(ack)
        }
        #expect(try await awaitStep("client recv ack") {
            try await client.recv()
        } == ack)

        try await awaitStep("client send beta") {
            try await client.send(beta)
        }
        try await clientLink1.close()

        async let betaReceived = awaitStep("server recv beta") {
            try await server.recv()
        }
        try await awaitStep("client send gamma") {
            try await client.send(gamma)
        }

        #expect(try await betaReceived == beta)
        #expect(try await awaitStep("server recv gamma") {
            try await server.recv()
        } == gamma)
    }
}
