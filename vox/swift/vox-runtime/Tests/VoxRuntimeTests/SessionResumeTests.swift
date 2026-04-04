import Foundation
import Testing

@testable import VoxRuntime

private enum ResumeInboundEvent: Sendable {
    case frame([UInt8])
    case closed
}

private actor ResumeScriptedLink: Link {
    private var sentMessages: [Message] = []
    private var sentHandshakes: [HandshakeMessage] = []
    private var inboundQueue: [ResumeInboundEvent] = []
    private var recvWaiters: [CheckedContinuation<ResumeInboundEvent, Never>] = []
    private var closed = false
    private var pendingInitialHandshake: HandshakeMessage?

    init(initialHandshake: HandshakeMessage? = nil) {
        self.pendingInitialHandshake = initialHandshake
        if let initialHandshake, case .hello = initialHandshake {
            self.inboundQueue = [
                .frame(encodeTransportHello(.bare)),
                .frame(initialHandshake.encodeCbor()),
            ]
            self.pendingInitialHandshake = nil
        } else {
            self.inboundQueue = []
        }
    }

    func sendFrame(_ bytes: [UInt8]) async throws {
        if bytes.count == 8,
            Array(bytes[0..<4]) == Array("VOTH".utf8)
                || Array(bytes[0..<4]) == Array("VOTA".utf8)
                || Array(bytes[0..<4]) == Array("VOTR".utf8)
        {
            if Array(bytes[0..<4]) == Array("VOTH".utf8) {
                enqueue(.frame(encodeTransportAccept(.bare)))
                return
            }
            if Array(bytes[0..<4]) == Array("VOTA".utf8) {
                return
            }
            throw TransportError.protocolViolation(
                "unexpected raw transport frame in scripted link")
        }

        if let handshake = try? HandshakeMessage.decodeCbor(bytes) {
            sentHandshakes.append(handshake)
            if case .hello = handshake, let initialHandshake = pendingInitialHandshake {
                pendingInitialHandshake = nil
                enqueue(.frame(initialHandshake.encodeCbor()))
            }
            if case .helloYourself = handshake {
                enqueueHandshake(.letsGo(HandshakeLetsGo()))
            }
            return
        }
        let message = try Message.decode(fromBytes: bytes)
        sentMessages.append(message)
    }

    func recvFrame() async throws -> [UInt8]? {
        let event: ResumeInboundEvent
        if !inboundQueue.isEmpty {
            event = inboundQueue.removeFirst()
        } else {
            event = await withCheckedContinuation { continuation in
                recvWaiters.append(continuation)
            }
        }

        switch event {
        case .frame(let bytes):
            return bytes
        case .closed:
            return nil
        }
    }

    func setMaxFrameSize(_: Int) async throws {}

    func close() async throws {
        guard !closed else {
            return
        }
        closed = true
        enqueue(.closed)
    }

    func enqueueMessage(_ message: Message) {
        enqueue(.frame(message.encode()))
    }

    func enqueueHandshake(_ message: HandshakeMessage) {
        enqueue(.frame(message.encodeCbor()))
    }

    func sentRequestIds() -> [UInt64] {
        sentMessages.compactMap { message in
            if case .requestMessage(let request) = message.payload,
                case .call = request.body
            {
                return request.id
            }
            return nil
        }
    }

    func sentMessagesSnapshot() -> [Message] {
        sentMessages
    }

    func sentHandshakeMessages() -> [HandshakeMessage] {
        sentHandshakes
    }

    private func enqueue(_ event: ResumeInboundEvent) {
        if let waiter = recvWaiters.first {
            recvWaiters.removeFirst()
            waiter.resume(returning: event)
            return
        }
        inboundQueue.append(event)
    }
}

private actor ResumeScriptedConnector: SessionConnector {
    let transport: ConduitKind = .bare
    private var links: [ResumeScriptedLink]

    init(_ links: [ResumeScriptedLink]) {
        self.links = links
    }

    func openAttachment() async throws -> LinkAttachment {
        guard !links.isEmpty else {
            throw ConnectionError.connectionClosed
        }
        let link = links.removeFirst()
        try await performInitiatorLinkPrologue(link: link, conduit: .bare)
        return .negotiated(link, conduit: .bare)
    }
}

private struct ResumeNoopDispatcher: ServiceDispatcher {
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

private actor ResumeBlockingProbe {
    private var released = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func waitForRelease() async {
        guard !released else {
            return
        }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
    }

    func release() {
        released = true
        let waiters = waiters
        self.waiters.removeAll()
        for waiter in waiters {
            waiter.resume()
        }
    }
}

private struct ResumeBlockingDispatcher: ServiceDispatcher {
    let probe: ResumeBlockingProbe

    func retryPolicy(methodId _: UInt64) -> RetryPolicy {
        .persistIdem
    }

    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId: UInt64,
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        await probe.waitForRelease()
        taskTx(.response(requestId: requestId, payload: [0x42]))
    }
}

private func awaitResumeRequestId(
    _ conduit: ResumeScriptedLink,
    index: Int,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let requestIds = await conduit.sentRequestIds()
        if requestIds.count > index {
            return requestIds[index]
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitResumeResponsePayload(
    _ conduit: ResumeScriptedLink,
    requestId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> [UInt8]? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await conduit.sentMessagesSnapshot()
        for message in sent {
            if case .requestMessage(let request) = message.payload,
                case .response(let response) = request.body,
                request.id == requestId
            {
                return response.ret.bytes
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

@Suite(.serialized)
struct SessionResumeTests {
    @Test func manualResumeKeepsPendingCallAliveAcrossDisconnect() async throws {
        let resumeKey = freshSessionResumeKey()
        let initial = ResumeScriptedLink(
            initialHandshake: .helloYourself(
                HandshakeHelloYourself(
                    connectionSettings: ConnectionSettings(
                        parity: .even, maxConcurrentRequests: 64),
                    messagePayloadSchemaCbor: wireMessageSchemasCbor,
                    supportsRetry: true,
                    resumeKey: .init(bytes: resumeKey),
                    metadata: []
                ))
        )

        let (connection, driver, handle, _, _) = try await establishInitiator(
            conduit: initial,
            dispatcher: ResumeNoopDispatcher(),
            resumable: true
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await initial.close()
            await cancelAndDrain(driverTask)
        }) {
            let callTask = Task {
                try await connection.callRaw(
                    methodId: 7,
                    payload: [0xAB],
                    retry: .persistIdem,
                    timeout: 5.0
                )
            }

            guard let requestId = await awaitResumeRequestId(initial, index: 0) else {
                Issue.record("expected initial request to be sent")
                return
            }

            try await initial.close()

            let replacement = ResumeScriptedLink(
                initialHandshake: .helloYourself(
                    HandshakeHelloYourself(
                        connectionSettings: ConnectionSettings(
                            parity: .even, maxConcurrentRequests: 64),
                        messagePayloadSchemaCbor: wireMessageSchemasCbor,
                        supportsRetry: true,
                        resumeKey: .init(bytes: resumeKey),
                        metadata: []
                    ))
            )
            try await handle.resume(replacement)
            await replacement.enqueueMessage(
                .response(connId: 0, requestId: requestId, metadata: [], payload: [0x42])
            )

            let response = try await callTask.value
            #expect(response == [0x42])
        }
    }

    @Test func acceptorRegistryResumesExistingSession() async throws {
        let registry = SessionRegistry()
        let probe = ResumeBlockingProbe()
        let initial = ResumeScriptedLink(
            initialHandshake: .hello(
                HandshakeHello(
                    parity: .odd,
                    connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64),
                    messagePayloadSchemaCbor: wireMessageSchemasCbor,
                    supportsRetry: true,
                    resumeKey: nil,
                    metadata: []
                ))
        )

        let outcome = try await Session.acceptFreshLinkOrResume(
            initial,
            registry: registry,
            dispatcher: ResumeBlockingDispatcher(probe: probe),
            resumable: true
        )

        guard case .established(let session) = outcome else {
            Issue.record("expected fresh session establishment")
            return
        }

        let driverTask = Task {
            try await session.run()
        }
        try await withAsyncCleanup({
            try? await initial.close()
            await cancelAndDrain(driverTask)
        }) {
            guard let sessionResumeKey = session.sessionResumeKey else {
                Issue.record("expected session resume key")
                return
            }

            let operationMetadata = ensureOperationId([], operationId: 99)
            await initial.enqueueMessage(
                .request(
                    connId: 0,
                    requestId: 11,
                    methodId: 7,
                    metadata: operationMetadata,
                    payload: [0xAB]
                )
            )

            try await initial.close()

            let replacement = ResumeScriptedLink(
                initialHandshake: .hello(
                    HandshakeHello(
                        parity: .odd,
                        connectionSettings: ConnectionSettings(
                            parity: .odd, maxConcurrentRequests: 64),
                        messagePayloadSchemaCbor: wireMessageSchemasCbor,
                        supportsRetry: true,
                        resumeKey: .init(bytes: sessionResumeKey),
                        metadata: []
                    ))
            )

            let resumed = try await Session.acceptFreshLinkOrResume(
                replacement,
                registry: registry,
                dispatcher: ResumeBlockingDispatcher(probe: probe),
                resumable: true
            )
            guard case .resumed = resumed else {
                Issue.record("expected resumed session outcome")
                return
            }

            await probe.release()
            #expect(await awaitResumeResponsePayload(replacement, requestId: 11) == [0x42])
        }
    }

    @Test func connectorInitiatorAutoResumesPendingCall() async throws {
        let resumeKey = freshSessionResumeKey()
        let initial = ResumeScriptedLink(
            initialHandshake: .helloYourself(
                HandshakeHelloYourself(
                    connectionSettings: ConnectionSettings(
                        parity: .even, maxConcurrentRequests: 64),
                    messagePayloadSchemaCbor: wireMessageSchemasCbor,
                    supportsRetry: true,
                    resumeKey: .init(bytes: resumeKey),
                    metadata: []
                ))
        )
        let replacement = ResumeScriptedLink(
            initialHandshake: .helloYourself(
                HandshakeHelloYourself(
                    connectionSettings: ConnectionSettings(
                        parity: .even, maxConcurrentRequests: 64),
                    messagePayloadSchemaCbor: wireMessageSchemasCbor,
                    supportsRetry: true,
                    resumeKey: .init(bytes: resumeKey),
                    metadata: []
                ))
        )
        let connector = ResumeScriptedConnector([initial, replacement])

        let session = try await Session.initiator(
            connector,
            dispatcher: ResumeNoopDispatcher(),
            resumable: true
        )
        let driverTask = Task {
            try await session.run()
        }
        try await withAsyncCleanup({
            try? await initial.close()
            try? await replacement.close()
            await cancelAndDrain(driverTask)
        }) {
            let callTask = Task {
                try await session.connection.callRaw(
                    methodId: 13,
                    payload: [0xCD],
                    retry: .persistIdem,
                    timeout: 5.0
                )
            }

            guard let requestId = await awaitResumeRequestId(initial, index: 0) else {
                Issue.record("expected initial request to be sent")
                return
            }

            try await initial.close()

            guard let replayedRequestId = await awaitResumeRequestId(replacement, index: 0) else {
                Issue.record("expected request to be replayed on replacement link")
                return
            }

            #expect(replayedRequestId == requestId)
            await replacement.enqueueMessage(
                .response(connId: 0, requestId: replayedRequestId, metadata: [], payload: [0x24])
            )

            let response = try await callTask.value
            #expect(response == [0x24])
        }
    }
}
