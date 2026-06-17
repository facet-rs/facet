import Foundation

// MARK: - Driver

/// Bidirectional connection driver.
///
/// r[impl rpc.pipelining] - Handle requests as they arrive, each independently.
/// r[impl rpc.request] - compatibility lane-id field provides multiplexing.
///
/// Uses AsyncStream to multiplex between:
/// - Incoming messages from transport
/// - Task messages from handlers (Data/Close/Response)
/// - Commands from lane and connection handles
public final class Driver: @unchecked Sendable {
    var conduit: any Conduit
    let dispatcher: any ServiceDispatcher
    let role: Role
    let negotiated: Negotiated
    let handle: LaneHandle
    let laneAcceptor: (any LaneAcceptor)?
    let keepalive: ConnectionKeepaliveConfig?

    let serverRegistry: ChannelRegistry
    let state: DriverState
    let laneState: LaneState
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

    let localControlSettings: ConnectionSettings?
    let peerControlSettings: ConnectionSettings?
    let peerMessageSchema: [UInt8]

    init(
        conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: LaneHandle,
        laneAcceptor: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil
    ) {
        self.conduit = conduit
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.laneAcceptor = laneAcceptor
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.laneState = LaneState(role: role)
        self.schemaSendTracker = SchemaSendTracker()
        self.commandQueue = LockedQueue<HandleCommand>()
        self.taskQueue = LockedQueue<DriverQueuedTaskMessage>()
        self.localControlSettings = nil
        self.peerControlSettings = nil
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
        handle: LaneHandle,
        laneAcceptor: (any LaneAcceptor)?,
        keepalive: ConnectionKeepaliveConfig?,
        eventStream: AsyncStream<DriverEvent>,
        eventContinuation: AsyncStream<DriverEvent>.Continuation,
        commandQueue: LockedQueue<HandleCommand>,
        taskQueue: LockedQueue<DriverQueuedTaskMessage>,
        schemaSendTracker: SchemaSendTracker = SchemaSendTracker(),
        localControlSettings: ConnectionSettings? = nil,
        peerControlSettings: ConnectionSettings? = nil,
        peerMessageSchema: [UInt8] = []
    ) {
        self.conduit = conduit
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.laneAcceptor = laneAcceptor
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.laneState = LaneState(role: role)
        self.schemaSendTracker = schemaSendTracker
        self.eventStream = eventStream
        self.eventContinuation = eventContinuation
        self.commandQueue = commandQueue
        self.taskQueue = taskQueue
        self.localControlSettings = localControlSettings
        self.peerControlSettings = peerControlSettings
        self.peerMessageSchema = peerMessageSchema
    }

    // r[impl rpc.debug.snapshot]
    func debugSnapshot() async -> VoxConnectionDebugSnapshot {
        VoxConnectionDebugSnapshot(
            role: role,
            driverState: await state.debugSnapshot(),
            controlLaneChannels: await handle.channelRegistry.debugSnapshot(laneId: 0),
            serverChannels: await serverRegistry.debugSnapshot(laneId: 0),
            lanes: await laneState.debugSnapshot(),
            pendingCallCount: pendingCalls.count,
            pendingTaskMessageCount: pendingTaskMessages.count
        )
    }
}
