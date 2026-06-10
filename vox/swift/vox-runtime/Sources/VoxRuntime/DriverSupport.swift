import Foundation

struct InFlightResponseContext: Sendable {
    let connectionId: UInt64
    let responseMetadata: Metadata
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

func makeDriverAndConnection(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    connectionAcceptor: (any ConnectionAcceptor)? = nil,
    keepalive: SessionKeepaliveConfig? = nil
) -> (Connection, Driver) {
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
        guard taskQueue.push(DriverQueuedTaskMessage(connectionId: 0, taskMessage: msg)) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }

    let handle = ConnectionHandle(
        connectionId: 0,
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
        connectionAcceptor: connectionAcceptor,
        keepalive: keepalive,
        eventStream: eventStream,
        eventContinuation: continuation,
        commandQueue: commandQueue,
        taskQueue: taskQueue
    )

    return (Connection(handle: handle, schemaReceiveTracker: driver.schemaReceiveTracker), driver)
}

func makeSessionDriverAndConnection(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    connectionAcceptor: (any ConnectionAcceptor)? = nil,
    keepalive: SessionKeepaliveConfig? = nil,
    localRootSettings: ConnectionSettings,
    peerRootSettings: ConnectionSettings,
    peerMessageSchema: [UInt8]
) -> (Connection, Driver, SessionHandle) {
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
        guard taskQueue.push(DriverQueuedTaskMessage(connectionId: 0, taskMessage: msg)) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }

    let handle = ConnectionHandle(
        connectionId: 0,
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
        connectionAcceptor: connectionAcceptor,
        keepalive: keepalive,
        eventStream: eventStream,
        eventContinuation: continuation,
        commandQueue: commandQueue,
        taskQueue: taskQueue,
        localRootSettings: localRootSettings,
        peerRootSettings: peerRootSettings,
        peerMessageSchema: peerMessageSchema
    )

    let sessionHandle = SessionHandle(
        commandTx: commandSender,
        eventContinuation: continuation
    )

    return (Connection(handle: handle, schemaReceiveTracker: driver.schemaReceiveTracker), driver, sessionHandle)
}
