import Foundation

// MARK: - Driver

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
    var conduit: any Conduit
    let dispatcher: any ServiceDispatcher
    let role: Role
    let negotiated: Negotiated
    let handle: ConnectionHandle
    let connectionAcceptor: (any ConnectionAcceptor)?
    let keepalive: SessionKeepaliveConfig?

    let serverRegistry: ChannelRegistry
    let state: DriverState
    let virtualConnState: VirtualConnectionState
    let schemaSendTracker: SchemaSendTracker
    /// Writer schema closures the peer advertised (per method+direction), used by
    /// the dispatcher to build args compat decoders.
    let schemaReceiveTracker = SchemaTracker()

    let eventContinuation: AsyncStream<DriverEvent>.Continuation
    let eventStream: AsyncStream<DriverEvent>
    let commandQueue: LockedQueue<HandleCommand>
    let taskQueue: LockedQueue<DriverQueuedTaskMessage>
    var pendingTaskMessages: [DriverQueuedWireMessage] = []
    var pendingCalls: [DriverQueuedCall] = []

    let localRootSettings: ConnectionSettings?
    let peerRootSettings: ConnectionSettings?
    let peerMessageSchema: [UInt8]

    init(
        conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        connectionAcceptor: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil
    ) {
        self.conduit = conduit
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.connectionAcceptor = connectionAcceptor
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState(role: role)
        self.schemaSendTracker = SchemaSendTracker()
        self.commandQueue = LockedQueue<HandleCommand>()
        self.taskQueue = LockedQueue<DriverQueuedTaskMessage>()
        self.localRootSettings = nil
        self.peerRootSettings = nil
        self.peerMessageSchema = []

        // Create event stream
        var continuation: AsyncStream<DriverEvent>.Continuation!
        self.eventStream = AsyncStream { cont in
            continuation = cont
        }
        self.eventContinuation = continuation
    }

    /// Internal initializer with external event stream (for proper wiring).
    init(
        conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        connectionAcceptor: (any ConnectionAcceptor)?,
        keepalive: SessionKeepaliveConfig?,
        eventStream: AsyncStream<DriverEvent>,
        eventContinuation: AsyncStream<DriverEvent>.Continuation,
        commandQueue: LockedQueue<HandleCommand>,
        taskQueue: LockedQueue<DriverQueuedTaskMessage>,
        schemaSendTracker: SchemaSendTracker = SchemaSendTracker(),
        localRootSettings: ConnectionSettings? = nil,
        peerRootSettings: ConnectionSettings? = nil,
        peerMessageSchema: [UInt8] = []
    ) {
        self.conduit = conduit
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.connectionAcceptor = connectionAcceptor
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState(role: role)
        self.schemaSendTracker = schemaSendTracker
        self.eventStream = eventStream
        self.eventContinuation = eventContinuation
        self.commandQueue = commandQueue
        self.taskQueue = taskQueue
        self.localRootSettings = localRootSettings
        self.peerRootSettings = peerRootSettings
        self.peerMessageSchema = peerMessageSchema
    }
}
