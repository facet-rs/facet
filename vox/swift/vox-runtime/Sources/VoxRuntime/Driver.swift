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
    let operations: OperationRegistry
    let acceptConnections: Bool
    let keepalive: DriverKeepaliveConfig?

    let serverRegistry: ChannelRegistry
    let state: DriverState
    let virtualConnState: VirtualConnectionState
    let schemaSendTracker: SchemaSendTracker

    let eventContinuation: AsyncStream<DriverEvent>.Continuation
    let eventStream: AsyncStream<DriverEvent>
    let commandQueue: LockedQueue<HandleCommand>
    let taskQueue: LockedQueue<TaskMessage>
    var pendingTaskMessages: [DriverQueuedTaskMessage] = []
    var pendingCalls: [DriverQueuedCall] = []

    // Session resumption support
    let resumable: Bool
    let localRootSettings: ConnectionSettings?
    let peerRootSettings: ConnectionSettings?
    let transport: ConduitKind?
    let recoverAttachment: (@Sendable () async throws -> LinkAttachment)?
    let sessionResumeKey: [UInt8]?

    init(
        conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        operations: OperationRegistry,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil
    ) {
        self.conduit = conduit
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.operations = operations
        self.acceptConnections = acceptConnections
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState()
        self.schemaSendTracker = SchemaSendTracker()
        self.commandQueue = LockedQueue<HandleCommand>()
        self.taskQueue = LockedQueue<TaskMessage>()
        self.resumable = false
        self.localRootSettings = nil
        self.peerRootSettings = nil
        self.transport = nil
        self.recoverAttachment = nil
        self.sessionResumeKey = nil

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
        operations: OperationRegistry,
        acceptConnections: Bool,
        keepalive: DriverKeepaliveConfig?,
        eventStream: AsyncStream<DriverEvent>,
        eventContinuation: AsyncStream<DriverEvent>.Continuation,
        commandQueue: LockedQueue<HandleCommand>,
        taskQueue: LockedQueue<TaskMessage>,
        schemaSendTracker: SchemaSendTracker = SchemaSendTracker(),
        resumable: Bool = false,
        localRootSettings: ConnectionSettings? = nil,
        peerRootSettings: ConnectionSettings? = nil,
        transport: ConduitKind? = nil,
        recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil,
        sessionResumeKey: [UInt8]? = nil
    ) {
        self.conduit = conduit
        self.dispatcher = dispatcher
        self.role = role
        self.negotiated = negotiated
        self.handle = handle
        self.operations = operations
        self.acceptConnections = acceptConnections
        self.keepalive = keepalive
        self.serverRegistry = ChannelRegistry()
        self.state = DriverState()
        self.virtualConnState = VirtualConnectionState()
        self.schemaSendTracker = schemaSendTracker
        self.eventStream = eventStream
        self.eventContinuation = eventContinuation
        self.commandQueue = commandQueue
        self.taskQueue = taskQueue
        self.resumable = resumable
        self.localRootSettings = localRootSettings
        self.peerRootSettings = peerRootSettings
        self.transport = transport
        self.recoverAttachment = recoverAttachment
        self.sessionResumeKey = sessionResumeKey
    }
}
