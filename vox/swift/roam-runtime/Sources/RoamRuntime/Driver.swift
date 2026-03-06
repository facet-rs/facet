import Foundation

private let metadataFlagsNone: UInt64 = 0

private let peepsMethodNameMetadataKey = "moire.method_name"
private let peepsRequestEntityIdMetadataKey = "moire.request_entity_id"
private let peepsConnectionCorrelationIdMetadataKey = "moire.connection_correlation_id"

private func metadataString(_ metadata: [MetadataEntryV7], key: String) -> String? {
    for entry in metadata where entry.key == key {
        if case .string(let value) = entry.value {
            return value
        }
    }
    return nil
}

private func upsertStringMetadata(
    _ metadata: inout [MetadataEntryV7],
    key: String,
    value: String
) {
    for idx in metadata.indices where metadata[idx].key == key {
        metadata[idx] = MetadataEntryV7(
            key: key,
            value: .string(value),
            flags: metadataFlagsNone
        )
        return
    }
    metadata.append(
        MetadataEntryV7(
            key: key,
            value: .string(value),
            flags: metadataFlagsNone
        ))
}

private func responseMetadataFromRequest(_ requestMetadata: [MetadataEntryV7]) -> [MetadataEntryV7] {
    var responseMetadata: [MetadataEntryV7] = []
    for entry in requestMetadata {
        if entry.key == peepsMethodNameMetadataKey || entry.key == peepsRequestEntityIdMetadataKey {
            responseMetadata.append(entry)
        }
    }
    return responseMetadata
}

private func helloCorrelationId(_ hello: HelloV7) -> String? {
    let metadata = hello.metadata
    return metadataString(metadata, key: peepsConnectionCorrelationIdMetadataKey)
}

private func nextConnectionCorrelationId() -> String {
    "swift.\(UUID().uuidString.lowercased())"
}

// MARK: - Negotiated Parameters

/// Parameters negotiated during handshake.
///
/// r[impl session.handshake] - Effective limit is min of both peers.
/// r[impl rpc.flow-control.credit.initial] - Negotiated during handshake.
public struct Negotiated: Sendable {
    public let maxPayloadSize: UInt32
    public let initialCredit: UInt32
    public let maxConcurrentRequests: UInt32

    public init(maxPayloadSize: UInt32, initialCredit: UInt32, maxConcurrentRequests: UInt32) {
        self.maxPayloadSize = maxPayloadSize
        self.initialCredit = initialCredit
        self.maxConcurrentRequests = maxConcurrentRequests
    }
}

/// Driver-level protocol keepalive configuration.
public struct DriverKeepaliveConfig: Sendable {
    public let pingInterval: TimeInterval
    public let pongTimeout: TimeInterval

    public init(pingInterval: TimeInterval, pongTimeout: TimeInterval) {
        self.pingInterval = pingInterval
        self.pongTimeout = pongTimeout
    }
}

// MARK: - Service Dispatcher Protocol

/// Protocol for dispatching incoming requests.
public protocol ServiceDispatcher: Sendable {
    /// Pre-register any channels in the request payload.
    /// This is called synchronously BEFORE spawning the handler task,
    /// ensuring channels are registered before any Data messages arrive.
    func preregister(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        registry: ChannelRegistry
    ) async

    /// Dispatch a request. Called in a spawned task after preregister.
    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async
}

// MARK: - Handle Command

/// Commands from ConnectionHandle to Driver.
public enum HandleCommand: Sendable {
    case call(
        requestId: UInt64,
        methodId: UInt64,
        metadata: [MetadataEntryV7],
        payload: [UInt8],
        channels: [UInt64],
        timeout: TimeInterval?,
        responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void
    )
}

// MARK: - Connection Handle

/// Actor for allocating request IDs.
private actor RequestIdAllocator {
    private var nextId: UInt64 = 1

    func allocate() -> UInt64 {
        let id = nextId
        nextId += 1
        return id
    }
}

/// Async semaphore for limiting concurrent outgoing requests.
///

/// FIFO fairness: waiters are resumed in order. `close()` fails all waiters
/// when the connection dies, preventing callers from hanging forever.
private actor AsyncSemaphore {
    private var permits: Int
    private var waiters: [CheckedContinuation<Void, Error>] = []
    private var closed = false

    init(permits: Int) {
        self.permits = permits
    }

    func acquire() async throws {
        if closed { throw ConnectionError.connectionClosed }
        if permits > 0 {
            permits -= 1
            return
        }
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            if closed {
                cont.resume(throwing: ConnectionError.connectionClosed)
            } else {
                waiters.append(cont)
            }
        }
    }

    func release() {
        if !waiters.isEmpty {
            waiters.removeFirst().resume()
        } else {
            permits += 1
        }
    }

    func close() {
        closed = true
        let pending = waiters
        waiters.removeAll()
        for w in pending {
            w.resume(throwing: ConnectionError.connectionClosed)
        }
    }
}

private final class SingleResume<ResultValue: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var finished = false
    private let body: @Sendable (Result<ResultValue, ConnectionError>) -> Void

    init(body: @escaping @Sendable (Result<ResultValue, ConnectionError>) -> Void) {
        self.body = body
    }

    func callAsFunction(_ result: Result<ResultValue, ConnectionError>) {
        let shouldRun = lock.withLock {
            if finished {
                return false
            }
            finished = true
            return true
        }
        guard shouldRun else {
            return
        }
        body(result)
    }
}

/// Handle for making outgoing RPC calls.
public final class ConnectionHandle: @unchecked Sendable {
    private let commandTx: @Sendable (HandleCommand) -> Bool
    private let taskTx: @Sendable (TaskMessage) -> Bool
    private let requestIdAllocator = RequestIdAllocator()
    fileprivate let requestSemaphore: AsyncSemaphore?

    public let channelAllocator: ChannelIdAllocator
    public let channelRegistry: ChannelRegistry

    init(
        commandTx: @escaping @Sendable (HandleCommand) -> Bool,
        taskTx: @escaping @Sendable (TaskMessage) -> Bool,
        role: Role,
        maxConcurrentRequests: UInt32 = UInt32.max
    ) {
        self.commandTx = commandTx
        self.taskTx = taskTx
        self.channelAllocator = ChannelIdAllocator(role: role)
        self.channelRegistry = ChannelRegistry()
        if maxConcurrentRequests < UInt32.max {
            self.requestSemaphore = AsyncSemaphore(permits: Int(maxConcurrentRequests))
        } else {
            self.requestSemaphore = nil
        }
    }

    /// Make a raw RPC call.
    ///
    /// r[impl rpc.flow-control.max-concurrent-requests] - Blocks if maxConcurrentRequests are in-flight.
    public func callRaw(
        methodId: UInt64,
        metadata: [MetadataEntryV7] = [],
        payload: [UInt8],
        channels: [UInt64] = [],
        timeout: TimeInterval? = nil
    ) async throws
        -> [UInt8]
    {
        // Acquire a request slot before entering the driver queue.
        // This prevents flooding the event stream under high concurrency.
        if let sem = requestSemaphore {
            try await sem.acquire()
        }

        let requestId = await requestIdAllocator.allocate()

        return try await withCheckedThrowingContinuation { cont in
            let sem = requestSemaphore
            let responseTx = SingleResume<[UInt8]> { result in
                if let sem {
                    Task { await sem.release() }
                }
                cont.resume(with: result)
            }
            let accepted = commandTx(
                .call(
                    requestId: requestId,
                    methodId: methodId,
                    metadata: metadata,
                    payload: payload,
                    channels: channels,
                    timeout: timeout,
                    responseTx: { result in responseTx(result) }
                ))
            guard accepted else {
                responseTx(.failure(.connectionClosed))
                return
            }
        }
    }

    /// Close the request semaphore, failing all blocked callers.
    fileprivate func closeRequestSemaphore() async {
        await requestSemaphore?.close()
    }

    public func sendTaskMessage(_ msg: TaskMessage) {
        _ = taskTx(msg)
    }
}

// MARK: - Driver Event

/// Events the driver processes in its run loop.
private enum DriverEvent: Sendable {
    case incomingMessage(MessageV7)
    case wake
    case retryTick
    case transportClosed
    case transportFailed(String)
}

private final class LockedQueue<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var items: [T] = []
    private var closed = false

    func push(_ item: T) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if closed {
            return false
        }
        items.append(item)
        return true
    }

    func popAll() -> [T] {
        lock.lock()
        defer { lock.unlock() }
        if items.isEmpty {
            return []
        }
        let out = items
        items = []
        return out
    }

    func close() {
        lock.lock()
        defer { lock.unlock() }
        closed = true
        items.removeAll()
    }
}

// MARK: - Driver

// MARK: - Driver State Actor

/// Actor that holds mutable driver state to avoid NSLock in async contexts.
private actor DriverState {
    // Temporary kill-switch: keep the implementation in-tree but disabled
    // while we debug the primary state-machine bug.
    private let retainFinalizedRequests = false

    private struct FinalizedRequest: Sendable {
        let reason: String
        let atUptimeNs: UInt64
    }

    struct PendingCall: Sendable {
        let responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void
        var timeoutTask: Task<Void, Never>?
    }

    var pendingResponses: [UInt64: PendingCall] = [:]
    var inFlightRequests: Set<UInt64> = []
    var inFlightResponseContext: [UInt64: InFlightResponseContext] = [:]
    private var finalizedRequests: [UInt64: FinalizedRequest] = [:]
    var isClosed = false

    func addPendingResponse(
        _ requestId: UInt64,
        _ handler: @escaping @Sendable (Result<[UInt8], ConnectionError>) -> Void,
        timeoutTask: Task<Void, Never>?
    ) -> Bool {
        guard !isClosed else {
            return false
        }
        pendingResponses[requestId] = PendingCall(responseTx: handler, timeoutTask: timeoutTask)
        return true
    }

    func removePendingResponse(_ requestId: UInt64) -> PendingCall? {
        pendingResponses.removeValue(forKey: requestId)
    }

    func markFinalizedRequest(_ requestId: UInt64, reason: String) {
        guard retainFinalizedRequests else {
            return
        }
        let now = DispatchTime.now().uptimeNanoseconds
        finalizedRequests[requestId] = FinalizedRequest(reason: reason, atUptimeNs: now)
        pruneFinalizedRequests(now: now)
    }

    func takeFinalizedRequest(_ requestId: UInt64) -> (reason: String, ageMs: UInt64)? {
        guard retainFinalizedRequests else {
            return nil
        }
        let now = DispatchTime.now().uptimeNanoseconds
        pruneFinalizedRequests(now: now)
        guard let finalized = finalizedRequests.removeValue(forKey: requestId) else {
            return nil
        }
        let ageNs = now >= finalized.atUptimeNs ? now - finalized.atUptimeNs : 0
        return (reason: finalized.reason, ageMs: ageNs / 1_000_000)
    }

    func contextSummary(requestId: UInt64?) -> String {
        let pendingCount = pendingResponses.count
        let inFlightCount = inFlightRequests.count
        let pendingHasRequest = requestId.map { pendingResponses[$0] != nil } ?? false
        let inFlightHasRequest = requestId.map { inFlightRequests.contains($0) } ?? false
        return
            "pending_count=\(pendingCount) in_flight_count=\(inFlightCount) "
            + "pending_has_request=\(pendingHasRequest) in_flight_has_request=\(inFlightHasRequest)"
    }

    private func pruneFinalizedRequests(now: UInt64) {
        let keepNs: UInt64 = 120 * 1_000_000_000
        finalizedRequests = finalizedRequests.filter { _, finalized in
            now >= finalized.atUptimeNs && (now - finalized.atUptimeNs) <= keepNs
        }
    }

    func setPendingTimeoutTask(_ requestId: UInt64, timeoutTask: Task<Void, Never>) -> Bool {
        guard var pending = pendingResponses[requestId] else {
            return false
        }
        pending.timeoutTask = timeoutTask
        pendingResponses[requestId] = pending
        return true
    }

    func addInFlight(
        _ requestId: UInt64,
        connectionId: UInt64,
        responseMetadata: [MetadataEntryV7]
    ) -> Bool {
        let inserted = inFlightRequests.insert(requestId).inserted
        if inserted {
            inFlightResponseContext[requestId] = InFlightResponseContext(
                connectionId: connectionId,
                responseMetadata: responseMetadata
            )
        }
        return inserted
    }

    func removeInFlight(_ requestId: UInt64) -> (
        removed: Bool,
        connectionId: UInt64,
        responseMetadata: [MetadataEntryV7]
    ) {
        let removed = inFlightRequests.remove(requestId) != nil
        let context = inFlightResponseContext.removeValue(forKey: requestId)
        return (
            removed,
            context?.connectionId ?? 0,
            context?.responseMetadata ?? []
        )
    }

    func failAllPending() -> [UInt64: PendingCall] {
        isClosed = true
        let responses = pendingResponses
        pendingResponses.removeAll()
        inFlightRequests.removeAll()
        inFlightResponseContext.removeAll()
        return responses
    }

    func isConnectionClosed() -> Bool {
        isClosed
    }
}

/// Actor for virtual connection state.
private actor VirtualConnectionState {
    private var nextConnId: UInt64 = 1
    private var virtualConnections: Set<UInt64> = []

    func allocateConnId() -> UInt64 {
        let id = nextConnId
        nextConnId += 1
        return id
    }

    func addConnection(_ connId: UInt64) {
        virtualConnections.insert(connId)
    }

    func removeConnection(_ connId: UInt64) {
        virtualConnections.remove(connId)
    }

    func hasConnection(_ connId: UInt64) -> Bool {
        virtualConnections.contains(connId)
    }
}

/// Bidirectional connection driver.
///
/// r[impl rpc.pipelining] - Handle requests as they arrive, each independently.
/// r[impl rpc.request] - connection_id field provides multiplexing.
///
/// Uses AsyncStream to multiplex between:
/// - Incoming messages from transport
/// - Task messages from handlers (Data/Close/Response)
/// - Commands from ConnectionHandle
public final class Driver: @unchecked Sendable {
    private struct QueuedTaskMessage: Sendable {
        let message: MessageV7
    }

    private struct QueuedCall: Sendable {
        let requestId: UInt64
        let methodId: UInt64
        let metadata: [MetadataEntryV7]
        let payload: [UInt8]
        let channels: [UInt64]
        let timeout: TimeInterval?
    }

    private struct KeepaliveRuntime {
        let pingIntervalNs: UInt64
        let pongTimeoutNs: UInt64
        var nextPingAtNs: UInt64
        var waitingPongNonce: UInt64?
        var pongDeadlineNs: UInt64
        var nextPingNonce: UInt64
    }

    private let transport: any MessageTransport
    private let dispatcher: any ServiceDispatcher
    private let role: Role
    private let negotiated: Negotiated
    private let handle: ConnectionHandle
    private let acceptConnections: Bool
    private let keepalive: DriverKeepaliveConfig?

    private let serverRegistry: ChannelRegistry
    private let state: DriverState
    private let virtualConnState: VirtualConnectionState

    // Event stream for multiplexing
    private let eventContinuation: AsyncStream<DriverEvent>.Continuation
    private let eventStream: AsyncStream<DriverEvent>
    private let commandQueue: LockedQueue<HandleCommand>
    private let taskQueue: LockedQueue<TaskMessage>
    private var pendingTaskMessages: [QueuedTaskMessage] = []
    private var pendingCalls: [QueuedCall] = []

    public init(
        transport: any MessageTransport,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil
    ) {
        self.transport = transport
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.acceptConnections = acceptConnections
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState()
        self.commandQueue = LockedQueue<HandleCommand>()
        self.taskQueue = LockedQueue<TaskMessage>()

        // Create event stream
        var continuation: AsyncStream<DriverEvent>.Continuation!
        self.eventStream = AsyncStream { cont in
            continuation = cont
        }
        self.eventContinuation = continuation
    }

    /// Internal initializer with external event stream (for proper wiring).
    fileprivate init(
        transport: any MessageTransport,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        acceptConnections: Bool,
        keepalive: DriverKeepaliveConfig?,
        eventStream: AsyncStream<DriverEvent>,
        eventContinuation: AsyncStream<DriverEvent>.Continuation,
        commandQueue: LockedQueue<HandleCommand>,
        taskQueue: LockedQueue<TaskMessage>
    ) {
        self.transport = transport
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.acceptConnections = acceptConnections
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState()
        self.eventStream = eventStream
        self.eventContinuation = eventContinuation
        self.commandQueue = commandQueue
        self.taskQueue = taskQueue
    }

    // Virtual connection helpers
    private func allocateConnId() async -> UInt64 {
        await virtualConnState.allocateConnId()
    }

    private func addVirtualConnection(_ connId: UInt64) async {
        await virtualConnState.addConnection(connId)
    }

    private func removeVirtualConnection(_ connId: UInt64) async {
        await virtualConnState.removeConnection(connId)
    }

    /// Get the task sender for handlers to send responses.
    public func taskSender() -> @Sendable (TaskMessage) -> Void {
        let cont = eventContinuation
        let queue = taskQueue
        return { msg in
            guard queue.push(msg) else {
                return
            }
            _ = cont.yield(.wake)
        }
    }

    private func drainInjectedQueues() async throws {
        let commands = commandQueue.popAll()
        for command in commands {
            await handleCommand(command)
        }
        let taskMessages = taskQueue.popAll()
        for message in taskMessages {
            try await handleTaskMessage(message)
        }
    }

    /// Run the driver until connection closes.
    public func run() async throws {
        var keepaliveRuntime = makeKeepaliveRuntime()

        // Start transport reader task
        let cont = eventContinuation
        let transport = self.transport
        let readerTask = Task {
            do {
                while true {
                    if let msg = try await transport.recv() {
                        cont.yield(.incomingMessage(msg))
                    } else {
                        cont.yield(.transportClosed)
                        break
                    }
                }
            } catch {
                cont.yield(.transportFailed(String(describing: error)))
            }
        }

        let retryTask = Task {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 10_000_000)  // 10ms
                cont.yield(.retryTick)
            }
        }

        defer {
            readerTask.cancel()
            retryTask.cancel()
            commandQueue.close()
            taskQueue.close()
            eventContinuation.finish()
        }
        do {
            // Process events
            for await event in eventStream {
                try await drainInjectedQueues()
                try await flushPendingTaskMessages()
                try await flushPendingCalls()
                switch event {
                case .incomingMessage(let msg):
                    try await handleMessage(msg, keepaliveRuntime: &keepaliveRuntime)

                case .wake:
                    break

                case .retryTick:
                    try await handleKeepaliveTick(keepaliveRuntime: &keepaliveRuntime)
                    break

                case .transportClosed:
                    warnLog("transport reader closed (recv returned nil)")
                    await failAllPending()
                    eventContinuation.finish()

                case .transportFailed(let reason):
                    warnLog("transport reader failed: \(reason)")
                    await failAllPending()
                    eventContinuation.finish()
                }
            }
        } catch {
            eventContinuation.finish()
            await failAllPending()
            try? await transport.close()
            throw error
        }
        await failAllPending()
        try? await transport.close()
    }

    /// Handle a task message from a handler.
    private func handleTaskMessage(_ msg: TaskMessage) async throws {
        let wireMsg: MessageV7
        switch msg {
        case .data(let channelId, let payload):
            wireMsg = .data(connId: 0, channelId: channelId, payload: payload)
        case .close(let channelId):
            wireMsg = .close(connId: 0, channelId: channelId)
        case .response(let requestId, let payload):
            let responseContext = await state.removeInFlight(requestId)
            guard responseContext.removed else {
                return  // Already cancelled
            }
            // r[impl rpc.flow-control.credit.exhaustion] - Outgoing responses are also bounded
            // by max_payload_size. If a handler produces a too-large response, send
            // a Cancelled error instead so the call doesn't hang.
            let checkedPayload: [UInt8]
            if payload.count > Int(negotiated.maxPayloadSize) {
                debugLog(
                    "outgoing response for request \(requestId) exceeds max_payload_size "
                        + "(\(payload.count) > \(negotiated.maxPayloadSize)), sending Cancelled")
                // Cancelled error: Result::Err(1) + RoamError::Cancelled(3)
                checkedPayload = [1, 3]
            } else {
                checkedPayload = payload
            }
            wireMsg = .response(
                connId: responseContext.connectionId,
                requestId: requestId,
                metadata: responseContext.responseMetadata,
                channels: [],
                payload: checkedPayload)
        }
        do {
            try await transport.send(wireMsg)
        } catch TransportError.wouldBlock {
            pendingTaskMessages.append(QueuedTaskMessage(message: wireMsg))
        }
    }

    /// Handle a command from ConnectionHandle.
    private func handleCommand(_ cmd: HandleCommand) async {
        switch cmd {
        case .call(
            let requestId, let methodId, let metadata, let payload, let channels, let timeout,
            let responseTx):
            let isClosed = await state.isConnectionClosed()
            guard !isClosed else {
                responseTx(.failure(.connectionClosed))
                return
            }

            let inserted = await state.addPendingResponse(
                requestId,
                responseTx,
                timeoutTask: nil
            )
            guard inserted else {
                responseTx(.failure(.connectionClosed))
                return
            }

            let msg = MessageV7.request(
                connId: 0,
                requestId: requestId,
                methodId: methodId,
                metadata: metadata,
                channels: channels,
                payload: payload
            )
            do {
                try await transport.send(msg)
            } catch TransportError.wouldBlock {
                pendingCalls.append(
                    QueuedCall(
                        requestId: requestId,
                        methodId: methodId,
                        metadata: metadata,
                        payload: payload,
                        channels: channels,
                        timeout: timeout
                    ))
                return
            } catch {
                let pending = await state.removePendingResponse(requestId)
                pending?.timeoutTask?.cancel()
                warnLog(
                    "transport send failed for request_id \(requestId): \(String(describing: error))"
                )
                responseTx(.failure(.transportError(String(describing: error))))
                await failAllPending()
                eventContinuation.finish()
                return
            }

            guard let timeout else {
                return
            }

            // Direct timeout delivery: the timeout task fires the response callback
            // directly instead of routing through the event stream. This prevents
            // timeout delivery from being delayed by event loop starvation.
            let timeoutNs = Self.timeoutToNanoseconds(timeout)
            let capturedState = state
            let capturedTransport = transport
            let timeoutTask = Task {
                do {
                    try await Task.sleep(nanoseconds: timeoutNs)
                } catch {
                    return
                }
                guard let pending = await capturedState.removePendingResponse(requestId) else {
                    return
                }
                await capturedState.markFinalizedRequest(requestId, reason: "timeout")
                pending.timeoutTask?.cancel()
                warnLog("request timed out request_id=\(requestId) timeout_s=\(timeout)")
                pending.responseTx(.failure(.timeout))
                try? await capturedTransport.send(.cancel(connId: 0, requestId: requestId))
            }
            let installed = await state.setPendingTimeoutTask(requestId, timeoutTask: timeoutTask)
            if !installed {
                timeoutTask.cancel()
            }
        }
    }

    private func flushPendingCalls() async throws {
        if pendingCalls.isEmpty {
            return
        }

        while let call = pendingCalls.first {
            let msg = MessageV7.request(
                connId: 0,
                requestId: call.requestId,
                methodId: call.methodId,
                metadata: call.metadata,
                channels: call.channels,
                payload: call.payload
            )

            do {
                try await transport.send(msg)
            } catch TransportError.wouldBlock {
                return
            } catch {
                let pending = await state.removePendingResponse(call.requestId)
                pending?.timeoutTask?.cancel()
                pending?.responseTx(.failure(.transportError(String(describing: error))))
                pendingCalls.removeFirst()
                await failAllPending()
                eventContinuation.finish()
                return
            }

            pendingCalls.removeFirst()

            guard let timeout = call.timeout else {
                continue
            }

            let timeoutNs = Self.timeoutToNanoseconds(timeout)
            let capturedState = state
            let capturedTransport = transport
            let requestId = call.requestId
            let timeoutTask = Task {
                do {
                    try await Task.sleep(nanoseconds: timeoutNs)
                } catch {
                    return
                }
                guard let pending = await capturedState.removePendingResponse(requestId) else {
                    return
                }
                await capturedState.markFinalizedRequest(requestId, reason: "timeout")
                pending.timeoutTask?.cancel()
                warnLog("request timed out request_id=\(requestId) timeout_s=\(timeout)")
                pending.responseTx(.failure(.timeout))
                try? await capturedTransport.send(.cancel(connId: 0, requestId: requestId))
            }
            let installed = await state.setPendingTimeoutTask(requestId, timeoutTask: timeoutTask)
            if !installed {
                timeoutTask.cancel()
            }
        }
    }

    private func flushPendingTaskMessages() async throws {
        if pendingTaskMessages.isEmpty {
            return
        }

        while let pending = pendingTaskMessages.first {
            do {
                try await transport.send(pending.message)
            } catch TransportError.wouldBlock {
                return
            } catch {
                await failAllPending()
                eventContinuation.finish()
                return
            }

            pendingTaskMessages.removeFirst()
        }
    }

    /// Handle an incoming message.
    ///
    /// r[impl connection.close.semantics] - Stop sending, close connection, fail in-flight.
    /// r[impl rpc.request] - Request before Response in message sequence.
    /// r[impl session.protocol-error] - Unknown message variant triggers Goodbye.
    private func handleMessage(
        _ msg: MessageV7,
        keepaliveRuntime: inout KeepaliveRuntime?
    ) async throws {
        switch msg.payload {
        case .hello, .helloYourself:
            // Duplicate handshake message, ignore once connected.
            break
        case .ping(let ping):
            do {
                try await transport.send(.pong(.init(nonce: ping.nonce)))
            } catch TransportError.wouldBlock {
                pendingTaskMessages.append(QueuedTaskMessage(message: .pong(.init(nonce: ping.nonce))))
            }
        case .pong(let pong):
            handlePong(nonce: pong.nonce, keepaliveRuntime: &keepaliveRuntime)
        case .protocolError(let error):
            await failAllPending()
            throw ConnectionError.protocolViolation(rule: error.description)
        case .connectionOpen(let open):
            if acceptConnections {
                await addVirtualConnection(msg.connectionId)
                try await transport.send(
                    .connectionAccept(
                        connId: msg.connectionId,
                        settings: open.connectionSettings,
                        metadata: []
                    ))
            } else {
                try await transport.send(.connectionReject(connId: msg.connectionId, metadata: []))
            }
        case .connectionAccept, .connectionReject:
            break
        case .connectionClose:
            warnLog("received ConnectionClose conn_id=\(msg.connectionId)")
            if msg.connectionId == 0 {
                warnLog("received ConnectionClose for root connection; shutting down driver")
                await failAllPending()
                throw ConnectionError.connectionClosed
            }
            await removeVirtualConnection(msg.connectionId)
        case .requestMessage(let request):
            switch request.body {
            case .call(let call):
                try await handleRequest(
                    connId: msg.connectionId,
                    requestId: request.id,
                    methodId: call.methodId,
                    metadata: call.metadata,
                    channels: call.channels,
                    payload: call.args.bytes
                )
            case .response(let response):
                let payload = response.ret.bytes
                guard let pending = await state.removePendingResponse(request.id) else {
                    if let finalized = await state.takeFinalizedRequest(request.id) {
                        warnLog(
                            "dropping late response for finalized request_id \(request.id) "
                                + "(reason=\(finalized.reason) age_ms=\(finalized.ageMs) "
                                + "payload_size=\(payload.count)); continuing"
                        )
                        return
                    }
                    let stateContext = await state.contextSummary(requestId: request.id)
                    warnLog(
                        "received response for unknown request_id \(request.id) "
                            + "(payload_size=\(payload.count)); state{\(stateContext)} "
                            + "queues{pending_calls=\(pendingCalls.count) "
                            + "pending_task_messages=\(pendingTaskMessages.count)}; closing connection"
                    )
                    try await sendProtocolError("call.lifecycle.unknown-request-id")
                    throw ConnectionError.protocolViolation(rule: "call.lifecycle.unknown-request-id")
                }
                pending.timeoutTask?.cancel()
                pending.responseTx(.success(payload))
            case .cancel:
                let _ = await state.removeInFlight(request.id)
            }
        case .channelMessage(let channel):
            switch channel.body {
            case .item(let item):
                try await handleData(channelId: channel.id, payload: item.item.bytes)
            case .close:
                try await handleClose(channelId: channel.id)
            case .reset:
                await serverRegistry.deliverReset(channelId: channel.id)
                await handle.channelRegistry.deliverReset(channelId: channel.id)
            case .grantCredit(let credit):
                await serverRegistry.deliverCredit(channelId: channel.id, bytes: credit.additional)
                await handle.channelRegistry.deliverCredit(channelId: channel.id, bytes: credit.additional)
            }
        }
    }

    private func makeKeepaliveRuntime() -> KeepaliveRuntime? {
        guard let keepalive else {
            return nil
        }
        let pingIntervalNs = Self.timeoutToNanoseconds(keepalive.pingInterval)
        let pongTimeoutNs = Self.timeoutToNanoseconds(keepalive.pongTimeout)
        if pingIntervalNs == 0 || pongTimeoutNs == 0 {
            warnLog("keepalive disabled due to non-positive interval/timeout")
            return nil
        }
        let now = DispatchTime.now().uptimeNanoseconds
        return KeepaliveRuntime(
            pingIntervalNs: pingIntervalNs,
            pongTimeoutNs: pongTimeoutNs,
            nextPingAtNs: Self.saturatingAdd(now, pingIntervalNs),
            waitingPongNonce: nil,
            pongDeadlineNs: 0,
            nextPingNonce: 1
        )
    }

    private func handlePong(nonce: UInt64, keepaliveRuntime: inout KeepaliveRuntime?) {
        guard var runtime = keepaliveRuntime else {
            return
        }
        guard runtime.waitingPongNonce == nonce else {
            return
        }
        runtime.waitingPongNonce = nil
        runtime.pongDeadlineNs = 0
        runtime.nextPingAtNs = Self.saturatingAdd(
            DispatchTime.now().uptimeNanoseconds,
            runtime.pingIntervalNs
        )
        keepaliveRuntime = runtime
    }

    private func handleKeepaliveTick(keepaliveRuntime: inout KeepaliveRuntime?) async throws {
        guard var runtime = keepaliveRuntime else {
            return
        }
        let now = DispatchTime.now().uptimeNanoseconds

        if let waitingNonce = runtime.waitingPongNonce,
            now >= runtime.pongDeadlineNs
        {
            warnLog(
                "keepalive timeout waiting for pong nonce=\(waitingNonce) "
                    + "timeout_ns=\(runtime.pongTimeoutNs)"
            )
            await failAllPending()
            throw ConnectionError.connectionClosed
        }

        guard runtime.waitingPongNonce == nil else {
            keepaliveRuntime = runtime
            return
        }
        guard now >= runtime.nextPingAtNs else {
            keepaliveRuntime = runtime
            return
        }

        let nonce = runtime.nextPingNonce
        do {
            try await transport.send(.ping(.init(nonce: nonce)))
            runtime.waitingPongNonce = nonce
            runtime.pongDeadlineNs = Self.saturatingAdd(now, runtime.pongTimeoutNs)
            runtime.nextPingAtNs = Self.saturatingAdd(now, runtime.pingIntervalNs)
            runtime.nextPingNonce = nonce &+ 1
        } catch TransportError.wouldBlock {
            // Retry on the next tick without starting the pong deadline.
        } catch {
            throw error
        }

        keepaliveRuntime = runtime
    }

    /// r[impl rpc.flow-control.credit.exhaustion] - Payloads bounded by max_payload_size.
    /// r[impl session.connection-settings.hello] - Exceeding limit requires Goodbye.
    /// r[impl rpc.request.id-allocation] - Each request uses a unique ID.
    private func handleRequest(
        connId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        metadata: [MetadataEntryV7],
        channels: [UInt64],
        payload: [UInt8]
    ) async throws {
        let inserted = await state.addInFlight(
            requestId,
            connectionId: connId,
            responseMetadata: responseMetadataFromRequest(metadata)
        )

        guard inserted else {
            try await sendProtocolError("call.request-id.duplicate-detection")
            throw ConnectionError.protocolViolation(rule: "call.request-id.duplicate-detection")
        }

        // r[impl rpc.flow-control.credit.exhaustion]
        if payload.count > Int(negotiated.maxPayloadSize) {
            try await sendProtocolError("rpc.flow-control.credit.exhaustion")
            throw ConnectionError.protocolViolation(rule: "rpc.flow-control.credit.exhaustion")
        }

        // Pre-register channels BEFORE spawning the handler task.
        // This ensures channels are registered before any Data messages arrive.
        await dispatcher.preregister(
            methodId: methodId,
            payload: payload,
            channels: channels,
            registry: serverRegistry
        )

        // Create task sender
        let taskTx = taskSender()

        // Dispatch (spawns handler task)
        Task {
            await dispatcher.dispatch(
                methodId: methodId,
                payload: payload,
                channels: channels,
                requestId: requestId,
                registry: serverRegistry,
                taskTx: taskTx
            )
        }
    }

    /// r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    /// r[impl rpc.metadata.unknown] - Unknown channel IDs cause Goodbye.
    /// r[impl rpc.channel.item] - Data messages routed by channel_id.
    private func handleData(channelId: UInt64, payload: [UInt8]) async throws {
        // r[impl rpc.channel.allocation]
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        // Try server registry first, then client registry
        var delivered = await serverRegistry.deliverData(channelId: channelId, payload: payload)
        if !delivered {
            delivered = await handle.channelRegistry.deliverData(
                channelId: channelId, payload: payload)
        }

        // r[impl rpc.metadata.unknown]
        if !delivered {
            try await sendProtocolError("rpc.metadata.unknown")
            throw ConnectionError.protocolViolation(rule: "rpc.metadata.unknown")
        }
    }

    /// r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    /// r[impl rpc.metadata.unknown] - Unknown channel IDs cause Goodbye.
    /// r[impl rpc.channel.close] - Close terminates the channel.
    private func handleClose(channelId: UInt64) async throws {
        // r[impl rpc.channel.allocation]
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        var delivered = await serverRegistry.deliverClose(channelId: channelId)
        if !delivered {
            delivered = await handle.channelRegistry.deliverClose(channelId: channelId)
        }

        // r[impl rpc.metadata.unknown]
        if !delivered {
            try await sendProtocolError("rpc.metadata.unknown")
            throw ConnectionError.protocolViolation(rule: "rpc.metadata.unknown")
        }
    }

    /// r[impl connection.close.semantics] - Send Goodbye with rule ID before closing.
    /// r[impl session.protocol-error] - Reason contains violated rule ID.
    private func sendProtocolError(_ reason: String) async throws {
        try await transport.send(.protocolError(description: reason))
    }

    private func failAllPending() async {
        // Close the semaphore first so blocked callRaw callers get connectionClosed
        // instead of hanging forever.
        await handle.closeRequestSemaphore()

        let responses = await state.failAllPending()

        for (_, pending) in responses {
            pending.timeoutTask?.cancel()
            pending.responseTx(.failure(.connectionClosed))
        }
    }

    private static func timeoutToNanoseconds(_ timeout: TimeInterval) -> UInt64 {
        if timeout <= 0 {
            return 0
        }
        let nanoseconds = timeout * 1_000_000_000
        if nanoseconds >= Double(UInt64.max) {
            return UInt64.max
        }
        return UInt64(nanoseconds)
    }

    private static func saturatingAdd(_ lhs: UInt64, _ rhs: UInt64) -> UInt64 {
        if lhs > UInt64.max - rhs {
            return UInt64.max
        }
        return lhs + rhs
    }
}

// MARK: - Connection Errors

/// r[impl connection] - Connection-level errors terminate the connection.
/// r[impl session.protocol-error] - Protocol errors are connection-fatal.
public enum ConnectionError: Error {
    case connectionClosed
    case timeout
    case transportError(String)
    case goodbye(reason: String)
    case protocolViolation(rule: String)
    case handshakeFailed(String)
}

// MARK: - Establish Connection

/// Establish a SHM guest connection as an initiator.
///
/// SHM is a transport; session establishment still performs the v7
/// Hello/HelloYourself exchange.
public func establishShmGuest<D: ServiceDispatcher>(
    transport: ShmGuestTransport,
    dispatcher: D,
    role: Role = .initiator,
    acceptConnections: Bool = false,
    keepalive: DriverKeepaliveConfig? = nil
) async throws -> (ConnectionHandle, Driver) {
    switch role {
    case .initiator:
        return try await establishInitiator(
            transport: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            maxPayloadSize: transport.negotiated.maxPayloadSize,
            keepalive: keepalive
        )
    case .acceptor:
        return try await establishAcceptor(
            transport: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive
        )
    }
}

/// Establish a connection as initiator.
///
/// r[impl session.message] - Hello is the first message sent.
/// r[impl session.connection-settings] - Send Hello immediately on connection.
/// r[impl rpc.request] - Initiator can start calls after Hello exchange.
public func establishInitiator(
    transport: any MessageTransport,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil
) async throws -> (ConnectionHandle, Driver) {
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let ourInitialCredit: UInt32 = 64 * 1024
    let ourCorrelationId = nextConnectionCorrelationId()
    let ourHello = HelloV7(
        version: 7,
        connectionSettings: ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: 64),
        metadata: [
            MetadataEntryV7(
                key: peepsConnectionCorrelationIdMetadataKey,
                value: .string(ourCorrelationId),
                flags: metadataFlagsNone
            )
        ]
    )
    try await transport.send(.hello(ourHello))

    guard let peerMsg = try await transport.recv() else {
        try? await transport.send(.protocolError(description: "handshake.expected-hello-yourself"))
        throw ConnectionError.handshakeFailed("expected HelloYourself")
    }
    guard case .helloYourself(let peerHello) = peerMsg.payload else {
        try? await transport.send(.protocolError(description: "handshake.expected-hello-yourself"))
        throw ConnectionError.handshakeFailed("expected HelloYourself")
    }

    let peerCorrelationId = metadataString(
        peerHello.metadata,
        key: peepsConnectionCorrelationIdMetadataKey
    )
    let canonicalCorrelationId = ourCorrelationId.isEmpty ? peerCorrelationId : ourCorrelationId
    _ = canonicalCorrelationId

    let negotiated = Negotiated(
        maxPayloadSize: ourMaxPayload,
        initialCredit: ourInitialCredit,
        maxConcurrentRequests: min(
            ourHello.connectionSettings.maxConcurrentRequests,
            peerHello.connectionSettings.maxConcurrentRequests
        )
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), initialCredit=\(negotiated.initialCredit), maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    try await transport.setMaxFrameSize(Int(negotiated.maxPayloadSize) + 64)

    return makeDriverAndHandle(
        transport: transport,
        dispatcher: dispatcher,
        role: .initiator,
        negotiated: negotiated,
        acceptConnections: acceptConnections,
        keepalive: keepalive
    )
}

/// Establish a connection as acceptor.
///
/// r[impl session.message] - Hello is the first message sent.
/// r[impl session.connection-settings] - Send Hello immediately on connection.
public func establishAcceptor(
    transport: any MessageTransport,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil
) async throws -> (ConnectionHandle, Driver) {
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let ourInitialCredit: UInt32 = 64 * 1024
    let ourCorrelationId = nextConnectionCorrelationId()
    guard let peerMsg = try await transport.recv() else {
        throw ConnectionError.handshakeFailed("expected Hello")
    }
    guard case .hello(let peerHello) = peerMsg.payload else {
        try? await transport.send(.protocolError(description: "handshake.expected-hello"))
        throw ConnectionError.handshakeFailed("expected Hello")
    }
    if peerHello.version != 7 {
        try? await transport.send(.protocolError(description: "message.hello.unknown-version"))
        throw ConnectionError.handshakeFailed("message.hello.unknown-version")
    }

    let ourHello = HelloYourselfV7(
        connectionSettings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 64),
        metadata: [
            MetadataEntryV7(
                key: peepsConnectionCorrelationIdMetadataKey,
                value: .string(ourCorrelationId),
                flags: metadataFlagsNone
            )
        ]
    )
    try await transport.send(.helloYourself(ourHello))

    let peerCorrelationId = helloCorrelationId(peerHello)
    let canonicalCorrelationId = peerCorrelationId ?? ourCorrelationId
    _ = canonicalCorrelationId

    let negotiated = Negotiated(
        maxPayloadSize: ourMaxPayload,
        initialCredit: ourInitialCredit,
        maxConcurrentRequests: min(
            ourHello.connectionSettings.maxConcurrentRequests,
            peerHello.connectionSettings.maxConcurrentRequests
        )
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), initialCredit=\(negotiated.initialCredit), maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    try await transport.setMaxFrameSize(Int(negotiated.maxPayloadSize) + 64)

    return makeDriverAndHandle(
        transport: transport,
        dispatcher: dispatcher,
        role: .acceptor,
        negotiated: negotiated,
        acceptConnections: acceptConnections,
        keepalive: keepalive
    )
}

/// Create a Driver and ConnectionHandle with properly wired command/task channels.
func makeDriverAndHandle(
    transport: any MessageTransport,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    acceptConnections: Bool,
    keepalive: DriverKeepaliveConfig? = nil
) -> (ConnectionHandle, Driver) {
    let commandQueue = LockedQueue<HandleCommand>()
    let taskQueue = LockedQueue<TaskMessage>()
    // Create the event stream that will be shared
    var continuation: AsyncStream<DriverEvent>.Continuation!
    let eventStream = AsyncStream<DriverEvent> { cont in
        continuation = cont
    }
    // Capture as let to satisfy Sendable requirements
    let capturedContinuation = continuation!

    // Create command sender that uses this continuation
    let commandSender: @Sendable (HandleCommand) -> Bool = { cmd in
        guard commandQueue.push(cmd) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }
    let taskSender: @Sendable (TaskMessage) -> Bool = { msg in
        guard taskQueue.push(msg) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }

    // Create handle with the command sender
    let handle = ConnectionHandle(
        commandTx: commandSender,
        taskTx: taskSender,
        role: role,
        maxConcurrentRequests: negotiated.maxConcurrentRequests
    )

    // Create driver with the handle and shared event stream
    let driver = Driver(
        transport: transport,
        dispatcher: dispatcher,
        role: role,
        negotiated: negotiated,
        handle: handle,
        acceptConnections: acceptConnections,
        keepalive: keepalive,
        eventStream: eventStream,
        eventContinuation: continuation,
        commandQueue: commandQueue,
        taskQueue: taskQueue
    )

    return (handle, driver)
}
    private struct InFlightResponseContext: Sendable {
        let connectionId: UInt64
        let responseMetadata: [MetadataEntryV7]
    }
