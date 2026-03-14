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
    let conduit: any Conduit
    let dispatcher: any ServiceDispatcher
    let role: Role
    let negotiated: Negotiated
    let handle: ConnectionHandle
    let acceptConnections: Bool
    let keepalive: DriverKeepaliveConfig?

    let serverRegistry: ChannelRegistry
    let state: DriverState
    let virtualConnState: VirtualConnectionState

    let eventContinuation: AsyncStream<DriverEvent>.Continuation
    let eventStream: AsyncStream<DriverEvent>
    let commandQueue: LockedQueue<HandleCommand>
    let taskQueue: LockedQueue<TaskMessage>
    var pendingTaskMessages: [DriverQueuedTaskMessage] = []
    var pendingCalls: [DriverQueuedCall] = []

    init(
        conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil
    ) {
        self.conduit = conduit
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
    init(
        conduit: any Conduit,
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
        self.conduit = conduit
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
}
