import Foundation

// MARK: - Negotiated Parameters

/// Parameters negotiated during handshake.
///
/// r[impl message.hello.negotiation] - Effective limit is min of both peers.
/// r[impl flow.channel.initial-credit] - Negotiated during handshake.
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

// MARK: - Service Dispatcher Protocol

/// Protocol for dispatching incoming requests.
public protocol ServiceDispatcher: Sendable {
    /// Pre-register any channels in the request payload.
    /// This is called synchronously BEFORE spawning the handler task,
    /// ensuring channels are registered before any Data messages arrive.
    func preregister(
        methodId: UInt64,
        payload: [UInt8],
        registry: ChannelRegistry
    ) async

    /// Dispatch a request. Called in a spawned task after preregister.
    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
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
        payload: [UInt8],
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

/// Handle for making outgoing RPC calls.
public final class ConnectionHandle: @unchecked Sendable {
    private let commandTx: @Sendable (HandleCommand) -> Bool
    private let requestIdAllocator = RequestIdAllocator()

    public let channelAllocator: ChannelIdAllocator
    public let channelRegistry: ChannelRegistry

    init(
        commandTx: @escaping @Sendable (HandleCommand) -> Bool,
        role: Role
    ) {
        self.commandTx = commandTx
        self.channelAllocator = ChannelIdAllocator(role: role)
        self.channelRegistry = ChannelRegistry()
    }

    /// Make a raw RPC call.
    public func callRaw(methodId: UInt64, payload: [UInt8], timeout: TimeInterval? = nil) async throws
        -> [UInt8]
    {
        let requestId = await requestIdAllocator.allocate()

        return try await withCheckedThrowingContinuation { cont in
            let responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void = { result in
                cont.resume(with: result)
            }
            let accepted = commandTx(
                .call(
                    requestId: requestId,
                    methodId: methodId,
                    payload: payload,
                    timeout: timeout,
                    responseTx: responseTx
                ))
            guard accepted else {
                cont.resume(throwing: ConnectionError.connectionClosed)
                return
            }
        }
    }
}

// MARK: - Driver Event

/// Events the driver processes in its run loop.
private enum DriverEvent: Sendable {
    case incomingMessage(Message)
    case taskMessage(TaskMessage)
    case command(HandleCommand)
    case callTimeout(requestId: UInt64)
    case transportClosed
}

// MARK: - Driver

// MARK: - Driver State Actor

/// Actor that holds mutable driver state to avoid NSLock in async contexts.
private actor DriverState {
    struct PendingCall: Sendable {
        let responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void
        var timeoutTask: Task<Void, Never>?
    }

    var pendingResponses: [UInt64: PendingCall] = [:]
    var inFlightRequests: Set<UInt64> = []
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

    func setPendingTimeoutTask(_ requestId: UInt64, timeoutTask: Task<Void, Never>) -> Bool {
        guard var pending = pendingResponses[requestId] else {
            return false
        }
        pending.timeoutTask = timeoutTask
        pendingResponses[requestId] = pending
        return true
    }

    func addInFlight(_ requestId: UInt64) -> Bool {
        inFlightRequests.insert(requestId).inserted
    }

    func removeInFlight(_ requestId: UInt64) -> Bool {
        inFlightRequests.remove(requestId) != nil
    }

    func failAllPending() -> [UInt64: PendingCall] {
        isClosed = true
        let responses = pendingResponses
        pendingResponses.removeAll()
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
/// r[impl call.pipelining.allowed] - Handle requests as they arrive.
/// r[impl call.pipelining.independence] - Each request handled independently.
/// r[impl transport.message.multiplexing] - channel_id field provides multiplexing.
///
/// Uses AsyncStream to multiplex between:
/// - Incoming messages from transport
/// - Task messages from handlers (Data/Close/Response)
/// - Commands from ConnectionHandle
public final class Driver: @unchecked Sendable {
    private let transport: any MessageTransport
    private let dispatcher: any ServiceDispatcher
    private let role: Role
    private let negotiated: Negotiated
    private let handle: ConnectionHandle
    private let acceptConnections: Bool

    private let serverRegistry: ChannelRegistry
    private let state: DriverState
    private let virtualConnState: VirtualConnectionState

    // Event stream for multiplexing
    private let eventContinuation: AsyncStream<DriverEvent>.Continuation
    private let eventStream: AsyncStream<DriverEvent>

    public init(
        transport: any MessageTransport,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        acceptConnections: Bool = false
    ) {
        self.transport = transport
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.acceptConnections = acceptConnections
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState()

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
        eventStream: AsyncStream<DriverEvent>,
        eventContinuation: AsyncStream<DriverEvent>.Continuation
    ) {
        self.transport = transport
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.acceptConnections = acceptConnections
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState()
        self.eventStream = eventStream
        self.eventContinuation = eventContinuation
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
        return { msg in
            cont.yield(.taskMessage(msg))
        }
    }

    /// Run the driver until connection closes.
    public func run() async throws {
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
                cont.yield(.transportClosed)
            }
        }

        defer {
            readerTask.cancel()
            eventContinuation.finish()
        }
        do {
            // Process events
            for await event in eventStream {
                switch event {
                case .incomingMessage(let msg):
                    try await handleMessage(msg)

                case .taskMessage(let msg):
                    try await handleTaskMessage(msg)

                case .command(let cmd):
                    await handleCommand(cmd)

                case .callTimeout(let requestId):
                    await handleCallTimeout(requestId: requestId)

                case .transportClosed:
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
        let wireMsg: Message
        switch msg {
        case .data(let channelId, let payload):
            wireMsg = .data(connId: 0, channelId: channelId, payload: payload)
        case .close(let channelId):
            wireMsg = .close(connId: 0, channelId: channelId)
        case .response(let requestId, let payload):
            let wasInFlight = await state.removeInFlight(requestId)
            guard wasInFlight else {
                return  // Already cancelled
            }
            // r[impl flow.call.payload-limit] - Outgoing responses are also bounded
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
                connId: 0, requestId: requestId, metadata: [], channels: [],
                payload: checkedPayload)
        }
        try await transport.send(wireMsg)
    }

    /// Handle a command from ConnectionHandle.
    private func handleCommand(_ cmd: HandleCommand) async {
        switch cmd {
        case .call(let requestId, let methodId, let payload, let timeout, let responseTx):
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

            let msg = Message.request(
                connId: 0,
                requestId: requestId,
                methodId: methodId,
                metadata: [],
                channels: [],  // TODO: Collect channel IDs for streaming methods
                payload: payload
            )
            do {
                try await transport.send(msg)
            } catch {
                let pending = await state.removePendingResponse(requestId)
                pending?.timeoutTask?.cancel()
                warnLog("transport send failed for request_id \(requestId): \(String(describing: error))")
                responseTx(.failure(.transportError(String(describing: error))))
                await failAllPending()
                eventContinuation.finish()
                return
            }

            guard let timeout else {
                return
            }

            let timeoutNs = Self.timeoutToNanoseconds(timeout)
            let continuation = eventContinuation
            let timeoutTask = Task {
                do {
                    try await Task.sleep(nanoseconds: timeoutNs)
                } catch {
                    return
                }
                continuation.yield(.callTimeout(requestId: requestId))
            }
            let installed = await state.setPendingTimeoutTask(requestId, timeoutTask: timeoutTask)
            if !installed {
                timeoutTask.cancel()
            }
        }
    }

    private func handleCallTimeout(requestId: UInt64) async {
        guard let pending = await state.removePendingResponse(requestId) else {
            return
        }
        pending.timeoutTask?.cancel()
        pending.responseTx(.failure(.timeout))
        try? await transport.send(.cancel(connId: 0, requestId: requestId))
    }

    /// Handle an incoming message.
    ///
    /// r[impl message.goodbye.receive] - Stop sending, close connection, fail in-flight.
    /// r[impl call.lifecycle.ordering] - Request before Response in message sequence.
    /// r[impl message.unknown-variant] - Unknown message variant triggers Goodbye.
    private func handleMessage(_ msg: Message) async throws {
        switch msg {
        case .hello:
            // Duplicate hello, ignore
            break

        case .connect(let requestId, _):
            // r[impl core.conn.accept-required] - Accept or reject the connection request.
            if acceptConnections {
                // r[impl core.conn.id-allocation] - Allocate a new connection ID.
                let connId = await allocateConnId()
                await addVirtualConnection(connId)
                // r[impl message.accept.response] - Send Accept with new conn_id.
                try await transport.send(
                    .accept(requestId: requestId, connId: connId, metadata: []))
            } else {
                // r[impl message.reject.response] - Reject since not listening.
                try await transport.send(
                    .reject(requestId: requestId, reason: "not listening", metadata: []))
            }

        case .accept, .reject:
            // Responses to our Connect requests - ignore in server mode
            break

        case .goodbye(let connId, let reason):
            // r[impl message.goodbye.connection-zero] - Goodbye on conn 0 closes entire link.
            if connId == 0 {
                // r[impl message.goodbye.receive]
                await failAllPending()
                throw ConnectionError.goodbye(reason: reason)
            }
            // r[impl core.conn.lifecycle] - Close virtual connection if it exists.
            // r[impl core.conn.independence] - Ignore Goodbye on unknown connection.
            await removeVirtualConnection(connId)

        case .request(_, let requestId, let methodId, _, _, let payload):
            // Note: connId and channels field is ignored for now - server uses payload parsing for channel IDs
            try await handleRequest(requestId: requestId, methodId: methodId, payload: payload)

        case .response(_, let requestId, _, _, let payload):
            // r[impl call.lifecycle.single-response] - One response per request.
            // r[impl call.complete] - Response completes the call.
            // r[impl call.response.encoding] - Response payload is Postcard-encoded.
            guard let pending = await state.removePendingResponse(requestId) else {
                warnLog(
                    "received response for unknown request_id \(requestId) "
                        + "(payload_size=\(payload.count)); closing connection"
                )
                try await sendGoodbye("call.lifecycle.unknown-request-id")
                throw ConnectionError.protocolViolation(rule: "call.lifecycle.unknown-request-id")
            }
            pending.timeoutTask?.cancel()
            pending.responseTx(.success(payload))

        case .cancel(_, let requestId):
            // r[impl call.cancel.message] - Cancel requests termination.
            // r[impl call.cancel.best-effort] - Cancel is best-effort, response may still arrive.
            // r[impl core.call.cancel] - Cancel message uses request_id.
            // r[impl call.request-id.cancel-still-in-flight] - Cancel only valid for in-flight.
            let _ = await state.removeInFlight(requestId)
        // Handler may still be processing; best-effort cancellation

        case .data(_, let channelId, let payload):
            try await handleData(channelId: channelId, payload: payload)

        case .close(_, let channelId):
            try await handleClose(channelId: channelId)

        case .reset(_, let channelId):
            // r[impl channeling.reset] - Reset abruptly terminates channel.
            // r[impl channeling.reset.effect] - Data in flight may be lost.
            // r[impl channeling.reset.credit] - Credit is discarded on reset.
            await serverRegistry.deliverReset(channelId: channelId)
            await handle.channelRegistry.deliverReset(channelId: channelId)

        case .credit(_, let channelId, let bytes):
            // r[impl flow.channel.credit-based] - Credit controls data flow.
            // r[impl flow.channel.credit-grant] - Credit message grants permission.
            // r[impl flow.channel.credit-additive] - Credits are additive.
            // r[impl flow.channel.all-transports] - Flow control on all transports.
            await serverRegistry.deliverCredit(channelId: channelId, bytes: bytes)
            await handle.channelRegistry.deliverCredit(channelId: channelId, bytes: bytes)
        }
    }

    /// r[impl call.request-id.duplicate-detection] - Duplicate request_id is fatal.
    /// r[impl flow.call.payload-limit] - Payloads bounded by max_payload_size.
    /// r[impl message.hello.enforcement] - Exceeding limit requires Goodbye.
    /// r[impl call.request-id.in-flight] - Request IDs must be tracked while in-flight.
    /// r[impl call.request-id.uniqueness] - Each request uses a unique ID.
    private func handleRequest(requestId: UInt64, methodId: UInt64, payload: [UInt8]) async throws {
        // r[impl call.request-id.duplicate-detection]
        let inserted = await state.addInFlight(requestId)

        guard inserted else {
            try await sendGoodbye("call.request-id.duplicate-detection")
            throw ConnectionError.protocolViolation(rule: "call.request-id.duplicate-detection")
        }

        // r[impl flow.call.payload-limit]
        if payload.count > Int(negotiated.maxPayloadSize) {
            try await sendGoodbye("flow.call.payload-limit")
            throw ConnectionError.protocolViolation(rule: "flow.call.payload-limit")
        }

        // Pre-register channels BEFORE spawning the handler task.
        // This ensures channels are registered before any Data messages arrive.
        await dispatcher.preregister(
            methodId: methodId,
            payload: payload,
            registry: serverRegistry
        )

        // Create task sender
        let taskTx = taskSender()

        // Dispatch (spawns handler task)
        Task {
            await dispatcher.dispatch(
                methodId: methodId,
                payload: payload,
                requestId: requestId,
                registry: serverRegistry,
                taskTx: taskTx
            )
        }
    }

    /// r[impl channeling.id.zero-reserved] - Channel ID 0 is reserved.
    /// r[impl channeling.unknown] - Unknown channel IDs cause Goodbye.
    /// r[impl channeling.data] - Data messages routed by channel_id.
    private func handleData(channelId: UInt64, payload: [UInt8]) async throws {
        // r[impl channeling.id.zero-reserved]
        if channelId == 0 {
            try await sendGoodbye("channeling.id.zero-reserved")
            throw ConnectionError.protocolViolation(rule: "channeling.id.zero-reserved")
        }

        // Try server registry first, then client registry
        var delivered = await serverRegistry.deliverData(channelId: channelId, payload: payload)
        if !delivered {
            delivered = await handle.channelRegistry.deliverData(
                channelId: channelId, payload: payload)
        }

        // r[impl channeling.unknown]
        if !delivered {
            try await sendGoodbye("channeling.unknown")
            throw ConnectionError.protocolViolation(rule: "channeling.unknown")
        }
    }

    /// r[impl channeling.id.zero-reserved] - Channel ID 0 is reserved.
    /// r[impl channeling.unknown] - Unknown channel IDs cause Goodbye.
    /// r[impl channeling.close] - Close terminates the channel.
    private func handleClose(channelId: UInt64) async throws {
        // r[impl channeling.id.zero-reserved]
        if channelId == 0 {
            try await sendGoodbye("channeling.id.zero-reserved")
            throw ConnectionError.protocolViolation(rule: "channeling.id.zero-reserved")
        }

        var delivered = await serverRegistry.deliverClose(channelId: channelId)
        if !delivered {
            delivered = await handle.channelRegistry.deliverClose(channelId: channelId)
        }

        // r[impl channeling.unknown]
        if !delivered {
            try await sendGoodbye("channeling.unknown")
            throw ConnectionError.protocolViolation(rule: "channeling.unknown")
        }
    }

    /// r[impl message.goodbye.send] - Send Goodbye with rule ID before closing.
    /// r[impl core.error.goodbye-reason] - Reason contains violated rule ID.
    private func sendGoodbye(_ reason: String) async throws {
        try await transport.send(.goodbye(connId: 0, reason: reason))
    }

    private func failAllPending() async {
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
}

// MARK: - Connection Errors

/// r[impl core.error.connection] - Connection-level errors terminate the connection.
/// r[impl call.error.protocol] - Protocol errors are connection-fatal.
public enum ConnectionError: Error {
    case connectionClosed
    case timeout
    case transportError(String)
    case goodbye(reason: String)
    case protocolViolation(rule: String)
    case handshakeFailed(String)
}

// MARK: - Establish Connection

/// Establish a connection as initiator.
///
/// r[impl message.hello.ordering] - Hello is the first message sent.
/// r[impl message.hello.timing] - Send Hello immediately on connection.
/// r[impl call.initiate] - Initiator can start calls after Hello exchange.
public func establishInitiator(
    transport: any MessageTransport,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil
) async throws -> (ConnectionHandle, Driver) {
    let ourHello: Hello =
        if let maxPayloadSize {
            .v5(
                maxPayloadSize: maxPayloadSize,
                initialChannelCredit: 64 * 1024,
                maxConcurrentRequests: 64
            )
        } else {
            defaultHello()
        }
    // Send our hello
    try await transport.send(.hello(ourHello))

    // Wait for peer hello - handle decode errors by sending Goodbye
    let peerHello: Hello
    do {
        guard let peerMsg = try await transport.recv(),
            case .hello(let hello) = peerMsg
        else {
            try await transport.send(.goodbye(connId: 0, reason: "handshake.expected-hello"))
            throw ConnectionError.handshakeFailed("expected Hello")
        }
        peerHello = hello
    } catch let error as WireError {
        // r[impl message.hello.unknown-version] - Unknown version triggers Goodbye.
        // r[impl message.decode-error] - Decode errors trigger Goodbye.
        let reason =
            error == .unknownHelloVariant
            ? "message.hello.unknown-version" : "handshake.decode-error"
        try? await transport.send(.goodbye(connId: 0, reason: reason))
        throw ConnectionError.handshakeFailed(reason)
    }
    switch peerHello {
    case .v5:
        break
    default:
        try? await transport.send(.goodbye(connId: 0, reason: "message.hello.unknown-version"))
        throw ConnectionError.handshakeFailed("message.hello.unknown-version")
    }

    let negotiated = Negotiated(
        maxPayloadSize: min(ourHello.maxPayloadSize, peerHello.maxPayloadSize),
        initialCredit: min(ourHello.initialChannelCredit, peerHello.initialChannelCredit),
        maxConcurrentRequests: min(ourHello.maxConcurrentRequests, peerHello.maxConcurrentRequests)
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), initialCredit=\(negotiated.initialCredit), maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    // Update the transport's frame limit to match the negotiated payload size.
    // The +64 accounts for message header overhead (conn_id, request_id, metadata, etc.).
    try await transport.setMaxFrameSize(Int(negotiated.maxPayloadSize) + 64)

    return makeDriverAndHandle(
        transport: transport,
        dispatcher: dispatcher,
        role: .initiator,
        negotiated: negotiated,
        acceptConnections: acceptConnections
    )
}

/// Establish a connection as acceptor.
///
/// r[impl message.hello.ordering] - Hello is the first message sent.
/// r[impl message.hello.timing] - Send Hello immediately on connection.
public func establishAcceptor(
    transport: any MessageTransport,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil
) async throws -> (ConnectionHandle, Driver) {
    let ourHello: Hello =
        if let maxPayloadSize {
            .v5(
                maxPayloadSize: maxPayloadSize,
                initialChannelCredit: 64 * 1024,
                maxConcurrentRequests: 64
            )
        } else {
            defaultHello()
        }
    // Send our hello immediately
    try await transport.send(.hello(ourHello))

    // Wait for peer hello - handle decode errors by sending Goodbye
    let peerHello: Hello
    do {
        guard let peerMsg = try await transport.recv(),
            case .hello(let hello) = peerMsg
        else {
            try await transport.send(.goodbye(connId: 0, reason: "handshake.expected-hello"))
            throw ConnectionError.handshakeFailed("expected Hello")
        }
        peerHello = hello
    } catch let error as WireError {
        // Unknown Hello variant or decode error - send Goodbye per spec
        let reason =
            error == .unknownHelloVariant
            ? "message.hello.unknown-version" : "handshake.decode-error"
        try? await transport.send(.goodbye(connId: 0, reason: reason))
        throw ConnectionError.handshakeFailed(reason)
    }
    switch peerHello {
    case .v5:
        break
    default:
        try? await transport.send(.goodbye(connId: 0, reason: "message.hello.unknown-version"))
        throw ConnectionError.handshakeFailed("message.hello.unknown-version")
    }

    let negotiated = Negotiated(
        maxPayloadSize: min(ourHello.maxPayloadSize, peerHello.maxPayloadSize),
        initialCredit: min(ourHello.initialChannelCredit, peerHello.initialChannelCredit),
        maxConcurrentRequests: min(ourHello.maxConcurrentRequests, peerHello.maxConcurrentRequests)
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), initialCredit=\(negotiated.initialCredit), maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    // Update the transport's frame limit to match the negotiated payload size.
    // The +64 accounts for message header overhead (conn_id, request_id, metadata, etc.).
    try await transport.setMaxFrameSize(Int(negotiated.maxPayloadSize) + 64)

    return makeDriverAndHandle(
        transport: transport,
        dispatcher: dispatcher,
        role: .acceptor,
        negotiated: negotiated,
        acceptConnections: acceptConnections
    )
}

/// Create a Driver and ConnectionHandle with properly wired command/task channels.
private func makeDriverAndHandle(
    transport: any MessageTransport,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    acceptConnections: Bool
) -> (ConnectionHandle, Driver) {
    // Create the event stream that will be shared
    var continuation: AsyncStream<DriverEvent>.Continuation!
    let eventStream = AsyncStream<DriverEvent> { cont in
        continuation = cont
    }
    // Capture as let to satisfy Sendable requirements
    let capturedContinuation = continuation!

    // Create command sender that uses this continuation
    let commandSender: @Sendable (HandleCommand) -> Bool = { cmd in
        let result = capturedContinuation.yield(.command(cmd))
        guard case .terminated = result else {
            return true
        }
        return false
    }

    // Create handle with the command sender
    let handle = ConnectionHandle(
        commandTx: commandSender,
        role: role
    )

    // Create driver with the handle and shared event stream
    let driver = Driver(
        transport: transport,
        dispatcher: dispatcher,
        role: role,
        negotiated: negotiated,
        handle: handle,
        acceptConnections: acceptConnections,
        eventStream: eventStream,
        eventContinuation: continuation
    )

    return (handle, driver)
}
