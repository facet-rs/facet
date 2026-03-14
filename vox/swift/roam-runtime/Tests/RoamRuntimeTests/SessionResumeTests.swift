import Foundation
import Testing

@testable import RoamRuntime

private enum ResumeInboundEvent: Sendable {
    case message(MessageV7)
    case closed
}

private actor ResumeScriptedConduit: Conduit {
    private var sentMessages: [MessageV7] = []
    private var inboundQueue: [ResumeInboundEvent] = []
    private var recvWaiters: [CheckedContinuation<ResumeInboundEvent, Never>] = []
    private var closed = false

    init(initialMessage: MessageV7? = nil) {
        if let initialMessage {
            inboundQueue.append(.message(initialMessage))
        }
    }

    func send(_ message: MessageV7) async throws {
        sentMessages.append(message)
    }

    func recv() async throws -> MessageV7? {
        let event: ResumeInboundEvent
        if !inboundQueue.isEmpty {
            event = inboundQueue.removeFirst()
        } else {
            event = await withCheckedContinuation { continuation in
                recvWaiters.append(continuation)
            }
        }

        switch event {
        case .message(let message):
            return message
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

    func enqueueMessage(_ message: MessageV7) {
        enqueue(.message(message))
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

    func sentMessagesSnapshot() -> [MessageV7] {
        sentMessages
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
    private var conduits: [ResumeScriptedConduit]

    init(_ conduits: [ResumeScriptedConduit]) {
        self.conduits = conduits
    }

    func openConduit() async throws -> any Conduit {
        guard !conduits.isEmpty else {
            throw ConnectionError.connectionClosed
        }
        return conduits.removeFirst()
    }
}

private struct ResumeNoopDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        requestId _: UInt64,
        registry _: ChannelRegistry,
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
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        requestId: UInt64,
        registry _: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        await probe.waitForRelease()
        taskTx(.response(requestId: requestId, payload: [0x42]))
    }
}

private func awaitResumeRequestId(
    _ conduit: ResumeScriptedConduit,
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
    _ conduit: ResumeScriptedConduit,
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

struct SessionResumeTests {
    @Test func manualResumeKeepsPendingCallAliveAcrossDisconnect() async throws {
        let resumeKey = freshSessionResumeKey()
        let initial = ResumeScriptedConduit(
            initialMessage: .helloYourself(
                HelloYourselfV7(
                    connectionSettings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 64),
                    metadata: appendSessionResumeKeyMetadata(
                        appendRetrySupportMetadata([]),
                        key: resumeKey
                    )
                ))
        )

        let (connection, driver, handle, _) = try await establishInitiator(
            conduit: initial,
            dispatcher: ResumeNoopDispatcher(),
            resumable: true
        )
        let driverTask = Task {
            try await driver.run()
        }
        defer {
            driverTask.cancel()
            Task { try? await initial.close() }
        }

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

        let replacement = ResumeScriptedConduit(
            initialMessage: .helloYourself(
                HelloYourselfV7(
                    connectionSettings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 64),
                    metadata: appendSessionResumeKeyMetadata(
                        appendRetrySupportMetadata([]),
                        key: resumeKey
                    )
                ))
        )
        try await handle.resume(replacement)
        await replacement.enqueueMessage(
            .response(connId: 0, requestId: requestId, metadata: [], channels: [], payload: [0x42])
        )

        let response = try await callTask.value
        #expect(response == [0x42])
    }

    @Test func acceptorRegistryResumesExistingSession() async throws {
        let registry = SessionRegistry()
        let probe = ResumeBlockingProbe()
        let initial = ResumeScriptedConduit(
            initialMessage: .hello(
                HelloV7(
                    version: 7,
                    connectionSettings: ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: 64),
                    metadata: appendRetrySupportMetadata([])
                ))
        )

        let outcome = try await Session.acceptorOnOrResume(
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
        defer {
            driverTask.cancel()
            Task { try? await initial.close() }
        }

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
                channels: [],
                payload: [0xAB]
            )
        )

        try await initial.close()

        let replacement = ResumeScriptedConduit(
            initialMessage: .hello(
                HelloV7(
                    version: 7,
                    connectionSettings: ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: 64),
                    metadata: appendSessionResumeKeyMetadata(
                        appendRetrySupportMetadata([]),
                        key: sessionResumeKey
                    )
                ))
        )

        let resumed = try await Session.acceptorOnOrResume(
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

    @Test func connectorInitiatorAutoResumesPendingCall() async throws {
        let resumeKey = freshSessionResumeKey()
        let initial = ResumeScriptedConduit(
            initialMessage: .helloYourself(
                HelloYourselfV7(
                    connectionSettings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 64),
                    metadata: appendSessionResumeKeyMetadata(
                        appendRetrySupportMetadata([]),
                        key: resumeKey
                    )
                ))
        )
        let replacement = ResumeScriptedConduit(
            initialMessage: .helloYourself(
                HelloYourselfV7(
                    connectionSettings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 64),
                    metadata: appendSessionResumeKeyMetadata(
                        appendRetrySupportMetadata([]),
                        key: resumeKey
                    )
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
        defer {
            driverTask.cancel()
            Task { try? await initial.close() }
            Task { try? await replacement.close() }
        }

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
        await replacement.enqueueMessage(
            .response(connId: 0, requestId: requestId, metadata: [], channels: [], payload: [0x24])
        )

        let response = try await callTask.value
        #expect(response == [0x24])
    }
}
