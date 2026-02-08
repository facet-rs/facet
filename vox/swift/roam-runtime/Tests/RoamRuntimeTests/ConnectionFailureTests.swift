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

    func sent() -> [Message] {
        sentMessages
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
    func preregister(methodId _: UInt64, payload _: [UInt8], registry _: ChannelRegistry) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
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

private func isConnectionClosed(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .connectionClosed = connError {
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

struct ConnectionFailureTests {
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
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }

        try? await transport.close()
        _ = try? await driverTask.value
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
