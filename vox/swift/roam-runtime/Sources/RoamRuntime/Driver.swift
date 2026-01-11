import Foundation

// MARK: - Negotiated Parameters

/// Parameters negotiated during handshake.
///
/// r[impl message.hello.negotiation] - Effective limit is min of both peers.
/// r[impl flow.channel.initial-credit] - Negotiated during handshake.
public struct Negotiated: Sendable {
    public let maxPayloadSize: UInt32
    public let initialCredit: UInt32

    public init(maxPayloadSize: UInt32, initialCredit: UInt32) {
        self.maxPayloadSize = maxPayloadSize
        self.initialCredit = initialCredit
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
    private let commandTx: @Sendable (HandleCommand) -> Void
    private let requestIdAllocator = RequestIdAllocator()

    public let channelAllocator: ChannelIdAllocator
    public let channelRegistry: ChannelRegistry

    init(
        commandTx: @escaping @Sendable (HandleCommand) -> Void,
        role: Role
    ) {
        self.commandTx = commandTx
        self.channelAllocator = ChannelIdAllocator(role: role)
        self.channelRegistry = ChannelRegistry()
    }

    /// Make a raw RPC call.
    public func callRaw(methodId: UInt64, payload: [UInt8]) async throws -> [UInt8] {
        let requestId = await requestIdAllocator.allocate()

        return try await withCheckedThrowingContinuation { cont in
            let responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void = { result in
                cont.resume(with: result)
            }
            commandTx(
                .call(
                    requestId: requestId,
                    methodId: methodId,
                    payload: payload,
                    responseTx: responseTx
                ))
        }
    }
}

// MARK: - Driver Event

/// Events the driver processes in its run loop.
private enum DriverEvent: Sendable {
    case incomingMessage(Message)
    case taskMessage(TaskMessage)
    case command(HandleCommand)
    case transportClosed
}

// MARK: - Driver

// MARK: - Driver State Actor

/// Actor that holds mutable driver state to avoid NSLock in async contexts.
private actor DriverState {
    var pendingResponses: [UInt64: @Sendable (Result<[UInt8], ConnectionError>) -> Void] = [:]
    var inFlightRequests: Set<UInt64> = []

    func addPendingResponse(
        _ requestId: UInt64,
        _ handler: @escaping @Sendable (Result<[UInt8], ConnectionError>) -> Void
    ) {
        pendingResponses[requestId] = handler
    }

    func removePendingResponse(_ requestId: UInt64) -> (
        @Sendable (Result<[UInt8], ConnectionError>) -> Void
    )? {
        pendingResponses.removeValue(forKey: requestId)
    }

    func addInFlight(_ requestId: UInt64) -> Bool {
        inFlightRequests.insert(requestId).inserted
    }

    func removeInFlight(_ requestId: UInt64) -> Bool {
        inFlightRequests.remove(requestId) != nil
    }

    func failAllPending() -> [UInt64: @Sendable (Result<[UInt8], ConnectionError>) -> Void] {
        let responses = pendingResponses
        pendingResponses.removeAll()
        return responses
    }
}

/// Bidirectional connection driver.
///
/// r[impl unary.pipelining.allowed] - Handle requests as they arrive.
/// r[impl unary.pipelining.independence] - Each request handled independently.
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

    private let serverRegistry: ChannelRegistry
    private let state: DriverState

    // Event stream for multiplexing
    private let eventContinuation: AsyncStream<DriverEvent>.Continuation
    private let eventStream: AsyncStream<DriverEvent>

    public init(
        transport: any MessageTransport,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle
    ) {
        self.transport = transport
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()

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
        eventStream: AsyncStream<DriverEvent>,
        eventContinuation: AsyncStream<DriverEvent>.Continuation
    ) {
        self.transport = transport
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.eventStream = eventStream
        self.eventContinuation = eventContinuation
    }

    /// Get the task sender for handlers to send responses.
    public func taskSender() -> @Sendable (TaskMessage) -> Void {
        let cont = eventContinuation
        return { msg in
            cont.yield(.taskMessage(msg))
        }
    }

    /// Get the command sender for ConnectionHandle.
    func commandSender() -> @Sendable (HandleCommand) -> Void {
        let cont = eventContinuation
        return { cmd in
            cont.yield(.command(cmd))
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

        // Process events
        for await event in eventStream {
            switch event {
            case .incomingMessage(let msg):
                try await handleMessage(msg)

            case .taskMessage(let msg):
                try await handleTaskMessage(msg)

            case .command(let cmd):
                await handleCommand(cmd)

            case .transportClosed:
                await failAllPending()
                return
            }
        }
    }

    /// Handle a task message from a handler.
    private func handleTaskMessage(_ msg: TaskMessage) async throws {
        let wireMsg: Message
        switch msg {
        case .data(let channelId, let payload):
            wireMsg = .data(channelId: channelId, payload: payload)
        case .close(let channelId):
            wireMsg = .close(channelId: channelId)
        case .response(let requestId, let payload):
            let wasInFlight = await state.removeInFlight(requestId)
            guard wasInFlight else {
                return  // Already cancelled
            }
            wireMsg = .response(requestId: requestId, metadata: [], payload: payload)
        }
        try await transport.send(wireMsg)
    }

    /// Handle a command from ConnectionHandle.
    private func handleCommand(_ cmd: HandleCommand) async {
        switch cmd {
        case .call(let requestId, let methodId, let payload, let responseTx):
            await state.addPendingResponse(requestId, responseTx)

            let msg = Message.request(
                requestId: requestId,
                methodId: methodId,
                metadata: [],
                payload: payload
            )
            try? await transport.send(msg)
        }
    }

    /// Handle an incoming message.
    ///
    /// r[impl message.goodbye.receive] - Stop sending, close connection, fail in-flight.
    /// r[impl unary.lifecycle.ordering] - Request before Response in message sequence.
    /// r[impl message.unknown-variant] - Unknown message variant triggers Goodbye.
    private func handleMessage(_ msg: Message) async throws {
        switch msg {
        case .hello:
            // Duplicate hello, ignore
            break

        case .goodbye(let reason):
            // r[impl message.goodbye.receive]
            await failAllPending()
            throw ConnectionError.goodbye(reason: reason)

        case .request(let requestId, let methodId, _, let payload):
            try await handleRequest(requestId: requestId, methodId: methodId, payload: payload)

        case .response(let requestId, _, let payload):
            // r[impl unary.lifecycle.single-response] - One response per request.
            // r[impl unary.complete] - Response completes the call.
            // r[impl unary.response.encoding] - Response payload is Postcard-encoded.
            let responseTx = await state.removePendingResponse(requestId)
            responseTx?(.success(payload))

        case .cancel(let requestId):
            // r[impl unary.cancel.message] - Cancel requests termination.
            // r[impl unary.cancel.best-effort] - Cancel is best-effort, response may still arrive.
            // r[impl core.call.cancel] - Cancel message uses request_id.
            // r[impl unary.request-id.cancel-still-in-flight] - Cancel only valid for in-flight.
            let _ = await state.removeInFlight(requestId)
        // Handler may still be processing; best-effort cancellation

        case .data(let channelId, let payload):
            try await handleData(channelId: channelId, payload: payload)

        case .close(let channelId):
            try await handleClose(channelId: channelId)

        case .reset(let channelId):
            // r[impl channeling.reset] - Reset abruptly terminates channel.
            // r[impl channeling.reset.effect] - Data in flight may be lost.
            // r[impl channeling.reset.credit] - Credit is discarded on reset.
            await serverRegistry.deliverReset(channelId: channelId)
            await handle.channelRegistry.deliverReset(channelId: channelId)

        case .credit(let channelId, let bytes):
            // r[impl flow.channel.credit-based] - Credit controls data flow.
            // r[impl flow.channel.credit-grant] - Credit message grants permission.
            // r[impl flow.channel.credit-additive] - Credits are additive.
            // r[impl flow.channel.all-transports] - Flow control on all transports.
            await serverRegistry.deliverCredit(channelId: channelId, bytes: bytes)
            await handle.channelRegistry.deliverCredit(channelId: channelId, bytes: bytes)
        }
    }

    /// r[impl unary.request-id.duplicate-detection] - Duplicate request_id is fatal.
    /// r[impl flow.unary.payload-limit] - Payloads bounded by max_payload_size.
    /// r[impl message.hello.enforcement] - Exceeding limit requires Goodbye.
    /// r[impl unary.request-id.in-flight] - Request IDs must be tracked while in-flight.
    /// r[impl unary.request-id.uniqueness] - Each request uses a unique ID.
    private func handleRequest(requestId: UInt64, methodId: UInt64, payload: [UInt8]) async throws {
        // r[impl unary.request-id.duplicate-detection]
        let inserted = await state.addInFlight(requestId)

        guard inserted else {
            try await sendGoodbye("unary.request-id.duplicate-detection")
            throw ConnectionError.protocolViolation(rule: "unary.request-id.duplicate-detection")
        }

        // r[impl flow.unary.payload-limit]
        if payload.count > Int(negotiated.maxPayloadSize) {
            try await sendGoodbye("flow.unary.payload-limit")
            throw ConnectionError.protocolViolation(rule: "flow.unary.payload-limit")
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
        try await transport.send(.goodbye(reason: reason))
    }

    private func failAllPending() async {
        let responses = await state.failAllPending()

        for (_, responseTx) in responses {
            responseTx(.failure(.connectionClosed))
        }
    }
}

// MARK: - Connection Errors

/// r[impl core.error.connection] - Connection-level errors terminate the connection.
/// r[impl unary.error.protocol] - Protocol errors are connection-fatal.
public enum ConnectionError: Error {
    case connectionClosed
    case goodbye(reason: String)
    case protocolViolation(rule: String)
    case handshakeFailed(String)
}

// MARK: - Establish Connection

/// Establish a connection as initiator.
///
/// r[impl message.hello.ordering] - Hello is the first message sent.
/// r[impl message.hello.timing] - Send Hello immediately on connection.
/// r[impl unary.initiate] - Initiator can start calls after Hello exchange.
public func establishInitiator(
    transport: any MessageTransport,
    ourHello: Hello,
    dispatcher: any ServiceDispatcher
) async throws -> (ConnectionHandle, Driver) {
    // Send our hello
    try await transport.send(.hello(ourHello))

    // Wait for peer hello - handle decode errors by sending Goodbye
    let peerHello: Hello
    do {
        guard let peerMsg = try await transport.recv(),
            case .hello(let hello) = peerMsg
        else {
            try await transport.send(.goodbye(reason: "handshake.expected-hello"))
            throw ConnectionError.handshakeFailed("expected Hello")
        }
        peerHello = hello
    } catch let error as WireError {
        // r[impl message.hello.unknown-version] - Unknown version triggers Goodbye.
        // r[impl message.decode-error] - Decode errors trigger Goodbye.
        let reason =
            error == .unknownHelloVariant
            ? "message.hello.unknown-version" : "handshake.decode-error"
        try? await transport.send(.goodbye(reason: reason))
        throw ConnectionError.handshakeFailed(reason)
    }

    let (ourMax, ourCredit) =
        switch ourHello {
        case .v1(let max, let credit): (max, credit)
        }
    let (peerMax, peerCredit) =
        switch peerHello {
        case .v1(let max, let credit): (max, credit)
        }

    let negotiated = Negotiated(
        maxPayloadSize: min(ourMax, peerMax),
        initialCredit: min(ourCredit, peerCredit)
    )

    return makeDriverAndHandle(
        transport: transport,
        dispatcher: dispatcher,
        role: .initiator,
        negotiated: negotiated
    )
}

/// Establish a connection as acceptor.
///
/// r[impl message.hello.ordering] - Hello is the first message sent.
/// r[impl message.hello.timing] - Send Hello immediately on connection.
public func establishAcceptor(
    transport: any MessageTransport,
    ourHello: Hello,
    dispatcher: any ServiceDispatcher
) async throws -> (ConnectionHandle, Driver) {
    // Send our hello immediately
    try await transport.send(.hello(ourHello))

    // Wait for peer hello - handle decode errors by sending Goodbye
    let peerHello: Hello
    do {
        guard let peerMsg = try await transport.recv(),
            case .hello(let hello) = peerMsg
        else {
            try await transport.send(.goodbye(reason: "handshake.expected-hello"))
            throw ConnectionError.handshakeFailed("expected Hello")
        }
        peerHello = hello
    } catch let error as WireError {
        // Unknown Hello variant or decode error - send Goodbye per spec
        let reason =
            error == .unknownHelloVariant
            ? "message.hello.unknown-version" : "handshake.decode-error"
        try? await transport.send(.goodbye(reason: reason))
        throw ConnectionError.handshakeFailed(reason)
    }

    let (ourMax, ourCredit) =
        switch ourHello {
        case .v1(let max, let credit): (max, credit)
        }
    let (peerMax, peerCredit) =
        switch peerHello {
        case .v1(let max, let credit): (max, credit)
        }

    let negotiated = Negotiated(
        maxPayloadSize: min(ourMax, peerMax),
        initialCredit: min(ourCredit, peerCredit)
    )

    return makeDriverAndHandle(
        transport: transport,
        dispatcher: dispatcher,
        role: .acceptor,
        negotiated: negotiated
    )
}

/// Create a Driver and ConnectionHandle with properly wired command/task channels.
private func makeDriverAndHandle(
    transport: any MessageTransport,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated
) -> (ConnectionHandle, Driver) {
    // Create the event stream that will be shared
    var continuation: AsyncStream<DriverEvent>.Continuation!
    let eventStream = AsyncStream<DriverEvent> { cont in
        continuation = cont
    }
    // Capture as let to satisfy Sendable requirements
    let capturedContinuation = continuation!

    // Create command sender that uses this continuation
    let commandSender: @Sendable (HandleCommand) -> Void = { cmd in
        capturedContinuation.yield(.command(cmd))
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
        eventStream: eventStream,
        eventContinuation: continuation
    )

    return (handle, driver)
}
