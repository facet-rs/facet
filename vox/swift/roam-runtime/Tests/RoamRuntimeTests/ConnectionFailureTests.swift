import Foundation
import Testing

@testable import RoamRuntime

private enum TestTransportError: Error {
    case sendFailed
}

private enum InboundEvent: Sendable {
    case message(Message)
    case closed
}

private actor ScriptedTransport: MessageTransport {
    private var sentMessages: [Message] = []
    private var inboundQueue: [InboundEvent] = []
    private var recvWaiters: [CheckedContinuation<InboundEvent, Never>] = []

    private var failNextRequestSend = false
    private let autoRespondRequestCount: Int
    private let dropAfterRequestCount: Int?
    private var requestSends = 0
    private var didClose = false

    init(autoRespondRequestCount: Int = 0, dropAfterRequestCount: Int? = nil) {
        self.autoRespondRequestCount = autoRespondRequestCount
        self.dropAfterRequestCount = dropAfterRequestCount
        inboundQueue.append(.message(.hello(defaultHello())))
    }

    func setFailNextRequestSend() {
        failNextRequestSend = true
    }

    func enqueueMessage(_ message: Message) {
        enqueueInbound(.message(message))
    }

    func sent() -> [Message] {
        sentMessages
    }

    func sentRequestIds() -> [UInt64] {
        sentMessages.compactMap { message in
            if case .request(_, let requestId, _, _, _, _) = message {
                return requestId
            }
            return nil
        }
    }

    func send(_ message: Message) async throws {
        sentMessages.append(message)

        if case .request(_, let requestId, _, _, _, _) = message {
            if failNextRequestSend {
                failNextRequestSend = false
                throw TestTransportError.sendFailed
            }

            requestSends += 1

            if requestSends <= autoRespondRequestCount {
                enqueueInbound(
                    .message(
                        .response(connId: 0, requestId: requestId, metadata: [], channels: [], payload: [0])
                    ))
            }

            if let dropAfterRequestCount, requestSends == dropAfterRequestCount {
                didClose = true
                enqueueInbound(.closed)
            }
        }
    }

    func recv() async throws -> Message? {
        let event: InboundEvent
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

private func awaitHasCancel(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 500
) async -> Bool {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        if sent.contains(where: { if case .cancel = $0 { true } else { false } }) {
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

private func awaitGoodbyeReason(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 1_000
) async -> String? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for msg in sent {
            if case .goodbye(_, let reason) = msg {
                return reason
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

struct ConnectionFailureTests {
    @Test func immediateResponseAfterSendStillCompletesCall() async throws {
        let transport = ScriptedTransport(autoRespondRequestCount: 1)
        let (handle, driver) = try await establishInitiator(
            transport: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        defer {
            Task {
                try? await transport.close()
            }
            driverTask.cancel()
        }

        let payload = try await handle.callRaw(methodId: 1, payload: [1, 2, 3], timeout: 2.0)
        #expect(payload == [0])
    }

    @Test func callFailsFastAfterDriverExit() async throws {
        let transport = ScriptedTransport()
        let (handle, driver) = try await establishInitiator(
            transport: transport,
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
        let (handle, driver) = try await establishInitiator(
            transport: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        defer {
            Task {
                try? await transport.close()
            }
            driverTask.cancel()
        }

        do {
            _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 0.0)
            Issue.record("expected timeout")
        } catch {
            #expect(isTimeout(error))
        }

        #expect(await awaitHasCancel(transport))
    }

    @Test func callTimesOutAndSendsCancel() async throws {
        let transport = ScriptedTransport()
        let (handle, driver) = try await establishInitiator(
            transport: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        defer {
            Task {
                try? await transport.close()
            }
            driverTask.cancel()
        }

        do {
            _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 0.05)
            Issue.record("expected timeout")
        } catch {
            #expect(isTimeout(error))
        }

        #expect(await awaitHasCancel(transport))
    }

    @Test func callFailsWhenRequestSendFails() async throws {
        let transport = ScriptedTransport()
        await transport.setFailNextRequestSend()

        let (handle, driver) = try await establishInitiator(
            transport: transport,
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
        let (handle, driver) = try await establishInitiator(
            transport: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let callTask = Task { try await handle.callRaw(methodId: 1, payload: [], timeout: 2.0) }
        let requestId = await awaitRequestId(transport, index: 0)
        #expect(requestId != nil)
        await transport.enqueueMessage(
            .response(connId: 0, requestId: 999, metadata: [], channels: [], payload: [7, 7, 7])
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

        let goodbyeReason = await awaitGoodbyeReason(transport)
        #expect(goodbyeReason == "call.lifecycle.unknown-request-id")
    }

    @Test func lateResponseAfterTimeoutTriggersProtocolViolation() async throws {
        let transport = ScriptedTransport()
        let (handle, driver) = try await establishInitiator(
            transport: transport,
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
                channels: [],
                payload: [0xAA]
            )
        )

        do {
            try await driverTask.value
            Issue.record("expected protocol violation")
        } catch {
            #expect(isProtocolViolation(error, rule: "call.lifecycle.unknown-request-id"))
        }

        do {
            _ = try await handle.callRaw(methodId: 99, payload: [9], timeout: 2.0)
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }
    }

    @Test func protocolViolationFromIncomingMessageFailsPendingCalls() async throws {
        let transport = ScriptedTransport()
        let (handle, driver) = try await establishInitiator(
            transport: transport,
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
            #expect(isProtocolViolation(error, rule: "channeling.id.zero-reserved"))
        }
    }

    @Test func manyCallsFailFastWhenConnectionDrops() async throws {
        let transport = ScriptedTransport(autoRespondRequestCount: 20, dropAfterRequestCount: 20)
        let (handle, driver) = try await establishInitiator(
            transport: transport,
            dispatcher: NoopDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        defer {
            Task {
                try? await transport.close()
            }
            driverTask.cancel()
        }

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
