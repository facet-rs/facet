import Foundation
import Testing

@testable import VoxRuntime

private enum TestTransportError: Error {
    case sendFailed
}

private enum AsyncTestError: Error {
    case timedOut
}

private let peepsMethodNameMetadataKey = "moire.method_name"
private let peepsRequestEntityIdMetadataKey = "moire.request_entity_id"
private let peepsConnectionCorrelationIdMetadataKey = "moire.connection_correlation_id"

private enum InboundEvent: Sendable {
    case frame([UInt8])
    case closed
}

private enum SentFrame: Sendable {
    case raw([UInt8])
    case handshake(HandshakeMessage)
    case message(Message)
}

private actor ScriptedTransport: Link {
    private var sentFrames: [SentFrame] = []
    private var sentMessages: [Message] = []
    private var sentHandshakes: [HandshakeMessage] = []
    private var inboundQueue: [InboundEvent] = []
    private var recvWaiters: [CheckedContinuation<InboundEvent, Never>] = []

    private var failNextRequestSend = false
    private let autoRespondRequestCount: Int
    private let dropAfterRequestCount: Int?
    private let autoRespondPing: Bool
    private var requestSends = 0
    private var didClose = false

    init(
        autoRespondRequestCount: Int = 0,
        dropAfterRequestCount: Int? = nil,
        autoRespondPing: Bool = false,
        initialHandshake: HandshakeMessage? = .helloYourself(
            HandshakeHelloYourself(
                connectionSettings: ConnectionSettings(parity: .even, maxConcurrentRequests: 64),
                messagePayloadSchemaCbor: wireMessageSchemasCbor,
                supportsRetry: true,
                resumeKey: nil,
                metadata: []
            ))
    ) {
        self.autoRespondRequestCount = autoRespondRequestCount
        self.dropAfterRequestCount = dropAfterRequestCount
        self.autoRespondPing = autoRespondPing
        if let initialHandshake {
            if case .hello = initialHandshake {
                inboundQueue.append(.frame(encodeTransportHello(.bare)))
            }
            inboundQueue.append(.frame(initialHandshake.encodeCbor()))
            if case .hello = initialHandshake {
                inboundQueue.append(.frame(HandshakeMessage.letsGo(HandshakeLetsGo()).encodeCbor()))
            }
        }
    }

    func setFailNextRequestSend() {
        failNextRequestSend = true
    }

    func enqueueMessage(_ message: Message) {
        enqueueInbound(.frame(message.encode()))
    }

    func enqueueRaw(_ bytes: [UInt8]) {
        enqueueInbound(.frame(bytes))
    }

    func enqueueHandshake(_ handshake: HandshakeMessage) {
        enqueueInbound(.frame(handshake.encodeCbor()))
    }

    func sent() -> [Message] {
        sentMessages
    }

    func sentHandshakeMessages() -> [HandshakeMessage] {
        sentHandshakes
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

    func sendFrame(_ bytes: [UInt8]) async throws {
        if bytes.count == 8,
            (Array(bytes[0..<4]) == Array("VOTH".utf8)
                || Array(bytes[0..<4]) == Array("VOTA".utf8)
                || Array(bytes[0..<4]) == Array("VOTR".utf8))
        {
            sentFrames.append(.raw(bytes))
            return
        }

        if let handshake = try? HandshakeMessage.decodeCbor(bytes) {
            sentFrames.append(.handshake(handshake))
            sentHandshakes.append(handshake)
            return
        }

        let message = try Message.decode(from: Data(bytes))
        sentFrames.append(.message(message))
        sentMessages.append(message)

        if case .ping(let ping) = message.payload {
            if autoRespondPing {
                enqueueInbound(.frame(Message.pong(.init(nonce: ping.nonce)).encode()))
            }
            return
        }

        if case .requestMessage(let request) = message.payload,
            case .call = request.body
        {
            let requestId = request.id
            if failNextRequestSend {
                failNextRequestSend = false
                throw TestTransportError.sendFailed
            }

            requestSends += 1

            if requestSends <= autoRespondRequestCount {
                enqueueInbound(
                    .frame(
                        Message.response(
                            connId: 0,
                            requestId: requestId,
                            metadata: [],
                            payload: [0]
                        ).encode()
                    )
                )
            }

            if let dropAfterRequestCount, requestSends == dropAfterRequestCount {
                didClose = true
                enqueueInbound(.closed)
            }
        }
    }

    func recvFrame() async throws -> [UInt8]? {
        let event: InboundEvent
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
        guard !didClose else {
            return
        }
        didClose = true
        enqueueInbound(.closed)
        for waiter in recvWaiters {
            waiter.resume(returning: .closed)
        }
        recvWaiters.removeAll()
    }

    private func enqueueInbound(_ event: InboundEvent) {
        if let waiter = recvWaiters.first {
            recvWaiters.removeFirst()
            waiter.resume(returning: event)
            return
        }
        inboundQueue.append(event)
    }
}

private struct NoopDispatcher: ServiceDispatcher {
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


@Test func acceptorSessionExposesPeerHandshakeMetadata() async throws {
    let metadata = [
        MetadataEntry(key: "vox-service", value: .string("Noop"), flags: 0),
        MetadataEntry(key: "vixenfs-sid", value: .string("abc123"), flags: 0),
    ]
    let link = ScriptedTransport(
        initialHandshake: .hello(
            HandshakeHello(
                parity: .odd,
                connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64),
                messagePayloadSchemaCbor: wireMessageSchemasCbor,
                supportsRetry: true,
                resumeKey: nil,
                metadata: metadata
            ))
    )

    let session = try await Session.acceptFreshLink(link, dispatcher: NoopDispatcher())

    #expect(session.peerMetadata == metadata)
}

@Test func acceptorSessionConsumesTransportPrologueBeforeHandshake() async throws {
    let metadata = [
        MetadataEntry(key: "vox-service", value: .string("Noop"), flags: 0),
        MetadataEntry(key: "vixenfs-sid", value: .string("abc123"), flags: 0),
    ]
    let link = ScriptedTransport(initialHandshake: nil)
    await link.enqueueRaw(encodeTransportHello(.bare))
    await link.enqueueHandshake(
        .hello(
            HandshakeHello(
                parity: .odd,
                connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64),
                messagePayloadSchemaCbor: wireMessageSchemasCbor,
                supportsRetry: true,
                resumeKey: nil,
                metadata: metadata
            ))
    )
    await link.enqueueHandshake(.letsGo(HandshakeLetsGo()))

    let session = try await Session.acceptFreshLink(link, dispatcher: NoopDispatcher())

    #expect(session.peerMetadata == metadata)
}

private struct ImmediateResponseDispatcher: ServiceDispatcher {
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
        taskTx(.response(requestId: requestId, payload: [0x01]))
    }
}

private actor BlockingDispatchProbe {
    private var dispatchCount = 0
    private var released = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func waitForRelease() async {
        dispatchCount += 1
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

    func count() -> Int {
        dispatchCount
    }
}

private struct BlockingResponseDispatcher: ServiceDispatcher {
    let probe: BlockingDispatchProbe

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

private func metadataString(_ metadata: [MetadataEntry], key: String) -> String? {
    for entry in metadata where entry.key == key {
        if case .string(let value) = entry.value {
            return value
        }
    }
    return nil
}

private func awaitHasCancel(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 500
) async -> Bool {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        if sent.contains(where: { message in
            if case .requestMessage(let request) = message.payload,
                case .cancel = request.body
            {
                return true
            }
            return false
        }) {
            return true
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return false
}

private func awaitRequestId(
    _ transport: ScriptedTransport,
    index: Int,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let requestIds = await transport.sentRequestIds()
        if requestIds.count > index {
            return requestIds[index]
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitProtocolReason(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 1_000
) async -> String? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for msg in sent {
            if case .protocolError(let err) = msg.payload {
                return err.description
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitResponsePayload(
    _ transport: ScriptedTransport,
    requestId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> [UInt8]? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
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

private func isConnectionClosed(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .connectionClosed = connError {
        return true
    }
    return false
}

private func isTransportError(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .transportError = connError {
        return true
    }
    return false
}

private func isTimeout(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .timeout = connError {
        return true
    }
    return false
}

private func isProtocolViolation(_ error: Error, rule: String) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .protocolViolation(let actualRule) = connError {
        return actualRule == rule
    }
    return false
}

private func awaitSentPingNonce(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent {
            if case .ping(let ping) = message.payload {
                return ping.nonce
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitTaskResult<T: Sendable>(
    _ task: Task<T, Error>,
    timeoutMs: UInt64 = 1_000
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await task.value
        }
        group.addTask {
            try await Task.sleep(nanoseconds: timeoutMs * 1_000_000)
            throw AsyncTestError.timedOut
        }
        guard let result = try await group.next() else {
            throw AsyncTestError.timedOut
        }
        group.cancelAll()
        return result
    }
}

@Suite(.serialized)
struct ConnectionFailureTests {
    @Test func initiatorHelloCarriesConnectionCorrelationMetadata() async throws {
        let transport = ScriptedTransport()
        _ = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )

        let sent = await transport.sentHandshakeMessages()
        guard let first = sent.first else {
            Issue.record("expected hello to be sent")
            return
        }
        guard case .hello(let hello) = first else {
            Issue.record("expected first sent message to be hello")
            return
        }
        #expect(hello.supportsRetry)
        #expect(hello.messagePayloadSchemaCbor == wireMessageSchemasCbor)
    }

    @Test func callerInjectsOperationIdWhenPeerSupportsRetry() async throws {
        let transport = ScriptedTransport(
            autoRespondRequestCount: 1,
            initialHandshake: .helloYourself(
                HandshakeHelloYourself(
                    connectionSettings: ConnectionSettings(parity: .even, maxConcurrentRequests: 64),
                    messagePayloadSchemaCbor: wireMessageSchemasCbor,
                    supportsRetry: true,
                    resumeKey: nil,
                    metadata: []
                )))
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            _ = try await handle.callRaw(methodId: 1, payload: [0x01], retry: .persist, timeout: 1.0)

            let sent = await transport.sent()
            guard let request = sent.first(where: { message in
                if case .requestMessage(let request) = message.payload,
                    case .call = request.body
                {
                    return request.id == 1
                }
                return false
            }) else {
                Issue.record("expected request to be sent")
                return
            }
            guard case .requestMessage(let outboundRequest) = request.payload,
                case .call(let call) = outboundRequest.body
            else {
                Issue.record("expected outbound request call")
                return
            }

            #expect(metadataOperationId(call.metadata) != nil)
        }
    }

    @Test func duplicateOperationIdAttachesLiveAndReplaysSealedOutcome() async throws {
        let transport = ScriptedTransport(
            initialHandshake: .hello(
                HandshakeHello(
                    parity: .odd,
                    connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64),
                    messagePayloadSchemaCbor: wireMessageSchemasCbor,
                    supportsRetry: true,
                    resumeKey: nil,
                    metadata: []
                )))
        let probe = BlockingDispatchProbe()
        let (_, driver, _, _, _) = try await establishAcceptor(
            conduit: transport,
            dispatcher: BlockingResponseDispatcher(probe: probe)
        )
        let driverTask = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let operationMetadata = ensureOperationId([], operationId: 99)

            await transport.enqueueMessage(
                .request(
                    connId: 0,
                    requestId: 11,
                    methodId: 7,
                    metadata: operationMetadata,
                    payload: [0xAB]
                )
            )

            let start = ContinuousClock.now
            while ContinuousClock.now - start < .milliseconds(250) {
                if await probe.count() == 1 {
                    break
                }
                try? await Task.sleep(nanoseconds: 5_000_000)
            }
            #expect(await probe.count() == 1)

            await transport.enqueueMessage(
                .request(
                    connId: 0,
                    requestId: 13,
                    methodId: 7,
                    metadata: operationMetadata,
                    payload: [0xAB]
                )
            )

            try? await Task.sleep(nanoseconds: 20_000_000)
            #expect(await probe.count() == 1)

            await probe.release()

            #expect(await awaitResponsePayload(transport, requestId: 11) == [0x42])
            #expect(await awaitResponsePayload(transport, requestId: 13) == [0x42])

            await transport.enqueueMessage(
                .request(
                    connId: 0,
                    requestId: 15,
                    methodId: 7,
                    metadata: operationMetadata,
                    payload: [0xAB]
                )
            )

            #expect(await awaitResponsePayload(transport, requestId: 15) == [0x42])
            #expect(await probe.count() == 1)
        }
    }

    @Test func serverResponsePreservesPeepsRequestMetadata() async throws {
        let transport = ScriptedTransport(
            initialHandshake: .hello(
                HandshakeHello(
                    parity: .odd,
                    connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64),
                    messagePayloadSchemaCbor: wireMessageSchemasCbor,
                    supportsRetry: true,
                    resumeKey: nil,
                    metadata: []
                )))
        let (_, driver, _, _, _) = try await establishAcceptor(
            conduit: transport,
            dispatcher: ImmediateResponseDispatcher()
        )
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            await transport.enqueueMessage(
                .request(
                    connId: 0,
                    requestId: 77,
                    methodId: 42,
                    metadata: [
                        MetadataEntry(
                            key: peepsMethodNameMetadataKey,
                            value: .string("DemoRpc.test"),
                            flags: 0
                        ),
                        MetadataEntry(
                            key: peepsRequestEntityIdMetadataKey,
                            value: .string("request:abc"),
                            flags: 0
                        ),
                        MetadataEntry(key: "unrelated", value: .string("keep_out"), flags: 0),
                    ],
                    payload: []
                )
            )

            let start = ContinuousClock.now
            let timeout = Duration.milliseconds(1_000)
            var responseMetadata: [MetadataEntry]? = nil
            while ContinuousClock.now - start < timeout {
                let sent = await transport.sent()
                for message in sent {
                    if case .requestMessage(let request) = message.payload,
                        case .response(let response) = request.body,
                        request.id == 77
                    {
                        responseMetadata = response.metadata
                        break
                    }
                }
                if responseMetadata != nil {
                    break
                }
                try? await Task.sleep(nanoseconds: 5_000_000)
            }

            guard let responseMetadata else {
                Issue.record("expected response to be sent")
                return
            }

            #expect(metadataString(responseMetadata, key: peepsMethodNameMetadataKey) == "DemoRpc.test")
            #expect(
                metadataString(responseMetadata, key: peepsRequestEntityIdMetadataKey) == "request:abc")
            #expect(metadataString(responseMetadata, key: "unrelated") == nil)
        }
    }

    @Test func immediateResponseAfterSendStillCompletesCall() async throws {
        let transport = ScriptedTransport(autoRespondRequestCount: 1)
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let payload = try await handle.callRaw(methodId: 1, payload: [1, 2, 3], timeout: 2.0)
            #expect(payload == [0])
        }
    }

    @Test func callFailsFastAfterDriverExit() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        try? await transport.close()
        _ = try? await driverTask.value

        let start = ContinuousClock.now
        do {
            _ = try await handle.callRaw(methodId: 123, payload: [1], timeout: 2.0)
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }
        let elapsed = ContinuousClock.now - start
        #expect(elapsed < .milliseconds(250))
    }

    @Test func zeroTimeoutDoesNotOrphanContinuation() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            do {
                _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 0.0)
                Issue.record("expected timeout")
            } catch {
                #expect(isTimeout(error))
            }

            #expect(await awaitHasCancel(transport))
        }
    }

    @Test func callTimesOutAndSendsCancel() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            do {
                _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 0.05)
                Issue.record("expected timeout")
            } catch {
                #expect(isTimeout(error))
            }

            #expect(await awaitHasCancel(transport))
        }
    }

    @Test func callFailsWhenRequestSendFails() async throws {
        let transport = ScriptedTransport()
        await transport.setFailNextRequestSend()

        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        do {
            _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 2.0)
            Issue.record("expected transport error")
        } catch {
            #expect(isTransportError(error))
        }

        try? await transport.close()
        _ = try? await driverTask.value
    }

    @Test func unknownResponseRequestIdClosesConnectionAndFailsPendingCalls() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let callTask = Task { try await handle.callRaw(methodId: 1, payload: [], timeout: 2.0) }
        let requestId = await awaitRequestId(transport, index: 0)
        #expect(requestId != nil)
        await transport.enqueueMessage(
            .response(connId: 0, requestId: 999, metadata: [], payload: [7, 7, 7])
        )

        do {
            _ = try await callTask.value
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }

        do {
            try await driverTask.value
            Issue.record("expected protocol violation")
        } catch {
            #expect(isProtocolViolation(error, rule: "call.lifecycle.unknown-request-id"))
        }

        let protocolReason = await awaitProtocolReason(transport)
        #expect(protocolReason == "call.lifecycle.unknown-request-id")
    }

    @Test func lateResponseAfterTimeoutIsIgnoredAndConnectionStaysUsable() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let timedOutCall = Task { try await handle.callRaw(methodId: 42, payload: [4, 2], timeout: 0.05) }
        guard let timedOutRequestId = await awaitRequestId(transport, index: 0) else {
            Issue.record("expected first request to be sent")
            return
        }

        do {
            _ = try await timedOutCall.value
            Issue.record("expected timeout")
        } catch {
            #expect(isTimeout(error))
        }

        await transport.enqueueMessage(
            .response(
                connId: 0,
                requestId: timedOutRequestId,
                metadata: [],
                payload: [0xAA]
            )
        )

        let followupCall = Task {
            try await handle.callRaw(methodId: 99, payload: [9], timeout: 1.0)
        }
        guard let followupRequestId = await awaitRequestId(transport, index: 1) else {
            Issue.record("expected follow-up request to be sent")
            return
        }

        await transport.enqueueMessage(
            .response(
                connId: 0,
                requestId: followupRequestId,
                metadata: [],
                payload: [0xBB]
            )
        )

        let followupResponse = try await awaitTaskResult(followupCall, timeoutMs: 1_000)
        #expect(followupResponse == [0xBB])
        #expect(await awaitProtocolReason(transport, timeoutMs: 100) == nil)

        try? await transport.close()
        _ = try? await driverTask.value
    }

    @Test func duplicateResponseAfterSuccessIsIgnored() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let firstCall = Task {
            try await handle.callRaw(methodId: 7, payload: [7], timeout: 1.0)
        }
        guard let firstRequestId = await awaitRequestId(transport, index: 0) else {
            Issue.record("expected first request to be sent")
            return
        }

        await transport.enqueueMessage(
            .response(
                connId: 0,
                requestId: firstRequestId,
                metadata: [],
                payload: [0x01]
            )
        )

        let firstResponse = try await awaitTaskResult(firstCall, timeoutMs: 1_000)
        #expect(firstResponse == [0x01])

        await transport.enqueueMessage(
            .response(
                connId: 0,
                requestId: firstRequestId,
                metadata: [],
                payload: [0x02]
            )
        )

        let secondCall = Task {
            try await handle.callRaw(methodId: 8, payload: [8], timeout: 1.0)
        }
        guard let secondRequestId = await awaitRequestId(transport, index: 1) else {
            Issue.record("expected second request to be sent")
            return
        }

        await transport.enqueueMessage(
            .response(
                connId: 0,
                requestId: secondRequestId,
                metadata: [],
                payload: [0x03]
            )
        )

        let secondResponse = try await awaitTaskResult(secondCall, timeoutMs: 1_000)
        #expect(secondResponse == [0x03])
        #expect(await awaitProtocolReason(transport, timeoutMs: 100) == nil)

        try? await transport.close()
        _ = try? await driverTask.value
    }

    @Test func protocolViolationFromIncomingMessageFailsPendingCalls() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let pendingCall = Task { try await handle.callRaw(methodId: 7, payload: [7], timeout: 2.0) }
        let reqId = await awaitRequestId(transport, index: 0)
        #expect(reqId != nil)

        await transport.enqueueMessage(.data(connId: 0, channelId: 0, payload: [1]))

        do {
            _ = try await pendingCall.value
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }

        do {
            try await driverTask.value
            Issue.record("expected protocol violation")
        } catch {
            #expect(isProtocolViolation(error, rule: "rpc.channel.allocation"))
        }
    }

    @Test func manyCallsFailFastWhenConnectionDrops() async throws {
        let transport = ScriptedTransport(autoRespondRequestCount: 20, dropAfterRequestCount: 20)
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let results = try await withThrowingTaskGroup(of: Result<[UInt8], Error>.self) { group in
                for _ in 0..<100 {
                    group.addTask {
                        do {
                            let response = try await handle.callRaw(methodId: 1, payload: [], timeout: 1.0)
                            return .success(response)
                        } catch {
                            return .failure(error)
                        }
                    }
                }

                var all: [Result<[UInt8], Error>] = []
                for try await result in group {
                    all.append(result)
                }
                return all
            }

            let successCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial + 1
                case .failure:
                    return partial
                }
            }
            let closedCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial
                case .failure(let error):
                    return partial + (isConnectionClosed(error) ? 1 : 0)
                }
            }
            let timeoutCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial
                case .failure(let error):
                    return partial + (isTimeout(error) ? 1 : 0)
                }
            }

            #expect(successCount > 0)
            #expect(closedCount > 0)
            #expect(timeoutCount == 0)
        }
    }

    @Test func keepalivePingPongHealthyPath() async throws {
        let transport = ScriptedTransport(autoRespondPing: true)
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher(),
            keepalive: DriverKeepaliveConfig(pingInterval: 0.02, pongTimeout: 0.05)
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let firstPing = await awaitSentPingNonce(transport, timeoutMs: 1_000)
            #expect(firstPing != nil)

            let callTask: Task<[UInt8], Error> = Task {
                try await handle.callRaw(methodId: 99, payload: [1, 2], timeout: 1.0)
            }
            guard let requestId = await awaitRequestId(transport, index: 0) else {
                Issue.record("expected outbound request")
                return
            }
            await transport.enqueueMessage(
                .response(connId: 0, requestId: requestId, metadata: [], payload: [0x42])
            )

            let response = try await awaitTaskResult(callTask, timeoutMs: 1_000)
            #expect(response == [0x42])
        }
    }

    @Test func keepaliveMissingPongClosesDriver() async throws {
        let transport = ScriptedTransport()
        let (_, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher(),
            keepalive: DriverKeepaliveConfig(pingInterval: 0.02, pongTimeout: 0.05)
        )
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let firstPing = await awaitSentPingNonce(transport, timeoutMs: 1_000)
            #expect(firstPing != nil)

            do {
                _ = try await awaitTaskResult(driverTask, timeoutMs: 1_000)
                Issue.record("expected connection closed")
            } catch {
                #expect(isConnectionClosed(error))
            }
        }
    }

    @Test func keepaliveFailureFailsPendingCall() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: NoopDispatcher(),
            keepalive: DriverKeepaliveConfig(pingInterval: 0.02, pongTimeout: 0.05)
        )
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let callTask: Task<[UInt8], Error> = Task {
                try await handle.callRaw(methodId: 123, payload: [9], timeout: TimeInterval?.none)
            }

            do {
                _ = try await awaitTaskResult(callTask, timeoutMs: 1_000)
                Issue.record("expected connection closed")
            } catch {
                #expect(isConnectionClosed(error))
            }
        }

        do {
            _ = try await awaitTaskResult(driverTask, timeoutMs: 1_000)
            Issue.record("expected driver shutdown")
        } catch {
            #expect(isConnectionClosed(error))
        }
    }
}
