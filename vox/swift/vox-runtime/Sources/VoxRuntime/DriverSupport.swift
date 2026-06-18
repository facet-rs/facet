import Foundation

struct InFlightResponseContext: Sendable {
    let laneId: UInt64
    let responseMetadata: Metadata
    let channels: [UInt64]
}

enum DriverEvent: Sendable {
    case incomingMessage(Message)
    case wake
    case keepaliveTick
    case conduitClosed
    case conduitFailed(String)
}

final class LockedQueue<T>: @unchecked Sendable {
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

func makeDriverAndLane(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    laneAcceptor: (any LaneAcceptor)? = nil,
    keepalive: ConnectionKeepaliveConfig? = nil
) -> (Lane, Driver) {
    let commandQueue = LockedQueue<HandleCommand>()
    let taskQueue = LockedQueue<DriverQueuedTaskMessage>()
    var continuation: AsyncStream<DriverEvent>.Continuation!
    let eventStream = AsyncStream<DriverEvent> { cont in
        continuation = cont
    }
    let capturedContinuation = continuation!

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
        guard taskQueue.push(DriverQueuedTaskMessage(laneId: 0, taskMessage: msg)) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }

    let handle = LaneHandle(
        laneId: 0,
        commandTx: commandSender,
        taskTx: taskSender,
        role: role,
        maxConcurrentRequests: negotiated.maxConcurrentRequests
    )

    let driver = Driver(
        conduit: conduit,
        dispatcher: dispatcher,
        role: role,
        negotiated: negotiated,
        handle: handle,
        laneAcceptor: laneAcceptor,
        keepalive: keepalive,
        eventStream: eventStream,
        eventContinuation: continuation,
        commandQueue: commandQueue,
        taskQueue: taskQueue
    )

    return (Lane(handle: handle, schemaReceiveTracker: driver.schemaReceiveTracker), driver)
}

func makeConnectionDriverAndControlLane(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    laneAcceptor: (any LaneAcceptor)? = nil,
    keepalive: ConnectionKeepaliveConfig? = nil,
    localControlSettings: ConnectionSettings,
    peerControlSettings: ConnectionSettings,
    peerMessageSchema: [UInt8],
    peerEvidence: PeerEvidence = .none,
    peerIdentity: PeerIdentity = .anonymous
) -> (Lane, Driver, ConnectionHandle) {
    let commandQueue = LockedQueue<HandleCommand>()
    let taskQueue = LockedQueue<DriverQueuedTaskMessage>()
    var continuation: AsyncStream<DriverEvent>.Continuation!
    let eventStream = AsyncStream<DriverEvent> { cont in
        continuation = cont
    }
    let capturedContinuation = continuation!

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
        guard taskQueue.push(DriverQueuedTaskMessage(laneId: 0, taskMessage: msg)) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }

    let handle = LaneHandle(
        laneId: 0,
        commandTx: commandSender,
        taskTx: taskSender,
        role: role,
        maxConcurrentRequests: negotiated.maxConcurrentRequests
    )

    let driver = Driver(
        conduit: conduit,
        dispatcher: dispatcher,
        role: role,
        negotiated: negotiated,
        handle: handle,
        laneAcceptor: laneAcceptor,
        keepalive: keepalive,
        eventStream: eventStream,
        eventContinuation: continuation,
        commandQueue: commandQueue,
        taskQueue: taskQueue,
        localControlSettings: localControlSettings,
        peerControlSettings: peerControlSettings,
        peerMessageSchema: peerMessageSchema,
        peerEvidence: peerEvidence,
        peerIdentity: peerIdentity
    )

    let connectionHandle = ConnectionHandle(
        commandTx: commandSender,
        eventContinuation: continuation,
        peerEvidence: peerEvidence,
        peerIdentity: peerIdentity
    )

    return (Lane(handle: handle, schemaReceiveTracker: driver.schemaReceiveTracker), driver, connectionHandle)
}
